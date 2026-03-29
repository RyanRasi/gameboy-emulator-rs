//! Shared emulator state for the web server.
//!
//! `EmulatorState` is wrapped in `Arc<Mutex<>>` and shared across all
//! Axum route handlers. It owns the CPU (and therefore the PPU, timer,
//! joypad, and MMU) and exposes the minimum surface area needed by the
//! HTTP routes.

use gb_core::cartridge::Cartridge;
use gb_core::cpu::Cpu;
use gb_core::input::Button;
use gb_core::ppu::{FRAMEBUFFER_SIZE, LCDC_ADDR, BGP_ADDR};

/// Maximum ROM size accepted by the upload endpoint (8 MiB).
pub const MAX_ROM_BYTES: usize = 8 * 1024 * 1024;

/// Maximum BIOS size accepted by the upload endpoint (256 bytes).
pub const MAX_BIOS_BYTES: usize = 256;

/// T-cycles in one Game Boy frame (154 lines × 456 cycles).
pub const CYCLES_PER_FRAME: u64 = 70_224;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmulatorStatus {
    /// No ROM loaded yet.
    NoRom,
    /// ROM loaded, waiting for /start.
    Ready,
    /// Running — /frame will advance and return a framebuffer.
    Running,
}

pub struct EmulatorState {
    pub cpu:    Option<Cpu>,
    pub status: EmulatorStatus,
    /// Raw bytes of the last uploaded BIOS (optional).
    bios: Option<Vec<u8>>,
}

impl EmulatorState {
    pub fn new() -> Self {
        EmulatorState {
            cpu:    None,
            status: EmulatorStatus::NoRom,
            bios:   None,
        }
    }

    /// Store a BIOS image for use when the next ROM is started.
    pub fn upload_bios(&mut self, data: Vec<u8>) -> Result<(), String> {
        if data.len() != MAX_BIOS_BYTES {
            return Err(format!(
                "BIOS must be exactly {} bytes, got {}",
                MAX_BIOS_BYTES,
                data.len()
            ));
        }
        self.bios = Some(data);
        Ok(())
    }

    /// Load a ROM. Parses the cartridge header; sets status to Ready.
    pub fn upload_rom(&mut self, data: Vec<u8>) -> Result<String, String> {
        if data.len() > MAX_ROM_BYTES {
            return Err(format!("ROM too large: {} bytes (max {})", data.len(), MAX_ROM_BYTES));
        }
        let cart = Cartridge::load(data)?;
        let title = cart.header.title.clone();
        let mut cpu = Cpu::new();
        cpu.mmu.load_cartridge(cart);
        self.cpu    = Some(cpu);
        self.status = EmulatorStatus::Ready;
        Ok(title)
    }

    /// Start emulation. Applies BIOS if one was uploaded, sets hardware
    /// defaults (LCDC, BGP), and sets status to Running.
    pub fn start(&mut self) -> Result<(), String> {
        match self.status {
            EmulatorStatus::NoRom => return Err("No ROM loaded".into()),
            EmulatorStatus::Running => return Ok(()), // idempotent
            EmulatorStatus::Ready => {}
        }

        let cpu = self.cpu.as_mut().ok_or("CPU not initialised")?;

        if let Some(bios) = &self.bios {
            cpu.mmu.load_bios(bios).map_err(|e| e.to_string())?;
        } else {
            // No BIOS: replicate post-boot hardware state
            cpu.mmu.write_byte(LCDC_ADDR, 0x91);
            cpu.mmu.write_byte(BGP_ADDR,  0xE4);
        }

        self.status = EmulatorStatus::Running;
        Ok(())
    }

    /// Advance the emulator by exactly one frame.
    /// Returns a copy of the raw framebuffer (one byte per pixel, shade 0–3).
    pub fn run_frame(&mut self) -> Result<Vec<u8>, String> {
        if self.status != EmulatorStatus::Running {
            return Err("Emulator is not running".into());
        }
        let cpu = self.cpu.as_mut().ok_or("CPU not initialised")?;

        let budget = cpu.cycles + CYCLES_PER_FRAME * 2;
        while cpu.cycles < budget {
            cpu.tick();
            if cpu.ppu.frame_ready {
                cpu.ppu.frame_ready = false;
                break;
            }
        }

        Ok(cpu.ppu.framebuffer.to_vec())
    }

    /// Send a button press event to the joypad.
    pub fn press(&mut self, button: Button) -> Result<(), String> {
        if self.status != EmulatorStatus::Running {
            return Err("Emulator is not running".into());
        }
        self.cpu.as_mut().ok_or("CPU not initialised")?.button_press(button);
        Ok(())
    }

    /// Send a button release event to the joypad.
    pub fn release(&mut self, button: Button) -> Result<(), String> {
        if self.status != EmulatorStatus::Running {
            return Err("Emulator is not running".into());
        }
        self.cpu.as_mut().ok_or("CPU not initialised")?.button_release(button);
        Ok(())
    }
}

impl Default for EmulatorState {
    fn default() -> Self { Self::new() }
}

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

    // ── upload_bios ───────────────────────────────────────────────────────────

    #[test]
    fn test_upload_bios_correct_size_succeeds() {
        let mut state = EmulatorState::new();
        assert!(state.upload_bios(vec![0u8; 256]).is_ok());
    }

    #[test]
    fn test_upload_bios_wrong_size_fails() {
        let mut state = EmulatorState::new();
        assert!(state.upload_bios(vec![0u8; 512]).is_err());
    }

    #[test]
    fn test_upload_bios_empty_fails() {
        let mut state = EmulatorState::new();
        assert!(state.upload_bios(vec![]).is_err());
    }

    // ── upload_rom ────────────────────────────────────────────────────────────

    #[test]
    fn test_upload_rom_valid_sets_status_ready() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert_eq!(state.status, EmulatorStatus::Ready);
    }

    #[test]
    fn test_upload_rom_invalid_returns_error() {
        let mut state = EmulatorState::new();
        assert!(state.upload_rom(vec![0u8; 10]).is_err());
    }

    #[test]
    fn test_upload_rom_too_large_returns_error() {
        let mut state = EmulatorState::new();
        assert!(state.upload_rom(vec![0u8; MAX_ROM_BYTES + 1]).is_err());
    }

    #[test]
    fn test_upload_rom_returns_title() {
        let mut state = EmulatorState::new();
        let title = state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert!(title.len() <= 16); // DMG title is at most 16 chars
    }

    #[test]
    fn test_initial_status_is_no_rom() {
        assert_eq!(EmulatorState::new().status, EmulatorStatus::NoRom);
    }

    // ── start ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_start_without_rom_returns_error() {
        let mut state = EmulatorState::new();
        assert!(state.start().is_err());
    }

    #[test]
    fn test_start_after_rom_upload_sets_running() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        state.start().unwrap();
        assert_eq!(state.status, EmulatorStatus::Running);
    }

    #[test]
    fn test_start_is_idempotent() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        state.start().unwrap();
        assert!(state.start().is_ok());
        assert_eq!(state.status, EmulatorStatus::Running);
    }

    #[test]
    fn test_start_with_bios_succeeds() {
        let mut state = EmulatorState::new();
        state.upload_bios(vec![0u8; 256]).unwrap();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert!(state.start().is_ok());
    }

    // ── run_frame ─────────────────────────────────────────────────────────────

    #[test]
    fn test_run_frame_before_start_returns_error() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert!(state.run_frame().is_err());
    }

    #[test]
    fn test_run_frame_without_rom_returns_error() {
        let mut state = EmulatorState::new();
        assert!(state.run_frame().is_err());
    }

    #[test]
    fn test_run_frame_returns_framebuffer_of_correct_size() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        state.start().unwrap();
        let fb = state.run_frame().unwrap();
        assert_eq!(fb.len(), FRAMEBUFFER_SIZE);
    }

    #[test]
    fn test_run_frame_returns_valid_shades() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        state.start().unwrap();
        let fb = state.run_frame().unwrap();
        assert!(fb.iter().all(|&s| s <= 3), "All shades must be 0–3");
    }

    #[test]
    fn test_run_frame_advances_emulator_state() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        state.start().unwrap();
        state.run_frame().unwrap();
        let cycles_after_1 = state.cpu.as_ref().unwrap().cycles;
        state.run_frame().unwrap();
        let cycles_after_2 = state.cpu.as_ref().unwrap().cycles;
        assert!(cycles_after_2 > cycles_after_1, "Cycles must increase across frames");
    }

    #[test]
    fn test_multiple_frames_do_not_panic() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        state.start().unwrap();
        for _ in 0..10 {
            state.run_frame().unwrap();
        }
    }

    // ── input ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_press_before_start_returns_error() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        // ROM loaded but not started → should fail
        assert!(state.press(Button::A).is_err());
    }

    #[test]
    fn test_release_before_start_returns_error() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        assert!(state.release(Button::A).is_err());
    }

    #[test]
    fn test_press_after_start_succeeds() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        state.start().unwrap();
        assert!(state.press(Button::A).is_ok());
    }

    #[test]
    fn test_release_after_press_succeeds() {
        let mut state = EmulatorState::new();
        state.upload_rom(make_rom(0x00, 0x00, 0x00)).unwrap();
        state.start().unwrap();
        state.press(Button::Start).unwrap();
        assert!(state.release(Button::Start).is_ok());
    }
}