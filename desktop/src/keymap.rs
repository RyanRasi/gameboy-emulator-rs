//! Keyboard → Game Boy button mapping.
//!
//! Default bindings:
//!   Z          → A
//!   X          → B
//!   Enter      → Start
//!   Backspace  → Select
//!   Arrow keys → D-pad

use minifb::Key;
use core::input::Button;

/// Map a minifb `Key` to a Game Boy `Button`.
/// Returns `None` for unmapped keys.
pub fn key_to_button(key: Key) -> Option<Button> {
    match key {
        Key::Z         => Some(Button::A),
        Key::X         => Some(Button::B),
        Key::Enter     => Some(Button::Start),
        Key::Backspace => Some(Button::Select),
        Key::Right     => Some(Button::Right),
        Key::Left      => Some(Button::Left),
        Key::Up        => Some(Button::Up),
        Key::Down      => Some(Button::Down),
        _              => None,
    }
}

/// Detect newly pressed keys by comparing the current and previous key sets.
/// Returns a `Vec` of keys that appear in `current` but not in `previous`.
pub fn newly_pressed<'a>(current: &'a [Key], previous: &[Key]) -> Vec<Key> {
    current
        .iter()
        .copied()
        .filter(|k| !previous.contains(k))
        .collect()
}

/// Detect newly released keys.
/// Returns a `Vec` of keys that appear in `previous` but not in `current`.
pub fn newly_released<'a>(previous: &'a [Key], current: &[Key]) -> Vec<Key> {
    previous
        .iter()
        .copied()
        .filter(|k| !current.contains(k))
        .collect()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── key_to_button ─────────────────────────────────────────────────────────

    #[test]
    fn test_z_maps_to_a() {
        assert_eq!(key_to_button(Key::Z), Some(Button::A));
    }

    #[test]
    fn test_x_maps_to_b() {
        assert_eq!(key_to_button(Key::X), Some(Button::B));
    }

    #[test]
    fn test_enter_maps_to_start() {
        assert_eq!(key_to_button(Key::Enter), Some(Button::Start));
    }

    #[test]
    fn test_backspace_maps_to_select() {
        assert_eq!(key_to_button(Key::Backspace), Some(Button::Select));
    }

    #[test]
    fn test_right_maps_to_right() {
        assert_eq!(key_to_button(Key::Right), Some(Button::Right));
    }

    #[test]
    fn test_left_maps_to_left() {
        assert_eq!(key_to_button(Key::Left), Some(Button::Left));
    }

    #[test]
    fn test_up_maps_to_up() {
        assert_eq!(key_to_button(Key::Up), Some(Button::Up));
    }

    #[test]
    fn test_down_maps_to_down() {
        assert_eq!(key_to_button(Key::Down), Some(Button::Down));
    }

    #[test]
    fn test_unmapped_key_returns_none() {
        assert_eq!(key_to_button(Key::A), None);
        assert_eq!(key_to_button(Key::F1), None);
        assert_eq!(key_to_button(Key::Space), None);
    }

    #[test]
    fn test_all_eight_buttons_have_a_mapping() {
        let mapped: Vec<Button> = [
            Key::Z, Key::X, Key::Enter, Key::Backspace,
            Key::Right, Key::Left, Key::Up, Key::Down,
        ]
        .iter()
        .filter_map(|&k| key_to_button(k))
        .collect();
        assert_eq!(mapped.len(), 8, "All 8 Game Boy buttons must have a key binding");
    }

    #[test]
    fn test_all_mappings_are_distinct() {
        let buttons: Vec<Button> = [
            Key::Z, Key::X, Key::Enter, Key::Backspace,
            Key::Right, Key::Left, Key::Up, Key::Down,
        ]
        .iter()
        .filter_map(|&k| key_to_button(k))
        .collect();
        // No two keys map to the same button
        for i in 0..buttons.len() {
            for j in (i + 1)..buttons.len() {
                assert_ne!(
                    buttons[i], buttons[j],
                    "Two keys must not map to the same button"
                );
            }
        }
    }

    // ── newly_pressed ─────────────────────────────────────────────────────────

    #[test]
    fn test_newly_pressed_empty_previous() {
        let current  = vec![Key::Z, Key::Up];
        let previous = vec![];
        let pressed = newly_pressed(&current, &previous);
        assert!(pressed.contains(&Key::Z));
        assert!(pressed.contains(&Key::Up));
    }

    #[test]
    fn test_newly_pressed_no_change() {
        let keys = vec![Key::Z];
        let pressed = newly_pressed(&keys, &keys);
        assert!(pressed.is_empty());
    }

    #[test]
    fn test_newly_pressed_detects_new_key() {
        let previous = vec![Key::Z];
        let current  = vec![Key::Z, Key::X];
        let pressed = newly_pressed(&current, &previous);
        assert_eq!(pressed, vec![Key::X]);
    }

    #[test]
    fn test_newly_pressed_held_key_not_included() {
        let previous = vec![Key::Enter];
        let current  = vec![Key::Enter, Key::Up];
        let pressed = newly_pressed(&current, &previous);
        assert!(!pressed.contains(&Key::Enter));
        assert!(pressed.contains(&Key::Up));
    }

    // ── newly_released ────────────────────────────────────────────────────────

    #[test]
    fn test_newly_released_empty_current() {
        let previous = vec![Key::Z];
        let current  = vec![];
        let released = newly_released(&previous, &current);
        assert!(released.contains(&Key::Z));
    }

    #[test]
    fn test_newly_released_no_change() {
        let keys = vec![Key::Down];
        let released = newly_released(&keys, &keys);
        assert!(released.is_empty());
    }

    #[test]
    fn test_newly_released_detects_dropped_key() {
        let previous = vec![Key::Z, Key::Left];
        let current  = vec![Key::Z];
        let released = newly_released(&previous, &current);
        assert_eq!(released, vec![Key::Left]);
    }

    #[test]
    fn test_press_and_release_are_mutually_exclusive_for_same_key() {
        let previous = vec![Key::Z];
        let current  = vec![Key::X];
        let pressed  = newly_pressed(&current,  &previous);
        let released = newly_released(&previous, &current);
        assert!(pressed.contains(&Key::X));
        assert!(released.contains(&Key::Z));
        assert!(!pressed.contains(&Key::Z));
        assert!(!released.contains(&Key::X));
    }
}