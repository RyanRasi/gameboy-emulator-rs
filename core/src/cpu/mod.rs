//! SM83 CPU — Game Boy processor core.

pub mod registers;
pub mod alu;
pub mod interrupts;
mod instructions;

pub use registers::Registers;

use crate::mmu::Mmu;
use crate::timer::Timer;
use crate::ppu::Ppu;
use crate::input::Joypad;

pub struct Cpu {
    pub regs:   Registers,
    pub mmu:    Mmu,
    pub timer:  Timer,
    pub ppu:    Ppu,
    pub joypad: Joypad,
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
            cycles: 0,
            ime:    false,
            halted: false,
        }
    }

    /// Execute one full CPU tick:
    ///   1. Service any pending interrupt (20 cycles, returns early).
    ///   2. If halted, idle for 4 cycles.
    ///   3. Otherwise fetch-decode-execute one instruction.
    ///   4. Step Timer, PPU, and Joypad; propagate any generated interrupts.
    pub fn tick(&mut self) -> u32 {
        // ── Interrupt service ────────────────────────────────────────────────
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

        // ── Instruction / idle ───────────────────────────────────────────────
        let instr_cycles = if self.halted { 4 } else { self.step() };

        self.cycles += instr_cycles as u64;
        self.step_peripherals(instr_cycles);
        instr_cycles
    }

    /// Step Timer, PPU, and Joypad; request any resulting interrupts.
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
    }

    /// Notify the emulator that a button was pressed.
    pub fn button_press(&mut self, button: crate::input::Button) {
        self.joypad.press(button);
    }

    /// Notify the emulator that a button was released.
    pub fn button_release(&mut self, button: crate::input::Button) {
        self.joypad.release(button);
    }

    /// Request an interrupt from outside the CPU.
    pub fn request_interrupt(&mut self, mask: u8) {
        interrupts::request(&mut self.mmu, mask);
    }
}

impl Default for Cpu {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::interrupts::{source, IF_ADDR, IE_ADDR};
    use crate::timer::{TAC_ADDR, TIMA_ADDR, TMA_ADDR, DIV_ADDR};
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
    fn test_ppu_vblank_irq_triggers_cpu_handler() {
        let mut cpu = Cpu::new();
        let mut rom = vec![0x00u8; 0x8000];
        rom[0x0040] = 0xC9;
        cpu.mmu.load_rom(&rom).unwrap();
        cpu.mmu.write_byte(ppu::LCDC_ADDR, 0x91);
        cpu.mmu.write_byte(IE_ADDR, source::VBLANK);
        cpu.ime = true;
        let mut jumped = false;
        for _ in 0..20_000 {
            cpu.tick();
            if cpu.regs.pc == 0x0040 { jumped = true; break; }
        }
        assert!(jumped);
    }

    // ── Input integration ─────────────────────────────────────────────────────

    #[test]
    fn test_button_press_updates_joyp_register_after_tick() {
        let mut cpu = cpu_with_program(&[0x00u8; 4]);
        cpu.mmu.write_byte(JOYP_ADDR, 0xDF); // action group only (bit5=0, bit4=1)
        cpu.button_press(Button::A);
        cpu.tick();
        let joyp = cpu.mmu.read_byte(JOYP_ADDR);
        assert_eq!(joyp & 0x01, 0, "A button must read low (pressed) after tick");
    }

    #[test]
    fn test_button_release_restores_joyp_bit_after_tick() {
        let mut cpu = cpu_with_program(&[0x00u8; 4]);
        cpu.mmu.write_byte(JOYP_ADDR, 0xDF); // action group only
        cpu.button_press(Button::A);
        cpu.tick();
        cpu.button_release(Button::A);
        cpu.tick();
        let joyp = cpu.mmu.read_byte(JOYP_ADDR);
        assert_eq!(joyp & 0x01, 0x01, "Released A must read high again");
    }

    #[test]
    fn test_joypad_interrupt_fires_on_button_press() {
        let mut cpu = cpu_with_program(&[0x00u8; 8]);
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR,   source::JOYPAD);
        cpu.mmu.write_byte(JOYP_ADDR, 0xDF); // action group only
        cpu.tick();                // baseline sync
        cpu.button_press(Button::Start);
        cpu.tick();                // joypad sync → IF bit set
        cpu.tick();                // IRQ serviced → PC = 0x0060
        assert_eq!(cpu.regs.pc, 0x0060, "CPU must jump to Joypad vector 0x0060");
    }

    #[test]
    fn test_joypad_irq_not_fired_on_release() {
        let mut cpu = cpu_with_program(&[0x00u8; 8]);
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR,   source::JOYPAD);
        cpu.mmu.write_byte(JOYP_ADDR, 0xDF); // action group only
        cpu.button_press(Button::A);
        cpu.tick(); // press fires IRQ
        cpu.tick(); // service IRQ
        cpu.regs.pc = 0xC000;
        cpu.mmu.write_byte(IF_ADDR, 0x00);
        cpu.button_release(Button::A);
        cpu.tick();
        cpu.tick();
        assert_ne!(cpu.regs.pc, 0x0060, "Release must not fire Joypad IRQ");
    }

    #[test]
    fn test_dpad_buttons_readable_via_joyp() {
        let mut cpu = cpu_with_program(&[0x00u8; 4]);
        cpu.mmu.write_byte(JOYP_ADDR, 0xEF); // d-pad group only (bit5=1, bit4=0)
        cpu.button_press(Button::Up);
        cpu.tick();
        let joyp = cpu.mmu.read_byte(JOYP_ADDR);
        assert_eq!(joyp & 0x04, 0, "Up must read low in d-pad group");
    }

    #[test]
    fn test_action_buttons_not_visible_in_dpad_group() {
        let mut cpu = cpu_with_program(&[0x00u8; 4]);
        cpu.mmu.write_byte(JOYP_ADDR, 0xEF); // d-pad group only
        cpu.button_press(Button::A);          // action button — must not appear
        cpu.tick();
        let joyp = cpu.mmu.read_byte(JOYP_ADDR);
        assert_eq!(joyp & 0x01, 0x01, "Action A must not appear in d-pad group");
    }

    #[test]
    fn test_all_buttons_independently_controllable() {
        let mut cpu = cpu_with_program(&[0x00u8; 4]);
        for btn in Button::ALL {
            cpu.button_press(btn);
        }
        cpu.tick();
        for btn in Button::ALL {
            assert!(cpu.joypad.is_pressed(btn), "{:?} must be pressed", btn);
        }
        for btn in Button::ALL {
            cpu.button_release(btn);
        }
        cpu.tick();
        for btn in Button::ALL {
            assert!(!cpu.joypad.is_pressed(btn), "{:?} must be released", btn);
        }
    }
}