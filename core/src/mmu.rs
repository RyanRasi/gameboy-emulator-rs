//! Memory Management Unit (MMU)
//!
//! Memory map:
//!   0x0000–0x7FFF  ROM (via Cartridge)
//!   0x8000–0x9FFF  VRAM
//!   0xA000–0xBFFF  External RAM (via Cartridge)
//!   0xC000–0xDFFF  WRAM
//!   0xE000–0xFDFF  Echo RAM (mirrors WRAM)
//!   0xFE00–0xFE9F  OAM
//!   0xFEA0–0xFEFF  Unusable
//!   0xFF00–0xFF7F  I/O Registers
//!   0xFF80–0xFFFE  HRAM
//!   0xFFFF         IE register

use crate::cartridge::Cartridge;

const WRAM_SIZE: usize = 0x2000;
const VRAM_SIZE: usize = 0x2000;
const OAM_SIZE:  usize = 0x00A0;
const HRAM_SIZE: usize = 0x007F;
const IO_SIZE:   usize = 0x0080;

pub const BIOS_SIZE: usize     = 0x0100;
pub const ROM_BANK_SIZE: usize = 0x4000;

pub struct Mmu {
    bios:         [u8; BIOS_SIZE],
    bios_active:  bool,

    /// Loaded cartridge. None until `load_cartridge` is called.
    cartridge:    Option<Cartridge>,

    /// Fallback bare ROM buffer (used by tests that call load_rom directly).
    bare_rom:     Vec<u8>,

    vram: [u8; VRAM_SIZE],
    wram: [u8; WRAM_SIZE],
    oam:  [u8; OAM_SIZE],
    io:   [u8; IO_SIZE],
    hram: [u8; HRAM_SIZE],
    ie:   u8,
}

impl Mmu {
    pub fn new() -> Self {
        Mmu {
            bios:        [0u8; BIOS_SIZE],
            bios_active: false,
            cartridge:   None,
            bare_rom:    vec![0u8; ROM_BANK_SIZE * 2],
            vram: [0u8; VRAM_SIZE],
            wram: [0u8; WRAM_SIZE],
            oam:  [0u8; OAM_SIZE],
            io:   [0u8; IO_SIZE],
            hram: [0u8; HRAM_SIZE],
            ie:   0,
        }
    }

    // ── Cartridge / ROM loading ───────────────────────────────────────────────

    /// Load a fully parsed Cartridge (preferred path).
    pub fn load_cartridge(&mut self, cart: Cartridge) {
        self.cartridge = Some(cart);
    }

    /// Load raw ROM bytes directly (used by unit tests and legacy code).
    pub fn load_rom(&mut self, data: &[u8]) -> Result<(), String> {
        if data.is_empty() {
            return Err("ROM data must not be empty".into());
        }
        self.bare_rom = data.to_vec();
        Ok(())
    }

    /// Load a BIOS image and activate the overlay.
    pub fn load_bios(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() != BIOS_SIZE {
            return Err(format!(
                "BIOS must be exactly {} bytes, got {}", BIOS_SIZE, data.len()
            ));
        }
        self.bios.copy_from_slice(data);
        self.bios_active = true;
        Ok(())
    }

    pub fn disable_bios(&mut self)  { self.bios_active = false; }
    pub fn bios_active(&self) -> bool { self.bios_active }

    // ── ROM read routing ─────────────────────────────────────────────────────

    fn rom_read(&self, addr: u16) -> u8 {
        if let Some(cart) = &self.cartridge {
            cart.read_rom(addr)
        } else {
            self.bare_rom.get(addr as usize).copied().unwrap_or(0xFF)
        }
    }

    fn rom_write(&mut self, addr: u16, value: u8) {
        if let Some(cart) = &mut self.cartridge {
            cart.write_rom(addr, value);
        }
        // bare_rom writes are silently ignored (read-only)
    }

    fn eram_read(&self, addr: u16) -> u8 {
        if let Some(cart) = &self.cartridge {
            cart.read_ram(addr)
        } else {
            0xFF
        }
    }

    fn eram_write(&mut self, addr: u16, value: u8) {
        if let Some(cart) = &mut self.cartridge {
            cart.write_ram(addr, value);
        }
    }

    // ── Public read / write interface ─────────────────────────────────────────

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x00FF if self.bios_active => self.bios[addr as usize],
            0x0000..=0x7FFF => self.rom_read(addr),
            0x8000..=0x9FFF => self.vram[(addr - 0x8000) as usize],
            0xA000..=0xBFFF => self.eram_read(addr - 0xA000),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],
            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize],
            0xFEA0..=0xFEFF => 0xFF,
            0xFF00..=0xFF7F => self.io[(addr - 0xFF00) as usize],
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF          => self.ie,
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.rom_write(addr, value),
            0x8000..=0x9FFF => self.vram[(addr - 0x8000) as usize] = value,
            0xA000..=0xBFFF => self.eram_write(addr - 0xA000, value),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = value,
            0xE000..=0xFDFF => {
                let m = (addr - 0xE000) as usize;
                if m < WRAM_SIZE { self.wram[m] = value; }
            }
            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize] = value,
            0xFEA0..=0xFEFF => {}
            0xFF00..=0xFF7F => self.io[(addr - 0xFF00) as usize] = value,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = value,
            0xFFFF          => self.ie = value,
        }
    }

    pub fn read_word(&self, addr: u16) -> u16 {
        let lo = self.read_byte(addr) as u16;
        let hi = self.read_byte(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    pub fn write_word(&mut self, addr: u16, value: u16) {
        self.write_byte(addr, (value & 0xFF) as u8);
        self.write_byte(addr.wrapping_add(1), (value >> 8) as u8);
    }
}

impl Default for Mmu {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::Cartridge;

    fn make_cart_rom(size: usize, cart_type: u8, rom_code: u8, ram_code: u8) -> Vec<u8> {
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

    // ── Existing MMU tests (unchanged behaviour) ──────────────────────────────

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
        mmu.write_byte(0xFF40, 0x91);
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
        assert_eq!(mmu.read_byte(0xC100), 0xEF);
        assert_eq!(mmu.read_byte(0xC101), 0xBE);
        assert_eq!(mmu.read_word(0xC100), 0xBEEF);
    }

    #[test]
    fn test_echo_ram_mirrors_wram_on_read() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0xC123, 0x99);
        assert_eq!(mmu.read_byte(0xE123), 0x99);
    }

    #[test]
    fn test_echo_ram_write_mirrors_to_wram() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0xE200, 0x33);
        assert_eq!(mmu.read_byte(0xC200), 0x33);
    }

    #[test]
    fn test_unusable_region_reads_0xff() {
        let mmu = Mmu::new();
        assert_eq!(mmu.read_byte(0xFEA0), 0xFF);
        assert_eq!(mmu.read_byte(0xFEFF), 0xFF);
    }

    #[test]
    fn test_unusable_region_write_is_ignored() {
        let mut mmu = Mmu::new();
        mmu.write_byte(0xFEA0, 0x42);
        assert_eq!(mmu.read_byte(0xFEA0), 0xFF);
    }

    #[test]
    fn test_bare_rom_load_and_read() {
        let mut mmu = Mmu::new();
        let mut rom = vec![0u8; 0x8000];
        rom[0x0150] = 0xDE;
        rom[0x4000] = 0xAD;
        mmu.load_rom(&rom).unwrap();
        assert_eq!(mmu.read_byte(0x0150), 0xDE);
        assert_eq!(mmu.read_byte(0x4000), 0xAD);
    }

    #[test]
    fn test_bare_rom_write_is_ignored() {
        let mut mmu = Mmu::new();
        let rom = vec![0xFFu8; 0x8000];
        mmu.load_rom(&rom).unwrap();
        mmu.write_byte(0x0000, 0x00);
        assert_eq!(mmu.read_byte(0x0000), 0xFF);
    }

    #[test]
    fn test_bios_overlay_active_on_load() {
        let mut mmu = Mmu::new();
        let bios = vec![0xAAu8; BIOS_SIZE];
        mmu.load_bios(&bios).unwrap();
        assert!(mmu.bios_active());
        assert_eq!(mmu.read_byte(0x0000), 0xAA);
    }

    #[test]
    fn test_bios_overlay_disabled_exposes_rom() {
        let mut mmu = Mmu::new();
        let mut bios = vec![0xBBu8; BIOS_SIZE];
        mmu.load_bios(&bios).unwrap();
        let mut rom = vec![0u8; 0x8000];
        rom[0x0000] = 0xCC;
        mmu.load_rom(&rom).unwrap();
        assert_eq!(mmu.read_byte(0x0000), 0xBB);
        mmu.disable_bios();
        assert_eq!(mmu.read_byte(0x0000), 0xCC);
    }

    #[test]
    fn test_bios_wrong_size_returns_error() {
        let mut mmu = Mmu::new();
        assert!(mmu.load_bios(&vec![0u8; 512]).is_err());
    }

    #[test]
    fn test_rom_empty_returns_error() {
        let mut mmu = Mmu::new();
        assert!(mmu.load_rom(&[]).is_err());
    }

    #[test]
    fn test_all_wram_bytes_independently_writable() {
        let mut mmu = Mmu::new();
        for i in 0u16..0x2000 {
            mmu.write_byte(0xC000 + i, (i & 0xFF) as u8);
        }
        for i in 0u16..0x2000 {
            assert_eq!(mmu.read_byte(0xC000 + i), (i & 0xFF) as u8);
        }
    }

    // ── Cartridge integration ─────────────────────────────────────────────────

    #[test]
    fn test_cartridge_rom_read_via_mmu() {
        let mut rom = make_cart_rom(0x8000, 0x00, 0x00, 0x00);
        rom[0x0150] = 0xAB;
        let cs = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = cs;
        let cart = Cartridge::load(rom).unwrap();
        let mut mmu = Mmu::new();
        mmu.load_cartridge(cart);
        assert_eq!(mmu.read_byte(0x0150), 0xAB);
    }

    #[test]
    fn test_cartridge_takes_priority_over_bare_rom() {
        let mut rom = make_cart_rom(0x8000, 0x00, 0x00, 0x00);
        rom[0x0200] = 0xCC;
        let cs = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = cs;
        let cart = Cartridge::load(rom).unwrap();
        let mut mmu = Mmu::new();
        let mut bare = vec![0xAAu8; 0x8000];
        mmu.load_rom(&bare).unwrap();
        mmu.load_cartridge(cart);
        assert_eq!(mmu.read_byte(0x0200), 0xCC, "Cartridge must take priority");
    }

    #[test]
    fn test_mbc1_bank_switch_via_mmu() {
        let mut big_rom = vec![0u8; 0x10000]; // 64 KiB = 4 banks
        big_rom[0x0147] = 0x01; // MBC1
        big_rom[0x0148] = 0x01; // 4 banks
        big_rom[0x0149] = 0x00;
        for b in &mut big_rom[0x8000..0xC000] { *b = 0x22; }
        let cs = big_rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        big_rom[0x014D] = cs;
        let cart = Cartridge::load(big_rom).unwrap();
        let mut mmu = Mmu::new();
        mmu.load_cartridge(cart);
        mmu.write_byte(0x2000, 0x02); // select bank 2
        assert_eq!(mmu.read_byte(0x4000), 0x22);
    }

    #[test]
    fn test_cartridge_ram_read_write_via_mmu() {
        let rom = make_cart_rom(0x8000, 0x02, 0x01, 0x02); // MBC1+RAM
        let cart = Cartridge::load(rom).unwrap();
        let mut mmu = Mmu::new();
        mmu.load_cartridge(cart);
        mmu.write_byte(0x0000, 0x0A); // enable RAM
        mmu.write_byte(0xA000, 0x55);
        assert_eq!(mmu.read_byte(0xA000), 0x55);
    }
}