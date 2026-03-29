//! MBC1 — Memory Bank Controller 1.
//!
//! Supports up to 2 MiB ROM (128 × 16 KiB banks) and up to 32 KiB RAM
//! (4 × 8 KiB banks).
//!
//! Address space control registers (write-only, mapped to ROM space):
//!
//!   0x0000–0x1FFF  RAM enable   (0x0A in lower nibble → enable, else disable)
//!   0x2000–0x3FFF  ROM bank low  (5-bit, bits 4–0 of bank number; 0 → 1)
//!   0x4000–0x5FFF  ROM bank hi / RAM bank (2-bit)
//!   0x6000–0x7FFF  Banking mode: 0 = ROM mode (default), 1 = RAM mode
//!
//! In ROM mode  (mode = 0): upper 2 bits select ROM bank (hi); RAM bank fixed 0.
//! In RAM mode  (mode = 1): upper 2 bits select RAM bank; ROM bank 0 is fixed.

const ROM_BANK_SIZE: usize = 0x4000; // 16 KiB
const RAM_BANK_SIZE: usize = 0x2000; //  8 KiB

pub struct Mbc1 {
    rom: Vec<u8>,
    ram: Vec<u8>,

    rom_bank_lo: u8, // 5-bit (bits 4–0)
    rom_bank_hi: u8, // 2-bit (bits 6–5)
    ram_bank:    u8, // 2-bit (used in RAM mode)
    ram_enabled: bool,
    mode:        u8, // 0 = ROM mode, 1 = RAM mode
}

impl Mbc1 {
    pub fn new(rom: Vec<u8>, num_ram_banks: u8) -> Self {
        let ram_size = RAM_BANK_SIZE * (num_ram_banks as usize).max(1);
        Mbc1 {
            rom,
            ram: vec![0u8; ram_size],
            rom_bank_lo: 1,
            rom_bank_hi: 0,
            ram_bank:    0,
            ram_enabled: false,
            mode:        0,
        }
    }

    // ── Bank number calculation ───────────────────────────────────────────────

    /// Effective ROM bank for the 0x0000–0x3FFF window.
    /// In ROM mode: upper bits affect bank 0 (advanced ROM banking — rare).
    /// In RAM mode: bank 0 is always 0.
    fn rom_bank_0_index(&self) -> usize {
        if self.mode == 1 {
            0
        } else {
            ((self.rom_bank_hi as usize) << 5) & self.rom_bank_mask()
        }
    }

    /// Effective ROM bank for the 0x4000–0x7FFF window.
    fn rom_bank_n_index(&self) -> usize {
        let bank = ((self.rom_bank_hi as usize) << 5)
            | (self.rom_bank_lo as usize);
        bank & self.rom_bank_mask()
    }

    /// Mask to clamp bank index to the actual number of banks in the ROM.
    fn rom_bank_mask(&self) -> usize {
        let num_banks = (self.rom.len() / ROM_BANK_SIZE).max(2);
        num_banks - 1
    }

    /// Effective RAM bank (only in RAM mode).
    fn ram_bank_index(&self) -> usize {
        if self.mode == 1 { self.ram_bank as usize } else { 0 }
    }

    // ── ROM read ─────────────────────────────────────────────────────────────

    pub fn read_rom(&self, addr: u16) -> u8 {
        let (bank, offset) = if addr < 0x4000 {
            (self.rom_bank_0_index(), addr as usize)
        } else {
            (self.rom_bank_n_index(), (addr as usize) - 0x4000)
        };
        let physical = bank * ROM_BANK_SIZE + offset;
        self.rom.get(physical).copied().unwrap_or(0xFF)
    }

    // ── ROM write (register control) ──────────────────────────────────────────

    pub fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                let lo = value & 0x1F;
                self.rom_bank_lo = if lo == 0 { 1 } else { lo }; // 0 → 1
            }
            0x4000..=0x5FFF => {
                let hi = value & 0x03;
                self.rom_bank_hi = hi;
                self.ram_bank    = hi;
            }
            0x6000..=0x7FFF => {
                self.mode = value & 0x01;
            }
            _ => {}
        }
    }

    // ── RAM read / write ──────────────────────────────────────────────────────

    pub fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled { return 0xFF; }
        let offset = self.ram_bank_index() * RAM_BANK_SIZE + (addr as usize);
        self.ram.get(offset).copied().unwrap_or(0xFF)
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        if !self.ram_enabled { return; }
        let offset = self.ram_bank_index() * RAM_BANK_SIZE + (addr as usize);
        if let Some(cell) = self.ram.get_mut(offset) {
            *cell = value;
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an MBC1 with `num_banks` × 16 KiB ROM banks.
    /// Bank N is filled with byte N (for easy identification).
    fn mbc1_with_banks(num_banks: usize, num_ram_banks: u8) -> Mbc1 {
        let mut rom = vec![0u8; num_banks * ROM_BANK_SIZE];
        for bank in 0..num_banks {
            let start = bank * ROM_BANK_SIZE;
            for byte in &mut rom[start..start + ROM_BANK_SIZE] {
                *byte = bank as u8;
            }
        }
        Mbc1::new(rom, num_ram_banks)
    }

    // ── default state ─────────────────────────────────────────────────────────

    #[test]
    fn test_default_bank0_reads_bank0() {
        let mbc = mbc1_with_banks(4, 0);
        assert_eq!(mbc.read_rom(0x0000), 0x00);
    }

    #[test]
    fn test_default_bank_n_reads_bank1() {
        // Default rom_bank_lo = 1
        let mbc = mbc1_with_banks(4, 0);
        assert_eq!(mbc.read_rom(0x4000), 0x01);
    }

    // ── ROM bank switching ────────────────────────────────────────────────────

    #[test]
    fn test_select_bank_2() {
        let mut mbc = mbc1_with_banks(4, 0);
        mbc.write_rom(0x2000, 0x02);
        assert_eq!(mbc.read_rom(0x4000), 0x02);
    }

    #[test]
    fn test_select_bank_3() {
        let mut mbc = mbc1_with_banks(4, 0);
        mbc.write_rom(0x2000, 0x03);
        assert_eq!(mbc.read_rom(0x4000), 0x03);
    }

    #[test]
    fn test_bank_0_write_maps_to_bank_1() {
        // Writing 0x00 to 0x2000 should resolve to bank 1
        let mut mbc = mbc1_with_banks(4, 0);
        mbc.write_rom(0x2000, 0x00);
        assert_eq!(mbc.read_rom(0x4000), 0x01);
    }

    #[test]
    fn test_bank_mask_wraps_for_small_rom() {
        // 4 banks → mask = 3; selecting bank 5 → 5 & 3 = 1
        let mut mbc = mbc1_with_banks(4, 0);
        mbc.write_rom(0x2000, 0x05);
        assert_eq!(mbc.read_rom(0x4000), 0x01); // 5 & 3 = 1
    }

    #[test]
    fn test_hi_bank_bits_select_upper_banks() {
        // 32 banks → lo=0x01, hi=0x01 → bank = (1<<5)|1 = 33 & 31 = 1... 
        // Use 64 banks so hi bits matter: hi=1, lo=1 → bank 33
        let mut mbc = mbc1_with_banks(64, 0);
        mbc.write_rom(0x2000, 0x01); // lo = 1
        mbc.write_rom(0x4000, 0x01); // hi = 1 → bank = (1<<5)|1 = 33
        assert_eq!(mbc.read_rom(0x4000), 33);
    }

    // ── RAM enable ────────────────────────────────────────────────────────────

    #[test]
    fn test_ram_disabled_reads_0xff() {
        let mbc = mbc1_with_banks(4, 1);
        assert_eq!(mbc.read_ram(0x0000), 0xFF);
    }

    #[test]
    fn test_ram_enable_allows_write_and_read() {
        let mut mbc = mbc1_with_banks(4, 1);
        mbc.write_rom(0x0000, 0x0A); // enable RAM
        mbc.write_ram(0x0000, 0x55);
        assert_eq!(mbc.read_ram(0x0000), 0x55);
    }

    #[test]
    fn test_ram_disabled_write_ignored() {
        let mut mbc = mbc1_with_banks(4, 1);
        // RAM not enabled
        mbc.write_ram(0x0000, 0x55);
        mbc.write_rom(0x0000, 0x0A); // enable RAM
        assert_eq!(mbc.read_ram(0x0000), 0x00); // write was ignored, RAM zeroed
    }

    #[test]
    fn test_ram_disable_prevents_reads_after_enable() {
        let mut mbc = mbc1_with_banks(4, 1);
        mbc.write_rom(0x0000, 0x0A); // enable
        mbc.write_ram(0x0000, 0x42);
        mbc.write_rom(0x0000, 0x00); // disable
        assert_eq!(mbc.read_ram(0x0000), 0xFF);
    }

    // ── Banking mode ──────────────────────────────────────────────────────────

    #[test]
    fn test_rom_mode_is_default() {
        let mbc = mbc1_with_banks(4, 0);
        assert_eq!(mbc.mode, 0);
    }

    #[test]
    fn test_switch_to_ram_mode() {
        let mut mbc = mbc1_with_banks(4, 1);
        mbc.write_rom(0x6000, 0x01);
        assert_eq!(mbc.mode, 1);
    }

    #[test]
    fn test_ram_mode_selects_ram_bank() {
        let mut mbc = mbc1_with_banks(4, 4);
        mbc.write_rom(0x0000, 0x0A); // enable RAM
        mbc.write_rom(0x6000, 0x01); // RAM mode
        mbc.write_rom(0x4000, 0x01); // select RAM bank 1
        mbc.write_ram(0x0000, 0xBB); // write to bank 1
        // Switch to bank 0 and verify isolation
        mbc.write_rom(0x4000, 0x00);
        assert_eq!(mbc.read_ram(0x0000), 0x00, "Bank 0 must not see bank 1 data");
        // Switch back to bank 1
        mbc.write_rom(0x4000, 0x01);
        assert_eq!(mbc.read_ram(0x0000), 0xBB, "Bank 1 data must survive bank switch");
    }

    #[test]
    fn test_ram_mode_fixes_rom_bank0_to_bank0() {
        let mut mbc = mbc1_with_banks(64, 0);
        mbc.write_rom(0x4000, 0x01); // hi bits = 1
        mbc.write_rom(0x6000, 0x01); // RAM mode → bank 0 area always maps to bank 0
        assert_eq!(mbc.read_rom(0x0000), 0x00, "Bank 0 window must read physical bank 0");
    }
}