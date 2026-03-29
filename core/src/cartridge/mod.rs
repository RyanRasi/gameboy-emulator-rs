//! Cartridge abstraction.
//!
//! `Cartridge::load()` inspects the header, validates the ROM, and selects
//! the correct MBC implementation. The MMU calls `read_rom`, `write_rom`,
//! `read_ram`, and `write_ram` without knowing which MBC is active.

pub mod header;
pub mod mbc0;
pub mod mbc1;

pub use header::{CartridgeHeader, mbc_type};

use mbc0::Mbc0;
use mbc1::Mbc1;

/// Internal MBC dispatch enum.
enum Mbc {
    Mbc0(Mbc0),
    Mbc1(Mbc1),
}

/// A fully loaded cartridge: parsed header + active MBC.
pub struct Cartridge {
    pub header: CartridgeHeader,
    mbc: Mbc,
}

impl Cartridge {
    /// Parse the ROM, select the correct MBC, and return a ready-to-use
    /// Cartridge. Returns an error for unsupported MBC types.
    pub fn load(rom: Vec<u8>) -> Result<Self, String> {
        let header = CartridgeHeader::parse(&rom)?;

        let num_ram_banks = header::ram_banks(header.ram_size_code)
            .ok_or_else(|| format!("Unknown RAM size code: 0x{:02X}", header.ram_size_code))?;

        let mbc = match header.cartridge_type {
            mbc_type::ROM_ONLY => Mbc::Mbc0(Mbc0::new(rom)),
            mbc_type::MBC1
            | mbc_type::MBC1_RAM
            | mbc_type::MBC1_RAM_BATTERY => Mbc::Mbc1(Mbc1::new(rom, num_ram_banks)),
            other => return Err(format!("Unsupported cartridge type: 0x{:02X}", other)),
        };

        Ok(Cartridge { header, mbc })
    }

    // ── Unified read/write interface used by the MMU ──────────────────────────

    pub fn read_rom(&self, addr: u16) -> u8 {
        match &self.mbc {
            Mbc::Mbc0(m) => m.read_rom(addr),
            Mbc::Mbc1(m) => m.read_rom(addr),
        }
    }

    pub fn write_rom(&mut self, addr: u16, value: u8) {
        match &mut self.mbc {
            Mbc::Mbc0(m) => m.write_rom(addr, value),
            Mbc::Mbc1(m) => m.write_rom(addr, value),
        }
    }

    pub fn read_ram(&self, addr: u16) -> u8 {
        match &self.mbc {
            Mbc::Mbc0(m) => m.read_ram(addr),
            Mbc::Mbc1(m) => m.read_ram(addr),
        }
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        match &mut self.mbc {
            Mbc::Mbc0(m) => m.write_ram(addr, value),
            Mbc::Mbc1(m) => m.write_ram(addr, value),
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use header::HEADER_END;

    /// Build a minimal valid ROM with the given type and size.
    fn make_rom(size: usize, cartridge_type: u8, rom_size_code: u8, ram_size_code: u8) -> Vec<u8> {
        let mut rom = vec![0u8; size];
        rom[0x0147] = cartridge_type;
        rom[0x0148] = rom_size_code;
        rom[0x0149] = ram_size_code;
        let checksum = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = checksum;
        rom
    }

    // ── Cartridge::load ───────────────────────────────────────────────────────

    #[test]
    fn test_load_rom_only_succeeds() {
        let rom = make_rom(0x8000, mbc_type::ROM_ONLY, 0x00, 0x00);
        assert!(Cartridge::load(rom).is_ok());
    }

    #[test]
    fn test_load_mbc1_succeeds() {
        let rom = make_rom(0x8000, mbc_type::MBC1, 0x01, 0x00);
        assert!(Cartridge::load(rom).is_ok());
    }

    #[test]
    fn test_load_mbc1_ram_succeeds() {
        let rom = make_rom(0x8000, mbc_type::MBC1_RAM, 0x01, 0x02);
        assert!(Cartridge::load(rom).is_ok());
    }

    #[test]
    fn test_load_unsupported_type_returns_error() {
        let rom = make_rom(0x8000, 0x20, 0x00, 0x00); // MBC6 — unsupported
        assert!(Cartridge::load(rom).is_err());
    }

    #[test]
    fn test_load_too_short_returns_error() {
        let rom = vec![0u8; 10];
        assert!(Cartridge::load(rom).is_err());
    }

    #[test]
    fn test_header_parsed_correctly_on_load() {
        let mut rom = make_rom(0x8000, mbc_type::ROM_ONLY, 0x00, 0x00);
        rom[0x0134..0x013C].copy_from_slice(b"MYGAME!!");
        // Recompute checksum after title change
        let checksum = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = checksum;
        let cart = Cartridge::load(rom).unwrap();
        assert_eq!(cart.header.title, "MYGAME!!");
    }

    // ── ROM reads through Cartridge ───────────────────────────────────────────

    #[test]
    fn test_rom_read_bank0_via_cartridge() {
        let mut rom = make_rom(0x8000, mbc_type::ROM_ONLY, 0x00, 0x00);
        rom[0x0150] = 0xAB;
        let cart = Cartridge::load(rom).unwrap();
        assert_eq!(cart.read_rom(0x0150), 0xAB);
    }

    #[test]
    fn test_rom_read_bank1_via_cartridge() {
        let mut rom = make_rom(0x8000, mbc_type::ROM_ONLY, 0x00, 0x00);
        rom[0x4001] = 0xCD;
        let cart = Cartridge::load(rom).unwrap();
        assert_eq!(cart.read_rom(0x4001), 0xCD);
    }

    // ── MBC1 bank switching through Cartridge ─────────────────────────────────

    #[test]
    fn test_mbc1_bank_switch_via_cartridge() {
        // 4 banks: bank 2 filled with 0x22
        let mut rom = make_rom(0x8000, mbc_type::MBC1, 0x01, 0x00);
        for b in &mut rom[0x8000 - 0x4000..] {
            // 4-bank ROM: bank 2 = offset 0x8000.. not applicable for 32KiB
        }
        // Simpler: build a 4-bank ROM manually
        let mut big_rom = vec![0u8; 0x10000]; // 64 KiB = 4 banks
        big_rom[0x0147] = mbc_type::MBC1;
        big_rom[0x0148] = 0x01; // 4 banks
        big_rom[0x0149] = 0x00;
        let checksum = big_rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        big_rom[0x014D] = checksum;
        // Fill bank 2 (offset 0x8000) with 0x22
        for b in &mut big_rom[0x8000..0xC000] { *b = 0x22; }

        let mut cart = Cartridge::load(big_rom).unwrap();
        cart.write_rom(0x2000, 0x02); // select bank 2
        assert_eq!(cart.read_rom(0x4000), 0x22);
    }

    // ── MBC1 RAM through Cartridge ────────────────────────────────────────────

    #[test]
    fn test_mbc1_ram_read_write_via_cartridge() {
        let rom = make_rom(0x8000, mbc_type::MBC1_RAM, 0x01, 0x02); // 1 RAM bank
        let mut cart = Cartridge::load(rom).unwrap();
        cart.write_rom(0x0000, 0x0A); // enable RAM
        cart.write_ram(0x0000, 0x99);
        assert_eq!(cart.read_ram(0x0000), 0x99);
    }
}