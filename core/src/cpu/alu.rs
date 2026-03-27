//! Arithmetic Logic Unit helpers.
//!
//! All ALU operations are pure functions — they take operand(s) and the
//! current flag byte, and return a Result containing the computed value
//! and the updated flag byte.  The CPU wires these into register state.
//!
//! Keeping ALU logic separate from the instruction dispatcher makes it
//! easy to unit-test flag behaviour in isolation.

use super::registers::flags;

pub struct AluResult {
    pub value: u8,
    pub flags: u8,
}

// ─────────────────────────────────────────────────────────────────────────────
// ADD  A, x
// ─────────────────────────────────────────────────────────────────────────────

/// ADD A, x  (no carry in)
pub fn add(a: u8, x: u8) -> AluResult {
    let result = a.wrapping_add(x);
    let mut f: u8 = 0;
    if result == 0              { f |= flags::Z; }
    // N = 0
    if (a & 0x0F) + (x & 0x0F) > 0x0F { f |= flags::H; }
    if (a as u16) + (x as u16) > 0xFF  { f |= flags::C; }
    AluResult { value: result, flags: f }
}

/// ADC A, x  (add with carry)
pub fn adc(a: u8, x: u8, carry: bool) -> AluResult {
    let c = carry as u8;
    let result = a.wrapping_add(x).wrapping_add(c);
    let mut f: u8 = 0;
    if result == 0 { f |= flags::Z; }
    if (a & 0x0F) + (x & 0x0F) + c > 0x0F { f |= flags::H; }
    if (a as u16) + (x as u16) + (c as u16) > 0xFF { f |= flags::C; }
    AluResult { value: result, flags: f }
}

// ─────────────────────────────────────────────────────────────────────────────
// SUB  A, x  /  SBC  A, x  /  CP  A, x
// ─────────────────────────────────────────────────────────────────────────────

/// SUB A, x  (no borrow)
pub fn sub(a: u8, x: u8) -> AluResult {
    let result = a.wrapping_sub(x);
    let mut f: u8 = flags::N;
    if result == 0               { f |= flags::Z; }
    if (a & 0x0F) < (x & 0x0F)  { f |= flags::H; }
    if (a as u16) < (x as u16)  { f |= flags::C; }
    AluResult { value: result, flags: f }
}

/// SBC A, x  (subtract with carry/borrow)
pub fn sbc(a: u8, x: u8, carry: bool) -> AluResult {
    let c = carry as u8;
    let result = a.wrapping_sub(x).wrapping_sub(c);
    let mut f: u8 = flags::N;
    if result == 0 { f |= flags::Z; }
    if (a & 0x0F) < (x & 0x0F) + c { f |= flags::H; }
    if (a as u16) < (x as u16) + (c as u16) { f |= flags::C; }
    AluResult { value: result, flags: f }
}

/// CP A, x  — like SUB but result is discarded (only flags matter)
pub fn cp(a: u8, x: u8) -> AluResult {
    let r = sub(a, x);
    AluResult { value: a, flags: r.flags } // A unchanged
}

// ─────────────────────────────────────────────────────────────────────────────
// Bitwise
// ─────────────────────────────────────────────────────────────────────────────

/// AND A, x
pub fn and(a: u8, x: u8) -> AluResult {
    let result = a & x;
    let mut f: u8 = flags::H; // H always set
    if result == 0 { f |= flags::Z; }
    AluResult { value: result, flags: f }
}

/// OR A, x
pub fn or(a: u8, x: u8) -> AluResult {
    let result = a | x;
    let mut f: u8 = 0;
    if result == 0 { f |= flags::Z; }
    AluResult { value: result, flags: f }
}

/// XOR A, x
pub fn xor(a: u8, x: u8) -> AluResult {
    let result = a ^ x;
    let mut f: u8 = 0;
    if result == 0 { f |= flags::Z; }
    AluResult { value: result, flags: f }
}

// ─────────────────────────────────────────────────────────────────────────────
// INC / DEC  (carry flag NOT affected)
// ─────────────────────────────────────────────────────────────────────────────

/// INC r — preserves C flag, caller passes current C flag bit
pub fn inc(x: u8, old_f: u8) -> AluResult {
    let result = x.wrapping_add(1);
    let mut f: u8 = old_f & flags::C; // preserve carry
    if result == 0          { f |= flags::Z; }
    if (x & 0x0F) == 0x0F  { f |= flags::H; }
    // N = 0
    AluResult { value: result, flags: f }
}

/// DEC r — preserves C flag
pub fn dec(x: u8, old_f: u8) -> AluResult {
    let result = x.wrapping_sub(1);
    let mut f: u8 = flags::N | (old_f & flags::C); // N set, preserve carry
    if result == 0          { f |= flags::Z; }
    if (x & 0x0F) == 0x00  { f |= flags::H; }
    AluResult { value: result, flags: f }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::flags;

    // ── ADD ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_add_basic() {
        let r = add(0x10, 0x20);
        assert_eq!(r.value, 0x30);
        assert_eq!(r.flags & flags::Z, 0);
        assert_eq!(r.flags & flags::N, 0);
        assert_eq!(r.flags & flags::H, 0);
        assert_eq!(r.flags & flags::C, 0);
    }

    #[test]
    fn test_add_zero_flag() {
        let r = add(0x00, 0x00);
        assert_ne!(r.flags & flags::Z, 0);
    }

    #[test]
    fn test_add_zero_flag_on_overflow_wrap() {
        let r = add(0xFF, 0x01);
        assert_eq!(r.value, 0x00);
        assert_ne!(r.flags & flags::Z, 0);
        assert_ne!(r.flags & flags::C, 0);
    }

    #[test]
    fn test_add_half_carry() {
        let r = add(0x0F, 0x01);
        assert_ne!(r.flags & flags::H, 0);
    }

    #[test]
    fn test_add_carry() {
        let r = add(0xFF, 0x01);
        assert_ne!(r.flags & flags::C, 0);
    }

    #[test]
    fn test_add_no_carry_when_not_overflow() {
        let r = add(0x10, 0x10);
        assert_eq!(r.flags & flags::C, 0);
    }

    // ── ADC ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_adc_with_carry() {
        let r = adc(0x10, 0x10, true);
        assert_eq!(r.value, 0x21);
    }

    #[test]
    fn test_adc_carry_causes_overflow() {
        let r = adc(0xFF, 0x00, true);
        assert_eq!(r.value, 0x00);
        assert_ne!(r.flags & flags::Z, 0);
        assert_ne!(r.flags & flags::C, 0);
    }

    // ── SUB ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_sub_basic() {
        let r = sub(0x30, 0x10);
        assert_eq!(r.value, 0x20);
        assert_ne!(r.flags & flags::N, 0); // N always set on sub
        assert_eq!(r.flags & flags::Z, 0);
        assert_eq!(r.flags & flags::C, 0);
    }

    #[test]
    fn test_sub_zero_flag() {
        let r = sub(0x42, 0x42);
        assert_eq!(r.value, 0x00);
        assert_ne!(r.flags & flags::Z, 0);
        assert_ne!(r.flags & flags::N, 0);
    }

    #[test]
    fn test_sub_borrow_sets_carry() {
        let r = sub(0x00, 0x01);
        assert_ne!(r.flags & flags::C, 0);
        assert_ne!(r.flags & flags::N, 0);
    }

    #[test]
    fn test_sub_half_borrow_sets_h() {
        let r = sub(0x10, 0x01);
        assert_ne!(r.flags & flags::H, 0);
    }

    // ── SBC ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_sbc_with_carry() {
        let r = sbc(0x10, 0x05, true);
        assert_eq!(r.value, 0x0A);
    }

    // ── CP ───────────────────────────────────────────────────────────────────

    #[test]
    fn test_cp_equal_sets_z_and_n() {
        let r = cp(0x42, 0x42);
        assert_eq!(r.value, 0x42, "CP must not change A");
        assert_ne!(r.flags & flags::Z, 0);
        assert_ne!(r.flags & flags::N, 0);
    }

    #[test]
    fn test_cp_less_than_sets_c() {
        let r = cp(0x01, 0x02);
        assert_ne!(r.flags & flags::C, 0);
    }

    // ── AND / OR / XOR ───────────────────────────────────────────────────────

    #[test]
    fn test_and_result_and_flags() {
        let r = and(0b1010_1010, 0b1100_1100);
        assert_eq!(r.value, 0b1000_1000);
        assert_ne!(r.flags & flags::H, 0); // H always set
        assert_eq!(r.flags & flags::N, 0);
        assert_eq!(r.flags & flags::C, 0);
    }

    #[test]
    fn test_and_zero_sets_z() {
        let r = and(0x00, 0xFF);
        assert_ne!(r.flags & flags::Z, 0);
    }

    #[test]
    fn test_or_result() {
        let r = or(0b1010_0000, 0b0000_0101);
        assert_eq!(r.value, 0b1010_0101);
        assert_eq!(r.flags & flags::Z, 0);
    }

    #[test]
    fn test_or_zero_sets_z() {
        let r = or(0x00, 0x00);
        assert_ne!(r.flags & flags::Z, 0);
    }

    #[test]
    fn test_xor_self_gives_zero() {
        let r = xor(0xAB, 0xAB);
        assert_eq!(r.value, 0x00);
        assert_ne!(r.flags & flags::Z, 0);
    }

    #[test]
    fn test_xor_clears_n_h_c() {
        let r = xor(0xFF, 0x0F);
        assert_eq!(r.flags & flags::N, 0);
        assert_eq!(r.flags & flags::H, 0);
        assert_eq!(r.flags & flags::C, 0);
    }

    // ── INC / DEC ────────────────────────────────────────────────────────────

    #[test]
    fn test_inc_basic() {
        let r = inc(0x0E, 0x00);
        assert_eq!(r.value, 0x0F);
        assert_eq!(r.flags & flags::Z, 0);
        assert_eq!(r.flags & flags::H, 0);
    }

    #[test]
    fn test_inc_half_carry() {
        let r = inc(0x0F, 0x00);
        assert_eq!(r.value, 0x10);
        assert_ne!(r.flags & flags::H, 0);
    }

    #[test]
    fn test_inc_wraps_to_zero() {
        let r = inc(0xFF, 0x00);
        assert_eq!(r.value, 0x00);
        assert_ne!(r.flags & flags::Z, 0);
    }

    #[test]
    fn test_inc_preserves_carry() {
        let r = inc(0x00, flags::C);
        assert_ne!(r.flags & flags::C, 0);
    }

    #[test]
    fn test_dec_basic() {
        let r = dec(0x10, 0x00);
        assert_eq!(r.value, 0x0F);
        assert_ne!(r.flags & flags::N, 0);
        assert_ne!(r.flags & flags::H, 0); // borrow from nibble
    }

    #[test]
    fn test_dec_to_zero() {
        let r = dec(0x01, 0x00);
        assert_eq!(r.value, 0x00);
        assert_ne!(r.flags & flags::Z, 0);
        assert_ne!(r.flags & flags::N, 0);
    }

    #[test]
    fn test_dec_preserves_carry() {
        let r = dec(0x05, flags::C);
        assert_ne!(r.flags & flags::C, 0);
    }
}