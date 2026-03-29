//! Framebuffer renderer.
//!
//! Converts the PPU's raw shade buffer (bytes 0–3) into a packed-pixel
//! buffer suitable for display via minifb (`0x00RRGGBB` u32 per pixel).
//!
//! The DMG has four shades mapped to a classic green-tinted palette
//! (or monochrome white-to-black depending on preference — easily swapped).

use core::ppu::FRAMEBUFFER_SIZE;

/// Classic DMG-style monochrome palette — shade 0 = lightest, 3 = darkest.
/// Format: 0x00RRGGBB (minifb native).
pub const PALETTE: [u32; 4] = [
    0x00FFFFFF, // shade 0 — white
    0x00AAAAAA, // shade 1 — light grey
    0x00555555, // shade 2 — dark grey
    0x00000000, // shade 3 — black
];

/// Convert a raw PPU framebuffer (one byte per pixel, value 0–3)
/// into a `Vec<u32>` of packed `0x00RRGGBB` values ready for minifb.
pub fn framebuffer_to_pixels(framebuffer: &[u8; FRAMEBUFFER_SIZE]) -> Vec<u32> {
    framebuffer
        .iter()
        .map(|&shade| PALETTE[(shade & 0x03) as usize])
        .collect()
}

/// Convert a single shade byte (0–3) to its `0x00RRGGBB` colour value.
pub fn shade_to_rgb(shade: u8) -> u32 {
    PALETTE[(shade & 0x03) as usize]
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── shade_to_rgb ──────────────────────────────────────────────────────────

    #[test]
    fn test_shade_0_is_white() {
        assert_eq!(shade_to_rgb(0), 0x00FFFFFF);
    }

    #[test]
    fn test_shade_1_is_light_grey() {
        assert_eq!(shade_to_rgb(1), 0x00AAAAAA);
    }

    #[test]
    fn test_shade_2_is_dark_grey() {
        assert_eq!(shade_to_rgb(2), 0x00555555);
    }

    #[test]
    fn test_shade_3_is_black() {
        assert_eq!(shade_to_rgb(3), 0x00000000);
    }

    #[test]
    fn test_shade_clamped_to_2_bits() {
        // Values above 3 should be masked to lower 2 bits
        assert_eq!(shade_to_rgb(4), shade_to_rgb(0)); // 4 & 3 = 0
        assert_eq!(shade_to_rgb(5), shade_to_rgb(1)); // 5 & 3 = 1
        assert_eq!(shade_to_rgb(7), shade_to_rgb(3)); // 7 & 3 = 3
    }

    // ── framebuffer_to_pixels ─────────────────────────────────────────────────

    #[test]
    fn test_output_length_matches_framebuffer() {
        let fb = [0u8; FRAMEBUFFER_SIZE];
        let pixels = framebuffer_to_pixels(&fb);
        assert_eq!(pixels.len(), FRAMEBUFFER_SIZE);
    }

    #[test]
    fn test_output_length_is_160_times_144() {
        let fb = [0u8; FRAMEBUFFER_SIZE];
        let pixels = framebuffer_to_pixels(&fb);
        assert_eq!(pixels.len(), 160 * 144);
    }

    #[test]
    fn test_all_zeros_give_all_white() {
        let fb = [0u8; FRAMEBUFFER_SIZE];
        let pixels = framebuffer_to_pixels(&fb);
        assert!(pixels.iter().all(|&p| p == 0x00FFFFFF));
    }

    #[test]
    fn test_all_threes_give_all_black() {
        let fb = [3u8; FRAMEBUFFER_SIZE];
        let pixels = framebuffer_to_pixels(&fb);
        assert!(pixels.iter().all(|&p| p == 0x00000000));
    }

    #[test]
    fn test_mixed_shades_map_to_correct_colours() {
        let mut fb = [0u8; FRAMEBUFFER_SIZE];
        fb[0] = 0;
        fb[1] = 1;
        fb[2] = 2;
        fb[3] = 3;
        let pixels = framebuffer_to_pixels(&fb);
        assert_eq!(pixels[0], 0x00FFFFFF);
        assert_eq!(pixels[1], 0x00AAAAAA);
        assert_eq!(pixels[2], 0x00555555);
        assert_eq!(pixels[3], 0x00000000);
    }

    #[test]
    fn test_each_pixel_maps_independently() {
        // Alternating 0 and 3 → alternating white and black
        let mut fb = [0u8; FRAMEBUFFER_SIZE];
        for (i, b) in fb.iter_mut().enumerate() {
            *b = if i % 2 == 0 { 0 } else { 3 };
        }
        let pixels = framebuffer_to_pixels(&fb);
        for (i, &px) in pixels.iter().enumerate() {
            let expected = if i % 2 == 0 { 0x00FFFFFF } else { 0x00000000 };
            assert_eq!(px, expected, "Pixel {} wrong", i);
        }
    }

    #[test]
    fn test_pixel_order_is_row_major() {
        // Pixel at (x, y) = index y*160 + x
        // Set pixel (5, 2) = shade 3 and verify its position in the output
        let mut fb = [0u8; FRAMEBUFFER_SIZE];
        fb[2 * 160 + 5] = 3;
        let pixels = framebuffer_to_pixels(&fb);
        assert_eq!(pixels[2 * 160 + 5], 0x00000000, "Pixel (5,2) must be black");
        assert_eq!(pixels[2 * 160 + 4], 0x00FFFFFF, "Pixel (4,2) must be white");
    }

    #[test]
    fn test_palette_has_four_entries() {
        assert_eq!(PALETTE.len(), 4);
    }

    #[test]
    fn test_palette_values_are_distinct() {
        // All four shades must produce different colours
        assert_ne!(PALETTE[0], PALETTE[1]);
        assert_ne!(PALETTE[1], PALETTE[2]);
        assert_ne!(PALETTE[2], PALETTE[3]);
    }
}