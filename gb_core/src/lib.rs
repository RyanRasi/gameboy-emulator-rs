//! Game Boy DMG/CGB emulator core — pure logic, zero I/O dependencies.

pub mod apu;
pub mod cartridge;
pub mod cpu;
pub mod gbc_bios_palettes;
pub mod input;
pub mod mmu;
pub mod ppu;
pub mod save_state;
pub mod serial;
pub mod timer;

pub fn version() -> &'static str { "gb-emulator-core 0.2.0" }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_version() { assert!(version().contains("core")); }
}