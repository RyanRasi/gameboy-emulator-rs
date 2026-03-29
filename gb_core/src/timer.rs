//! Game Boy Timer subsystem.
//!
//! Four memory-mapped registers:
//!
//!   0xFF04  DIV  — Divider. Upper byte of a 16-bit internal counter.
//!                  Increments every T-cycle. Any write resets to 0.
//!   0xFF05  TIMA — Timer Counter. Increments at the frequency set by TAC.
//!                  On overflow (past 0xFF), reloads from TMA and requests
//!                  a Timer interrupt (bit 2 of IF).
//!   0xFF06  TMA  — Timer Modulo. Reload value for TIMA on overflow.
//!   0xFF07  TAC  — Timer Control.
//!                  Bit 2:   Timer enable (1 = running).
//!                  Bits 1–0: Clock select:
//!                    00 → 4096 Hz   (1024 T-cycles per TIMA tick)
//!                    01 → 262144 Hz (  16 T-cycles per TIMA tick)
//!                    10 → 65536 Hz  (  64 T-cycles per TIMA tick)
//!                    11 → 16384 Hz  ( 256 T-cycles per TIMA tick)

use crate::mmu::Mmu;

pub const DIV_ADDR:  u16 = 0xFF04;
pub const TIMA_ADDR: u16 = 0xFF05;
pub const TMA_ADDR:  u16 = 0xFF06;
pub const TAC_ADDR:  u16 = 0xFF07;

/// Return the number of T-cycles between each TIMA increment for a
/// given TAC register value (only bits 1–0 are used).
pub fn tima_period(tac: u8) -> u32 {
    match tac & 0x03 {
        0b00 => 1024,
        0b01 => 16,
        0b10 => 64,
        0b11 => 256,
        _    => unreachable!(),
    }
}

pub struct Timer {
    /// 16-bit internal divider counter.
    /// Increments every T-cycle; upper byte is the DIV register (0xFF04).
    div_counter: u16,

    /// Accumulated T-cycles since the last TIMA increment.
    tima_counter: u32,
}

impl Timer {
    pub fn new() -> Self {
        Timer {
            div_counter:  0,
            tima_counter: 0,
        }
    }

    /// Current value of the DIV register (upper byte of the internal counter).
    pub fn div(&self) -> u8 {
        (self.div_counter >> 8) as u8
    }

    /// Reset the internal divider counter.
    /// Must be called whenever software writes any value to 0xFF04.
    pub fn reset_div(&mut self) {
        self.div_counter  = 0;
        self.tima_counter = 0; // internal clock resets too
    }

    /// Advance the timer by `cycles` T-cycles.
    ///
    /// - Always increments the DIV counter and syncs 0xFF04 in the MMU.
    /// - If the timer is enabled (TAC bit 2), increments TIMA at the
    ///   configured frequency; handles overflow / TMA reload.
    ///
    /// Returns `true` if TIMA overflowed and a Timer interrupt should be
    /// requested. The caller is responsible for calling
    /// `interrupts::request(&mut mmu, source::TIMER)`.
    pub fn step(&mut self, cycles: u32, mmu: &mut Mmu) -> bool {
        // ── DIV ──────────────────────────────────────────────────────────────
        self.div_counter = self.div_counter.wrapping_add(cycles as u16);
        mmu.write_byte(DIV_ADDR, self.div());

        // ── TIMA ─────────────────────────────────────────────────────────────
        let tac = mmu.read_byte(TAC_ADDR);
        if tac & 0x04 == 0 {
            return false; // timer disabled
        }

        let period = tima_period(tac);
        self.tima_counter += cycles;

        let mut interrupt = false;

        while self.tima_counter >= period {
            self.tima_counter -= period;

            let tima = mmu.read_byte(TIMA_ADDR);
            if tima == 0xFF {
                // Overflow: reload from TMA, request Timer interrupt
                let tma = mmu.read_byte(TMA_ADDR);
                mmu.write_byte(TIMA_ADDR, tma);
                interrupt = true;
            } else {
                mmu.write_byte(TIMA_ADDR, tima + 1);
            }
        }

        interrupt
    }
}

impl Default for Timer {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Mmu;

    // Helpers
    fn setup() -> (Timer, Mmu) {
        (Timer::new(), Mmu::new())
    }

    fn enabled_tac(bits: u8) -> u8 {
        0x04 | (bits & 0x03) // timer enable bit + clock select
    }

    // ── tima_period ───────────────────────────────────────────────────────────

    #[test]
    fn test_tima_period_00_is_1024() {
        assert_eq!(tima_period(0b00), 1024);
    }

    #[test]
    fn test_tima_period_01_is_16() {
        assert_eq!(tima_period(0b01), 16);
    }

    #[test]
    fn test_tima_period_10_is_64() {
        assert_eq!(tima_period(0b10), 64);
    }

    #[test]
    fn test_tima_period_11_is_256() {
        assert_eq!(tima_period(0b11), 256);
    }

    // ── DIV ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_div_starts_at_zero() {
        let timer = Timer::new();
        assert_eq!(timer.div(), 0);
    }

    #[test]
    fn test_div_in_mmu_after_256_cycles() {
        let (mut timer, mut mmu) = setup();
        timer.step(256, &mut mmu);
        assert_eq!(mmu.read_byte(DIV_ADDR), 1);
    }

    #[test]
    fn test_div_not_incremented_before_256_cycles() {
        let (mut timer, mut mmu) = setup();
        timer.step(255, &mut mmu);
        assert_eq!(mmu.read_byte(DIV_ADDR), 0);
    }

    #[test]
    fn test_div_increments_multiple_times() {
        let (mut timer, mut mmu) = setup();
        timer.step(512, &mut mmu);
        assert_eq!(mmu.read_byte(DIV_ADDR), 2);
    }

    #[test]
    fn test_div_accumulates_across_small_steps() {
        let (mut timer, mut mmu) = setup();
        for _ in 0..256 {
            timer.step(1, &mut mmu);
        }
        assert_eq!(mmu.read_byte(DIV_ADDR), 1);
    }

    #[test]
    fn test_div_wraps_at_256_increments() {
        let (mut timer, mut mmu) = setup();
        // 256 increments × 256 cycles each = 65536 T-cycles → u16 wraps to 0
        timer.step(256u32 * 256, &mut mmu);
        assert_eq!(mmu.read_byte(DIV_ADDR), 0);
    }

    #[test]
    fn test_reset_div_clears_counter() {
        let (mut timer, mut mmu) = setup();
        timer.step(200, &mut mmu);
        timer.reset_div();
        assert_eq!(timer.div(), 0);
    }

    #[test]
    fn test_reset_div_requires_full_period_before_next_increment() {
        let (mut timer, mut mmu) = setup();
        timer.step(200, &mut mmu);
        timer.reset_div();
        // Only 255 more cycles — not enough for a full 256-cycle increment
        timer.step(255, &mut mmu);
        assert_eq!(mmu.read_byte(DIV_ADDR), 0);
    }

    #[test]
    fn test_reset_div_also_resets_tima_counter() {
        // With period=16 and 8 accumulated cycles, reset should
        // require a full 16 more before next TIMA increment.
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR, enabled_tac(0b01)); // period=16
        timer.step(8, &mut mmu); // 8 cycles accumulated
        timer.reset_div();       // resets tima_counter too
        timer.step(15, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 0, "TIMA must not increment with <16 cycles after reset");
    }

    // ── TIMA disabled ────────────────────────────────────────────────────────

    #[test]
    fn test_tima_does_not_increment_when_disabled() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR, 0x00); // enable bit clear
        timer.step(100_000, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 0);
    }

    #[test]
    fn test_step_returns_false_when_timer_disabled() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR, 0x00);
        assert!(!timer.step(100_000, &mut mmu));
    }

    // ── TIMA frequency ───────────────────────────────────────────────────────

    #[test]
    fn test_tima_increments_at_4096hz() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR, enabled_tac(0b00)); // 1024 T-cycles
        timer.step(1024, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 1);
    }

    #[test]
    fn test_tima_no_increment_before_4096hz_period() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR, enabled_tac(0b00));
        timer.step(1023, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 0);
    }

    #[test]
    fn test_tima_increments_at_262144hz() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR, enabled_tac(0b01)); // 16 T-cycles
        timer.step(16, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 1);
    }

    #[test]
    fn test_tima_increments_at_65536hz() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR, enabled_tac(0b10)); // 64 T-cycles
        timer.step(64, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 1);
    }

    #[test]
    fn test_tima_increments_at_16384hz() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR, enabled_tac(0b11)); // 256 T-cycles
        timer.step(256, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 1);
    }

    #[test]
    fn test_tima_increments_multiple_times_in_one_step() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR, enabled_tac(0b00)); // period = 1024
        timer.step(1024 * 5, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 5);
    }

    #[test]
    fn test_tima_accumulates_across_small_steps() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR, enabled_tac(0b00)); // period = 1024
        for _ in 0..1024 {
            timer.step(1, &mut mmu);
        }
        assert_eq!(mmu.read_byte(TIMA_ADDR), 1);
    }

    // ── TIMA overflow ─────────────────────────────────────────────────────────

    #[test]
    fn test_tima_overflow_reloads_from_tma() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR,  enabled_tac(0b01)); // period = 16
        mmu.write_byte(TMA_ADDR,  0x42);
        mmu.write_byte(TIMA_ADDR, 0xFF);
        timer.step(16, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 0x42);
    }

    #[test]
    fn test_tima_overflow_returns_true() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR,  enabled_tac(0b01));
        mmu.write_byte(TIMA_ADDR, 0xFF);
        assert!(timer.step(16, &mut mmu));
    }

    #[test]
    fn test_tima_no_overflow_returns_false() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR,  enabled_tac(0b01));
        mmu.write_byte(TIMA_ADDR, 0x00);
        assert!(!timer.step(16, &mut mmu));
    }

    #[test]
    fn test_tima_overflow_with_zero_tma_reloads_zero() {
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR,  enabled_tac(0b01));
        mmu.write_byte(TMA_ADDR,  0x00);
        mmu.write_byte(TIMA_ADDR, 0xFF);
        timer.step(16, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 0x00);
    }

    #[test]
    fn test_tima_multiple_overflows_in_one_step() {
        // TIMA=0xFE, period=16, step 32 T-cycles:
        //   tick 1: 0xFE → 0xFF
        //   tick 2: 0xFF overflows → reload TMA=0x00, interrupt
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR,  enabled_tac(0b01)); // period = 16
        mmu.write_byte(TMA_ADDR,  0x00);
        mmu.write_byte(TIMA_ADDR, 0xFE);
        let fired = timer.step(32, &mut mmu);
        assert!(fired);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 0x00);
    }

    #[test]
    fn test_tima_continues_from_tma_after_overflow() {
        // After overflow reload, TIMA should keep counting from TMA,
        // not from zero.
        let (mut timer, mut mmu) = setup();
        mmu.write_byte(TAC_ADDR,  enabled_tac(0b01)); // period = 16
        mmu.write_byte(TMA_ADDR,  0x10);
        mmu.write_byte(TIMA_ADDR, 0xFF);
        // Overflow tick: reloads 0x10
        timer.step(16, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 0x10);
        // One more tick: 0x10 → 0x11
        timer.step(16, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 0x11);
    }

    // ── TAC frequency behaviour ───────────────────────────────────────────────

    #[test]
    fn test_switching_tac_frequency_changes_period() {
        let (mut timer, mut mmu) = setup();
        // Start at 262144 Hz (16 T-cycles)
        mmu.write_byte(TAC_ADDR, enabled_tac(0b01));
        timer.step(16, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 1);

        // Switch to 4096 Hz (1024 T-cycles) — 16 more cycles should NOT tick
        mmu.write_byte(TAC_ADDR, enabled_tac(0b00));
        timer.step(16, &mut mmu);
        assert_eq!(mmu.read_byte(TIMA_ADDR), 1, "TIMA must not increment at new slower rate yet");
    }
}