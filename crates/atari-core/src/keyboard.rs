//! Atari keyboard codes, as read from POKEY `KBCODE`.
//!
//! Design: `docs/design/pokey.md`, "Module keyboard". Requirements: REQ-523, REQ-524.

/// The `KBCODE` bit for the SHIFT key.
pub const SHIFT: u8 = 0x40;

/// The `KBCODE` bit for the CONTROL key.
pub const CONTROL: u8 = 0x80;

/// The `KBCODE` of the RETURN key.
pub const RETURN: u8 = 0x0C;

/// Converts one ASCII character to a `KBCODE` value (REQ-524). `'\n'` is RETURN. Returns
/// `None` for a character that has no key legend.
#[must_use]
pub const fn ascii_to_kbcode(c: char) -> Option<u8> {
    // WP-0 STUB: WP-3 implements the full table.
    match c {
        '\n' => Some(RETURN),
        _ => None,
    }
}
