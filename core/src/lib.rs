//! Game Boy DMG emulator core — pure logic, zero I/O dependencies.

pub mod cartridge;
pub mod cpu;
pub mod mmu;
pub mod timer;

pub fn version() -> &'static str {
    "gb-emulator-core 0.1.0"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_framework_is_working() {
        assert!(true == true);
    }

    #[test]
    fn test_version_string() {
        let v = version();
        assert!(!v.is_empty());
        assert!(v.contains("core"));
    }
}