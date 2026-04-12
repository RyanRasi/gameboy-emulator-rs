//! Converts the u32 RGB framebuffer to a minifb-compatible pixel buffer.

use gb_core::ppu::{SCREEN_WIDTH, SCREEN_HEIGHT, FRAMEBUFFER_SIZE};

/// Copy the emulator framebuffer (0x00RRGGBB) directly for minifb.
pub fn framebuffer_to_pixels(fb: &[u32]) -> Vec<u32> {
    debug_assert_eq!(fb.len(), FRAMEBUFFER_SIZE);
    fb.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_framebuffer_size() {
        assert_eq!(FRAMEBUFFER_SIZE, SCREEN_WIDTH * SCREEN_HEIGHT);
    }

    #[test]
    fn test_passthrough() {
        let fb: Vec<u32> = (0..FRAMEBUFFER_SIZE as u32).collect();
        let out = framebuffer_to_pixels(&fb);
        assert_eq!(out.len(), FRAMEBUFFER_SIZE);
        assert_eq!(out[0], 0);
        assert_eq!(out[FRAMEBUFFER_SIZE - 1], (FRAMEBUFFER_SIZE - 1) as u32);
    }

    #[test]
    fn test_white_pixel() {
        let fb = vec![0x00FFFFFFu32; FRAMEBUFFER_SIZE];
        let out = framebuffer_to_pixels(&fb);
        assert!(out.iter().all(|&p| p == 0x00FFFFFF));
    }
}