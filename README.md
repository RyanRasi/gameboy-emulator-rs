
# 🕹️ gameboy-emulator-rs

A modular, cycle-accurate Game Boy emulator written in Rust with desktop and web support.

![Rust](https://img.shields.io/badge/rust-1.70+-orange?logo=rust) ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/rust.yml) ![License](https://img.shields.io/github/license/RyanRasi/gameboy-emulator-rs) ![Status](https://img.shields.io/badge/status-in%20development-yellow) ![Platform](https://img.shields.io/badge/platform-cross--platform-blue)

| App | Result |
|--|--|
| gb_core | ![Build - gb_core](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/gb_core_rust.yml?label=test)|
| desktop | ![Build - desktop](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/desktop_rust.yml?label=test)|
| web | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/web_rust.yml?label=test)|

 
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

## 🛠️ Setup

1. Clone repo
``` bash
git clone https://github.com/RyanRasi/gameboy-emulator-rs.git
cd gameboy-emulator-rs
```
2. Build and test project
``` bash
cargo build
cargo test -p gb_core
cargo test -p desktop
cargo test -p web
```

3. Run project with either

   a. Desktop
	``` bash
	cargo run -p desktop "roms/rom_name.gb"
	```
	or

	b. WebServer
	``` bash
	cargo run -p web
	```
	Then open ```/web/index.html```

## 🕹️ Controls

| Original GB control | Keyboard Mapping |
|--|--|
| D-Pad | Arrow Keys |
| A | Z |
| B | X |
| Start | Enter |
| Select | Right Shift |

When running in Desktop mode, use ```esc``` to quit

## ⚡ Status

 - [x] **Memory (MMU)**
	 - [x] Full Game Boy memory map
	 - [x] Read/write system
	 - [x] ROM + BIOS overlay logic
 - [x] **CPU Core**
	 - [x] Registers:
		- [x] A F B C D E H L
		- [x] PC SP
	 - [x] Instructions:
		 - [x] NOP
		 - [x] LD r, n
- [x] **CPU Expansion**
	- [x] arithmetic
	- [x] jumps
	- [x] stack
- [x] **Interrupts**
	- [x] Implement interrupt registers (IE, IF) and IME flag
	- [x] Handle interrupt priority + execution (jump to ISR)
	- [x] Integrate interrupts into CPU step cycle
- [x] **Times**
	- [x] Implement DIV, TIMA, TMA, TAC registers
	- [x] Increment timers based on CPU cycles
	- [x] Trigger timer interrupt on overflow
- [x] **Cartridge System**
	- [x] Load and parse ROM file (header + metadata)
	- [x] Implement ROM-only and MBC1 bank switching
	- [x] Map cartridge reads/writes through MMU
- [x] **PPU (Graphics)**
	- [x] Implement scanline pipeline + PPU modes
	- [x] Render background tiles to framebuffer
	- [x] Add sprite rendering + VBlank signaling
- [x] **Input**
	- [x] Map user input to Game Boy buttons
	- [x] Update joypad register (0xFF00)
	- [x] Handle press/release state correctly
- [x] **Desktop App**
	- [x] Create window + rendering loop
	- [x] Display framebuffer at ~60 FPS
	- [x] Capture keyboard input and pass to core
- [x] **Web Server**
	- [x] Implement ROM + BIOS upload endpoints
	- [x] Run emulator instance headlessly
	- [x] Serve frames (HTTP or WebSocket stream)
- [ ] **Audio (APU)**
	- [ ] Stub audio system (no sound)
	- [ ] Implement basic sound channels (square, wave, noise)
	- [ ] Output mixed audio stream
- [ ] **Testing & Validation**
	- [ ] Run CPU test ROMs (instruction accuracy) Blargg's Tests
	- [ ] Validate PPU output with test ROMs
	- [ ] Add regression tests for stability
- [ ] **Save States**
	- [ ] Serialize emulator state (CPU, memory, PPU)
	- [ ] Implement save/load state functions
	- [ ] Ensure deterministic restore
- [ ] **Colour Game Boy Upgrade**
	- [ ] Mode detection
	- [ ] VRAM banking
	- [ ] WRAM banking
	- [ ] Palette system
	- [ ] Tile attributes
	- [ ] PPU color rendering
	- [ ] DMA (HDMA)
	- [ ] Double-speed mode
