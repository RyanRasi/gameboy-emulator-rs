//! Instruction decode and execution — Phase 3 + HALT (Phase 4).

use super::Cpu;
use super::alu;
use super::registers::flags;

impl Cpu {
    pub(super) fn fetch_byte(&mut self) -> u8 {
        let b = self.mmu.read_byte(self.regs.pc);
        self.regs.pc = self.regs.pc.wrapping_add(1);
        b
    }

    pub(super) fn fetch_word(&mut self) -> u16 {
        let lo = self.fetch_byte() as u16;
        let hi = self.fetch_byte() as u16;
        (hi << 8) | lo
    }

    pub(super) fn stack_push(&mut self, value: u16) {
        self.regs.sp = self.regs.sp.wrapping_sub(2);
        self.mmu.write_word(self.regs.sp, value);
    }

    pub(super) fn stack_pop(&mut self) -> u16 {
        let value = self.mmu.read_word(self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(2);
        value
    }

    fn alu_add(&mut self, x: u8) {
        let r = alu::add(self.regs.a, x);
        self.regs.a = r.value; self.regs.f = r.flags;
    }
    fn alu_adc(&mut self, x: u8) {
        let r = alu::adc(self.regs.a, x, self.regs.flag_c());
        self.regs.a = r.value; self.regs.f = r.flags;
    }
    fn alu_sub(&mut self, x: u8) {
        let r = alu::sub(self.regs.a, x);
        self.regs.a = r.value; self.regs.f = r.flags;
    }
    fn alu_sbc(&mut self, x: u8) {
        let r = alu::sbc(self.regs.a, x, self.regs.flag_c());
        self.regs.a = r.value; self.regs.f = r.flags;
    }
    fn alu_and(&mut self, x: u8) {
        let r = alu::and(self.regs.a, x);
        self.regs.a = r.value; self.regs.f = r.flags;
    }
    fn alu_or(&mut self, x: u8) {
        let r = alu::or(self.regs.a, x);
        self.regs.a = r.value; self.regs.f = r.flags;
    }
    fn alu_xor(&mut self, x: u8) {
        let r = alu::xor(self.regs.a, x);
        self.regs.a = r.value; self.regs.f = r.flags;
    }
    fn alu_cp(&mut self, x: u8) {
        let r = alu::cp(self.regs.a, x);
        self.regs.f = r.flags;
    }

    pub fn step(&mut self) -> u32 {
        let opcode = self.fetch_byte();
        match opcode {

            // ── NOP ─────────────────────────────────────────────────────────
            0x00 => 4,

            // ── HALT ────────────────────────────────────────────────────────
            0x76 => { self.halted = true; 4 }

            // ── 8-bit immediate loads ────────────────────────────────────────
            0x06 => { let n = self.fetch_byte(); self.regs.b = n; 8 }
            0x0E => { let n = self.fetch_byte(); self.regs.c = n; 8 }
            0x16 => { let n = self.fetch_byte(); self.regs.d = n; 8 }
            0x1E => { let n = self.fetch_byte(); self.regs.e = n; 8 }
            0x26 => { let n = self.fetch_byte(); self.regs.h = n; 8 }
            0x2E => { let n = self.fetch_byte(); self.regs.l = n; 8 }
            0x3E => { let n = self.fetch_byte(); self.regs.a = n; 8 }

            // ── 16-bit immediate loads ────────────────────────────────────────
            0x01 => { let nn = self.fetch_word(); self.regs.set_bc(nn); 12 }
            0x11 => { let nn = self.fetch_word(); self.regs.set_de(nn); 12 }
            0x21 => { let nn = self.fetch_word(); self.regs.set_hl(nn); 12 }
            0x31 => { let nn = self.fetch_word(); self.regs.sp = nn;    12 }

            // ── INC r8 ───────────────────────────────────────────────────────
            0x04 => { let r = alu::inc(self.regs.b, self.regs.f); self.regs.b = r.value; self.regs.f = r.flags; 4 }
            0x0C => { let r = alu::inc(self.regs.c, self.regs.f); self.regs.c = r.value; self.regs.f = r.flags; 4 }
            0x14 => { let r = alu::inc(self.regs.d, self.regs.f); self.regs.d = r.value; self.regs.f = r.flags; 4 }
            0x1C => { let r = alu::inc(self.regs.e, self.regs.f); self.regs.e = r.value; self.regs.f = r.flags; 4 }
            0x24 => { let r = alu::inc(self.regs.h, self.regs.f); self.regs.h = r.value; self.regs.f = r.flags; 4 }
            0x2C => { let r = alu::inc(self.regs.l, self.regs.f); self.regs.l = r.value; self.regs.f = r.flags; 4 }
            0x3C => { let r = alu::inc(self.regs.a, self.regs.f); self.regs.a = r.value; self.regs.f = r.flags; 4 }

            // ── DEC r8 ───────────────────────────────────────────────────────
            0x05 => { let r = alu::dec(self.regs.b, self.regs.f); self.regs.b = r.value; self.regs.f = r.flags; 4 }
            0x0D => { let r = alu::dec(self.regs.c, self.regs.f); self.regs.c = r.value; self.regs.f = r.flags; 4 }
            0x15 => { let r = alu::dec(self.regs.d, self.regs.f); self.regs.d = r.value; self.regs.f = r.flags; 4 }
            0x1D => { let r = alu::dec(self.regs.e, self.regs.f); self.regs.e = r.value; self.regs.f = r.flags; 4 }
            0x25 => { let r = alu::dec(self.regs.h, self.regs.f); self.regs.h = r.value; self.regs.f = r.flags; 4 }
            0x2D => { let r = alu::dec(self.regs.l, self.regs.f); self.regs.l = r.value; self.regs.f = r.flags; 4 }
            0x3D => { let r = alu::dec(self.regs.a, self.regs.f); self.regs.a = r.value; self.regs.f = r.flags; 4 }

            // ── Register-to-register LD r, r' ────────────────────────────────
            0x40 => 4,
            0x41 => { self.regs.b = self.regs.c; 4 }
            0x42 => { self.regs.b = self.regs.d; 4 }
            0x43 => { self.regs.b = self.regs.e; 4 }
            0x44 => { self.regs.b = self.regs.h; 4 }
            0x45 => { self.regs.b = self.regs.l; 4 }
            0x47 => { self.regs.b = self.regs.a; 4 }

            0x48 => { self.regs.c = self.regs.b; 4 }
            0x49 => 4,
            0x4A => { self.regs.c = self.regs.d; 4 }
            0x4B => { self.regs.c = self.regs.e; 4 }
            0x4C => { self.regs.c = self.regs.h; 4 }
            0x4D => { self.regs.c = self.regs.l; 4 }
            0x4F => { self.regs.c = self.regs.a; 4 }

            0x50 => { self.regs.d = self.regs.b; 4 }
            0x51 => { self.regs.d = self.regs.c; 4 }
            0x52 => 4,
            0x53 => { self.regs.d = self.regs.e; 4 }
            0x54 => { self.regs.d = self.regs.h; 4 }
            0x55 => { self.regs.d = self.regs.l; 4 }
            0x57 => { self.regs.d = self.regs.a; 4 }

            0x58 => { self.regs.e = self.regs.b; 4 }
            0x59 => { self.regs.e = self.regs.c; 4 }
            0x5A => { self.regs.e = self.regs.d; 4 }
            0x5B => 4,
            0x5C => { self.regs.e = self.regs.h; 4 }
            0x5D => { self.regs.e = self.regs.l; 4 }
            0x5F => { self.regs.e = self.regs.a; 4 }

            0x60 => { self.regs.h = self.regs.b; 4 }
            0x61 => { self.regs.h = self.regs.c; 4 }
            0x62 => { self.regs.h = self.regs.d; 4 }
            0x63 => { self.regs.h = self.regs.e; 4 }
            0x64 => 4,
            0x65 => { self.regs.h = self.regs.l; 4 }
            0x67 => { self.regs.h = self.regs.a; 4 }

            0x68 => { self.regs.l = self.regs.b; 4 }
            0x69 => { self.regs.l = self.regs.c; 4 }
            0x6A => { self.regs.l = self.regs.d; 4 }
            0x6B => { self.regs.l = self.regs.e; 4 }
            0x6C => { self.regs.l = self.regs.h; 4 }
            0x6D => 4,
            0x6F => { self.regs.l = self.regs.a; 4 }

            0x78 => { self.regs.a = self.regs.b; 4 }
            0x79 => { self.regs.a = self.regs.c; 4 }
            0x7A => { self.regs.a = self.regs.d; 4 }
            0x7B => { self.regs.a = self.regs.e; 4 }
            0x7C => { self.regs.a = self.regs.h; 4 }
            0x7D => { self.regs.a = self.regs.l; 4 }
            0x7F => 4,

            // ── ADD A, r ─────────────────────────────────────────────────────
            0x80 => { self.alu_add(self.regs.b); 4 }
            0x81 => { self.alu_add(self.regs.c); 4 }
            0x82 => { self.alu_add(self.regs.d); 4 }
            0x83 => { self.alu_add(self.regs.e); 4 }
            0x84 => { self.alu_add(self.regs.h); 4 }
            0x85 => { self.alu_add(self.regs.l); 4 }
            0x87 => { self.alu_add(self.regs.a); 4 }
            0xC6 => { let n = self.fetch_byte(); self.alu_add(n); 8 }

            // ── ADC A, r ─────────────────────────────────────────────────────
            0x88 => { self.alu_adc(self.regs.b); 4 }
            0x89 => { self.alu_adc(self.regs.c); 4 }
            0x8A => { self.alu_adc(self.regs.d); 4 }
            0x8B => { self.alu_adc(self.regs.e); 4 }
            0x8C => { self.alu_adc(self.regs.h); 4 }
            0x8D => { self.alu_adc(self.regs.l); 4 }
            0x8F => { self.alu_adc(self.regs.a); 4 }
            0xCE => { let n = self.fetch_byte(); self.alu_adc(n); 8 }

            // ── SUB A, r ─────────────────────────────────────────────────────
            0x90 => { self.alu_sub(self.regs.b); 4 }
            0x91 => { self.alu_sub(self.regs.c); 4 }
            0x92 => { self.alu_sub(self.regs.d); 4 }
            0x93 => { self.alu_sub(self.regs.e); 4 }
            0x94 => { self.alu_sub(self.regs.h); 4 }
            0x95 => { self.alu_sub(self.regs.l); 4 }
            0x97 => { self.alu_sub(self.regs.a); 4 }
            0xD6 => { let n = self.fetch_byte(); self.alu_sub(n); 8 }

            // ── SBC A, r ─────────────────────────────────────────────────────
            0x98 => { self.alu_sbc(self.regs.b); 4 }
            0x99 => { self.alu_sbc(self.regs.c); 4 }
            0x9A => { self.alu_sbc(self.regs.d); 4 }
            0x9B => { self.alu_sbc(self.regs.e); 4 }
            0x9C => { self.alu_sbc(self.regs.h); 4 }
            0x9D => { self.alu_sbc(self.regs.l); 4 }
            0x9F => { self.alu_sbc(self.regs.a); 4 }
            0xDE => { let n = self.fetch_byte(); self.alu_sbc(n); 8 }

            // ── AND A, r ─────────────────────────────────────────────────────
            0xA0 => { self.alu_and(self.regs.b); 4 }
            0xA1 => { self.alu_and(self.regs.c); 4 }
            0xA2 => { self.alu_and(self.regs.d); 4 }
            0xA3 => { self.alu_and(self.regs.e); 4 }
            0xA4 => { self.alu_and(self.regs.h); 4 }
            0xA5 => { self.alu_and(self.regs.l); 4 }
            0xA7 => { self.alu_and(self.regs.a); 4 }
            0xE6 => { let n = self.fetch_byte(); self.alu_and(n); 8 }

            // ── XOR A, r ─────────────────────────────────────────────────────
            0xA8 => { self.alu_xor(self.regs.b); 4 }
            0xA9 => { self.alu_xor(self.regs.c); 4 }
            0xAA => { self.alu_xor(self.regs.d); 4 }
            0xAB => { self.alu_xor(self.regs.e); 4 }
            0xAC => { self.alu_xor(self.regs.h); 4 }
            0xAD => { self.alu_xor(self.regs.l); 4 }
            0xAF => { self.alu_xor(self.regs.a); 4 }
            0xEE => { let n = self.fetch_byte(); self.alu_xor(n); 8 }

            // ── OR A, r ──────────────────────────────────────────────────────
            0xB0 => { self.alu_or(self.regs.b); 4 }
            0xB1 => { self.alu_or(self.regs.c); 4 }
            0xB2 => { self.alu_or(self.regs.d); 4 }
            0xB3 => { self.alu_or(self.regs.e); 4 }
            0xB4 => { self.alu_or(self.regs.h); 4 }
            0xB5 => { self.alu_or(self.regs.l); 4 }
            0xB7 => { self.alu_or(self.regs.a); 4 }
            0xF6 => { let n = self.fetch_byte(); self.alu_or(n); 8 }

            // ── CP A, r ──────────────────────────────────────────────────────
            0xB8 => { self.alu_cp(self.regs.b); 4 }
            0xB9 => { self.alu_cp(self.regs.c); 4 }
            0xBA => { self.alu_cp(self.regs.d); 4 }
            0xBB => { self.alu_cp(self.regs.e); 4 }
            0xBC => { self.alu_cp(self.regs.h); 4 }
            0xBD => { self.alu_cp(self.regs.l); 4 }
            0xBF => { self.alu_cp(self.regs.a); 4 }
            0xFE => { let n = self.fetch_byte(); self.alu_cp(n); 8 }

            // ── PUSH / POP ───────────────────────────────────────────────────
            0xC1 => { let v = self.stack_pop(); self.regs.set_bc(v); 12 }
            0xD1 => { let v = self.stack_pop(); self.regs.set_de(v); 12 }
            0xE1 => { let v = self.stack_pop(); self.regs.set_hl(v); 12 }
            0xF1 => { let v = self.stack_pop(); self.regs.set_af(v); 12 }
            0xC5 => { let v = self.regs.bc(); self.stack_push(v); 16 }
            0xD5 => { let v = self.regs.de(); self.stack_push(v); 16 }
            0xE5 => { let v = self.regs.hl(); self.stack_push(v); 16 }
            0xF5 => { let v = self.regs.af(); self.stack_push(v); 16 }

            // ── JP ───────────────────────────────────────────────────────────
            0xC3 => { let nn = self.fetch_word(); self.regs.pc = nn; 16 }
            0xC2 => { let nn = self.fetch_word(); if !self.regs.flag_z() { self.regs.pc = nn; 16 } else { 12 } }
            0xCA => { let nn = self.fetch_word(); if  self.regs.flag_z() { self.regs.pc = nn; 16 } else { 12 } }
            0xD2 => { let nn = self.fetch_word(); if !self.regs.flag_c() { self.regs.pc = nn; 16 } else { 12 } }
            0xDA => { let nn = self.fetch_word(); if  self.regs.flag_c() { self.regs.pc = nn; 16 } else { 12 } }

            // ── JR ───────────────────────────────────────────────────────────
            0x18 => { let e = self.fetch_byte() as i8; self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 }
            0x20 => { let e = self.fetch_byte() as i8; if !self.regs.flag_z() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 } }
            0x28 => { let e = self.fetch_byte() as i8; if  self.regs.flag_z() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 } }
            0x30 => { let e = self.fetch_byte() as i8; if !self.regs.flag_c() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 } }
            0x38 => { let e = self.fetch_byte() as i8; if  self.regs.flag_c() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 } }

            // ── CALL ─────────────────────────────────────────────────────────
            0xCD => { let nn = self.fetch_word(); let r = self.regs.pc; self.stack_push(r); self.regs.pc = nn; 24 }
            0xC4 => { let nn = self.fetch_word(); if !self.regs.flag_z() { let r = self.regs.pc; self.stack_push(r); self.regs.pc = nn; 24 } else { 12 } }
            0xCC => { let nn = self.fetch_word(); if  self.regs.flag_z() { let r = self.regs.pc; self.stack_push(r); self.regs.pc = nn; 24 } else { 12 } }
            0xD4 => { let nn = self.fetch_word(); if !self.regs.flag_c() { let r = self.regs.pc; self.stack_push(r); self.regs.pc = nn; 24 } else { 12 } }
            0xDC => { let nn = self.fetch_word(); if  self.regs.flag_c() { let r = self.regs.pc; self.stack_push(r); self.regs.pc = nn; 24 } else { 12 } }

            // ── RET ──────────────────────────────────────────────────────────
            0xC9 => { let a = self.stack_pop(); self.regs.pc = a; 16 }
            0xC0 => { if !self.regs.flag_z() { let a = self.stack_pop(); self.regs.pc = a; 20 } else { 8 } }
            0xC8 => { if  self.regs.flag_z() { let a = self.stack_pop(); self.regs.pc = a; 20 } else { 8 } }
            0xD0 => { if !self.regs.flag_c() { let a = self.stack_pop(); self.regs.pc = a; 20 } else { 8 } }
            0xD8 => { if  self.regs.flag_c() { let a = self.stack_pop(); self.regs.pc = a; 20 } else { 8 } }

            // ── Interrupt control ─────────────────────────────────────────────
            0xF3 => { self.ime = false; 4 }
            0xFB => { self.ime = true;  4 }

            // ── Unimplemented ─────────────────────────────────────────────────
            unknown => {
                log::warn!(
                    "Unimplemented opcode 0x{:02X} at PC=0x{:04X}",
                    unknown,
                    self.regs.pc.wrapping_sub(1)
                );
                4
            }
        }
    }
}