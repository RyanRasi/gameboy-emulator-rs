//! Game Boy DMG emulator core.
//!
//! This crate contains pure emulation logic with zero I/O dependencies.
//! Frontends (desktop, web) depend on this crate — never the reverse.

pub fn version() -> &'static str {
    "gb-emulator-core 0.1.0"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phase 0 sanity check — confirms the test framework is wired up correctly.
    #[test]
    fn test_framework_is_working() {
        assert!(true == true, "Test framework must be operational");
    }

    /// Confirms the version string is reachable from tests.
    #[test]
    fn test_version_string() {
        let v = version();
        assert!(!v.is_empty(), "Version string should not be empty");
        assert!(v.contains("core"), "Version string should identify the crate");
    }
}