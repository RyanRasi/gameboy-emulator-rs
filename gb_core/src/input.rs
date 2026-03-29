//! Game Boy Joypad input subsystem.
//!
//! The joypad is accessed through a single register at 0xFF00 (P1/JOYP).
//!
//! The register is bidirectional:
//!   Bits 5–4 (write): Select button group to read
//!     Bit 5 = 0 → select action  buttons (A, B, Select, Start)
//!     Bit 4 = 0 → select d-pad   buttons (Right, Left, Up, Down)
//!
//!   Bits 3–0 (read): Button state for selected group
//!     0 = pressed, 1 = released  (active-low logic)
//!
//! Layout within each nibble (bits 3–0):
//!   Action group:  bit 3=Start  bit 2=Select  bit 1=B  bit 0=A
//!   D-pad group:   bit 3=Down   bit 2=Up      bit 1=Left bit 0=Right
//!
//! A Joypad interrupt (bit 4 of IF) is requested when any button
//! transitions from released → pressed.

use crate::mmu::Mmu;

/// I/O address of the joypad register.
pub const JOYP_ADDR: u16 = 0xFF00;

/// Button identifiers — used as indices into the button state array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    // Action buttons
    A      = 0,
    B      = 1,
    Select = 2,
    Start  = 3,
    // D-pad
    Right  = 4,
    Left   = 5,
    Up     = 6,
    Down   = 7,
}

impl Button {
    /// All eight buttons in order, for iteration.
    pub const ALL: [Button; 8] = [
        Button::A, Button::B, Button::Select, Button::Start,
        Button::Right, Button::Left, Button::Up, Button::Down,
    ];
}

/// Tracks the pressed/released state of all eight Game Boy buttons and
/// generates the correct byte for register 0xFF00.
pub struct Joypad {
    /// pressed[i] is true when Button with discriminant i is held down.
    pressed: [bool; 8],

    /// Tracks whether a Joypad interrupt should be requested on the next
    /// `sync` call (set when any button is newly pressed).
    pub irq_pending: bool,
}

impl Joypad {
    pub fn new() -> Self {
        Joypad {
            pressed:     [false; 8],
            irq_pending: false,
        }
    }

    /// Press a button. If the button was previously released this sets
    /// `irq_pending` so the CPU can request a Joypad interrupt.
    pub fn press(&mut self, button: Button) {
        if !self.pressed[button as usize] {
            self.pressed[button as usize] = true;
            self.irq_pending = true;
        }
    }

    /// Release a button.
    pub fn release(&mut self, button: Button) {
        self.pressed[button as usize] = false;
    }

    /// Returns true if the given button is currently held.
    pub fn is_pressed(&self, button: Button) -> bool {
        self.pressed[button as usize]
    }

    /// Compute the byte that should appear when the CPU reads 0xFF00,
    /// based on which group the CPU has selected (written to bits 5–4).
    ///
    /// Bits 5–4 reflect the select lines written by the CPU.
    /// Bits 3–0 are active-low button states for the selected group.
    /// Bits 7–6 are always 1 (open bus).
    pub fn read_joyp(&self, mmu: &Mmu) -> u8 {
        let select = mmu.read_byte(JOYP_ADDR);
        let action_sel = select & 0x20 == 0; // bit 5 low → action group
        let dpad_sel   = select & 0x10 == 0; // bit 4 low → d-pad group

        // Start with all bits high (nothing pressed) + preserve select bits
        let mut lo: u8 = 0x0F;

        if action_sel {
            if self.pressed[Button::A      as usize] { lo &= !0x01; }
            if self.pressed[Button::B      as usize] { lo &= !0x02; }
            if self.pressed[Button::Select as usize] { lo &= !0x04; }
            if self.pressed[Button::Start  as usize] { lo &= !0x08; }
        }

        if dpad_sel {
            if self.pressed[Button::Right as usize] { lo &= !0x01; }
            if self.pressed[Button::Left  as usize] { lo &= !0x02; }
            if self.pressed[Button::Up    as usize] { lo &= !0x04; }
            if self.pressed[Button::Down  as usize] { lo &= !0x08; }
        }

        // Bits 7–6 open bus (1), bits 5–4 from select, bits 3–0 active-low
        0xC0 | (select & 0x30) | lo
    }

    /// Write the CPU's select byte into 0xFF00, then update the readable
    /// value so subsequent reads return the correct button state.
    pub fn write_joyp(&self, mmu: &mut Mmu, value: u8) {
        // Only bits 5–4 are writable; preserve them then recompute the output
        let select = value & 0x30;
        mmu.write_byte(JOYP_ADDR, 0xC0 | select | 0x0F); // placeholder
    }

    /// Synchronise the joypad output byte in the MMU with current button
    /// state. Call this once per CPU tick after any input events.
    /// Returns true if a Joypad interrupt should be requested.
    pub fn sync(&mut self, mmu: &mut Mmu) -> bool {
        let current = mmu.read_byte(JOYP_ADDR);
        let updated = self.read_joyp(mmu);
        mmu.write_byte(JOYP_ADDR, updated);

        // Joypad IRQ fires when any input bit transitions 1→0 (released→pressed)
        let newly_pressed = (!updated & current) & 0x0F;
        let fire_irq = newly_pressed != 0 && self.irq_pending;
        self.irq_pending = false;
        fire_irq
    }
}

impl Default for Joypad {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Mmu;

    fn setup() -> (Joypad, Mmu) {
        let joy = Joypad::new();
        let mut mmu = Mmu::new();
        // Default: neither group selected (bits 5–4 both high → no group)
        mmu.write_byte(JOYP_ADDR, 0xFF);
        (joy, mmu)
    }

    /// Select the action button group only (bit 5 low, bit 4 high).
    fn select_action(mmu: &mut Mmu) {
        mmu.write_byte(JOYP_ADDR, 0xDF); // 1101_1111 — bit5=0, bit4=1
    }

    /// Select the d-pad group only (bit 4 low, bit 5 high).
    fn select_dpad(mmu: &mut Mmu) {
        mmu.write_byte(JOYP_ADDR, 0xEF); // 1110_1111 — bit5=1, bit4=0
    }

    // ── Initial state ─────────────────────────────────────────────────────────

    #[test]
    fn test_initial_no_buttons_pressed() {
        let (joy, _) = setup();
        for btn in Button::ALL {
            assert!(!joy.is_pressed(btn), "{:?} must start released", btn);
        }
    }

    #[test]
    fn test_initial_irq_pending_false() {
        let (joy, _) = setup();
        assert!(!joy.irq_pending);
    }

    // ── press / release ───────────────────────────────────────────────────────

    #[test]
    fn test_press_marks_button_as_pressed() {
        let (mut joy, _) = setup();
        joy.press(Button::A);
        assert!(joy.is_pressed(Button::A));
    }

    #[test]
    fn test_release_clears_button() {
        let (mut joy, _) = setup();
        joy.press(Button::A);
        joy.release(Button::A);
        assert!(!joy.is_pressed(Button::A));
    }

    #[test]
    fn test_press_sets_irq_pending() {
        let (mut joy, _) = setup();
        joy.press(Button::Start);
        assert!(joy.irq_pending);
    }

    #[test]
    fn test_press_already_pressed_does_not_set_irq_again() {
        let (mut joy, _) = setup();
        joy.press(Button::B);
        joy.irq_pending = false; // clear manually
        joy.press(Button::B);   // already pressed
        assert!(!joy.irq_pending, "Holding a pressed button must not re-trigger IRQ");
    }

    #[test]
    fn test_release_does_not_set_irq_pending() {
        let (mut joy, _) = setup();
        joy.press(Button::A);
        joy.irq_pending = false;
        joy.release(Button::A);
        assert!(!joy.irq_pending);
    }

    #[test]
    fn test_multiple_buttons_pressed_independently() {
        let (mut joy, _) = setup();
        joy.press(Button::A);
        joy.press(Button::Up);
        joy.press(Button::Start);
        assert!(joy.is_pressed(Button::A));
        assert!(joy.is_pressed(Button::Up));
        assert!(joy.is_pressed(Button::Start));
        assert!(!joy.is_pressed(Button::B));
    }

    // ── read_joyp — action group ──────────────────────────────────────────────

    #[test]
    fn test_read_joyp_action_group_no_buttons_returns_0xcf() {
        let (joy, mut mmu) = setup();
        select_action(&mut mmu);
        let byte = joy.read_joyp(&mmu);
        // Bits 3–0 all high (nothing pressed), bit5=0 (selected), bit4=1, bits7-6=1
        assert_eq!(byte & 0x0F, 0x0F, "All action bits must be high when nothing pressed");
    }

    #[test]
    fn test_read_joyp_action_a_pressed() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        joy.press(Button::A);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x01, 0, "Bit 0 must be low when A is pressed");
        assert_eq!(byte & 0x0E, 0x0E, "Other action bits must stay high");
    }

    #[test]
    fn test_read_joyp_action_b_pressed() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        joy.press(Button::B);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x02, 0, "Bit 1 must be low when B is pressed");
    }

    #[test]
    fn test_read_joyp_action_select_pressed() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        joy.press(Button::Select);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x04, 0, "Bit 2 must be low when Select is pressed");
    }

    #[test]
    fn test_read_joyp_action_start_pressed() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        joy.press(Button::Start);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x08, 0, "Bit 3 must be low when Start is pressed");
    }

    #[test]
    fn test_read_joyp_multiple_action_buttons_pressed() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        joy.press(Button::A);
        joy.press(Button::Start);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x01, 0, "A bit must be low");
        assert_eq!(byte & 0x08, 0, "Start bit must be low");
        assert_ne!(byte & 0x02, 0, "B bit must stay high");
        assert_ne!(byte & 0x04, 0, "Select bit must stay high");
    }

    // ── read_joyp — d-pad group ───────────────────────────────────────────────

    #[test]
    fn test_read_joyp_dpad_no_buttons_returns_high_nibble() {
        let (joy, mut mmu) = setup();
        select_dpad(&mut mmu);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x0F, 0x0F);
    }

    #[test]
    fn test_read_joyp_dpad_right_pressed() {
        let (mut joy, mut mmu) = setup();
        select_dpad(&mut mmu);
        joy.press(Button::Right);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x01, 0, "Bit 0 must be low when Right is pressed");
    }

    #[test]
    fn test_read_joyp_dpad_left_pressed() {
        let (mut joy, mut mmu) = setup();
        select_dpad(&mut mmu);
        joy.press(Button::Left);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x02, 0, "Bit 1 must be low when Left is pressed");
    }

    #[test]
    fn test_read_joyp_dpad_up_pressed() {
        let (mut joy, mut mmu) = setup();
        select_dpad(&mut mmu);
        joy.press(Button::Up);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x04, 0, "Bit 2 must be low when Up is pressed");
    }

    #[test]
    fn test_read_joyp_dpad_down_pressed() {
        let (mut joy, mut mmu) = setup();
        select_dpad(&mut mmu);
        joy.press(Button::Down);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x08, 0, "Bit 3 must be low when Down is pressed");
    }

    // ── group isolation ───────────────────────────────────────────────────────

    #[test]
    fn test_action_button_not_visible_in_dpad_group() {
        let (mut joy, mut mmu) = setup();
        select_dpad(&mut mmu);
        joy.press(Button::A); // action button
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x01, 0x01, "A must not appear in d-pad group");
    }

    #[test]
    fn test_dpad_button_not_visible_in_action_group() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        joy.press(Button::Right); // d-pad button
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x01, 0x01, "Right must not appear in action group");
    }

    #[test]
    fn test_no_group_selected_all_bits_high() {
        let (mut joy, mut mmu) = setup();
        // Both bits 5 and 4 high → neither group selected
        mmu.write_byte(JOYP_ADDR, 0xFF);
        joy.press(Button::A);
        joy.press(Button::Down);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x0F, 0x0F, "No group selected → all bits high");
    }

    // ── release resets state ──────────────────────────────────────────────────

    #[test]
    fn test_release_restores_high_bit_in_register() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        joy.press(Button::A);
        joy.release(Button::A);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x01, 0x01, "Released button must read high again");
    }

    #[test]
    fn test_release_one_button_does_not_affect_others() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        joy.press(Button::A);
        joy.press(Button::B);
        joy.release(Button::A);
        let byte = joy.read_joyp(&mmu);
        assert_eq!(byte & 0x01, 0x01, "A must be released");
        assert_eq!(byte & 0x02, 0x00, "B must still be pressed");
    }

    // ── sync ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_sync_updates_mmu_register() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        joy.press(Button::A);
        joy.sync(&mut mmu);
        let byte = mmu.read_byte(JOYP_ADDR);
        assert_eq!(byte & 0x01, 0, "A must read low after sync");
    }

    #[test]
    fn test_sync_returns_true_on_new_press() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        // First sync with nothing pressed — establish baseline
        joy.sync(&mut mmu);
        joy.press(Button::B);
        let irq = joy.sync(&mut mmu);
        assert!(irq, "sync must return true when a new button was pressed");
    }

    #[test]
    fn test_sync_returns_false_when_no_new_press() {
        let (mut joy, mut mmu) = setup();
        let irq = joy.sync(&mut mmu);
        assert!(!irq, "sync must return false when nothing changed");
    }

    #[test]
    fn test_sync_returns_false_on_release() {
        let (mut joy, mut mmu) = setup();
        select_action(&mut mmu);
        joy.press(Button::A);
        joy.sync(&mut mmu);  // consume the press IRQ
        joy.release(Button::A);
        let irq = joy.sync(&mut mmu);
        assert!(!irq, "sync must not fire IRQ on release");
    }

    #[test]
    fn test_sync_clears_irq_pending_after_call() {
        let (mut joy, mut mmu) = setup();
        joy.press(Button::Start);
        assert!(joy.irq_pending);
        joy.sync(&mut mmu);
        assert!(!joy.irq_pending, "irq_pending must clear after sync");
    }

    #[test]
    fn test_sync_does_not_fire_irq_when_group_not_selected() {
        // Button is pressed but neither group is selected (bits 5–4 both high)
        // so the output bits don't change → no IRQ
        let (mut joy, mut mmu) = setup();
        mmu.write_byte(JOYP_ADDR, 0xFF); // no group selected
        joy.sync(&mut mmu);              // baseline
        joy.press(Button::A);
        let irq = joy.sync(&mut mmu);
        assert!(!irq, "No IRQ when group not selected (bit can't go 1→0)");
    }
}