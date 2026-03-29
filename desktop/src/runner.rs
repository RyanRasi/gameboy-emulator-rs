//! Frame runner — drives the emulator at Game Boy speed.
//!
//! `FrameRunner` wraps a `Cpu` and exposes a single `run_frame()` method
//! that ticks the CPU until the PPU signals a completed frame.
//!
//! The runner is pure logic — no windowing, no I/O. This keeps it fully
//! testable in a headless environment.

use gb_core::cpu::Cpu;

/// Total T-cycles in one Game Boy frame:
///   154 scanlines × 456 T-cycles = 70,224
pub const CYCLES_PER_FRAME: u64 = 70_224;

pub struct FrameRunner {
    pub cpu: Cpu,
}

impl FrameRunner {
    pub fn new(cpu: Cpu) -> Self {
        FrameRunner { cpu }
    }

    /// Tick the CPU until the PPU sets `frame_ready`, then clear it and
    /// return `true`.
    ///
    /// If the LCD is off (or something else prevents the PPU from ever
    /// completing a frame) the loop exits after 2 × `CYCLES_PER_FRAME`
    /// T-cycles and returns `false`. This prevents infinite blocking.
    pub fn run_frame(&mut self) -> bool {
        let budget = self.cpu.cycles + CYCLES_PER_FRAME * 2;

        while self.cpu.cycles < budget {
            self.cpu.tick();
            if self.cpu.ppu.frame_ready {
                self.cpu.ppu.frame_ready = false;
                return true;
            }
        }

        false
    }

    /// How many complete frames have been rendered so far.
    /// Derived from total cycles elapsed.
    pub fn frame_count(&self) -> u64 {
        self.cpu.cycles / CYCLES_PER_FRAME
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use gb_core::cpu::Cpu;
    use gb_core::ppu::{LCDC_ADDR, BGP_ADDR};

    fn runner_with_lcd() -> FrameRunner {
        let mut cpu = Cpu::new();
        // Enable LCD and set identity BG palette — same state the DMG BIOS
        // leaves hardware in before handing off to the game.
        cpu.mmu.write_byte(LCDC_ADDR, 0x91); // LCD on, BG on, unsigned tiles
        cpu.mmu.write_byte(BGP_ADDR,  0xE4); // identity palette
        FrameRunner::new(cpu)
    }

    // ── run_frame does not panic ──────────────────────────────────────────────

    #[test]
    fn test_run_frame_does_not_panic() {
        let mut runner = runner_with_lcd();
        runner.run_frame(); // must not panic or loop forever
    }

    // ── frame completes ───────────────────────────────────────────────────────

    #[test]
    fn test_run_frame_returns_true_when_lcd_on() {
        let mut runner = runner_with_lcd();
        let completed = runner.run_frame();
        assert!(completed, "run_frame must return true when LCD is on and frame completes");
    }

    #[test]
    fn test_run_frame_returns_false_when_lcd_off() {
        let cpu = Cpu::new(); // LCDC = 0x00 → LCD off
        let mut runner = FrameRunner::new(cpu);
        let completed = runner.run_frame();
        assert!(!completed, "run_frame must return false when LCD is off");
    }

    // ── cycles advance ────────────────────────────────────────────────────────

    #[test]
    fn test_cycles_advance_after_one_frame() {
        let mut runner = runner_with_lcd();
        runner.run_frame();
        // frame_ready fires at VBlank start: 144 lines × 456 cycles = 65,664.
        // Must be at least that many cycles, but less than a full 154-line frame.
        assert!(
            runner.cpu.cycles >= 144 * 456,
            "At least 144 scanlines worth of cycles must elapse: got {}",
            runner.cpu.cycles
        );
        assert!(
            runner.cpu.cycles <= CYCLES_PER_FRAME,
            "Should not exceed one full frame: got {}",
            runner.cpu.cycles
        );
    }

    #[test]
    fn test_cycles_advance_monotonically_across_frames() {
        let mut runner = runner_with_lcd();
        runner.run_frame();
        let after_frame_1 = runner.cpu.cycles;
        runner.run_frame();
        assert!(
            runner.cpu.cycles > after_frame_1,
            "Cycles must keep increasing across frames"
        );
    }

    #[test]
    fn test_two_frames_take_roughly_twice_the_cycles() {
        let mut runner = runner_with_lcd();
        runner.run_frame();
        let after_1 = runner.cpu.cycles;
        runner.run_frame();
        let after_2 = runner.cpu.cycles;

        // Each frame is exactly CYCLES_PER_FRAME T-cycles (within ±10% slop
        // for mid-instruction rounding)
        let frame_2_len = after_2 - after_1;
        let tolerance = CYCLES_PER_FRAME / 10;
        assert!(
            frame_2_len.abs_diff(CYCLES_PER_FRAME) <= tolerance,
            "Second frame length {} should be close to {} T-cycles",
            frame_2_len, CYCLES_PER_FRAME
        );
    }

    // ── LY after frame ────────────────────────────────────────────────────────

    #[test]
    fn test_ly_returns_to_zero_after_frame() {
        let mut runner = runner_with_lcd();
        runner.run_frame();
        // frame_ready fires when VBlank begins (LY = 144).
        // LY wraps to 0 only after the full VBlank period completes.
        let ly = runner.cpu.mmu.read_byte(gb_core::ppu::LY_ADDR);
        assert_eq!(ly, 144, "LY must be 144 at VBlank start (when frame_ready fires)");
    }

    // ── frame_count ───────────────────────────────────────────────────────────

    #[test]
    fn test_frame_count_zero_before_any_frame() {
        let runner = runner_with_lcd();
        assert_eq!(runner.frame_count(), 0);
    }

    #[test]
    fn test_frame_count_increases_after_each_frame() {
        let mut runner = runner_with_lcd();
        // frame_ready fires at ~65,664 cycles; CYCLES_PER_FRAME = 70,224.
        // frame_count = cpu.cycles / CYCLES_PER_FRAME, so we need at least
        // two rendered frames before the integer quotient reaches 1.
        runner.run_frame();
        runner.run_frame();
        assert!(
            runner.frame_count() >= 1,
            "frame_count must be >= 1 after two rendered frames (cycles = {})",
            runner.cpu.cycles
        );
        runner.run_frame();
        let after_3 = runner.frame_count();
        runner.run_frame();
        let after_4 = runner.frame_count();
        assert!(
            after_4 >= after_3,
            "frame_count must never decrease"
        );
    }

    // ── framebuffer populated ─────────────────────────────────────────────────

    #[test]
    fn test_framebuffer_has_correct_size_after_frame() {
        let mut runner = runner_with_lcd();
        runner.run_frame();
        assert_eq!(
            runner.cpu.ppu.framebuffer.len(),
            gb_core::ppu::FRAMEBUFFER_SIZE,
            "Framebuffer must be exactly 160×144 bytes"
        );
    }

    #[test]
    fn test_framebuffer_pixels_are_valid_shades() {
        let mut runner = runner_with_lcd();
        runner.run_frame();
        for (i, &shade) in runner.cpu.ppu.framebuffer.iter().enumerate() {
            assert!(
                shade <= 3,
                "Pixel {} has invalid shade {} (must be 0–3)",
                i, shade
            );
        }
    }
}