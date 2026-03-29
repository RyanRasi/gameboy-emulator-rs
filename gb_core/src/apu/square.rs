//! Square wave channel — used for CH1 and CH2.
//!
//! The channel produces a square wave at a programmable frequency and duty
//! cycle, with a hardware volume envelope. CH1 also supports frequency sweep
//! (handled in the APU frame sequencer).

/// Duty cycle waveform patterns.
/// DUTY_TABLE[duty][step] → 0 (low) or 1 (high).
pub const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [0, 0, 0, 0, 0, 0, 1, 1], // 25%
    [0, 0, 0, 0, 1, 1, 1, 1], // 50%
    [1, 1, 1, 1, 1, 1, 0, 0], // 75%
];

#[derive(Clone, Debug)]
pub struct SquareChannel {
    /// Channel output active (cleared by length expiry or DAC power-off).
    pub enabled: bool,
    /// DAC powered (NRx2 bits 7–3 != 0).
    pub dac_enabled: bool,

    // ── Wave generation ───────────────────────────────────────────────────────
    /// Duty pattern index (0–3).
    pub duty: u8,
    /// Current position within the 8-step duty waveform (0–7).
    pub duty_step: u8,
    /// T-cycles remaining until the next duty step advances.
    pub freq_timer: u32,
    /// 11-bit frequency value from NRx3/NRx4.
    pub frequency: u16,

    // ── Volume envelope ───────────────────────────────────────────────────────
    pub initial_volume: u8,
    /// true = volume increases, false = decreases.
    pub env_add: bool,
    /// Envelope period in frame-sequencer steps (0 = disabled).
    pub env_period: u8,
    /// Current running volume (0–15).
    pub volume: u8,
    /// Countdown to next envelope step.
    pub env_timer: u8,
    /// false once volume has hit 0 or 15 and envelope stops.
    pub env_running: bool,

    // ── Length counter ────────────────────────────────────────────────────────
    pub length_counter: u16,
    pub length_enabled: bool,
}

impl SquareChannel {
    pub fn new() -> Self {
        SquareChannel {
            enabled: false,
            dac_enabled: false,
            duty: 0,
            duty_step: 0,
            freq_timer: 0,
            frequency: 0,
            initial_volume: 0,
            env_add: false,
            env_period: 0,
            volume: 0,
            env_timer: 0,
            env_running: false,
            length_counter: 0,
            length_enabled: false,
        }
    }

    /// Restart the channel (NRx4 bit 7 written high).
    pub fn trigger(&mut self) {
        if self.dac_enabled {
            self.enabled = true;
        }
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        self.freq_timer = (2048 - self.frequency as u32) * 4;
        self.volume      = self.initial_volume;
        self.env_timer   = if self.env_period == 0 { 8 } else { self.env_period };
        self.env_running = true;
    }

    /// Advance the frequency timer by `cycles` T-cycles.
    pub fn step(&mut self, cycles: u32) {
        if !self.enabled || self.freq_timer == 0 { return; }
        let period = (2048 - self.frequency as u32) * 4;
        if period == 0 { return; }
        let mut rem = cycles;
        while rem > 0 {
            let consume = rem.min(self.freq_timer);
            self.freq_timer -= consume;
            rem            -= consume;
            if self.freq_timer == 0 {
                self.freq_timer = period;
                self.duty_step  = (self.duty_step + 1) % 8;
            }
        }
    }

    /// Current output sample as f32 in [–1.0, +1.0].
    /// Returns 0.0 when the channel or DAC is off.
    pub fn sample(&self) -> f32 {
        if !self.enabled || !self.dac_enabled { return 0.0; }
        let high  = DUTY_TABLE[self.duty as usize][self.duty_step as usize] != 0;
        let level = if high { self.volume as f32 } else { 0.0 };
        // DMG DAC: 0 → –1.0, 15 → +1.0
        level / 7.5 - 1.0
    }

    /// Clock the length counter (frame sequencer steps 0, 2, 4, 6 → 256 Hz).
    pub fn clock_length(&mut self) {
        if self.length_enabled && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }

    /// Clock the volume envelope (frame sequencer step 7 → 64 Hz).
    pub fn clock_envelope(&mut self) {
        if self.env_period == 0 { return; }
        if self.env_timer > 0 { self.env_timer -= 1; }
        if self.env_timer == 0 {
            self.env_timer = self.env_period;
            if self.env_running {
                if self.env_add && self.volume < 15 {
                    self.volume += 1;
                } else if !self.env_add && self.volume > 0 {
                    self.volume -= 1;
                } else {
                    self.env_running = false;
                }
            }
        }
    }
}

impl Default for SquareChannel {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn triggered_ch(freq: u16, duty: u8, volume: u8) -> SquareChannel {
        let mut ch = SquareChannel::new();
        ch.frequency      = freq;
        ch.duty           = duty;
        ch.initial_volume = volume;
        ch.dac_enabled    = true;
        ch.freq_timer     = (2048 - freq as u32) * 4;
        ch.trigger();
        ch
    }

    // ── trigger ───────────────────────────────────────────────────────────────

    #[test]
    fn test_trigger_enables_channel_when_dac_on() {
        let mut ch = SquareChannel::new();
        ch.dac_enabled = true;
        ch.trigger();
        assert!(ch.enabled);
    }

    #[test]
    fn test_trigger_does_not_enable_when_dac_off() {
        let mut ch = SquareChannel::new();
        ch.dac_enabled = false;
        ch.trigger();
        assert!(!ch.enabled);
    }

    #[test]
    fn test_trigger_restores_volume() {
        let mut ch = triggered_ch(1000, 2, 12);
        ch.volume = 3; // simulate decay
        ch.trigger();
        assert_eq!(ch.volume, 12);
    }

    #[test]
    fn test_trigger_sets_length_to_64_when_zero() {
        let mut ch = SquareChannel::new();
        ch.dac_enabled    = true;
        ch.length_counter = 0;
        ch.trigger();
        assert_eq!(ch.length_counter, 64);
    }

    #[test]
    fn test_trigger_preserves_nonzero_length() {
        let mut ch = SquareChannel::new();
        ch.dac_enabled    = true;
        ch.length_counter = 20;
        ch.trigger();
        assert_eq!(ch.length_counter, 20);
    }

    #[test]
    fn test_trigger_loads_freq_timer() {
        let mut ch = SquareChannel::new();
        ch.dac_enabled = true;
        ch.frequency   = 1024;
        ch.trigger();
        assert_eq!(ch.freq_timer, (2048 - 1024) * 4);
    }

    // ── sample ────────────────────────────────────────────────────────────────

    #[test]
    fn test_sample_zero_when_disabled() {
        let ch = SquareChannel::new(); // not triggered, enabled=false
        assert_eq!(ch.sample(), 0.0);
    }

    #[test]
    fn test_sample_zero_when_dac_off() {
        let mut ch = triggered_ch(1000, 2, 15);
        ch.dac_enabled = false;
        assert_eq!(ch.sample(), 0.0);
    }

    #[test]
    fn test_sample_range_is_minus_one_to_plus_one() {
        for volume in 0..=15u8 {
            let ch = triggered_ch(1000, 2, volume);
            let s = ch.sample();
            assert!(
                (-1.0..=1.0).contains(&s),
                "volume {} → sample {} out of range",
                volume, s
            );
        }
    }

    #[test]
    fn test_sample_max_volume_high_duty_is_plus_one() {
        let mut ch = triggered_ch(1000, 2, 15);
        // Step to a high-output duty position (steps 4-7 for duty=2)
        ch.duty_step = 4;
        assert!((ch.sample() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_sample_any_volume_low_duty_is_minus_one() {
        let mut ch = triggered_ch(1000, 2, 15);
        // Steps 0-3 for duty=2 are low → output = -1.0
        ch.duty_step = 0;
        assert!((ch.sample() - (-1.0)).abs() < 1e-5);
    }

    // ── duty table ────────────────────────────────────────────────────────────

    #[test]
    fn test_duty_0_has_one_high_step() {
        let highs: usize = DUTY_TABLE[0].iter().filter(|&&v| v != 0).count();
        assert_eq!(highs, 1);
    }

    #[test]
    fn test_duty_1_has_two_high_steps() {
        let highs: usize = DUTY_TABLE[1].iter().filter(|&&v| v != 0).count();
        assert_eq!(highs, 2);
    }

    #[test]
    fn test_duty_2_has_four_high_steps() {
        let highs: usize = DUTY_TABLE[2].iter().filter(|&&v| v != 0).count();
        assert_eq!(highs, 4);
    }

    #[test]
    fn test_duty_3_has_six_high_steps() {
        let highs: usize = DUTY_TABLE[3].iter().filter(|&&v| v != 0).count();
        assert_eq!(highs, 6);
    }

    // ── step / frequency timer ────────────────────────────────────────────────

    #[test]
    fn test_step_does_nothing_when_disabled() {
        let mut ch = SquareChannel::new();
        ch.duty_step = 3;
        ch.step(100_000);
        assert_eq!(ch.duty_step, 3);
    }

    #[test]
    fn test_step_advances_duty_after_one_period() {
        // freq = 1792 → period = (2048-1792)*4 = 1024 T-cycles per step
        let mut ch = triggered_ch(1792, 2, 15);
        let initial_step = ch.duty_step;
        ch.step(1024);
        assert_eq!(ch.duty_step, (initial_step + 1) % 8);
    }

    #[test]
    fn test_step_advances_multiple_steps() {
        let mut ch = triggered_ch(1792, 2, 15);
        ch.step(1024 * 4); // four full periods
        assert_eq!(ch.duty_step, 4 % 8);
    }

    #[test]
    fn test_step_wraps_duty_step_at_8() {
        let mut ch = triggered_ch(1792, 2, 15);
        ch.step(1024 * 8); // exactly one full wave cycle
        assert_eq!(ch.duty_step, 0);
    }

    // ── length counter ────────────────────────────────────────────────────────

    #[test]
    fn test_clock_length_decrements_counter() {
        let mut ch = triggered_ch(1000, 2, 15);
        ch.length_enabled = true;
        ch.length_counter = 10;
        ch.clock_length();
        assert_eq!(ch.length_counter, 9);
    }

    #[test]
    fn test_clock_length_disables_channel_at_zero() {
        let mut ch = triggered_ch(1000, 2, 15);
        ch.length_enabled = true;
        ch.length_counter = 1;
        ch.clock_length();
        assert!(!ch.enabled);
    }

    #[test]
    fn test_clock_length_no_effect_when_disabled() {
        let mut ch = triggered_ch(1000, 2, 15);
        ch.length_enabled = false;
        ch.length_counter = 1;
        ch.clock_length();
        assert!(ch.enabled, "Length counter must not fire when length_enabled=false");
    }

    // ── volume envelope ───────────────────────────────────────────────────────

    #[test]
    fn test_clock_envelope_decreases_volume() {
        let mut ch = triggered_ch(1000, 2, 8);
        ch.env_period = 1;
        ch.env_add    = false;
        ch.env_timer  = 1;
        ch.clock_envelope();
        assert_eq!(ch.volume, 7);
    }

    #[test]
    fn test_clock_envelope_increases_volume() {
        let mut ch = triggered_ch(1000, 2, 8);
        ch.env_period = 1;
        ch.env_add    = true;
        ch.env_timer  = 1;
        ch.clock_envelope();
        assert_eq!(ch.volume, 9);
    }

    #[test]
    fn test_clock_envelope_stops_at_max() {
        let mut ch = triggered_ch(1000, 2, 15);
        ch.env_period = 1;
        ch.env_add    = true;
        ch.env_timer  = 1;
        ch.clock_envelope();
        assert_eq!(ch.volume, 15);
        assert!(!ch.env_running);
    }

    #[test]
    fn test_clock_envelope_stops_at_min() {
        let mut ch = triggered_ch(1000, 2, 0);
        ch.env_period = 1;
        ch.env_add    = false;
        ch.env_timer  = 1;
        ch.clock_envelope();
        assert_eq!(ch.volume, 0);
        assert!(!ch.env_running);
    }

    #[test]
    fn test_clock_envelope_disabled_when_period_zero() {
        let mut ch = triggered_ch(1000, 2, 8);
        ch.env_period = 0;
        ch.clock_envelope();
        assert_eq!(ch.volume, 8, "Volume must not change when env_period=0");
    }
}