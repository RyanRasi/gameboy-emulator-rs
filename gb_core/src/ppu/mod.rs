//! Game Boy Pixel Processing Unit (PPU)
//!
//! Renders a 160×144 pixel display using a scanline-based approach.
//! Each visible scanline cycles through three modes:
//!
//!   Mode 2 — OAM Search      80 T-cycles
//!   Mode 3 — Pixel Transfer  172 T-cycles
//!   Mode 0 — HBlank          204 T-cycles
//!   ── repeat for lines 0–143 ──
//!   Mode 1 — VBlank          lines 144–153, 456 T-cycles each
//!
//! One complete frame = 154 lines × 456 T-cycles = 70,224 T-cycles
//!
//! Key registers (all in I/O space):
//!   0xFF40  LCDC — LCD Control
//!   0xFF41  STAT — LCD Status
//!   0xFF42  SCY  — Scroll Y
//!   0xFF43  SCX  — Scroll X
//!   0xFF44  LY   — Current scanline (read-only to CPU)
//!   0xFF45  LYC  — LY Compare
//!   0xFF47  BGP  — Background Palette Data

use crate::mmu::Mmu;

pub const SCREEN_WIDTH:     usize = 160;
pub const SCREEN_HEIGHT:    usize = 144;
pub const FRAMEBUFFER_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT;

pub const CYCLES_OAM:      u32 = 80;
pub const CYCLES_TRANSFER: u32 = 172;
pub const CYCLES_HBLANK:   u32 = 204;
pub const CYCLES_PER_LINE: u32 = 456;

pub const VBLANK_START: u8 = 144;
pub const TOTAL_LINES:  u8 = 154;

// ── I/O registers ─────────────────────────────────────────────────────────────
pub const LCDC_ADDR: u16 = 0xFF40;
pub const STAT_ADDR: u16 = 0xFF41;
pub const SCY_ADDR:  u16 = 0xFF42;
pub const SCX_ADDR:  u16 = 0xFF43;
pub const LY_ADDR:   u16 = 0xFF44;
pub const LYC_ADDR:  u16 = 0xFF45;
pub const BGP_ADDR:  u16 = 0xFF47;
pub const OBP0_ADDR: u16 = 0xFF48;
pub const OBP1_ADDR: u16 = 0xFF49;
pub const WY_ADDR:   u16 = 0xFF4A;
pub const WX_ADDR:   u16 = 0xFF4B;

// ── PPU modes ─────────────────────────────────────────────────────────────────
pub const MODE_HBLANK:   u8 = 0;
pub const MODE_VBLANK:   u8 = 1;
pub const MODE_OAM:      u8 = 2;
pub const MODE_TRANSFER: u8 = 3;

#[derive(Debug, Default, Clone)]
pub struct PpuResult {
    pub vblank_irq: bool,
    pub stat_irq:   bool,
}

pub struct Ppu {
    pub framebuffer: Box<[u8; FRAMEBUFFER_SIZE]>,
    /// Tracks which pixels were written by a non-transparent BG/Window pixel.
    /// Used for sprite priority (sprites hide behind BG color 1–3 when flag set).
    bg_priority:     Box<[bool; FRAMEBUFFER_SIZE]>,
    cycle:           u32,
    mode:            u8,
    pub frame_ready: bool,
    /// Internal window line counter — increments each time a window line is drawn.
    window_line:     u8,
}

impl Ppu {
    pub fn new() -> Self {
        Ppu {
            framebuffer:  Box::new([0u8; FRAMEBUFFER_SIZE]),
            bg_priority:  Box::new([false; FRAMEBUFFER_SIZE]),
            cycle:        0,
            mode:         MODE_OAM,
            frame_ready:  false,
            window_line:  0,
        }
    }

    pub fn mode(&self) -> u8 { self.mode }

    pub fn step(&mut self, cycles: u32, mmu: &mut Mmu) -> PpuResult {
        let mut result = PpuResult::default();

        if mmu.read_byte(LCDC_ADDR) & 0x80 == 0 {
            self.cycle       = 0;
            self.mode        = MODE_HBLANK;
            self.window_line = 0;
            mmu.write_byte(LY_ADDR, 0);
            self.update_stat(mmu);
            return result;
        }

        self.cycle += cycles;

        loop {
            let ly = mmu.read_byte(LY_ADDR);

            if ly < VBLANK_START {
                match self.mode {
                    MODE_OAM if self.cycle >= CYCLES_OAM => {
                        self.cycle -= CYCLES_OAM;
                        self.mode   = MODE_TRANSFER;
                    }
                    MODE_TRANSFER if self.cycle >= CYCLES_TRANSFER => {
                        self.cycle -= CYCLES_TRANSFER;
                        self.mode   = MODE_HBLANK;
                        self.render_scanline(ly, mmu);
                        if mmu.read_byte(STAT_ADDR) & 0x08 != 0 {
                            result.stat_irq = true;
                        }
                    }
                    MODE_HBLANK if self.cycle >= CYCLES_HBLANK => {
                        self.cycle -= CYCLES_HBLANK;
                        let next_ly = ly + 1;
                        mmu.write_byte(LY_ADDR, next_ly);

                        if next_ly >= VBLANK_START {
                            self.mode        = MODE_VBLANK;
                            self.frame_ready = true;
                            self.window_line = 0; // reset window counter for next frame
                            result.vblank_irq = true;
                            if mmu.read_byte(STAT_ADDR) & 0x10 != 0 {
                                result.stat_irq = true;
                            }
                        } else {
                            self.mode = MODE_OAM;
                            if mmu.read_byte(STAT_ADDR) & 0x20 != 0 {
                                result.stat_irq = true;
                            }
                        }

                        {
                            let new_ly = mmu.read_byte(LY_ADDR);
                            let lyc    = mmu.read_byte(LYC_ADDR);
                            if new_ly == lyc && mmu.read_byte(STAT_ADDR) & 0x40 != 0 {
                                result.stat_irq = true;
                            }
                        }
                    }
                    _ => break,
                }
            } else {
                if self.cycle >= CYCLES_PER_LINE {
                    self.cycle -= CYCLES_PER_LINE;
                    let next_ly = ly + 1;
                    if next_ly >= TOTAL_LINES {
                        mmu.write_byte(LY_ADDR, 0);
                        self.mode = MODE_OAM;
                        self.frame_ready = false;
                        if mmu.read_byte(STAT_ADDR) & 0x20 != 0 {
                            result.stat_irq = true;
                        }
                    } else {
                        mmu.write_byte(LY_ADDR, next_ly);
                    }
                    {
                        let new_ly = mmu.read_byte(LY_ADDR);
                        let lyc    = mmu.read_byte(LYC_ADDR);
                        if new_ly == lyc && mmu.read_byte(STAT_ADDR) & 0x40 != 0 {
                            result.stat_irq = true;
                        }
                    }
                } else {
                    break;
                }
            }
        }

        self.update_stat(mmu);
        result
    }

    fn update_stat(&self, mmu: &mut Mmu) {
        let stat  = mmu.read_byte(STAT_ADDR);
        let ly    = mmu.read_byte(LY_ADDR);
        let lyc   = mmu.read_byte(LYC_ADDR);
        let coinc = if ly == lyc { 0x04 } else { 0x00 };
        mmu.write_byte(STAT_ADDR, (stat & 0xF8) | coinc | (self.mode & 0x03));
    }

    // =========================================================================
    // Scanline rendering — Background → Window → Sprites
    // =========================================================================

    fn render_scanline(&mut self, ly: u8, mmu: &Mmu) {
        let lcdc = mmu.read_byte(LCDC_ADDR);

        // Clear priority buffer for this line
        let base = ly as usize * SCREEN_WIDTH;
        for i in 0..SCREEN_WIDTH {
            self.bg_priority[base + i] = false;
        }

        if lcdc & 0x01 != 0 {
            self.render_background(ly, lcdc, mmu);
        }

        if lcdc & 0x20 != 0 {
            self.render_window(ly, lcdc, mmu);
        }

        if lcdc & 0x02 != 0 {
            self.render_sprites(ly, lcdc, mmu);
        }
    }

    // ── Background ────────────────────────────────────────────────────────────

    fn render_background(&mut self, ly: u8, lcdc: u8, mmu: &Mmu) {
        let scy = mmu.read_byte(SCY_ADDR);
        let scx = mmu.read_byte(SCX_ADDR);
        let bgp = mmu.read_byte(BGP_ADDR);
        let map_base: u16 = if lcdc & 0x08 != 0 { 0x9C00 } else { 0x9800 };
        let use_signed = lcdc & 0x10 == 0;

        let y        = ly.wrapping_add(scy);
        let tile_row = (y / 8) as u16;

        for px in 0..SCREEN_WIDTH as u8 {
            let x        = px.wrapping_add(scx);
            let tile_col = (x / 8) as u16;
            let map_addr = map_base + tile_row * 32 + tile_col;
            let tile_num = mmu.read_byte(map_addr);
            let tile_addr = tile_addr(tile_num, use_signed);
            let row_off   = (y % 8) as u16 * 2;
            let lo = mmu.read_byte(tile_addr + row_off);
            let hi = mmu.read_byte(tile_addr + row_off + 1);
            let bit   = 7 - (x % 8);
            let cidx  = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
            let shade = (bgp >> (cidx * 2)) & 0x03;

            let idx = ly as usize * SCREEN_WIDTH + px as usize;
            self.framebuffer[idx]  = shade;
            // Priority: BG color 1–3 blocks sprites when sprite has BG-priority flag
            self.bg_priority[idx]  = cidx != 0;
        }
    }

    // ── Window ────────────────────────────────────────────────────────────────

    fn render_window(&mut self, ly: u8, lcdc: u8, mmu: &Mmu) {
        let wy = mmu.read_byte(WY_ADDR);
        let wx = mmu.read_byte(WX_ADDR).wrapping_sub(7); // WX is offset by 7

        // Window only draws on lines >= WY
        if ly < wy { return; }

        let bgp = mmu.read_byte(BGP_ADDR);
        let map_base: u16 = if lcdc & 0x40 != 0 { 0x9C00 } else { 0x9800 };
        let use_signed = lcdc & 0x10 == 0;

        let y        = self.window_line;
        let tile_row = (y / 8) as u16;
        let mut drew_any = false;

        for px in 0..SCREEN_WIDTH as u8 {
            if px < wx { continue; }
            let x        = px - wx;
            let tile_col = (x / 8) as u16;
            let map_addr = map_base + tile_row * 32 + tile_col;
            let tile_num = mmu.read_byte(map_addr);
            let tile_addr = tile_addr(tile_num, use_signed);
            let row_off   = (y % 8) as u16 * 2;
            let lo = mmu.read_byte(tile_addr + row_off);
            let hi = mmu.read_byte(tile_addr + row_off + 1);
            let bit   = 7 - (x % 8);
            let cidx  = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
            let shade = (bgp >> (cidx * 2)) & 0x03;

            let idx = ly as usize * SCREEN_WIDTH + px as usize;
            self.framebuffer[idx] = shade;
            self.bg_priority[idx] = cidx != 0;
            drew_any = true;
        }

        if drew_any {
            self.window_line = self.window_line.wrapping_add(1);
        }
    }

    // ── Sprites ───────────────────────────────────────────────────────────────

    fn render_sprites(&mut self, ly: u8, lcdc: u8, mmu: &Mmu) {
        // Sprite height: 8 or 16 pixels (LCDC bit 2)
        let tall = lcdc & 0x04 != 0;
        let height: u8 = if tall { 16 } else { 8 };

        // Collect visible sprites for this scanline (max 10 per line, hardware limit)
        let mut sprites: Vec<(u8, u8, u8, u8)> = Vec::with_capacity(10);

        for sprite in 0..40usize {
            let base  = 0xFE00 + (sprite * 4) as u16;
            let spy   = mmu.read_byte(base);
            let spx   = mmu.read_byte(base + 1);
            let tile  = mmu.read_byte(base + 2);
            let flags = mmu.read_byte(base + 3);

            // Sprite Y is stored as screen_y + 16
            let screen_y = spy.wrapping_sub(16);
            if ly < screen_y || ly >= screen_y.wrapping_add(height) { continue; }
            // Sprite X=0 means off-screen left
            if spx == 0 { continue; }

            sprites.push((spy, spx, tile, flags));
            if sprites.len() == 10 { break; }
        }

        // Draw in reverse order so lower-indexed sprites win on overlap
        for (spy, spx, tile_raw, flags) in sprites.iter().rev() {
            let spy   = *spy;
            let spx   = *spx;
            let flags = *flags;

            let y_flip    = flags & 0x40 != 0;
            let x_flip    = flags & 0x20 != 0;
            let use_obp1  = flags & 0x10 != 0;
            let bg_pri    = flags & 0x80 != 0;

            let palette = mmu.read_byte(if use_obp1 { OBP1_ADDR } else { OBP0_ADDR });

            // For 8×16 sprites, bit 0 of tile index is ignored
            let tile_index = if tall { tile_raw & 0xFE } else { *tile_raw };

            let screen_y    = spy.wrapping_sub(16);
            let mut row     = ly.wrapping_sub(screen_y);
            if y_flip { row = (height - 1) - row; }

            let tile_addr = 0x8000u16 + (tile_index as u16) * 16 + (row as u16) * 2;
            let lo = mmu.read_byte(tile_addr);
            let hi = mmu.read_byte(tile_addr + 1);

            for bit_pos in 0..8u8 {
                let screen_x = (spx as i16) - 8 + (bit_pos as i16);
                if screen_x < 0 || screen_x >= SCREEN_WIDTH as i16 { continue; }
                let screen_x = screen_x as usize;

                let bit   = if x_flip { bit_pos } else { 7 - bit_pos };
                let cidx  = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
                if cidx == 0 { continue; } // color 0 = transparent for sprites

                let idx = ly as usize * SCREEN_WIDTH + screen_x;

                // BG priority: sprite hidden behind BG colors 1–3
                if bg_pri && self.bg_priority[idx] { continue; }

                let shade = (palette >> (cidx * 2)) & 0x03;
                self.framebuffer[idx] = shade;
            }
        }
    }
}

impl Default for Ppu {
    fn default() -> Self { Self::new() }
}

// ── Shared tile address helper ────────────────────────────────────────────────

fn tile_addr(tile_num: u8, use_signed: bool) -> u16 {
    if use_signed {
        let signed = tile_num as i8 as i32;
        (0x9000i32 + signed * 16) as u16
    } else {
        0x8000u16 + (tile_num as u16) * 16
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Mmu;

    fn setup() -> (Ppu, Mmu) { (Ppu::new(), Mmu::new()) }

    fn enable_lcd(mmu: &mut Mmu) {
        mmu.write_byte(LCDC_ADDR, 0x91); // LCD on, BG on, unsigned tiles
        mmu.write_byte(BGP_ADDR,  0xE4); // identity palette
    }

    fn write_tile_row(mmu: &mut Mmu, tile: u8, row: u8, lo: u8, hi: u8) {
        let base = 0x8000u16 + (tile as u16) * 16 + (row as u16) * 2;
        mmu.write_byte(base, lo);
        mmu.write_byte(base + 1, hi);
    }

    fn write_tile_map(mmu: &mut Mmu, tx: u8, ty: u8, tile: u8) {
        mmu.write_byte(0x9800 + (ty as u16) * 32 + tx as u16, tile);
    }

    // ── Dimensions ───────────────────────────────────────────────────────────

    #[test]
    fn test_screen_dimensions_are_correct() {
        assert_eq!(SCREEN_WIDTH,     160);
        assert_eq!(SCREEN_HEIGHT,    144);
        assert_eq!(FRAMEBUFFER_SIZE, 23040);
    }

    #[test]
    fn test_framebuffer_length_matches_constant() {
        let ppu = Ppu::new();
        assert_eq!(ppu.framebuffer.len(), FRAMEBUFFER_SIZE);
    }

    // ── Initial state ─────────────────────────────────────────────────────────

    #[test]
    fn test_initial_mode_is_oam() {
        assert_eq!(Ppu::new().mode(), MODE_OAM);
    }

    #[test]
    fn test_initial_frame_ready_is_false() {
        assert!(!Ppu::new().frame_ready);
    }

    #[test]
    fn test_initial_framebuffer_is_all_zero() {
        assert!(Ppu::new().framebuffer.iter().all(|&b| b == 0));
    }

    // ── Mode transitions ──────────────────────────────────────────────────────

    #[test]
    fn test_mode_stays_oam_before_80_cycles() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_OAM - 1, &mut mmu);
        assert_eq!(ppu.mode(), MODE_OAM);
    }

    #[test]
    fn test_mode_transitions_to_transfer_at_80_cycles() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_OAM, &mut mmu);
        assert_eq!(ppu.mode(), MODE_TRANSFER);
    }

    #[test]
    fn test_mode_transitions_to_hblank_after_oam_plus_transfer() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.mode(), MODE_HBLANK);
    }

    #[test]
    fn test_mode_returns_to_oam_after_full_line() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE, &mut mmu);
        assert_eq!(ppu.mode(), MODE_OAM);
    }

    // ── Scanline counter ─────────────────────────────────────────────────────

    #[test]
    fn test_ly_starts_at_zero() {
        let (_ppu, mmu) = setup();
        assert_eq!(mmu.read_byte(LY_ADDR), 0);
    }

    #[test]
    fn test_scanline_increments_after_one_full_line() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), 1);
    }

    #[test]
    fn test_scanline_increments_correctly_over_10_lines() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * 10, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), 10);
    }

    #[test]
    fn test_scanline_increments_in_small_steps() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        for _ in 0..(CYCLES_PER_LINE / 4) { ppu.step(4, &mut mmu); }
        assert_eq!(mmu.read_byte(LY_ADDR), 1);
    }

    // ── VBlank ───────────────────────────────────────────────────────────────

    #[test]
    fn test_vblank_fires_after_144_lines() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        let r = ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert!(r.vblank_irq);
    }

    #[test]
    fn test_ly_equals_144_when_vblank_fires() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), VBLANK_START);
    }

    #[test]
    fn test_mode_is_vblank_at_line_144() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert_eq!(ppu.mode(), MODE_VBLANK);
    }

    #[test]
    fn test_frame_ready_set_at_vblank() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert!(ppu.frame_ready);
    }

    #[test]
    fn test_vblank_does_not_fire_before_144_lines() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        let r = ppu.step(CYCLES_PER_LINE * (VBLANK_START as u32 - 1), &mut mmu);
        assert!(!r.vblank_irq);
    }

    #[test]
    fn test_ly_resets_to_zero_after_154_lines() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * TOTAL_LINES as u32, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), 0);
    }

    #[test]
    fn test_frame_ready_clears_at_start_of_new_frame() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert!(ppu.frame_ready);
        ppu.step(CYCLES_PER_LINE * (TOTAL_LINES - VBLANK_START) as u32, &mut mmu);
        assert!(!ppu.frame_ready);
    }

    // ── Background tile fetch ─────────────────────────────────────────────────

    #[test]
    fn test_tile_fetch_color1_all_pixels() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0xFF, 0x00);
        write_tile_map(&mut mmu, 0, 0, 0);
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.framebuffer[0], 1);
    }

    #[test]
    fn test_tile_fetch_color3_all_pixels() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0xFF, 0xFF);
        write_tile_map(&mut mmu, 0, 0, 0);
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.framebuffer[0], 3);
    }

    #[test]
    fn test_tile_fetch_full_row_width() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0xFF, 0x00);
        for tx in 0..20u8 { write_tile_map(&mut mmu, tx, 0, 0); }
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        for px in 0..SCREEN_WIDTH {
            assert_eq!(ppu.framebuffer[px], 1, "px {} wrong", px);
        }
    }

    // ── Sprite rendering ──────────────────────────────────────────────────────

    /// Write a sprite into OAM slot `i`.
    fn write_sprite(mmu: &mut Mmu, i: u8, y: u8, x: u8, tile: u8, flags: u8) {
        let base = 0xFE00 + (i as u16) * 4;
        mmu.write_byte(base,     y);     // screen_y + 16
        mmu.write_byte(base + 1, x);     // screen_x + 8
        mmu.write_byte(base + 2, tile);
        mmu.write_byte(base + 3, flags);
    }

    #[test]
    fn test_sprite_renders_on_scanline() {
        let (mut ppu, mut mmu) = setup();
        // LCDC: LCD on, BG on, sprites on (bits 7,1,0)
        mmu.write_byte(LCDC_ADDR, 0x83);
        mmu.write_byte(BGP_ADDR,  0xE4);
        mmu.write_byte(OBP0_ADDR, 0xE4); // identity palette

        // Tile 1, row 0: all color 1 (lo=0xFF, hi=0x00)
        write_tile_row(&mut mmu, 1, 0, 0xFF, 0x00);

        // Sprite 0: Y=16 (screen_y=0), X=8 (screen_x=0), tile 1, no flags
        write_sprite(&mut mmu, 0, 16, 8, 1, 0x00);

        // Step through OAM + Transfer to render line 0
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);

        assert_eq!(ppu.framebuffer[0], 1, "Sprite pixel must be shade 1");
    }

    #[test]
    fn test_sprite_color0_is_transparent() {
        let (mut ppu, mut mmu) = setup();
        // 0x93 = LCD on, sprites on, BG on, unsigned tile data (bit 4 = 1)
        mmu.write_byte(LCDC_ADDR, 0x93);
        mmu.write_byte(BGP_ADDR,  0xE4);
        mmu.write_byte(OBP0_ADDR, 0xE4);

        // BG tile 0, row 0: lo=0x00, hi=0xFF → color 2 everywhere
        write_tile_row(&mut mmu, 0, 0, 0x00, 0xFF);
        write_tile_map(&mut mmu, 0, 0, 0);

        // Sprite tile 1, row 0: all color 0 (transparent)
        write_tile_row(&mut mmu, 1, 0, 0x00, 0x00);
        write_sprite(&mut mmu, 0, 16, 8, 1, 0x00);

        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);

        assert_eq!(ppu.framebuffer[0], 2, "Transparent sprite must not cover BG");
    }

    #[test]
    fn test_sprite_x_flip() {
        let (mut ppu, mut mmu) = setup();
        mmu.write_byte(LCDC_ADDR, 0x83);
        mmu.write_byte(BGP_ADDR,  0x00); // all BG shade 0
        mmu.write_byte(OBP0_ADDR, 0xE4);

        // Tile: lo=0x80 (10000000), hi=0x00 → only leftmost bit set → pixel 0 = color 1
        write_tile_row(&mut mmu, 1, 0, 0x80, 0x00);
        // With X-flip, bit 7 (leftmost) becomes pixel 7
        write_sprite(&mut mmu, 0, 16, 8, 1, 0x20); // flag 0x20 = X-flip

        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);

        assert_eq!(ppu.framebuffer[0], 0, "X-flipped: pixel 0 should be transparent");
        assert_eq!(ppu.framebuffer[7], 1, "X-flipped: pixel 7 should be shade 1");
    }

    #[test]
    fn test_sprite_y_flip() {
        let (mut ppu, mut mmu) = setup();
        mmu.write_byte(LCDC_ADDR, 0x83);
        mmu.write_byte(BGP_ADDR,  0x00);
        mmu.write_byte(OBP0_ADDR, 0xE4);

        // Row 0 of tile: all transparent. Row 7: all color 1.
        write_tile_row(&mut mmu, 1, 0, 0x00, 0x00);
        write_tile_row(&mut mmu, 1, 7, 0xFF, 0x00);

        // Sprite at screen_y=0 with Y-flip: row 0 maps to tile row 7
        write_sprite(&mut mmu, 0, 16, 8, 1, 0x40); // 0x40 = Y-flip

        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);

        assert_eq!(ppu.framebuffer[0], 1, "Y-flipped row 0 should read tile row 7");
    }

    #[test]
    fn test_sprite_bg_priority_hides_behind_bg() {
        let (mut ppu, mut mmu) = setup();
        // 0x93 = LCD on, sprites on, BG on, unsigned tile data (bit 4 = 1)
        mmu.write_byte(LCDC_ADDR, 0x93);
        mmu.write_byte(BGP_ADDR,  0xE4);
        mmu.write_byte(OBP0_ADDR, 0xE4);

        // BG tile 0 row 0: all color 1 (non-zero → bg_priority set)
        write_tile_row(&mut mmu, 0, 0, 0xFF, 0x00);
        write_tile_map(&mut mmu, 0, 0, 0);

        // Sprite tile 1 row 0: all color 3
        write_tile_row(&mut mmu, 1, 0, 0xFF, 0xFF);
        // Sprite with BG priority flag (0x80)
        write_sprite(&mut mmu, 0, 16, 8, 1, 0x80);

        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);

        assert_eq!(ppu.framebuffer[0], 1, "BG priority flag must hide sprite behind BG");
    }

    #[test]
    fn test_sprite_obp1_palette() {
        let (mut ppu, mut mmu) = setup();
        mmu.write_byte(LCDC_ADDR, 0x83);
        mmu.write_byte(BGP_ADDR,  0x00);
        mmu.write_byte(OBP0_ADDR, 0xE4); // identity
        mmu.write_byte(OBP1_ADDR, 0x00); // all → shade 0

        write_tile_row(&mut mmu, 1, 0, 0xFF, 0x00); // color 1
        write_sprite(&mut mmu, 0, 16, 8, 1, 0x10);  // flag 0x10 = OBP1

        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);

        // OBP1 maps color 1 → shade 0
        assert_eq!(ppu.framebuffer[0], 0, "OBP1 palette must be used");
    }

    // ── Window rendering ──────────────────────────────────────────────────────

    #[test]
    fn test_window_renders_over_background() {
        let (mut ppu, mut mmu) = setup();
        // LCD on, BG on, Window on
        mmu.write_byte(LCDC_ADDR, 0xB1); // bits 7,5,4,0
        mmu.write_byte(BGP_ADDR,  0xE4);

        // BG tile 0: all color 1
        write_tile_row(&mut mmu, 0, 0, 0xFF, 0x00);
        write_tile_map(&mut mmu, 0, 0, 0);

        // Window tile 1: all color 3
        write_tile_row(&mut mmu, 1, 0, 0xFF, 0xFF);
        // Window tile map uses 0x9800 (LCDC bit 6 = 0)
        mmu.write_byte(0x9800, 1); // window map position (0,0) → tile 1

        // WY=0, WX=7 (wx-7=0 means window starts at screen x=0)
        mmu.write_byte(WY_ADDR, 0);
        mmu.write_byte(WX_ADDR, 7);

        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);

        // Window (color 3) must cover BG (color 1) at pixel 0
        assert_eq!(ppu.framebuffer[0], 3, "Window must render over background");
    }

    #[test]
    fn test_window_does_not_render_above_wy() {
        let (mut ppu, mut mmu) = setup();
        // 0xF1: LCD on, Window on (bit5), Window map=0x9C00 (bit6),
        //        unsigned tiles (bit4), BG on (bit0)
        mmu.write_byte(LCDC_ADDR, 0xF1);
        mmu.write_byte(BGP_ADDR,  0xE4);

        // BG uses 0x9800 (bit3=0). Tile 0, row 0: all color 1
        write_tile_row(&mut mmu, 0, 0, 0xFF, 0x00);
        write_tile_map(&mut mmu, 0, 0, 0); // 0x9800[0] = tile 0

        // Window uses 0x9C00 (bit6=1). Tile 1, row 0: all color 3
        write_tile_row(&mut mmu, 1, 0, 0xFF, 0xFF);
        mmu.write_byte(0x9C00, 1); // window map position (0,0) → tile 1

        // WY=5 → window must NOT appear on line 0
        mmu.write_byte(WY_ADDR, 5);
        mmu.write_byte(WX_ADDR, 7);

        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);

        assert_eq!(ppu.framebuffer[0], 1, "Window must not render above WY");
    }

    // ── LCD disabled ─────────────────────────────────────────────────────────

    #[test]
    fn test_lcd_disabled_resets_ly_to_zero() {
        let (mut ppu, mut mmu) = setup();
        mmu.write_byte(LY_ADDR,   50);
        mmu.write_byte(LCDC_ADDR, 0x00);
        ppu.step(100_000, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), 0);
    }

    #[test]
    fn test_lcd_disabled_does_not_fire_vblank() {
        let (mut ppu, mut mmu) = setup();
        mmu.write_byte(LCDC_ADDR, 0x00);
        let r = ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert!(!r.vblank_irq);
    }

    // ── STAT ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_stat_reflects_current_mode() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        ppu.step(0, &mut mmu);
        assert_eq!(mmu.read_byte(STAT_ADDR) & 0x03, MODE_OAM);
    }

    #[test]
    fn test_stat_coincidence_bit_set_when_ly_equals_lyc() {
        let (mut ppu, mut mmu) = setup(); enable_lcd(&mut mmu);
        mmu.write_byte(LYC_ADDR, 1);
        ppu.step(CYCLES_PER_LINE, &mut mmu);
        assert_ne!(mmu.read_byte(STAT_ADDR) & 0x04, 0);
    }
}