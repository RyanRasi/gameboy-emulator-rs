//! GBC boot ROM palette emulation for DMG games.
//!
//! The GBC boot ROM used a title checksum + 4th title character to select
//! one of 12 built-in palette slots for DMG games without CGB support.
//! This module replicates that logic so DMG games automatically get the
//! same colorization the real GBC hardware applied.

use crate::ppu::cgb_palette::CgbPalette;

/// Apply GBC boot ROM palettes to the CGB palette RAM for a DMG cartridge.
/// `rom` — the full ROM bytes.
/// `bg`  — the MMU's BG CgbPalette object (will be written to).
/// `obj` — the MMU's OBJ CgbPalette object (will be written to).
pub fn apply_bios_palettes(rom: &[u8], bg: &mut CgbPalette, obj: &mut CgbPalette) {
    let checksum = title_checksum(rom);
    let fourth   = if rom.len() > 0x0137 { rom[0x0137] } else { 0 };

    if is_pokemon_yellow(rom) {
        apply_pokemon_yellow_palettes(bg, obj);
        return;
    }

    let pal_idx = COMBINATION_TABLE.iter()
        .find(|&&(cs, fc, _)| cs == checksum && (fc == 0 || fc == fourth))
        .map(|&(_, _, idx)| idx)
        .unwrap_or(0); // default = palette 0 (green)

    write_palette_slot(bg,  &BG_PALETTES[pal_idx as usize]);
    write_palette_slot(obj, &OBJ_PALETTES[pal_idx as usize]);
}

/// Returns true if the ROM is Pokémon Yellow (needs special cheek color).
fn is_pokemon_yellow(rom: &[u8]) -> bool {
    let title: String = rom.get(0x0134..0x013C)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    title.to_uppercase().contains("POKEMON Y")
}

fn apply_pokemon_yellow_palettes(bg: &mut CgbPalette, obj: &mut CgbPalette) {
    // BG: yellow world
    write_palette_slot(bg, &[0xF8F8F8, 0xF8D000, 0xD09000, 0x302000]);
    // OBJ palette 0: Pikachu yellow
    write_obj_palette_slot(obj, 0, &[0xF8F8F8, 0xF8D000, 0xD09000, 0x302000]);
    // OBJ palette 1: yellow
    write_obj_palette_slot(obj, 1, &[0xF8F8F8, 0xF8D000, 0xD09000, 0x302000]);
    // OBJ palette 2: red (Pikachu's cheeks/mouth)
    write_obj_palette_slot(obj, 2, &[0xFF0000, 0xD82000, 0xA00000, 0x300000]);
}

/// Compute the title checksum (sum of 0x0134-0x013C mod 256).
fn title_checksum(rom: &[u8]) -> u8 {
    let end = 0x013D.min(rom.len());
    let start = 0x0134.min(end);
    rom[start..end].iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

fn write_palette_slot(pal: &mut CgbPalette, colors: &[u32; 4]) {
    write_obj_palette_slot(pal, 0, colors);
}

fn write_obj_palette_slot(pal: &mut CgbPalette, slot: usize, colors: &[u32; 4]) {
    for (i, &rgb888) in colors.iter().enumerate() {
        let r5 = ((rgb888 >> 16) & 0xFF) >> 3;
        let g5 = ((rgb888 >>  8) & 0xFF) >> 3;
        let b5 = ( rgb888        & 0xFF) >> 3;
        let rgb555 = (r5 | (g5 << 5) | (b5 << 10)) as u16;
        let off = slot * 8 + i * 2;
        if off + 1 < 64 {
            pal.ram[off]     = (rgb555 & 0xFF) as u8;
            pal.ram[off + 1] = (rgb555 >> 8) as u8;
        }
    }
}

// ── Official GBC boot ROM palettes (12 slots) ─────────────────────────────────
// Colors stored as 0x00RRGGBB.

type Palette = [u32; 4];

/// BG palettes for each slot.
const BG_PALETTES: [Palette; 12] = [
    [0xE0F8D0, 0x88C070, 0x346856, 0x081820], // 0 Green (default)
    [0xF8F8F8, 0xA8A8A8, 0x505050, 0x000000], // 1 Grayscale
    [0xF8E8C8, 0xD0A870, 0x885830, 0x281000], // 2 Brown
    [0xD0E8F8, 0x6090C8, 0x204878, 0x081020], // 3 Blue
    [0xF8F0C8, 0xE09038, 0x904808, 0x201000], // 4 Orange
    [0xF8F888, 0xD8C838, 0x807010, 0x181800], // 5 Yellow
    [0xF8D8E8, 0xF090B8, 0xB02878, 0x300010], // 6 Pink
    [0xF0FFF0, 0x50E8A0, 0x007860, 0x001820], // 7 Teal
    [0xF8F8F8, 0x90B8C8, 0x385878, 0x080818], // 8 Cool Blue
    [0xF8F8E0, 0xB8C858, 0x507818, 0x182808], // 9 Olive
    [0xF8D8E8, 0xC888A8, 0x785870, 0x100020], // 10 Purple
    [0x9BBC0F, 0x8BAC0F, 0x306230, 0x0F380F], // 11 Classic DMG Green
];

/// OBJ palettes for each slot.
const OBJ_PALETTES: [Palette; 12] = [
    [0xE0F8D0, 0x88C070, 0x346856, 0x081820],
    [0xF8F8F8, 0xA8A8A8, 0x505050, 0x000000],
    [0xF8D888, 0xE09030, 0x905008, 0x201000],
    [0xF8E8D0, 0xD09858, 0x784820, 0x200800],
    [0xC8F8C8, 0x60C050, 0x208030, 0x082010],
    [0xF8C8F8, 0xC870C8, 0x683068, 0x180818],
    [0xF8F8A8, 0xD8D040, 0x808000, 0x181800],
    [0xF8E8C8, 0xD0A870, 0x885830, 0x281000],
    [0xF8D8A8, 0xE09050, 0x884818, 0x200800],
    [0xF8E888, 0xD8A000, 0x905000, 0x281800],
    [0xE8C880, 0xC89830, 0x785008, 0x181000],
    [0x9BBC0F, 0x8BAC0F, 0x306230, 0x0F380F],
];

/// Combination table: (checksum, 4th_char, palette_index).
/// fourth_char = 0 means "any".
const COMBINATION_TABLE: &[(u8, u8, u8)] = &[
    (0x00, 0,    0), // default green
    (0x88, b'K', 11), // Kirby — classic green
    (0x16, b'A', 4),  // Tetris — orange/blue
    (0xB3, 0,    3),  // Zelda — blue
    (0x03, b'B', 5),  // Super Mario Land — yellow
    (0xE8, 0,    2),  // Donkey Kong — brown
    (0x97, b'O', 7),  // Metroid — teal
    (0x54, 0,    1),  // Grayscale fallback
    (0x39, b'E', 5),  // Mega Man — yellow
    (0x43, b'S', 0),  // Various — green
    (0x97, b'C', 6),  // Castlevania — pink
];

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rom_with_title(title: &[u8]) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        let end = (0x013C).min(0x0134 + title.len());
        rom[0x0134..end].copy_from_slice(&title[..end - 0x0134]);
        rom
    }

    #[test]
    fn test_apply_bios_palettes_does_not_panic() {
        let rom = make_rom_with_title(b"TETRIS");
        let mut bg  = CgbPalette::new();
        let mut obj = CgbPalette::new();
        apply_bios_palettes(&rom, &mut bg, &mut obj);
    }

    #[test]
    fn test_apply_sets_nonzero_bg_color() {
        let rom = make_rom_with_title(b"TETRIS");
        let mut bg  = CgbPalette::new();
        let mut obj = CgbPalette::new();
        apply_bios_palettes(&rom, &mut bg, &mut obj);
        // At least one non-0xFF palette byte should have been written
        let all_ff = bg.ram.iter().all(|&b| b == 0xFF);
        assert!(!all_ff, "BG palette should have been written");
    }

    #[test]
    fn test_pokemon_yellow_gets_red_cheeks() {
        let rom = make_rom_with_title(b"POKEMON YELLOW");
        let mut bg  = CgbPalette::new();
        let mut obj = CgbPalette::new();
        apply_bios_palettes(&rom, &mut bg, &mut obj);
        // OBJ palette 2 color 0 should be red (non-zero in channel)
        let r = obj.ram[16]; // palette 2, color 0, low byte
        let g = obj.ram[17];
        let color = r as u16 | ((g as u16) << 8);
        assert!(color & 0x001F != 0, "Pikachu cheeks must have red component: {:04X}", color);
    }

    #[test]
    fn test_title_checksum_empty() {
        let rom = vec![0u8; 0x0140];
        assert_eq!(title_checksum(&rom), 0);
    }

    #[test]
    fn test_is_not_pokemon_yellow() {
        let rom = make_rom_with_title(b"POKEMON RED");
        assert!(!is_pokemon_yellow(&rom));
    }
}