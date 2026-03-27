
# 🕹️ gameboy-emulator-rs

A modular, cycle-accurate Game Boy emulator written in Rust with desktop and web support.

![Rust](https://img.shields.io/badge/rust-1.70+-orange?logo=rust) ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/rust.yml) ![License](https://img.shields.io/github/license/RyanRasi/gameboy-emulator-rs) ![Status](https://img.shields.io/badge/status-in%20development-yellow) ![Platform](https://img.shields.io/badge/platform-cross--platform-blue)

 
🎮 Preview
(Add a GIF here later of Tetris and Pokémon Yellow running)

## ✨ Features

🧠 Cycle-accurate CPU emulation (LR35902)

🧩 Full memory map + MMU implementation

🎨 PPU rendering pipeline (tiles, sprites, scanlines)

🔊 Audio subsystem (APU) with waveform generation

💾 Cartridge support (ROM + MBC1/MBC3 planned)

⏱️ Hardware timers + interrupt system

🎮 Input handling (D-pad + buttons)

🖥️ Desktop frontend (real-time rendering)

🌐 Web server frontend (ROM upload + streaming output)

🧪 Test-driven architecture with emulator validation ROMs

🔧 Designed for Game Boy Color extension (CGB-ready architecture)

## 🏗️ Architecture

| Path | Purpose |
|--|--|
| Core | Pure emulation engine (CPU, PPU, MMU, APU) |
| Desktop | Native windowed emulator frontend |
| Web | HTTP server + ROM upload + framebuffer streaming |

## 🌍 Targets

✔ Game Boy (DMG) — primary focus

🚧 Save states — planned

🚧 Game Boy Color (CGB) — planned

## ⚡ Status

 - [x] Memory (MMU)
	 - [x] Full Game Boy memory map
	 - [x] Read/write system
	 - [x] ROM + BIOS overlay logic
 - [x] CPU CORE
	 - [x] Registers:
		- [x] A F B C D E H L
		- [x] PC SP
	 - [x] Instructions:
		 - [x] NOP
		 - [x] LD r, n
- [x] CPU EXPANSION
	- [x] arithmetic
	- [x] jumps
	- [x] stack
- [ ] INTERRUPTS
	- [x] Implement interrupt registers (IE, IF) and IME flag
	- [ ] Handle interrupt priority + execution (jump to ISR)
	- [ ] Integrate interrupts into CPU step cycle
- [ ] TIMERS
	- [ ] Implement DIV, TIMA, TMA, TAC registers
	- [ ] Increment timers based on CPU cycles
	- [ ] Trigger timer interrupt on overflow
- [ ] Cartridge System
	- [ ] Load and parse ROM file (header + metadata)
	- [ ] Implement ROM-only and MBC1 bank switching
	- [ ] Map cartridge reads/writes through MMU
- [ ] PPU (Graphics)
	- [ ] Implement scanline pipeline + PPU modes
	- [ ] Render background tiles to framebuffer
	- [ ] Add sprite rendering + VBlank signaling
- [ ] Input
	- [ ] Map user input to Game Boy buttons
	- [ ] Update joypad register (0xFF00)
	- [ ] Handle press/release state correctly
- [ ] Desktop App
	- [ ] Create window + rendering loop
	- [ ] Display framebuffer at ~60 FPS
	- [ ] Capture keyboard input and pass to core
- [ ] Web Server
	- [ ] Implement ROM + BIOS upload endpoints
	- [ ] Run emulator instance headlessly
	- [ ] Serve frames (HTTP or WebSocket stream)
- [ ] Audio (APU)
	- [ ] Stub audio system (no sound)
	- [ ] Implement basic sound channels (square, wave, noise)
	- [ ] Output mixed audio stream
- [ ] Testing & Validation
	- [ ] Run CPU test ROMs (instruction accuracy) BLAARG Tests
	- [ ] Validate PPU output with test ROMs
	- [ ] Add regression tests for stability
- [ ] Save States
	- [ ] Serialize emulator state (CPU, memory, PPU)
	- [ ] Implement save/load state functions
	- [ ] Ensure deterministic restore
- [ ] Colour Game Boy Upgrade
	- [ ] Mode detection
	- [ ] VRAM banking
	- [ ] WRAM banking
	- [ ] Palette system
	- [ ] Tile attributes
	- [ ] PPU color rendering
	- [ ] DMA (HDMA)
	- [ ] Double-speed mode
