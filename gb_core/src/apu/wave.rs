//! CH3 — Wave channel.
//!
//! Plays back 32 4-bit PCM samples stored in wave RAM (0xFF30–0xFF3F).
//! Games load digitised speech and instrument waveforms here.
//!
//! Timer period = (2048 − frequency) × 2 T-cycles per sample step.
//! 32 steps per wave cycle → full period = (2048 − frequency) × 64 T-cycles.

const WAVE_RAM_LEN: usize = 16; // 16 bytes = 32 nibbles

#[derive(Clone, Debug)]
pub struct WaveChannel {
    pub enabled:        bool,
    pub dac_enabled:    bool,

    /// Current position in the 32-nibble wave table (0–31).
    pub position:       u8,
    pub freq_timer:     u32,
    pub frequency:      u16,

    /// Volume code from NR32 bits 6–5:
    ///   0 = mute, 1 = 100%, 2 = 50%, 3 = 25%
    pub volume_code:    u8,

    pub length_counter: u16,
    pub length_enabled: bool,

    /// Shadow copy of wave RAM (read from MMU each sync cycle).
    pub wave_ram:       [u8; WAVE_RAM_LEN],
}

impl WaveChannel {
    pub fn new() -> Self {
        WaveChannel {
            enabled:        false,
            dac_enabled:    false,
            position:       0,
            freq_timer:     0,
            frequency:      0,
            volume_code:    0,
            length_counter: 0,
            length_enabled: false,
            wave_ram:       [0u8; WAVE_RAM_LEN],
        }
    }

    /// Restart the wave channel (NR34 bit 7 written high).
    pub fn trigger(&mut self) {
        if self.dac_enabled {
            self.enabled = true;
        }
        if self.length_counter == 0 {
            self.length_counter = 256;
        }
        self.position  = 0;
        self.freq_timer = (2048u32.saturating_sub(self.frequency as u32)) * 2;
    }

    /// Advance the frequency timer by `cycles` T-cycles.
    pub fn step(&mut self, cycles: u32) {
        if !self.enabled || !self.dac_enabled { return; }
        let period = (2048u32.saturating_sub(self.frequency as u32)) * 2;
        if period == 0 { return; }

        let mut rem = cycles;
        while rem > 0 {
            let consume     = rem.min(self.freq_timer.max(1));
            self.freq_timer  = self.freq_timer.saturating_sub(consume);
            rem             -= consume;
            if self.freq_timer == 0 {
                self.freq_timer = period;
                self.position   = (self.position + 1) % 32;
            }
        }
    }

    /// Current output sample as f32 in [–1.0, +1.0].
    pub fn sample(&self) -> f32 {
        if !self.enabled || !self.dac_enabled { return 0.0; }

        let byte_index = (self.position / 2) as usize;
        let byte       = self.wave_ram[byte_index];
        let nibble: u8 = if self.position % 2 == 0 {
            byte >> 4         // high nibble first
        } else {
            byte & 0x0F
        };

        let shifted = match self.volume_code {
            0 => 0,           // mute
            1 => nibble,      // 100%
            2 => nibble >> 1, // 50%
            3 => nibble >> 2, // 25%
            _ => 0,
        };

        // Map 0–15 → –1.0 … +1.0
        shifted as f32 / 7.5 - 1.0
    }

    pub fn clock_length(&mut self) {
        if self.length_enabled && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }
}

impl Default for WaveChannel {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn triggered_wave(freq: u16, volume_code: u8, ram: [u8; 16]) -> WaveChannel {
        let mut ch = WaveChannel::new();
        ch.dac_enabled = true;
        ch.frequency   = freq;
        ch.volume_code = volume_code;
        ch.wave_ram    = ram;
        ch.trigger();
        ch
    }

    // ── trigger ───────────────────────────────────────────────────────────────

    #[test]
    fn test_trigger_enables_when_dac_on() {
        let mut ch = WaveChannel::new();
        ch.dac_enabled = true;
        ch.trigger();
        assert!(ch.enabled);
    }

    #[test]
    fn test_trigger_does_not_enable_when_dac_off() {
        let mut ch = WaveChannel::new();
        ch.dac_enabled = false;
        ch.trigger();
        assert!(!ch.enabled);
    }

    #[test]
    fn test_trigger_resets_position_to_zero() {
        let mut ch = WaveChannel::new();
        ch.dac_enabled = true;
        ch.position    = 15;
        ch.trigger();
        assert_eq!(ch.position, 0);
    }

    #[test]
    fn test_trigger_sets_length_to_256_when_zero() {
        let mut ch = WaveChannel::new();
        ch.dac_enabled    = true;
        ch.length_counter = 0;
        ch.trigger();
        assert_eq!(ch.length_counter, 256);
    }

    #[test]
    fn test_trigger_preserves_nonzero_length() {
        let mut ch = WaveChannel::new();
        ch.dac_enabled    = true;
        ch.length_counter = 100;
        ch.trigger();
        assert_eq!(ch.length_counter, 100);
    }

    // ── sample ────────────────────────────────────────────────────────────────

    #[test]
    fn test_sample_zero_when_disabled() {
        let ch = WaveChannel::new();
        assert_eq!(ch.sample(), 0.0);
    }

    #[test]
    fn test_sample_zero_when_dac_off() {
        let mut ch = triggered_wave(1000, 1, [0xFF; 16]);
        ch.dac_enabled = false;
        assert_eq!(ch.sample(), 0.0);
    }

    #[test]
    fn test_sample_muted_when_volume_code_0() {
        let ch = triggered_wave(1000, 0, [0xFF; 16]);
        assert_eq!(ch.sample(), 0.0 / 7.5 - 1.0); // nibble=0 → -1.0... wait: 0>>4=0
        // volume_code=0 → shifted=0 → 0/7.5-1.0 = -1.0
        // but DAC with 0 output = -1.0 (not silence). Actually mute = shifted=0 = DAC min.
        // This is correct DMG behavior.
        assert!((ch.sample() - (-1.0)).abs() < 1e-5);
    }

    #[test]
    fn test_sample_100pct_volume_high_nibble() {
        // wave_ram[0] = 0xF0 → high nibble = 0xF = 15
        let mut ram = [0u8; 16];
        ram[0] = 0xF0;
        let ch = triggered_wave(1000, 1, ram); // position=0, vol=100%
        // nibble = 15, shifted = 15, sample = 15/7.5 - 1.0 = +1.0
        assert!((ch.sample() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_sample_50pct_volume_halves_nibble() {
        let mut ram = [0u8; 16];
        ram[0] = 0xF0; // high nibble = 15
        let ch = triggered_wave(1000, 2, ram); // 50% → nibble>>1 = 7
        let expected = 7.0 / 7.5 - 1.0;
        assert!((ch.sample() - expected).abs() < 1e-4);
    }

    #[test]
    fn test_sample_25pct_volume_quarter_nibble() {
        let mut ram = [0u8; 16];
        ram[0] = 0xF0; // high nibble = 15
        let ch = triggered_wave(1000, 3, ram); // 25% → nibble>>2 = 3
        let expected = 3.0 / 7.5 - 1.0;
        assert!((ch.sample() - expected).abs() < 1e-4);
    }

    #[test]
    fn test_sample_range_within_minus_one_to_plus_one() {
        let ram = [0x7F; 16]; // nibbles = 7 and 15 alternating
        for vol in 0..=3u8 {
            let ch = triggered_wave(1000, vol, ram);
            let s  = ch.sample();
            assert!(
                (-1.0..=1.0).contains(&s),
                "volume_code={} → sample {} out of range",
                vol, s
            );
        }
    }

    // ── nibble selection ──────────────────────────────────────────────────────

    #[test]
    fn test_high_nibble_at_even_position() {
        let mut ram = [0u8; 16];
        ram[0] = 0xAB; // high=0xA, low=0xB
        let mut ch = triggered_wave(1000, 1, ram);
        ch.position = 0; // even → high nibble = 0xA = 10
        let expected = 10.0 / 7.5 - 1.0;
        assert!((ch.sample() - expected).abs() < 1e-4);
    }

    #[test]
    fn test_low_nibble_at_odd_position() {
        let mut ram = [0u8; 16];
        ram[0] = 0xAB; // high=0xA, low=0xB
        let mut ch = triggered_wave(1000, 1, ram);
        ch.position = 1; // odd → low nibble = 0xB = 11
        let expected = 11.0 / 7.5 - 1.0;
        assert!((ch.sample() - expected).abs() < 1e-4);
    }

    // ── frequency timer / position stepping ───────────────────────────────────

    #[test]
    fn test_step_advances_position_after_one_period() {
        // freq=1792 → period=(2048-1792)*2=512 T-cycles per step
        let ram = [0x00u8; 16];
        let mut ch = triggered_wave(1792, 1, ram);
        ch.step(512);
        assert_eq!(ch.position, 1);
    }

    #[test]
    fn test_step_wraps_position_at_32() {
        let ram = [0x00u8; 16];
        let mut ch = triggered_wave(1792, 1, ram);
        ch.step(512 * 32); // exactly one full cycle
        assert_eq!(ch.position, 0);
    }

    #[test]
    fn test_step_does_nothing_when_disabled() {
        let mut ch = WaveChannel::new();
        ch.position = 5;
        ch.step(100_000);
        assert_eq!(ch.position, 5);
    }

    // ── length counter ────────────────────────────────────────────────────────

    #[test]
    fn test_clock_length_disables_channel_at_zero() {
        let mut ch = triggered_wave(1000, 1, [0x00; 16]);
        ch.length_enabled = true;
        ch.length_counter = 1;
        ch.clock_length();
        assert!(!ch.enabled);
    }

    #[test]
    fn test_clock_length_no_effect_when_length_disabled() {
        let mut ch = triggered_wave(1000, 1, [0x00; 16]);
        ch.length_enabled = false;
        ch.length_counter = 1;
        ch.clock_length();
        assert!(ch.enabled);
    }

    // ── PCM playback ──────────────────────────────────────────────────────────

    #[test]
    fn test_wave_ram_all_max_nibbles_produce_nonzero_samples() {
        // 0xFF = two nibbles of 15 → sample = +1.0 at 100% volume
        let ch = triggered_wave(1000, 1, [0xFF; 16]);
        assert!((ch.sample() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_wave_ram_different_nibbles_produce_different_samples() {
        let mut ram = [0u8; 16];
        ram[0] = 0xF0; // high=15, low=0
        let mut ch1 = triggered_wave(1000, 1, ram);
        ch1.position = 0; // nibble=15
        let mut ch2 = triggered_wave(1000, 1, ram);
        ch2.position = 1; // nibble=0
        assert_ne!(ch1.sample(), ch2.sample());
    }

    #[test]
    fn test_wave_playback_cycles_through_all_32_positions() {
        let mut ram = [0u8; 16];
        // Set each byte so nibbles are unique (0,1,2,3,...)
        for i in 0..16 { ram[i] = ((i * 2) as u8) << 4 | ((i * 2 + 1) as u8 & 0xF); }
        let mut ch = triggered_wave(1792, 1, ram);
        // Freq=1792: period=512 T-cycles per step
        let mut positions_seen = std::collections::HashSet::new();
        for _ in 0..32 {
            positions_seen.insert(ch.position);
            ch.step(512);
        }
        assert_eq!(positions_seen.len(), 32, "All 32 positions must be visited");
    }
}