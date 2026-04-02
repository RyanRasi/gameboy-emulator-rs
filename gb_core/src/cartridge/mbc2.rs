//! MBC2 — Memory Bank Controller 2.
//!
//! MBC2 is unique:
//!   - Up to 256 KiB ROM (16 × 16 KiB banks)
//!   - Built-in 512 × 4-bit RAM (NOT addressable as full bytes — upper nibble
//!     is always 0xF when read). No external RAM chip.
//!
//! Write register rules:
//!   Address bit 8 determines function:
//!     Bit 8 = 0 → RAM enable (lower nibble 0x0A = enable, else disable)
//!     Bit 8 = 1 → ROM bank select (lower 4 bits, 0 → 1)
//!
//! RAM is only 512 nibbles = 512 bytes at 0xA000–0xA1FF (mirrored to 0xBFFF).
//! Upper nibble of each byte always reads as 0xF.

const ROM_BANK_SIZE: usize  = 0x4000; // 16 KiB
const MBC2_RAM_SIZE: usize  = 512;    // 512 nibbles stored as bytes

pub struct Mbc2 {
    rom:         Vec<u8>,
    /// 512-byte internal RAM (only lower nibble valid).
    ram:         [u8; MBC2_RAM_SIZE],
    rom_bank:    u8,
    ram_enabled: bool,
}

impl Mbc2 {
    pub fn new(rom: Vec<u8>) -> Self {
        Mbc2 {
            rom,
            ram:         [0u8; MBC2_RAM_SIZE],
            rom_bank:    1,
            ram_enabled: false,
        }
    }

    fn rom_bank_index(&self) -> usize {
        let num_banks = (self.rom.len() / ROM_BANK_SIZE).max(2);
        (self.rom_bank as usize) & (num_banks - 1)
    }

    pub fn read_rom(&self, addr: u16) -> u8 {
        let (bank, offset) = if addr < 0x4000 {
            (0, addr as usize)
        } else {
            (self.rom_bank_index(), (addr as usize) - 0x4000)
        };
        let physical = bank * ROM_BANK_SIZE + offset;
        self.rom.get(physical).copied().unwrap_or(0xFF)
    }

    pub fn write_rom(&mut self, addr: u16, value: u8) {
        // Bit 8 of address selects function
        if addr & 0x0100 == 0 {
            // RAM enable
            self.ram_enabled = (value & 0x0F) == 0x0A;
        } else {
            // ROM bank select (4-bit, 0 → 1)
            let bank = value & 0x0F;
            self.rom_bank = if bank == 0 { 1 } else { bank };
        }
    }

    pub fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled { return 0xFF; }
        // Mirror across 512-byte window
        let offset = (addr as usize) % MBC2_RAM_SIZE;
        // Upper nibble always reads as 0xF
        0xF0 | (self.ram[offset] & 0x0F)
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        if !self.ram_enabled { return; }
        let offset = (addr as usize) % MBC2_RAM_SIZE;
        // Only lower nibble is stored
        self.ram[offset] = value & 0x0F;
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn mbc2_with_banks(num_banks: usize) -> Mbc2 {
        let mut rom = vec![0u8; num_banks * ROM_BANK_SIZE];
        for bank in 0..num_banks {
            let start = bank * ROM_BANK_SIZE;
            for byte in &mut rom[start..start + ROM_BANK_SIZE] {
                *byte = bank as u8;
            }
        }
        Mbc2::new(rom)
    }

    // ── ROM reads ─────────────────────────────────────────────────────────────

    #[test]
    fn test_bank0_always_reads_physical_bank_0() {
        let mbc = mbc2_with_banks(4);
        assert_eq!(mbc.read_rom(0x0000), 0x00);
    }

    #[test]
    fn test_default_bank_n_reads_bank_1() {
        let mbc = mbc2_with_banks(4);
        assert_eq!(mbc.read_rom(0x4000), 0x01);
    }

    #[test]
    fn test_rom_bank_switching() {
        let mut mbc = mbc2_with_banks(4);
        // Bit 8 set → ROM bank select
        mbc.write_rom(0x0100, 0x02);
        assert_eq!(mbc.read_rom(0x4000), 0x02);
    }

    #[test]
    fn test_bank_0_maps_to_bank_1() {
        let mut mbc = mbc2_with_banks(4);
        mbc.write_rom(0x0100, 0x00);
        assert_eq!(mbc.read_rom(0x4000), 0x01);
    }

    #[test]
    fn test_rom_bank_only_4_bits() {
        // Lower 4 bits only: 0x12 & 0x0F = 2
        let mut mbc = mbc2_with_banks(4);
        mbc.write_rom(0x0100, 0x12);
        assert_eq!(mbc.read_rom(0x4000), 0x02);
    }

    // ── RAM enable ────────────────────────────────────────────────────────────

    #[test]
    fn test_ram_disabled_by_default() {
        let mbc = mbc2_with_banks(4);
        assert_eq!(mbc.read_ram(0x0000), 0xFF);
    }

    #[test]
    fn test_ram_enable_via_bit8_clear() {
        let mut mbc = mbc2_with_banks(4);
        // Bit 8 clear → RAM enable; lower nibble 0x0A
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_ram(0x0000, 0x07);
        assert_eq!(mbc.read_ram(0x0000) & 0x0F, 0x07);
    }

    #[test]
    fn test_ram_disabled_ignores_writes() {
        let mut mbc = mbc2_with_banks(4);
        mbc.write_ram(0x0000, 0x07);
        mbc.write_rom(0x0000, 0x0A); // now enable
        assert_eq!(mbc.read_ram(0x0000) & 0x0F, 0x00);
    }

    // ── RAM nibble-only storage ───────────────────────────────────────────────

    #[test]
    fn test_ram_upper_nibble_always_f() {
        let mut mbc = mbc2_with_banks(4);
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_ram(0x0000, 0xFF);
        assert_eq!(mbc.read_ram(0x0000), 0xFF); // 0xF0 | 0x0F
    }

    #[test]
    fn test_ram_stores_only_lower_nibble() {
        let mut mbc = mbc2_with_banks(4);
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_ram(0x0001, 0xAB); // 0xAB & 0x0F = 0x0B
        assert_eq!(mbc.read_ram(0x0001) & 0x0F, 0x0B);
    }

    #[test]
    fn test_ram_mirrors_across_512_bytes() {
        let mut mbc = mbc2_with_banks(4);
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_ram(0x0000, 0x05);
        // 0xA200 mirrors to 0xA000 (offset % 512 = 0)
        assert_eq!(mbc.read_ram(0x0200) & 0x0F, 0x05);
    }

    #[test]
    fn test_ram_size_is_512_bytes() {
        assert_eq!(MBC2_RAM_SIZE, 512);
    }
}