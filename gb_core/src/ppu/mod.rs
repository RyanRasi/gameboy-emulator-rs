//! Game Boy PPU — supports both DMG and CGB rendering.

pub mod cgb_palette;
pub mod palettes;

use cgb_palette::CgbPalette;
use palettes::{DmgColorPalettes, DEFAULT_PALETTE};
use crate::mmu::Mmu;
use serde::{Serialize, Deserialize};

// ── Public addresses ─────────────────────────────────────────────────────────
pub const LCDC_ADDR: u16 = 0xFF40;
pub const STAT_ADDR: u16 = 0xFF41;
pub const SCY_ADDR:  u16 = 0xFF42;
pub const SCX_ADDR:  u16 = 0xFF43;
pub const LY_ADDR:   u16 = 0xFF44;
pub const LYC_ADDR:  u16 = 0xFF45;
pub const DMA_ADDR:  u16 = 0xFF46;
pub const BGP_ADDR:  u16 = 0xFF47;
pub const OBP0_ADDR: u16 = 0xFF48;
pub const OBP1_ADDR: u16 = 0xFF49;
pub const WY_ADDR:   u16 = 0xFF4A;
pub const WX_ADDR:   u16 = 0xFF4B;

pub const SCREEN_WIDTH:     usize = 160;
pub const SCREEN_HEIGHT:    usize = 144;
pub const FRAMEBUFFER_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT;

// PPU mode cycle counts
const MODE2_CYCLES: u32 = 80;
const MODE3_CYCLES: u32 = 172;
const MODE0_CYCLES: u32 = 204;
const SCANLINE_CYCLES: u32 = MODE2_CYCLES + MODE3_CYCLES + MODE0_CYCLES; // 456
const VBLANK_LINES:    u32 = 10;

/// Result from one PPU step.
pub struct PpuStepResult {
    pub vblank_irq: bool,
    pub stat_irq:   bool,
    pub hblank:     bool, // entered mode 0 this step (used for HDMA)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Ppu {
    #[serde(with = "serde_fb_u32")]
    pub framebuffer: Box<[u32; FRAMEBUFFER_SIZE]>,

    // BG info per pixel for sprite priority resolution
    #[serde(with = "serde_fb_u8")]
    bg_color_index: Box<[u8; FRAMEBUFFER_SIZE]>,
    #[serde(with = "serde_fb_bool")]
    bg_priority:    Box<[bool; FRAMEBUFFER_SIZE]>,

    cycle:           u32,
    mode:            u8,
    pub frame_ready: bool,
    window_line:     u8,

    pub cgb_mode:          bool,
    /// When true, apply dmg_palettes even in CGB mode (palette override).
    pub force_dmg_palette: bool,
    pub dmg_palettes:      DmgColorPalettes,
}

impl Ppu {
    pub fn new() -> Self {
        Ppu {
            framebuffer:       Box::new([0x00FFFFFFu32; FRAMEBUFFER_SIZE]),
            bg_color_index:    Box::new([0u8;   FRAMEBUFFER_SIZE]),
            bg_priority:       Box::new([false; FRAMEBUFFER_SIZE]),
            cycle:             0,
            mode:              2,
            frame_ready:       false,
            window_line:       0,
            cgb_mode:          false,
            force_dmg_palette: false,
            dmg_palettes:      DEFAULT_PALETTE.clone(),
        }
    }

    pub fn step(&mut self, cycles: u32, mmu: &mut Mmu) -> PpuStepResult {
        let mut result = PpuStepResult { vblank_irq: false, stat_irq: false, hblank: false };

        let lcdc = mmu.read_byte(LCDC_ADDR);
        if lcdc & 0x80 == 0 {
            // LCD off — reset timing, blank screen
            self.cycle      = 0;
            self.mode       = 0;
            self.window_line = 0;
            mmu.write_byte(LY_ADDR, 0);
            return result;
        }

        self.cycle += cycles;
        let ly = mmu.read_byte(LY_ADDR);

        if ly < 144 {
            // Visible scanline
            match self.mode {
                2 => {
                    // OAM scan
                    if self.cycle >= MODE2_CYCLES {
                        self.cycle -= MODE2_CYCLES;
                        self.mode = 3;
                        self.update_stat(mmu, 3, &mut result);
                    }
                }
                3 => {
                    // Pixel transfer
                    if self.cycle >= MODE3_CYCLES {
                        self.cycle -= MODE3_CYCLES;
                        self.mode = 0;
                        self.render_scanline(ly, mmu);
                        result.hblank = true;
                        self.update_stat(mmu, 0, &mut result);
                    }
                }
                0 => {
                    // H-blank
                    if self.cycle >= MODE0_CYCLES {
                        self.cycle -= MODE0_CYCLES;
                        let next_ly = ly + 1;
                        mmu.write_byte(LY_ADDR, next_ly);
                        self.check_lyc(mmu, next_ly, &mut result);
                        if next_ly == 144 {
                            self.mode = 1;
                            self.frame_ready = true;
                            result.vblank_irq = true;
                            self.update_stat(mmu, 1, &mut result);
                        } else {
                            self.mode = 2;
                            self.update_stat(mmu, 2, &mut result);
                        }
                    }
                }
                _ => {}
            }
        } else {
            // V-blank
            if self.cycle >= SCANLINE_CYCLES {
                self.cycle -= SCANLINE_CYCLES;
                let next_ly = ly + 1;
                if next_ly > 153 {
                    mmu.write_byte(LY_ADDR, 0);
                    self.mode = 2;
                    self.window_line = 0;
                    self.update_stat(mmu, 2, &mut result);
                } else {
                    mmu.write_byte(LY_ADDR, next_ly);
                    self.check_lyc(mmu, next_ly, &mut result);
                }
            }
        }
        result
    }

    fn update_stat(&self, mmu: &mut Mmu, mode: u8, result: &mut PpuStepResult) {
        let stat = (mmu.read_byte(STAT_ADDR) & 0xF8) | (mode & 0x03);
        mmu.write_byte(STAT_ADDR, stat);
        let irq = match mode {
            0 => stat & 0x08 != 0,
            1 => stat & 0x10 != 0,
            2 => stat & 0x20 != 0,
            _ => false,
        };
        if irq { result.stat_irq = true; }
    }

    fn check_lyc(&self, mmu: &mut Mmu, ly: u8, result: &mut PpuStepResult) {
        let lyc  = mmu.read_byte(LYC_ADDR);
        let mut stat = mmu.read_byte(STAT_ADDR);
        if ly == lyc {
            stat |= 0x04;
            if stat & 0x40 != 0 { result.stat_irq = true; }
        } else {
            stat &= !0x04;
        }
        mmu.write_byte(STAT_ADDR, stat);
    }

    // ── Scanline rendering ────────────────────────────────────────────────────

    fn render_scanline(&mut self, ly: u8, mmu: &mut Mmu) {
        let lcdc = mmu.read_byte(LCDC_ADDR);

        // Clear priority buffer
        let base = ly as usize * SCREEN_WIDTH;
        for i in 0..SCREEN_WIDTH {
            self.bg_color_index[base + i] = 0;
            self.bg_priority[base + i]    = false;
        }

        let use_cgb = self.cgb_mode && !self.force_dmg_palette;

        // BG + Window
        if use_cgb || (lcdc & 0x01 != 0) {
            self.render_bg(ly, lcdc, mmu, use_cgb);
            if lcdc & 0x20 != 0 { self.render_window(ly, lcdc, mmu, use_cgb); }
        }

        // Sprites
        if lcdc & 0x02 != 0 {
            self.render_sprites(ly, lcdc, mmu, use_cgb);
        }
    }

    fn render_bg(&mut self, ly: u8, lcdc: u8, mmu: &mut Mmu, use_cgb: bool) {
        let scx  = mmu.read_byte(SCX_ADDR);
        let scy  = mmu.read_byte(SCY_ADDR);
        let bgp  = mmu.read_byte(BGP_ADDR);
        let tile_map_base: u16 = if lcdc & 0x08 != 0 { 0x9C00 } else { 0x9800 };
        let signed_tile = lcdc & 0x10 == 0;
        let base = ly as usize * SCREEN_WIDTH;

        for x in 0..SCREEN_WIDTH {
            let bg_x = (x as u8).wrapping_add(scx) as usize;
            let bg_y = (ly).wrapping_add(scy) as usize;
            let tile_col = bg_x / 8;
            let tile_row = bg_y / 8;
            let map_addr = tile_map_base + (tile_row * 32 + tile_col) as u16;

            let tile_num = mmu.read_vram_bank(map_addr, 0);
            let tile_attr = if use_cgb { mmu.read_vram_bank(map_addr, 1) } else { 0 };

            let vram_bank = if use_cgb && tile_attr & 0x08 != 0 { 1 } else { 0 };
            let y_flip    = use_cgb && tile_attr & 0x40 != 0;
            let x_flip    = use_cgb && tile_attr & 0x20 != 0;
            let pal_num   = (tile_attr & 0x07) as usize;
            let priority  = use_cgb && tile_attr & 0x80 != 0;

            let tile_addr = tile_addr(tile_num, signed_tile);
            let row = bg_y % 8;
            let pixel_row = if y_flip { 7 - row } else { row };
            let col = bg_x % 8;
            let pixel_col = if x_flip { 7 - col } else { col };

            let lo = mmu.read_vram_bank(tile_addr + (pixel_row * 2) as u16,     vram_bank);
            let hi = mmu.read_vram_bank(tile_addr + (pixel_row * 2 + 1) as u16, vram_bank);
            let bit = 7 - pixel_col;
            let color_idx = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);

            let color = if use_cgb {
                mmu.bg_palette.get_color(pal_num, color_idx as usize)
            } else {
                let shade = (bgp >> (color_idx * 2)) & 0x03;
                self.dmg_palettes.bg[shade as usize]
            };

            self.framebuffer[base + x]    = color;
            self.bg_color_index[base + x] = color_idx;
            self.bg_priority[base + x]    = priority;
        }
    }

    fn render_window(&mut self, ly: u8, lcdc: u8, mmu: &mut Mmu, use_cgb: bool) {
        let wy = mmu.read_byte(WY_ADDR);
        let wx = mmu.read_byte(WX_ADDR).wrapping_sub(7);

        if ly < wy { return; }
        let base = ly as usize * SCREEN_WIDTH;
        let tile_map_base: u16 = if lcdc & 0x40 != 0 { 0x9C00 } else { 0x9800 };
        let signed_tile = lcdc & 0x10 == 0;
        let bgp = mmu.read_byte(BGP_ADDR);
        let win_y = self.window_line as usize;

        let mut rendered = false;
        for x in 0..SCREEN_WIDTH {
            if (x as u8) < wx { continue; }
            rendered = true;
            let win_x = x - wx as usize;
            let tile_col = win_x / 8;
            let tile_row = win_y / 8;
            let map_addr = tile_map_base + (tile_row * 32 + tile_col) as u16;

            let tile_num  = mmu.read_vram_bank(map_addr, 0);
            let tile_attr = if use_cgb { mmu.read_vram_bank(map_addr, 1) } else { 0 };
            let vram_bank = if use_cgb && tile_attr & 0x08 != 0 { 1 } else { 0 };
            let y_flip    = use_cgb && tile_attr & 0x40 != 0;
            let x_flip    = use_cgb && tile_attr & 0x20 != 0;
            let pal_num   = (tile_attr & 0x07) as usize;
            let priority  = use_cgb && tile_attr & 0x80 != 0;

            let tile_addr = tile_addr(tile_num, signed_tile);
            let row = win_y % 8;
            let pixel_row = if y_flip { 7 - row } else { row };
            let col = win_x % 8;
            let pixel_col = if x_flip { 7 - col } else { col };

            let lo = mmu.read_vram_bank(tile_addr + (pixel_row * 2) as u16,     vram_bank);
            let hi = mmu.read_vram_bank(tile_addr + (pixel_row * 2 + 1) as u16, vram_bank);
            let bit = 7 - pixel_col;
            let color_idx = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);

            let color = if use_cgb {
                mmu.bg_palette.get_color(pal_num, color_idx as usize)
            } else {
                let shade = (bgp >> (color_idx * 2)) & 0x03;
                self.dmg_palettes.bg[shade as usize]
            };

            self.framebuffer[base + x]    = color;
            self.bg_color_index[base + x] = color_idx;
            self.bg_priority[base + x]    = priority;
        }
        if rendered { self.window_line += 1; }
    }

    fn render_sprites(&mut self, ly: u8, lcdc: u8, mmu: &mut Mmu, use_cgb: bool) {
        let sprite_height: i32 = if lcdc & 0x04 != 0 { 16 } else { 8 };
        let obp0 = mmu.read_byte(OBP0_ADDR);
        let obp1 = mmu.read_byte(OBP1_ADDR);
        let base = ly as usize * SCREEN_WIDTH;
        let bg_enabled = lcdc & 0x01 != 0;

        // Collect visible sprites (up to 10)
        let mut sprites: Vec<[u8; 4]> = Vec::with_capacity(10);
        for s in 0..40 {
            if sprites.len() == 10 { break; }
            let oam_base = s * 4;
            let sy   = mmu.read_oam(oam_base)     as i32 - 16;
            let sx   = mmu.read_oam(oam_base + 1) as i32 - 8;
            let tile = mmu.read_oam(oam_base + 2);
            let flags= mmu.read_oam(oam_base + 3);
            let ly_i = ly as i32;
            if ly_i >= sy && ly_i < sy + sprite_height {
                sprites.push([
                    sy as u8,
                    (sx + 128) as u8,  // offset to keep sign info
                    tile,
                    flags,
                ]);
                let _ = (sy, sx, tile, flags); // suppress warnings
            }
        }

        // Re-collect with proper signed values for rendering
        let mut vis: Vec<(i32, i32, u8, u8)> = Vec::with_capacity(10);
        for s in 0..40 {
            if vis.len() == 10 { break; }
            let oam_base = s * 4;
            let sy   = mmu.read_oam(oam_base)     as i32 - 16;
            let sx   = mmu.read_oam(oam_base + 1) as i32 - 8;
            let tile = mmu.read_oam(oam_base + 2);
            let flags= mmu.read_oam(oam_base + 3);
            let ly_i = ly as i32;
            if ly_i >= sy && ly_i < sy + sprite_height {
                vis.push((sy, sx, tile, flags));
            }
        }

        // Draw in reverse order (lower index = higher priority in DMG)
        for &(sy, sx, mut tile, flags) in vis.iter().rev() {
            let y_flip = flags & 0x40 != 0;
            let x_flip = flags & 0x20 != 0;
            let behind = flags & 0x80 != 0;  // behind BG colors 1-3

            if sprite_height == 16 { tile &= 0xFE; }

            let sprite_row = (ly as i32 - sy) as usize;
            let row = if y_flip { sprite_height as usize - 1 - sprite_row } else { sprite_row };

            let vram_bank: u8 = if use_cgb { (flags >> 3) & 0x01 } else { 0 };
            let tile_addr = 0x8000u16 + tile as u16 * 16;

            let lo = mmu.read_vram_bank(tile_addr + (row * 2) as u16,     vram_bank);
            let hi = mmu.read_vram_bank(tile_addr + (row * 2 + 1) as u16, vram_bank);

            for col in 0..8i32 {
                let screen_x = sx + col;
                if screen_x < 0 || screen_x >= SCREEN_WIDTH as i32 { continue; }
                let sx_usize = screen_x as usize;

                let bit = if x_flip { col as usize } else { 7 - col as usize };
                let color_idx = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
                if color_idx == 0 { continue; } // transparent

                // Priority rules
                let bg_color = self.bg_color_index[base + sx_usize];
                if bg_enabled {
                    if behind && bg_color != 0 { continue; }
                    if use_cgb && self.bg_priority[base + sx_usize] && bg_color != 0 { continue; }
                }

                let color = if use_cgb {
                    let pal_num = (flags & 0x07) as usize;
                    mmu.obj_palette.get_color(pal_num, color_idx as usize)
                } else {
                    let obp = if flags & 0x10 != 0 { obp1 } else { obp0 };
                    let shade = (obp >> (color_idx * 2)) & 0x03;
                    if flags & 0x10 != 0 {
                        self.dmg_palettes.obj1[shade as usize]
                    } else {
                        self.dmg_palettes.obj0[shade as usize]
                    }
                };
                self.framebuffer[base + sx_usize] = color;
            }
        }
    }
}

impl Default for Ppu { fn default() -> Self { Self::new() } }

// ── Helper: compute tile data address in VRAM ─────────────────────────────────

fn tile_addr(tile_num: u8, signed: bool) -> u16 {
    if signed {
        // 0x9000 base, signed offset
        let offset = (tile_num as i8 as i16) * 16;
        (0x9000i32 + offset as i32) as u16
    } else {
        0x8000 + tile_num as u16 * 16
    }
}

// ── Serde helpers for fixed-size boxed arrays ─────────────────────────────────

mod serde_fb_u32 {
    use serde::{Deserializer, Serializer, Deserialize};
    const N: usize = super::FRAMEBUFFER_SIZE;
    pub fn serialize<S>(v: &Box<[u32; N]>, s: S) -> Result<S::Ok, S::Error> where S: Serializer {
        s.collect_seq(v.iter())
    }
    pub fn deserialize<'de, D>(d: D) -> Result<Box<[u32; N]>, D::Error> where D: Deserializer<'de> {
        let v = Vec::<u32>::deserialize(d)?;
        if v.len() != N { return Err(serde::de::Error::custom("wrong fb size")); }
        let mut a = Box::new([0u32; N]);
        a.copy_from_slice(&v);
        Ok(a)
    }
}

mod serde_fb_u8 {
    use serde::{Deserializer, Serializer, Deserialize};
    const N: usize = super::FRAMEBUFFER_SIZE;
    pub fn serialize<S>(v: &Box<[u8; N]>, s: S) -> Result<S::Ok, S::Error> where S: Serializer {
        s.collect_seq(v.iter())
    }
    pub fn deserialize<'de, D>(d: D) -> Result<Box<[u8; N]>, D::Error> where D: Deserializer<'de> {
        let v = Vec::<u8>::deserialize(d)?;
        if v.len() != N { return Err(serde::de::Error::custom("wrong fb size")); }
        let mut a = Box::new([0u8; N]);
        a.copy_from_slice(&v);
        Ok(a)
    }
}

mod serde_fb_bool {
    use serde::{Deserializer, Serializer, Deserialize};
    const N: usize = super::FRAMEBUFFER_SIZE;
    pub fn serialize<S>(v: &Box<[bool; N]>, s: S) -> Result<S::Ok, S::Error> where S: Serializer {
        s.collect_seq(v.iter())
    }
    pub fn deserialize<'de, D>(d: D) -> Result<Box<[bool; N]>, D::Error> where D: Deserializer<'de> {
        let v = Vec::<bool>::deserialize(d)?;
        if v.len() != N { return Err(serde::de::Error::custom("wrong fb size")); }
        let mut a = Box::new([false; N]);
        a.copy_from_slice(&v);
        Ok(a)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Mmu;

    fn ppu_and_mmu() -> (Ppu, Mmu) {
        let mut ppu = Ppu::new();
        let mut mmu = Mmu::new();
        ppu.cgb_mode = false;
        mmu.write_byte(LCDC_ADDR, 0x91);
        mmu.write_byte(BGP_ADDR,  0xE4);
        (ppu, mmu)
    }

    #[test]
    fn test_ly_advances_per_scanline() {
        let (mut ppu, mut mmu) = ppu_and_mmu();
        for _ in 0..SCANLINE_CYCLES { ppu.step(1, &mut mmu); }
        assert_eq!(mmu.read_byte(LY_ADDR), 1);
    }

    #[test]
    fn test_vblank_after_144_lines() {
        let (mut ppu, mut mmu) = ppu_and_mmu();
        let mut vblank = false;
        for _ in 0..(SCANLINE_CYCLES * 144) {
            let r = ppu.step(1, &mut mmu);
            if r.vblank_irq { vblank = true; }
        }
        assert!(vblank);
    }

    #[test]
    fn test_frame_ready_set_at_vblank() {
        let (mut ppu, mut mmu) = ppu_and_mmu();
        for _ in 0..(SCANLINE_CYCLES * 144) { ppu.step(1, &mut mmu); }
        assert!(ppu.frame_ready);
    }

    #[test]
    fn test_framebuffer_size() {
        let ppu = Ppu::new();
        assert_eq!(ppu.framebuffer.len(), FRAMEBUFFER_SIZE);
    }

    #[test]
    fn test_cgb_mode_flag() {
        let mut ppu = Ppu::new();
        ppu.cgb_mode = true;
        assert!(ppu.cgb_mode);
    }

    #[test]
    fn test_force_dmg_palette_flag() {
        let mut ppu = Ppu::new();
        ppu.force_dmg_palette = true;
        assert!(ppu.force_dmg_palette);
    }

    #[test]
    fn test_tile_addr_unsigned() {
        assert_eq!(tile_addr(0, false),  0x8000);
        assert_eq!(tile_addr(1, false),  0x8010);
        assert_eq!(tile_addr(255, false), 0x8FF0);
    }

    #[test]
    fn test_tile_addr_signed() {
        assert_eq!(tile_addr(0,   true), 0x9000);
        assert_eq!(tile_addr(127, true), 0x97F0);
        // tile_num=128 = -128i8 → 0x9000 - 128*16 = 0x9000 - 0x800 = 0x8800
        assert_eq!(tile_addr(128, true), 0x8800);
    }
}