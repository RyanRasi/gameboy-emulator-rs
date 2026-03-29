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

mod runner;
mod renderer;
mod keymap;

use minifb::{Key, Window, WindowOptions, Scale};
use gb_core::cpu::Cpu;
use gb_core::cartridge::Cartridge;
use gb_core::ppu::{SCREEN_WIDTH, SCREEN_HEIGHT, LCDC_ADDR, BGP_ADDR};

fn main() {
    env_logger::init();

    // ── Build CPU ─────────────────────────────────────────────────────────────
    let mut cpu = Cpu::new();

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let path = &args[1];
        let data = std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("Failed to read ROM '{}': {}", path, e);
            std::process::exit(1);
        });
        match Cartridge::load(data) {
            Ok(cart) => {
                log::info!("Loaded: {} (type 0x{:02X})",
                    cart.header.title, cart.header.cartridge_type);
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

    // Initialise hardware state (normally done by the BIOS)
    cpu.mmu.write_byte(LCDC_ADDR, 0x91); // LCD on, BG on
    cpu.mmu.write_byte(BGP_ADDR,  0xE4); // identity palette

    // ── Create window ─────────────────────────────────────────────────────────
    let mut window = Window::new(
        "Game Boy Emulator",
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
        WindowOptions {
            scale: Scale::X4, // 640×576 native window
            ..WindowOptions::default()
        },
    )
    .unwrap_or_else(|e| {
        eprintln!("Failed to create window: {}", e);
        std::process::exit(1);
    });

    // Cap frame rate to ~60 fps
    window.limit_update_rate(Some(std::time::Duration::from_micros(16_600)));

    let mut runner = runner::FrameRunner::new(cpu);
    let mut prev_keys: Vec<Key> = Vec::new();

    // ── Main loop ─────────────────────────────────────────────────────────────
    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Input — detect edges
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

        // Emulate one frame
        runner.run_frame();

        // Render
        let pixels = renderer::framebuffer_to_pixels(&runner.cpu.ppu.framebuffer);
        window
            .update_with_buffer(&pixels, SCREEN_WIDTH, SCREEN_HEIGHT)
            .unwrap_or_else(|e| log::error!("Window update failed: {}", e));
    }

    log::info!("Emulator exited after {} frames.", runner.frame_count());
}