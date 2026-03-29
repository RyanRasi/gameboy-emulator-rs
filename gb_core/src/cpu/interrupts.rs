//! Game Boy interrupt system.
//!
//! Five interrupt sources, each with a fixed vector address:
//!
//!  Bit  Name      Vector   Priority
//!  0    VBlank    0x0040   highest
//!  1    LCD STAT  0x0048
//!  2    Timer     0x0050
//!  3    Serial    0x0058
//!  4    Joypad    0x0060   lowest
//!
//! Key registers:
//!   0xFF0F  IF — Interrupt Flag   (requested interrupts — set by hardware)
//!   0xFFFF  IE — Interrupt Enable (enabled interrupts   — set by software)
//!
//! An interrupt fires when:
//!   IME == true  AND  (IE & IF) != 0
//!
//! Servicing sequence (20 T-cycles):
//!   1. Clear IME
//!   2. Clear the serviced bit in IF
//!   3. Push current PC onto the stack
//!   4. Jump to the interrupt vector
//!
//! HALT behaviour:
//!   CPU stays halted until (IE & IF) != 0, then wakes regardless of IME.
//!   If IME is false when it wakes, the interrupt is NOT serviced — execution
//!   simply continues from the instruction after HALT (HALT bug handled in
//!   Phase 4 as "no-HALT-bug" mode; full bug optional later).

use crate::mmu::Mmu;

/// I/O address of the Interrupt Flag register.
pub const IF_ADDR: u16 = 0xFF0F;

/// I/O address of the Interrupt Enable register.
pub const IE_ADDR: u16 = 0xFFFF;

/// Bit masks for individual interrupt sources.
pub mod source {
    pub const VBLANK:   u8 = 1 << 0;
    pub const LCD_STAT: u8 = 1 << 1;
    pub const TIMER:    u8 = 1 << 2;
    pub const SERIAL:   u8 = 1 << 3;
    pub const JOYPAD:   u8 = 1 << 4;
}

/// Vector addresses for each interrupt source (in priority order).
const VECTORS: [(u8, u16); 5] = [
    (source::VBLANK,   0x0040),
    (source::LCD_STAT, 0x0048),
    (source::TIMER,    0x0050),
    (source::SERIAL,   0x0058),
    (source::JOYPAD,   0x0060),
];

/// Request an interrupt by setting the corresponding bit in IF.
pub fn request(mmu: &mut Mmu, mask: u8) {
    let current = mmu.read_byte(IF_ADDR);
    mmu.write_byte(IF_ADDR, current | mask);
}

/// Clear an interrupt request bit in IF.
pub fn acknowledge(mmu: &mut Mmu, mask: u8) {
    let current = mmu.read_byte(IF_ADDR);
    mmu.write_byte(IF_ADDR, current & !mask);
}

/// Returns the pending interrupt mask: `IE & IF & 0x1F`.
/// Non-zero means at least one interrupt is both enabled and requested.
pub fn pending(mmu: &Mmu) -> u8 {
    let ie = mmu.read_byte(IE_ADDR);
    let if_ = mmu.read_byte(IF_ADDR);
    ie & if_ & 0x1F
}

/// Check for a pending interrupt and, if IME is set, service the
/// highest-priority one.
///
/// Returns the number of T-cycles consumed:
///   - 0  if no interrupt was pending or IME was false (but HALT may still wake)
///   - 20 if an interrupt was serviced
///
/// The caller (CPU tick) is responsible for:
///   - Waking from HALT when `pending(mmu) != 0` (regardless of IME)
///   - Only calling this *after* the current instruction finishes
pub fn service(
    mmu:    &mut Mmu,
    ime:    &mut bool,
    halted: &mut bool,
    pc:     &mut u16,
    sp:     &mut u16,
) -> u32 {
    let p = pending(mmu);

    // Always wake from HALT if anything is pending
    if p != 0 {
        *halted = false;
    }

    // Only service the interrupt if IME is enabled
    if !*ime || p == 0 {
        return 0;
    }

    // Find the highest-priority pending interrupt
    for &(mask, vector) in &VECTORS {
        if p & mask != 0 {
            *ime = false;
            acknowledge(mmu, mask);

            // Push current PC onto the stack
            *sp = sp.wrapping_sub(2);
            mmu.write_word(*sp, *pc);

            // Jump to the vector
            *pc = vector;

            return 20; // interrupt dispatch costs 20 T-cycles
        }
    }

    0
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Mmu;

    fn mmu_with_ie(ie: u8) -> Mmu {
        let mut mmu = Mmu::new();
        mmu.write_byte(IE_ADDR, ie);
        mmu
    }

    // ── request / acknowledge ────────────────────────────────────────────────

    #[test]
    fn test_request_sets_if_bit() {
        let mut mmu = Mmu::new();
        request(&mut mmu, source::VBLANK);
        assert_eq!(mmu.read_byte(IF_ADDR) & source::VBLANK, source::VBLANK);
    }

    #[test]
    fn test_request_does_not_clear_other_bits() {
        let mut mmu = Mmu::new();
        mmu.write_byte(IF_ADDR, source::TIMER);
        request(&mut mmu, source::VBLANK);
        let if_ = mmu.read_byte(IF_ADDR);
        assert_ne!(if_ & source::TIMER, 0, "TIMER bit must remain set");
        assert_ne!(if_ & source::VBLANK, 0, "VBLANK bit must now be set");
    }

    #[test]
    fn test_acknowledge_clears_if_bit() {
        let mut mmu = Mmu::new();
        mmu.write_byte(IF_ADDR, source::VBLANK | source::TIMER);
        acknowledge(&mut mmu, source::VBLANK);
        let if_ = mmu.read_byte(IF_ADDR);
        assert_eq!(if_ & source::VBLANK, 0, "VBLANK bit should be cleared");
        assert_ne!(if_ & source::TIMER, 0, "TIMER bit must remain set");
    }

    // ── pending ──────────────────────────────────────────────────────────────

    #[test]
    fn test_pending_zero_when_nothing_requested() {
        let mmu = mmu_with_ie(0xFF); // all enabled, none requested
        assert_eq!(pending(&mmu), 0);
    }

    #[test]
    fn test_pending_zero_when_nothing_enabled() {
        let mut mmu = Mmu::new();
        mmu.write_byte(IE_ADDR, 0x00); // nothing enabled
        request(&mut mmu, source::VBLANK);
        assert_eq!(pending(&mmu), 0);
    }

    #[test]
    fn test_pending_nonzero_when_enabled_and_requested() {
        let mut mmu = mmu_with_ie(source::VBLANK);
        request(&mut mmu, source::VBLANK);
        assert_ne!(pending(&mmu), 0);
    }

    #[test]
    fn test_pending_masks_to_5_bits() {
        let mut mmu = Mmu::new();
        mmu.write_byte(IE_ADDR, 0xFF);
        mmu.write_byte(IF_ADDR, 0xFF);
        assert_eq!(pending(&mmu), 0x1F, "Only 5 interrupt bits are valid");
    }

    // ── service — IME disabled ───────────────────────────────────────────────

    #[test]
    fn test_service_does_nothing_when_ime_false() {
        let mut mmu = mmu_with_ie(source::VBLANK);
        request(&mut mmu, source::VBLANK);
        let mut ime    = false;
        let mut halted = false;
        let mut pc     = 0x0200u16;
        let mut sp     = 0xFFFEu16;
        let cycles = service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert_eq!(cycles, 0);
        assert_eq!(pc, 0x0200, "PC must not change");
        assert!(!ime, "IME must stay false");
    }

    #[test]
    fn test_service_wakes_halt_even_when_ime_false() {
        let mut mmu = mmu_with_ie(source::TIMER);
        request(&mut mmu, source::TIMER);
        let mut ime    = false;
        let mut halted = true;
        let mut pc     = 0x0200u16;
        let mut sp     = 0xFFFEu16;
        service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert!(!halted, "HALT must clear even if IME is false");
    }

    // ── service — IME enabled ────────────────────────────────────────────────

    #[test]
    fn test_service_vblank_jumps_to_0040() {
        let mut mmu = mmu_with_ie(source::VBLANK);
        request(&mut mmu, source::VBLANK);
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x0200u16;
        let mut sp     = 0xFFFEu16;
        let cycles = service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert_eq!(pc, 0x0040, "VBlank vector is 0x0040");
        assert_eq!(cycles, 20);
    }

    #[test]
    fn test_service_clears_ime() {
        let mut mmu = mmu_with_ie(source::VBLANK);
        request(&mut mmu, source::VBLANK);
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x0200u16;
        let mut sp     = 0xFFFEu16;
        service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert!(!ime, "IME must be cleared after servicing");
    }

    #[test]
    fn test_service_pushes_return_address_onto_stack() {
        let mut mmu = mmu_with_ie(source::VBLANK);
        request(&mut mmu, source::VBLANK);
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x1234u16;
        let mut sp     = 0xFFFEu16;
        service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert_eq!(sp, 0xFFFC, "SP should decrement by 2");
        let ret = mmu.read_word(sp);
        assert_eq!(ret, 0x1234, "Return address on stack must be old PC");
    }

    #[test]
    fn test_service_acknowledges_if_bit() {
        let mut mmu = mmu_with_ie(source::VBLANK);
        request(&mut mmu, source::VBLANK);
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x0200u16;
        let mut sp     = 0xFFFEu16;
        service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert_eq!(mmu.read_byte(IF_ADDR) & source::VBLANK, 0, "IF bit must be cleared");
    }

    #[test]
    fn test_service_timer_jumps_to_0050() {
        let mut mmu = mmu_with_ie(source::TIMER);
        request(&mut mmu, source::TIMER);
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x0300u16;
        let mut sp     = 0xFFFEu16;
        service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert_eq!(pc, 0x0050);
    }

    #[test]
    fn test_service_lcd_stat_jumps_to_0048() {
        let mut mmu = mmu_with_ie(source::LCD_STAT);
        request(&mut mmu, source::LCD_STAT);
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x0300u16;
        let mut sp     = 0xFFFEu16;
        service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert_eq!(pc, 0x0048);
    }

    #[test]
    fn test_service_joypad_jumps_to_0060() {
        let mut mmu = mmu_with_ie(source::JOYPAD);
        request(&mut mmu, source::JOYPAD);
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x0300u16;
        let mut sp     = 0xFFFEu16;
        service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert_eq!(pc, 0x0060);
    }

    // ── priority ─────────────────────────────────────────────────────────────

    #[test]
    fn test_vblank_takes_priority_over_timer() {
        let mut mmu = mmu_with_ie(source::VBLANK | source::TIMER);
        request(&mut mmu, source::VBLANK);
        request(&mut mmu, source::TIMER);
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x0200u16;
        let mut sp     = 0xFFFEu16;
        service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert_eq!(pc, 0x0040, "VBlank (bit 0) must win over Timer (bit 2)");
    }

    #[test]
    fn test_only_highest_priority_interrupt_serviced_per_call() {
        let mut mmu = mmu_with_ie(source::VBLANK | source::TIMER);
        request(&mut mmu, source::VBLANK);
        request(&mut mmu, source::TIMER);
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x0200u16;
        let mut sp     = 0xFFFEu16;
        service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        // TIMER bit must still be set in IF after one service call
        assert_ne!(
            mmu.read_byte(IF_ADDR) & source::TIMER, 0,
            "TIMER must remain pending after VBlank is serviced"
        );
    }

    // ── HALT wake ────────────────────────────────────────────────────────────

    #[test]
    fn test_halt_not_cleared_when_nothing_pending() {
        let mut mmu = Mmu::new(); // IE = 0, IF = 0
        let mut ime    = true;
        let mut halted = true;
        let mut pc     = 0x0200u16;
        let mut sp     = 0xFFFEu16;
        service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert!(halted, "HALT must NOT clear when nothing is pending");
    }

    #[test]
    fn test_service_costs_20_cycles_when_fired() {
        let mut mmu = mmu_with_ie(source::VBLANK);
        request(&mut mmu, source::VBLANK);
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x0200u16;
        let mut sp     = 0xFFFEu16;
        let t = service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert_eq!(t, 20);
    }

    #[test]
    fn test_service_costs_0_cycles_when_nothing_pending() {
        let mut mmu = Mmu::new();
        let mut ime    = true;
        let mut halted = false;
        let mut pc     = 0x0200u16;
        let mut sp     = 0xFFFEu16;
        let t = service(&mut mmu, &mut ime, &mut halted, &mut pc, &mut sp);
        assert_eq!(t, 0);
    }
}