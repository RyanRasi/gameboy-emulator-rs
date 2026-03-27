//! Instruction decode and execution.
//!
//! Each instruction handler takes a mutable reference to the CPU state
//! and returns the number of T-cycles consumed.
//!
//! T-cycles vs M-cycles:
//!   The Game Boy clock runs at 4,194,304 Hz.
//!   One M-cycle = 4 T-cycles.
//!   We track T-cycles internally; callers may divide by 4 for M-cycles.

use super::Cpu;

impl Cpu {
    /// Fetch the byte at PC and advance PC by 1.
    pub(super) fn fetch_byte(&mut self) -> u8 {
        let byte = self.mmu.read_byte(self.regs.pc);
        self.regs.pc = self.regs.pc.wrapping_add(1);
        byte
    }

    /// Fetch a 16-bit little-endian immediate and advance PC by 2.
    pub(super) fn fetch_word(&mut self) -> u16 {
        let lo = self.fetch_byte() as u16;
        let hi = self.fetch_byte() as u16;
        (hi << 8) | lo
    }

    /// Decode and execute the next instruction.
    /// Returns the number of T-cycles consumed.
    pub fn step(&mut self) -> u32 {
        let opcode = self.fetch_byte();

        match opcode {
            // ------------------------------------------------------------------
            // 0x00 — NOP  (4 T-cycles)
            // ------------------------------------------------------------------
            0x00 => 4,

            // ------------------------------------------------------------------
            // 8-bit immediate loads  LD r, n8  (8 T-cycles each)
            // ------------------------------------------------------------------
            0x06 => { let n = self.fetch_byte(); self.regs.b = n; 8 }  // LD B, n
            0x0E => { let n = self.fetch_byte(); self.regs.c = n; 8 }  // LD C, n
            0x16 => { let n = self.fetch_byte(); self.regs.d = n; 8 }  // LD D, n
            0x1E => { let n = self.fetch_byte(); self.regs.e = n; 8 }  // LD E, n
            0x26 => { let n = self.fetch_byte(); self.regs.h = n; 8 }  // LD H, n
            0x2E => { let n = self.fetch_byte(); self.regs.l = n; 8 }  // LD L, n
            0x3E => { let n = self.fetch_byte(); self.regs.a = n; 8 }  // LD A, n

            // ------------------------------------------------------------------
            // 16-bit immediate loads  LD rr, n16  (12 T-cycles each)
            // ------------------------------------------------------------------
            0x01 => { let nn = self.fetch_word(); self.regs.set_bc(nn); 12 } // LD BC, nn
            0x11 => { let nn = self.fetch_word(); self.regs.set_de(nn); 12 } // LD DE, nn
            0x21 => { let nn = self.fetch_word(); self.regs.set_hl(nn); 12 } // LD HL, nn
            0x31 => { let nn = self.fetch_word(); self.regs.sp = nn;    12 } // LD SP, nn

            // ------------------------------------------------------------------
            // 8-bit register-to-register loads  LD r, r'  (4 T-cycles each)
            // Row B (0x40–0x47)
            // ------------------------------------------------------------------
            0x40 => 4, // LD B, B (no-op in effect)
            0x41 => { self.regs.b = self.regs.c; 4 } // LD B, C
            0x42 => { self.regs.b = self.regs.d; 4 } // LD B, D
            0x43 => { self.regs.b = self.regs.e; 4 } // LD B, E
            0x44 => { self.regs.b = self.regs.h; 4 } // LD B, H
            0x45 => { self.regs.b = self.regs.l; 4 } // LD B, L
            0x47 => { self.regs.b = self.regs.a; 4 } // LD B, A

            // Row C (0x48–0x4F)
            0x48 => { self.regs.c = self.regs.b; 4 } // LD C, B
            0x49 => 4,                                 // LD C, C
            0x4A => { self.regs.c = self.regs.d; 4 } // LD C, D
            0x4B => { self.regs.c = self.regs.e; 4 } // LD C, E
            0x4C => { self.regs.c = self.regs.h; 4 } // LD C, H
            0x4D => { self.regs.c = self.regs.l; 4 } // LD C, L
            0x4F => { self.regs.c = self.regs.a; 4 } // LD C, A

            // Row D (0x50–0x57)
            0x50 => { self.regs.d = self.regs.b; 4 } // LD D, B
            0x51 => { self.regs.d = self.regs.c; 4 } // LD D, C
            0x52 => 4,                                 // LD D, D
            0x53 => { self.regs.d = self.regs.e; 4 } // LD D, E
            0x54 => { self.regs.d = self.regs.h; 4 } // LD D, H
            0x55 => { self.regs.d = self.regs.l; 4 } // LD D, L
            0x57 => { self.regs.d = self.regs.a; 4 } // LD D, A

            // Row E (0x58–0x5F)
            0x58 => { self.regs.e = self.regs.b; 4 } // LD E, B
            0x59 => { self.regs.e = self.regs.c; 4 } // LD E, C
            0x5A => { self.regs.e = self.regs.d; 4 } // LD E, D
            0x5B => 4,                                 // LD E, E
            0x5C => { self.regs.e = self.regs.h; 4 } // LD E, H
            0x5D => { self.regs.e = self.regs.l; 4 } // LD E, L
            0x5F => { self.regs.e = self.regs.a; 4 } // LD E, A

            // Row H (0x60–0x67)
            0x60 => { self.regs.h = self.regs.b; 4 } // LD H, B
            0x61 => { self.regs.h = self.regs.c; 4 } // LD H, C
            0x62 => { self.regs.h = self.regs.d; 4 } // LD H, D
            0x63 => { self.regs.h = self.regs.e; 4 } // LD H, E
            0x64 => 4,                                 // LD H, H
            0x65 => { self.regs.h = self.regs.l; 4 } // LD H, L
            0x67 => { self.regs.h = self.regs.a; 4 } // LD H, A

            // Row L (0x68–0x6F)
            0x68 => { self.regs.l = self.regs.b; 4 } // LD L, B
            0x69 => { self.regs.l = self.regs.c; 4 } // LD L, C
            0x6A => { self.regs.l = self.regs.d; 4 } // LD L, D
            0x6B => { self.regs.l = self.regs.e; 4 } // LD L, E
            0x6C => { self.regs.l = self.regs.h; 4 } // LD L, H
            0x6D => 4,                                 // LD L, L
            0x6F => { self.regs.l = self.regs.a; 4 } // LD L, A

            // Row A (0x78–0x7F)
            0x78 => { self.regs.a = self.regs.b; 4 } // LD A, B
            0x79 => { self.regs.a = self.regs.c; 4 } // LD A, C
            0x7A => { self.regs.a = self.regs.d; 4 } // LD A, D
            0x7B => { self.regs.a = self.regs.e; 4 } // LD A, E
            0x7C => { self.regs.a = self.regs.h; 4 } // LD A, H
            0x7D => { self.regs.a = self.regs.l; 4 } // LD A, L
            0x7F => 4,                                 // LD A, A

            // ------------------------------------------------------------------
            // Unimplemented — will be filled in during Phase 3+
            // ------------------------------------------------------------------
            unknown => {
                log::warn!(
                    "Unimplemented opcode 0x{:02X} at PC=0x{:04X}",
                    unknown,
                    self.regs.pc.wrapping_sub(1)
                );
                // Treat as NOP — 4 cycles — keeps emulator alive during development
                4
            }
        }
    }
}