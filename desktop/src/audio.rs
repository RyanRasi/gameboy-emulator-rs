//! Audio output using cpal.
//!
//! `AudioOutput::new()` discovers the default audio device and starts a
//! background stream. The emulator pushes stereo f32 samples via
//! `push_samples()`; the cpal callback drains them on demand.
//!
//! If no audio device is available, `new()` returns `None` and the emulator
//! runs silently.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct AudioOutput {
    /// Kept alive to hold the stream open.
    _stream:         cpal::Stream,
    buffer:          Arc<Mutex<VecDeque<f32>>>,
    pub channels:    u16,
    pub sample_rate: u32,
}

impl AudioOutput {
    /// Attempt to open the default audio output device.
    /// Returns `None` if no device is available or stream creation fails.
    pub fn new() -> Option<Self> {
        let host   = cpal::default_host();
        let device = host.default_output_device()?;
        let config = device.default_output_config()
            .map_err(|e| log::warn!("Audio config error: {}", e))
            .ok()?;

        let channels    = config.channels();
        let sample_rate = config.sample_rate().0;

        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let cb_buf = buffer.clone();

        let stream_config: cpal::StreamConfig = config.into();

        let stream = device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _| {
                let mut buf = cb_buf.lock().unwrap();
                for out in data.iter_mut() {
                    *out = buf.pop_front().unwrap_or(0.0);
                }
            },
            |err| log::error!("Audio stream error: {}", err),
            None,
        )
        .map_err(|e| log::warn!("Audio stream error: {}", e))
        .ok()?;

        stream.play()
            .map_err(|e| log::warn!("Audio play error: {}", e))
            .ok()?;

        log::info!(
            "Audio: {} channels @ {} Hz",
            channels, sample_rate
        );

        Some(AudioOutput { _stream: stream, buffer, channels, sample_rate })
    }

    /// Push interleaved stereo f32 samples (L, R, L, R, …) to the output.
    ///
    /// Drops incoming samples silently if the internal buffer is full
    /// (emulator running faster than audio — prevents unbounded growth).
    pub fn push_samples(&self, samples: &[f32]) {
        let mut buf = self.buffer.lock().unwrap();
        const MAX_BUFFER_SAMPLES: usize = 8_192; // ~185 ms at 44100 Hz stereo
        if buf.len() >= MAX_BUFFER_SAMPLES {
            return; // back-pressure: drop rather than grow unboundedly
        }
        match self.channels {
            1 => {
                // Mono output: mix stereo pair down to mono
                for pair in samples.chunks(2) {
                    let l    = pair[0];
                    let r    = if pair.len() > 1 { pair[1] } else { l };
                    buf.push_back((l + r) * 0.5);
                }
            }
            2 => {
                // Native stereo — push as-is
                for &s in samples {
                    buf.push_back(s);
                }
            }
            n => {
                // Multi-channel (e.g. 5.1): upmix stereo pair across all channels
                for pair in samples.chunks(2) {
                    let l = pair[0];
                    let r = if pair.len() > 1 { pair[1] } else { l };
                    for c in 0..n as usize {
                        buf.push_back(if c % 2 == 0 { l } else { r });
                    }
                }
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    // AudioOutput tests require a real audio device, so we test the logic
    // that doesn't depend on cpal directly.

    /// Verify the stereo→mono mixing formula.
    #[test]
    fn test_mono_mix_averages_lr() {
        let l = 0.6f32;
        let r = 0.4f32;
        let mono = (l + r) * 0.5;
        assert!((mono - 0.5).abs() < 1e-6);
    }

    /// Verify that 10 stereo pairs produce 10 mono samples.
    #[test]
    fn test_stereo_to_mono_sample_count() {
        let stereo: Vec<f32> = (0..20).map(|i| i as f32 * 0.1).collect();
        let mono: Vec<f32> = stereo.chunks(2)
            .map(|p| (p[0] + p[1]) * 0.5)
            .collect();
        assert_eq!(mono.len(), 10);
    }
}