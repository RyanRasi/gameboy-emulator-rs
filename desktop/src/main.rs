//! Game Boy Desktop Frontend
//!
//! Usage: desktop [ROM_FILE]
//!
//! Controls:
//!   Z          → A
//!   X          → B
//!   Enter      → Start
//!   Backspace  → Select
//!   Arrow keys → D-pad
//!   Escape     → Quit

mod audio;
mod keymap;
mod renderer;
mod runner;

use minifb::{Key, Scale, Window, WindowOptions};
use gb_core::cartridge::Cartridge;
use gb_core::cpu::Cpu;
use gb_core::ppu::{BGP_ADDR, LCDC_ADDR, SCREEN_HEIGHT, SCREEN_WIDTH};

fn main() {
    env_logger::init();

    // ── Build CPU ─────────────────────────────────────────────────────────────
    let mut cpu = Cpu::new();

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let path = &args[1];
        let data = std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("Failed to read '{}': {}", path, e);
            std::process::exit(1);
        });
        match Cartridge::load(data) {
            Ok(cart) => {
                log::info!(
                    "Loaded: '{}' (type 0x{:02X})",
                    cart.header.title, cart.header.cartridge_type
                );
                cpu.mmu.load_cartridge(cart);
            }
            Err(e) => {
                eprintln!("Cartridge error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        log::info!("No ROM provided — running blank ROM");
    }

    cpu.mmu.write_byte(LCDC_ADDR, 0x91);
    cpu.mmu.write_byte(BGP_ADDR,  0xE4);

    // ── Audio ─────────────────────────────────────────────────────────────────
    let audio = audio::AudioOutput::new();
    if let Some(ref a) = audio {
        cpu.apu.set_sample_rate(a.sample_rate);
    } else {
        log::warn!("No audio device found — running silently");
    }

    // ── Window ────────────────────────────────────────────────────────────────
    let mut window = Window::new(
        "Game Boy Emulator",
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
        WindowOptions {
            scale: Scale::X4,
            ..WindowOptions::default()
        },
    )
    .unwrap_or_else(|e| {
        eprintln!("Window error: {}", e);
        std::process::exit(1);
    });

    window.limit_update_rate(Some(std::time::Duration::from_micros(16_600)));

    let mut runner   = runner::FrameRunner::new(cpu);
    let mut prev_keys: Vec<Key> = Vec::new();

    // ── Main loop ─────────────────────────────────────────────────────────────
    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Input
        let curr_keys = window.get_keys();

        for &key in &keymap::newly_pressed(&curr_keys, &prev_keys) {
            if let Some(btn) = keymap::key_to_button(key) {
                runner.cpu.button_press(btn);
            }
        }
        for &key in &keymap::newly_released(&prev_keys, &curr_keys) {
            if let Some(btn) = keymap::key_to_button(key) {
                runner.cpu.button_release(btn);
            }
        }
        prev_keys = curr_keys;

        // Emulate
        runner.run_frame();

        // Audio
        if let Some(ref a) = audio {
            let samples = runner.cpu.apu.drain_samples();
            if !samples.is_empty() {
                a.push_samples(&samples);
            }
        }

        // Video
        let pixels = renderer::framebuffer_to_pixels(&runner.cpu.ppu.framebuffer);
        window
            .update_with_buffer(&pixels, SCREEN_WIDTH, SCREEN_HEIGHT)
            .unwrap_or_else(|e| log::error!("Window update: {}", e));
    }

    log::info!("Emulator exited after {} frames.", runner.frame_count());
}