//! Game Boy Desktop Frontend — CGB support, palette cycling, window title.
//!
//! Keyboard controls:
//!   Z=A, X=B, Enter=Start, Backspace=Select, Arrows=Dpad
//!   F1=prev palette, F2=next palette, F3=game palette
//!   F5=save state, F7=load state
//!   Escape=quit

mod audio;
mod keymap;
mod renderer;
mod runner;

use minifb::{Key, Scale, Window, WindowOptions};
use gb_core::cartridge::Cartridge;
use gb_core::cpu::Cpu;
use gb_core::ppu::{BGP_ADDR, LCDC_ADDR, SCREEN_HEIGHT, SCREEN_WIDTH};
use gb_core::ppu::palettes::{ALL_MANUAL_PALETTES, DEFAULT_PALETTE, detect_game_palette, detect_game_name};
use gb_core::gbc_bios_palettes::apply_bios_palettes;

fn palette_name(idx: Option<usize>, is_cgb: bool) -> String {
    match idx {
        None    => if is_cgb { "Native".to_string() } else { "Game Palette".to_string() },
        Some(i) => ALL_MANUAL_PALETTES[i].0.to_string(),
    }
}

fn main() {
    env_logger::init();

    let mut cpu = Cpu::new();
    let mut game_title    = String::from("Game Boy Emulator");
    let mut game_rom_data: Option<Vec<u8>> = None;

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let path = &args[1];
        let data = std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("Failed to read '{}': {}", path, e);
            std::process::exit(1);
        });
        match Cartridge::load(data.clone()) {
            Ok(cart) => {
                log::info!("Loaded: '{}' CGB={}", cart.header.title, cart.header.is_cgb());
                let rom_title = cart.header.title.clone();
                let is_cgb    = cart.header.is_cgb();

                cpu.ppu.cgb_mode = is_cgb;
                cpu.mmu.load_cartridge(cart);
                cpu.mmu.write_byte(LCDC_ADDR, 0x91);
                cpu.mmu.write_byte(BGP_ADDR,  0xE4);

                // Set post-boot registers for CGB mode
                if is_cgb {
                    cpu.regs.a  = 0x11;
                    cpu.regs.f  = 0x80;
                    cpu.regs.b  = 0x00;
                    cpu.regs.c  = 0x00;
                    cpu.regs.d  = 0xFF;
                    cpu.regs.e  = 0x56;
                    cpu.regs.h  = 0x00;
                    cpu.regs.l  = 0x0D;
                    cpu.regs.sp = 0xFFFE;
                    cpu.regs.pc = 0x0100;
                }

                // Apply GBC boot ROM palettes for DMG-only games
                if !is_cgb {
                    apply_bios_palettes(&data, &mut cpu.mmu.bg_palette, &mut cpu.mmu.obj_palette);
                    if let Some(pal) = detect_game_palette(&rom_title) {
                        cpu.ppu.dmg_palettes = pal;
                    } else {
                        cpu.ppu.dmg_palettes = DEFAULT_PALETTE.clone();
                    }
                }

                game_title = detect_game_name(&rom_title)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        if rom_title.is_empty() { "Game Boy".to_string() } else { rom_title.clone() }
                    });

                game_rom_data = Some(data);
            }
            Err(e) => {
                eprintln!("Cartridge error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        cpu.mmu.write_byte(LCDC_ADDR, 0x91);
        cpu.mmu.write_byte(BGP_ADDR,  0xE4);
        log::info!("No ROM provided — running blank ROM");
    }

    // ── Palette state ─────────────────────────────────────────────────────────

    let is_cgb_game = cpu.ppu.cgb_mode;
    let mut manual_palette_idx: Option<usize> = None;

    // ── Audio ─────────────────────────────────────────────────────────────────

    let audio = audio::AudioOutput::new();
    if let Some(ref a) = audio {
        cpu.apu.set_sample_rate(a.sample_rate);
    } else {
        log::warn!("No audio device found — running silently");
    }

    // ── Window ────────────────────────────────────────────────────────────────

    let initial_title = format!("{} [{}]", game_title, palette_name(manual_palette_idx, is_cgb_game));
    let mut window = Window::new(
        &initial_title,
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
        WindowOptions {
            scale: Scale::X4,
            ..WindowOptions::default()
        },
    ).unwrap_or_else(|e| {
        eprintln!("Window error: {}", e);
        std::process::exit(1);
    });

    window.limit_update_rate(Some(std::time::Duration::from_micros(16_600)));

    let mut runner    = runner::FrameRunner::new(cpu);
    let mut prev_keys: Vec<Key> = Vec::new();

    // ── Main loop ─────────────────────────────────────────────────────────────

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let curr_keys = window.get_keys();

        // ── Gamepad input ─────────────────────────────────────────────────────

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

        // ── Palette keys ──────────────────────────────────────────────────────

        // F1 — previous palette
        if window.is_key_pressed(Key::F1, minifb::KeyRepeat::No) {
            let n = ALL_MANUAL_PALETTES.len();
            manual_palette_idx = Some(match manual_palette_idx {
                None    => n - 1,
                Some(0) => n - 1,
                Some(i) => i - 1,
            });
            let idx = manual_palette_idx.unwrap();
            runner.cpu.ppu.dmg_palettes      = ALL_MANUAL_PALETTES[idx].1.clone();
            runner.cpu.ppu.force_dmg_palette = true;
            let title = format!("{} [{}]", game_title, palette_name(manual_palette_idx, is_cgb_game));
            window.set_title(&title);
        }

        // F2 — next palette
        if window.is_key_pressed(Key::F2, minifb::KeyRepeat::No) {
            let n = ALL_MANUAL_PALETTES.len();
            manual_palette_idx = Some(match manual_palette_idx {
                None    => 0,
                Some(i) => (i + 1) % n,
            });
            let idx = manual_palette_idx.unwrap();
            runner.cpu.ppu.dmg_palettes      = ALL_MANUAL_PALETTES[idx].1.clone();
            runner.cpu.ppu.force_dmg_palette = true;
            let title = format!("{} [{}]", game_title, palette_name(manual_palette_idx, is_cgb_game));
            window.set_title(&title);
        }

        // F3 — restore game/native palette
        if window.is_key_pressed(Key::F3, minifb::KeyRepeat::No) {
            manual_palette_idx               = None;
            runner.cpu.ppu.force_dmg_palette = false;
            if !is_cgb_game {
                if let Some(ref data) = game_rom_data {
                    apply_bios_palettes(
                        data,
                        &mut runner.cpu.mmu.bg_palette,
                        &mut runner.cpu.mmu.obj_palette,
                    );
                }
                if let Some(pal) = detect_game_palette(&game_title) {
                    runner.cpu.ppu.dmg_palettes = pal;
                } else {
                    runner.cpu.ppu.dmg_palettes = DEFAULT_PALETTE.clone();
                }
            }
            let title = format!("{} [{}]", game_title, palette_name(manual_palette_idx, is_cgb_game));
            window.set_title(&title);
        }

        // ── Save / load state ─────────────────────────────────────────────────

        if window.is_key_pressed(Key::F5, minifb::KeyRepeat::No) {
            use gb_core::save_state::SaveState;
            let state = SaveState::capture(&runner.cpu);
            match state.to_bytes() {
                Ok(bytes) => match std::fs::write("save.state", &bytes[..]) {
                    Ok(_)  => log::info!("State saved ({} bytes)", bytes.len()),
                    Err(e) => log::error!("Save failed: {}", e),
                },
                Err(e) => log::error!("Serialize failed: {}", e),
            }
        }

        if window.is_key_pressed(Key::F7, minifb::KeyRepeat::No) {
            use gb_core::save_state::SaveState;
            match std::fs::read("save.state") {
                Ok(bytes) => match SaveState::from_bytes(&bytes) {
                    Ok(state) => {
                        let result: Result<(), String> = state.restore(&mut runner.cpu);
                        match result {
                            Ok(_)  => log::info!("State loaded"),
                            Err(e) => log::error!("Restore failed: {}", e),
                        }
                    }
                    Err(e) => log::error!("Deserialize failed: {}", e),
                },
                Err(e) => log::error!("Load failed: {}", e),
            }
        }

        prev_keys = curr_keys;

        // ── Emulate one frame ─────────────────────────────────────────────────

        runner.run_frame();

        // ── Audio ─────────────────────────────────────────────────────────────

        if let Some(ref a) = audio {
            let samples = runner.cpu.apu.drain_samples();
            if !samples.is_empty() {
                a.push_samples(&samples);
            }
        }

        // ── Video ─────────────────────────────────────────────────────────────

        let pixels = renderer::framebuffer_to_pixels(&*runner.cpu.ppu.framebuffer);
        window
            .update_with_buffer(&pixels, SCREEN_WIDTH, SCREEN_HEIGHT)
            .unwrap_or_else(|e| log::error!("Window update: {}", e));
    }

    log::info!("Emulator exited after {} frames.", runner.frame_count());
}