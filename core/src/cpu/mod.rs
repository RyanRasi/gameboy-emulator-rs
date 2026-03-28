//! SM83 CPU — Game Boy processor core.

pub mod registers;
pub mod alu;
pub mod interrupts;
mod instructions;

pub use registers::Registers;

use crate::mmu::Mmu;

pub struct Cpu {
    pub regs:   Registers,
    pub mmu:    Mmu,
    pub cycles: u64,
    pub ime:    bool,
    pub halted: bool,
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            regs:    Registers::new(),
            mmu:     Mmu::new(),
            cycles:  0,
            ime:     false,
            halted:  false,
        }
    }

    /// Execute one full CPU tick.
    ///
    /// Priority order:
    ///   1. If an interrupt is pending and IME is set, service it (20 cycles)
    ///      and return — no instruction executes this tick.
    ///   2. If halted, burn 4 cycles waiting for an interrupt.
    ///   3. Otherwise fetch-decode-execute one instruction.
    pub fn tick(&mut self) -> u32 {
        // ── Step 1: interrupt check ──────────────────────────────────────────
        let irq_cycles = interrupts::service(
            &mut self.mmu,
            &mut self.ime,
            &mut self.halted,
            &mut self.regs.pc,
            &mut self.regs.sp,
        );

        if irq_cycles > 0 {
            // An interrupt was dispatched — that's the whole tick.
            self.cycles += irq_cycles as u64;
            return irq_cycles;
        }

        // ── Step 2: halted / normal execution ────────────────────────────────
        let instr_cycles = if self.halted {
            4 // CPU idles until something wakes it
        } else {
            self.step()
        };

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
    use super::registers::flags as cpu_flags;
    use super::interrupts::{source, IF_ADDR, IE_ADDR};

    fn cpu_with_program(program: &[u8]) -> Cpu {
        let mut cpu = Cpu::new();
        for (i, &byte) in program.iter().enumerate() {
            cpu.mmu.write_byte(0xC000 + i as u16, byte);
        }
        cpu.regs.pc = 0xC000;
        cpu
    }

    // ── Phase 2 / 3 regressions ─────────────────────────────────────────────

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
    fn test_call_ret_still_works() {
        let mut cpu = cpu_with_program(&[0xCD, 0x06, 0xC0, 0x00, 0x00, 0x00, 0x00, 0xC9]);
        cpu.tick(); // CALL
        assert_eq!(cpu.regs.pc, 0xC006);
        cpu.tick(); // NOP
        cpu.tick(); // RET
        assert_eq!(cpu.regs.pc, 0xC003);
    }

    // ── IME flag (DI/EI) ────────────────────────────────────────────────────

    #[test]
    fn test_di_clears_ime() {
        let mut cpu = cpu_with_program(&[0xF3]);
        cpu.ime = true;
        cpu.tick();
        assert!(!cpu.ime);
    }

    #[test]
    fn test_ei_sets_ime() {
        let mut cpu = cpu_with_program(&[0xFB]);
        cpu.ime = false;
        cpu.tick();
        assert!(cpu.ime);
    }

    // ── HALT ────────────────────────────────────────────────────────────────

    #[test]
    fn test_halt_stops_execution() {
        // HALT at 0xC000, NOP at 0xC001
        let mut cpu = cpu_with_program(&[0x76, 0x00]);
        cpu.ime = true;
        cpu.tick(); // executes HALT → cpu.halted = true
        let pc_after_halt = cpu.regs.pc;
        cpu.tick(); // halted — PC must not advance
        assert_eq!(cpu.regs.pc, pc_after_halt, "PC must not advance while halted");
    }

    #[test]
    fn test_halt_wakes_on_pending_interrupt() {
        let mut cpu = cpu_with_program(&[0x76]); // HALT at 0xC000
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR, source::VBLANK);
        cpu.tick(); // HALT executes → halted = true, PC = 0xC001

        // Fire VBlank interrupt
        cpu.request_interrupt(source::VBLANK);

        // Next tick: service() runs first — clears halted, jumps to 0x0040,
        // returns early (no instruction executes this tick)
        cpu.tick();

        assert!(!cpu.halted, "CPU must wake from HALT");
        assert_eq!(cpu.regs.pc, 0x0040, "PC must jump to VBlank vector");
    }

    #[test]
    fn test_halt_wakes_without_servicing_when_ime_false() {
        let mut cpu = cpu_with_program(&[0x76, 0x00]); // HALT, NOP
        cpu.ime = false;
        cpu.mmu.write_byte(IE_ADDR, source::TIMER);
        cpu.tick(); // HALT executes
        cpu.request_interrupt(source::TIMER);
        cpu.tick(); // wakes (IME=false → no service), executes NOP
        assert!(!cpu.halted, "HALT must clear even with IME=false");
    }

    // ── Interrupt flag set triggers handler ─────────────────────────────────

    #[test]
    fn test_vblank_interrupt_fires_and_jumps_to_vector() {
        let mut cpu = cpu_with_program(&[0x00]); // NOP at 0xC000
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR, source::VBLANK);
        cpu.request_interrupt(source::VBLANK);
        // Interrupt is serviced — PC jumps to 0x0040, no instruction runs
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0x0040, "PC must be at VBlank vector after IRQ dispatch");
    }

    #[test]
    fn test_interrupt_pushes_correct_return_address() {
        let mut cpu = cpu_with_program(&[0x00, 0x00]);
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR, source::VBLANK);
        cpu.regs.pc = 0xC005;
        cpu.request_interrupt(source::VBLANK);
        let sp_before = cpu.regs.sp;
        let cycles = interrupts::service(
            &mut cpu.mmu,
            &mut cpu.ime,
            &mut cpu.halted,
            &mut cpu.regs.pc,
            &mut cpu.regs.sp,
        );
        assert_eq!(cycles, 20);
        let ret = cpu.mmu.read_word(cpu.regs.sp);
        assert_eq!(ret, 0xC005, "Return address must be PC at time of interrupt");
        assert_eq!(cpu.regs.sp, sp_before - 2);
    }

    #[test]
    fn test_interrupt_clears_ime() {
        let mut cpu = cpu_with_program(&[0x00]);
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR, source::VBLANK);
        cpu.request_interrupt(source::VBLANK);
        cpu.tick();
        assert!(!cpu.ime, "IME must be cleared after interrupt serviced");
    }

    #[test]
    fn test_interrupt_does_not_fire_when_ie_bit_not_set() {
        let mut cpu = cpu_with_program(&[0x00]);
        cpu.ime = true;
        cpu.mmu.write_byte(IE_ADDR, 0x00);
        cpu.request_interrupt(source::VBLANK);
        let pc_before = cpu.regs.pc;
        cpu.tick(); // NOP executes — no interrupt
        assert_eq!(cpu.regs.pc, pc_before + 1, "No interrupt should fire");
    }

    #[test]
    fn test_interrupt_does_not_fire_when_ime_false() {
        let mut cpu = cpu_with_program(&[0x00]);
        cpu.ime = false;
        cpu.mmu.write_byte(IE_ADDR, source::VBLANK);
        cpu.request_interrupt(source::VBLANK);
        let pc_before = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, pc_before + 1, "Interrupt must not fire when IME=false");
    }

    // ── VBlank simulated test ────────────────────────────────────────────────

#[test]
fn test_simulated_vblank_interrupt_full_sequence() {
    // The VBlank vector (0x0040) lives in ROM space — writes are ignored.
    // Load a proper ROM image with RET (0xC9) baked in at 0x0040.
    let mut cpu = Cpu::new();

    let mut rom = vec![0x00u8; 0x8000]; // 32 KiB, all NOPs
    rom[0x0040] = 0xC9;                 // RET at VBlank vector
    cpu.mmu.load_rom(&rom).unwrap();

    // Main code: NOPs in WRAM (writable)
    for i in 0..10u16 {
        cpu.mmu.write_byte(0xC000 + i, 0x00);
    }
    cpu.regs.pc = 0xC000;
    cpu.ime = true;
    cpu.mmu.write_byte(IE_ADDR, source::VBLANK);

    cpu.tick(); // NOP → PC = 0xC001
    cpu.tick(); // NOP → PC = 0xC002

    // Fire VBlank
    cpu.request_interrupt(source::VBLANK);

    // Tick 3: IRQ dispatch only — PC → 0x0040, return addr 0xC002 pushed
    cpu.tick();
    assert_eq!(cpu.regs.pc, 0x0040, "IRQ dispatch must jump to 0x0040");

    // Tick 4: RET at 0x0040 executes — pops 0xC002, PC → 0xC002
    cpu.tick();
    assert_eq!(
        cpu.regs.pc, 0xC002,
        "After VBlank ISR (RET), execution must resume at 0xC002"
    );
}

    // ── request_interrupt helper ─────────────────────────────────────────────

    #[test]
    fn test_request_interrupt_sets_if_register() {
        let mut cpu = cpu_with_program(&[]);
        cpu.request_interrupt(source::TIMER);
        let if_ = cpu.mmu.read_byte(IF_ADDR);
        assert_ne!(if_ & source::TIMER, 0);
    }

    #[test]
    fn test_multiple_interrupt_requests_accumulate() {
        let mut cpu = cpu_with_program(&[]);
        cpu.request_interrupt(source::VBLANK);
        cpu.request_interrupt(source::TIMER);
        let if_ = cpu.mmu.read_byte(IF_ADDR);
        assert_ne!(if_ & source::VBLANK, 0);
        assert_ne!(if_ & source::TIMER, 0);
    }
}