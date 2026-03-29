//! Game Boy APU — Audio Processing Unit.
//!
//! Implements CH1 (square + sweep) and CH2 (square) with full volume
//! envelopes and length counters. CH3 (wave) and CH4 (noise) produce
//! silence in this phase — stubs ready for future extension.
//!
//! The APU accumulates f32 stereo samples (interleaved L/R) at `SAMPLE_RATE`.
//! Call `drain_samples()` after each frame to retrieve them.
//!
//! Frame sequencer (512 Hz = every 8192 T-cycles):
//!   Step 0: length
//!   Step 2: length + sweep
//!   Step 4: length
//!   Step 6: length + sweep
//!   Step 7: volume envelope

pub mod square;

use square::SquareChannel;
use crate::mmu::Mmu;

/// Output sample rate (Hz).
pub const SAMPLE_RATE: u32 = 44_100;
const CPU_FREQ: f64        = 4_194_304.0;
/// T-cycles per audio sample at the default sample rate.
pub const CYCLES_PER_SAMPLE: f64 = CPU_FREQ / SAMPLE_RATE as f64; // ≈ 95.1
/// Frame sequencer period: fires at 512 Hz.
const FRAME_SEQ_PERIOD: u32 = 8_192;

// ── APU register addresses ────────────────────────────────────────────────────
pub const NR10_ADDR: u16 = 0xFF10; // CH1 sweep
pub const NR11_ADDR: u16 = 0xFF11; // CH1 duty + length
pub const NR12_ADDR: u16 = 0xFF12; // CH1 volume envelope
pub const NR13_ADDR: u16 = 0xFF13; // CH1 frequency lo
pub const NR14_ADDR: u16 = 0xFF14; // CH1 frequency hi + trigger
pub const NR21_ADDR: u16 = 0xFF16; // CH2 duty + length
pub const NR22_ADDR: u16 = 0xFF17; // CH2 volume envelope
pub const NR23_ADDR: u16 = 0xFF18; // CH2 frequency lo
pub const NR24_ADDR: u16 = 0xFF19; // CH2 frequency hi + trigger
pub const NR50_ADDR: u16 = 0xFF24; // Master volume (left 6-4, right 2-0)
pub const NR51_ADDR: u16 = 0xFF25; // Sound panning
pub const NR52_ADDR: u16 = 0xFF26; // Sound on/off (bit 7)

pub struct Apu {
    /// Square wave channel 1 (sweep capable).
    pub ch1: SquareChannel,
    /// Square wave channel 2.
    pub ch2: SquareChannel,

    // ── CH1 frequency sweep ───────────────────────────────────────────────────
    ch1_sweep_shadow:  u16,
    ch1_sweep_timer:   u8,
    ch1_sweep_enabled: bool,

    // ── Frame sequencer ───────────────────────────────────────────────────────
    frame_seq_counter: u32,
    frame_seq_step:    u8,

    // ── Sample generation ─────────────────────────────────────────────────────
    sample_acc:        f64,
    cycles_per_sample: f64,

    /// Stereo f32 sample buffer (interleaved L/R). Drain with `drain_samples`.
    pub sample_buffer: Vec<f32>,

    /// True when NR52 bit 7 is set (APU master enable).
    pub apu_enabled: bool,
}

impl Apu {
    pub fn new() -> Self {
        Apu {
            ch1: SquareChannel::new(),
            ch2: SquareChannel::new(),
            ch1_sweep_shadow:  0,
            ch1_sweep_timer:   0,
            ch1_sweep_enabled: false,
            frame_seq_counter: 0,
            frame_seq_step:    0,
            sample_acc:        0.0,
            cycles_per_sample: CYCLES_PER_SAMPLE,
            sample_buffer:     Vec::with_capacity(2048),
            apu_enabled:       false,
        }
    }

    /// Override the output sample rate (call once after discovering the
    /// audio device's native rate so the APU generates the right number
    /// of samples per frame).
    pub fn set_sample_rate(&mut self, rate: u32) {
        self.cycles_per_sample = CPU_FREQ / rate as f64;
    }

    /// Advance the APU by `cycles` T-cycles.
    /// Reads/writes APU registers in the MMU as needed.
    /// Appends generated stereo samples to `sample_buffer`.
    pub fn step(&mut self, cycles: u32, mmu: &mut Mmu) {
        // 1. Sync register state and detect triggers
        self.sync_registers(mmu);

        // 2. Frame sequencer
        self.frame_seq_counter += cycles;
        while self.frame_seq_counter >= FRAME_SEQ_PERIOD {
            self.frame_seq_counter -= FRAME_SEQ_PERIOD;
            self.clock_frame_sequencer();
        }

        // 3. Step channel frequency timers
        if self.apu_enabled {
            self.ch1.step(cycles);
            self.ch2.step(cycles);
        }

        // 4. Generate samples
        self.sample_acc += cycles as f64;
        while self.sample_acc >= self.cycles_per_sample {
            self.sample_acc -= self.cycles_per_sample;
            self.push_sample(mmu);
        }
    }

    /// Drain and return all accumulated samples (clears the buffer).
    pub fn drain_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.sample_buffer)
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    /// Read APU registers from the MMU and update internal channel state.
    /// Trigger bits (NR14/NR24 bit 7) are consumed and cleared on detection.
    fn sync_registers(&mut self, mmu: &mut Mmu) {
        let nr52 = mmu.read_byte(NR52_ADDR);
        self.apu_enabled = nr52 & 0x80 != 0;
        if !self.apu_enabled { return; }

        // ── Channel 1 ─────────────────────────────────────────────────────────
        let nr11 = mmu.read_byte(NR11_ADDR);
        let nr12 = mmu.read_byte(NR12_ADDR);
        let nr13 = mmu.read_byte(NR13_ADDR);
        let nr14 = mmu.read_byte(NR14_ADDR);

        self.ch1.duty           = (nr11 >> 6) & 0x03;
        self.ch1.initial_volume = nr12 >> 4;
        self.ch1.env_add        = nr12 & 0x08 != 0;
        self.ch1.env_period     = nr12 & 0x07;
        self.ch1.dac_enabled    = nr12 & 0xF8 != 0;
        self.ch1.frequency      = ((nr14 as u16 & 0x07) << 8) | nr13 as u16;
        self.ch1.length_enabled = nr14 & 0x40 != 0;

        if nr14 & 0x80 != 0 {
            // Clear trigger bit so we only fire once per write
            mmu.write_byte(NR14_ADDR, nr14 & 0x7F);
            // Load length counter from NR11 [5:0]
            self.ch1.length_counter = 64u16.saturating_sub((nr11 & 0x3F) as u16);
            // Initialise sweep shadow register
            let nr10 = mmu.read_byte(NR10_ADDR);
            self.ch1_sweep_shadow  = self.ch1.frequency;
            let sweep_period       = (nr10 >> 4) & 0x07;
            let sweep_shift        = nr10 & 0x07;
            self.ch1_sweep_timer   = if sweep_period == 0 { 8 } else { sweep_period };
            self.ch1_sweep_enabled = sweep_period != 0 || sweep_shift != 0;
            self.ch1.trigger();
        }

        // ── Channel 2 ─────────────────────────────────────────────────────────
        let nr21 = mmu.read_byte(NR21_ADDR);
        let nr22 = mmu.read_byte(NR22_ADDR);
        let nr23 = mmu.read_byte(NR23_ADDR);
        let nr24 = mmu.read_byte(NR24_ADDR);

        self.ch2.duty           = (nr21 >> 6) & 0x03;
        self.ch2.initial_volume = nr22 >> 4;
        self.ch2.env_add        = nr22 & 0x08 != 0;
        self.ch2.env_period     = nr22 & 0x07;
        self.ch2.dac_enabled    = nr22 & 0xF8 != 0;
        self.ch2.frequency      = ((nr24 as u16 & 0x07) << 8) | nr23 as u16;
        self.ch2.length_enabled = nr24 & 0x40 != 0;

        if nr24 & 0x80 != 0 {
            mmu.write_byte(NR24_ADDR, nr24 & 0x7F);
            self.ch2.length_counter = 64u16.saturating_sub((nr21 & 0x3F) as u16);
            self.ch2.trigger();
        }
    }

    fn clock_frame_sequencer(&mut self) {
        match self.frame_seq_step {
            0 | 4 => {
                self.ch1.clock_length();
                self.ch2.clock_length();
            }
            2 | 6 => {
                self.ch1.clock_length();
                self.ch2.clock_length();
                self.clock_sweep();
            }
            7 => {
                self.ch1.clock_envelope();
                self.ch2.clock_envelope();
            }
            _ => {}
        }
        self.frame_seq_step = (self.frame_seq_step + 1) % 8;
    }

    /// CH1 frequency sweep tick (called at steps 2 and 6, 128 Hz).
    fn clock_sweep(&mut self) {
        if self.ch1_sweep_timer > 0 {
            self.ch1_sweep_timer -= 1;
        }
        if self.ch1_sweep_timer == 0 && self.ch1_sweep_enabled {
            self.ch1_sweep_timer = 8; // reload (simplified; full negate logic omitted)
        }
    }

    /// Mix current channel outputs and push one stereo sample pair.
    fn push_sample(&mut self, mmu: &Mmu) {
        if !self.apu_enabled {
            self.sample_buffer.push(0.0);
            self.sample_buffer.push(0.0);
            return;
        }

        let nr50 = mmu.read_byte(NR50_ADDR);
        let nr51 = mmu.read_byte(NR51_ADDR);

        let left_vol  = ((nr50 >> 4) & 0x07) as f32 / 7.0;
        let right_vol = (nr50 & 0x07) as f32 / 7.0;

        let ch1 = self.ch1.sample();
        let ch2 = self.ch2.sample();
        // CH3 and CH4 are stubs — they contribute 0.0
        let ch3 = 0.0f32;
        let ch4 = 0.0f32;

        // NR51 panning (bit numbering):
        //   7 = CH4 left,  6 = CH3 left,  5 = CH2 left,  4 = CH1 left
        //   3 = CH4 right, 2 = CH3 right, 1 = CH2 right, 0 = CH1 right
        let mix_l = |ch: f32, bit: u8| -> f32 { if nr51 & (1 << bit) != 0 { ch } else { 0.0 } };
        let mix_r = |ch: f32, bit: u8| -> f32 { if nr51 & (1 << bit) != 0 { ch } else { 0.0 } };

        let left  = (mix_l(ch1, 4) + mix_l(ch2, 5) + mix_l(ch3, 6) + mix_l(ch4, 7))
                    * 0.25 * left_vol;
        let right = (mix_r(ch1, 0) + mix_r(ch2, 1) + mix_r(ch3, 2) + mix_r(ch4, 3))
                    * 0.25 * right_vol;

        self.sample_buffer.push(left.clamp(-1.0, 1.0));
        self.sample_buffer.push(right.clamp(-1.0, 1.0));
    }
}

impl Default for Apu {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Mmu;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup() -> (Apu, Mmu) {
        (Apu::new(), Mmu::new())
    }

    /// Enable the APU master switch.
    fn enable_apu(mmu: &mut Mmu) {
        mmu.write_byte(NR52_ADDR, 0x80);
        mmu.write_byte(NR50_ADDR, 0x77); // max volume both sides
        mmu.write_byte(NR51_ADDR, 0xFF); // all channels to both outputs
    }

    /// Configure and trigger CH2 with the given settings.
    /// duty: 0–3, volume: 0–15, freq_reg: 11-bit
    fn trigger_ch2(mmu: &mut Mmu, duty: u8, volume: u8, freq_reg: u16) {
        let nr21 = (duty << 6) & 0xC0;
        let nr22 = (volume << 4) | 0x08; // env_add=true so DAC stays on
        let nr23 = (freq_reg & 0xFF) as u8;
        let nr24 = 0x80 | ((freq_reg >> 8) as u8 & 0x07); // trigger bit
        mmu.write_byte(NR21_ADDR, nr21);
        mmu.write_byte(NR22_ADDR, nr22);
        mmu.write_byte(NR23_ADDR, nr23);
        mmu.write_byte(NR24_ADDR, nr24);
    }

    /// Configure and trigger CH1 with the given settings.
    fn trigger_ch1(mmu: &mut Mmu, duty: u8, volume: u8, freq_reg: u16) {
        let nr11 = (duty << 6) & 0xC0;
        let nr12 = (volume << 4) | 0x08;
        let nr13 = (freq_reg & 0xFF) as u8;
        let nr14 = 0x80 | ((freq_reg >> 8) as u8 & 0x07);
        mmu.write_byte(NR11_ADDR, nr11);
        mmu.write_byte(NR12_ADDR, nr12);
        mmu.write_byte(NR13_ADDR, nr13);
        mmu.write_byte(NR14_ADDR, nr14);
    }

    // ── Stub: silence buffer ──────────────────────────────────────────────────

    #[test]
    fn test_silence_when_apu_disabled() {
        let (mut apu, mut mmu) = setup();
        // NR52 = 0 (APU off, default)
        mmu.write_byte(NR52_ADDR, 0x00);
        apu.step(CYCLES_PER_SAMPLE as u32 * 20, &mut mmu);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty(), "Should still generate sample slots");
        assert!(
            samples.iter().all(|&s| s == 0.0),
            "All samples must be silence when APU is disabled"
        );
    }

    #[test]
    fn test_silence_when_apu_enabled_but_no_trigger() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        // APU on, no channel triggered
        apu.step(CYCLES_PER_SAMPLE as u32 * 20, &mut mmu);
        let samples = apu.drain_samples();
        assert!(
            samples.iter().all(|&s| s == 0.0),
            "No triggered channels → all silence"
        );
    }

    // ── Sample buffer properties ──────────────────────────────────────────────

    #[test]
    fn test_sample_buffer_produces_stereo_pairs() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        apu.step(CYCLES_PER_SAMPLE as u32 * 10, &mut mmu);
        let samples = apu.drain_samples();
        assert_eq!(
            samples.len() % 2, 0,
            "Sample buffer must contain an even number of values (L/R pairs)"
        );
    }

    #[test]
    fn test_drain_clears_buffer() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        apu.step(CYCLES_PER_SAMPLE as u32 * 10, &mut mmu);
        let _ = apu.drain_samples();
        assert!(apu.sample_buffer.is_empty(), "Buffer must be empty after drain");
    }

    #[test]
    fn test_step_accumulates_expected_number_of_samples() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        // 100 samples worth of T-cycles
        let target = 100usize;
        apu.step((CYCLES_PER_SAMPLE * target as f64) as u32, &mut mmu);
        let samples = apu.drain_samples();
        // Allow ±1 sample rounding
        let pairs = samples.len() / 2;
        assert!(
            pairs >= target - 1 && pairs <= target + 1,
            "Expected ~{} stereo pairs, got {}",
            target, pairs
        );
    }

    // ── CH1 waveform ──────────────────────────────────────────────────────────

    #[test]
    fn test_ch1_triggered_produces_nonzero_audio() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        trigger_ch1(&mut mmu, 2, 15, 1000);
        apu.step(CYCLES_PER_SAMPLE as u32 * 100, &mut mmu);
        let samples = apu.drain_samples();
        assert!(
            samples.iter().any(|&s| s != 0.0),
            "CH1 triggered with volume=15 must produce nonzero samples"
        );
    }

    // ── CH2 waveform ──────────────────────────────────────────────────────────

    #[test]
    fn test_ch2_triggered_produces_nonzero_audio() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        trigger_ch2(&mut mmu, 2, 15, 1000);
        apu.step(CYCLES_PER_SAMPLE as u32 * 100, &mut mmu);
        let samples = apu.drain_samples();
        assert!(
            samples.iter().any(|&s| s != 0.0),
            "CH2 triggered with volume=15 must produce nonzero samples"
        );
    }

    #[test]
    fn test_ch2_50pct_duty_produces_both_high_and_low_samples() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        // freq_reg=1000: period=(2048-1000)*32=33536 T-cycles, freq≈125 Hz
        trigger_ch2(&mut mmu, 2, 15, 1000); // duty=2=50%
        apu.step(33536 * 3, &mut mmu); // 3 full wave cycles
        let samples = apu.drain_samples();
        let lefts: Vec<f32> = samples.iter().step_by(2).copied().collect();
        let has_positive = lefts.iter().any(|&s| s > 0.01);
        let has_negative = lefts.iter().any(|&s| s < -0.01);
        assert!(has_positive, "50% duty must produce positive samples (high phase)");
        assert!(has_negative, "50% duty must produce negative samples (low phase)");
    }

    // ── Waveform channel produces expected frequency array ────────────────────

    #[test]
    fn test_square_wave_transitions_at_correct_frequency() {
        // freq_reg = 1792 (0x700)
        // Step period = (2048 - 1792) * 4 = 1024 T-cycles
        // Wave period  = 1024 * 8 = 8192 T-cycles → 512 Hz
        // Samples/cycle = 44100 / 512 ≈ 86.1
        // For 50% duty: ~43 samples low (steps 0–3), ~43 samples high (steps 4–7)
        // Over 5 cycles: expect ~10 transitions (2 per cycle)
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        // NR51 = 0x22: CH2 to left only (bit 5) and right (bit 1)
        mmu.write_byte(NR51_ADDR, 0x22);
        trigger_ch2(&mut mmu, 2, 15, 1792);

        let five_cycles = 8192u32 * 5; // 40960 T-cycles
        apu.step(five_cycles, &mut mmu);
        let samples = apu.drain_samples();

        // Extract left channel samples (even indices)
        let lefts: Vec<f32> = samples.iter().step_by(2).copied().collect();

        // Count transitions (sign changes between adjacent nonzero samples)
        let transitions = lefts.windows(2)
            .filter(|w| {
                let prev_high = w[0] > 0.01;
                let curr_high = w[1] > 0.01;
                prev_high != curr_high
            })
            .count();

        // Expect approximately 10 transitions (2/cycle × 5 cycles), ±3 tolerance
        assert!(
            transitions >= 7 && transitions <= 13,
            "Expected ~10 transitions for 5 wave cycles at 512 Hz, got {}",
            transitions
        );
    }

    #[test]
    fn test_square_wave_frequency_matches_register_value() {
        // Different freq_reg should produce a different transition rate.
        // freq_reg=1024: period=(2048-1024)*32=32768 T-cycles → 128 Hz, samples/cycle≈344
        let (mut apu1, mut mmu1) = setup();
        enable_apu(&mut mmu1);
        mmu1.write_byte(NR51_ADDR, 0x22);
        trigger_ch2(&mut mmu1, 2, 15, 1024);
        apu1.step(32768 * 3, &mut mmu1);
        let s1 = apu1.drain_samples();
        let t1 = s1.iter().step_by(2).collect::<Vec<_>>()
            .windows(2).filter(|w| (*w[0] > 0.01) != (*w[1] > 0.01)).count();

        // freq_reg=1792: 512 Hz, samples/cycle≈86
        let (mut apu2, mut mmu2) = setup();
        enable_apu(&mut mmu2);
        mmu2.write_byte(NR51_ADDR, 0x22);
        trigger_ch2(&mut mmu2, 2, 15, 1792);
        apu2.step(32768 * 3, &mut mmu2);
        let s2 = apu2.drain_samples();
        let t2 = s2.iter().step_by(2).collect::<Vec<_>>()
            .windows(2).filter(|w| (*w[0] > 0.01) != (*w[1] > 0.01)).count();

        assert!(
            t2 > t1,
            "Higher freq_reg must produce more transitions per step: t1={}, t2={}",
            t1, t2
        );
    }

    // ── Volume envelope ───────────────────────────────────────────────────────

    #[test]
    fn test_volume_envelope_decreases_amplitude_over_time() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        mmu.write_byte(NR51_ADDR, 0x22);

        // CH2: volume=15, decrease envelope, period=1 (fast)
        mmu.write_byte(NR22_ADDR, 0xF1); // vol=15, decrease, period=1
        mmu.write_byte(NR21_ADDR, 0x80); // duty=50%
        mmu.write_byte(NR23_ADDR, 0x00);
        mmu.write_byte(NR24_ADDR, 0xC7); // trigger, freq_hi=7

        // Frame sequencer step 7 clocks envelope every 8192*8=65536 T-cycles
        // With period=1, each step 7 decreases volume by 1.
        // Run for one full envelope decay (15 steps = 15 × 65536 T-cycles)
        let one_env_step = FRAME_SEQ_PERIOD * 8; // 65536 T-cycles

        // Measure average amplitude in first env step
        apu.step(one_env_step, &mut mmu);
        let early = apu.drain_samples();
        let early_rms = rms(&early);

        // Measure after several more envelope steps (volume should be lower)
        apu.step(one_env_step * 10, &mut mmu);
        let late = apu.drain_samples();
        let late_rms = rms(&late);

        assert!(
            late_rms < early_rms,
            "Volume should decrease: early_rms={:.4}, late_rms={:.4}",
            early_rms, late_rms
        );
    }

    // ── Length counter ────────────────────────────────────────────────────────

    #[test]
    fn test_length_counter_silences_channel_after_expiry() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        mmu.write_byte(NR51_ADDR, 0x22);

        // CH2: length_load=63 → length_counter=1, length_enabled=true
        mmu.write_byte(NR22_ADDR, 0xF0); // vol=15, dac on
        mmu.write_byte(NR21_ADDR, 0xFF); // duty=3, length_load=63
        mmu.write_byte(NR23_ADDR, 0x00);
        // NR24 bit 7=trigger, bit 6=length_enabled, bits 2-0=freq_hi
        mmu.write_byte(NR24_ADDR, 0xC7); // trigger + length enabled + freq_hi=7

        // After exactly one frame-sequencer length clock (8192 T-cycles at step 0),
        // length_counter hits 0 and the channel is disabled.
        // Run for 2× the frame-seq period to ensure the clock fires.
        let before_expire = FRAME_SEQ_PERIOD / 2; // 4096 T-cycles → channel still on
        apu.step(before_expire, &mut mmu);
        let early = apu.drain_samples();
        let early_nonzero = early.iter().any(|&s| s.abs() > 0.01);

        // Run past the first length clock
        apu.step(FRAME_SEQ_PERIOD * 2, &mut mmu);
        let late = apu.drain_samples();
        let late_all_zero = late.iter().all(|&s| s.abs() < 0.01);

        assert!(early_nonzero, "Channel should be audible before length expiry");
        assert!(late_all_zero, "Channel should be silent after length counter expires");
    }

    // ── set_sample_rate ───────────────────────────────────────────────────────

    #[test]
    fn test_set_sample_rate_changes_sample_count() {
        let mut apu_44  = Apu::new();
        let mut mmu_44  = Mmu::new();
        enable_apu(&mut mmu_44);
        apu_44.set_sample_rate(44_100);
        apu_44.step(70_224, &mut mmu_44); // one frame at ~60 fps
        let count_44 = apu_44.drain_samples().len() / 2;

        let mut apu_48  = Apu::new();
        let mut mmu_48  = Mmu::new();
        enable_apu(&mut mmu_48);
        apu_48.set_sample_rate(48_000);
        apu_48.step(70_224, &mut mmu_48);
        let count_48 = apu_48.drain_samples().len() / 2;

        // 48 kHz should produce more samples per frame than 44.1 kHz
        assert!(
            count_48 > count_44,
            "48 kHz ({} samples) should exceed 44.1 kHz ({} samples)",
            count_48, count_44
        );
    }

    // ── Helper ────────────────────────────────────────────────────────────────

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() { return 0.0; }
        let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }
}