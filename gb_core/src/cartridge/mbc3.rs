//! MBC3 — Memory Bank Controller 3.
//!
//! Supports:
//!   - Up to 2 MiB ROM (128 × 16 KiB banks) — same as MBC1 but no quirks
//!   - Up to 32 KiB RAM (4 × 8 KiB banks)
//!   - Real-Time Clock (RTC) — 5 registers: seconds, minutes, hours, DL, DH
//!
//! Write registers:
//!   0x0000–0x1FFF  RAM + RTC enable (0x0A = enable)
//!   0x2000–0x3FFF  ROM bank (7-bit, 0 → 1)
//!   0x4000–0x5FFF  RAM bank / RTC select:
//!                    0x00–0x03 → RAM bank 0–3
//!                    0x08–0x0C → RTC register 0–4
//!   0x6000–0x7FFF  Latch clock data (0x00 then 0x01 latches RTC)
//!
//! RTC registers (when selected via 0x4000–0x5FFF):
//!   0x08  RTC S  — Seconds   (0–59)
//!   0x09  RTC M  — Minutes   (0–59)
//!   0x0A  RTC H  — Hours     (0–23)
//!   0x0B  RTC DL — Day lower (bits 7–0)
//!   0x0C  RTC DH — Day upper / flags
//!            Bit 0: Day counter bit 8
//!            Bit 6: Halt flag (1 = RTC stopped)
//!            Bit 7: Day counter carry (set when >511 days)

const ROM_BANK_SIZE: usize = 0x4000; // 16 KiB
const RAM_BANK_SIZE: usize = 0x2000; //  8 KiB

/// RTC register indices (used internally).
const RTC_S:  usize = 0;
const RTC_M:  usize = 1;
const RTC_H:  usize = 2;
const RTC_DL: usize = 3;
const RTC_DH: usize = 4;

pub struct Mbc3 {
    rom:         Vec<u8>,
    ram:         Vec<u8>,
    rom_bank:    u8,
    ram_bank:    u8,
    ram_enabled: bool,

    /// 0x00–0x03 = RAM bank; 0x08–0x0C = RTC register
    ram_rtc_select: u8,

    // ── RTC ──────────────────────────────────────────────────────────────────
    rtc: [u8; 5],
    rtc_latched: [u8; 5],
    latch_step:  u8, // 0 = waiting for 0x00, 1 = waiting for 0x01

    /// Accumulated T-cycles for RTC tick (4,194,304 per second).
    rtc_cycles: u64,
}

impl Mbc3 {
    pub fn new(rom: Vec<u8>, num_ram_banks: u8) -> Self {
        let ram_size = RAM_BANK_SIZE * (num_ram_banks as usize).max(1);
        Mbc3 {
            rom,
            ram:            vec![0u8; ram_size],
            rom_bank:       1,
            ram_bank:       0,
            ram_enabled:    false,
            ram_rtc_select: 0,
            rtc:            [0u8; 5],
            rtc_latched:    [0u8; 5],
            latch_step:     0,
            rtc_cycles:     0,
        }
    }

    // ── RTC helpers ───────────────────────────────────────────────────────────

    /// Advance the RTC by `cycles` T-cycles.
    /// Only ticks when the halt flag (DH bit 6) is clear.
    pub fn tick_rtc(&mut self, cycles: u64) {
        if self.rtc[RTC_DH] & 0x40 != 0 { return; } // halted
        self.rtc_cycles += cycles;
        const CYCLES_PER_SECOND: u64 = 4_194_304;
        while self.rtc_cycles >= CYCLES_PER_SECOND {
            self.rtc_cycles -= CYCLES_PER_SECOND;
            self.increment_rtc();
        }
    }

    fn increment_rtc(&mut self) {
        self.rtc[RTC_S] += 1;
        if self.rtc[RTC_S] >= 60 {
            self.rtc[RTC_S] = 0;
            self.rtc[RTC_M] += 1;
            if self.rtc[RTC_M] >= 60 {
                self.rtc[RTC_M] = 0;
                self.rtc[RTC_H] += 1;
                if self.rtc[RTC_H] >= 24 {
                    self.rtc[RTC_H] = 0;
                    let day = self.day_counter() + 1;
                    self.set_day_counter(day);
                }
            }
        }
    }

    fn day_counter(&self) -> u16 {
        (self.rtc[RTC_DL] as u16) | ((self.rtc[RTC_DH] as u16 & 0x01) << 8)
    }

    fn set_day_counter(&mut self, day: u16) {
        if day > 511 {
            self.rtc[RTC_DH] |= 0x80; // day carry
        }
        let day = day & 0x1FF;
        self.rtc[RTC_DL]  = (day & 0xFF) as u8;
        self.rtc[RTC_DH]  = (self.rtc[RTC_DH] & 0xFE) | ((day >> 8) as u8 & 0x01);
    }

    // ── Read/write ────────────────────────────────────────────────────────────

    pub fn read_rom(&self, addr: u16) -> u8 {
        let (bank, offset) = if addr < 0x4000 {
            (0, addr as usize)
        } else {
            let num_banks = (self.rom.len() / ROM_BANK_SIZE).max(2);
            let b = (self.rom_bank as usize) & (num_banks - 1);
            (b, (addr as usize) - 0x4000)
        };
        self.rom.get(bank * ROM_BANK_SIZE + offset).copied().unwrap_or(0xFF)
    }

    pub fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                let bank = value & 0x7F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => {
                self.ram_rtc_select = value;
                if value <= 0x03 {
                    self.ram_bank = value;
                }
            }
            0x6000..=0x7FFF => {
                // Latch clock: write 0x00 then 0x01
                if value == 0x00 {
                    self.latch_step = 1;
                } else if value == 0x01 && self.latch_step == 1 {
                    self.rtc_latched = self.rtc;
                    self.latch_step  = 0;
                } else {
                    self.latch_step = 0;
                }
            }
            _ => {}
        }
    }

    pub fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled { return 0xFF; }
        match self.ram_rtc_select {
            0x00..=0x03 => {
                let offset = self.ram_bank as usize * RAM_BANK_SIZE + addr as usize;
                self.ram.get(offset).copied().unwrap_or(0xFF)
            }
            0x08 => self.rtc_latched[RTC_S],
            0x09 => self.rtc_latched[RTC_M],
            0x0A => self.rtc_latched[RTC_H],
            0x0B => self.rtc_latched[RTC_DL],
            0x0C => self.rtc_latched[RTC_DH],
            _    => 0xFF,
        }
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        if !self.ram_enabled { return; }
        match self.ram_rtc_select {
            0x00..=0x03 => {
                let offset = self.ram_bank as usize * RAM_BANK_SIZE + addr as usize;
                if let Some(cell) = self.ram.get_mut(offset) {
                    *cell = value;
                }
            }
            0x08 => self.rtc[RTC_S]  = value & 0x3F,
            0x09 => self.rtc[RTC_M]  = value & 0x3F,
            0x0A => self.rtc[RTC_H]  = value & 0x1F,
            0x0B => self.rtc[RTC_DL] = value,
            0x0C => self.rtc[RTC_DH] = value & 0xC1,
            _    => {}
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn mbc3_with_banks(num_rom: usize, num_ram: u8) -> Mbc3 {
        let mut rom = vec![0u8; num_rom * ROM_BANK_SIZE];
        for bank in 0..num_rom {
            let start = bank * ROM_BANK_SIZE;
            for b in &mut rom[start..start + ROM_BANK_SIZE] {
                *b = bank as u8;
            }
        }
        Mbc3::new(rom, num_ram)
    }

    // ── ROM reads ─────────────────────────────────────────────────────────────

    #[test]
    fn test_bank0_always_reads_physical_0() {
        let mbc = mbc3_with_banks(4, 0);
        assert_eq!(mbc.read_rom(0x0000), 0x00);
    }

    #[test]
    fn test_default_bank_n_reads_bank_1() {
        let mbc = mbc3_with_banks(4, 0);
        assert_eq!(mbc.read_rom(0x4000), 0x01);
    }

    #[test]
    fn test_rom_bank_switching() {
        let mut mbc = mbc3_with_banks(4, 0);
        mbc.write_rom(0x2000, 0x03);
        assert_eq!(mbc.read_rom(0x4000), 0x03);
    }

    #[test]
    fn test_bank_0_maps_to_bank_1() {
        let mut mbc = mbc3_with_banks(4, 0);
        mbc.write_rom(0x2000, 0x00);
        assert_eq!(mbc.read_rom(0x4000), 0x01);
    }

    #[test]
    fn test_rom_bank_7bit_mask() {
        // 128 banks → mask = 0x7F; bank 0x80 → 0x80 & 0x7F = 0x00 → 0x01
        let mut mbc = mbc3_with_banks(128, 0);
        mbc.write_rom(0x2000, 0x80); // 0x80 & 0x7F = 0 → 1
        assert_eq!(mbc.read_rom(0x4000), 0x01);
    }

    // ── RAM reads/writes ──────────────────────────────────────────────────────

    #[test]
    fn test_ram_disabled_by_default() {
        let mbc = mbc3_with_banks(4, 1);
        assert_eq!(mbc.read_ram(0x0000), 0xFF);
    }

    #[test]
    fn test_ram_enable_and_write() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_ram(0x0000, 0x42);
        assert_eq!(mbc.read_ram(0x0000), 0x42);
    }

    #[test]
    fn test_ram_bank_switching() {
        let mut mbc = mbc3_with_banks(4, 4);
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_rom(0x4000, 0x01); // RAM bank 1
        mbc.write_ram(0x0000, 0xBB);
        mbc.write_rom(0x4000, 0x00); // RAM bank 0
        assert_eq!(mbc.read_ram(0x0000), 0x00);
        mbc.write_rom(0x4000, 0x01);
        assert_eq!(mbc.read_ram(0x0000), 0xBB);
    }

    // ── RTC ───────────────────────────────────────────────────────────────────

    #[test]
    fn test_rtc_seconds_increment() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.rtc[RTC_S] = 0;
        mbc.tick_rtc(4_194_304); // one second
        assert_eq!(mbc.rtc[RTC_S], 1);
    }

    #[test]
    fn test_rtc_seconds_overflow_increments_minutes() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.rtc[RTC_S] = 59;
        mbc.tick_rtc(4_194_304);
        assert_eq!(mbc.rtc[RTC_S], 0);
        assert_eq!(mbc.rtc[RTC_M], 1);
    }

    #[test]
    fn test_rtc_minutes_overflow_increments_hours() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.rtc[RTC_M] = 59;
        mbc.rtc[RTC_S] = 59;
        mbc.tick_rtc(4_194_304);
        assert_eq!(mbc.rtc[RTC_M], 0);
        assert_eq!(mbc.rtc[RTC_H], 1);
    }

    #[test]
    fn test_rtc_hours_overflow_increments_day() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.rtc[RTC_H] = 23;
        mbc.rtc[RTC_M] = 59;
        mbc.rtc[RTC_S] = 59;
        mbc.tick_rtc(4_194_304);
        assert_eq!(mbc.rtc[RTC_H], 0);
        assert_eq!(mbc.day_counter(), 1);
    }

    #[test]
    fn test_rtc_does_not_tick_when_halted() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.rtc[RTC_DH] = 0x40; // halt bit
        mbc.rtc[RTC_S]  = 0;
        mbc.tick_rtc(4_194_304 * 10);
        assert_eq!(mbc.rtc[RTC_S], 0, "RTC must not tick when halted");
    }

    #[test]
    fn test_rtc_day_counter_carry_after_511_days() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.set_day_counter(511);
        mbc.rtc[RTC_H] = 23;
        mbc.rtc[RTC_M] = 59;
        mbc.rtc[RTC_S] = 59;
        mbc.tick_rtc(4_194_304);
        assert_ne!(mbc.rtc[RTC_DH] & 0x80, 0, "Day carry bit must be set after 511 days");
    }

    // ── RTC latch ─────────────────────────────────────────────────────────────

    #[test]
    fn test_rtc_latch_captures_current_time() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.write_rom(0x0000, 0x0A); // enable
        mbc.rtc[RTC_S] = 42;
        // Latch: write 0x00 then 0x01
        mbc.write_rom(0x6000, 0x00);
        mbc.write_rom(0x6000, 0x01);
        // Select RTC S register
        mbc.write_rom(0x4000, 0x08);
        assert_eq!(mbc.read_ram(0x0000), 42, "Latched seconds must be 42");
    }

    #[test]
    fn test_rtc_latch_freezes_while_rtc_advances() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.write_rom(0x0000, 0x0A);
        mbc.rtc[RTC_S] = 5;
        // Latch at S=5
        mbc.write_rom(0x6000, 0x00);
        mbc.write_rom(0x6000, 0x01);
        // Advance RTC
        mbc.tick_rtc(4_194_304 * 10); // 10 seconds
        // Latched value must still be 5
        mbc.write_rom(0x4000, 0x08);
        assert_eq!(mbc.read_ram(0x0000), 5, "Latch must freeze time");
        // Latch again to get updated time
        mbc.write_rom(0x6000, 0x00);
        mbc.write_rom(0x6000, 0x01);
        assert_eq!(mbc.read_ram(0x0000), 15, "Re-latching must capture updated time");
    }

    #[test]
    fn test_rtc_write_sets_seconds() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_rom(0x4000, 0x08); // select RTC S
        mbc.write_ram(0x0000, 30);
        assert_eq!(mbc.rtc[RTC_S], 30);
    }

    #[test]
    fn test_rtc_write_seconds_masked_to_6_bits() {
        let mut mbc = mbc3_with_banks(4, 1);
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_rom(0x4000, 0x08);
        mbc.write_ram(0x0000, 0xFF); // 0xFF & 0x3F = 63 — clamped
        assert_eq!(mbc.rtc[RTC_S], 63);
    }
}