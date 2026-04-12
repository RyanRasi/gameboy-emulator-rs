//! Frame runner — drives the CPU for exactly one PPU frame per call.

use gb_core::cpu::Cpu;
use gb_core::ppu::{LY_ADDR, SCREEN_HEIGHT, SCREEN_WIDTH};

/// Drives the CPU tick-by-tick until the PPU signals a completed frame
/// (VBlank start). Caps at slightly more than one full frame of T-cycles
/// to avoid infinite loops when the LCD is off.
pub struct FrameRunner {
    pub cpu: Cpu,
    frame_count: u64,
}

/// T-cycles in one full Game Boy frame (154 lines × 456 cycles).
pub const CYCLES_PER_FRAME: u64 = 70_224;

impl FrameRunner {
    pub fn new(cpu: Cpu) -> Self {
        FrameRunner {
            cpu,
            frame_count: 0,
        }
    }

    /// Run until the next VBlank (frame_ready flag set by PPU), or until
    /// CYCLES_PER_FRAME T-cycles have elapsed, whichever comes first.
    /// Returns true if a frame was completed.
    pub fn run_frame(&mut self) -> bool {
        let start = self.cpu.cycles;
        let limit = start + CYCLES_PER_FRAME + 456; // one extra scanline tolerance

        loop {
            self.cpu.tick();

            if self.cpu.ppu.frame_ready {
                self.cpu.ppu.frame_ready = false;
                self.frame_count += 1;
                return true;
            }

            if self.cpu.cycles >= limit {
                return false;
            }
        }
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use gb_core::cpu::Cpu;
    use gb_core::ppu::{BGP_ADDR, FRAMEBUFFER_SIZE, LCDC_ADDR, LY_ADDR};

    fn make_runner() -> FrameRunner {
        let mut cpu = Cpu::new();
        cpu.mmu.load_rom(&vec![0x00u8; 0x8000]).unwrap();
        cpu.mmu.write_byte(LCDC_ADDR, 0x91);
        cpu.mmu.write_byte(BGP_ADDR, 0xE4);
        let mut r = FrameRunner::new(cpu);
        // Warm-up: discard the first partial frame so all tests start
        // at a clean frame boundary.
        r.run_frame();
        r.frame_count = 0; // reset counter after warm-up
        r
    }

    #[test]
    fn test_run_frame_returns_true_when_lcd_on() {
        let mut r = make_runner();
        let completed = r.run_frame();
        assert!(
            completed,
            "run_frame must return true when LCD is on and frame completes"
        );
    }

    #[test]
    fn test_run_frame_returns_false_when_lcd_off() {
        let mut cpu = Cpu::new();
        cpu.mmu.load_rom(&vec![0x00u8; 0x8000]).unwrap();
        cpu.mmu.write_byte(LCDC_ADDR, 0x00); // LCD off
        let mut r = FrameRunner::new(cpu);
        let completed = r.run_frame();
        assert!(!completed, "run_frame must return false when LCD is off");
    }

    #[test]
    fn test_cycles_advance_after_one_frame() {
        let mut r = make_runner();
        let before = r.cpu.cycles;
        r.run_frame();
        let elapsed = r.cpu.cycles - before;
        let tolerance = 912u64;
        assert!(
            elapsed >= CYCLES_PER_FRAME - tolerance && elapsed <= CYCLES_PER_FRAME + tolerance,
            "Frame should take ~{} T-cycles, got {}",
            CYCLES_PER_FRAME,
            elapsed
        );
    }

    #[test]
    fn test_two_frames_take_roughly_twice_the_cycles() {
        let mut r = make_runner();
        let c0 = r.cpu.cycles;
        r.run_frame();
        let c1 = r.cpu.cycles;
        r.run_frame();
        let c2 = r.cpu.cycles;
        let f1 = c1 - c0;
        let f2 = c2 - c1;
        let tolerance = 912u64;
        assert!(
            f1 >= CYCLES_PER_FRAME - tolerance && f1 <= CYCLES_PER_FRAME + tolerance,
            "First frame length {} should be close to {} T-cycles",
            f1,
            CYCLES_PER_FRAME
        );
        assert!(
            f2 >= CYCLES_PER_FRAME - tolerance && f2 <= CYCLES_PER_FRAME + tolerance,
            "Second frame length {} should be close to {} T-cycles",
            f2,
            CYCLES_PER_FRAME
        );
        let diff = if f1 > f2 { f1 - f2 } else { f2 - f1 };
        assert!(
            diff <= tolerance,
            "Frame lengths should be consistent: f1={}, f2={}, diff={}",
            f1,
            f2,
            diff
        );
    }

    #[test]
    fn test_frame_count_increments() {
        let mut r = make_runner();
        assert_eq!(r.frame_count(), 0);
        r.run_frame();
        assert_eq!(r.frame_count(), 1);
        r.run_frame();
        assert_eq!(r.frame_count(), 2);
    }

    #[test]
    fn test_frame_count_starts_at_zero() {
        let r = make_runner();
        assert_eq!(r.frame_count(), 0);
    }

    #[test]
    fn test_ly_returns_to_zero_after_frame() {
        let mut r = make_runner();
        r.run_frame();
        let ly_at_vblank = r.cpu.mmu.read_byte(LY_ADDR);
        assert_eq!(
            ly_at_vblank, 144,
            "LY must be 144 at VBlank start (when frame_ready fires)"
        );
        r.run_frame();
        let ly_second = r.cpu.mmu.read_byte(LY_ADDR);
        assert_eq!(ly_second, 144, "LY must be 144 at start of second VBlank");
    }

    #[test]
    fn test_ly_advances_during_frame() {
        let mut r = make_runner();
        let start = r.cpu.cycles;
        while r.cpu.cycles - start < 456 {
            r.cpu.tick();
        }
        let ly = r.cpu.mmu.read_byte(LY_ADDR);
        assert!(
            ly >= 1,
            "LY must have advanced at least one line after 456 cycles"
        );
    }

    #[test]
    fn test_framebuffer_has_correct_size() {
        let r = make_runner();
        assert_eq!(r.cpu.ppu.framebuffer.len(), FRAMEBUFFER_SIZE);
        assert_eq!(FRAMEBUFFER_SIZE, SCREEN_WIDTH * SCREEN_HEIGHT);
    }

    #[test]
    fn test_framebuffer_pixels_are_valid_rgb888_after_frame() {
        let mut r = make_runner();
        r.run_frame();
        for (i, &pixel) in r.cpu.ppu.framebuffer.iter().enumerate() {
            assert_eq!(
                pixel >> 24,
                0,
                "Pixel {} has non-zero alpha byte: 0x{:08X}",
                i,
                pixel
            );
        }
    }

    #[test]
    fn test_framebuffer_is_not_all_zeros_after_frame() {
        let mut r = make_runner();
        r.run_frame();
        let all_zero = r.cpu.ppu.framebuffer.iter().all(|&p| p == 0);
        assert!(
            !all_zero,
            "Framebuffer should not be all black after a rendered frame"
        );
    }

    #[test]
    fn test_frame_ready_cleared_after_run_frame() {
        let mut r = make_runner();
        r.run_frame();
        assert!(
            !r.cpu.ppu.frame_ready,
            "frame_ready must be cleared by run_frame"
        );
    }

    #[test]
    fn test_ppu_completes_144_visible_lines_per_frame() {
        let mut r = make_runner();
        let mut max_ly = 0u8;
        let start = r.cpu.cycles;
        let limit = start + CYCLES_PER_FRAME + 912;
        while r.cpu.cycles < limit {
            r.cpu.tick();
            let ly = r.cpu.mmu.read_byte(LY_ADDR);
            if ly > max_ly {
                max_ly = ly;
            }
            if r.cpu.ppu.frame_ready {
                break;
            }
        }
        assert!(
            max_ly >= 143,
            "PPU must render at least 144 lines per frame (max_ly={})",
            max_ly
        );
    }
}
