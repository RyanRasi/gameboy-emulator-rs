//! MBC5 — Memory Bank Controller 5.
//!
//! Supports up to 8 MiB ROM (512 × 16 KiB banks) and up to 128 KiB RAM
//! (16 × 8 KiB banks).
//!
//! Unlike MBC1, bank 0 is always bank 0 and there is no banking mode.
//!
//! Write registers:
//!   0x0000–0x1FFF  RAM enable   (0x0A → enable, anything else → disable)
//!   0x2000–0x2FFF  ROM bank lo  (lower 8 bits of bank number)
//!   0x3000–0x3FFF  ROM bank hi  (bit 8 — the 9th bit)
//!   0x4000–0x5FFF  RAM bank     (4 bits, 0x00–0x0F)

const ROM_BANK_SIZE: usize = 0x4000; // 16 KiB
const RAM_BANK_SIZE: usize = 0x2000; //  8 KiB

pub struct Mbc5 {
    rom: Vec<u8>,
    ram: Vec<u8>,

    rom_bank_lo: u8,  // bits 7–0 of bank number
    rom_bank_hi: u8,  // bit 8 of bank number
    ram_bank:    u8,  // 4-bit RAM bank selector
    ram_enabled: bool,
}

impl Mbc5 {
    pub fn new(rom: Vec<u8>, num_ram_banks: u8) -> Self {
        let ram_size = RAM_BANK_SIZE * (num_ram_banks as usize).max(1);
        Mbc5 {
            rom,
            ram: vec![0u8; ram_size],
            rom_bank_lo: 1,
            rom_bank_hi: 0,
            ram_bank:    0,
            ram_enabled: false,
        }
    }

    fn rom_bank_0_index(&self) -> usize { 0 } // always physical bank 0

    fn rom_bank_n_index(&self) -> usize {
        let bank = ((self.rom_bank_hi as usize) << 8) | (self.rom_bank_lo as usize);
        let num_banks = (self.rom.len() / ROM_BANK_SIZE).max(2);
        bank & (num_banks - 1)
    }

    pub fn read_rom(&self, addr: u16) -> u8 {
        let (bank, offset) = if addr < 0x4000 {
            (self.rom_bank_0_index(), addr as usize)
        } else {
            (self.rom_bank_n_index(), (addr as usize) - 0x4000)
        };
        let physical = bank * ROM_BANK_SIZE + offset;
        self.rom.get(physical).copied().unwrap_or(0xFF)
    }

    pub fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x2FFF => {
                self.rom_bank_lo = value;
            }
            0x3000..=0x3FFF => {
                self.rom_bank_hi = value & 0x01;
            }
            0x4000..=0x5FFF => {
                self.ram_bank = value & 0x0F;
            }
            _ => {}
        }
    }

    pub fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled { return 0xFF; }
        let offset = self.ram_bank as usize * RAM_BANK_SIZE + addr as usize;
        self.ram.get(offset).copied().unwrap_or(0xFF)
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        if !self.ram_enabled { return; }
        let offset = self.ram_bank as usize * RAM_BANK_SIZE + addr as usize;
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

    fn mbc5_with_banks(num_banks: usize, num_ram_banks: u8) -> Mbc5 {
        let mut rom = vec![0u8; num_banks * ROM_BANK_SIZE];
        for bank in 0..num_banks {
            let start = bank * ROM_BANK_SIZE;
            for byte in &mut rom[start..start + ROM_BANK_SIZE] {
                *byte = (bank & 0xFF) as u8;
            }
        }
        Mbc5::new(rom, num_ram_banks)
    }

    // ── default state ─────────────────────────────────────────────────────────

    #[test]
    fn test_bank0_always_reads_physical_bank_0() {
        let mbc = mbc5_with_banks(4, 0);
        assert_eq!(mbc.read_rom(0x0000), 0x00);
    }

    #[test]
    fn test_default_bank_n_reads_bank_1() {
        let mbc = mbc5_with_banks(4, 0);
        assert_eq!(mbc.read_rom(0x4000), 0x01);
    }

    // ── ROM bank switching ────────────────────────────────────────────────────

    #[test]
    fn test_select_bank_0_reads_bank_0() {
        // MBC5 allows bank 0 in the upper window (unlike MBC1)
        let mut mbc = mbc5_with_banks(4, 0);
        mbc.write_rom(0x2000, 0x00);
        assert_eq!(mbc.read_rom(0x4000), 0x00);
    }

    #[test]
    fn test_select_bank_2() {
        let mut mbc = mbc5_with_banks(4, 0);
        mbc.write_rom(0x2000, 0x02);
        assert_eq!(mbc.read_rom(0x4000), 0x02);
    }

    #[test]
    fn test_select_bank_3() {
        let mut mbc = mbc5_with_banks(4, 0);
        mbc.write_rom(0x2000, 0x03);
        assert_eq!(mbc.read_rom(0x4000), 0x03);
    }

    #[test]
    fn test_hi_bit_selects_bank_256_plus() {
        // 512 banks so hi bit matters: hi=1, lo=0 → bank 256
        let mut mbc = mbc5_with_banks(512, 0);
        mbc.write_rom(0x2000, 0x00); // lo = 0
        mbc.write_rom(0x3000, 0x01); // hi = 1 → bank 256
        assert_eq!(mbc.read_rom(0x4000), (256 & 0xFF) as u8);
    }

    #[test]
    fn test_bank_mask_wraps_for_small_rom() {
        // 4 banks → mask = 3; selecting bank 5 → 5 & 3 = 1
        let mut mbc = mbc5_with_banks(4, 0);
        mbc.write_rom(0x2000, 0x05);
        assert_eq!(mbc.read_rom(0x4000), 0x01);
    }

    // ── RAM ───────────────────────────────────────────────────────────────────

    #[test]
    fn test_ram_disabled_reads_0xff() {
        let mbc = mbc5_with_banks(4, 1);
        assert_eq!(mbc.read_ram(0x0000), 0xFF);
    }

    #[test]
    fn test_ram_enable_allows_write_and_read() {
        let mut mbc = mbc5_with_banks(4, 1);
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_ram(0x0000, 0x55);
        assert_eq!(mbc.read_ram(0x0000), 0x55);
    }

    #[test]
    fn test_ram_bank_switching() {
        let mut mbc = mbc5_with_banks(4, 4);
        mbc.write_rom(0x0000, 0x0A); // enable RAM
        mbc.write_rom(0x4000, 0x01); // select RAM bank 1
        mbc.write_ram(0x0000, 0xAB);
        mbc.write_rom(0x4000, 0x00); // switch to bank 0
        assert_eq!(mbc.read_ram(0x0000), 0x00, "Bank 0 must not see bank 1 data");
        mbc.write_rom(0x4000, 0x01);
        assert_eq!(mbc.read_ram(0x0000), 0xAB, "Bank 1 data must survive switch");
    }

    #[test]
    fn test_ram_disable_prevents_read() {
        let mut mbc = mbc5_with_banks(4, 1);
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_ram(0x0000, 0x42);
        mbc.write_rom(0x0000, 0x00); // disable
        assert_eq!(mbc.read_ram(0x0000), 0xFF);
    }
}