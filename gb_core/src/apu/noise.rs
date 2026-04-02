//! CH4 — Noise channel.
//!
//! Generates pseudo-random noise using a Linear Feedback Shift Register (LFSR).
//!
//! The LFSR is 15 bits wide by default. When the "short mode" flag is set
//! (NR43 bit 3), it is locked to 7 bits, producing a more periodic tone
//! useful for metallic percussion sounds.
//!
//! Clock frequency = CPU_FREQ / (divisor × (1 << shift_clock))
//! where divisor comes from NR43 bits 2–0:
//!   0 → divisor = 8
//!   1 → divisor = 16
//!   2 → divisor = 32  … etc.
//!
//! The LFSR advances at this rate. Each step, bit 0 is XOR'd with bit 1,
//! the result is stored in bit 14 (and bit 6 in short mode), and the
//! register is right-shifted.  The output is the inverted bit 0.

#[derive(Clone, Debug)]
pub struct NoiseChannel {
    pub enabled:     bool,
    pub dac_enabled: bool,

    /// 15-bit LFSR (bits 14–0 used).
    lfsr: u16,
    /// true = 7-bit mode (bit 3 of NR43), false = 15-bit mode.
    pub short_mode: bool,

    /// T-cycles remaining until the next LFSR clock.
    freq_timer: u32,
    /// Clock shift amount (NR43 bits 7–4).
    pub shift_clock: u8,
    /// Divisor code (NR43 bits 2–0). 0 → divisor 8, N → N×16.
    pub divisor_code: u8,

    // ── Volume envelope ───────────────────────────────────────────────────────
    pub initial_volume: u8,
    pub env_add:        bool,
    pub env_period:     u8,
    pub volume:         u8,
    pub env_timer:      u8,
    pub env_running:    bool,

    // ── Length counter ────────────────────────────────────────────────────────
    pub length_counter: u16,
    pub length_enabled: bool,

    //avg_output: f32,
}

impl NoiseChannel {
    pub fn new() -> Self {
        NoiseChannel {
            enabled:      false,
            dac_enabled:  false,
            lfsr:         0x7FFF, // all bits set on power-on
            short_mode:   false,
            freq_timer:   0,
            shift_clock:  0,
            divisor_code: 0,
            initial_volume: 0,
            env_add:      false,
            env_period:   0,
            volume:       0,
            env_timer:    0,
            env_running:  false,
            length_counter: 0,
            length_enabled: false,
        }
    }

    /// Divisor from the divisor code (NR43 bits 2–0).
    fn divisor(&self) -> u32 {
        match self.divisor_code & 0x07 {
            0 => 8,
            n => (n as u32) * 16,
        }
    }

    /// Timer period in T-cycles.
    pub fn period(&self) -> u32 {
        self.divisor() << self.shift_clock
    }

    /// Restart the noise channel (NR44 bit 7).
    pub fn trigger(&mut self) {
        if self.dac_enabled {
            self.enabled = true;
        }
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        self.lfsr        = 0x7FFF;
        self.freq_timer  = self.period();
        self.volume      = self.initial_volume;
        self.env_timer   = if self.env_period == 0 { 8 } else { self.env_period };
        self.env_running = true;
    }

    /// Advance the LFSR by `cycles` T-cycles.
    pub fn step(&mut self, cycles: u32) {
        if !self.enabled || !self.dac_enabled { return; }
        let period = self.period();
        if period == 0 { return; }

        let mut rem = cycles;
        while rem > 0 {
            let consume     = rem.min(self.freq_timer.max(1));
            self.freq_timer  = self.freq_timer.saturating_sub(consume);
            rem             -= consume;
            if self.freq_timer == 0 {
                self.freq_timer = period;
                self.clock_lfsr();
            }
        }
    }

    fn clock_lfsr(&mut self) {
        let feedback = (self.lfsr ^ (self.lfsr >> 1)) & 0x01;
        self.lfsr >>= 1;
        if self.short_mode {
            // 7-bit mode: mask to 6 bits after shift, feedback to bit 6 only
            self.lfsr &= 0x003F;
            self.lfsr |= feedback << 6;
        } else {
            // 15-bit mode: feedback to bit 14
            self.lfsr |= feedback << 14;
        }
    }

    /// Current output sample as f32 in [–1.0, +1.0].
    pub fn sample(&self) -> f32 {
        if !self.enabled || !self.dac_enabled { return 0.0; }
        // Output = inverted bit 0
        let high = (self.lfsr & 0x01) == 0;
        let level = if high { self.volume as f32 } else { 0.0 };
        level / 7.5 - 1.0
    }

    pub fn clock_length(&mut self) {
        if self.length_enabled && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }

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

impl Default for NoiseChannel {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn triggered_noise(volume: u8, shift: u8, div: u8, short: bool) -> NoiseChannel {
        let mut ch = NoiseChannel::new();
        ch.dac_enabled    = true;
        ch.initial_volume = volume;
        ch.shift_clock    = shift;
        ch.divisor_code   = div;
        ch.short_mode     = short;
        ch.trigger();
        ch
    }

    // ── trigger ───────────────────────────────────────────────────────────────

    #[test]
    fn test_trigger_enables_when_dac_on() {
        let mut ch = NoiseChannel::new();
        ch.dac_enabled = true;
        ch.trigger();
        assert!(ch.enabled);
    }

    #[test]
    fn test_trigger_does_not_enable_when_dac_off() {
        let mut ch = NoiseChannel::new();
        ch.dac_enabled = false;
        ch.trigger();
        assert!(!ch.enabled);
    }

    #[test]
    fn test_trigger_resets_lfsr() {
        let mut ch = triggered_noise(15, 0, 0, false);
        ch.lfsr = 0x0001; // corrupt it
        ch.trigger();
        assert_eq!(ch.lfsr, 0x7FFF);
    }

    #[test]
    fn test_trigger_restores_volume() {
        let mut ch = triggered_noise(10, 0, 0, false);
        ch.volume = 2;
        ch.trigger();
        assert_eq!(ch.volume, 10);
    }

    #[test]
    fn test_trigger_sets_length_to_64_when_zero() {
        let mut ch = NoiseChannel::new();
        ch.dac_enabled    = true;
        ch.length_counter = 0;
        ch.trigger();
        assert_eq!(ch.length_counter, 64);
    }

    #[test]
    fn test_trigger_preserves_nonzero_length() {
        let mut ch = NoiseChannel::new();
        ch.dac_enabled    = true;
        ch.length_counter = 20;
        ch.trigger();
        assert_eq!(ch.length_counter, 20);
    }

    // ── period / divisor ─────────────────────────────────────────────────────

    #[test]
    fn test_divisor_code_0_gives_8() {
        let ch = triggered_noise(15, 0, 0, false);
        assert_eq!(ch.divisor(), 8);
    }

    #[test]
    fn test_divisor_code_1_gives_16() {
        let ch = triggered_noise(15, 0, 1, false);
        assert_eq!(ch.divisor(), 16);
    }

    #[test]
    fn test_divisor_code_7_gives_112() {
        let ch = triggered_noise(15, 0, 7, false);
        assert_eq!(ch.divisor(), 112);
    }

    #[test]
    fn test_period_includes_shift() {
        // div=0 → 8, shift=2 → period = 8 << 2 = 32
        let ch = triggered_noise(15, 2, 0, false);
        assert_eq!(ch.period(), 32);
    }

    #[test]
    fn test_period_increases_with_shift_clock() {
        let ch_lo = triggered_noise(15, 0, 0, false);
        let ch_hi = triggered_noise(15, 4, 0, false);
        assert!(ch_hi.period() > ch_lo.period());
    }

    // ── sample ────────────────────────────────────────────────────────────────

    #[test]
    fn test_sample_zero_when_disabled() {
        let ch = NoiseChannel::new();
        assert_eq!(ch.sample(), 0.0);
    }

    #[test]
    fn test_sample_zero_when_dac_off() {
        let mut ch = triggered_noise(15, 0, 0, false);
        ch.dac_enabled = false;
        assert_eq!(ch.sample(), 0.0);
    }

    #[test]
    fn test_sample_in_valid_range() {
        let ch = triggered_noise(15, 0, 0, false);
        let s = ch.sample();
        assert!((-1.0..=1.0).contains(&s));
    }

    #[test]
    fn test_sample_zero_volume_outputs_minus_one_when_high() {
        // volume=0 → level=0 → 0/7.5-1.0 = -1.0 when high, same when low
        let mut ch = triggered_noise(0, 0, 0, false);
        // Force LFSR bit 0 = 0 (high output)
        ch.lfsr = 0x7FFE;
        let s = ch.sample();
        assert!((s - (-1.0)).abs() < 1e-5);
    }

    // ── LFSR / noise output ──────────────────────────────────────────────────

    #[test]
    fn test_lfsr_changes_after_step() {
        let mut ch = triggered_noise(15, 0, 0, false); // period=8
        let initial = ch.lfsr;
        ch.step(8);
        assert_ne!(ch.lfsr, initial, "LFSR must change after one period");
    }

    #[test]
    fn test_noise_produces_varying_samples() {
        let mut ch = triggered_noise(15, 0, 0, false);
        let mut samples = Vec::new();
        for _ in 0..64 {
            samples.push(ch.sample());
            ch.step(8); // advance one period per sample
        }
        let first = samples[0];
        let all_same = samples.iter().all(|&s| s == first);
        assert!(!all_same, "Noise must produce varying samples (not constant)");
    }

    #[test]
    fn test_short_mode_repeats_pattern() {
        let mut ch_short = triggered_noise(15, 0, 0, true);
        let mut ch_long  = triggered_noise(15, 0, 0, false);

        let collect = |ch: &mut NoiseChannel| -> Vec<f32> {
            (0..256).map(|_| { let s = ch.sample(); ch.step(8); s }).collect()
        };

        let short_samples = collect(&mut ch_short);
        let long_samples  = collect(&mut ch_long);

        // Period = 127: samples[0..127] must equal samples[127..254]
        let short_repeats = short_samples[..127] == short_samples[127..254];
        let long_repeats  = long_samples[..127]  == long_samples[127..254];

        assert!(short_repeats, "7-bit LFSR must repeat with period 127");
        assert!(!long_repeats, "15-bit LFSR must not repeat within 256 steps");
    }

    #[test]
    fn test_noise_step_does_nothing_when_disabled() {
        let mut ch = NoiseChannel::new();
        ch.lfsr = 0x7FFF;
        ch.step(100_000);
        assert_eq!(ch.lfsr, 0x7FFF, "LFSR must not change when disabled");
    }

    // ── length counter ────────────────────────────────────────────────────────

    #[test]
    fn test_clock_length_disables_at_zero() {
        let mut ch = triggered_noise(15, 0, 0, false);
        ch.length_enabled = true;
        ch.length_counter = 1;
        ch.clock_length();
        assert!(!ch.enabled);
    }

    #[test]
    fn test_clock_length_no_effect_when_disabled() {
        let mut ch = triggered_noise(15, 0, 0, false);
        ch.length_enabled = false;
        ch.length_counter = 1;
        ch.clock_length();
        assert!(ch.enabled);
    }

    // ── volume envelope ───────────────────────────────────────────────────────

    #[test]
    fn test_clock_envelope_decreases_volume() {
        let mut ch = triggered_noise(8, 0, 0, false);
        ch.env_period = 1;
        ch.env_add    = false;
        ch.env_timer  = 1;
        ch.clock_envelope();
        assert_eq!(ch.volume, 7);
    }

    #[test]
    fn test_clock_envelope_increases_volume() {
        let mut ch = triggered_noise(8, 0, 0, false);
        ch.env_period = 1;
        ch.env_add    = true;
        ch.env_timer  = 1;
        ch.clock_envelope();
        assert_eq!(ch.volume, 9);
    }

    #[test]
    fn test_clock_envelope_stops_at_max() {
        let mut ch = triggered_noise(15, 0, 0, false);
        ch.env_period = 1;
        ch.env_add    = true;
        ch.env_timer  = 1;
        ch.clock_envelope();
        assert_eq!(ch.volume, 15);
        assert!(!ch.env_running);
    }

    #[test]
    fn test_clock_envelope_stops_at_min() {
        let mut ch = triggered_noise(0, 0, 0, false);
        ch.env_period = 1;
        ch.env_add    = false;
        ch.env_timer  = 1;
        ch.clock_envelope();
        assert_eq!(ch.volume, 0);
        assert!(!ch.env_running);
    }

    #[test]
    fn test_noise_produces_nonzero_rms() {
        let mut ch = triggered_noise(15, 0, 0, false);
        let samples: Vec<f32> = (0..256)
            .map(|_| { let s = ch.sample(); ch.step(8); s })
            .collect();
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / 256.0).sqrt();
        assert!(rms > 0.1, "Noise with volume=15 must have meaningful RMS");
    }
}