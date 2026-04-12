//! Shared emulator state for the web server.

use gb_core::cartridge::Cartridge;
use gb_core::cpu::Cpu;
use gb_core::gbc_bios_palettes::apply_bios_palettes;
use gb_core::input::Button;
use gb_core::ppu::palettes::{ALL_MANUAL_PALETTES, DEFAULT_PALETTE, detect_game_palette, detect_game_name};
use gb_core::ppu::{BGP_ADDR, FRAMEBUFFER_SIZE, LCDC_ADDR};

pub const MAX_ROM_BYTES:  usize = 8 * 1024 * 1024;
pub const MAX_BIOS_BYTES: usize = 256;
pub const CYCLES_PER_FRAME: u64 = 70_224;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmulatorStatus {
    NoRom,
    Ready,
    Running,
}

pub struct EmulatorState {
    pub cpu:         Option<Cpu>,
    pub status:      EmulatorStatus,
    bios:            Option<Vec<u8>>,
    rom_data:        Option<Vec<u8>>,   // kept for palette re-application
    pub is_cgb:      bool,
    pub game_title:  String,
    /// None = game/native palette, Some(i) = manual palette index
    pub palette_idx: Option<usize>,
}

impl EmulatorState {
    pub fn new() -> Self {
        EmulatorState {
            cpu:         None,
            status:      EmulatorStatus::NoRom,
            bios:        None,
            rom_data:    None,
            is_cgb:      false,
            game_title:  String::new(),
            palette_idx: None,
        }
    }

    pub fn upload_bios(&mut self, data: Vec<u8>) -> Result<(), String> {
        if data.len() != MAX_BIOS_BYTES {
            return Err(format!(
                "BIOS must be exactly {} bytes, got {}",
                MAX_BIOS_BYTES, data.len()
            ));
        }
        self.bios = Some(data);
        Ok(())
    }

    pub fn upload_rom(&mut self, data: Vec<u8>) -> Result<String, String> {
        if data.len() > MAX_ROM_BYTES {
            return Err(format!("ROM too large: {} bytes (max {})", data.len(), MAX_ROM_BYTES));
        }

        let cart = Cartridge::load(data.clone())?;
        let title   = cart.header.title.clone();
        let is_cgb  = cart.header.is_cgb();

        let display_name = detect_game_name(&title)
            .map(|s| s.to_string())
            .unwrap_or_else(|| if title.is_empty() { "Game Boy".into() } else { title.clone() });

        let mut cpu = Cpu::new();
        cpu.ppu.cgb_mode = is_cgb;
        cpu.mmu.load_cartridge(cart);

        // Apply colour palettes for DMG games
        if !is_cgb {
            apply_bios_palettes(&data, &mut cpu.mmu.bg_palette, &mut cpu.mmu.obj_palette);
            cpu.ppu.dmg_palettes = detect_game_palette(&title)
                .unwrap_or_else(|| DEFAULT_PALETTE.clone());
        }

        // Set CGB post-boot registers
        if is_cgb {
            cpu.regs.a  = 0x11;
            cpu.regs.f  = 0x80;
            cpu.regs.b  = 0x00;
            cpu.regs.c  = 0x00;
            cpu.regs.d  = 0xFF;
            cpu.regs.e  = 0x56;
            cpu.regs.h  = 0x00;
            cpu.regs.l  = 0x0D;
            cpu.regs.sp = 0xFFFE;
            cpu.regs.pc = 0x0100;
        }

        self.cpu         = Some(cpu);
        self.status      = EmulatorStatus::Ready;
        self.is_cgb      = is_cgb;
        self.game_title  = display_name.clone();
        self.rom_data    = Some(data);
        self.palette_idx = None;

        Ok(display_name)
    }

    pub fn start(&mut self) -> Result<(), String> {
        match self.status {
            EmulatorStatus::NoRom   => return Err("No ROM loaded".into()),
            EmulatorStatus::Running => return Ok(()),
            EmulatorStatus::Ready   => {}
        }

        let cpu = self.cpu.as_mut().ok_or("CPU not initialised")?;

        if let Some(ref bios) = self.bios {
            cpu.mmu.load_bios(bios).map_err(|e| e)?;
        }

        cpu.mmu.write_byte(LCDC_ADDR, 0x91);
        cpu.mmu.write_byte(BGP_ADDR,  0xE4);

        self.status = EmulatorStatus::Running;

        // Warm-up: discard first partial frame
        self.run_frame_inner();

        Ok(())
    }

    fn run_frame_inner(&mut self) {
        if let Some(cpu) = self.cpu.as_mut() {
            let limit = cpu.cycles + CYCLES_PER_FRAME + 456;
            loop {
                cpu.tick();
                if cpu.ppu.frame_ready {
                    cpu.ppu.frame_ready = false;
                    break;
                }
                if cpu.cycles >= limit { break; }
            }
        }
    }

    pub fn run_frame(&mut self) -> Result<Vec<u8>, String> {
        if self.status != EmulatorStatus::Running {
            return Err("Emulator is not running".into());
        }
        self.run_frame_inner();
        let cpu = self.cpu.as_ref().ok_or("CPU not initialised")?;
        Ok(cpu.ppu.framebuffer.iter().flat_map(|&px| {
            [((px >> 16) & 0xFF) as u8, ((px >> 8) & 0xFF) as u8, (px & 0xFF) as u8]
        }).collect())
    }

    // ── Palette cycling ───────────────────────────────────────────────────────

    pub fn palette_next(&mut self) {
        let n = ALL_MANUAL_PALETTES.len();
        self.palette_idx = Some(match self.palette_idx {
            None    => 0,
            Some(i) => (i + 1) % n,
        });
        self.apply_current_palette();
    }

    pub fn palette_prev(&mut self) {
        let n = ALL_MANUAL_PALETTES.len();
        self.palette_idx = Some(match self.palette_idx {
            None    => n - 1,
            Some(0) => n - 1,
            Some(i) => i - 1,
        });
        self.apply_current_palette();
    }

    pub fn palette_game(&mut self) {
        self.palette_idx = None;
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.ppu.force_dmg_palette = false;
            if !self.is_cgb {
                if let Some(ref data) = self.rom_data {
                    apply_bios_palettes(data, &mut cpu.mmu.bg_palette, &mut cpu.mmu.obj_palette);
                }
                // re-detect from stored title
                cpu.ppu.dmg_palettes = detect_game_palette(&self.game_title)
                    .unwrap_or_else(|| DEFAULT_PALETTE.clone());
            }
        }
    }

    pub fn palette_name(&self) -> String {
        match self.palette_idx {
            None    => if self.is_cgb { "Native".into() } else { "Game Palette".into() },
            Some(i) => ALL_MANUAL_PALETTES[i].0.to_string(),
        }
    }

    pub fn palette_index(&self) -> Option<usize> { self.palette_idx }

    pub fn palette_count(&self) -> usize { ALL_MANUAL_PALETTES.len() }

    fn apply_current_palette(&mut self) {
        if let Some(idx) = self.palette_idx {
            if let Some(cpu) = self.cpu.as_mut() {
                cpu.ppu.dmg_palettes      = ALL_MANUAL_PALETTES[idx].1.clone();
                cpu.ppu.force_dmg_palette = true;
            }
        }
    }

    // ── Input ─────────────────────────────────────────────────────────────────

    pub fn press(&mut self, button: Button) -> Result<(), String> {
        if self.status != EmulatorStatus::Running {
            return Err("Emulator is not running".into());
        }
        self.cpu.as_mut().ok_or("CPU not initialised")?.button_press(button);
        Ok(())
    }

    pub fn release(&mut self, button: Button) -> Result<(), String> {
        if self.status != EmulatorStatus::Running {
            return Err("Emulator is not running".into());
        }
        self.cpu.as_mut().ok_or("CPU not initialised")?.button_release(button);
        Ok(())
    }
}

impl Default for EmulatorState { fn default() -> Self { Self::new() } }

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rom(cart_type: u8, rom_code: u8, ram_code: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = cart_type;
        rom[0x0148] = rom_code;
        rom[0x0149] = ram_code;
        let cs = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = cs;
        rom
    }

    #[test]
    fn test_upload_bios_correct_size_succeeds() {
        let mut s = EmulatorState::new();
        assert!(s.upload_bios(vec![0u8; 256]).is_ok());
    }

    #[test]
    fn test_upload_bios_wrong_size_fails() {
        let mut s = EmulatorState::new();
        assert!(s.upload_bios(vec![0u8; 512]).is_err());
    }

    #[test]
    fn test_upload_bios_empty_fails() {
        let mut s = EmulatorState::new();
        assert!(s.upload_bios(vec![]).is_err());
    }

    #[test]
    fn test_upload_rom_valid_sets_status_ready() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert_eq!(s.status, EmulatorStatus::Ready);
    }

    #[test]
    fn test_upload_rom_invalid_returns_error() {
        let mut s = EmulatorState::new();
        assert!(s.upload_rom(vec![0u8; 10]).is_err());
    }

    #[test]
    fn test_upload_rom_too_large_returns_error() {
        let mut s = EmulatorState::new();
        assert!(s.upload_rom(vec![0u8; MAX_ROM_BYTES + 1]).is_err());
    }

    #[test]
    fn test_upload_rom_returns_title() {
        let mut s = EmulatorState::new();
        let title = s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert!(title.len() <= 16);
    }

    #[test]
    fn test_initial_status_is_no_rom() {
        assert_eq!(EmulatorState::new().status, EmulatorStatus::NoRom);
    }

    #[test]
    fn test_start_without_rom_returns_error() {
        let mut s = EmulatorState::new();
        assert!(s.start().is_err());
    }

    #[test]
    fn test_start_after_rom_upload_sets_running() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        assert_eq!(s.status, EmulatorStatus::Running);
    }

    #[test]
    fn test_start_is_idempotent() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        assert!(s.start().is_ok());
        assert_eq!(s.status, EmulatorStatus::Running);
    }

    #[test]
    fn test_start_with_bios_succeeds() {
        let mut s = EmulatorState::new();
        s.upload_bios(vec![0u8; 256]).unwrap();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert!(s.start().is_ok());
    }

    #[test]
    fn test_run_frame_before_start_returns_error() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert!(s.run_frame().is_err());
    }

    #[test]
    fn test_run_frame_without_rom_returns_error() {
        let mut s = EmulatorState::new();
        assert!(s.run_frame().is_err());
    }

    #[test]
    fn test_run_frame_returns_framebuffer_of_correct_size() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        let fb = s.run_frame().unwrap();
        assert_eq!(fb.len(), FRAMEBUFFER_SIZE * 3);
    }

    #[test]
    fn test_run_frame_returns_valid_rgb24() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        let fb = s.run_frame().unwrap();
        assert_eq!(fb.len(), FRAMEBUFFER_SIZE * 3);
    }

    #[test]
    fn test_run_frame_advances_emulator_state() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        s.run_frame().unwrap();
        let c1 = s.cpu.as_ref().unwrap().cycles;
        s.run_frame().unwrap();
        let c2 = s.cpu.as_ref().unwrap().cycles;
        assert!(c2 > c1);
    }

    #[test]
    fn test_multiple_frames_do_not_panic() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        for _ in 0..10 { s.run_frame().unwrap(); }
    }

    #[test]
    fn test_press_before_start_returns_error() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert!(s.press(Button::A).is_err());
    }

    #[test]
    fn test_release_before_start_returns_error() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert!(s.release(Button::A).is_err());
    }

    #[test]
    fn test_press_after_start_succeeds() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        assert!(s.press(Button::A).is_ok());
    }

    #[test]
    fn test_release_after_press_succeeds() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        s.press(Button::Start).unwrap();
        assert!(s.release(Button::Start).is_ok());
    }

    #[test]
    fn test_palette_next_cycles_through() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        s.palette_next();
        assert_eq!(s.palette_idx, Some(0));
        s.palette_next();
        assert_eq!(s.palette_idx, Some(1));
    }

    #[test]
    fn test_palette_prev_wraps() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        s.palette_prev();
        assert_eq!(s.palette_idx, Some(ALL_MANUAL_PALETTES.len() - 1));
    }

    #[test]
    fn test_palette_game_resets_to_none() {
        let mut s = EmulatorState::new();
        s.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        s.start().unwrap();
        s.palette_next();
        s.palette_game();
        assert_eq!(s.palette_idx, None);
    }

    #[test]
    fn test_palette_name_game() {
        let s = EmulatorState::new();
        let name = s.palette_name();
        assert!(!name.is_empty());
    }
}