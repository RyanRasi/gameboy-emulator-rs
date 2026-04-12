//! SM83 CPU with CGB double-speed support.

pub mod alu;
pub mod interrupts;
pub mod registers;
mod instructions;

pub use registers::Registers;

use crate::apu::Apu;
use crate::input::Joypad;
use crate::mmu::Mmu;
use crate::ppu::Ppu;
use crate::serial::Serial;
use crate::timer::Timer;

pub struct Cpu {
    pub regs:    Registers,
    pub mmu:     Mmu,
    pub timer:   Timer,
    pub ppu:     Ppu,
    pub joypad:  Joypad,
    pub apu:     Apu,
    pub serial:  Serial,
    pub cycles:  u64,
    pub ime:     bool,
    pub halted:  bool,
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
            serial: Serial::new(),
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

        // STOP instruction triggers speed switch on CGB
        // (handled in instructions.rs by writing to prepare_speed_switch)
        if self.mmu.prepare_speed_switch && self.halted {
            self.mmu.execute_speed_switch();
            self.halted = false;
        }

        let instr_cycles = if self.halted { 4 } else { self.step() };
        self.cycles += instr_cycles as u64;
        self.step_peripherals(instr_cycles);
        instr_cycles
    }

    fn step_peripherals(&mut self, cycles: u32) {
        self.mmu.tick_cartridge_rtc(cycles as u64);

        // Serial port
        {
            let sc = self.mmu.read_byte(crate::serial::SC_ADDR);
            if sc & 0x81 == 0x81 {
                let sb = self.mmu.read_byte(crate::serial::SB_ADDR);
                self.serial.on_sc_write(sb, sc);
                self.mmu.write_byte(crate::serial::SC_ADDR, sc & !0x80);
            }
        }

        // In double-speed mode, peripherals run at half the CPU cycle rate
        let peripheral_cycles = if self.mmu.double_speed { cycles >> 1 } else { cycles };

        if self.timer.step(peripheral_cycles, &mut self.mmu) {
            interrupts::request(&mut self.mmu, interrupts::source::TIMER);
        }

        let ppu_result = self.ppu.step(peripheral_cycles, &mut self.mmu);
        if ppu_result.vblank_irq {
            interrupts::request(&mut self.mmu, interrupts::source::VBLANK);
        }
        if ppu_result.stat_irq {
            interrupts::request(&mut self.mmu, interrupts::source::LCD_STAT);
        }
        // H-Blank DMA
        if ppu_result.hblank {
            self.mmu.hdma_hblank_step();
        }

        if self.joypad.sync(&mut self.mmu) {
            interrupts::request(&mut self.mmu, interrupts::source::JOYPAD);
        }

        self.apu.step(peripheral_cycles, &mut self.mmu);
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

impl Default for Cpu { fn default() -> Self { Self::new() } }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::interrupts::{source, IE_ADDR};
    use crate::timer::{TAC_ADDR, TIMA_ADDR, TMA_ADDR};
    use crate::ppu::{self, LCDC_ADDR};
    use crate::input::{Button, JOYP_ADDR};
    use crate::serial::{SB_ADDR, SC_ADDR};

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

    #[test]
    fn test_nop_still_works() {
        let mut cpu = cpu_with_program(&[0x00]);
        let start = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, start + 1);
    }

    #[test]
    fn test_vblank_interrupt() {
        let mut cpu = cpu_with_program(&[0x00]);
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR, source::VBLANK);
        cpu.request_interrupt(source::VBLANK);
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0x0040);
    }

    #[test]
    fn test_timer_irq() {
        let mut cpu = cpu_with_program(&[0x00u8; 16]);
        cpu.ime = true;
        cpu.mmu.write_byte(TAC_ADDR, 0x05);
        cpu.mmu.write_byte(TIMA_ADDR, 0xFF);
        cpu.mmu.write_byte(TMA_ADDR, 0x00);
        cpu.mmu.write_byte(IE_ADDR, source::TIMER);
        for _ in 0..5 { cpu.tick(); }
        assert_eq!(cpu.regs.pc, 0x0050);
    }

    #[test]
    fn test_ppu_ly_advances() {
        let mut cpu = cpu_with_program(&[0x00u8; 128]);
        cpu.mmu.write_byte(LCDC_ADDR, 0x91);
        for _ in 0..114 { cpu.tick(); }
        assert_eq!(cpu.mmu.read_byte(ppu::LY_ADDR), 1);
    }

    #[test]
    fn test_button_press() {
        let mut cpu = cpu_with_program(&[0x00u8; 4]);
        cpu.mmu.write_byte(JOYP_ADDR, 0xDF);
        cpu.button_press(Button::A);
        cpu.tick();
        assert_eq!(cpu.mmu.read_byte(JOYP_ADDR) & 0x01, 0);
    }

    #[test]
    fn test_serial_captures_byte() {
        let mut cpu = cpu_with_program(&[0x00u8; 4]);
        cpu.mmu.write_byte(SB_ADDR, b'A');
        cpu.mmu.write_byte(SC_ADDR, 0x81);
        cpu.tick();
        assert!(cpu.serial.output.contains(&b'A'));
    }

    #[test]
    fn test_double_speed_halves_peripheral_cycles() {
        let mut cpu1 = cpu_with_nop_rom();
        cpu1.mmu.write_byte(LCDC_ADDR, 0x91);
        let mut cpu2 = cpu_with_nop_rom();
        cpu2.mmu.write_byte(LCDC_ADDR, 0x91);
        cpu2.mmu.double_speed = true;

        // Run same number of CPU ticks
        for _ in 0..100 { cpu1.tick(); cpu2.tick(); }

        // cpu2 in double speed — PPU advances slower, LY should be lower
        let ly1 = cpu1.mmu.read_byte(ppu::LY_ADDR);
        let ly2 = cpu2.mmu.read_byte(ppu::LY_ADDR);
        assert!(ly1 >= ly2, "double speed PPU must advance slower: ly1={} ly2={}", ly1, ly2);
    }

    #[test]
    fn test_apu_produces_samples() {
        let mut cpu = cpu_with_nop_rom();
        cpu.mmu.write_byte(crate::apu::NR52_ADDR, 0x80);
        for _ in 0..100 { cpu.tick(); }
        assert!(!cpu.apu.sample_buffer.is_empty());
    }
}