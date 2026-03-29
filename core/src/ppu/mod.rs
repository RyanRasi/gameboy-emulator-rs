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

// ── Screen dimensions ─────────────────────────────────────────────────────────
pub const SCREEN_WIDTH:     usize = 160;
pub const SCREEN_HEIGHT:    usize = 144;
pub const FRAMEBUFFER_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT; // 23,040

// ── T-cycle budgets ───────────────────────────────────────────────────────────
pub const CYCLES_OAM:      u32 = 80;
pub const CYCLES_TRANSFER: u32 = 172;
pub const CYCLES_HBLANK:   u32 = 204;
pub const CYCLES_PER_LINE: u32 = 456; // OAM + TRANSFER + HBLANK

// ── Scanline counters ─────────────────────────────────────────────────────────
pub const VBLANK_START: u8 = 144; // first VBlank line
pub const TOTAL_LINES:  u8 = 154; // wraps to 0 after this

// ── I/O register addresses ────────────────────────────────────────────────────
pub const LCDC_ADDR: u16 = 0xFF40;
pub const STAT_ADDR: u16 = 0xFF41;
pub const SCY_ADDR:  u16 = 0xFF42;
pub const SCX_ADDR:  u16 = 0xFF43;
pub const LY_ADDR:   u16 = 0xFF44;
pub const LYC_ADDR:  u16 = 0xFF45;
pub const BGP_ADDR:  u16 = 0xFF47;

// ── PPU mode constants ────────────────────────────────────────────────────────
pub const MODE_HBLANK:   u8 = 0;
pub const MODE_VBLANK:   u8 = 1;
pub const MODE_OAM:      u8 = 2;
pub const MODE_TRANSFER: u8 = 3;

/// Interrupt flags returned after each `Ppu::step` call.
/// The CPU is responsible for requesting the corresponding IRQs.
#[derive(Debug, Default, Clone)]
pub struct PpuResult {
    /// VBlank interrupt should be requested (bit 0 of IF).
    pub vblank_irq: bool,
    /// LCD STAT interrupt should be requested (bit 1 of IF).
    pub stat_irq: bool,
}

/// Game Boy Pixel Processing Unit.
pub struct Ppu {
    /// Raw pixel output — one byte per pixel, shade 0–3, row-major.
    /// Index = y * SCREEN_WIDTH + x.
    pub framebuffer: Box<[u8; FRAMEBUFFER_SIZE]>,

    /// T-cycles accumulated within the current scanline.
    cycle: u32,

    /// Current PPU mode (0–3).
    mode: u8,

    /// True from VBlank start until the next frame begins.
    pub frame_ready: bool,
}

impl Ppu {
    pub fn new() -> Self {
        Ppu {
            framebuffer: Box::new([0u8; FRAMEBUFFER_SIZE]),
            cycle:       0,
            mode:        MODE_OAM, // power-on state
            frame_ready: false,
        }
    }

    /// Current PPU mode.
    pub fn mode(&self) -> u8 { self.mode }

    /// Advance the PPU by `cycles` T-cycles.
    ///
    /// Handles all mode transitions that fall within the cycle budget
    /// (the internal loop runs until no further transitions are possible).
    /// Scanlines are rendered at the Mode 3 → Mode 0 boundary.
    pub fn step(&mut self, cycles: u32, mmu: &mut Mmu) -> PpuResult {
        let mut result = PpuResult::default();

        // If the LCD is off, hold everything in reset state.
        if mmu.read_byte(LCDC_ADDR) & 0x80 == 0 {
            self.cycle = 0;
            self.mode  = MODE_HBLANK;
            mmu.write_byte(LY_ADDR, 0);
            self.update_stat(mmu);
            return result;
        }

        self.cycle += cycles;

        // Process all mode transitions that fit in the accumulated cycle budget.
        loop {
            let ly = mmu.read_byte(LY_ADDR);

            if ly < VBLANK_START {
                // ── Visible scanlines: OAM → Transfer → HBlank ───────────────
                match self.mode {
                    MODE_OAM if self.cycle >= CYCLES_OAM => {
                        self.cycle -= CYCLES_OAM;
                        self.mode   = MODE_TRANSFER;
                        // loop continues to check next transition
                    }

                    MODE_TRANSFER if self.cycle >= CYCLES_TRANSFER => {
                        self.cycle -= CYCLES_TRANSFER;
                        self.mode   = MODE_HBLANK;
                        self.render_scanline(ly, mmu);
                        // HBlank STAT interrupt (bit 3)
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
                            result.vblank_irq = true;
                            // VBlank STAT interrupt (bit 4)
                            if mmu.read_byte(STAT_ADDR) & 0x10 != 0 {
                                result.stat_irq = true;
                            }
                        } else {
                            self.mode = MODE_OAM;
                            // OAM STAT interrupt (bit 5)
                            if mmu.read_byte(STAT_ADDR) & 0x20 != 0 {
                                result.stat_irq = true;
                            }
                        }

                        // LYC=LY coincidence check on every LY change
                        {
                            let new_ly = mmu.read_byte(LY_ADDR);
                            let lyc    = mmu.read_byte(LYC_ADDR);
                            if new_ly == lyc && mmu.read_byte(STAT_ADDR) & 0x40 != 0 {
                                result.stat_irq = true;
                            }
                        }
                    }

                    _ => break, // insufficient cycles for next transition
                }
            } else {
                // ── VBlank lines (144–153) ────────────────────────────────────
                if self.cycle >= CYCLES_PER_LINE {
                    self.cycle -= CYCLES_PER_LINE;
                    let next_ly = ly + 1;

                    if next_ly >= TOTAL_LINES {
                        // Frame complete — return to line 0
                        mmu.write_byte(LY_ADDR, 0);
                        self.mode        = MODE_OAM;
                        self.frame_ready = false;
                        if mmu.read_byte(STAT_ADDR) & 0x20 != 0 {
                            result.stat_irq = true; // OAM STAT for new frame
                        }
                    } else {
                        mmu.write_byte(LY_ADDR, next_ly);
                    }

                    // LYC=LY check on each VBlank LY change
                    {
                        let new_ly = mmu.read_byte(LY_ADDR);
                        let lyc    = mmu.read_byte(LYC_ADDR);
                        if new_ly == lyc && mmu.read_byte(STAT_ADDR) & 0x40 != 0 {
                            result.stat_irq = true;
                        }
                    }
                } else {
                    break; // still accumulating cycles in VBlank
                }
            }
        }

        self.update_stat(mmu);
        result
    }

    /// Write the current mode and LYC=LY coincidence flag into the STAT register.
    fn update_stat(&self, mmu: &mut Mmu) {
        let stat  = mmu.read_byte(STAT_ADDR);
        let ly    = mmu.read_byte(LY_ADDR);
        let lyc   = mmu.read_byte(LYC_ADDR);
        let coinc = if ly == lyc { 0x04 } else { 0x00 };
        // Preserve interrupt-enable bits (7–3); update coincidence (2) and mode (1–0)
        mmu.write_byte(STAT_ADDR, (stat & 0xF8) | coinc | (self.mode & 0x03));
    }

    fn render_scanline(&mut self, ly: u8, mmu: &Mmu) {
        let lcdc = mmu.read_byte(LCDC_ADDR);
        if lcdc & 0x01 != 0 {
            self.render_background(ly, lcdc, mmu);
        }
    }

    fn render_background(&mut self, ly: u8, lcdc: u8, mmu: &Mmu) {
        let scy = mmu.read_byte(SCY_ADDR);
        let scx = mmu.read_byte(SCX_ADDR);
        let bgp = mmu.read_byte(BGP_ADDR);

        // BG tile map base: LCDC bit 3
        let map_base: u16 = if lcdc & 0x08 != 0 { 0x9C00 } else { 0x9800 };

        // Tile data addressing: LCDC bit 4
        //   0 → signed index, base 0x9000 (-128..127 → 0x8800..0x97F0)
        //   1 → unsigned index, base 0x8000 (0..255 → 0x8000..0x8FF0)
        let use_signed = lcdc & 0x10 == 0;

        let y        = ly.wrapping_add(scy);
        let tile_row = (y / 8) as u16;

        for px in 0..SCREEN_WIDTH as u8 {
            let x        = px.wrapping_add(scx);
            let tile_col = (x / 8) as u16;

            // Tile index from the BG tile map
            let map_addr = map_base + tile_row * 32 + tile_col;
            let tile_num = mmu.read_byte(map_addr);

            // Tile data start address
            let tile_addr: u16 = if use_signed {
                let signed = tile_num as i8 as i32;
                (0x9000i32 + signed * 16) as u16
            } else {
                0x8000u16 + (tile_num as u16) * 16
            };

            // Two bytes encode one row of 8 pixels
            let row_offset = (y % 8) as u16 * 2;
            let lo  = mmu.read_byte(tile_addr + row_offset);
            let hi  = mmu.read_byte(tile_addr + row_offset + 1);

            // Bit 7 = leftmost pixel of the tile
            let bit   = 7 - (x % 8);
            let cidx  = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
            let shade = (bgp >> (cidx * 2)) & 0x03;

            self.framebuffer[ly as usize * SCREEN_WIDTH + px as usize] = shade;
        }
    }
}

impl Default for Ppu {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Mmu;

    fn setup() -> (Ppu, Mmu) {
        (Ppu::new(), Mmu::new())
    }

    /// LCDC = 0x91: LCD on, BG enabled, unsigned tile data at 0x8000, map at 0x9800.
    /// BGP = 0xE4: identity palette — shade == color index.
    fn enable_lcd(mmu: &mut Mmu) {
        mmu.write_byte(LCDC_ADDR, 0x91);
        mmu.write_byte(BGP_ADDR,  0xE4);
    }

    /// Write lo/hi bytes for one row of a tile (unsigned addressing, 0x8000-based).
    fn write_tile_row(mmu: &mut Mmu, tile_index: u8, row: u8, lo: u8, hi: u8) {
        let base = 0x8000u16 + (tile_index as u16) * 16 + (row as u16) * 2;
        mmu.write_byte(base,     lo);
        mmu.write_byte(base + 1, hi);
    }

    /// Write a tile index into the BG tile map at tile grid position (tx, ty).
    fn write_tile_map(mmu: &mut Mmu, tx: u8, ty: u8, tile_index: u8) {
        mmu.write_byte(0x9800 + (ty as u16) * 32 + tx as u16, tile_index);
    }

    // ── Dimensions ───────────────────────────────────────────────────────────

    #[test]
    fn test_screen_dimensions_are_correct() {
        assert_eq!(SCREEN_WIDTH,     160);
        assert_eq!(SCREEN_HEIGHT,    144);
        assert_eq!(FRAMEBUFFER_SIZE, 160 * 144);
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
        let ppu = Ppu::new();
        assert_eq!(ppu.mode(), MODE_OAM);
    }

    #[test]
    fn test_initial_frame_ready_is_false() {
        let ppu = Ppu::new();
        assert!(!ppu.frame_ready);
    }

    #[test]
    fn test_initial_framebuffer_is_all_zero() {
        let ppu = Ppu::new();
        assert!(ppu.framebuffer.iter().all(|&b| b == 0));
    }

    // ── Mode transitions ──────────────────────────────────────────────────────

    #[test]
    fn test_mode_stays_oam_before_80_cycles() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_OAM - 1, &mut mmu);
        assert_eq!(ppu.mode(), MODE_OAM);
    }

    #[test]
    fn test_mode_transitions_to_transfer_at_80_cycles() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_OAM, &mut mmu);
        assert_eq!(ppu.mode(), MODE_TRANSFER);
    }

    #[test]
    fn test_mode_transitions_to_hblank_after_oam_plus_transfer() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.mode(), MODE_HBLANK);
    }

    #[test]
    fn test_mode_returns_to_oam_after_full_line() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
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
    fn test_ly_does_not_increment_mid_line() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE - 1, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), 0);
    }

    #[test]
    fn test_scanline_increments_after_one_full_line() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), 1);
    }

    #[test]
    fn test_scanline_increments_correctly_over_10_lines() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * 10, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), 10);
    }

    #[test]
    fn test_scanline_increments_in_small_steps() {
        // Step 4 cycles at a time — simulates CPU NOP execution
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        for _ in 0..(CYCLES_PER_LINE / 4) {
            ppu.step(4, &mut mmu);
        }
        assert_eq!(mmu.read_byte(LY_ADDR), 1);
    }

    // ── VBlank ───────────────────────────────────────────────────────────────

    #[test]
    fn test_vblank_fires_after_144_lines() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        let result = ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert!(result.vblank_irq, "VBlank IRQ must fire at line 144");
    }

    #[test]
    fn test_ly_equals_144_when_vblank_fires() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), VBLANK_START);
    }

    #[test]
    fn test_mode_is_vblank_at_line_144() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert_eq!(ppu.mode(), MODE_VBLANK);
    }

    #[test]
    fn test_frame_ready_set_at_vblank() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert!(ppu.frame_ready);
    }

    #[test]
    fn test_vblank_does_not_fire_before_144_lines() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        // Step through 143 complete lines
        let result = ppu.step(CYCLES_PER_LINE * (VBLANK_START as u32 - 1), &mut mmu);
        assert!(!result.vblank_irq, "VBlank must not fire before line 144");
    }

    #[test]
    fn test_ly_resets_to_zero_after_154_lines() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * TOTAL_LINES as u32, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), 0, "LY must wrap to 0 after line 153");
    }

    #[test]
    fn test_frame_ready_clears_at_start_of_new_frame() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert!(ppu.frame_ready);
        ppu.step(CYCLES_PER_LINE * (TOTAL_LINES - VBLANK_START) as u32, &mut mmu);
        assert!(!ppu.frame_ready, "frame_ready must clear at start of new frame");
    }

    // ── Tile fetch / pixel data ───────────────────────────────────────────────

    #[test]
    fn test_tile_fetch_color1_all_pixels() {
        // lo=0xFF, hi=0x00 → every pixel in row 0 has color index 1
        // BGP=0xE4 → shade for color 1 = 1
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0xFF, 0x00);
        write_tile_map(&mut mmu, 0, 0, 0);
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.framebuffer[0], 1, "Pixel (0,0) must be shade 1");
        assert_eq!(ppu.framebuffer[7], 1, "Pixel (7,0) must be shade 1");
    }

    #[test]
    fn test_tile_fetch_color3_all_pixels() {
        // lo=0xFF, hi=0xFF → color index 3 → shade 3
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0xFF, 0xFF);
        write_tile_map(&mut mmu, 0, 0, 0);
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.framebuffer[0], 3);
    }

    #[test]
    fn test_tile_fetch_color0_all_pixels() {
        // lo=0x00, hi=0x00 → color index 0 → shade 0
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0x00, 0x00);
        write_tile_map(&mut mmu, 0, 0, 0);
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.framebuffer[0], 0);
    }

    #[test]
    fn test_tile_fetch_alternating_colors() {
        // lo=0xAA=10101010, hi=0x00
        // bit 7 (px 0): color=(0<<1)|1 = 1 → shade 1
        // bit 6 (px 1): color=(0<<1)|0 = 0 → shade 0
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0xAA, 0x00);
        write_tile_map(&mut mmu, 0, 0, 0);
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.framebuffer[0], 1, "px 0: color 1 → shade 1");
        assert_eq!(ppu.framebuffer[1], 0, "px 1: color 0 → shade 0");
        assert_eq!(ppu.framebuffer[2], 1, "px 2: color 1 → shade 1");
        assert_eq!(ppu.framebuffer[3], 0, "px 3: color 0 → shade 0");
    }

    #[test]
    fn test_tile_fetch_high_bit_gives_color2() {
        // lo=0x00, hi=0xFF → every pixel has color index 2 → shade 2
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0x00, 0xFF);
        write_tile_map(&mut mmu, 0, 0, 0);
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.framebuffer[0], 2, "hi=1, lo=0 → color 2 → shade 2");
    }

    #[test]
    fn test_tile_fetch_second_tile_in_row() {
        // tile 0 at map (0,0): all color 0
        // tile 1 at map (1,0): all color 3
        // Screen pixels 0–7 come from tile 0; pixels 8–15 from tile 1
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0x00, 0x00); // color 0
        write_tile_row(&mut mmu, 1, 0, 0xFF, 0xFF); // color 3
        write_tile_map(&mut mmu, 0, 0, 0);
        write_tile_map(&mut mmu, 1, 0, 1);
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.framebuffer[0],  0, "Tile 0 → shade 0");
        assert_eq!(ppu.framebuffer[7],  0, "Tile 0 end → shade 0");
        assert_eq!(ppu.framebuffer[8],  3, "Tile 1 start → shade 3");
        assert_eq!(ppu.framebuffer[15], 3, "Tile 1 end → shade 3");
    }

    #[test]
    fn test_tile_fetch_full_row_width() {
        // Fill all 20 tiles across row 0 with the same tile (color 1)
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0xFF, 0x00); // color 1
        for tx in 0..20u8 {
            write_tile_map(&mut mmu, tx, 0, 0);
        }
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        for px in 0..SCREEN_WIDTH {
            assert_eq!(
                ppu.framebuffer[px], 1,
                "All 160 pixels in row 0 must be shade 1 (failed at px {})", px
            );
        }
    }

    #[test]
    fn test_second_scanline_renders_separately() {
        // Row 0 of tile 0 = color 1; Row 1 of tile 0 = color 3
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        write_tile_row(&mut mmu, 0, 0, 0xFF, 0x00); // row 0 → color 1
        write_tile_row(&mut mmu, 0, 1, 0xFF, 0xFF); // row 1 → color 3
        write_tile_map(&mut mmu, 0, 0, 0);
        // Render line 0
        ppu.step(CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.framebuffer[0], 1, "Line 0 px 0 = shade 1");
        // Render line 1 (step through HBlank + OAM + Transfer for line 1)
        ppu.step(CYCLES_HBLANK + CYCLES_OAM + CYCLES_TRANSFER, &mut mmu);
        assert_eq!(ppu.framebuffer[SCREEN_WIDTH], 3, "Line 1 px 0 = shade 3");
    }

    // ── LCD disabled ─────────────────────────────────────────────────────────

    #[test]
    fn test_lcd_disabled_resets_ly_to_zero() {
        let (mut ppu, mut mmu) = setup();
        mmu.write_byte(LY_ADDR,   50);
        mmu.write_byte(LCDC_ADDR, 0x00); // LCD off
        ppu.step(100_000, &mut mmu);
        assert_eq!(mmu.read_byte(LY_ADDR), 0);
    }

    #[test]
    fn test_lcd_disabled_does_not_fire_vblank() {
        let (mut ppu, mut mmu) = setup();
        mmu.write_byte(LCDC_ADDR, 0x00);
        let result = ppu.step(CYCLES_PER_LINE * VBLANK_START as u32, &mut mmu);
        assert!(!result.vblank_irq);
    }

    // ── STAT register ─────────────────────────────────────────────────────────

    #[test]
    fn test_stat_reflects_current_mode() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(0, &mut mmu);
        assert_eq!(mmu.read_byte(STAT_ADDR) & 0x03, MODE_OAM);
    }

    #[test]
    fn test_stat_updates_on_mode_transition() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        ppu.step(CYCLES_OAM, &mut mmu);
        assert_eq!(mmu.read_byte(STAT_ADDR) & 0x03, MODE_TRANSFER);
    }

    #[test]
    fn test_stat_coincidence_bit_set_when_ly_equals_lyc() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        mmu.write_byte(LYC_ADDR, 1); // expect coincidence when LY = 1
        ppu.step(CYCLES_PER_LINE, &mut mmu); // advance to LY = 1
        let stat = mmu.read_byte(STAT_ADDR);
        assert_ne!(stat & 0x04, 0, "Coincidence bit must be set when LY == LYC");
    }

    #[test]
    fn test_stat_coincidence_bit_clear_when_ly_not_lyc() {
        let (mut ppu, mut mmu) = setup();
        enable_lcd(&mut mmu);
        mmu.write_byte(LYC_ADDR, 50); // won't match for a while
        ppu.step(4, &mut mmu);
        let stat = mmu.read_byte(STAT_ADDR);
        assert_eq!(stat & 0x04, 0, "Coincidence bit must be clear when LY != LYC");
    }
}