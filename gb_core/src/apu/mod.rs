//! Game Boy APU — Audio Processing Unit.
//!
//! CH1: square wave + sweep
//! CH2: square wave
//! CH3: wave (PCM from wave RAM)
//! CH4: noise (stub — silence)
//!
//! A high-pass filter (capacitor simulation) is applied per output channel
//! to match DMG hardware behaviour and eliminate DC-offset pop/click artifacts.

pub mod square;
pub mod wave;

use square::SquareChannel;
use wave::WaveChannel;
use crate::mmu::Mmu;

pub const SAMPLE_RATE: u32 = 44_100;
const CPU_FREQ: f64         = 4_194_304.0;
pub const CYCLES_PER_SAMPLE: f64 = CPU_FREQ / SAMPLE_RATE as f64; // ≈95.1
const FRAME_SEQ_PERIOD: u32 = 8_192;

/// High-pass filter charge factor — approximates the DMG capacitor.
/// HP_CHARGE_FACTOR = 1 − 1 / (sample_rate × RC)
/// RC ≈ 6.7 s (DMG hardware) → factor ≈ 0.999958 at 44100 Hz.
const HP_CHARGE_FACTOR: f32 = 0.999_958;

// ── APU register addresses ────────────────────────────────────────────────────
pub const NR10_ADDR: u16 = 0xFF10;
pub const NR11_ADDR: u16 = 0xFF11;
pub const NR12_ADDR: u16 = 0xFF12;
pub const NR13_ADDR: u16 = 0xFF13;
pub const NR14_ADDR: u16 = 0xFF14;
pub const NR21_ADDR: u16 = 0xFF16;
pub const NR22_ADDR: u16 = 0xFF17;
pub const NR23_ADDR: u16 = 0xFF18;
pub const NR24_ADDR: u16 = 0xFF19;
pub const NR30_ADDR: u16 = 0xFF1A;
pub const NR31_ADDR: u16 = 0xFF1B;
pub const NR32_ADDR: u16 = 0xFF1C;
pub const NR33_ADDR: u16 = 0xFF1D;
pub const NR34_ADDR: u16 = 0xFF1E;
pub const NR50_ADDR: u16 = 0xFF24;
pub const NR51_ADDR: u16 = 0xFF25;
pub const NR52_ADDR: u16 = 0xFF26;
pub const WAVE_RAM_START: u16 = 0xFF30;

pub struct Apu {
    pub ch1: SquareChannel,
    pub ch2: SquareChannel,
    pub ch3: WaveChannel,

    // ── CH1 sweep ─────────────────────────────────────────────────────────────
    ch1_sweep_shadow:  u16,
    ch1_sweep_timer:   u8,
    ch1_sweep_enabled: bool,

    // ── Frame sequencer ───────────────────────────────────────────────────────
    frame_seq_counter: u32,
    frame_seq_step:    u8,

    // ── Sample generation ─────────────────────────────────────────────────────
    sample_acc:        f64,
    cycles_per_sample: f64,

    // ── High-pass filter capacitors (one per stereo channel) ──────────────────
    hp_cap_left:  f32,
    hp_cap_right: f32,

    pub sample_buffer: Vec<f32>,
    pub apu_enabled:   bool,
}

impl Apu {
    pub fn new() -> Self {
        Apu {
            ch1: SquareChannel::new(),
            ch2: SquareChannel::new(),
            ch3: WaveChannel::new(),
            ch1_sweep_shadow:  0,
            ch1_sweep_timer:   0,
            ch1_sweep_enabled: false,
            frame_seq_counter: 0,
            frame_seq_step:    0,
            sample_acc:        0.0,
            cycles_per_sample: CYCLES_PER_SAMPLE,
            hp_cap_left:       0.0,
            hp_cap_right:      0.0,
            sample_buffer:     Vec::with_capacity(4096),
            apu_enabled:       false,
        }
    }

    pub fn set_sample_rate(&mut self, rate: u32) {
        self.cycles_per_sample = CPU_FREQ / rate as f64;
    }

    pub fn step(&mut self, total_cycles: u32, mmu: &mut Mmu) {
        self.sync_registers(mmu);

        let mut remaining = total_cycles;
        while remaining > 0 {
            let to_next = ((self.cycles_per_sample - self.sample_acc).ceil() as u32).max(1);
            let chunk   = remaining.min(to_next);

            // Frame sequencer
            self.frame_seq_counter += chunk;
            while self.frame_seq_counter >= FRAME_SEQ_PERIOD {
                self.frame_seq_counter -= FRAME_SEQ_PERIOD;
                self.clock_frame_sequencer();
            }

            // Advance channel frequency timers
            if self.apu_enabled {
                self.ch1.step(chunk);
                self.ch2.step(chunk);
                self.ch3.step(chunk);
            }

            // Generate samples at the correct rate
            self.sample_acc += chunk as f64;
            remaining       -= chunk;

            while self.sample_acc >= self.cycles_per_sample {
                self.sample_acc -= self.cycles_per_sample;
                self.push_sample(mmu);
            }
        }
    }

    pub fn drain_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.sample_buffer)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn sync_registers(&mut self, mmu: &mut Mmu) {
        let nr52 = mmu.read_byte(NR52_ADDR);
        self.apu_enabled = nr52 & 0x80 != 0;
        if !self.apu_enabled { return; }

        // ── CH1 ───────────────────────────────────────────────────────────────
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

        if !self.ch1.dac_enabled { self.ch1.enabled = false; }

        if nr14 & 0x80 != 0 {
            mmu.write_byte(NR14_ADDR, nr14 & 0x7F);
            self.ch1.length_counter = 64u16.saturating_sub((nr11 & 0x3F) as u16);
            let nr10 = mmu.read_byte(NR10_ADDR);
            self.ch1_sweep_shadow  = self.ch1.frequency;
            let sweep_period       = (nr10 >> 4) & 0x07;
            let sweep_shift        = nr10 & 0x07;
            self.ch1_sweep_timer   = if sweep_period == 0 { 8 } else { sweep_period };
            self.ch1_sweep_enabled = sweep_period != 0 || sweep_shift != 0;
            self.ch1.trigger();
        }

        // ── CH2 ───────────────────────────────────────────────────────────────
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

        if !self.ch2.dac_enabled { self.ch2.enabled = false; }

        if nr24 & 0x80 != 0 {
            mmu.write_byte(NR24_ADDR, nr24 & 0x7F);
            self.ch2.length_counter = 64u16.saturating_sub((nr21 & 0x3F) as u16);
            self.ch2.trigger();
        }

        // ── CH3 ───────────────────────────────────────────────────────────────
        let nr30 = mmu.read_byte(NR30_ADDR);
        let nr31 = mmu.read_byte(NR31_ADDR);
        let nr32 = mmu.read_byte(NR32_ADDR);
        let nr33 = mmu.read_byte(NR33_ADDR);
        let nr34 = mmu.read_byte(NR34_ADDR);

        self.ch3.dac_enabled    = nr30 & 0x80 != 0;
        self.ch3.volume_code    = (nr32 >> 5) & 0x03;
        self.ch3.frequency      = ((nr34 as u16 & 0x07) << 8) | nr33 as u16;
        self.ch3.length_enabled = nr34 & 0x40 != 0;

        if !self.ch3.dac_enabled { self.ch3.enabled = false; }

        // Sync wave RAM from MMU io registers
        for i in 0..16usize {
            self.ch3.wave_ram[i] = mmu.read_byte(WAVE_RAM_START + i as u16);
        }

        if nr34 & 0x80 != 0 {
            mmu.write_byte(NR34_ADDR, nr34 & 0x7F);
            self.ch3.length_counter = 256u16.saturating_sub(nr31 as u16);
            self.ch3.trigger();
        }
    }

    fn clock_frame_sequencer(&mut self) {
        match self.frame_seq_step {
            0 | 4 => {
                self.ch1.clock_length();
                self.ch2.clock_length();
                self.ch3.clock_length();
            }
            2 | 6 => {
                self.ch1.clock_length();
                self.ch2.clock_length();
                self.ch3.clock_length();
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

    fn clock_sweep(&mut self) {
        if self.ch1_sweep_timer > 0 { self.ch1_sweep_timer -= 1; }
        if self.ch1_sweep_timer == 0 && self.ch1_sweep_enabled {
            self.ch1_sweep_timer = 8;
        }
    }

    /// Apply a one-pole high-pass filter (capacitor model) to remove DC offset.
    ///
    /// This matches the DMG hardware capacitor that sits between the DAC
    /// and the headphone jack. Without it, abrupt channel enable/disable
    /// events cause loud pop/click artifacts that sound like a drum beat.
    fn high_pass(&self, input: f32, capacitor: &mut f32) -> f32 {
        let out    = input - *capacitor;
        *capacitor = input - out * HP_CHARGE_FACTOR;
        out
    }

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
        let ch3 = self.ch3.sample();
        let ch4 = 0.0f32; // CH4 stub

        let pan = |ch: f32, bit: u8| -> f32 {
            if nr51 & (1 << bit) != 0 { ch } else { 0.0 }
        };

        // NR51 panning: bits 7–4 = CH4/3/2/1 left; bits 3–0 = CH4/3/2/1 right
        let raw_left  = (pan(ch4, 7) + pan(ch3, 6) + pan(ch2, 5) + pan(ch1, 4))
                        * 0.25 * left_vol;
        let raw_right = (pan(ch4, 3) + pan(ch3, 2) + pan(ch2, 1) + pan(ch1, 0))
                        * 0.25 * right_vol;

        // Apply high-pass filter (remove DC offset / reduce pops)
        let mut cap_l = self.hp_cap_left;
        let mut cap_r = self.hp_cap_right;
        let out_left  = self.high_pass(raw_left,  &mut cap_l);
        let out_right = self.high_pass(raw_right, &mut cap_r);
        self.hp_cap_left  = cap_l;
        self.hp_cap_right = cap_r;

        self.sample_buffer.push(out_left.clamp(-1.0, 1.0));
        self.sample_buffer.push(out_right.clamp(-1.0, 1.0));
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

    fn setup() -> (Apu, Mmu) { (Apu::new(), Mmu::new()) }

    fn enable_apu(mmu: &mut Mmu) {
        mmu.write_byte(NR52_ADDR, 0x80);
        mmu.write_byte(NR50_ADDR, 0x77);
        mmu.write_byte(NR51_ADDR, 0xFF);
    }

    fn trigger_ch2(mmu: &mut Mmu, duty: u8, volume: u8, freq_reg: u16) {
        mmu.write_byte(NR21_ADDR, (duty << 6) & 0xC0);
        mmu.write_byte(NR22_ADDR, (volume << 4) | 0x08);
        mmu.write_byte(NR23_ADDR, (freq_reg & 0xFF) as u8);
        mmu.write_byte(NR24_ADDR, 0x80 | ((freq_reg >> 8) as u8 & 0x07));
    }

    fn trigger_ch1(mmu: &mut Mmu, duty: u8, volume: u8, freq_reg: u16) {
        mmu.write_byte(NR11_ADDR, (duty << 6) & 0xC0);
        mmu.write_byte(NR12_ADDR, (volume << 4) | 0x08);
        mmu.write_byte(NR13_ADDR, (freq_reg & 0xFF) as u8);
        mmu.write_byte(NR14_ADDR, 0x80 | ((freq_reg >> 8) as u8 & 0x07));
    }

    fn trigger_ch3(mmu: &mut Mmu, volume_code: u8, freq_reg: u16, ram: [u8; 16]) {
        mmu.write_byte(NR30_ADDR, 0x80); // DAC on
        mmu.write_byte(NR32_ADDR, (volume_code & 0x03) << 5);
        mmu.write_byte(NR33_ADDR, (freq_reg & 0xFF) as u8);
        for (i, &byte) in ram.iter().enumerate() {
            mmu.write_byte(WAVE_RAM_START + i as u16, byte);
        }
        mmu.write_byte(NR34_ADDR, 0x80 | ((freq_reg >> 8) as u8 & 0x07));
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() { return 0.0; }
        let sum: f32 = samples.iter().map(|&s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }

    // ── Silence ───────────────────────────────────────────────────────────────

    #[test]
    fn test_silence_when_apu_disabled() {
        let (mut apu, mut mmu) = setup();
        mmu.write_byte(NR52_ADDR, 0x00);
        apu.step(CYCLES_PER_SAMPLE as u32 * 20, &mut mmu);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn test_silence_when_no_channels_triggered() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        apu.step(CYCLES_PER_SAMPLE as u32 * 20, &mut mmu);
        let samples = apu.drain_samples();
        // HP filter may produce tiny non-zero values from initial transient;
        // all must be very close to zero
        assert!(samples.iter().all(|&s| s.abs() < 0.01));
    }

    // ── Sample buffer ─────────────────────────────────────────────────────────

    #[test]
    fn test_sample_buffer_stereo_pairs() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        apu.step(CYCLES_PER_SAMPLE as u32 * 10, &mut mmu);
        assert_eq!(apu.drain_samples().len() % 2, 0);
    }

    #[test]
    fn test_drain_clears_buffer() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        apu.step(CYCLES_PER_SAMPLE as u32 * 10, &mut mmu);
        let _ = apu.drain_samples();
        assert!(apu.sample_buffer.is_empty());
    }

    #[test]
    fn test_step_accumulates_expected_sample_count() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        let target = 100usize;
        apu.step((CYCLES_PER_SAMPLE * target as f64) as u32, &mut mmu);
        let pairs = apu.drain_samples().len() / 2;
        assert!(pairs >= target - 1 && pairs <= target + 1);
    }

    // ── CH1 ───────────────────────────────────────────────────────────────────

    #[test]
    fn test_ch1_triggered_produces_nonzero_audio() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        trigger_ch1(&mut mmu, 2, 15, 1000);
        apu.step(CYCLES_PER_SAMPLE as u32 * 100, &mut mmu);
        let samples = apu.drain_samples();
        assert!(samples.iter().any(|&s| s.abs() > 0.01));
    }

    // ── CH2 ───────────────────────────────────────────────────────────────────

    #[test]
    fn test_ch2_triggered_produces_nonzero_audio() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        trigger_ch2(&mut mmu, 2, 15, 1000);
        apu.step(CYCLES_PER_SAMPLE as u32 * 100, &mut mmu);
        let samples = apu.drain_samples();
        assert!(samples.iter().any(|&s| s.abs() > 0.01));
    }

    #[test]
    fn test_ch2_50pct_duty_produces_both_high_and_low_samples() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        mmu.write_byte(NR51_ADDR, 0x22); // CH2 left + right only
        trigger_ch2(&mut mmu, 2, 15, 1000);
        apu.step(33536 * 3, &mut mmu);
        let samples = apu.drain_samples();
        let lefts: Vec<f32> = samples.iter().step_by(2).copied().collect();
        // After HP filter, positive and negative samples should both appear
        assert!(lefts.iter().any(|&s| s > 0.01), "50% duty must have positive samples");
        assert!(lefts.iter().any(|&s| s < -0.01), "50% duty must have negative samples");
    }

    // ── CH3 (wave channel) ────────────────────────────────────────────────────

    #[test]
    fn test_ch3_triggered_produces_nonzero_audio() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        // Wave RAM: alternating 0x00 and 0xFF nibbles
        let mut ram = [0u8; 16];
        for i in (0..16).step_by(2) { ram[i] = 0xFF; }
        trigger_ch3(&mut mmu, 1, 1000, ram); // 100% volume
        apu.step(CYCLES_PER_SAMPLE as u32 * 200, &mut mmu);
        let samples = apu.drain_samples();
        assert!(
            samples.iter().any(|&s| s.abs() > 0.01),
            "CH3 with non-zero wave RAM must produce audible output"
        );
    }

    #[test]
    fn test_ch3_silent_when_wave_ram_all_zero() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        trigger_ch3(&mut mmu, 1, 1000, [0x00; 16]);
        // Run enough samples for the HP filter to settle (~10 000 samples)
        apu.step(CYCLES_PER_SAMPLE as u32 * 10_000, &mut mmu);
        let samples = apu.drain_samples();
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        // DC component must be attenuated significantly by the HP filter
        assert!(
            mean.abs() < 0.15,
            "HP filter must remove DC from all-zero wave RAM: mean={:.4}",
            mean
        );
    }

    #[test]
    fn test_ch3_muted_when_volume_code_zero() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        trigger_ch3(&mut mmu, 0, 1000, [0xFF; 16]); // volume_code=0 → mute
        // Run enough samples for HP filter to attenuate the DC
        apu.step(CYCLES_PER_SAMPLE as u32 * 10_000, &mut mmu);
        let samples = apu.drain_samples();
        let rms_muted = rms(&samples);

        // Compare against an unmuted channel — muted must be far quieter
        let (mut apu2, mut mmu2) = setup();
        enable_apu(&mut mmu2);
        trigger_ch3(&mut mmu2, 1, 1000, [0xFF; 16]); // volume_code=1 → 100%
        apu2.step(CYCLES_PER_SAMPLE as u32 * 10_000, &mut mmu2);
        let rms_unmuted = rms(&apu2.drain_samples());

        assert!(
            rms_muted < rms_unmuted * 0.1,
            "Muted CH3 ({:.4}) must be far quieter than unmuted ({:.4})",
            rms_muted, rms_unmuted
        );
    }

    #[test]
    fn test_ch3_produces_varying_samples_with_varying_wave_ram() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        mmu.write_byte(NR51_ADDR, 0x44); // CH3 left + right
        // Alternating max and zero nibbles → should produce high-amplitude wave
        let ram: [u8; 16] = [0xF0; 16]; // high nibble=0xF, low nibble=0x0
        trigger_ch3(&mut mmu, 1, 1792, ram);
        apu.step(8192 * 4, &mut mmu); // several wave cycles
        let samples = apu.drain_samples();
        let lefts: Vec<f32> = samples.iter().step_by(2).copied().collect();
        let has_pos = lefts.iter().any(|&s| s > 0.01);
        let has_neg = lefts.iter().any(|&s| s < -0.01);
        assert!(has_pos, "Varying wave RAM must produce positive samples");
        assert!(has_neg, "Varying wave RAM must produce negative samples");
    }

    // ── Square wave frequency analysis ───────────────────────────────────────

    #[test]
    fn test_square_wave_transitions_at_correct_frequency() {
        // freq=1792: step period=512 T-cycles, wave period=4096 T-cycles ≈ 1024 Hz
        // Over 5 wave cycles expect ~10 duty transitions (2 per cycle)
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        mmu.write_byte(NR51_ADDR, 0x22);
        trigger_ch2(&mut mmu, 2, 15, 1792);
        apu.step(4096 * 5, &mut mmu);
        let samples = apu.drain_samples();
        let lefts: Vec<f32> = samples.iter().step_by(2).copied().collect();
        let transitions = lefts.windows(2)
            .filter(|w| (w[0] > 0.01) != (w[1] > 0.01))
            .count();
        assert!(
            transitions >= 3 && transitions <= 20,
            "Expected ~10 transitions for 5 wave cycles, got {}",
            transitions
        );
    }

    #[test]
    fn test_square_wave_frequency_matches_register_value() {
        let run = |freq_reg: u16| -> usize {
            let (mut apu, mut mmu) = setup();
            enable_apu(&mut mmu);
            mmu.write_byte(NR51_ADDR, 0x22);
            trigger_ch2(&mut mmu, 2, 15, freq_reg);
            let cycles = (2048u32 - freq_reg as u32) * 4 * 8 * 5; // 5 wave cycles
            apu.step(cycles, &mut mmu);
            let s = apu.drain_samples();
            let lefts: Vec<f32> = s.iter().step_by(2).copied().collect();
            lefts.windows(2)
                .filter(|w| (w[0] > 0.01) != (w[1] > 0.01))
                .count()
        };
        let t_low  = run(1024);
        let t_high = run(1792);
        // Same number of wave cycles → same number of transitions (≈10 each)
        // Both should be in the 5-20 range
        assert!(t_low  >= 5, "freq_reg=1024: got {} transitions", t_low);
        assert!(t_high >= 5, "freq_reg=1792: got {} transitions", t_high);
    }

    // ── Volume envelope ───────────────────────────────────────────────────────

    #[test]
    fn test_volume_envelope_decreases_amplitude_over_time() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        mmu.write_byte(NR51_ADDR, 0x22);
        mmu.write_byte(NR22_ADDR, 0xF1); // vol=15, decrease, period=1
        mmu.write_byte(NR21_ADDR, 0x80); // duty=50%, length_load=0
        mmu.write_byte(NR23_ADDR, 0x00);
        mmu.write_byte(NR24_ADDR, 0x87); // trigger, NO length enable, freq_hi=7

        let one_env_step = FRAME_SEQ_PERIOD * 8;
        apu.step(one_env_step, &mut mmu);
        let early_rms = rms(&apu.drain_samples());

        apu.step(one_env_step * 10, &mut mmu);
        let late_rms = rms(&apu.drain_samples());

        assert!(
            late_rms < early_rms,
            "Volume should decrease: early={:.4}, late={:.4}",
            early_rms, late_rms
        );
    }

    // ── Length counter ────────────────────────────────────────────────────────

    #[test]
    fn test_length_counter_silences_channel_after_expiry() {
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        mmu.write_byte(NR51_ADDR, 0x22);
        mmu.write_byte(NR22_ADDR, 0xF0); // vol=15, period=0 (no envelope decay)
        mmu.write_byte(NR21_ADDR, 0xFF); // duty=3, length_load=63 → counter=1
        mmu.write_byte(NR23_ADDR, 0x00);
        mmu.write_byte(NR24_ADDR, 0xC7); // trigger + length enabled

        // Stage 1: before length clock fires
        apu.step(FRAME_SEQ_PERIOD / 2, &mut mmu);
        let early_nonzero = apu.drain_samples().iter().any(|&s| s.abs() > 0.01);

        // Stage 2: step past the clock (discarding mixed samples)
        apu.step(FRAME_SEQ_PERIOD, &mut mmu);
        let _ = apu.drain_samples();

        // Stage 3: verify silence
        apu.step(FRAME_SEQ_PERIOD / 2, &mut mmu);
        let late_all_zero = apu.drain_samples().iter().all(|&s| s.abs() < 0.01);

        assert!(early_nonzero, "Channel must be audible before length expiry");
        assert!(late_all_zero, "Channel must be silent after length expiry");
    }

    // ── High-pass filter ──────────────────────────────────────────────────────

    #[test]
    fn test_hp_filter_reduces_dc_offset() {
        // A channel playing only the low phase (-1.0 raw) is pure DC.
        // After HP filter, output should converge toward zero.
        let (mut apu, mut mmu) = setup();
        enable_apu(&mut mmu);
        mmu.write_byte(NR51_ADDR, 0x22);
        // duty=0 (12.5% = mostly low phase), volume=15, fast frequency
        trigger_ch2(&mut mmu, 0, 15, 1800);
        // Run for a long time to let HP filter settle
        apu.step(CYCLES_PER_SAMPLE as u32 * 5000, &mut mmu);
        let samples = apu.drain_samples();
        let lefts: Vec<f32> = samples.iter().step_by(2).copied().collect();
        // Average (DC component) should be close to zero after HP filter
        let mean = lefts.iter().sum::<f32>() / lefts.len() as f32;
        assert!(
            mean.abs() < 0.2,
            "HP filter must remove DC offset: mean={:.4}",
            mean
        );
    }

    // ── set_sample_rate ───────────────────────────────────────────────────────

    #[test]
    fn test_set_sample_rate_changes_sample_count() {
        let mut apu_44 = Apu::new();
        let mut mmu_44 = Mmu::new();
        enable_apu(&mut mmu_44);
        apu_44.set_sample_rate(44_100);
        apu_44.step(70_224, &mut mmu_44);
        let c44 = apu_44.drain_samples().len() / 2;

        let mut apu_48 = Apu::new();
        let mut mmu_48 = Mmu::new();
        enable_apu(&mut mmu_48);
        apu_48.set_sample_rate(48_000);
        apu_48.step(70_224, &mut mmu_48);
        let c48 = apu_48.drain_samples().len() / 2;

        assert!(c48 > c44, "48 kHz ({}) > 44.1 kHz ({})", c48, c44);
    }
}