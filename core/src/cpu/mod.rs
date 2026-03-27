//! SM83 CPU — Game Boy processor core.

pub mod registers;
pub mod alu;
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

    pub fn tick(&mut self) -> u32 {
        let t = self.step();
        self.cycles += t as u64;
        t
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
    use super::registers::flags;

    fn cpu_with_program(program: &[u8]) -> Cpu {
        let mut cpu = Cpu::new();
        for (i, &byte) in program.iter().enumerate() {
            cpu.mmu.write_byte(0xC000 + i as u16, byte);
        }
        cpu.regs.pc = 0xC000;
        cpu
    }

    // ── Phase 2 regression ───────────────────────────────────────────────────

    #[test]
    fn test_nop_advances_pc_by_1() {
        let mut cpu = cpu_with_program(&[0x00]);
        let start = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, start + 1);
    }

    #[test]
    fn test_ld_a_n_loads_correct_value() {
        let mut cpu = cpu_with_program(&[0x3E, 0x42]);
        cpu.tick();
        assert_eq!(cpu.regs.a, 0x42);
    }

    // ── INC / DEC ────────────────────────────────────────────────────────────

    #[test]
    fn test_inc_b_increments() {
        // LD B,0x0F then INC B → B == 0x10, H set
        let mut cpu = cpu_with_program(&[0x06, 0x0F, 0x04]);
        cpu.tick(); cpu.tick();
        assert_eq!(cpu.regs.b, 0x10);
        assert!(cpu.regs.flag_h());
    }

    #[test]
    fn test_dec_a_decrements() {
        // LD A,0x01 then DEC A → A == 0x00, Z set, N set
        let mut cpu = cpu_with_program(&[0x3E, 0x01, 0x3D]);
        cpu.tick(); cpu.tick();
        assert_eq!(cpu.regs.a, 0x00);
        assert!(cpu.regs.flag_z());
        assert!(cpu.regs.flag_n());
    }

    #[test]
    fn test_inc_does_not_affect_carry() {
        // Set carry, then INC A — carry must be preserved
        let mut cpu = cpu_with_program(&[0x3E, 0x00, 0x3C]);
        cpu.tick(); // LD A, 0x00
        cpu.regs.f = flags::C;
        cpu.tick(); // INC A
        assert!(cpu.regs.flag_c(), "INC must preserve carry flag");
    }

    // ── ADD / flags ──────────────────────────────────────────────────────────

    #[test]
    fn test_add_a_b_result_and_no_flags() {
        // LD A,0x10; LD B,0x20; ADD A,B → A == 0x30
        let mut cpu = cpu_with_program(&[0x3E, 0x10, 0x06, 0x20, 0x80]);
        cpu.tick(); cpu.tick(); cpu.tick();
        assert_eq!(cpu.regs.a, 0x30);
        assert!(!cpu.regs.flag_z());
        assert!(!cpu.regs.flag_n());
        assert!(!cpu.regs.flag_h());
        assert!(!cpu.regs.flag_c());
    }

    #[test]
    fn test_add_sets_zero_flag_on_overflow() {
        // LD A,0xFF; ADD A,0x01 → A == 0x00, Z set, C set
        let mut cpu = cpu_with_program(&[0x3E, 0xFF, 0xC6, 0x01]);
        cpu.tick(); cpu.tick();
        assert_eq!(cpu.regs.a, 0x00);
        assert!(cpu.regs.flag_z());
        assert!(cpu.regs.flag_c());
    }

    #[test]
    fn test_add_sets_half_carry() {
        // LD A,0x0F; ADD A,0x01 → H set
        let mut cpu = cpu_with_program(&[0x3E, 0x0F, 0xC6, 0x01]);
        cpu.tick(); cpu.tick();
        assert!(cpu.regs.flag_h());
        assert!(!cpu.regs.flag_n());
    }

    #[test]
    fn test_sub_sets_n_flag() {
        // LD A,0x30; SUB 0x10 → A == 0x20, N set
        let mut cpu = cpu_with_program(&[0x3E, 0x30, 0xD6, 0x10]);
        cpu.tick(); cpu.tick();
        assert_eq!(cpu.regs.a, 0x20);
        assert!(cpu.regs.flag_n());
    }

    #[test]
    fn test_sub_sets_zero_when_equal() {
        // LD A,0x42; SUB 0x42 → Z set
        let mut cpu = cpu_with_program(&[0x3E, 0x42, 0xD6, 0x42]);
        cpu.tick(); cpu.tick();
        assert_eq!(cpu.regs.a, 0x00);
        assert!(cpu.regs.flag_z());
    }

    #[test]
    fn test_xor_a_a_clears_a_and_sets_z() {
        // LD A,0xFF; XOR A → A == 0x00, Z set
        let mut cpu = cpu_with_program(&[0x3E, 0xFF, 0xAF]);
        cpu.tick(); cpu.tick();
        assert_eq!(cpu.regs.a, 0x00);
        assert!(cpu.regs.flag_z());
    }

    #[test]
    fn test_cp_does_not_change_a() {
        // LD A,0x42; CP 0x42 → A still 0x42, Z set
        let mut cpu = cpu_with_program(&[0x3E, 0x42, 0xFE, 0x42]);
        cpu.tick(); cpu.tick();
        assert_eq!(cpu.regs.a, 0x42);
        assert!(cpu.regs.flag_z());
    }

    // ── JP / JR ──────────────────────────────────────────────────────────────

    #[test]
    fn test_jp_nn_unconditional() {
        // JP 0xC010 → PC == 0xC010
        let mut cpu = cpu_with_program(&[0xC3, 0x10, 0xC0]);
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC010);
    }

    #[test]
    fn test_jp_nn_costs_16_cycles() {
        let mut cpu = cpu_with_program(&[0xC3, 0x00, 0xC0]);
        let t = cpu.tick();
        assert_eq!(t, 16);
    }

    #[test]
    fn test_jp_z_taken_when_z_set() {
        let mut cpu = cpu_with_program(&[0xCA, 0x20, 0xC0]);
        cpu.regs.f = flags::Z;
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC020);
    }

    #[test]
    fn test_jp_z_not_taken_when_z_clear() {
        let mut cpu = cpu_with_program(&[0xCA, 0x20, 0xC0]);
        cpu.regs.f = 0x00;
        let start = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, start + 3); // fell through
    }

    #[test]
    fn test_jp_nz_taken_when_z_clear() {
        let mut cpu = cpu_with_program(&[0xC2, 0x50, 0xC0]);
        cpu.regs.f = 0x00;
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC050);
    }

    #[test]
    fn test_jr_forward() {
        // JR +4 from 0xC000 → PC == 0xC000 + 2 (opcode+operand) + 4 == 0xC006
        let mut cpu = cpu_with_program(&[0x18, 0x04]);
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC006);
    }

    #[test]
    fn test_jr_backward() {
        // JR -2 → loops back 2 bytes from the address after the instruction
        let mut cpu = cpu_with_program(&[0x18, 0xFE_u8]); // 0xFE == -2i8
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC000); // wraps back to start
    }

    #[test]
    fn test_jr_nz_taken_when_z_clear() {
        let mut cpu = cpu_with_program(&[0x20, 0x03]);
        cpu.regs.f = 0x00;
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC000 + 2 + 3);
    }

    #[test]
    fn test_jr_nz_not_taken_when_z_set() {
        let mut cpu = cpu_with_program(&[0x20, 0x03]);
        cpu.regs.f = flags::Z;
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC000 + 2); // not jumped
    }

    // ── PUSH / POP ───────────────────────────────────────────────────────────

    #[test]
    fn test_push_bc_pop_de_round_trip() {
        // LD BC,0x1234; PUSH BC; POP DE → DE == 0x1234
        let mut cpu = cpu_with_program(&[0x01, 0x34, 0x12, 0xC5, 0xD1]);
        cpu.tick(); // LD BC
        cpu.tick(); // PUSH BC
        cpu.tick(); // POP DE
        assert_eq!(cpu.regs.de(), 0x1234);
    }

    #[test]
    fn test_push_decrements_sp_by_2() {
        let mut cpu = cpu_with_program(&[0x01, 0x00, 0x00, 0xC5]);
        let sp_before = cpu.regs.sp;
        cpu.tick(); // LD BC
        cpu.tick(); // PUSH BC
        assert_eq!(cpu.regs.sp, sp_before - 2);
    }

    #[test]
    fn test_pop_increments_sp_by_2() {
        let mut cpu = cpu_with_program(&[0x01, 0x00, 0x00, 0xC5, 0xD1]);
        cpu.tick(); // LD BC
        cpu.tick(); // PUSH BC
        let sp_after_push = cpu.regs.sp;
        cpu.tick(); // POP DE
        assert_eq!(cpu.regs.sp, sp_after_push + 2);
    }

    #[test]
    fn test_push_pop_af_preserves_flags() {
        // PUSH AF; modify A; POP AF → flags restored
        let mut cpu = cpu_with_program(&[0xF5, 0x3E, 0x00, 0xF1]);
        cpu.regs.a = 0xAB;
        cpu.regs.f = flags::Z | flags::C;
        cpu.tick(); // PUSH AF
        cpu.tick(); // LD A, 0x00  (clobbers A)
        cpu.tick(); // POP AF
        assert_eq!(cpu.regs.a, 0xAB);
        assert!(cpu.regs.flag_z());
        assert!(cpu.regs.flag_c());
    }

    // ── CALL / RET ───────────────────────────────────────────────────────────

    #[test]
    fn test_call_pushes_return_address_and_jumps() {
        // At 0xC000: CALL 0xC010  (3 bytes: CD 10 C0)
        // Return address = 0xC003
        let mut cpu = cpu_with_program(&[0xCD, 0x10, 0xC0]);
        let sp_before = cpu.regs.sp;
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC010, "PC should jump to callee");
        assert_eq!(cpu.regs.sp, sp_before - 2, "SP should decrement by 2");
        let ret_addr = cpu.mmu.read_word(cpu.regs.sp);
        assert_eq!(ret_addr, 0xC003, "Return address on stack should be 0xC003");
    }

    #[test]
    fn test_call_costs_24_cycles() {
        let mut cpu = cpu_with_program(&[0xCD, 0x00, 0xC0]);
        let t = cpu.tick();
        assert_eq!(t, 24);
    }

    #[test]
    fn test_ret_pops_return_address() {
        // CALL then RET should restore PC
        let mut cpu = cpu_with_program(&[0xCD, 0x05, 0xC0, 0x00, 0x00, 0xC9]);
        //                               ^CALL 0xC005               ^RET at 0xC005
        cpu.tick(); // CALL → PC = 0xC005
        cpu.tick(); // RET  → PC = 0xC003 (return address)
        assert_eq!(cpu.regs.pc, 0xC003);
    }

    #[test]
    fn test_ret_costs_16_cycles() {
        // Set up a return address manually on the stack
        let mut cpu = cpu_with_program(&[0xC9]);
        cpu.regs.sp = cpu.regs.sp.wrapping_sub(2);
        cpu.mmu.write_word(cpu.regs.sp, 0xC100);
        let t = cpu.tick();
        assert_eq!(t, 16);
        assert_eq!(cpu.regs.pc, 0xC100);
    }

    #[test]
    fn test_call_ret_full_round_trip() {
        // Layout:
        // 0xC000: CALL 0xC006   (CD 06 C0)
        // 0xC003: NOP            (00) ← execution continues here after RET
        // 0xC004: NOP            (00)
        // 0xC005: NOP            (00)
        // 0xC006: NOP            (00) ← subroutine body
        // 0xC007: RET            (C9)
        let mut cpu = cpu_with_program(&[0xCD, 0x06, 0xC0, 0x00, 0x00, 0x00, 0x00, 0xC9]);
        cpu.tick(); // CALL → PC = 0xC006
        assert_eq!(cpu.regs.pc, 0xC006);
        cpu.tick(); // NOP in subroutine
        cpu.tick(); // RET → PC = 0xC003
        assert_eq!(cpu.regs.pc, 0xC003);
    }

    #[test]
    fn test_conditional_call_nz_not_taken_when_z_set() {
        let mut cpu = cpu_with_program(&[0xC4, 0x00, 0xD0]);
        cpu.regs.f = flags::Z;
        let start_sp = cpu.regs.sp;
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC003, "PC should fall through");
        assert_eq!(cpu.regs.sp, start_sp, "SP must not change");
    }

    #[test]
    fn test_conditional_ret_z_not_taken_when_z_clear() {
        let mut cpu = cpu_with_program(&[0xC8]);
        cpu.regs.f = 0x00; // Z clear
        let pc_before = cpu.regs.pc;
        cpu.tick();
        assert_eq!(cpu.regs.pc, pc_before + 1, "RET Z must not fire when Z clear");
    }

    // ── DI / EI ──────────────────────────────────────────────────────────────

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
}