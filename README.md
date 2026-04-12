
# 🕹️ gameboy-emulator-rs

A modular, cycle-accurate Game Boy emulator written in Rust with desktop and web support.

![Rust](https://img.shields.io/badge/rust-1.70+-orange?logo=rust) ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/rust.yml) ![License](https://img.shields.io/github/license/RyanRasi/gameboy-emulator-rs) ![Status](https://img.shields.io/badge/status-in%20development-yellow) ![Platform](https://img.shields.io/badge/platform-cross--platform-blue)

| App | Result |
|--|--|
| gb_core | ![Build - gb_core](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/gb_core_rust.yml?label=)|
| desktop | ![Build - desktop](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/desktop_rust.yml?label=)|
| web | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/web_rust.yml?label=)|

| Blargg Test | Result |
|--|--|
| 01-special | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/01-special.yml?label=)|
| 02-interrups | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/02-interrupts.yml?label=)|
| 03-op sp,hl | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/03-opsphl.yml?label=)|
| 04-op r,imm | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/04-oprimm.yml?label=)|
| 05-op rp | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/05-oprp.yml?label=)|
| 06-ld r,r | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/06-ldrr.yml?label=)|
| 07-jr,jp,call,ret,rst | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/07-jrjpcallretrst.yml?label=)|
| 08-misc instrs | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/08-miscinstrs.yml?label=)|
| 09-op r,r | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/09-oprr.yml?label=)|
| 10-bit ops | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/10-bitops.yml?label=)|
| 11-op a,(hl) | ![Build](https://img.shields.io/github/actions/workflow/status/RyanRasi/gameboy-emulator-rs/11-opahl.yml?label=)|

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
- [x] **Audio (APU)**
	- [x] Stub audio system (no sound)
	- [x] Implement basic sound channels (square, wave, noise)
	- [x] Output mixed audio stream
- [x] **Testing & Validation**
	- [x] Run CPU test ROMs (instruction accuracy) Blargg's Tests
	- [x] Validate PPU output with test ROMs
	- [x] Add regression tests for stability
- [x] **Save States**
	- [x] Serialize emulator state (CPU, memory, PPU)
	- [x] Implement save/load state functions
	- [x] Ensure deterministic restore
- [ ] **Colour Game Boy Upgrade**
	- [x] Mode detection
	- [x] VRAM banking
	- [x] WRAM banking
	- [x] Palette system
	- [x] Tile attributes
	- [x] PPU color rendering
	- [x] DMA (HDMA)
	- [x] Double-speed mode
