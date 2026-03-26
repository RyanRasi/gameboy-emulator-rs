//! Memory Management Unit (MMU)
//!
//! Implements the full Game Boy DMG memory map:
//!
//! 0x0000–0x3FFF  ROM Bank 0 (or BIOS overlay at 0x0000–0x00FF)
//! 0x4000–0x7FFF  ROM Bank N (switchable, cartridge)
//! 0x8000–0x9FFF  Video RAM (VRAM)
//! 0xA000–0xBFFF  External RAM (cartridge)
//! 0xC000–0xCFFF  Work RAM Bank 0 (WRAM)
//! 0xD000–0xDFFF  Work RAM Bank 1 (WRAM)
//! 0xE000–0xFDFF  Echo RAM (mirrors 0xC000–0xDDFF — reads allowed, writes ignored)
//! 0xFE00–0xFE9F  OAM — Object Attribute Memory (sprite table)
//! 0xFEA0–0xFEFF  Unusable — reads 0xFF, writes ignored
//! 0xFF00–0xFF7F  I/O Registers
//! 0xFF80–0xFFFE  High RAM (HRAM / Zero Page)
//! 0xFFFF         Interrupt Enable Register (IE)

const WRAM_SIZE: usize  = 0x2000; // 8 KiB
const VRAM_SIZE: usize  = 0x2000; // 8 KiB
const ERAM_SIZE: usize  = 0x2000; // 8 KiB (external/cartridge RAM)
const OAM_SIZE:  usize  = 0x00A0; // 160 bytes
const HRAM_SIZE: usize  = 0x007F; // 127 bytes
const IO_SIZE:   usize  = 0x0080; // 128 bytes

pub const BIOS_SIZE: usize = 0x0100; // 256 bytes
pub const ROM_BANK_SIZE: usize = 0x4000; // 16 KiB per bank

/// The MMU owns all addressable memory regions.
/// ROM data is stored separately and provided by the cartridge layer (Phase 6).
/// For now, ROM is a fixed 32 KiB buffer (two banks, no MBC).
pub struct Mmu {
    /// BIOS ROM (256 bytes). Present only when bios_active is true.
    bios: [u8; BIOS_SIZE],

    /// Whether the BIOS overlay is active (true on power-on, false after 0x0100 reached).
    bios_active: bool,

    /// Cartridge ROM — up to 32 KiB for plain ROMs (no MBC yet).
    rom: Vec<u8>,

    /// Video RAM (0x8000–0x9FFF)
    vram: [u8; VRAM_SIZE],

    /// External / cartridge RAM (0xA000–0xBFFF)
    eram: [u8; ERAM_SIZE],

    /// Work RAM (0xC000–0xDFFF)
    wram: [u8; WRAM_SIZE],

    /// OAM — sprite attribute table (0xFE00–0xFE9F)
    oam: [u8; OAM_SIZE],

    /// I/O registers (0xFF00–0xFF7F)
    io: [u8; IO_SIZE],

    /// High RAM / Zero Page (0xFF80–0xFFFE)
    hram: [u8; HRAM_SIZE],

    /// Interrupt Enable register (0xFFFF)
    ie: u8,
}

impl Mmu {
    /// Create a new MMU with all memory zeroed and BIOS overlay inactive.
    /// Load a BIOS or ROM separately via `load_bios` / `load_rom`.
    pub fn new() -> Self {
        Mmu {
            bios: [0u8; BIOS_SIZE],
            bios_active: false,
            rom: vec![0u8; ROM_BANK_SIZE * 2], // default: two blank 16 KiB banks
            vram: [0u8; VRAM_SIZE],
            eram: [0u8; ERAM_SIZE],
            wram: [0u8; WRAM_SIZE],
            oam:  [0u8; OAM_SIZE],
            io:   [0u8; IO_SIZE],
            hram: [0u8; HRAM_SIZE],
            ie:   0,
        }
    }

    /// Load a BIOS image and activate the overlay.
    /// Returns an error string if the slice is the wrong size.
    pub fn load_bios(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() != BIOS_SIZE {
            return Err(format!(
                "BIOS must be exactly {} bytes, got {}",
                BIOS_SIZE,
                data.len()
            ));
        }
        self.bios.copy_from_slice(data);
        self.bios_active = true;
        Ok(())
    }

    /// Load cartridge ROM data. Accepts any size up to 8 MiB (MBC will handle banking later).
    /// Returns an error if data is empty.
    pub fn load_rom(&mut self, data: &[u8]) -> Result<(), String> {
        if data.is_empty() {
            return Err("ROM data must not be empty".into());
        }
        self.rom = data.to_vec();
        Ok(())
    }

    /// Disable the BIOS overlay (called by CPU when PC passes 0x00FF).
    pub fn disable_bios(&mut self) {
        self.bios_active = false;
    }

    /// Returns true if the BIOS overlay is currently active.
    pub fn bios_active(&self) -> bool {
        self.bios_active
    }

    // -------------------------------------------------------------------------
    // Public read / write interface
    // -------------------------------------------------------------------------

    /// Read a single byte from the given address.
    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            // BIOS overlay (only first 256 bytes of address space, only when active)
            0x0000..=0x00FF if self.bios_active => self.bios[addr as usize],

            // ROM Bank 0 (0x0000–0x3FFF)
            0x0000..=0x3FFF => self.rom_read(addr as usize),

            // ROM Bank N (0x4000–0x7FFF) — no MBC yet, reads bank 1 directly
            0x4000..=0x7FFF => self.rom_read(addr as usize),

            // VRAM (0x8000–0x9FFF)
            0x8000..=0x9FFF => self.vram[(addr - 0x8000) as usize],

            // External RAM (0xA000–0xBFFF)
            0xA000..=0xBFFF => self.eram[(addr - 0xA000) as usize],

            // WRAM Bank 0 + 1 (0xC000–0xDFFF)
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],

            // Echo RAM — mirrors 0xC000–0xDDFF
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],

            // OAM (0xFE00–0xFE9F)
            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize],

            // Unusable region (0xFEA0–0xFEFF) — hardware returns 0xFF
            0xFEA0..=0xFEFF => 0xFF,

            // I/O Registers (0xFF00–0xFF7F)
            0xFF00..=0xFF7F => self.io[(addr - 0xFF00) as usize],

            // HRAM (0xFF80–0xFFFE)
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],

            // Interrupt Enable (0xFFFF)
            0xFFFF => self.ie,
        }
    }

    /// Write a single byte to the given address.
    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            // ROM space — writes are ignored for plain ROMs (MBC will intercept later)
            0x0000..=0x7FFF => {
                log::trace!("Ignored write to ROM space: 0x{:04X} = 0x{:02X}", addr, value);
            }

            // VRAM
            0x8000..=0x9FFF => self.vram[(addr - 0x8000) as usize] = value,

            // External RAM
            0xA000..=0xBFFF => self.eram[(addr - 0xA000) as usize] = value,

            // WRAM
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = value,

            // Echo RAM — mirror write to WRAM, guard against overflow
            0xE000..=0xFDFF => {
                let mirror = (addr - 0xE000) as usize;
                if mirror < WRAM_SIZE {
                    self.wram[mirror] = value;
                }
            }

            // OAM
            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize] = value,

            // Unusable — writes silently ignored
            0xFEA0..=0xFEFF => {
                log::trace!("Ignored write to unusable region: 0x{:04X}", addr);
            }

            // I/O Registers
            0xFF00..=0xFF7F => self.io[(addr - 0xFF00) as usize] = value,

            // HRAM
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = value,

            // Interrupt Enable
            0xFFFF => self.ie = value,
        }
    }

    /// Read a 16-bit little-endian word.
    pub fn read_word(&self, addr: u16) -> u16 {
        let lo = self.read_byte(addr) as u16;
        let hi = self.read_byte(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    /// Write a 16-bit little-endian word.
    pub fn write_word(&mut self, addr: u16, value: u16) {
        self.write_byte(addr, (value & 0xFF) as u8);
        self.write_byte(addr.wrapping_add(1), (value >> 8) as u8);
    }

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    /// Safe ROM read — returns 0xFF if address is out of range (open bus).
    fn rom_read(&self, addr: usize) -> u8 {
        self.rom.get(addr).copied().unwrap_or(0xFF)
    }
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Basic read / write
    // -------------------------------------------------------------------------

    #[test]
    fn test_wram_write_and_read_back() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0xC000, 0xAB);
        assert_eq!(mmu.read_byte(0xC000), 0xAB);
    }

    #[test]
    fn test_wram_write_top_of_range() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0xDFFF, 0x55);
        assert_eq!(mmu.read_byte(0xDFFF), 0x55);
    }

    #[test]
    fn test_vram_write_and_read_back() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0x8000, 0x42);
        assert_eq!(mmu.read_byte(0x8000), 0x42);
    }

    #[test]
    fn test_hram_write_and_read_back() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0xFF80, 0x77);
        assert_eq!(mmu.read_byte(0xFF80), 0x77);
    }

    #[test]
    fn test_ie_register_write_and_read() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0xFFFF, 0x1F);
        assert_eq!(mmu.read_byte(0xFFFF), 0x1F);
    }

    #[test]
    fn test_io_register_write_and_read() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0xFF40, 0x91); // LCDC register
        assert_eq!(mmu.read_byte(0xFF40), 0x91);
    }

    #[test]
    fn test_oam_write_and_read_back() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0xFE00, 0x10);
        assert_eq!(mmu.read_byte(0xFE00), 0x10);
    }

    #[test]
    fn test_word_read_write_little_endian() {
        let mut mmu = Mmu::new();
        mmu.write_word(0xC100, 0xBEEF);
        assert_eq!(mmu.read_byte(0xC100), 0xEF); // low byte first
        assert_eq!(mmu.read_byte(0xC101), 0xBE); // high byte second
        assert_eq!(mmu.read_word(0xC100), 0xBEEF);
    }

    // -------------------------------------------------------------------------
    // Memory region routing
    // -------------------------------------------------------------------------

    #[test]
    fn test_echo_ram_mirrors_wram_on_read() {
        let mut mmu = Mmu::new();
        // Write to WRAM — should be readable via echo RAM
        mmu.write_byte(0xC123, 0x99);
        assert_eq!(mmu.read_byte(0xE123), 0x99);
    }

    #[test]
    fn test_echo_ram_write_mirrors_to_wram() {
        let mut mmu = Mmu::new();
        // Write through echo RAM — should appear in WRAM
        mmu.write_byte(0xE200, 0x33);
        assert_eq!(mmu.read_byte(0xC200), 0x33);
    }

    #[test]
    fn test_rom_read_before_load_returns_0xff() {
        let mmu = Mmu::new();
        // Default ROM is all zeros, but out-of-range ROM reads return 0xFF
        // With default two-bank ROM (zeroed), reads within range return 0x00
        assert_eq!(mmu.read_byte(0x0100), 0x00);
    }

    #[test]
    fn test_rom_load_and_read() {
        let mut mmu = Mmu::new();
        let mut rom = vec![0u8; 0x8000]; // 32 KiB
        rom[0x0150] = 0xDE;
        rom[0x4000] = 0xAD;
        mmu.load_rom(&rom).unwrap();
        assert_eq!(mmu.read_byte(0x0150), 0xDE);
        assert_eq!(mmu.read_byte(0x4000), 0xAD);
    }

    #[test]
    fn test_rom_write_is_ignored() {
        let mut mmu = Mmu::new();
        let rom = vec![0xFFu8; 0x8000];
        mmu.load_rom(&rom).unwrap();
        mmu.write_byte(0x0000, 0x00); // should be silently ignored
        assert_eq!(mmu.read_byte(0x0000), 0xFF); // ROM still reads original value
    }

    // -------------------------------------------------------------------------
    // BIOS overlay
    // -------------------------------------------------------------------------

    #[test]
    fn test_bios_overlay_active_on_load() {
        let mut mmu = Mmu::new();
        let bios = vec![0xAA; BIOS_SIZE];
        mmu.load_bios(&bios).unwrap();
        assert!(mmu.bios_active());
        assert_eq!(mmu.read_byte(0x0000), 0xAA);
    }

    #[test]
    fn test_bios_overlay_disabled_exposes_rom() {
        let mut mmu = Mmu::new();
        let mut bios = vec![0xAA; BIOS_SIZE];
        bios[0x00] = 0xBB;
        mmu.load_bios(&bios).unwrap();

        let mut rom = vec![0u8; 0x8000];
        rom[0x0000] = 0xCC;
        mmu.load_rom(&rom).unwrap();

        // BIOS active → reads BIOS
        assert_eq!(mmu.read_byte(0x0000), 0xBB);

        // Disable BIOS → reads ROM
        mmu.disable_bios();
        assert_eq!(mmu.read_byte(0x0000), 0xCC);
    }

    #[test]
    fn test_bios_wrong_size_returns_error() {
        let mut mmu = Mmu::new();
        let bad_bios = vec![0u8; 512]; // wrong size
        assert!(mmu.load_bios(&bad_bios).is_err());
    }

    #[test]
    fn test_rom_empty_returns_error() {
        let mut mmu = Mmu::new();
        assert!(mmu.load_rom(&[]).is_err());
    }

    // -------------------------------------------------------------------------
    // Edge cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_unusable_region_reads_0xff() {
        let mmu = Mmu::new();
        assert_eq!(mmu.read_byte(0xFEA0), 0xFF);
        assert_eq!(mmu.read_byte(0xFEFF), 0xFF);
    }

    #[test]
    fn test_unusable_region_write_is_ignored() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0xFEA0, 0x42); // must not panic
        assert_eq!(mmu.read_byte(0xFEA0), 0xFF); // still 0xFF
    }

    #[test]
    fn test_all_wram_bytes_independently_writable() {
        let mut mmu = Mmu::new();
        for i in 0u16..WRAM_SIZE as u16 {
            let addr = 0xC000 + i;
            let val = (i & 0xFF) as u8;
            mmu.write_byte(addr, val);
        }
        for i in 0u16..WRAM_SIZE as u16 {
            let addr = 0xC000 + i;
            let expected = (i & 0xFF) as u8;
            assert_eq!(
                mmu.read_byte(addr),
                expected,
                "WRAM mismatch at 0x{:04X}",
                addr
            );
        }
    }
}