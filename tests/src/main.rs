//! Headless Blargg test ROM runner.
//!
//! Usage:
//!   cargo run -p blargg_tests -- tests/roms/cpu_instrs.gb
//!   cargo run -p blargg_tests -- tests/roms/  (runs all .gb in directory)
//!
//! The runner loads each ROM, executes up to MAX_CYCLES T-cycles, and
//! checks the serial output for "Passed" or "Failed".
//!
//! Download Blargg's test ROMs from:
//!   https://gbdev.gg/files/roms/blargg-gb-tests.zip
//! Place .gb files in tests/roms/

use gb_core::cartridge::Cartridge;
use gb_core::cpu::Cpu;
use gb_core::ppu::{LCDC_ADDR, BGP_ADDR};

/// Maximum T-cycles before declaring timeout (~30 seconds of game time).
const MAX_CYCLES: u64 = 4_194_304 * 30;

#[derive(Debug)]
enum TestResult {
    Passed,
    Failed(String),
    Timeout,
}

fn run_rom(path: &str) -> TestResult {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return TestResult::Failed(format!("Could not read file: {}", e)),
    };

    let cart = match Cartridge::load(data) {
        Ok(c) => c,
        Err(e) => return TestResult::Failed(format!("Bad cartridge: {}", e)),
    };

    let mut cpu = Cpu::new();
    cpu.mmu.load_cartridge(cart);
    cpu.mmu.write_byte(LCDC_ADDR, 0x91);
    cpu.mmu.write_byte(BGP_ADDR,  0xE4);

    loop {
        cpu.tick();

        if cpu.serial.passed() {
            return TestResult::Passed;
        }
        if cpu.serial.failed() {
            return TestResult::Failed(cpu.serial.output_str());
        }
        if cpu.cycles >= MAX_CYCLES {
            return TestResult::Timeout;
        }
    }
}

fn run_path(path: &str) -> bool {
    let meta = std::fs::metadata(path).unwrap_or_else(|e| {
        eprintln!("Cannot access '{}': {}", path, e);
        std::process::exit(1);
    });

    let rom_paths: Vec<String> = if meta.is_dir() {
        let mut paths: Vec<String> = std::fs::read_dir(path)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |x| x == "gb"))
            .map(|e| e.path().to_string_lossy().into_owned())
            .collect();
        paths.sort();
        paths
    } else {
        vec![path.to_string()]
    };

    if rom_paths.is_empty() {
        println!("No .gb files found in '{}'", path);
        return true;
    }

    let mut all_passed = true;

    for rom_path in &rom_paths {
        let name = std::path::Path::new(rom_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        print!("  {:40} ... ", name);
        match run_rom(rom_path) {
            TestResult::Passed => {
                println!("PASSED ✓");
            }
            TestResult::Failed(output) => {
                println!("FAILED ✗");
                // Print serial output for diagnosis
                for line in output.lines() {
                    println!("    {}", line);
                }
                all_passed = false;
            }
            TestResult::Timeout => {
                println!("TIMEOUT (no result after 30s game-time)");
                all_passed = false;
            }
        }
    }

    all_passed
}

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: blargg <rom.gb | roms_directory/>");
        eprintln!();
        eprintln!("Download Blargg's test ROMs from:");
        eprintln!("  https://gbdev.gg/files/roms/blargg-gb-tests.zip");
        eprintln!("Place .gb files in tests/roms/ then run:");
        eprintln!("  cargo run -p blargg_tests -- tests/roms/");
        std::process::exit(1);
    }

    println!();
    println!("=== Blargg Test ROM Runner ===");
    println!();

    let all_passed = run_path(&args[1]);

    println!();
    if all_passed {
        println!("All tests passed! ✓");
        std::process::exit(0);
    } else {
        println!("Some tests failed. ✗");
        std::process::exit(1);
    }
}