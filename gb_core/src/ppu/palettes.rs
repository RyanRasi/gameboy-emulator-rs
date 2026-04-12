//! DMG colorization palettes — official GBC boot ROM palettes + game database.

use serde::{Serialize, Deserialize};

/// Three independent 4-color palettes for BG, OBJ0, OBJ1.
/// Each color is 0x00RRGGBB. Index 0 = lightest, 3 = darkest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DmgColorPalettes {
    pub bg:   [u32; 4],
    pub obj0: [u32; 4],
    pub obj1: [u32; 4],
}

impl DmgColorPalettes {
    pub const fn uniform(c: [u32; 4]) -> Self {
        DmgColorPalettes { bg: c, obj0: c, obj1: c }
    }
}

/// Official GBC palettes selectable with F1/F2.
/// Based on the 12 palettes built into the GBC boot ROM.
pub const ALL_MANUAL_PALETTES: &[(&'static str, DmgColorPalettes)] = &[
    // 1 — Classic DMG Green (original LCD phosphor)
    ("Classic Green", DmgColorPalettes {
        bg:   [0x9BBC0F, 0x8BAC0F, 0x306230, 0x0F380F],
        obj0: [0x9BBC0F, 0x8BAC0F, 0x306230, 0x0F380F],
        obj1: [0x9BBC0F, 0x8BAC0F, 0x306230, 0x0F380F],
    }),
    // 2 — Game Boy Pocket (warm grey)
    ("GB Pocket",  DmgColorPalettes::uniform([0xC4CFA1, 0x8B956D, 0x4A5139, 0x1A1F12])),
    // 3 — Grayscale (pure black/white)
    ("Grayscale",  DmgColorPalettes::uniform([0xFFFFFF, 0xAAAAAA, 0x555555, 0x000000])),
    // 4 — GBC Right-button (default GBC DMG palette — green)
    ("GBC Green", DmgColorPalettes {
        bg:   [0xE0F8D0, 0x88C070, 0x346856, 0x081820],
        obj0: [0xE0F8D0, 0x88C070, 0x346856, 0x081820],
        obj1: [0xD0D058, 0xA0A840, 0x607028, 0x283800],
    }),
    // 5 — GBC Up-button (greyish-blue)
    ("GBC Cool", DmgColorPalettes {
        bg:   [0xF8F8F8, 0x90B8C8, 0x385878, 0x080818],
        obj0: [0xF8F8F8, 0x90B8C8, 0x385878, 0x080818],
        obj1: [0xF8E8A8, 0xC8A030, 0x785000, 0x201000],
    }),
    // 6 — GBC A+Right (red)
    ("GBC Red", DmgColorPalettes {
        bg:   [0xF8F8F8, 0xE09090, 0xA02010, 0x300000],
        obj0: [0xF8F8F8, 0xF8A050, 0xA04010, 0x200000],
        obj1: [0xF8F8F8, 0xA0C8F0, 0x2060C0, 0x000828],
    }),
    // 7 — GBC B+Right (orange/brown)
    ("GBC Orange", DmgColorPalettes {
        bg:   [0xF8F0C8, 0xE09038, 0x904808, 0x201000],
        obj0: [0xF8F0C8, 0xE09038, 0x904808, 0x201000],
        obj1: [0xC8F8C8, 0x60C050, 0x208030, 0x082010],
    }),
    // 8 — GBC A+Left (dark blue)
    ("GBC Blue", DmgColorPalettes {
        bg:   [0xD0E8F8, 0x6090C8, 0x204878, 0x081020],
        obj0: [0xD0E8F8, 0x6090C8, 0x204878, 0x081020],
        obj1: [0xF8E8D0, 0xD09858, 0x784820, 0x200800],
    }),
    // 9 — GBC B+Left (brown/tan)
    ("GBC Brown", DmgColorPalettes {
        bg:   [0xF8E8C8, 0xD0A870, 0x885830, 0x281000],
        obj0: [0xF8E8C8, 0xD0A870, 0x885830, 0x281000],
        obj1: [0xD8F0D8, 0x80B878, 0x386838, 0x081808],
    }),
    // 10 — GBC A+Up (pastel)
    ("GBC Pastel", DmgColorPalettes {
        bg:   [0xF8F8F8, 0x90E8C8, 0x386888, 0x081828],
        obj0: [0xF8F8F8, 0x90E8C8, 0x386888, 0x081828],
        obj1: [0xF8D8A8, 0xE09050, 0x884818, 0x200800],
    }),
    // 11 — GBC B+Up (yellow/gold)
    ("GBC Yellow", DmgColorPalettes {
        bg:   [0xF8F888, 0xD8C838, 0x807010, 0x181800],
        obj0: [0xF8F888, 0xD8C838, 0x807010, 0x181800],
        obj1: [0xF8C8F8, 0xC870C8, 0x683068, 0x180818],
    }),
    // 12 — GBC B+Down (teal/cyan)
    ("GBC Teal", DmgColorPalettes {
        bg:   [0xF0FFF0, 0x50E8A0, 0x007860, 0x001820],
        obj0: [0xF0FFF0, 0x50E8A0, 0x007860, 0x001820],
        obj1: [0xFFE8C8, 0xE0A050, 0x905820, 0x201008],
    }),
];

pub struct GameEntry {
    pub title_substr: &'static str,
    pub name:         &'static str,
    pub palette:      DmgColorPalettes,
}

/// Game-specific palettes matching the GBC boot ROM combination table.
pub const GAME_DB: &[GameEntry] = &[
    GameEntry { title_substr: "POKEMON RED",   name: "Pokémon Red",
        palette: DmgColorPalettes {
            bg:   [0xF8F8F8, 0xE09090, 0xA02010, 0x300000],
            obj0: [0xF8F8F8, 0xF8A050, 0xA04010, 0x200000],
            obj1: [0xF8F8F8, 0xA0C8F0, 0x2060C0, 0x000828],
        }},
    GameEntry { title_substr: "POKEMON BLUE",  name: "Pokémon Blue",
        palette: DmgColorPalettes {
            bg:   [0xF8F8F8, 0x80A8F0, 0x1840C8, 0x000010],
            obj0: [0xF8F8F8, 0xA0C8F0, 0x2060C0, 0x000828],
            obj1: [0xF8F8F8, 0xF8A050, 0xA04010, 0x200000],
        }},
    GameEntry { title_substr: "POKEMON YELLO", name: "Pokémon Yellow",
        palette: DmgColorPalettes {
            bg:   [0xF8F8F8, 0xF8D000, 0xD09000, 0x302000],
            obj0: [0xF8F8F8, 0xF8D000, 0xD09000, 0x302000],
            obj1: [0xFF0000, 0xD82000, 0xA00000, 0x300000],
        }},
    GameEntry { title_substr: "ZELDA",         name: "The Legend of Zelda",
        palette: DmgColorPalettes {
            bg:   [0xF8F8E0, 0xB8C858, 0x507818, 0x182808],
            obj0: [0xF8E888, 0xD8A000, 0x905000, 0x281800],
            obj1: [0xE8E8E8, 0xA8A0A8, 0x605860, 0x181018],
        }},
    GameEntry { title_substr: "TETRIS",        name: "Tetris",
        palette: DmgColorPalettes {
            bg:   [0xF8F8F8, 0x60A8D0, 0x1050A0, 0x000818],
            obj0: [0xF8F8F8, 0xF8A838, 0xD05000, 0x281800],
            obj1: [0xF8F8F8, 0x80C840, 0x308030, 0x081800],
        }},
    GameEntry { title_substr: "KIRBY",         name: "Kirby",
        palette: DmgColorPalettes {
            bg:   [0xF8D8E8, 0xF090B8, 0xB02878, 0x300010],
            obj0: [0xF8D8E8, 0xF090B8, 0xB02878, 0x300010],
            obj1: [0xF8F8A8, 0xD8D040, 0x808000, 0x181800],
        }},
    GameEntry { title_substr: "METROID",       name: "Metroid",
        palette: DmgColorPalettes {
            bg:   [0xF8F0C8, 0xC0A800, 0x785800, 0x201400],
            obj0: [0xF8F8A8, 0xB0D010, 0x507800, 0x101800],
            obj1: [0xF8A0A0, 0xD04848, 0x901010, 0x200000],
        }},
    GameEntry { title_substr: "MEGAMAN",       name: "Mega Man",
        palette: DmgColorPalettes {
            bg:   [0xF8F8F8, 0x70C8F8, 0x1870C8, 0x000828],
            obj0: [0xF8F8F8, 0x70C8F8, 0x1870C8, 0x000828],
            obj1: [0xF8D888, 0xE09030, 0x905008, 0x201000],
        }},
    GameEntry { title_substr: "DONKEY KONG",   name: "Donkey Kong",
        palette: DmgColorPalettes {
            bg:   [0xF8D888, 0xE09030, 0x905008, 0x201000],
            obj0: [0xF8D888, 0xE09030, 0x905008, 0x201000],
            obj1: [0xF8F8F8, 0xA0D878, 0x308848, 0x081808],
        }},
    GameEntry { title_substr: "MARIO",         name: "Super Mario",
        palette: DmgColorPalettes {
            bg:   [0xF8F8F8, 0xF87858, 0xC02020, 0x280000],
            obj0: [0xF8F8F8, 0xF87858, 0xC02020, 0x280000],
            obj1: [0xF8F8A8, 0xD8B000, 0x807000, 0x181800],
        }},
    GameEntry { title_substr: "CASTLEVANIA",   name: "Castlevania",
        palette: DmgColorPalettes {
            bg:   [0xE8E0D0, 0xA098A8, 0x585068, 0x100818],
            obj0: [0xE8C880, 0xC89830, 0x785008, 0x181000],
            obj1: [0xF8A0A0, 0xC84040, 0x780808, 0x180000],
        }},
    GameEntry { title_substr: "WARIO",         name: "Wario Land",
        palette: DmgColorPalettes {
            bg:   [0xF8F8A8, 0xD8B000, 0x807000, 0x181800],
            obj0: [0xF8F8A8, 0xD8B000, 0x807000, 0x181800],
            obj1: [0xF8A8F8, 0xC850C8, 0x701870, 0x180018],
        }},
];

/// Look up a game palette from the ROM title string.
pub fn detect_game_palette(title: &str) -> Option<DmgColorPalettes> {
    let upper = title.to_uppercase();
    GAME_DB.iter().find(|e| upper.contains(e.title_substr)).map(|e| e.palette.clone())
}

/// Look up a game display name from the ROM title string.
pub fn detect_game_name(title: &str) -> Option<&'static str> {
    let upper = title.to_uppercase();
    GAME_DB.iter().find(|e| upper.contains(e.title_substr)).map(|e| e.name)
}

/// Default palette used when no game-specific entry is found.
pub const DEFAULT_PALETTE: DmgColorPalettes = DmgColorPalettes {
    bg:   [0xFFFFFF, 0xAAAAAA, 0x555555, 0x000000],
    obj0: [0xFFFFFF, 0xAAAAAA, 0x555555, 0x000000],
    obj1: [0xFFFFFF, 0xAAAAAA, 0x555555, 0x000000],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manual_palette_count() {
        assert!(ALL_MANUAL_PALETTES.len() >= 10);
    }

    #[test]
    fn test_all_palettes_have_4_colors() {
        for (name, p) in ALL_MANUAL_PALETTES {
            assert_eq!(p.bg.len(),   4, "{}", name);
            assert_eq!(p.obj0.len(), 4, "{}", name);
            assert_eq!(p.obj1.len(), 4, "{}", name);
        }
    }

    #[test]
    fn test_detect_tetris() {
        assert!(detect_game_palette("TETRIS").is_some());
    }

    #[test]
    fn test_detect_pokemon_red() {
        assert!(detect_game_palette("POKEMON RED").is_some());
    }

    #[test]
    fn test_detect_unknown_returns_none() {
        assert!(detect_game_palette("ZZUNKNOWNZZ").is_none());
    }

    #[test]
    fn test_game_name_lookup() {
        assert_eq!(detect_game_name("TETRIS"), Some("Tetris"));
    }

    #[test]
    fn test_colors_are_valid_rgb888() {
        for (_, p) in ALL_MANUAL_PALETTES {
            for &c in p.bg.iter().chain(&p.obj0).chain(&p.obj1) {
                assert_eq!(c >> 24, 0, "high byte must be zero in 0x{:08X}", c);
            }
        }
    }
}