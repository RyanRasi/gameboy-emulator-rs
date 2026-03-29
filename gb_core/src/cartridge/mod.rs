//! Cartridge abstraction — selects the correct MBC from the ROM header.

pub mod header;
pub mod mbc0;
pub mod mbc1;
pub mod mbc5;

pub use header::{CartridgeHeader, mbc_type};

use mbc0::Mbc0;
use mbc1::Mbc1;
use mbc5::Mbc5;

enum Mbc {
    Mbc0(Mbc0),
    Mbc1(Mbc1),
    Mbc5(Mbc5),
}

pub struct Cartridge {
    pub header: CartridgeHeader,
    mbc: Mbc,
}

impl Cartridge {
    pub fn load(rom: Vec<u8>) -> Result<Self, String> {
        let header = CartridgeHeader::parse(&rom)?;

        let num_ram_banks = header::ram_banks(header.ram_size_code)
            .ok_or_else(|| format!("Unknown RAM size code: 0x{:02X}", header.ram_size_code))?;

        let mbc = match header.cartridge_type {
            mbc_type::ROM_ONLY => {
                Mbc::Mbc0(Mbc0::new(rom))
            }
            mbc_type::MBC1
            | mbc_type::MBC1_RAM
            | mbc_type::MBC1_RAM_BATTERY => {
                Mbc::Mbc1(Mbc1::new(rom, num_ram_banks))
            }
            mbc_type::MBC5
            | mbc_type::MBC5_RAM
            | mbc_type::MBC5_RAM_BATTERY
            | mbc_type::MBC5_RUMBLE
            | mbc_type::MBC5_RUMBLE_RAM
            | mbc_type::MBC5_RUMBLE_RAM_BATTERY => {
                Mbc::Mbc5(Mbc5::new(rom, num_ram_banks))
            }
            other => return Err(format!("Unsupported cartridge type: 0x{:02X}", other)),
        };

        Ok(Cartridge { header, mbc })
    }

    pub fn read_rom(&self, addr: u16) -> u8 {
        match &self.mbc {
            Mbc::Mbc0(m) => m.read_rom(addr),
            Mbc::Mbc1(m) => m.read_rom(addr),
            Mbc::Mbc5(m) => m.read_rom(addr),
        }
    }

    pub fn write_rom(&mut self, addr: u16, value: u8) {
        match &mut self.mbc {
            Mbc::Mbc0(m) => m.write_rom(addr, value),
            Mbc::Mbc1(m) => m.write_rom(addr, value),
            Mbc::Mbc5(m) => m.write_rom(addr, value),
        }
    }

    pub fn read_ram(&self, addr: u16) -> u8 {
        match &self.mbc {
            Mbc::Mbc0(m) => m.read_ram(addr),
            Mbc::Mbc1(m) => m.read_ram(addr),
            Mbc::Mbc5(m) => m.read_ram(addr),
        }
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        match &mut self.mbc {
            Mbc::Mbc0(m) => m.write_ram(addr, value),
            Mbc::Mbc1(m) => m.write_ram(addr, value),
            Mbc::Mbc5(m) => m.write_ram(addr, value),
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

    fn make_rom(size: usize, cartridge_type: u8, rom_size_code: u8, ram_size_code: u8) -> Vec<u8> {
        let mut rom = vec![0u8; size];
        rom[0x0147] = cartridge_type;
        rom[0x0148] = rom_size_code;
        rom[0x0149] = ram_size_code;
        let cs = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = cs;
        rom
    }

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
    fn test_load_mbc5_succeeds() {
        let rom = make_rom(0x8000, mbc_type::MBC5, 0x01, 0x00);
        assert!(Cartridge::load(rom).is_ok());
    }

    #[test]
    fn test_load_mbc5_ram_battery_succeeds() {
        let rom = make_rom(0x8000, mbc_type::MBC5_RAM_BATTERY, 0x01, 0x02);
        assert!(Cartridge::load(rom).is_ok());
    }

    #[test]
    fn test_load_mbc5_rumble_ram_battery_succeeds() {
        let rom = make_rom(0x8000, mbc_type::MBC5_RUMBLE_RAM_BATTERY, 0x01, 0x02);
        assert!(Cartridge::load(rom).is_ok());
    }

    #[test]
    fn test_load_unsupported_type_returns_error() {
        let rom = make_rom(0x8000, 0x20, 0x00, 0x00);
        assert!(Cartridge::load(rom).is_err());
    }

    #[test]
    fn test_load_too_short_returns_error() {
        assert!(Cartridge::load(vec![0u8; 10]).is_err());
    }

    #[test]
    fn test_mbc5_bank_switch_via_cartridge() {
        let mut big_rom = vec![0u8; 0x10000]; // 64 KiB = 4 banks
        big_rom[0x0147] = mbc_type::MBC5;
        big_rom[0x0148] = 0x01;
        big_rom[0x0149] = 0x00;
        for b in &mut big_rom[0x8000..0xC000] { *b = 0x22; }
        let cs = big_rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        big_rom[0x014D] = cs;
        let mut cart = Cartridge::load(big_rom).unwrap();
        cart.write_rom(0x2000, 0x02);
        assert_eq!(cart.read_rom(0x4000), 0x22);
    }

    #[test]
    fn test_mbc5_ram_read_write_via_cartridge() {
        let rom = make_rom(0x8000, mbc_type::MBC5_RAM_BATTERY, 0x01, 0x02);
        let mut cart = Cartridge::load(rom).unwrap();
        cart.write_rom(0x0000, 0x0A); // enable RAM
        cart.write_ram(0x0000, 0x77);
        assert_eq!(cart.read_ram(0x0000), 0x77);
    }

    #[test]
    fn test_mbc1_bank_switch_via_cartridge() {
        let mut big_rom = vec![0u8; 0x10000];
        big_rom[0x0147] = mbc_type::MBC1;
        big_rom[0x0148] = 0x01;
        big_rom[0x0149] = 0x00;
        for b in &mut big_rom[0x8000..0xC000] { *b = 0x22; }
        let cs = big_rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        big_rom[0x014D] = cs;
        let mut cart = Cartridge::load(big_rom).unwrap();
        cart.write_rom(0x2000, 0x02);
        assert_eq!(cart.read_rom(0x4000), 0x22);
    }
}