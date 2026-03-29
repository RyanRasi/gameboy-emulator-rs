//! SM83 — complete instruction set implementation.

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
        let v = self.mmu.read_word(self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(2);
        v
    }

    // ── Memory via HL ─────────────────────────────────────────────────────────
    fn read_hl(&self)          -> u8  { self.mmu.read_byte(self.regs.hl()) }
    fn write_hl(&mut self, v: u8)     { self.mmu.write_byte(self.regs.hl(), v); }

    // ── ALU wiring ────────────────────────────────────────────────────────────
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

    // ── 16-bit ADD HL, rr ────────────────────────────────────────────────────
    fn add_hl(&mut self, value: u16) {
        let hl = self.regs.hl();
        let result = hl.wrapping_add(value);
        self.regs.set_flag_n(false);
        self.regs.set_flag_h((hl & 0x0FFF) + (value & 0x0FFF) > 0x0FFF);
        self.regs.set_flag_c((hl as u32) + (value as u32) > 0xFFFF);
        self.regs.set_hl(result);
    }

    // ── ADD SP, r8  (Z=0, N=0, H/C from low byte) ────────────────────────────
    fn add_sp_r8(&mut self, offset: i8) -> u16 {
        let sp = self.regs.sp;
        let off = offset as u16;
        let result = sp.wrapping_add(off);
        self.regs.f = 0; // Z=N=0
        if (sp & 0x000F) + (off & 0x000F) > 0x000F { self.regs.f |= flags::H; }
        if (sp & 0x00FF) + (off & 0x00FF) > 0x00FF { self.regs.f |= flags::C; }
        result
    }

    // ── CB prefix helpers ─────────────────────────────────────────────────────
    fn cb_get(&self, idx: u8) -> u8 {
        match idx {
            0 => self.regs.b, 1 => self.regs.c,
            2 => self.regs.d, 3 => self.regs.e,
            4 => self.regs.h, 5 => self.regs.l,
            6 => self.mmu.read_byte(self.regs.hl()),
            7 => self.regs.a,
            _ => unreachable!(),
        }
    }

    fn cb_set(&mut self, idx: u8, value: u8) {
        match idx {
            0 => self.regs.b = value, 1 => self.regs.c = value,
            2 => self.regs.d = value, 3 => self.regs.e = value,
            4 => self.regs.h = value, 5 => self.regs.l = value,
            6 => { let hl = self.regs.hl(); self.mmu.write_byte(hl, value); }
            7 => self.regs.a = value,
            _ => unreachable!(),
        }
    }

    /// Decode and execute one CB-prefixed instruction.
    fn cb_prefix(&mut self) -> u32 {
        let op  = self.fetch_byte();
        let reg = op & 0x07;
        let val = self.cb_get(reg);
        let hl  = reg == 6; // (HL) costs extra cycles

        match op >> 3 {
            // ── RLC ──────────────────────────────────────────────────────────
            0x00 => {
                let c = val >> 7;
                let r = (val << 1) | c;
                self.regs.f = if r == 0 { flags::Z } else { 0 };
                if c != 0 { self.regs.f |= flags::C; }
                self.cb_set(reg, r);
            }
            // ── RRC ──────────────────────────────────────────────────────────
            0x01 => {
                let c = val & 0x01;
                let r = (val >> 1) | (c << 7);
                self.regs.f = if r == 0 { flags::Z } else { 0 };
                if c != 0 { self.regs.f |= flags::C; }
                self.cb_set(reg, r);
            }
            // ── RL ───────────────────────────────────────────────────────────
            0x02 => {
                let old_c = if self.regs.flag_c() { 1u8 } else { 0 };
                let new_c = val >> 7;
                let r = (val << 1) | old_c;
                self.regs.f = if r == 0 { flags::Z } else { 0 };
                if new_c != 0 { self.regs.f |= flags::C; }
                self.cb_set(reg, r);
            }
            // ── RR ───────────────────────────────────────────────────────────
            0x03 => {
                let old_c = if self.regs.flag_c() { 0x80u8 } else { 0 };
                let new_c = val & 0x01;
                let r = (val >> 1) | old_c;
                self.regs.f = if r == 0 { flags::Z } else { 0 };
                if new_c != 0 { self.regs.f |= flags::C; }
                self.cb_set(reg, r);
            }
            // ── SLA ──────────────────────────────────────────────────────────
            0x04 => {
                let c = val >> 7;
                let r = val << 1;
                self.regs.f = if r == 0 { flags::Z } else { 0 };
                if c != 0 { self.regs.f |= flags::C; }
                self.cb_set(reg, r);
            }
            // ── SRA ──────────────────────────────────────────────────────────
            0x05 => {
                let c = val & 0x01;
                let r = (val >> 1) | (val & 0x80); // sign-extend
                self.regs.f = if r == 0 { flags::Z } else { 0 };
                if c != 0 { self.regs.f |= flags::C; }
                self.cb_set(reg, r);
            }
            // ── SWAP ─────────────────────────────────────────────────────────
            0x06 => {
                let r = (val >> 4) | (val << 4);
                self.regs.f = if r == 0 { flags::Z } else { 0 };
                self.cb_set(reg, r);
            }
            // ── SRL ──────────────────────────────────────────────────────────
            0x07 => {
                let c = val & 0x01;
                let r = val >> 1;
                self.regs.f = if r == 0 { flags::Z } else { 0 };
                if c != 0 { self.regs.f |= flags::C; }
                self.cb_set(reg, r);
            }
            // ── BIT b, r  (8 groups of 8 = indices 8–15) ─────────────────────
            b if b >= 8 && b <= 15 => {
                let bit = b - 8;
                let set = (val >> bit) & 1 != 0;
                self.regs.set_flag_z(!set);
                self.regs.set_flag_n(false);
                self.regs.set_flag_h(true);
                // BIT does not write back
                return if hl { 12 } else { 8 };
            }
            // ── RES b, r ─────────────────────────────────────────────────────
            b if b >= 16 && b <= 23 => {
                let bit = b - 16;
                self.cb_set(reg, val & !(1 << bit));
            }
            // ── SET b, r ─────────────────────────────────────────────────────
            b if b >= 24 && b <= 31 => {
                let bit = b - 24;
                self.cb_set(reg, val | (1 << bit));
            }
            _ => unreachable!(),
        }

        if hl { 16 } else { 8 }
    }

    // =========================================================================
    // Main decode / execute
    // =========================================================================

    pub fn step(&mut self) -> u32 {
        let opcode = self.fetch_byte();
        match opcode {

            // ── NOP ──────────────────────────────────────────────────────────
            0x00 => 4,

            // ── LD (nn), SP ──────────────────────────────────────────────────
            0x08 => {
                let nn = self.fetch_word();
                self.mmu.write_word(nn, self.regs.sp);
                20
            }

            // ── STOP ─────────────────────────────────────────────────────────
            0x10 => { let _ = self.fetch_byte(); 4 }

            // ── HALT ─────────────────────────────────────────────────────────
            0x76 => { self.halted = true; 4 }

            // ── 16-bit immediate loads ─────────────────────────────────────────
            0x01 => { let nn = self.fetch_word(); self.regs.set_bc(nn); 12 }
            0x11 => { let nn = self.fetch_word(); self.regs.set_de(nn); 12 }
            0x21 => { let nn = self.fetch_word(); self.regs.set_hl(nn); 12 }
            0x31 => { let nn = self.fetch_word(); self.regs.sp = nn;    12 }

            // ── 8-bit immediate loads ──────────────────────────────────────────
            0x06 => { let n = self.fetch_byte(); self.regs.b = n; 8 }
            0x0E => { let n = self.fetch_byte(); self.regs.c = n; 8 }
            0x16 => { let n = self.fetch_byte(); self.regs.d = n; 8 }
            0x1E => { let n = self.fetch_byte(); self.regs.e = n; 8 }
            0x26 => { let n = self.fetch_byte(); self.regs.h = n; 8 }
            0x2E => { let n = self.fetch_byte(); self.regs.l = n; 8 }
            0x3E => { let n = self.fetch_byte(); self.regs.a = n; 8 }
            0x36 => { let n = self.fetch_byte(); self.write_hl(n);  12 }

            // ── LD (rr), A ────────────────────────────────────────────────────
            0x02 => { let a = self.regs.a; self.mmu.write_byte(self.regs.bc(), a); 8 }
            0x12 => { let a = self.regs.a; self.mmu.write_byte(self.regs.de(), a); 8 }
            0x22 => { // LD (HL+), A
                let a = self.regs.a; let hl = self.regs.hl();
                self.mmu.write_byte(hl, a);
                self.regs.set_hl(hl.wrapping_add(1)); 8
            }
            0x32 => { // LD (HL-), A
                let a = self.regs.a; let hl = self.regs.hl();
                self.mmu.write_byte(hl, a);
                self.regs.set_hl(hl.wrapping_sub(1)); 8
            }

            // ── LD A, (rr) ────────────────────────────────────────────────────
            0x0A => { self.regs.a = self.mmu.read_byte(self.regs.bc()); 8 }
            0x1A => { self.regs.a = self.mmu.read_byte(self.regs.de()); 8 }
            0x2A => { // LD A, (HL+)
                let hl = self.regs.hl();
                self.regs.a = self.mmu.read_byte(hl);
                self.regs.set_hl(hl.wrapping_add(1)); 8
            }
            0x3A => { // LD A, (HL-)
                let hl = self.regs.hl();
                self.regs.a = self.mmu.read_byte(hl);
                self.regs.set_hl(hl.wrapping_sub(1)); 8
            }

            // ── INC rr ────────────────────────────────────────────────────────
            0x03 => { self.regs.set_bc(self.regs.bc().wrapping_add(1)); 8 }
            0x13 => { self.regs.set_de(self.regs.de().wrapping_add(1)); 8 }
            0x23 => { self.regs.set_hl(self.regs.hl().wrapping_add(1)); 8 }
            0x33 => { self.regs.sp = self.regs.sp.wrapping_add(1);       8 }

            // ── DEC rr ────────────────────────────────────────────────────────
            0x0B => { self.regs.set_bc(self.regs.bc().wrapping_sub(1)); 8 }
            0x1B => { self.regs.set_de(self.regs.de().wrapping_sub(1)); 8 }
            0x2B => { self.regs.set_hl(self.regs.hl().wrapping_sub(1)); 8 }
            0x3B => { self.regs.sp = self.regs.sp.wrapping_sub(1);       8 }

            // ── ADD HL, rr ───────────────────────────────────────────────────
            0x09 => { let v = self.regs.bc(); self.add_hl(v); 8 }
            0x19 => { let v = self.regs.de(); self.add_hl(v); 8 }
            0x29 => { let v = self.regs.hl(); self.add_hl(v); 8 }
            0x39 => { let v = self.regs.sp;   self.add_hl(v); 8 }

            // ── INC r8 ────────────────────────────────────────────────────────
            0x04 => { let r = alu::inc(self.regs.b, self.regs.f); self.regs.b = r.value; self.regs.f = r.flags; 4 }
            0x0C => { let r = alu::inc(self.regs.c, self.regs.f); self.regs.c = r.value; self.regs.f = r.flags; 4 }
            0x14 => { let r = alu::inc(self.regs.d, self.regs.f); self.regs.d = r.value; self.regs.f = r.flags; 4 }
            0x1C => { let r = alu::inc(self.regs.e, self.regs.f); self.regs.e = r.value; self.regs.f = r.flags; 4 }
            0x24 => { let r = alu::inc(self.regs.h, self.regs.f); self.regs.h = r.value; self.regs.f = r.flags; 4 }
            0x2C => { let r = alu::inc(self.regs.l, self.regs.f); self.regs.l = r.value; self.regs.f = r.flags; 4 }
            0x3C => { let r = alu::inc(self.regs.a, self.regs.f); self.regs.a = r.value; self.regs.f = r.flags; 4 }
            0x34 => { // INC (HL)
                let v = self.read_hl();
                let r = alu::inc(v, self.regs.f);
                self.write_hl(r.value); self.regs.f = r.flags; 12
            }

            // ── DEC r8 ────────────────────────────────────────────────────────
            0x05 => { let r = alu::dec(self.regs.b, self.regs.f); self.regs.b = r.value; self.regs.f = r.flags; 4 }
            0x0D => { let r = alu::dec(self.regs.c, self.regs.f); self.regs.c = r.value; self.regs.f = r.flags; 4 }
            0x15 => { let r = alu::dec(self.regs.d, self.regs.f); self.regs.d = r.value; self.regs.f = r.flags; 4 }
            0x1D => { let r = alu::dec(self.regs.e, self.regs.f); self.regs.e = r.value; self.regs.f = r.flags; 4 }
            0x25 => { let r = alu::dec(self.regs.h, self.regs.f); self.regs.h = r.value; self.regs.f = r.flags; 4 }
            0x2D => { let r = alu::dec(self.regs.l, self.regs.f); self.regs.l = r.value; self.regs.f = r.flags; 4 }
            0x3D => { let r = alu::dec(self.regs.a, self.regs.f); self.regs.a = r.value; self.regs.f = r.flags; 4 }
            0x35 => { // DEC (HL)
                let v = self.read_hl();
                let r = alu::dec(v, self.regs.f);
                self.write_hl(r.value); self.regs.f = r.flags; 12
            }

            // ── Rotates (A register, no Z flag) ──────────────────────────────
            0x07 => { // RLCA
                let c = self.regs.a >> 7;
                self.regs.a = (self.regs.a << 1) | c;
                self.regs.f = 0;
                if c != 0 { self.regs.f |= flags::C; }
                4
            }
            0x0F => { // RRCA
                let c = self.regs.a & 0x01;
                self.regs.a = (self.regs.a >> 1) | (c << 7);
                self.regs.f = 0;
                if c != 0 { self.regs.f |= flags::C; }
                4
            }
            0x17 => { // RLA
                let old_c = if self.regs.flag_c() { 1u8 } else { 0 };
                let new_c = self.regs.a >> 7;
                self.regs.a = (self.regs.a << 1) | old_c;
                self.regs.f = 0;
                if new_c != 0 { self.regs.f |= flags::C; }
                4
            }
            0x1F => { // RRA
                let old_c = if self.regs.flag_c() { 0x80u8 } else { 0 };
                let new_c = self.regs.a & 0x01;
                self.regs.a = (self.regs.a >> 1) | old_c;
                self.regs.f = 0;
                if new_c != 0 { self.regs.f |= flags::C; }
                4
            }

            // ── Miscellaneous ─────────────────────────────────────────────────
            0x27 => { // DAA
                let mut a = self.regs.a;
                if !self.regs.flag_n() {
                    if self.regs.flag_h() || (a & 0x0F) > 9  { a = a.wrapping_add(0x06); }
                    if self.regs.flag_c() || a > 0x99         { a = a.wrapping_add(0x60); self.regs.set_flag_c(true); }
                } else {
                    if self.regs.flag_h() { a = a.wrapping_sub(0x06); }
                    if self.regs.flag_c() { a = a.wrapping_sub(0x60); }
                }
                self.regs.set_flag_z(a == 0);
                self.regs.set_flag_h(false);
                self.regs.a = a;
                4
            }
            0x2F => { // CPL
                self.regs.a = !self.regs.a;
                self.regs.set_flag_n(true);
                self.regs.set_flag_h(true);
                4
            }
            0x37 => { // SCF
                self.regs.set_flag_n(false);
                self.regs.set_flag_h(false);
                self.regs.set_flag_c(true);
                4
            }
            0x3F => { // CCF
                let c = self.regs.flag_c();
                self.regs.set_flag_n(false);
                self.regs.set_flag_h(false);
                self.regs.set_flag_c(!c);
                4
            }

            // ── JR ───────────────────────────────────────────────────────────
            0x18 => { let e = self.fetch_byte() as i8; self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 }
            0x20 => { let e = self.fetch_byte() as i8; if !self.regs.flag_z() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 } }
            0x28 => { let e = self.fetch_byte() as i8; if  self.regs.flag_z() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 } }
            0x30 => { let e = self.fetch_byte() as i8; if !self.regs.flag_c() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 } }
            0x38 => { let e = self.fetch_byte() as i8; if  self.regs.flag_c() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 } }

            // ── LD r, r' ──────────────────────────────────────────────────────
            0x40 => 4,
            0x41 => { self.regs.b = self.regs.c; 4 }
            0x42 => { self.regs.b = self.regs.d; 4 }
            0x43 => { self.regs.b = self.regs.e; 4 }
            0x44 => { self.regs.b = self.regs.h; 4 }
            0x45 => { self.regs.b = self.regs.l; 4 }
            0x46 => { self.regs.b = self.read_hl(); 8 }
            0x47 => { self.regs.b = self.regs.a; 4 }

            0x48 => { self.regs.c = self.regs.b; 4 }
            0x49 => 4,
            0x4A => { self.regs.c = self.regs.d; 4 }
            0x4B => { self.regs.c = self.regs.e; 4 }
            0x4C => { self.regs.c = self.regs.h; 4 }
            0x4D => { self.regs.c = self.regs.l; 4 }
            0x4E => { self.regs.c = self.read_hl(); 8 }
            0x4F => { self.regs.c = self.regs.a; 4 }

            0x50 => { self.regs.d = self.regs.b; 4 }
            0x51 => { self.regs.d = self.regs.c; 4 }
            0x52 => 4,
            0x53 => { self.regs.d = self.regs.e; 4 }
            0x54 => { self.regs.d = self.regs.h; 4 }
            0x55 => { self.regs.d = self.regs.l; 4 }
            0x56 => { self.regs.d = self.read_hl(); 8 }
            0x57 => { self.regs.d = self.regs.a; 4 }

            0x58 => { self.regs.e = self.regs.b; 4 }
            0x59 => { self.regs.e = self.regs.c; 4 }
            0x5A => { self.regs.e = self.regs.d; 4 }
            0x5B => 4,
            0x5C => { self.regs.e = self.regs.h; 4 }
            0x5D => { self.regs.e = self.regs.l; 4 }
            0x5E => { self.regs.e = self.read_hl(); 8 }
            0x5F => { self.regs.e = self.regs.a; 4 }

            0x60 => { self.regs.h = self.regs.b; 4 }
            0x61 => { self.regs.h = self.regs.c; 4 }
            0x62 => { self.regs.h = self.regs.d; 4 }
            0x63 => { self.regs.h = self.regs.e; 4 }
            0x64 => 4,
            0x65 => { self.regs.h = self.regs.l; 4 }
            0x66 => { self.regs.h = self.read_hl(); 8 }
            0x67 => { self.regs.h = self.regs.a; 4 }

            0x68 => { self.regs.l = self.regs.b; 4 }
            0x69 => { self.regs.l = self.regs.c; 4 }
            0x6A => { self.regs.l = self.regs.d; 4 }
            0x6B => { self.regs.l = self.regs.e; 4 }
            0x6C => { self.regs.l = self.regs.h; 4 }
            0x6D => 4,
            0x6E => { self.regs.l = self.read_hl(); 8 }
            0x6F => { self.regs.l = self.regs.a; 4 }

            0x70 => { let v = self.regs.b; self.write_hl(v); 8 }
            0x71 => { let v = self.regs.c; self.write_hl(v); 8 }
            0x72 => { let v = self.regs.d; self.write_hl(v); 8 }
            0x73 => { let v = self.regs.e; self.write_hl(v); 8 }
            0x74 => { let v = self.regs.h; self.write_hl(v); 8 }
            0x75 => { let v = self.regs.l; self.write_hl(v); 8 }
            0x77 => { let v = self.regs.a; self.write_hl(v); 8 }

            0x78 => { self.regs.a = self.regs.b; 4 }
            0x79 => { self.regs.a = self.regs.c; 4 }
            0x7A => { self.regs.a = self.regs.d; 4 }
            0x7B => { self.regs.a = self.regs.e; 4 }
            0x7C => { self.regs.a = self.regs.h; 4 }
            0x7D => { self.regs.a = self.regs.l; 4 }
            0x7E => { self.regs.a = self.read_hl(); 8 }
            0x7F => 4,

            // ── ADD A ─────────────────────────────────────────────────────────
            0x80 => { self.alu_add(self.regs.b); 4 }
            0x81 => { self.alu_add(self.regs.c); 4 }
            0x82 => { self.alu_add(self.regs.d); 4 }
            0x83 => { self.alu_add(self.regs.e); 4 }
            0x84 => { self.alu_add(self.regs.h); 4 }
            0x85 => { self.alu_add(self.regs.l); 4 }
            0x86 => { let v = self.read_hl(); self.alu_add(v); 8 }
            0x87 => { self.alu_add(self.regs.a); 4 }
            0xC6 => { let n = self.fetch_byte(); self.alu_add(n); 8 }

            // ── ADC A ─────────────────────────────────────────────────────────
            0x88 => { self.alu_adc(self.regs.b); 4 }
            0x89 => { self.alu_adc(self.regs.c); 4 }
            0x8A => { self.alu_adc(self.regs.d); 4 }
            0x8B => { self.alu_adc(self.regs.e); 4 }
            0x8C => { self.alu_adc(self.regs.h); 4 }
            0x8D => { self.alu_adc(self.regs.l); 4 }
            0x8E => { let v = self.read_hl(); self.alu_adc(v); 8 }
            0x8F => { self.alu_adc(self.regs.a); 4 }
            0xCE => { let n = self.fetch_byte(); self.alu_adc(n); 8 }

            // ── SUB A ─────────────────────────────────────────────────────────
            0x90 => { self.alu_sub(self.regs.b); 4 }
            0x91 => { self.alu_sub(self.regs.c); 4 }
            0x92 => { self.alu_sub(self.regs.d); 4 }
            0x93 => { self.alu_sub(self.regs.e); 4 }
            0x94 => { self.alu_sub(self.regs.h); 4 }
            0x95 => { self.alu_sub(self.regs.l); 4 }
            0x96 => { let v = self.read_hl(); self.alu_sub(v); 8 }
            0x97 => { self.alu_sub(self.regs.a); 4 }
            0xD6 => { let n = self.fetch_byte(); self.alu_sub(n); 8 }

            // ── SBC A ─────────────────────────────────────────────────────────
            0x98 => { self.alu_sbc(self.regs.b); 4 }
            0x99 => { self.alu_sbc(self.regs.c); 4 }
            0x9A => { self.alu_sbc(self.regs.d); 4 }
            0x9B => { self.alu_sbc(self.regs.e); 4 }
            0x9C => { self.alu_sbc(self.regs.h); 4 }
            0x9D => { self.alu_sbc(self.regs.l); 4 }
            0x9E => { let v = self.read_hl(); self.alu_sbc(v); 8 }
            0x9F => { self.alu_sbc(self.regs.a); 4 }
            0xDE => { let n = self.fetch_byte(); self.alu_sbc(n); 8 }

            // ── AND A ─────────────────────────────────────────────────────────
            0xA0 => { self.alu_and(self.regs.b); 4 }
            0xA1 => { self.alu_and(self.regs.c); 4 }
            0xA2 => { self.alu_and(self.regs.d); 4 }
            0xA3 => { self.alu_and(self.regs.e); 4 }
            0xA4 => { self.alu_and(self.regs.h); 4 }
            0xA5 => { self.alu_and(self.regs.l); 4 }
            0xA6 => { let v = self.read_hl(); self.alu_and(v); 8 }
            0xA7 => { self.alu_and(self.regs.a); 4 }
            0xE6 => { let n = self.fetch_byte(); self.alu_and(n); 8 }

            // ── XOR A ─────────────────────────────────────────────────────────
            0xA8 => { self.alu_xor(self.regs.b); 4 }
            0xA9 => { self.alu_xor(self.regs.c); 4 }
            0xAA => { self.alu_xor(self.regs.d); 4 }
            0xAB => { self.alu_xor(self.regs.e); 4 }
            0xAC => { self.alu_xor(self.regs.h); 4 }
            0xAD => { self.alu_xor(self.regs.l); 4 }
            0xAE => { let v = self.read_hl(); self.alu_xor(v); 8 }
            0xAF => { self.alu_xor(self.regs.a); 4 }
            0xEE => { let n = self.fetch_byte(); self.alu_xor(n); 8 }

            // ── OR A ──────────────────────────────────────────────────────────
            0xB0 => { self.alu_or(self.regs.b); 4 }
            0xB1 => { self.alu_or(self.regs.c); 4 }
            0xB2 => { self.alu_or(self.regs.d); 4 }
            0xB3 => { self.alu_or(self.regs.e); 4 }
            0xB4 => { self.alu_or(self.regs.h); 4 }
            0xB5 => { self.alu_or(self.regs.l); 4 }
            0xB6 => { let v = self.read_hl(); self.alu_or(v); 8 }
            0xB7 => { self.alu_or(self.regs.a); 4 }
            0xF6 => { let n = self.fetch_byte(); self.alu_or(n); 8 }

            // ── CP A ──────────────────────────────────────────────────────────
            0xB8 => { self.alu_cp(self.regs.b); 4 }
            0xB9 => { self.alu_cp(self.regs.c); 4 }
            0xBA => { self.alu_cp(self.regs.d); 4 }
            0xBB => { self.alu_cp(self.regs.e); 4 }
            0xBC => { self.alu_cp(self.regs.h); 4 }
            0xBD => { self.alu_cp(self.regs.l); 4 }
            0xBE => { let v = self.read_hl(); self.alu_cp(v); 8 }
            0xBF => { self.alu_cp(self.regs.a); 4 }
            0xFE => { let n = self.fetch_byte(); self.alu_cp(n); 8 }

            // ── PUSH / POP ────────────────────────────────────────────────────
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
            0xE9 => { self.regs.pc = self.regs.hl(); 4 } // JP (HL)
            0xC2 => { let nn = self.fetch_word(); if !self.regs.flag_z() { self.regs.pc = nn; 16 } else { 12 } }
            0xCA => { let nn = self.fetch_word(); if  self.regs.flag_z() { self.regs.pc = nn; 16 } else { 12 } }
            0xD2 => { let nn = self.fetch_word(); if !self.regs.flag_c() { self.regs.pc = nn; 16 } else { 12 } }
            0xDA => { let nn = self.fetch_word(); if  self.regs.flag_c() { self.regs.pc = nn; 16 } else { 12 } }

            // ── CALL ─────────────────────────────────────────────────────────
            0xCD => { let nn = self.fetch_word(); let r = self.regs.pc; self.stack_push(r); self.regs.pc = nn; 24 }
            0xC4 => { let nn = self.fetch_word(); if !self.regs.flag_z() { let r = self.regs.pc; self.stack_push(r); self.regs.pc = nn; 24 } else { 12 } }
            0xCC => { let nn = self.fetch_word(); if  self.regs.flag_z() { let r = self.regs.pc; self.stack_push(r); self.regs.pc = nn; 24 } else { 12 } }
            0xD4 => { let nn = self.fetch_word(); if !self.regs.flag_c() { let r = self.regs.pc; self.stack_push(r); self.regs.pc = nn; 24 } else { 12 } }
            0xDC => { let nn = self.fetch_word(); if  self.regs.flag_c() { let r = self.regs.pc; self.stack_push(r); self.regs.pc = nn; 24 } else { 12 } }

            // ── RET ──────────────────────────────────────────────────────────
            0xC9 => { let a = self.stack_pop(); self.regs.pc = a; 16 }
            0xD9 => { // RETI
                let a = self.stack_pop(); self.regs.pc = a;
                self.ime = true; 16
            }
            0xC0 => { if !self.regs.flag_z() { let a = self.stack_pop(); self.regs.pc = a; 20 } else { 8 } }
            0xC8 => { if  self.regs.flag_z() { let a = self.stack_pop(); self.regs.pc = a; 20 } else { 8 } }
            0xD0 => { if !self.regs.flag_c() { let a = self.stack_pop(); self.regs.pc = a; 20 } else { 8 } }
            0xD8 => { if  self.regs.flag_c() { let a = self.stack_pop(); self.regs.pc = a; 20 } else { 8 } }

            // ── RST ──────────────────────────────────────────────────────────
            0xC7 => { let r = self.regs.pc; self.stack_push(r); self.regs.pc = 0x0000; 16 }
            0xCF => { let r = self.regs.pc; self.stack_push(r); self.regs.pc = 0x0008; 16 }
            0xD7 => { let r = self.regs.pc; self.stack_push(r); self.regs.pc = 0x0010; 16 }
            0xDF => { let r = self.regs.pc; self.stack_push(r); self.regs.pc = 0x0018; 16 }
            0xE7 => { let r = self.regs.pc; self.stack_push(r); self.regs.pc = 0x0020; 16 }
            0xEF => { let r = self.regs.pc; self.stack_push(r); self.regs.pc = 0x0028; 16 }
            0xF7 => { let r = self.regs.pc; self.stack_push(r); self.regs.pc = 0x0030; 16 }
            0xFF => { let r = self.regs.pc; self.stack_push(r); self.regs.pc = 0x0038; 16 }

            // ── High memory loads ─────────────────────────────────────────────
            0xE0 => { // LDH (n), A
                let n = self.fetch_byte();
                let a = self.regs.a;
                self.mmu.write_byte(0xFF00 | n as u16, a); 12
            }
            0xF0 => { // LDH A, (n)
                let n = self.fetch_byte();
                self.regs.a = self.mmu.read_byte(0xFF00 | n as u16); 12
            }
            0xE2 => { // LD (C), A
                let addr = 0xFF00 | self.regs.c as u16;
                let a = self.regs.a;
                self.mmu.write_byte(addr, a); 8
            }
            0xF2 => { // LD A, (C)
                let addr = 0xFF00 | self.regs.c as u16;
                self.regs.a = self.mmu.read_byte(addr); 8
            }
            0xEA => { // LD (nn), A
                let nn = self.fetch_word();
                let a = self.regs.a;
                self.mmu.write_byte(nn, a); 16
            }
            0xFA => { // LD A, (nn)
                let nn = self.fetch_word();
                self.regs.a = self.mmu.read_byte(nn); 16
            }

            // ── SP / HL transfers ─────────────────────────────────────────────
            0xF9 => { self.regs.sp = self.regs.hl(); 8 } // LD SP, HL
            0xF8 => { // LD HL, SP+r8
                let e = self.fetch_byte() as i8;
                let result = self.add_sp_r8(e);
                self.regs.set_hl(result); 12
            }
            0xE8 => { // ADD SP, r8
                let e = self.fetch_byte() as i8;
                self.regs.sp = self.add_sp_r8(e); 16
            }

            // ── Interrupt control ─────────────────────────────────────────────
            0xF3 => { self.ime = false; 4 } // DI
            0xFB => { self.ime = true;  4 } // EI

            // ── CB prefix ────────────────────────────────────────────────────
            0xCB => { let cycles = self.cb_prefix(); cycles + 4 } // +4 for CB fetch

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