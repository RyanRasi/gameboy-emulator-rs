//! SM83 CPU — Game Boy processor core.

pub mod alu;
pub mod interrupts;
pub mod registers;
mod instructions;

pub use registers::Registers;

use crate::apu::Apu;
use crate::input::Joypad;
use crate::mmu::Mmu;
use crate::ppu::Ppu;
use crate::timer::Timer;

pub struct Cpu {
    pub regs:   Registers,
    pub mmu:    Mmu,
    pub timer:  Timer,
    pub ppu:    Ppu,
    pub joypad: Joypad,
    pub apu:    Apu,
    pub cycles: u64,
    pub ime:    bool,
    pub halted: bool,
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            regs:   Registers::new(),
            mmu:    Mmu::new(),
            timer:  Timer::new(),
            ppu:    Ppu::new(),
            joypad: Joypad::new(),
            apu:    Apu::new(),
            cycles: 0,
            ime:    false,
            halted: false,
        }
    }

    pub fn tick(&mut self) -> u32 {
        let irq_cycles = interrupts::service(
            &mut self.mmu,
            &mut self.ime,
            &mut self.halted,
            &mut self.regs.pc,
            &mut self.regs.sp,
        );

        if irq_cycles > 0 {
            self.cycles += irq_cycles as u64;
            self.step_peripherals(irq_cycles);
            return irq_cycles;
        }

        let instr_cycles = if self.halted { 4 } else { self.step() };
        self.cycles += instr_cycles as u64;
        self.step_peripherals(instr_cycles);
        instr_cycles
    }

    fn step_peripherals(&mut self, cycles: u32) {
        if self.timer.step(cycles, &mut self.mmu) {
            interrupts::request(&mut self.mmu, interrupts::source::TIMER);
        }
        let ppu_result = self.ppu.step(cycles, &mut self.mmu);
        if ppu_result.vblank_irq {
            interrupts::request(&mut self.mmu, interrupts::source::VBLANK);
        }
        if ppu_result.stat_irq {
            interrupts::request(&mut self.mmu, interrupts::source::LCD_STAT);
        }
        if self.joypad.sync(&mut self.mmu) {
            interrupts::request(&mut self.mmu, interrupts::source::JOYPAD);
        }
        self.apu.step(cycles, &mut self.mmu);
    }

    pub fn button_press(&mut self, button: crate::input::Button) {
        self.joypad.press(button);
    }

    pub fn button_release(&mut self, button: crate::input::Button) {
        self.joypad.release(button);
    }

    pub fn request_interrupt(&mut self, mask: u8) {
        interrupts::request(&mut self.mmu, mask);
    }
}

impl Default for Cpu {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests (regression only — APU unit tests live in apu/mod.rs)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::interrupts::{source, IF_ADDR, IE_ADDR};
    use crate::timer::{TAC_ADDR, TIMA_ADDR, TMA_ADDR};
    use crate::ppu;
    use crate::input::{Button, JOYP_ADDR};

    fn cpu_with_program(program: &[u8]) -> Cpu {
        let mut cpu = Cpu::new();
        for (i, &byte) in program.iter().enumerate() {
            cpu.mmu.write_byte(0xC000 + i as u16, byte);
        }
        cpu.regs.pc = 0xC000;
        cpu
    }

    fn cpu_with_nop_rom() -> Cpu {
        let mut cpu = Cpu::new();
        cpu.mmu.load_rom(&vec![0x00u8; 0x8000]).unwrap();
        cpu
    }

    // ── Phase regressions ─────────────────────────────────────────────────────

    #[test]
    fn test_nop_still_works() {
        let mut cpu = cpu_with_program(&[0x00]);
        let start = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, start + 1);
    }

    #[test]
    fn test_vblank_interrupt_still_works() {
        let mut cpu = cpu_with_program(&[0x00]);
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR, source::VBLANK);
        cpu.request_interrupt(source::VBLANK);
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0x0040);
    }

    #[test]
    fn test_timer_overflow_still_fires_irq() {
        let mut cpu = cpu_with_program(&[0x00u8; 16]);
        cpu.ime = true;
        cpu.mmu.write_byte(TAC_ADDR,  0x05);
        cpu.mmu.write_byte(TIMA_ADDR, 0xFF);
        cpu.mmu.write_byte(TMA_ADDR,  0x00);
        cpu.mmu.write_byte(IE_ADDR,   source::TIMER);
        for _ in 0..4 { cpu.tick(); }
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0x0050);
    }

    #[test]
    fn test_ppu_ly_advances_as_cpu_executes_nops() {
        let mut cpu = cpu_with_program(&[0x00u8; 128]);
        cpu.mmu.write_byte(ppu::LCDC_ADDR, 0x91);
        for _ in 0..114 { cpu.tick(); }
        assert_eq!(cpu.mmu.read_byte(ppu::LY_ADDR), 1);
    }

    #[test]
    fn test_button_press_updates_joyp_register_after_tick() {
        let mut cpu = cpu_with_program(&[0x00u8; 4]);
        cpu.mmu.write_byte(JOYP_ADDR, 0xDF);
        cpu.button_press(Button::A);
        cpu.tick();
        assert_eq!(cpu.mmu.read_byte(JOYP_ADDR) & 0x01, 0);
    }

    // ── APU integration ───────────────────────────────────────────────────────

    #[test]
    fn test_apu_step_called_in_tick_produces_samples() {
        let mut cpu = cpu_with_nop_rom();
        cpu.mmu.write_byte(crate::apu::NR52_ADDR, 0x80);
        // Run enough ticks to accumulate at least one sample
        for _ in 0..100 { cpu.tick(); }
        // After 100 NOPs (400 T-cycles), we expect at least 4 samples
        assert!(
            !cpu.apu.sample_buffer.is_empty(),
            "APU must accumulate samples during CPU execution"
        );
    }

    #[test]
    fn test_apu_produces_silence_when_no_channels_triggered() {
        let mut cpu = cpu_with_nop_rom();
        cpu.mmu.write_byte(crate::apu::NR52_ADDR, 0x80);
        for _ in 0..500 { cpu.tick(); }
        let samples = cpu.apu.drain_samples();
        assert!(
            samples.iter().all(|&s| s == 0.0),
            "No triggered channels → all silence"
        );
    }

    #[test]
    fn test_apu_ch2_audible_when_triggered_via_cpu_ticks() {
        let mut cpu = cpu_with_nop_rom();
        cpu.mmu.write_byte(crate::apu::NR52_ADDR, 0x80);
        cpu.mmu.write_byte(crate::apu::NR50_ADDR, 0x77);
        cpu.mmu.write_byte(crate::apu::NR51_ADDR, 0xFF);
        cpu.mmu.write_byte(crate::apu::NR22_ADDR, 0xF8); // vol=15, dac on
        cpu.mmu.write_byte(crate::apu::NR21_ADDR, 0x80);
        cpu.mmu.write_byte(crate::apu::NR23_ADDR, 0x00);
        cpu.mmu.write_byte(crate::apu::NR24_ADDR, 0xC7); // trigger
        for _ in 0..500 { cpu.tick(); }
        let samples = cpu.apu.drain_samples();
        assert!(
            samples.iter().any(|&s| s.abs() > 0.01),
            "Triggered CH2 must produce audible samples during CPU ticks"
        );
    }
}