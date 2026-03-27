//! Game Boy CPU register file.
//!
//! The SM83 (Game Boy CPU) has eight 8-bit registers arranged as four
//! 16-bit pairs:
//!
//!   AF  — Accumulator (A) + Flags (F)
//!   BC  — General purpose pair
//!   DE  — General purpose pair
//!   HL  — Memory pointer pair
//!
//! Plus two 16-bit-only registers:
//!   SP  — Stack Pointer
//!   PC  — Program Counter
//!
//! The F register encodes four flags in its upper nibble:
//!   Bit 7 — Z  Zero flag
//!   Bit 6 — N  Subtract flag
//!   Bit 5 — H  Half-carry flag
//!   Bit 4 — C  Carry flag
//!   Bits 3–0 are always zero

/// Bitmasks for the F (flags) register.
pub mod flags {
    pub const Z: u8 = 1 << 7; // Zero
    pub const N: u8 = 1 << 6; // Subtract
    pub const H: u8 = 1 << 5; // Half-carry
    pub const C: u8 = 1 << 4; // Carry
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registers {
    pub a: u8,
    pub f: u8, // lower nibble is always 0
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}

impl Registers {
    /// Power-on state for DMG after BIOS execution.
    /// These are the values the hardware holds when the BIOS hands off to the ROM.
    pub fn new() -> Self {
        Registers {
            a:  0x01,
            f:  0xB0, // Z=1, N=0, H=1, C=1
            b:  0x00,
            c:  0x13,
            d:  0x00,
            e:  0xD8,
            h:  0x01,
            l:  0x4D,
            sp: 0xFFFE,
            pc: 0x0100, // execution starts at 0x0100 (after BIOS)
        }
    }

    // -------------------------------------------------------------------------
    // 16-bit pair accessors
    // -------------------------------------------------------------------------

    pub fn af(&self) -> u16 {
        ((self.a as u16) << 8) | (self.f as u16 & 0xF0)
    }

    pub fn set_af(&mut self, value: u16) {
        self.a = (value >> 8) as u8;
        self.f = (value as u8) & 0xF0; // lower nibble always zero
    }

    pub fn bc(&self) -> u16 {
        ((self.b as u16) << 8) | self.c as u16
    }

    pub fn set_bc(&mut self, value: u16) {
        self.b = (value >> 8) as u8;
        self.c = value as u8;
    }

    pub fn de(&self) -> u16 {
        ((self.d as u16) << 8) | self.e as u16
    }

    pub fn set_de(&mut self, value: u16) {
        self.d = (value >> 8) as u8;
        self.e = value as u8;
    }

    pub fn hl(&self) -> u16 {
        ((self.h as u16) << 8) | self.l as u16
    }

    pub fn set_hl(&mut self, value: u16) {
        self.h = (value >> 8) as u8;
        self.l = value as u8;
    }

    // -------------------------------------------------------------------------
    // Flag helpers
    // -------------------------------------------------------------------------

    pub fn flag_z(&self) -> bool { self.f & flags::Z != 0 }
    pub fn flag_n(&self) -> bool { self.f & flags::N != 0 }
    pub fn flag_h(&self) -> bool { self.f & flags::H != 0 }
    pub fn flag_c(&self) -> bool { self.f & flags::C != 0 }

    pub fn set_flag_z(&mut self, v: bool) { self.set_flag(flags::Z, v); }
    pub fn set_flag_n(&mut self, v: bool) { self.set_flag(flags::N, v); }
    pub fn set_flag_h(&mut self, v: bool) { self.set_flag(flags::H, v); }
    pub fn set_flag_c(&mut self, v: bool) { self.set_flag(flags::C, v); }

    fn set_flag(&mut self, mask: u8, value: bool) {
        if value {
            self.f |= mask;
        } else {
            self.f &= !mask;
        }
        self.f &= 0xF0; // enforce lower nibble always zero
    }
}

impl Default for Registers {
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

    #[test]
    fn test_initial_pc_is_0100() {
        let r = Registers::new();
        assert_eq!(r.pc, 0x0100);
    }

    #[test]
    fn test_initial_sp_is_fffe() {
        let r = Registers::new();
        assert_eq!(r.sp, 0xFFFE);
    }

    #[test]
    fn test_af_pair_round_trip() {
        let mut r = Registers::new();
        r.set_af(0x1230);
        assert_eq!(r.a, 0x12);
        assert_eq!(r.f, 0x30); // 0x30 & 0xF0 = 0x30 — lower nibble is zero
        assert_eq!(r.af(), 0x1230);
    }

    #[test]
    fn test_f_lower_nibble_always_zero() {
        let mut r = Registers::new();
        r.set_af(0x00FF); // try to set lower nibble of F
        assert_eq!(r.f & 0x0F, 0x00, "Lower nibble of F must always be zero");
    }

    #[test]
    fn test_bc_pair_round_trip() {
        let mut r = Registers::new();
        r.set_bc(0xABCD);
        assert_eq!(r.b, 0xAB);
        assert_eq!(r.c, 0xCD);
        assert_eq!(r.bc(), 0xABCD);
    }

    #[test]
    fn test_de_pair_round_trip() {
        let mut r = Registers::new();
        r.set_de(0x1234);
        assert_eq!(r.de(), 0x1234);
    }

    #[test]
    fn test_hl_pair_round_trip() {
        let mut r = Registers::new();
        r.set_hl(0xBEEF);
        assert_eq!(r.h, 0xBE);
        assert_eq!(r.l, 0xEF);
        assert_eq!(r.hl(), 0xBEEF);
    }

    #[test]
    fn test_flag_z_set_and_clear() {
        let mut r = Registers::new();
        r.set_flag_z(true);
        assert!(r.flag_z());
        r.set_flag_z(false);
        assert!(!r.flag_z());
    }

    #[test]
    fn test_flag_n_set_and_clear() {
        let mut r = Registers::new();
        r.set_flag_n(true);
        assert!(r.flag_n());
        r.set_flag_n(false);
        assert!(!r.flag_n());
    }

    #[test]
    fn test_flag_h_set_and_clear() {
        let mut r = Registers::new();
        r.set_flag_h(true);
        assert!(r.flag_h());
        r.set_flag_h(false);
        assert!(!r.flag_h());
    }

    #[test]
    fn test_flag_c_set_and_clear() {
        let mut r = Registers::new();
        r.set_flag_c(true);
        assert!(r.flag_c());
        r.set_flag_c(false);
        assert!(!r.flag_c());
    }

    #[test]
    fn test_flags_do_not_bleed_into_each_other() {
        let mut r = Registers::new();
        r.f = 0x00;
        r.set_flag_z(true);
        r.set_flag_c(true);
        assert!(r.flag_z());
        assert!(!r.flag_n());
        assert!(!r.flag_h());
        assert!(r.flag_c());
    }

    #[test]
    fn test_flag_set_never_corrupts_lower_nibble() {
        let mut r = Registers::new();
        r.set_flag_z(true);
        r.set_flag_n(true);
        r.set_flag_h(true);
        r.set_flag_c(true);
        assert_eq!(r.f & 0x0F, 0x00, "Lower nibble of F must remain zero");
    }
}