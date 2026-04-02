//! Cartridge abstraction — selects the correct MBC from the ROM header.

pub mod header;
pub mod mbc0;
pub mod mbc1;
pub mod mbc2;
pub mod mbc3;
pub mod mbc5;

pub use header::{CartridgeHeader, mbc_type};

use mbc0::Mbc0;
use mbc1::Mbc1;
use mbc2::Mbc2;
use mbc3::Mbc3;
use mbc5::Mbc5;

enum Mbc {
    Mbc0(Mbc0),
    Mbc1(Mbc1),
    Mbc2(Mbc2),
    Mbc3(Mbc3),
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
            mbc_type::MBC2
            | mbc_type::MBC2_BATTERY => {
                Mbc::Mbc2(Mbc2::new(rom))
            }
            mbc_type::MBC3_TIMER_BATTERY
            | mbc_type::MBC3_TIMER_RAM_BATTERY
            | mbc_type::MBC3
            | mbc_type::MBC3_RAM
            | mbc_type::MBC3_RAM_BATTERY => {
                Mbc::Mbc3(Mbc3::new(rom, num_ram_banks))
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

    /// Advance the RTC (only relevant for MBC3).
    /// Call this once per CPU tick with the T-cycle count.
    pub fn tick_rtc(&mut self, cycles: u64) {
        if let Mbc::Mbc3(ref mut m) = self.mbc {
            m.tick_rtc(cycles);
        }
    }

    pub fn read_rom(&self, addr: u16) -> u8 {
        match &self.mbc {
            Mbc::Mbc0(m) => m.read_rom(addr),
            Mbc::Mbc1(m) => m.read_rom(addr),
            Mbc::Mbc2(m) => m.read_rom(addr),
            Mbc::Mbc3(m) => m.read_rom(addr),
            Mbc::Mbc5(m) => m.read_rom(addr),
        }
    }

    pub fn write_rom(&mut self, addr: u16, value: u8) {
        match &mut self.mbc {
            Mbc::Mbc0(m) => m.write_rom(addr, value),
            Mbc::Mbc1(m) => m.write_rom(addr, value),
            Mbc::Mbc2(m) => m.write_rom(addr, value),
            Mbc::Mbc3(m) => m.write_rom(addr, value),
            Mbc::Mbc5(m) => m.write_rom(addr, value),
        }
    }

    pub fn read_ram(&self, addr: u16) -> u8 {
        match &self.mbc {
            Mbc::Mbc0(m) => m.read_ram(addr),
            Mbc::Mbc1(m) => m.read_ram(addr),
            Mbc::Mbc2(m) => m.read_ram(addr),
            Mbc::Mbc3(m) => m.read_ram(addr),
            Mbc::Mbc5(m) => m.read_ram(addr),
        }
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        match &mut self.mbc {
            Mbc::Mbc0(m) => m.write_ram(addr, value),
            Mbc::Mbc1(m) => m.write_ram(addr, value),
            Mbc::Mbc2(m) => m.write_ram(addr, value),
            Mbc::Mbc3(m) => m.write_ram(addr, value),
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

    fn make_rom(size: usize, cart_type: u8, rom_code: u8, ram_code: u8) -> Vec<u8> {
        let mut rom = vec![0u8; size];
        rom[0x0147] = cart_type;
        rom[0x0148] = rom_code;
        rom[0x0149] = ram_code;
        let cs = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = cs;
        rom
    }

    // ── Load variants ─────────────────────────────────────────────────────────

    #[test]
    fn test_load_rom_only() {
        assert!(Cartridge::load(make_rom(0x8000, mbc_type::ROM_ONLY, 0x00, 0x00)).is_ok());
    }

    #[test]
    fn test_load_mbc1() {
        assert!(Cartridge::load(make_rom(0x8000, mbc_type::MBC1, 0x01, 0x00)).is_ok());
    }

    #[test]
    fn test_load_mbc2() {
        assert!(Cartridge::load(make_rom(0x8000, mbc_type::MBC2, 0x02, 0x00)).is_ok());
    }

    #[test]
    fn test_load_mbc2_battery() {
        assert!(Cartridge::load(make_rom(0x8000, mbc_type::MBC2_BATTERY, 0x02, 0x00)).is_ok());
    }

    #[test]
    fn test_load_mbc3() {
        assert!(Cartridge::load(make_rom(0x8000, mbc_type::MBC3, 0x01, 0x00)).is_ok());
    }

    #[test]
    fn test_load_mbc3_timer_battery() {
        assert!(Cartridge::load(make_rom(0x8000, mbc_type::MBC3_TIMER_BATTERY, 0x01, 0x00)).is_ok());
    }

    #[test]
    fn test_load_mbc3_ram_battery() {
        assert!(Cartridge::load(make_rom(0x8000, mbc_type::MBC3_RAM_BATTERY, 0x01, 0x02)).is_ok());
    }

    #[test]
    fn test_load_mbc5() {
        assert!(Cartridge::load(make_rom(0x8000, mbc_type::MBC5, 0x01, 0x00)).is_ok());
    }

    #[test]
    fn test_load_unsupported_returns_error() {
        assert!(Cartridge::load(make_rom(0x8000, 0x20, 0x00, 0x00)).is_err());
    }

    // ── MBC2 via Cartridge ────────────────────────────────────────────────────

    #[test]
    fn test_mbc2_rom_read_via_cartridge() {
        let mut rom = make_rom(0x8000, mbc_type::MBC2, 0x02, 0x00);
        rom[0x4000] = 0xAB;
        let cs = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = cs;
        let cart = Cartridge::load(rom).unwrap();
        assert_eq!(cart.read_rom(0x4000), 0xAB);
    }

    #[test]
    fn test_mbc2_ram_read_write_via_cartridge() {
        let rom = make_rom(0x8000, mbc_type::MBC2, 0x02, 0x00);
        let mut cart = Cartridge::load(rom).unwrap();
        cart.write_rom(0x0000, 0x0A); // enable RAM
        cart.write_ram(0x0000, 0x07);
        assert_eq!(cart.read_ram(0x0000) & 0x0F, 0x07);
    }

    // ── MBC3 via Cartridge ────────────────────────────────────────────────────

    #[test]
    fn test_mbc3_rom_bank_switch_via_cartridge() {
        let mut big_rom = vec![0u8; 0x10000]; // 4 banks
        big_rom[0x0147] = mbc_type::MBC3;
        big_rom[0x0148] = 0x01;
        big_rom[0x0149] = 0x00;
        for b in &mut big_rom[0x8000..0xC000] { *b = 0x33; }
        let cs = big_rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        big_rom[0x014D] = cs;
        let mut cart = Cartridge::load(big_rom).unwrap();
        cart.write_rom(0x2000, 0x02);
        assert_eq!(cart.read_rom(0x4000), 0x33);
    }

    #[test]
    fn test_mbc3_ram_via_cartridge() {
        let rom = make_rom(0x8000, mbc_type::MBC3_RAM_BATTERY, 0x01, 0x02);
        let mut cart = Cartridge::load(rom).unwrap();
        cart.write_rom(0x0000, 0x0A);
        cart.write_ram(0x0000, 0x55);
        assert_eq!(cart.read_ram(0x0000), 0x55);
    }

    #[test]
    fn test_mbc3_rtc_tick_via_cartridge() {
        let rom = make_rom(0x8000, mbc_type::MBC3_TIMER_BATTERY, 0x01, 0x00);
        let mut cart = Cartridge::load(rom).unwrap();
        // Tick one full second
        cart.tick_rtc(4_194_304);
        // Enable RAM+RTC, latch, select RTC S, check seconds = 1
        cart.write_rom(0x0000, 0x0A);
        cart.write_rom(0x6000, 0x00);
        cart.write_rom(0x6000, 0x01);
        cart.write_rom(0x4000, 0x08);
        assert_eq!(cart.read_ram(0x0000), 1);
    }
}