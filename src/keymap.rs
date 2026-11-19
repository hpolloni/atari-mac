//! The host key map (REQ-009, REQ-010, REQ-017).
//!
//! Design: `docs/design/architecture.md`, "Key mapping". [`map_key`] is a pure function
//! from one host key to one [`Action`]. [`InputState::from_keys`] maps the set of host keys
//! that are down to the input of the machine.
//!
//! Letters, digits and punctuation type the same character as on a US host keyboard. For
//! digits and punctuation, the character decides the Atari SHIFT state: host Shift + `,`
//! types `<`, which is the unshifted Atari `<` key. For letters and for the other keys,
//! the Atari SHIFT state follows the host Shift keys.

use atari_core::JoystickState;
use atari_core::gtia::ConsoleKeys;
use atari_core::keyboard::{CONTROL, RETURN, SHIFT};
use minifb::Key;

/// Atari key codes (REQ-523) that the map uses and that `atari_core::keyboard` does not
/// export yet.
mod code {
    /// The HELP key.
    pub(super) const HELP: u8 = 0x11;
    /// The ESC key.
    pub(super) const ESC: u8 = 0x1C;
    /// The SPACE bar.
    pub(super) const SPACE: u8 = 0x21;
    /// The inverse video key (Atari logo key).
    pub(super) const INVERSE: u8 = 0x27;
    /// The TAB key.
    pub(super) const TAB: u8 = 0x2C;
    /// The DELETE/BACK SPACE key.
    pub(super) const BACK_SPACE: u8 = 0x34;
    /// The CAPS key.
    pub(super) const CAPS: u8 = 0x3C;
}

/// A console key of GTIA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleKey {
    /// START.
    Start,
    /// SELECT.
    Select,
    /// OPTION.
    Option,
}

/// A switch of joystick 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stick {
    /// Up.
    Up,
    /// Down.
    Down,
    /// Left.
    Left,
    /// Right.
    Right,
    /// The trigger (fire button).
    Fire,
}

/// What one host key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// An Atari key of the keyboard matrix: the 6-bit key code, with [`SHIFT`] set when
    /// the Atari SHIFT key must be down.
    Key(u8),
    /// The Atari SHIFT key.
    Shift,
    /// The Atari CONTROL key.
    Control,
    /// The Atari BREAK key.
    Break,
    /// A console key.
    Console(ConsoleKey),
    /// The RESET key (warm reset).
    Reset,
    /// A switch of joystick 1.
    Joystick(Stick),
    /// Pause or continue the emulation.
    Pause,
    /// Cold reset (power off and on).
    ColdReset,
    /// Quit the frontend.
    Quit,
}

/// The host modifier keys that change the meaning of another key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// A host Shift key is down.
    pub shift: bool,
    /// A host Command key is down.
    pub command: bool,
}

/// Adds [`SHIFT`] to `code` when `shift` is true.
const fn shifted(code: u8, shift: bool) -> u8 {
    if shift { code | SHIFT } else { code }
}

/// A key whose character decides the Atari SHIFT state: `plain` without host Shift,
/// `with_shift` with host Shift. `None` is a character with no Atari key legend.
const fn character(plain: Option<u8>, with_shift: Option<u8>, shift: bool) -> Option<Action> {
    let code = if shift { with_shift } else { plain };
    match code {
        Some(code) => Some(Action::Key(code)),
        None => None,
    }
}

/// Maps a letter key to its Atari key code (REQ-523).
const fn letter_code(key: Key) -> Option<u8> {
    Some(match key {
        Key::A => 0x3F,
        Key::B => 0x15,
        Key::C => 0x12,
        Key::D => 0x3A,
        Key::E => 0x2A,
        Key::F => 0x38,
        Key::G => 0x3D,
        Key::H => 0x39,
        Key::I => 0x0D,
        Key::J => 0x01,
        Key::K => 0x05,
        Key::L => 0x00,
        Key::M => 0x25,
        Key::N => 0x23,
        Key::O => 0x08,
        Key::P => 0x0A,
        Key::Q => 0x2F,
        Key::R => 0x28,
        Key::S => 0x3E,
        Key::T => 0x2D,
        Key::U => 0x0B,
        Key::V => 0x10,
        Key::W => 0x2E,
        Key::X => 0x16,
        Key::Y => 0x2B,
        Key::Z => 0x17,
        _ => return None,
    })
}

/// Maps a digit or punctuation key of a US keyboard to the Atari `KBCODE` values of its
/// two characters: (without host Shift, with host Shift). The values come from the key
/// legends of REQ-523 (the same values as `keyboard::ascii_to_kbcode`, REQ-524).
const fn character_codes(key: Key) -> Option<(Option<u8>, Option<u8>)> {
    Some(match key {
        Key::Key1 => (Some(0x1F), Some(0x5F)),       // 1 !
        Key::Key2 => (Some(0x1E), Some(0x75)),       // 2 @
        Key::Key3 => (Some(0x1A), Some(0x5A)),       // 3 #
        Key::Key4 => (Some(0x18), Some(0x58)),       // 4 $
        Key::Key5 => (Some(0x1D), Some(0x5D)),       // 5 %
        Key::Key6 => (Some(0x1B), Some(0x47)),       // 6 ^
        Key::Key7 => (Some(0x33), Some(0x5B)),       // 7 &
        Key::Key8 => (Some(0x35), Some(0x07)),       // 8 *
        Key::Key9 => (Some(0x30), Some(0x70)),       // 9 (
        Key::Key0 => (Some(0x32), Some(0x72)),       // 0 )
        Key::Minus => (Some(0x0E), Some(0x4E)),      // - _
        Key::Equal => (Some(0x0F), Some(0x06)),      // = +
        Key::LeftBracket => (Some(0x60), None),      // [ {
        Key::RightBracket => (Some(0x62), None),     // ] }
        Key::Backslash => (Some(0x46), Some(0x4F)),  // \ |
        Key::Semicolon => (Some(0x02), Some(0x42)),  // ; :
        Key::Apostrophe => (Some(0x73), Some(0x5E)), // ' "
        Key::Comma => (Some(0x20), Some(0x36)),      // , <
        Key::Period => (Some(0x22), Some(0x37)),     // . >
        Key::Slash => (Some(0x26), Some(0x66)),      // / ?
        Key::Backquote => (None, None),              // ` ~
        _ => return None,
    })
}

/// Maps a key that is not a character key and whose Atari SHIFT state follows the host
/// Shift keys to its Atari key code.
const fn special_code(key: Key) -> Option<u8> {
    Some(match key {
        Key::Space => code::SPACE,
        Key::Enter => RETURN,
        Key::Backspace => code::BACK_SPACE,
        Key::Tab => code::TAB,
        Key::Escape => code::ESC,
        Key::CapsLock => code::CAPS,
        Key::F1 => code::INVERSE,
        Key::F6 => code::HELP,
        _ => return None,
    })
}

/// Maps one host key to its action (the table in `docs/design/architecture.md`, "Key
/// mapping"). `mods` gives the host modifier keys that are down. Returns `None` for a host
/// key without an action.
#[must_use]
pub const fn map_key(key: Key, mods: Modifiers) -> Option<Action> {
    if let Some(code) = letter_code(key) {
        return Some(Action::Key(shifted(code, mods.shift)));
    }
    if let Some((plain, with_shift)) = character_codes(key) {
        return character(plain, with_shift, mods.shift);
    }
    if matches!(key, Key::Escape) && mods.command {
        return Some(Action::Quit);
    }
    if let Some(code) = special_code(key) {
        return Some(Action::Key(shifted(code, mods.shift)));
    }
    Some(match key {
        Key::LeftShift | Key::RightShift => Action::Shift,
        Key::LeftCtrl | Key::RightCtrl => Action::Control,
        Key::F2 => Action::Console(ConsoleKey::Start),
        Key::F3 => Action::Console(ConsoleKey::Select),
        Key::F4 => Action::Console(ConsoleKey::Option),
        Key::F5 => Action::Reset,
        Key::F7 => Action::Break,
        Key::F9 => Action::Pause,
        Key::F10 => Action::ColdReset,
        Key::Up => Action::Joystick(Stick::Up),
        Key::Down => Action::Joystick(Stick::Down),
        Key::Left => Action::Joystick(Stick::Left),
        Key::Right => Action::Joystick(Stick::Right),
        Key::LeftAlt => Action::Joystick(Stick::Fire),
        _ => return None,
    })
}

/// The input of the machine for one frame, made from the host keys that are down.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per host key or key group"
)]
pub struct InputState {
    /// The Atari key that is pressed, as a `KBCODE` value with the SHIFT and CONTROL bits.
    pub key: Option<u8>,
    /// All Atari keys (with the SHIFT and CONTROL bits) whose host keys are down.
    pub held: Vec<u8>,
    /// A host Shift key is down.
    pub shift: bool,
    /// A host Control key is down.
    pub control: bool,
    /// The BREAK key is down.
    pub break_key: bool,
    /// The console keys.
    pub console: ConsoleKeys,
    /// Joystick 1.
    pub joystick: JoystickState,
    /// The RESET key is down.
    pub reset: bool,
    /// The cold reset key is down.
    pub cold_reset: bool,
    /// The pause key is down.
    pub pause: bool,
    /// The quit combination is down.
    pub quit: bool,
}

impl InputState {
    /// Maps the host keys that are down. `previous` is the state of the last frame: when
    /// two Atari keys are down, a key that went down in this frame wins, so that fast
    /// typing with overlapping keys gives each key.
    #[must_use]
    pub fn from_keys(keys: &[Key], previous: &Self) -> Self {
        let mods = Modifiers {
            shift: keys
                .iter()
                .any(|k| matches!(k, Key::LeftShift | Key::RightShift)),
            command: keys
                .iter()
                .any(|k| matches!(k, Key::LeftSuper | Key::RightSuper)),
        };
        let mut state = Self {
            shift: mods.shift,
            ..Self::default()
        };
        for &key in keys {
            match map_key(key, mods) {
                Some(Action::Key(code)) => state.held.push(code),
                Some(Action::Control) => state.control = true,
                Some(Action::Break) => state.break_key = true,
                Some(Action::Console(ConsoleKey::Start)) => state.console.start = true,
                Some(Action::Console(ConsoleKey::Select)) => state.console.select = true,
                Some(Action::Console(ConsoleKey::Option)) => state.console.option = true,
                Some(Action::Reset) => state.reset = true,
                Some(Action::Joystick(stick)) => state.set_stick(stick),
                Some(Action::Pause) => state.pause = true,
                Some(Action::ColdReset) => state.cold_reset = true,
                Some(Action::Quit) => state.quit = true,
                Some(Action::Shift) | None => {}
            }
        }
        if state.control {
            for code in &mut state.held {
                *code |= CONTROL;
            }
        }
        state.key = state
            .held
            .iter()
            .rev()
            .copied()
            .find(|code| !previous.held.contains(code))
            .or_else(|| previous.key.filter(|code| state.held.contains(code)))
            .or_else(|| state.held.first().copied());
        state
    }

    /// Sets one switch of joystick 1.
    const fn set_stick(&mut self, stick: Stick) {
        match stick {
            Stick::Up => self.joystick.up = true,
            Stick::Down => self.joystick.down = true,
            Stick::Left => self.joystick.left = true,
            Stick::Right => self.joystick.right = true,
            Stick::Fire => self.joystick.fire = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, ConsoleKey, InputState, Key, Modifiers, Stick, map_key};
    use atari_core::JoystickState;
    use atari_core::keyboard::{CONTROL, RETURN, SHIFT};

    /// No host modifier.
    const NONE: Modifiers = Modifiers {
        shift: false,
        command: false,
    };
    /// Host Shift down.
    const SHIFTED: Modifiers = Modifiers {
        shift: true,
        command: false,
    };
    /// Host Command down.
    const COMMAND: Modifiers = Modifiers {
        shift: false,
        command: true,
    };

    /// The 54 keys of the 65XE keyboard (REQ-523, own copy of the table).
    ///
    /// When `atari_core::keyboard::KEYS` exists (WP-3), the test uses it instead.
    const ATARI_KEYS: [(u8, &str); 54] = [
        (0x00, "L"),
        (0x01, "J"),
        (0x02, ";"),
        (0x05, "K"),
        (0x06, "+"),
        (0x07, "*"),
        (0x08, "O"),
        (0x0A, "P"),
        (0x0B, "U"),
        (0x0C, "RETURN"),
        (0x0D, "I"),
        (0x0E, "-"),
        (0x0F, "="),
        (0x10, "V"),
        (0x11, "HELP"),
        (0x12, "C"),
        (0x15, "B"),
        (0x16, "X"),
        (0x17, "Z"),
        (0x18, "4"),
        (0x1A, "3"),
        (0x1B, "6"),
        (0x1C, "ESC"),
        (0x1D, "5"),
        (0x1E, "2"),
        (0x1F, "1"),
        (0x20, ","),
        (0x21, "SPACE"),
        (0x22, "."),
        (0x23, "N"),
        (0x25, "M"),
        (0x26, "/"),
        (0x27, "INVERSE"),
        (0x28, "R"),
        (0x2A, "E"),
        (0x2B, "Y"),
        (0x2C, "TAB"),
        (0x2D, "T"),
        (0x2E, "W"),
        (0x2F, "Q"),
        (0x30, "9"),
        (0x32, "0"),
        (0x33, "7"),
        (0x34, "BACK SPACE"),
        (0x35, "8"),
        (0x36, "<"),
        (0x37, ">"),
        (0x38, "F"),
        (0x39, "H"),
        (0x3A, "D"),
        (0x3C, "CAPS"),
        (0x3D, "G"),
        (0x3E, "S"),
        (0x3F, "A"),
    ];

    /// All host keys that the map can see.
    const HOST_KEYS: [Key; 106] = [
        Key::Key0,
        Key::Key1,
        Key::Key2,
        Key::Key3,
        Key::Key4,
        Key::Key5,
        Key::Key6,
        Key::Key7,
        Key::Key8,
        Key::Key9,
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
        Key::F13,
        Key::F14,
        Key::F15,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Up,
        Key::Apostrophe,
        Key::Backquote,
        Key::Backslash,
        Key::Comma,
        Key::Equal,
        Key::LeftBracket,
        Key::Minus,
        Key::Period,
        Key::RightBracket,
        Key::Semicolon,
        Key::Slash,
        Key::Backspace,
        Key::Delete,
        Key::End,
        Key::Enter,
        Key::Escape,
        Key::Home,
        Key::Insert,
        Key::Menu,
        Key::PageDown,
        Key::PageUp,
        Key::Pause,
        Key::Space,
        Key::Tab,
        Key::NumLock,
        Key::CapsLock,
        Key::ScrollLock,
        Key::LeftShift,
        Key::RightShift,
        Key::LeftCtrl,
        Key::RightCtrl,
        Key::NumPad0,
        Key::NumPad1,
        Key::NumPad2,
        Key::NumPad3,
        Key::NumPad4,
        Key::NumPad5,
        Key::NumPad6,
        Key::NumPad7,
        Key::NumPad8,
        Key::NumPad9,
        Key::NumPadDot,
        Key::NumPadSlash,
        Key::NumPadAsterisk,
        Key::NumPadMinus,
        Key::NumPadPlus,
        Key::NumPadEnter,
        Key::LeftAlt,
        Key::RightAlt,
        Key::LeftSuper,
        Key::RightSuper,
    ];

    /// Table row "Letters, digits, punctuation": the host key, the host modifiers, the
    /// character of a US host keyboard and the Atari `KBCODE` of the same character
    /// (REQ-524, with SHIFT where the Atari key needs it).
    const CHARACTERS: [(Key, Modifiers, char, u8); 66] = [
        (Key::A, NONE, 'a', 0x3F),
        (Key::B, NONE, 'b', 0x15),
        (Key::C, NONE, 'c', 0x12),
        (Key::D, NONE, 'd', 0x3A),
        (Key::E, NONE, 'e', 0x2A),
        (Key::F, NONE, 'f', 0x38),
        (Key::G, NONE, 'g', 0x3D),
        (Key::H, NONE, 'h', 0x39),
        (Key::I, NONE, 'i', 0x0D),
        (Key::J, NONE, 'j', 0x01),
        (Key::K, NONE, 'k', 0x05),
        (Key::L, NONE, 'l', 0x00),
        (Key::M, NONE, 'm', 0x25),
        (Key::N, NONE, 'n', 0x23),
        (Key::O, NONE, 'o', 0x08),
        (Key::P, NONE, 'p', 0x0A),
        (Key::Q, NONE, 'q', 0x2F),
        (Key::R, NONE, 'r', 0x28),
        (Key::S, NONE, 's', 0x3E),
        (Key::T, NONE, 't', 0x2D),
        (Key::U, NONE, 'u', 0x0B),
        (Key::V, NONE, 'v', 0x10),
        (Key::W, NONE, 'w', 0x2E),
        (Key::X, NONE, 'x', 0x16),
        (Key::Y, NONE, 'y', 0x2B),
        (Key::Z, NONE, 'z', 0x17),
        (Key::Key1, NONE, '1', 0x1F),
        (Key::Key2, NONE, '2', 0x1E),
        (Key::Key3, NONE, '3', 0x1A),
        (Key::Key4, NONE, '4', 0x18),
        (Key::Key5, NONE, '5', 0x1D),
        (Key::Key6, NONE, '6', 0x1B),
        (Key::Key7, NONE, '7', 0x33),
        (Key::Key8, NONE, '8', 0x35),
        (Key::Key9, NONE, '9', 0x30),
        (Key::Key0, NONE, '0', 0x32),
        (Key::Key1, SHIFTED, '!', 0x5F),
        (Key::Key2, SHIFTED, '@', 0x75),
        (Key::Key3, SHIFTED, '#', 0x5A),
        (Key::Key4, SHIFTED, '$', 0x58),
        (Key::Key5, SHIFTED, '%', 0x5D),
        (Key::Key6, SHIFTED, '^', 0x47),
        (Key::Key7, SHIFTED, '&', 0x5B),
        (Key::Key8, SHIFTED, '*', 0x07),
        (Key::Key9, SHIFTED, '(', 0x70),
        (Key::Key0, SHIFTED, ')', 0x72),
        (Key::Minus, NONE, '-', 0x0E),
        (Key::Minus, SHIFTED, '_', 0x4E),
        (Key::Equal, NONE, '=', 0x0F),
        (Key::Equal, SHIFTED, '+', 0x06),
        (Key::LeftBracket, NONE, '[', 0x60),
        (Key::RightBracket, NONE, ']', 0x62),
        (Key::Backslash, NONE, '\\', 0x46),
        (Key::Backslash, SHIFTED, '|', 0x4F),
        (Key::Semicolon, NONE, ';', 0x02),
        (Key::Semicolon, SHIFTED, ':', 0x42),
        (Key::Apostrophe, NONE, '\'', 0x73),
        (Key::Apostrophe, SHIFTED, '"', 0x5E),
        (Key::Comma, NONE, ',', 0x20),
        (Key::Comma, SHIFTED, '<', 0x36),
        (Key::Period, NONE, '.', 0x22),
        (Key::Period, SHIFTED, '>', 0x37),
        (Key::Slash, NONE, '/', 0x26),
        (Key::Slash, SHIFTED, '?', 0x66),
        (Key::Space, NONE, ' ', 0x21),
        (Key::A, SHIFTED, 'A', 0x7F),
    ];

    /// Each row of the key table in `docs/design/architecture.md` gives its Atari key.
    ///
    /// Verifies REQ-009
    #[test]
    fn every_table_entry_maps() {
        for (key, mods, ch, kbcode) in CHARACTERS {
            assert_eq!(
                map_key(key, mods),
                Some(Action::Key(kbcode)),
                "{key:?} (shift {}) types {ch:?}",
                mods.shift
            );
        }
        for (key, mods) in [
            (Key::LeftBracket, SHIFTED),
            (Key::RightBracket, SHIFTED),
            (Key::Backquote, NONE),
            (Key::Backquote, SHIFTED),
        ] {
            assert_eq!(map_key(key, mods), None, "{key:?} has no Atari legend");
        }

        let rows: [(Key, Action); 23] = [
            (Key::Enter, Action::Key(RETURN)),
            (Key::Backspace, Action::Key(0x34)),
            (Key::Tab, Action::Key(0x2C)),
            (Key::Escape, Action::Key(0x1C)),
            (Key::LeftShift, Action::Shift),
            (Key::RightShift, Action::Shift),
            (Key::LeftCtrl, Action::Control),
            (Key::RightCtrl, Action::Control),
            (Key::CapsLock, Action::Key(0x3C)),
            (Key::F1, Action::Key(0x27)),
            (Key::F2, Action::Console(ConsoleKey::Start)),
            (Key::F3, Action::Console(ConsoleKey::Select)),
            (Key::F4, Action::Console(ConsoleKey::Option)),
            (Key::F5, Action::Reset),
            (Key::F6, Action::Key(0x11)),
            (Key::F7, Action::Break),
            (Key::F9, Action::Pause),
            (Key::F10, Action::ColdReset),
            (Key::Up, Action::Joystick(Stick::Up)),
            (Key::Down, Action::Joystick(Stick::Down)),
            (Key::Left, Action::Joystick(Stick::Left)),
            (Key::Right, Action::Joystick(Stick::Right)),
            (Key::LeftAlt, Action::Joystick(Stick::Fire)),
        ];
        for (key, action) in rows {
            assert_eq!(map_key(key, NONE), Some(action), "{key:?}");
        }
        assert_eq!(
            map_key(Key::Escape, COMMAND),
            Some(Action::Quit),
            "Command + Escape"
        );
        // The non-character keys keep the host Shift state (for example Shift + DELETE).
        assert_eq!(
            map_key(Key::Backspace, SHIFTED),
            Some(Action::Key(0x34 | SHIFT)),
            "Shift + Backspace"
        );
    }

    /// Each key of the 65XE keyboard, and SHIFT, CONTROL, BREAK, START, SELECT, OPTION and
    /// RESET, has at least one host key.
    ///
    /// Verifies REQ-009
    #[test]
    fn every_atari_key_has_a_host_key() {
        let mut actions = Vec::new();
        for key in HOST_KEYS {
            for mods in [NONE, SHIFTED] {
                actions.extend(map_key(key, mods));
            }
        }
        for (code, name) in ATARI_KEYS {
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a, Action::Key(c) if c & 0x3F == code)),
                "no host key for the Atari key {name} (${code:02X})"
            );
        }
        for action in [
            Action::Shift,
            Action::Control,
            Action::Break,
            Action::Console(ConsoleKey::Start),
            Action::Console(ConsoleKey::Select),
            Action::Console(ConsoleKey::Option),
            Action::Reset,
        ] {
            assert!(actions.contains(&action), "no host key for {action:?}");
        }
        for action in &actions {
            if let Action::Key(code) = action {
                assert!(
                    ATARI_KEYS.iter().any(|&(c, _)| c == code & 0x3F),
                    "${code:02X} is not a 65XE key"
                );
                assert_eq!(code & CONTROL, 0, "map_key never sets CONTROL");
            }
        }
    }

    /// The arrow keys give the joystick 1 directions and Left Alt the trigger.
    ///
    /// Verifies REQ-010
    #[test]
    fn joystick_keys() {
        for (key, stick) in [
            (Key::Up, Stick::Up),
            (Key::Down, Stick::Down),
            (Key::Left, Stick::Left),
            (Key::Right, Stick::Right),
            (Key::LeftAlt, Stick::Fire),
        ] {
            assert_eq!(map_key(key, NONE), Some(Action::Joystick(stick)), "{key:?}");
        }
        let state = InputState::from_keys(&[Key::Up, Key::Left], &InputState::default());
        assert_eq!(
            state.joystick,
            JoystickState {
                up: true,
                left: true,
                ..JoystickState::default()
            },
            "Up and Left held"
        );
        let state = InputState::from_keys(
            &[Key::Down, Key::Right, Key::LeftAlt],
            &InputState::default(),
        );
        assert_eq!(
            state.joystick,
            JoystickState {
                down: true,
                right: true,
                fire: true,
                ..JoystickState::default()
            },
            "Down, Right and Left Alt held"
        );
        assert_eq!(state.key, None, "no Atari key");
    }

    /// F9 gives Pause, F10 Cold reset and Command + Escape Quit.
    ///
    /// Verifies REQ-017
    #[test]
    fn control_keys() {
        assert_eq!(map_key(Key::F9, NONE), Some(Action::Pause), "F9");
        assert_eq!(map_key(Key::F10, NONE), Some(Action::ColdReset), "F10");
        assert_eq!(
            map_key(Key::Escape, COMMAND),
            Some(Action::Quit),
            "Command + Escape"
        );
        assert_eq!(
            map_key(Key::Escape, NONE),
            Some(Action::Key(0x1C)),
            "Escape alone is ESC"
        );
        let none = InputState::default();
        for (keys, expected) in [
            (
                vec![Key::F9],
                InputState {
                    pause: true,
                    ..InputState::default()
                },
            ),
            (
                vec![Key::F10],
                InputState {
                    cold_reset: true,
                    ..InputState::default()
                },
            ),
            (
                vec![Key::RightSuper, Key::Escape],
                InputState {
                    quit: true,
                    ..InputState::default()
                },
            ),
            (
                vec![Key::F5, Key::F7],
                InputState {
                    reset: true,
                    break_key: true,
                    ..InputState::default()
                },
            ),
        ] {
            assert_eq!(InputState::from_keys(&keys, &none), expected, "{keys:?}");
        }
    }

    #[test]
    fn modifiers_reach_the_key_code() {
        let none = InputState::default();
        let state = InputState::from_keys(&[Key::LeftCtrl, Key::Comma], &none);
        assert_eq!(state.key, Some(0x20 | CONTROL), "Control + ,");
        assert!(state.control, "control");
        let state = InputState::from_keys(&[Key::RightShift, Key::A], &none);
        assert_eq!(state.key, Some(0x3F | SHIFT), "Shift + A");
        assert!(state.shift, "shift");
        let state = InputState::from_keys(&[Key::LeftShift, Key::Comma], &none);
        assert_eq!(state.key, Some(0x36), "Shift + , types the unshifted < key");
        let state = InputState::from_keys(&[Key::F2, Key::F3, Key::F4], &none);
        assert!(
            state.console.start && state.console.select && state.console.option,
            "console keys"
        );
    }

    #[test]
    fn newest_key_wins() {
        let a = InputState::from_keys(&[Key::A], &InputState::default());
        assert_eq!(a.key, Some(0x3F), "A");
        let ab = InputState::from_keys(&[Key::A, Key::B], &a);
        assert_eq!(ab.key, Some(0x15), "B went down after A");
        let ab2 = InputState::from_keys(&[Key::A, Key::B], &ab);
        assert_eq!(ab2.key, Some(0x15), "B stays while both are held");
        let a2 = InputState::from_keys(&[Key::A], &ab2);
        assert_eq!(a2.key, Some(0x3F), "A again after B goes up");
        let both = InputState::from_keys(&[Key::A, Key::B], &InputState::default());
        assert_eq!(both.key, Some(0x15), "two new keys: the last one");
        let held = InputState {
            held: vec![0x3F, 0x15],
            ..InputState::default()
        };
        let first = InputState::from_keys(&[Key::A, Key::B], &held);
        assert_eq!(
            first.key,
            Some(0x3F),
            "no new key, no previous key: the first"
        );
        let none = InputState::from_keys(&[], &a);
        assert_eq!(none.key, None, "all keys up");
    }
}
