//! Save state — serialise and restore the complete emulator state.
//!
//! ROM data is never included in the snapshot. The cartridge must be
//! reloaded from its original file before `restore()` is called.
//!
//! Format: bincode-encoded `SaveState` struct (compact binary, ~50 KiB).

use serde::{Deserialize, Serialize};

use crate::apu::Apu;
use crate::cartridge::CartridgeState;
use crate::cpu::Cpu;
use crate::cpu::registers::Registers;
use crate::input::Joypad;
use crate::mmu::Mmu;
use crate::ppu::Ppu;
use crate::timer::Timer;

/// Complete snapshot of emulator state (ROM excluded).
#[derive(Serialize, Deserialize)]
pub struct SaveState {
    pub version:   u32,
    pub regs:      Registers,
    pub mmu:       MmuState,
    pub ppu:       Ppu,
    pub timer:     Timer,
    pub joypad:    Joypad,
    pub apu:       Apu,
    pub cycles:    u64,
    pub ime:       bool,
    pub halted:    bool,
    pub cartridge: Option<CartridgeState>,
}

/// Serializable MMU snapshot (cartridge and bare ROM excluded).
#[derive(Serialize, Deserialize)]
pub struct MmuState {
    bios:        Vec<u8>,
    bios_active: bool,
    vram:        Vec<u8>,
    wram:        Vec<u8>,
    oam:         Vec<u8>,
    io:          Vec<u8>,
    hram:        Vec<u8>,
    ie:          u8,
}

impl SaveState {
    /// Current save state format version.
    pub const VERSION: u32 = 1;

    /// Capture a snapshot from the running CPU.
    pub fn capture(cpu: &Cpu) -> Self {
        let cart_state = cpu.mmu.cartridge.as_ref().map(|c| c.save_state());

        SaveState {
            version:   Self::VERSION,
            regs:      cpu.regs.clone(),
            mmu:       MmuState::from_mmu(&cpu.mmu),
            ppu:       cpu.ppu.clone(),
            timer:     cpu.timer.clone(),
            joypad:    cpu.joypad.clone(),
            apu:       cpu.apu.clone(),
            cycles:    cpu.cycles,
            ime:       cpu.ime,
            halted:    cpu.halted,
            cartridge: cart_state,
        }
    }

    /// Restore state into a CPU that already has its cartridge loaded.
    pub fn restore(self, cpu: &mut Cpu) -> Result<(), String> {
        if self.version != Self::VERSION {
            return Err(format!(
                "Save state version mismatch: got {}, expected {}",
                self.version, Self::VERSION
            ));
        }

        cpu.regs   = self.regs;
        cpu.cycles = self.cycles;
        cpu.ime    = self.ime;
        cpu.halted = self.halted;
        cpu.ppu    = self.ppu;
        cpu.timer  = self.timer;
        cpu.joypad = self.joypad;
        cpu.apu    = self.apu;
        // Restore APU sample buffer (cleared by serde skip)
        cpu.apu.sample_buffer = Vec::new();

        MmuState::restore_into_mmu(self.mmu, &mut cpu.mmu);

        if let Some(cart_state) = self.cartridge {
            if let Some(ref mut cart) = cpu.mmu.cartridge {
                cart.load_state(cart_state)?;
            } else {
                return Err("No cartridge loaded — cannot restore cartridge state".into());
            }
        }

        Ok(())
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(self).map_err(|e| format!("Serialize error: {}", e))
    }

    /// Deserialize from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        bincode::deserialize(data).map_err(|e| format!("Deserialize error: {}", e))
    }
}

impl MmuState {
    fn from_mmu(mmu: &Mmu) -> Self {
        MmuState {
            bios:        mmu.bios.to_vec(),
            bios_active: mmu.bios_active,
            vram:        mmu.vram.to_vec(),
            wram:        mmu.wram.to_vec(),
            oam:         mmu.oam.to_vec(),
            io:          mmu.io.to_vec(),
            hram:        mmu.hram.to_vec(),
            ie:          mmu.ie,
        }
    }

    fn restore_into_mmu(self, mmu: &mut Mmu) {
            if self.bios.len() == mmu.bios.len() {
                mmu.bios.copy_from_slice(&self.bios);
            }
            mmu.bios_active = self.bios_active;
            if self.vram.len() == mmu.vram.len() { mmu.vram.copy_from_slice(&self.vram); }
            if self.wram.len() == mmu.wram.len() { mmu.wram.copy_from_slice(&self.wram); }
            if self.oam.len()  == mmu.oam.len()  { mmu.oam.copy_from_slice(&self.oam);  }
            if self.io.len()   == mmu.io.len()   { mmu.io.copy_from_slice(&self.io);    }
            if self.hram.len() == mmu.hram.len() { mmu.hram.copy_from_slice(&self.hram);}
            mmu.ie = self.ie;
        }
    }

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::Cpu;
    use crate::ppu::{LCDC_ADDR, BGP_ADDR};

    fn running_cpu() -> Cpu {
        let mut cpu = Cpu::new();
        cpu.mmu.write_byte(LCDC_ADDR, 0x91);
        cpu.mmu.write_byte(BGP_ADDR,  0xE4);
        // Run a few ticks to get non-trivial state
        for _ in 0..1000 { cpu.tick(); }
        cpu
    }

    fn make_cart_rom(cart_type: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = cart_type;
        rom[0x0148] = 0x01;
        rom[0x0149] = 0x00;
        let cs = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = cs;
        rom
    }

    // ── Round-trip serialization ──────────────────────────────────────────────

    #[test]
    fn test_capture_and_restore_preserves_registers() {
        let mut cpu = running_cpu();
        cpu.regs.a  = 0x42;
        cpu.regs.pc = 0xC123;
        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        let loaded = SaveState::from_bytes(&bytes).unwrap();
        let mut cpu2 = Cpu::new();
        loaded.restore(&mut cpu2).unwrap();
        assert_eq!(cpu2.regs.a,  0x42);
        assert_eq!(cpu2.regs.pc, 0xC123);
    }

    #[test]
    fn test_capture_and_restore_preserves_wram() {
        let mut cpu = running_cpu();
        cpu.mmu.write_byte(0xC100, 0xAB);
        cpu.mmu.write_byte(0xC101, 0xCD);
        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        let loaded = SaveState::from_bytes(&bytes).unwrap();
        let mut cpu2 = Cpu::new();
        loaded.restore(&mut cpu2).unwrap();
        assert_eq!(cpu2.mmu.read_byte(0xC100), 0xAB);
        assert_eq!(cpu2.mmu.read_byte(0xC101), 0xCD);
    }

    #[test]
    fn test_capture_and_restore_preserves_vram() {
        let mut cpu = running_cpu();
        cpu.mmu.write_byte(0x8000, 0x55);
        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        let loaded = SaveState::from_bytes(&bytes).unwrap();
        let mut cpu2 = Cpu::new();
        loaded.restore(&mut cpu2).unwrap();
        assert_eq!(cpu2.mmu.read_byte(0x8000), 0x55);
    }

    #[test]
    fn test_capture_and_restore_preserves_cycles() {
        let cpu = running_cpu();
        let cycles_before = cpu.cycles;
        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        let loaded = SaveState::from_bytes(&bytes).unwrap();
        let mut cpu2 = Cpu::new();
        loaded.restore(&mut cpu2).unwrap();
        assert_eq!(cpu2.cycles, cycles_before);
    }

    #[test]
    fn test_capture_and_restore_preserves_ime() {
        let mut cpu = running_cpu();
        cpu.ime = true;
        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        let loaded = SaveState::from_bytes(&bytes).unwrap();
        let mut cpu2 = Cpu::new();
        loaded.restore(&mut cpu2).unwrap();
        assert!(cpu2.ime);
    }

    #[test]
    fn test_capture_and_restore_preserves_ppu_framebuffer() {
        let mut cpu = running_cpu();
        cpu.ppu.framebuffer[0]   = 3;
        cpu.ppu.framebuffer[100] = 2;
        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        let loaded = SaveState::from_bytes(&bytes).unwrap();
        let mut cpu2 = Cpu::new();
        loaded.restore(&mut cpu2).unwrap();
        assert_eq!(cpu2.ppu.framebuffer[0],   3);
        assert_eq!(cpu2.ppu.framebuffer[100], 2);
    }

    #[test]
    fn test_capture_and_restore_preserves_io_registers() {
        let mut cpu = running_cpu();
        cpu.mmu.write_byte(LCDC_ADDR, 0x91);
        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        let loaded = SaveState::from_bytes(&bytes).unwrap();
        let mut cpu2 = Cpu::new();
        loaded.restore(&mut cpu2).unwrap();
        assert_eq!(cpu2.mmu.read_byte(LCDC_ADDR), 0x91);
    }

    // ── Cartridge state ───────────────────────────────────────────────────────

    #[test]
    fn test_capture_and_restore_preserves_cartridge_ram() {
        use crate::cartridge::Cartridge;
        use crate::cartridge::mbc_type;

        let rom = make_cart_rom(mbc_type::MBC1_RAM);
        let cart = Cartridge::load(rom.clone()).unwrap();
        let mut cpu = Cpu::new();
        cpu.mmu.load_cartridge(cart);

        // Enable RAM and write a value
        cpu.mmu.write_byte(0x0000, 0x0A);
        cpu.mmu.write_byte(0xA000, 0x77);

        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        let loaded = SaveState::from_bytes(&bytes).unwrap();

        // Restore into a fresh CPU with the same cartridge
        let cart2 = Cartridge::load(rom).unwrap();
        let mut cpu2 = Cpu::new();
        cpu2.mmu.load_cartridge(cart2);
        loaded.restore(&mut cpu2).unwrap();

        assert_eq!(cpu2.mmu.read_byte(0xA000), 0x77,
            "Cartridge RAM must survive save/restore cycle");
    }

    #[test]
    fn test_save_state_without_cartridge_succeeds() {
        let cpu = running_cpu(); // no cartridge loaded
        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        let loaded = SaveState::from_bytes(&bytes).unwrap();
        let mut cpu2 = Cpu::new();
        assert!(loaded.restore(&mut cpu2).is_ok());
    }

    #[test]
    fn test_version_mismatch_returns_error() {
        let cpu = running_cpu();
        let mut state = SaveState::capture(&cpu);
        state.version = 99; // corrupt version
        let bytes = state.to_bytes().unwrap();
        let loaded = SaveState::from_bytes(&bytes).unwrap();
        let mut cpu2 = Cpu::new();
        assert!(loaded.restore(&mut cpu2).is_err());
    }

    // ── Serialization properties ──────────────────────────────────────────────

    #[test]
    fn test_to_bytes_produces_nonempty_output() {
        let cpu = running_cpu();
        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_from_bytes_roundtrip_preserves_all_data() {
        let cpu = running_cpu();
        let state1 = SaveState::capture(&cpu);
        let bytes  = state1.to_bytes().unwrap();
        let state2 = SaveState::from_bytes(&bytes).unwrap();
        // Verify key fields survived
        let mut cpu1 = Cpu::new();
        let mut cpu2 = Cpu::new();
        SaveState::capture(&cpu).restore(&mut cpu1).unwrap();
        state2.restore(&mut cpu2).unwrap();
        assert_eq!(cpu1.regs, cpu2.regs);
        assert_eq!(cpu1.cycles, cpu2.cycles);
    }

    #[test]
    fn test_save_state_size_is_reasonable() {
        let cpu = running_cpu();
        let state = SaveState::capture(&cpu);
        let bytes = state.to_bytes().unwrap();
        // Should be well under 1 MiB
        assert!(
            bytes.len() < 1024 * 1024,
            "Save state too large: {} bytes",
            bytes.len()
        );
        // Should be at least a few KiB (contains WRAM, VRAM etc.)
        assert!(
            bytes.len() > 1024,
            "Save state suspiciously small: {} bytes",
            bytes.len()
        );
    }

    #[test]
    fn test_multiple_save_states_are_independent() {
        let mut cpu = running_cpu();

        // Save state 1 with A=0x11
        cpu.regs.a = 0x11;
        let bytes1 = SaveState::capture(&cpu).to_bytes().unwrap();

        // Save state 2 with A=0x22
        cpu.regs.a = 0x22;
        let bytes2 = SaveState::capture(&cpu).to_bytes().unwrap();

        // Restore state 1 — A must be 0x11
        let mut cpu1 = Cpu::new();
        SaveState::from_bytes(&bytes1).unwrap().restore(&mut cpu1).unwrap();
        assert_eq!(cpu1.regs.a, 0x11);

        // Restore state 2 — A must be 0x22
        let mut cpu2 = Cpu::new();
        SaveState::from_bytes(&bytes2).unwrap().restore(&mut cpu2).unwrap();
        assert_eq!(cpu2.regs.a, 0x22);
    }

    #[test]
    fn test_corrupted_bytes_returns_error() {
        let data = vec![0xFFu8; 64]; // garbage
        assert!(SaveState::from_bytes(&data).is_err());
    }
}