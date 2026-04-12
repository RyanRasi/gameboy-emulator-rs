//! Save state — VERSION 2 (CGB: double VRAM banks, 8 WRAM banks, CGB flags).

use serde::{Deserialize, Serialize};

use crate::apu::Apu;
use crate::cartridge::CartridgeState;
use crate::cpu::Cpu;
use crate::cpu::registers::Registers;
use crate::input::Joypad;
use crate::mmu::Mmu;
use crate::ppu::Ppu;
use crate::timer::Timer;

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

#[derive(Serialize, Deserialize)]
pub struct MmuState {
    bios:         Vec<u8>,
    bios_active:  bool,
    vram:         Vec<u8>,  // 16 KB (2 banks flattened)
    vram_bank:    u8,
    wram:         Vec<u8>,  // 32 KB (8 banks flattened)
    wram_bank:    u8,
    oam:          Vec<u8>,
    io:           Vec<u8>,
    hram:         Vec<u8>,
    ie:           u8,
    cgb_mode:     bool,
    double_speed: bool,
}

impl SaveState {
    pub const VERSION: u32 = 2;

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

    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(self).map_err(|e| format!("Serialize error: {}", e))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        bincode::deserialize(data).map_err(|e| format!("Deserialize error: {}", e))
    }
}

impl MmuState {
    fn from_mmu(mmu: &Mmu) -> Self {
        MmuState {
            bios:         mmu.bios.to_vec(),
            bios_active:  mmu.bios_active,
            vram:         mmu.vram.clone(),
            vram_bank:    mmu.vram_bank,
            wram:         mmu.wram.clone(),
            wram_bank:    mmu.wram_bank,
            oam:          mmu.oam.to_vec(),
            io:           mmu.io.to_vec(),
            hram:         mmu.hram.to_vec(),
            ie:           mmu.ie,
            cgb_mode:     mmu.cgb_mode,
            double_speed: mmu.double_speed,
        }
    }

    fn restore_into_mmu(self, mmu: &mut Mmu) {
        if self.bios.len() == mmu.bios.len() { mmu.bios.copy_from_slice(&self.bios); }
        mmu.bios_active  = self.bios_active;
        mmu.vram         = self.vram;
        mmu.vram_bank    = self.vram_bank;
        mmu.wram         = self.wram;
        mmu.wram_bank    = self.wram_bank;
        if self.oam.len()  == mmu.oam.len()  { mmu.oam.copy_from_slice(&self.oam); }
        if self.io.len()   == mmu.io.len()   { mmu.io.copy_from_slice(&self.io); }
        if self.hram.len() == mmu.hram.len() { mmu.hram.copy_from_slice(&self.hram); }
        mmu.ie           = self.ie;
        mmu.cgb_mode     = self.cgb_mode;
        mmu.double_speed = self.double_speed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::Cpu;
    use crate::ppu::{LCDC_ADDR, BGP_ADDR};

    fn running_cpu() -> Cpu {
        let mut cpu = Cpu::new();
        cpu.mmu.write_byte(LCDC_ADDR, 0x91);
        cpu.mmu.write_byte(BGP_ADDR,  0xE4);
        for _ in 0..1000 { cpu.tick(); }
        cpu
    }

    #[test]
    fn test_registers_survive_roundtrip() {
        let mut cpu = running_cpu();
        cpu.regs.a  = 0x42;
        cpu.regs.pc = 0xC123;
        let bytes = SaveState::capture(&cpu).to_bytes().unwrap();
        let mut cpu2 = Cpu::new();
        SaveState::from_bytes(&bytes).unwrap().restore(&mut cpu2).unwrap();
        assert_eq!(cpu2.regs.a,  0x42);
        assert_eq!(cpu2.regs.pc, 0xC123);
    }

    #[test]
    fn test_wram_survives_roundtrip() {
        let mut cpu = running_cpu();
        cpu.mmu.write_byte(0xC100, 0xAB);
        let bytes = SaveState::capture(&cpu).to_bytes().unwrap();
        let mut cpu2 = Cpu::new();
        SaveState::from_bytes(&bytes).unwrap().restore(&mut cpu2).unwrap();
        assert_eq!(cpu2.mmu.read_byte(0xC100), 0xAB);
    }

    #[test]
    fn test_vram_survives_roundtrip() {
        let mut cpu = running_cpu();
        cpu.mmu.write_byte(0x8000, 0x55);
        let bytes = SaveState::capture(&cpu).to_bytes().unwrap();
        let mut cpu2 = Cpu::new();
        SaveState::from_bytes(&bytes).unwrap().restore(&mut cpu2).unwrap();
        assert_eq!(cpu2.mmu.read_byte(0x8000), 0x55);
    }

    #[test]
    fn test_cgb_mode_survives_roundtrip() {
        let mut cpu = running_cpu();
        cpu.mmu.cgb_mode = true;
        let bytes = SaveState::capture(&cpu).to_bytes().unwrap();
        let mut cpu2 = Cpu::new();
        SaveState::from_bytes(&bytes).unwrap().restore(&mut cpu2).unwrap();
        assert!(cpu2.mmu.cgb_mode);
    }

    #[test]
    fn test_version_mismatch_returns_error() {
        let cpu = running_cpu();
        let mut state = SaveState::capture(&cpu);
        state.version = 99;
        let bytes = state.to_bytes().unwrap();
        let mut cpu2 = Cpu::new();
        assert!(SaveState::from_bytes(&bytes).unwrap().restore(&mut cpu2).is_err());
    }

    #[test]
    fn test_size_is_reasonable() {
        let cpu = running_cpu();
        let bytes = SaveState::capture(&cpu).to_bytes().unwrap();
        assert!(bytes.len() > 1024);
        assert!(bytes.len() < 2 * 1024 * 1024);
    }

    #[test]
    fn test_corrupted_bytes_returns_error() {
        assert!(SaveState::from_bytes(&[0xFF; 64]).is_err());
    }

    #[test]
    fn test_no_cartridge_succeeds() {
        let cpu = running_cpu();
        let bytes = SaveState::capture(&cpu).to_bytes().unwrap();
        let mut cpu2 = Cpu::new();
        assert!(SaveState::from_bytes(&bytes).unwrap().restore(&mut cpu2).is_ok());
    }
}