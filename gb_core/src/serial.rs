//! Game Boy Serial Port.
//!
//! Two registers:
//!   0xFF01  SB — Serial transfer data
//!   0xFF02  SC — Serial transfer control
//!             Bit 7: Transfer start flag (1 = transfer in progress)
//!             Bit 0: Clock select (1 = internal clock)
//!
//! Blargg's test ROMs signal test output by:
//!   1. Writing a character to SB (0xFF01)
//!   2. Writing 0x81 to SC (0xFF02) — start transfer, internal clock
//!
//! We capture each byte written this way into a buffer.
//! The test ROMs print "Passed" or "Failed" followed by test details.

pub const SB_ADDR: u16 = 0xFF01;
pub const SC_ADDR: u16 = 0xFF02;

pub struct Serial {
    /// Accumulated output bytes from the serial port.
    pub output: Vec<u8>,
}

impl Serial {
    pub fn new() -> Self {
        Serial { output: Vec::new() }
    }

    /// Called when the CPU writes to SC (0xFF02).
    /// If bit 7 is set (transfer start) and bit 0 is set (internal clock),
    /// capture the current SB value as a serial output byte.
    pub fn on_sc_write(&mut self, sb: u8, sc: u8) {
        if sc & 0x81 == 0x81 {
            self.output.push(sb);
        }
    }

    /// Return accumulated output as a UTF-8 string (lossy).
    pub fn output_str(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }

    /// True if the output contains "Passed".
    pub fn passed(&self) -> bool {
        self.output_str().contains("Passed")
    }

    /// True if the output contains "Failed".
    pub fn failed(&self) -> bool {
        self.output_str().contains("Failed")
    }

    /// True if all individual sub-tests passed (output ends with "Passed").
    pub fn all_passed(&self) -> bool {
        let s = self.output_str();
        // Blargg format: each sub-test prints pass/fail, then overall result last
        s.contains("Passed") && !s.contains("Failed")
    }
}

impl Default for Serial {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_serial_has_empty_output() {
        let s = Serial::new();
        assert!(s.output.is_empty());
        assert_eq!(s.output_str(), "");
    }

    #[test]
    fn test_sc_write_with_0x81_captures_sb() {
        let mut s = Serial::new();
        s.on_sc_write(b'H', 0x81);
        assert_eq!(s.output, vec![b'H']);
    }

    #[test]
    fn test_sc_write_without_bit7_ignores_sb() {
        let mut s = Serial::new();
        s.on_sc_write(b'X', 0x01); // bit 7 not set
        assert!(s.output.is_empty());
    }

    #[test]
    fn test_sc_write_without_bit0_ignores_sb() {
        let mut s = Serial::new();
        s.on_sc_write(b'X', 0x80); // bit 0 not set
        assert!(s.output.is_empty());
    }

    #[test]
    fn test_multiple_chars_accumulate() {
        let mut s = Serial::new();
        for &c in b"Hello" {
            s.on_sc_write(c, 0x81);
        }
        assert_eq!(s.output_str(), "Hello");
    }

    #[test]
    fn test_passed_detection() {
        let mut s = Serial::new();
        for &c in b"Passed" {
            s.on_sc_write(c, 0x81);
        }
        assert!(s.passed());
        assert!(!s.failed());
        assert!(s.all_passed());
    }

    #[test]
    fn test_failed_detection() {
        let mut s = Serial::new();
        for &c in b"Failed #1" {
            s.on_sc_write(c, 0x81);
        }
        assert!(s.failed());
        assert!(!s.passed());
        assert!(!s.all_passed());
    }

    #[test]
    fn test_all_passed_requires_no_failed() {
        let mut s = Serial::new();
        // Has both "Failed" and "Passed" — should NOT be all_passed
        for &c in b"Failed #1\nPassed" {
            s.on_sc_write(c, 0x81);
        }
        assert!(!s.all_passed());
    }

    #[test]
    fn test_output_str_lossy_on_non_utf8() {
        let mut s = Serial::new();
        s.output.push(0xFF); // invalid UTF-8
        let out = s.output_str();
        assert!(!out.is_empty()); // lossy — produces replacement char
    }
}