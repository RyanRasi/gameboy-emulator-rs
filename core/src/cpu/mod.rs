//! SM83 CPU — Game Boy processor core.
//!
//! The CPU owns its register file and holds a reference to the MMU.
//! `step()` fetches, decodes, and executes one instruction, returning
//! the number of T-cycles consumed.

pub mod registers;
mod instructions; // instruction execution lives here, exposed via `step()`

pub use registers::Registers;

use crate::mmu::Mmu;

pub struct Cpu {
    pub regs: Registers,
    pub mmu:  Mmu,

    /// Total T-cycles elapsed since power-on.
    pub cycles: u64,

    /// Interrupt Master Enable flag.
    /// Set by EI, cleared by DI or when an interrupt fires.
    pub ime: bool,

    /// Halted state — CPU stops executing until an interrupt fires.
    pub halted: bool,
}

impl Cpu {
    /// Create a CPU with DMG power-on register state and a fresh MMU.
    pub fn new() -> Self {
        Cpu {
            regs:    Registers::new(),
            mmu:     Mmu::new(),
            cycles:  0,
            ime:     false,
            halted:  false,
        }
    }

    /// Execute one instruction and accumulate cycles.
    /// Returns T-cycles consumed by this step.
    pub fn tick(&mut self) -> u32 {
        let t = self.step();
        self.cycles += t as u64;
        t
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: load a small program into WRAM starting at 0xC000,
    // then point PC there so we execute it directly.
    fn cpu_with_program(program: &[u8]) -> Cpu {
        let mut cpu = Cpu::new();
        for (i, &byte) in program.iter().enumerate() {
            cpu.mmu.write_byte(0xC000 + i as u16, byte);
        }
        cpu.regs.pc = 0xC000;
        cpu
    }

    // -------------------------------------------------------------------------
    // NOP
    // -------------------------------------------------------------------------

    #[test]
    fn test_nop_advances_pc_by_1() {
        let mut cpu = cpu_with_program(&[0x00]);
        let start_pc = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, start_pc + 1);
    }

    #[test]
    fn test_nop_costs_4_cycles() {
        let mut cpu = cpu_with_program(&[0x00]);
        let t = cpu.tick();
        assert_eq!(t, 4);
    }

    #[test]
    fn test_nop_does_not_change_registers() {
        let mut cpu = cpu_with_program(&[0x00]);
        let regs_before = cpu.regs.clone();
        cpu.tick();
        // Only PC should have changed
        let mut expected = regs_before;
        expected.pc += 1;
        assert_eq!(cpu.regs, expected);
    }

    // -------------------------------------------------------------------------
    // LD r, n8 — immediate loads
    // -------------------------------------------------------------------------

    #[test]
    fn test_ld_a_n_loads_correct_value() {
        // 0x3E 0x42 → LD A, 0x42
        let mut cpu = cpu_with_program(&[0x3E, 0x42]);
        cpu.tick();
        assert_eq!(cpu.regs.a, 0x42);
    }

    #[test]
    fn test_ld_a_n_advances_pc_by_2() {
        let mut cpu = cpu_with_program(&[0x3E, 0x42]);
        let start = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, start + 2);
    }

    #[test]
    fn test_ld_a_n_costs_8_cycles() {
        let mut cpu = cpu_with_program(&[0x3E, 0x99]);
        let t = cpu.tick();
        assert_eq!(t, 8);
    }

    #[test]
    fn test_ld_b_n_loads_correct_value() {
        let mut cpu = cpu_with_program(&[0x06, 0x11]);
        cpu.tick();
        assert_eq!(cpu.regs.b, 0x11);
    }

    #[test]
    fn test_ld_c_n_loads_correct_value() {
        let mut cpu = cpu_with_program(&[0x0E, 0x22]);
        cpu.tick();
        assert_eq!(cpu.regs.c, 0x22);
    }

    #[test]
    fn test_ld_d_n_loads_correct_value() {
        let mut cpu = cpu_with_program(&[0x16, 0x33]);
        cpu.tick();
        assert_eq!(cpu.regs.d, 0x33);
    }

    #[test]
    fn test_ld_e_n_loads_correct_value() {
        let mut cpu = cpu_with_program(&[0x1E, 0x44]);
        cpu.tick();
        assert_eq!(cpu.regs.e, 0x44);
    }

    #[test]
    fn test_ld_h_n_loads_correct_value() {
        let mut cpu = cpu_with_program(&[0x26, 0x55]);
        cpu.tick();
        assert_eq!(cpu.regs.h, 0x55);
    }

    #[test]
    fn test_ld_l_n_loads_correct_value() {
        let mut cpu = cpu_with_program(&[0x2E, 0x66]);
        cpu.tick();
        assert_eq!(cpu.regs.l, 0x66);
    }

    // -------------------------------------------------------------------------
    // LD rr, n16 — 16-bit immediate loads
    // -------------------------------------------------------------------------

    #[test]
    fn test_ld_bc_nn_loads_correct_value() {
        // 0x01 0xCD 0xAB → LD BC, 0xABCD (little-endian)
        let mut cpu = cpu_with_program(&[0x01, 0xCD, 0xAB]);
        cpu.tick();
        assert_eq!(cpu.regs.bc(), 0xABCD);
    }

    #[test]
    fn test_ld_de_nn_loads_correct_value() {
        let mut cpu = cpu_with_program(&[0x11, 0x34, 0x12]);
        cpu.tick();
        assert_eq!(cpu.regs.de(), 0x1234);
    }

    #[test]
    fn test_ld_hl_nn_loads_correct_value() {
        let mut cpu = cpu_with_program(&[0x21, 0xEF, 0xBE]);
        cpu.tick();
        assert_eq!(cpu.regs.hl(), 0xBEEF);
    }

    #[test]
    fn test_ld_sp_nn_loads_correct_value() {
        let mut cpu = cpu_with_program(&[0x31, 0xFE, 0xFF]);
        cpu.tick();
        assert_eq!(cpu.regs.sp, 0xFFFE);
    }

    #[test]
    fn test_ld_nn_costs_12_cycles() {
        let mut cpu = cpu_with_program(&[0x01, 0x00, 0x00]);
        let t = cpu.tick();
        assert_eq!(t, 12);
    }

    #[test]
    fn test_ld_nn_advances_pc_by_3() {
        let mut cpu = cpu_with_program(&[0x21, 0x00, 0x80]);
        let start = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, start + 3);
    }

    // -------------------------------------------------------------------------
    // Register-to-register loads
    // -------------------------------------------------------------------------

    #[test]
    fn test_ld_b_a_copies_a_to_b() {
        // First LD A, 0x77 then LD B, A
        let mut cpu = cpu_with_program(&[0x3E, 0x77, 0x47]);
        cpu.tick(); // LD A, 0x77
        cpu.tick(); // LD B, A
        assert_eq!(cpu.regs.b, 0x77);
    }

    #[test]
    fn test_ld_r_r_costs_4_cycles() {
        let mut cpu = cpu_with_program(&[0x47]); // LD B, A
        let t = cpu.tick();
        assert_eq!(t, 4);
    }

    #[test]
    fn test_ld_c_b_copies_b_to_c() {
        let mut cpu = cpu_with_program(&[0x06, 0xAA, 0x48]); // LD B,0xAA; LD C,B
        cpu.tick();
        cpu.tick();
        assert_eq!(cpu.regs.c, 0xAA);
    }

    #[test]
    fn test_ld_a_l_copies_l_to_a() {
        let mut cpu = cpu_with_program(&[0x2E, 0x55, 0x7D]); // LD L,0x55; LD A,L
        cpu.tick();
        cpu.tick();
        assert_eq!(cpu.regs.a, 0x55);
    }

    // -------------------------------------------------------------------------
    // Program execution + PC sequencing
    // -------------------------------------------------------------------------

    #[test]
    fn test_multi_instruction_sequence_pc_correct() {
        // NOP, LD A,0x01, NOP  →  PC should advance 1+2+1 = 4
        let mut cpu = cpu_with_program(&[0x00, 0x3E, 0x01, 0x00]);
        let start = cpu.regs.pc;
        cpu.tick(); // NOP
        cpu.tick(); // LD A, 0x01
        cpu.tick(); // NOP
        assert_eq!(cpu.regs.pc, start + 4);
    }

    #[test]
    fn test_cycle_accumulation() {
        // NOP(4) + LD A,n(8) + NOP(4) = 16 T-cycles
        let mut cpu = cpu_with_program(&[0x00, 0x3E, 0xFF, 0x00]);
        cpu.tick();
        cpu.tick();
        cpu.tick();
        assert_eq!(cpu.cycles, 16);
    }

    #[test]
    fn test_tick_accumulates_into_total_cycles() {
        let mut cpu = cpu_with_program(&[0x00, 0x00, 0x00]); // three NOPs
        cpu.tick();
        cpu.tick();
        cpu.tick();
        assert_eq!(cpu.cycles, 12);
    }
}