//! MBC0 — No MBC (plain ROM only, max 32 KiB).
//!
//! ROM is split into two fixed 16 KiB banks:
//!   0x0000–0x3FFF  Bank 0 (always)
//!   0x4000–0x7FFF  Bank 1 (always)
//!
//! No RAM, no bank switching. Writes to ROM space are silently ignored.

pub struct Mbc0 {
    rom: Vec<u8>,
}

impl Mbc0 {
    pub fn new(rom: Vec<u8>) -> Self {
        Mbc0 { rom }
    }

    pub fn read_rom(&self, addr: u16) -> u8 {
        self.rom.get(addr as usize).copied().unwrap_or(0xFF)
    }

    /// Writes to ROM space are ignored on MBC0 hardware.
    pub fn write_rom(&mut self, _addr: u16, _value: u8) {}

    pub fn read_ram(&self, _addr: u16) -> u8 {
        0xFF // no external RAM
    }

    pub fn write_ram(&mut self, _addr: u16, _value: u8) {}
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn mbc0_with(data: &[u8]) -> Mbc0 {
        Mbc0::new(data.to_vec())
    }

    #[test]
    fn test_read_rom_bank0() {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0100] = 0xDE;
        let mbc = mbc0_with(&rom);
        assert_eq!(mbc.read_rom(0x0100), 0xDE);
    }

    #[test]
    fn test_read_rom_bank1() {
        let mut rom = vec![0u8; 0x8000];
        rom[0x4000] = 0xAD;
        let mbc = mbc0_with(&rom);
        assert_eq!(mbc.read_rom(0x4000), 0xAD);
    }

    #[test]
    fn test_read_beyond_rom_returns_0xff() {
        let rom = vec![0u8; 0x100]; // deliberately tiny
        let mbc = mbc0_with(&rom);
        assert_eq!(mbc.read_rom(0x7FFF), 0xFF);
    }

    #[test]
    fn test_write_rom_is_ignored() {
        let mut rom = vec![0xAAu8; 0x8000];
        let mut mbc = mbc0_with(&rom);
        mbc.write_rom(0x0000, 0x00);
        // ROM unchanged — reads still give original value
        assert_eq!(mbc.read_rom(0x0000), 0xAA);
    }

    #[test]
    fn test_read_ram_returns_0xff() {
        let mbc = mbc0_with(&[]);
        assert_eq!(mbc.read_ram(0x0000), 0xFF);
    }

    #[test]
    fn test_write_ram_is_ignored() {
        let mut mbc = mbc0_with(&[]);
        mbc.write_ram(0x0000, 0x42); // must not panic
    }
}