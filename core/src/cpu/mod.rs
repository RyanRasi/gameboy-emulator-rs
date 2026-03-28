//! SM83 CPU — Game Boy processor core.

pub mod registers;
pub mod alu;
pub mod interrupts;
mod instructions;

pub use registers::Registers;

use crate::mmu::Mmu;
use crate::timer::Timer;

pub struct Cpu {
    pub regs:   Registers,
    pub mmu:    Mmu,
    pub timer:  Timer,
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
            cycles: 0,
            ime:    false,
            halted: false,
        }
    }

    /// Execute one full CPU tick:
    ///   1. Service any pending interrupt (20 cycles, returns early if fired).
    ///   2. If halted, idle for 4 cycles.
    ///   3. Otherwise fetch-decode-execute one instruction.
    ///   4. Step the timer; if it overflows, request a Timer interrupt.
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
            // Step timer for the cycles used by interrupt dispatch
            if self.timer.step(irq_cycles, &mut self.mmu) {
                interrupts::request(&mut self.mmu, interrupts::source::TIMER);
            }
            return irq_cycles;
        }

        // ── Instruction / halt ───────────────────────────────────────────────
        let instr_cycles = if self.halted { 4 } else { self.step() };

        // ── Timer step ───────────────────────────────────────────────────────
        if self.timer.step(instr_cycles, &mut self.mmu) {
            interrupts::request(&mut self.mmu, interrupts::source::TIMER);
        }

        self.cycles += instr_cycles as u64;
        instr_cycles
    }

    /// Request an interrupt from outside the CPU (e.g. PPU triggers VBlank).
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

    fn cpu_with_program(program: &[u8]) -> Cpu {
        let mut cpu = Cpu::new();
        for (i, &byte) in program.iter().enumerate() {
            cpu.mmu.write_byte(0xC000 + i as u16, byte);
        }
        cpu.regs.pc = 0xC000;
        cpu
    }

    // ── Phase 2/3/4 regressions ──────────────────────────────────────────────

    #[test]
    fn test_nop_still_works() {
        let mut cpu = cpu_with_program(&[0x00]);
        let start = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, start + 1);
    }

    #[test]
    fn test_ld_a_n_still_works() {
        let mut cpu = cpu_with_program(&[0x3E, 0x42]);
        cpu.tick();
        assert_eq!(cpu.regs.a, 0x42);
    }

    #[test]
    fn test_vblank_interrupt_fires_and_jumps_to_vector() {
        let mut cpu = cpu_with_program(&[0x00]);
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR, source::VBLANK);
        cpu.request_interrupt(source::VBLANK);
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0x0040);
    }

    #[test]
    fn test_halt_stops_execution() {
        let mut cpu = cpu_with_program(&[0x76, 0x00]);
        cpu.ime = true;
        cpu.tick();
        let pc_after = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, pc_after);
    }

    // ── Timer integration ─────────────────────────────────────────────────────

    #[test]
    fn test_div_advances_as_cpu_ticks() {
        // Each NOP = 4 T-cycles. After 64 NOPs = 256 T-cycles, DIV = 1.
        let mut cpu = cpu_with_program(&[0x00u8; 128]);
        for _ in 0..64 {
            cpu.tick(); // NOP × 64 = 256 T-cycles
        }
        assert_eq!(cpu.mmu.read_byte(DIV_ADDR), 1);
    }

    #[test]
    fn test_tima_advances_as_cpu_ticks_when_enabled() {
        // TAC = 0x05 → enabled, 262144 Hz (period = 16 T-cycles)
        // 4 NOPs = 16 T-cycles → TIMA increments once
        let mut cpu = cpu_with_program(&[0x00u8; 16]);
        cpu.mmu.write_byte(TAC_ADDR, 0x05);
        for _ in 0..4 {
            cpu.tick(); // 4 × NOP = 16 T-cycles
        }
        assert_eq!(cpu.mmu.read_byte(TIMA_ADDR), 1);
    }

    #[test]
    fn test_tima_overflow_fires_timer_interrupt() {
        // TAC = 0x05 (period = 16), TIMA = 0xFF, TMA = 0x00
        // After one period, TIMA overflows → Timer IRQ should be pending
        let mut cpu = cpu_with_program(&[0x00u8; 16]);
        cpu.mmu.write_byte(TAC_ADDR,  0x05);
        cpu.mmu.write_byte(TIMA_ADDR, 0xFF);
        cpu.mmu.write_byte(TMA_ADDR,  0x00);
        cpu.mmu.write_byte(IE_ADDR,   source::TIMER);

        // 4 NOPs = 16 T-cycles → TIMA overflows → IF bit 2 set
        for _ in 0..4 {
            cpu.tick();
        }

        let if_ = cpu.mmu.read_byte(IF_ADDR);
        assert_ne!(if_ & source::TIMER, 0, "Timer interrupt must be pending after TIMA overflow");
    }

    #[test]
    fn test_timer_interrupt_services_and_jumps_to_0050() {
        // Set up Timer ISR at 0x0050 via ROM, enable timer, overflow TIMA,
        // then verify CPU dispatches to 0x0050.
        let mut cpu = Cpu::new();

        let mut rom = vec![0x00u8; 0x8000];
        rom[0x0050] = 0xC9; // RET at Timer vector
        cpu.mmu.load_rom(&rom).unwrap();

        // Main code in WRAM: NOPs
        for i in 0..32u16 {
            cpu.mmu.write_byte(0xC000 + i, 0x00);
        }
        cpu.regs.pc = 0xC000;
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR,   source::TIMER);
        cpu.mmu.write_byte(TAC_ADDR,  0x05); // enabled, period=16
        cpu.mmu.write_byte(TIMA_ADDR, 0xFF);
        cpu.mmu.write_byte(TMA_ADDR,  0x00);

        // One NOP tick (4 T-cycles) won't overflow yet — need 16 total
        // Run 4 NOPs (16 T-cycles) to trigger overflow
        for _ in 0..4 {
            cpu.tick();
        }

        // IF bit should now be set — next tick dispatches interrupt
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0x0050, "CPU must jump to Timer vector 0x0050");
    }

    #[test]
    fn test_tima_does_not_increment_when_tac_disabled() {
        let mut cpu = cpu_with_program(&[0x00u8; 128]);
        cpu.mmu.write_byte(TAC_ADDR, 0x00); // disabled
        for _ in 0..128 {
            cpu.tick();
        }
        assert_eq!(cpu.mmu.read_byte(TIMA_ADDR), 0);
    }

    #[test]
    fn test_tma_is_reloaded_into_tima_after_overflow() {
        let mut cpu = cpu_with_program(&[0x00u8; 16]);
        cpu.mmu.write_byte(TAC_ADDR,  0x05);
        cpu.mmu.write_byte(TIMA_ADDR, 0xFF);
        cpu.mmu.write_byte(TMA_ADDR,  0x77);
        for _ in 0..4 {
            cpu.tick(); // 16 T-cycles total
        }
        assert_eq!(cpu.mmu.read_byte(TIMA_ADDR), 0x77);
    }
}