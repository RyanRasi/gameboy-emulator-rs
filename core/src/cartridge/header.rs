//! Game Boy cartridge header parser.
//!
//! The header lives at 0x0100–0x014F in every ROM:
//!
//!   0x0100–0x0103  Entry point (usually NOP + JP nn)
//!   0x0104–0x0133  Nintendo logo (must match for boot)
//!   0x0134–0x0143  Title (ASCII, upper-case, padded with 0x00)
//!   0x0147         Cartridge type (MBC variant)
//!   0x0148         ROM size code
//!   0x0149         RAM size code
//!   0x014D         Header checksum (verified by BIOS)
//!   0x014E–0x014F  Global checksum (not verified at runtime)

/// Parsed representation of the cartridge header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeHeader {
    pub title:          String,
    pub cartridge_type: u8,
    pub rom_size_code:  u8,
    pub ram_size_code:  u8,
    pub header_checksum: u8,
}

/// The minimum ROM size that contains a complete header.
pub const HEADER_END: usize = 0x0150;

/// MBC type identifiers from byte 0x0147.
pub mod mbc_type {
    pub const ROM_ONLY:           u8 = 0x00;
    pub const MBC1:               u8 = 0x01;
    pub const MBC1_RAM:           u8 = 0x02;
    pub const MBC1_RAM_BATTERY:   u8 = 0x03;
    pub const MBC5:               u8 = 0x19;
    pub const MBC5_RAM:           u8 = 0x1A;
    pub const MBC5_RAM_BATTERY:   u8 = 0x1B;
    pub const MBC5_RUMBLE:        u8 = 0x1C;
    pub const MBC5_RUMBLE_RAM:    u8 = 0x1D;
    pub const MBC5_RUMBLE_RAM_BATTERY: u8 = 0x1E;
}

/// Return the total number of ROM banks for a given ROM size code (0x0148).
/// Each bank is 16 KiB. Returns None for unrecognised codes.
pub fn rom_banks(code: u8) -> Option<u16> {
    match code {
        0x00 => Some(2),   //  32 KiB
        0x01 => Some(4),   //  64 KiB
        0x02 => Some(8),   // 128 KiB
        0x03 => Some(16),  // 256 KiB
        0x04 => Some(32),  // 512 KiB
        0x05 => Some(64),  //   1 MiB
        0x06 => Some(128), //   2 MiB
        _ => None,
    }
}

/// Return the total number of RAM banks for a given RAM size code (0x0149).
/// Each bank is 8 KiB. Returns None for unrecognised codes.
pub fn ram_banks(code: u8) -> Option<u8> {
    match code {
        0x00 => Some(0),
        0x01 => Some(0), // unused in practice
        0x02 => Some(1),
        0x03 => Some(4),
        0x04 => Some(16),
        0x05 => Some(8),
        _ => None,
    }
}

impl CartridgeHeader {
    /// Parse the header from the raw ROM bytes.
    /// Returns an error if the ROM is too short or the checksum is wrong.
    pub fn parse(rom: &[u8]) -> Result<Self, String> {
        if rom.len() < HEADER_END {
            return Err(format!(
                "ROM too short: {} bytes (minimum {})",
                rom.len(), HEADER_END
            ));
        }

        // Title: bytes 0x0134–0x0143, null-terminated ASCII
        let title_bytes = &rom[0x0134..=0x0143];
        let title = title_bytes
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect::<String>();

        let cartridge_type  = rom[0x0147];
        let rom_size_code   = rom[0x0148];
        let ram_size_code   = rom[0x0149];
        let header_checksum = rom[0x014D];

        // Validate header checksum: x=0; for i in 0x0134..=0x014C: x=x-rom[i]-1
        let computed = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));

        if computed != header_checksum {
            return Err(format!(
                "Header checksum mismatch: computed 0x{:02X}, stored 0x{:02X}",
                computed, header_checksum
            ));
        }

        Ok(CartridgeHeader {
            title,
            cartridge_type,
            rom_size_code,
            ram_size_code,
            header_checksum,
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid ROM of `size` bytes with the header fields set.
    /// Computes and inserts the correct checksum automatically.
    fn make_rom(
        size:           usize,
        title:          &[u8],   // up to 16 bytes, placed at 0x0134
        cartridge_type: u8,
        rom_size_code:  u8,
        ram_size_code:  u8,
    ) -> Vec<u8> {
        assert!(size >= HEADER_END);
        let mut rom = vec![0u8; size];
        let end = title.len().min(16);
        rom[0x0134..0x0134 + end].copy_from_slice(&title[..end]);
        rom[0x0147] = cartridge_type;
        rom[0x0148] = rom_size_code;
        rom[0x0149] = ram_size_code;
        // Compute and store checksum
        let checksum = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = checksum;
        rom
    }

    // ── parse success ─────────────────────────────────────────────────────────

    #[test]
    fn test_parse_valid_rom_only_header() {
        let rom = make_rom(HEADER_END, b"TESTGAME", 0x00, 0x00, 0x00);
        let header = CartridgeHeader::parse(&rom).unwrap();
        assert_eq!(header.title, "TESTGAME");
        assert_eq!(header.cartridge_type, 0x00);
        assert_eq!(header.rom_size_code, 0x00);
        assert_eq!(header.ram_size_code, 0x00);
    }

    #[test]
    fn test_parse_title_null_terminated() {
        // Title has embedded null — should stop there
        let mut title = [0u8; 16];
        title[..4].copy_from_slice(b"ABCD");
        let rom = make_rom(HEADER_END, &title, 0x00, 0x00, 0x00);
        let header = CartridgeHeader::parse(&rom).unwrap();
        assert_eq!(header.title, "ABCD");
    }

    #[test]
    fn test_parse_full_16_char_title() {
        let rom = make_rom(HEADER_END, b"ABCDEFGHIJKLMNOP", 0x00, 0x00, 0x00);
        let header = CartridgeHeader::parse(&rom).unwrap();
        assert_eq!(header.title.len(), 16);
    }

    #[test]
    fn test_parse_mbc1_cartridge_type() {
        let rom = make_rom(HEADER_END, b"MBC1GAME", mbc_type::MBC1, 0x05, 0x02);
        let header = CartridgeHeader::parse(&rom).unwrap();
        assert_eq!(header.cartridge_type, mbc_type::MBC1);
        assert_eq!(header.rom_size_code, 0x05);
        assert_eq!(header.ram_size_code, 0x02);
    }

    #[test]
    fn test_parse_checksum_stored_correctly() {
        let rom = make_rom(HEADER_END, b"CHECKTEST", 0x00, 0x00, 0x00);
        let header = CartridgeHeader::parse(&rom).unwrap();
        assert_eq!(header.header_checksum, rom[0x014D]);
    }

    // ── parse failure ─────────────────────────────────────────────────────────

    #[test]
    fn test_parse_rom_too_short() {
        let rom = vec![0u8; 10];
        assert!(CartridgeHeader::parse(&rom).is_err());
    }

    #[test]
    fn test_parse_bad_checksum_returns_error() {
        let mut rom = make_rom(HEADER_END, b"BADCHECK", 0x00, 0x00, 0x00);
        rom[0x014D] = rom[0x014D].wrapping_add(1); // corrupt checksum
        assert!(CartridgeHeader::parse(&rom).is_err());
    }

    // ── rom_banks / ram_banks ─────────────────────────────────────────────────

    #[test]
    fn test_rom_banks_code_0_gives_2() {
        assert_eq!(rom_banks(0x00), Some(2));
    }

    #[test]
    fn test_rom_banks_code_6_gives_128() {
        assert_eq!(rom_banks(0x06), Some(128));
    }

    #[test]
    fn test_rom_banks_unknown_code_returns_none() {
        assert_eq!(rom_banks(0xFF), None);
    }

    #[test]
    fn test_ram_banks_code_0_gives_0() {
        assert_eq!(ram_banks(0x00), Some(0));
    }

    #[test]
    fn test_ram_banks_code_3_gives_4() {
        assert_eq!(ram_banks(0x03), Some(4));
    }

    #[test]
    fn test_ram_banks_unknown_code_returns_none() {
        assert_eq!(ram_banks(0xFF), None);
    }
}