//! Cartridge header parsing.

pub mod mbc_type {
    pub const ROM_ONLY:                u8 = 0x00;
    pub const MBC1:                    u8 = 0x01;
    pub const MBC1_RAM:                u8 = 0x02;
    pub const MBC1_RAM_BATTERY:        u8 = 0x03;
    pub const MBC2:                    u8 = 0x05;
    pub const MBC2_BATTERY:            u8 = 0x06;
    pub const MBC3_TIMER_BATTERY:      u8 = 0x0F;
    pub const MBC3_TIMER_RAM_BATTERY:  u8 = 0x10;
    pub const MBC3:                    u8 = 0x11;
    pub const MBC3_RAM:                u8 = 0x12;
    pub const MBC3_RAM_BATTERY:        u8 = 0x13;
    pub const MBC5:                    u8 = 0x19;
    pub const MBC5_RAM:                u8 = 0x1A;
    pub const MBC5_RAM_BATTERY:        u8 = 0x1B;
    pub const MBC5_RUMBLE:             u8 = 0x1C;
    pub const MBC5_RUMBLE_RAM:         u8 = 0x1D;
    pub const MBC5_RUMBLE_RAM_BATTERY: u8 = 0x1E;
}

pub const HEADER_END: usize = 0x014E;

#[derive(Debug, Clone)]
pub struct CartridgeHeader {
    pub title:          String,
    pub cgb_flag:       u8,   // 0x80 = CGB compatible, 0xC0 = CGB only
    pub cartridge_type: u8,
    pub rom_size_code:  u8,
    pub ram_size_code:  u8,
    pub checksum:       u8,
}

impl CartridgeHeader {
    pub fn parse(rom: &[u8]) -> Result<Self, String> {
        if rom.len() < 0x0150 {
            return Err("ROM too short to contain header".into());
        }

        // Title: 0x0134-0x0143 (16 bytes, may include manufacturer/CGB flag)
        let title_bytes = &rom[0x0134..=0x0143];
        let title = title_bytes.iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect::<String>()
            .trim()
            .to_string();

        let cgb_flag       = rom[0x0143];
        let cartridge_type = rom[0x0147];
        let rom_size_code  = rom[0x0148];
        let ram_size_code  = rom[0x0149];
        let checksum       = rom[0x014D];

        // Verify header checksum
        let calc: u8 = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        if calc != checksum {
            return Err(format!(
                "Header checksum mismatch: expected 0x{:02X}, got 0x{:02X}",
                checksum, calc
            ));
        }

        Ok(CartridgeHeader { title, cgb_flag, cartridge_type, rom_size_code, ram_size_code, checksum })
    }

    /// Returns true if this cartridge supports/requires CGB mode.
    pub fn is_cgb(&self) -> bool {
        self.cgb_flag == 0x80 || self.cgb_flag == 0xC0
    }

    /// Returns true if this is a DMG-only cartridge.
    pub fn is_dmg_only(&self) -> bool {
        !self.is_cgb()
    }
}

/// Return the number of RAM banks for a given RAM size code.
pub fn ram_banks(code: u8) -> Option<u8> {
    match code {
        0x00 => Some(0),
        0x01 => Some(0), // 2KB — treat as 0 banks for MBC purposes
        0x02 => Some(1),
        0x03 => Some(4),
        0x04 => Some(16),
        0x05 => Some(8),
        _    => Some(0), // unknown — allow loading
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_header(title: &[u8], cgb_flag: u8, cart_type: u8, rom_code: u8, ram_code: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x0150];
        let len = title.len().min(16);
        rom[0x0134..0x0134+len].copy_from_slice(&title[..len]);
        rom[0x0143] = cgb_flag;
        rom[0x0147] = cart_type;
        rom[0x0148] = rom_code;
        rom[0x0149] = ram_code;
        let cs: u8 = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |a, &b| a.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = cs;
        rom
    }

    #[test]
    fn test_parse_dmg_header() {
        let rom = make_header(b"TETRIS", 0x00, 0x00, 0x00, 0x00);
        let h = CartridgeHeader::parse(&rom).unwrap();
        assert_eq!(h.title, "TETRIS");
        assert!(!h.is_cgb());
        assert!(h.is_dmg_only());
    }

    #[test]
    fn test_parse_cgb_compatible() {
        let rom = make_header(b"POKEMON", 0x80, 0x00, 0x00, 0x00);
        let h = CartridgeHeader::parse(&rom).unwrap();
        assert!(h.is_cgb());
    }

    #[test]
    fn test_parse_cgb_only() {
        let rom = make_header(b"CRYSTAL", 0xC0, 0x00, 0x00, 0x00);
        let h = CartridgeHeader::parse(&rom).unwrap();
        assert!(h.is_cgb());
    }

    #[test]
    fn test_bad_checksum_returns_err() {
        let mut rom = make_header(b"TEST", 0x00, 0x00, 0x00, 0x00);
        rom[0x014D] ^= 0xFF; // corrupt checksum
        assert!(CartridgeHeader::parse(&rom).is_err());
    }

    #[test]
    fn test_too_short_returns_err() {
        assert!(CartridgeHeader::parse(&[0u8; 10]).is_err());
    }

    #[test]
    fn test_ram_banks() {
        assert_eq!(ram_banks(0x00), Some(0));
        assert_eq!(ram_banks(0x02), Some(1));
        assert_eq!(ram_banks(0x03), Some(4));
    }
}