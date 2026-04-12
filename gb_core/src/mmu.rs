//! Memory Management Unit — DMG + CGB banking, HDMA, and register routing.

use crate::cartridge::Cartridge;
use crate::ppu::cgb_palette::CgbPalette;

const BIOS_SIZE: usize = 0x100;
const OAM_SIZE: usize = 0x00A0;
const IO_SIZE: usize = 0x0080;
const HRAM_SIZE: usize = 0x007F;

pub struct Mmu {
    pub(crate) bios: [u8; BIOS_SIZE],
    pub bios_active: bool,

    pub cartridge: Option<Cartridge>,
    bare_rom: Vec<u8>,

    // VRAM: 2 banks × 8 KiB = 16 KiB flat (bank N at offset N*0x2000)
    pub(crate) vram: Vec<u8>,
    pub vram_bank: u8, // 0 or 1

    // WRAM: 8 banks × 4 KiB = 32 KiB flat
    // Bank 0 always at 0xC000, selected bank (1-7) at 0xD000
    pub(crate) wram: Vec<u8>,
    pub wram_bank: u8, // 1-7 (default 1)

    pub(crate) oam: [u8; OAM_SIZE],
    pub(crate) io: [u8; IO_SIZE],
    pub(crate) hram: [u8; HRAM_SIZE],
    pub(crate) ie: u8,

    pub cgb_mode: bool,
    pub double_speed: bool,
    pub prepare_speed_switch: bool,

    // CGB color palettes (accessed via 0xFF68-0xFF6B)
    pub bg_palette: CgbPalette,
    pub obj_palette: CgbPalette,

    // HDMA
    hdma_src: u16,
    hdma_dst: u16,
    hdma_remaining: u8, // remaining 16-byte blocks minus 1
    hdma_hblank: bool,  // true = H-Blank DMA mode
    pub hdma_active: bool,

    // OAM DMA
    dma_active: bool,
    dma_source: u16,
    dma_offset: u8,
}

impl Mmu {
    pub fn new() -> Self {
        Mmu {
            bios: [0u8; BIOS_SIZE],
            bios_active: false,
            cartridge: None,
            bare_rom: Vec::new(),
            vram: vec![0u8; 0x4000],
            vram_bank: 0,
            wram: vec![0u8; 0x8000],
            wram_bank: 1,
            oam: [0u8; OAM_SIZE],
            io: [0u8; IO_SIZE],
            hram: [0u8; HRAM_SIZE],
            ie: 0,
            cgb_mode: false,
            double_speed: false,
            prepare_speed_switch: false,
            bg_palette: CgbPalette::new(),
            obj_palette: CgbPalette::new(),
            hdma_src: 0,
            hdma_dst: 0,
            hdma_remaining: 0,
            hdma_hblank: false,
            hdma_active: false,
            dma_active: false,
            dma_source: 0,
            dma_offset: 0,
        }
    }

    // ── Cartridge loading ────────────────────────────────────────────────────

    pub fn load_cartridge(&mut self, cart: Cartridge) {
        // Detect CGB mode from header
        let cgb_flag = cart.header.cgb_flag;
        self.cgb_mode = cgb_flag == 0x80 || cgb_flag == 0xC0;
        self.cartridge = Some(cart);
    }

    pub fn load_rom(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() < 0x8000 {
            return Err("ROM too small".into());
        }
        self.bare_rom = data.to_vec();
        Ok(())
    }

    // ── VRAM bank helper (for PPU to read specific bank) ────────────────────

    /// Read a byte from VRAM at `addr` (0x8000-0x9FFF) from a specific bank.
    pub fn read_vram_bank(&self, addr: u16, bank: u8) -> u8 {
        let offset = (addr as usize).wrapping_sub(0x8000) & 0x1FFF;
        let idx = bank as usize * 0x2000 + offset;
        if idx < self.vram.len() {
            self.vram[idx]
        } else {
            0xFF
        }
    }

    /// Read an OAM byte by offset (0-159).
    pub fn read_oam(&self, offset: usize) -> u8 {
        if offset < OAM_SIZE {
            self.oam[offset]
        } else {
            0xFF
        }
    }

    // ── RTC tick ─────────────────────────────────────────────────────────────

    pub fn tick_cartridge_rtc(&mut self, cycles: u64) {
        if let Some(ref mut cart) = self.cartridge {
            cart.tick_rtc(cycles);
        }
    }

    // ── HDMA step (called by CPU at H-Blank) ─────────────────────────────────

    /// Execute one 16-byte H-Blank DMA transfer. Returns true if more remain.
    pub fn hdma_hblank_step(&mut self) -> bool {
        if !self.hdma_active || !self.hdma_hblank {
            return false;
        }
        self.hdma_transfer_block();
        if self.hdma_remaining == 0xFF {
            self.hdma_active = false;
            false
        } else {
            true
        }
    }

    fn hdma_transfer_block(&mut self) {
        for i in 0..16u16 {
            let src = self.hdma_src.wrapping_add(i);
            let dst = (self.hdma_dst.wrapping_add(i) & 0x1FFF) as usize;
            let val = self.read_byte(src);
            let bank = self.vram_bank as usize;
            if bank * 0x2000 + dst < self.vram.len() {
                self.vram[bank * 0x2000 + dst] = val;
            }
        }
        self.hdma_src = self.hdma_src.wrapping_add(16);
        self.hdma_dst = self.hdma_dst.wrapping_add(16);
        if self.hdma_remaining == 0 {
            self.hdma_remaining = 0xFF; // done sentinel
        } else {
            self.hdma_remaining -= 1;
        }
    }

    // ── Speed switch ─────────────────────────────────────────────────────────

    pub fn execute_speed_switch(&mut self) {
        if self.prepare_speed_switch {
            self.double_speed = !self.double_speed;
            self.prepare_speed_switch = false;
            // Update KEY1 register
            let key1 = if self.double_speed { 0x80 } else { 0x00 };
            self.io[0x4D] = key1;
        }
    }

    // ── Memory map ───────────────────────────────────────────────────────────

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x00FF if self.bios_active => self.bios[addr as usize],

            0x0000..=0x7FFF => {
                if let Some(ref cart) = self.cartridge {
                    cart.read_rom(addr)
                } else {
                    self.bare_rom.get(addr as usize).copied().unwrap_or(0xFF)
                }
            }

            0x8000..=0x9FFF => {
                let offset = (addr as usize - 0x8000) + self.vram_bank as usize * 0x2000;
                if offset < self.vram.len() {
                    self.vram[offset]
                } else {
                    0xFF
                }
            }

            0xA000..=0xBFFF => {
                if let Some(ref cart) = self.cartridge {
                    cart.read_ram(addr - 0xA000)
                } else {
                    0xFF
                }
            }

            0xC000..=0xCFFF => {
                let off = (addr - 0xC000) as usize;
                if off < self.wram.len() {
                    self.wram[off]
                } else {
                    0xFF
                }
            }

            0xD000..=0xDFFF => {
                let off = self.wram_bank as usize * 0x1000 + (addr - 0xD000) as usize;
                if off < self.wram.len() {
                    self.wram[off]
                } else {
                    0xFF
                }
            }

            0xE000..=0xFDFF => self.read_byte(addr - 0x2000), // echo

            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize],
            0xFEA0..=0xFEFF => 0xFF, // unusable

            0xFF00..=0xFF7F => self.read_io(addr),

            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie,

            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => {
                if let Some(ref mut cart) = self.cartridge {
                    cart.write_rom(addr, value);
                }
            }

            0x8000..=0x9FFF => {
                let offset = (addr as usize - 0x8000) + self.vram_bank as usize * 0x2000;
                if offset < self.vram.len() {
                    self.vram[offset] = value;
                }
            }

            0xA000..=0xBFFF => {
                if let Some(ref mut cart) = self.cartridge {
                    cart.write_ram(addr - 0xA000, value);
                }
            }

            0xC000..=0xCFFF => {
                let off = (addr - 0xC000) as usize;
                if off < self.wram.len() {
                    self.wram[off] = value;
                }
            }

            0xD000..=0xDFFF => {
                let off = self.wram_bank as usize * 0x1000 + (addr - 0xD000) as usize;
                if off < self.wram.len() {
                    self.wram[off] = value;
                }
            }

            0xE000..=0xFDFF => self.write_byte(addr - 0x2000, value),

            0xFE00..=0xFE9F => {
                if !self.dma_active {
                    self.oam[(addr - 0xFE00) as usize] = value;
                }
            }
            0xFEA0..=0xFEFF => {}

            0xFF00..=0xFF7F => self.write_io(addr, value),

            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = value,
            0xFFFF => self.ie = value,
            _ => {}
        }
    }

    // ── IO register read ─────────────────────────────────────────────────────

    fn read_io(&self, addr: u16) -> u8 {
        let off = (addr - 0xFF00) as usize;
        match addr {
            // CGB palette data
            0xFF69 => self.bg_palette.read_data(),
            0xFF6B => self.obj_palette.read_data(),
            // CGB palette spec (auto-inc flag + index)
            0xFF68 => self.bg_palette.spec,
            0xFF6A => self.obj_palette.spec,
            // VBK — VRAM bank, bits 1-7 always 1 in readback
            0xFF4F => 0xFE | self.vram_bank,
            // SVBK — WRAM bank
            0xFF70 => self.wram_bank,
            // KEY1 — double speed
            0xFF4D => {
                let mut v = if self.double_speed { 0x80 } else { 0x00 };
                if self.prepare_speed_switch {
                    v |= 0x01;
                }
                v
            }
            // HDMA5 status
            0xFF55 => {
                if self.hdma_active {
                    self.hdma_remaining & 0x7F
                } else {
                    0xFF
                }
            }
            _ => {
                if off < IO_SIZE {
                    self.io[off]
                } else {
                    0xFF
                }
            }
        }
    }

    // ── IO register write ────────────────────────────────────────────────────

    fn write_io(&mut self, addr: u16, value: u8) {
        let off = (addr - 0xFF00) as usize;
        match addr {
            // Joypad — handled externally
            0xFF00 => self.io[0x00] = (self.io[0x00] & 0x0F) | (value & 0x30),

            // Timer
            0xFF03 => {}                 // unused
            0xFF04 => self.io[0x04] = 0, // writing any value resets DIV

            // OAM DMA
            0xFF46 => {
                self.io[0x46] = value;
                let src = (value as u16) << 8;
                for i in 0..0xA0u16 {
                    self.oam[i as usize] = self.read_byte(src + i);
                }
            }

            // CGB: VRAM bank
            0xFF4F if self.cgb_mode => {
                self.vram_bank = value & 0x01;
                self.io[0x4F] = value;
            }

            // CGB: Speed switch
            0xFF4D if self.cgb_mode => {
                if value & 0x01 != 0 {
                    self.prepare_speed_switch = true;
                }
                self.io[0x4D] = value;
            }

            // CGB: HDMA source/dest
            0xFF51 => self.hdma_src = (self.hdma_src & 0x00FF) | ((value as u16) << 8),
            0xFF52 => self.hdma_src = (self.hdma_src & 0xFF00) | ((value & 0xF0) as u16),
            0xFF53 => {
                self.hdma_dst = (self.hdma_dst & 0x00FF) | (((value & 0x1F) as u16) << 8) | 0x8000
            }
            0xFF54 => self.hdma_dst = (self.hdma_dst & 0xFF00) | ((value & 0xF0) as u16),
            0xFF55 if self.cgb_mode => {
                let len = (value & 0x7F) as u8;
                if value & 0x80 == 0 {
                    // General Purpose DMA — transfer immediately
                    self.hdma_remaining = len;
                    self.hdma_hblank = false;
                    self.hdma_active = true;
                    while self.hdma_active {
                        self.hdma_transfer_block();
                        if self.hdma_remaining == 0xFF {
                            self.hdma_active = false;
                        }
                    }
                    self.io[0x55] = 0xFF;
                } else {
                    // H-Blank DMA
                    if self.hdma_active && self.hdma_hblank {
                        // Cancel active HDMA
                        self.hdma_active = false;
                        self.io[0x55] = 0xFF;
                    } else {
                        self.hdma_remaining = len;
                        self.hdma_hblank = true;
                        self.hdma_active = true;
                    }
                }
            }

            // CGB: WRAM bank
            0xFF70 if self.cgb_mode => {
                self.wram_bank = if value & 0x07 == 0 { 1 } else { value & 0x07 };
                self.io[0x70] = self.wram_bank;
            }

            // CGB: BG palette spec
            0xFF68 if self.cgb_mode => self.bg_palette.spec = value,
            // CGB: BG palette data
            0xFF69 if self.cgb_mode => self.bg_palette.write_data(value),
            // CGB: OBJ palette spec
            0xFF6A if self.cgb_mode => self.obj_palette.spec = value,
            // CGB: OBJ palette data
            0xFF6B if self.cgb_mode => self.obj_palette.write_data(value),

            _ => {
                if off < IO_SIZE {
                    self.io[off] = value;
                }
            }
        }
    }
    pub fn read_word(&self, addr: u16) -> u16 {
        let lo = self.read_byte(addr) as u16;
        let hi = self.read_byte(addr.wrapping_add(1)) as u16;
        lo | (hi << 8)
    }

    pub fn write_word(&mut self, addr: u16, value: u16) {
        self.write_byte(addr, (value & 0xFF) as u8);
        self.write_byte(addr.wrapping_add(1), (value >> 8) as u8);
    }
    /// Write DIV directly (used by Timer — bypasses the reset-on-write hardware behaviour).
    pub fn set_div(&mut self, val: u8) {
        self.io[0x04] = val;
    }

    /// Write the full JOYP register directly (used by Joypad — includes lower nibble).
    pub fn set_joyp(&mut self, val: u8) {
        self.io[0x00] = val;
    }

    /// Load a BIOS image (must be exactly 256 bytes).
    pub fn load_bios(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() != self.bios.len() {
            return Err(format!(
                "BIOS must be exactly {} bytes, got {}",
                self.bios.len(),
                data.len()
            ));
        }
        self.bios.copy_from_slice(data);
        self.bios_active = true;
        Ok(())
    }
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ppu::{BGP_ADDR, LCDC_ADDR};

    fn mmu() -> Mmu {
        Mmu::new()
    }

    // ── Basic read/write ──────────────────────────────────────────────────────

    #[test]
    fn test_wram_write_read() {
        let mut m = mmu();
        m.write_byte(0xC100, 0xAB);
        assert_eq!(m.read_byte(0xC100), 0xAB);
    }

    #[test]
    fn test_hram_write_read() {
        let mut m = mmu();
        m.write_byte(0xFF81, 0x55);
        assert_eq!(m.read_byte(0xFF81), 0x55);
    }

    #[test]
    fn test_ie_write_read() {
        let mut m = mmu();
        m.write_byte(0xFFFF, 0x1F);
        assert_eq!(m.read_byte(0xFFFF), 0x1F);
    }

    #[test]
    fn test_vram_write_read() {
        let mut m = mmu();
        m.write_byte(0x8100, 0xCD);
        assert_eq!(m.read_byte(0x8100), 0xCD);
    }

    // ── VRAM banking ──────────────────────────────────────────────────────────

    #[test]
    fn test_vram_bank_switch() {
        let mut m = mmu();
        m.cgb_mode = true;
        // Write to bank 0
        m.vram_bank = 0;
        m.write_byte(0x8000, 0x11);
        // Switch to bank 1
        m.write_byte(0xFF4F, 0x01);
        assert_eq!(m.vram_bank, 1);
        m.write_byte(0x8000, 0x22);
        // Back to bank 0
        m.write_byte(0xFF4F, 0x00);
        assert_eq!(m.read_byte(0x8000), 0x11);
        m.write_byte(0xFF4F, 0x01);
        assert_eq!(m.read_byte(0x8000), 0x22);
    }

    #[test]
    fn test_read_vram_bank_explicit() {
        let mut m = mmu();
        m.vram[0x0000] = 0xAA; // bank 0 offset 0
        m.vram[0x2000] = 0xBB; // bank 1 offset 0
        assert_eq!(m.read_vram_bank(0x8000, 0), 0xAA);
        assert_eq!(m.read_vram_bank(0x8000, 1), 0xBB);
    }

    // ── WRAM banking ──────────────────────────────────────────────────────────

    #[test]
    fn test_wram_bank_switch() {
        let mut m = mmu();
        m.cgb_mode = true;
        // Write to D000 in bank 1 (default)
        m.write_byte(0xD000, 0x11);
        // Switch to bank 2
        m.write_byte(0xFF70, 0x02);
        assert_eq!(m.wram_bank, 2);
        m.write_byte(0xD000, 0x22);
        // Back to bank 1
        m.write_byte(0xFF70, 0x01);
        assert_eq!(m.read_byte(0xD000), 0x11);
        m.write_byte(0xFF70, 0x02);
        assert_eq!(m.read_byte(0xD000), 0x22);
    }

    #[test]
    fn test_wram_bank_0_always_at_c000() {
        let mut m = mmu();
        m.cgb_mode = true;
        m.wram_bank = 3;
        m.write_byte(0xC100, 0x42);
        // bank 0, offset 0x100
        assert_eq!(m.wram[0x100], 0x42);
    }

    // ── CGB palettes ──────────────────────────────────────────────────────────

    #[test]
    fn test_cgb_bg_palette_write_read() {
        let mut m = mmu();
        m.cgb_mode = true;
        m.write_byte(0xFF68, 0x80); // auto-increment, index 0
        m.write_byte(0xFF69, 0xFF);
        m.write_byte(0xFF69, 0x7F);
        // palette 0, color 0 should be white (0x7FFF)
        assert_eq!(m.bg_palette.get_color(0, 0), 0x00FFFFFF);
    }

    #[test]
    fn test_cgb_obj_palette_write_read() {
        let mut m = mmu();
        m.cgb_mode = true;
        m.write_byte(0xFF6A, 0x80); // auto-increment, index 0
        m.write_byte(0xFF6B, 0x00);
        m.write_byte(0xFF6B, 0x00);
        assert_eq!(m.obj_palette.get_color(0, 0), 0x00000000);
    }

    // ── OAM DMA ───────────────────────────────────────────────────────────────

    #[test]
    fn test_oam_dma_copies_from_wram() {
        let mut m = mmu();
        m.write_byte(0xC000, 0xAB);
        m.write_byte(0xFF46, 0xC0); // DMA from 0xC000
        assert_eq!(m.oam[0], 0xAB);
    }

    // ── Speed switch ──────────────────────────────────────────────────────────

    #[test]
    fn test_speed_switch_toggles_double_speed() {
        let mut m = mmu();
        m.cgb_mode = true;
        m.write_byte(0xFF4D, 0x01); // prepare
        assert!(m.prepare_speed_switch);
        m.execute_speed_switch();
        assert!(m.double_speed);
        assert!(!m.prepare_speed_switch);
    }

    // ── Tick RTC ──────────────────────────────────────────────────────────────

    #[test]
    fn test_tick_cartridge_rtc_no_cart() {
        let mut m = mmu();
        m.tick_cartridge_rtc(4_194_304); // should not panic
    }

    // ── load_rom ──────────────────────────────────────────────────────────────

    #[test]
    fn test_load_rom_too_small_returns_err() {
        let mut m = mmu();
        assert!(m.load_rom(&[0u8; 100]).is_err());
    }

    #[test]
    fn test_load_rom_reads_back() {
        let mut m = mmu();
        let mut rom = vec![0u8; 0x8000];
        rom[0x0100] = 0x42;
        m.load_rom(&rom).unwrap();
        assert_eq!(m.read_byte(0x0100), 0x42);
    }
}
