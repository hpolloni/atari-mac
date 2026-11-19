//! Atari keyboard codes, as read from POKEY `KBCODE`.
//!
//! The codes are the scan codes of HRM 5.8 Keyboard scan, Table 11 and Table 12. The 65XE
//! keyboard is taken to be the 130XE keyboard of Table 12 (OI-030).
//!
//! Design: `docs/design/pokey.md`, "Module keyboard". Requirements: REQ-523, REQ-524.

/// The `KBCODE` bit for the SHIFT key.
pub const SHIFT: u8 = 0x40;

/// The `KBCODE` bit for the CONTROL key.
pub const CONTROL: u8 = 0x80;

/// The L key.
pub const L: u8 = 0x00;
/// The J key.
pub const J: u8 = 0x01;
/// The `;` key (Shift: `:`).
pub const SEMICOLON: u8 = 0x02;
/// The K key.
pub const K: u8 = 0x05;
/// The `+` key (Shift: `\`).
pub const PLUS: u8 = 0x06;
/// The `*` key (Shift: `^`).
pub const ASTERISK: u8 = 0x07;
/// The O key.
pub const O: u8 = 0x08;
/// The P key.
pub const P: u8 = 0x0A;
/// The U key.
pub const U: u8 = 0x0B;
/// The RETURN key.
pub const RETURN: u8 = 0x0C;
/// The I key.
pub const I: u8 = 0x0D;
/// The `-` key (Shift: `_`).
pub const MINUS: u8 = 0x0E;
/// The `=` key (Shift: `|`).
pub const EQUALS: u8 = 0x0F;
/// The V key.
pub const V: u8 = 0x10;
/// The HELP key.
pub const HELP: u8 = 0x11;
/// The C key.
pub const C: u8 = 0x12;
/// The B key.
pub const B: u8 = 0x15;
/// The X key.
pub const X: u8 = 0x16;
/// The Z key.
pub const Z: u8 = 0x17;
/// The `4` key (Shift: `$`).
pub const DIGIT_4: u8 = 0x18;
/// The `3` key (Shift: `#`).
pub const DIGIT_3: u8 = 0x1A;
/// The `6` key (Shift: `&`).
pub const DIGIT_6: u8 = 0x1B;
/// The ESC key.
pub const ESC: u8 = 0x1C;
/// The `5` key (Shift: `%`).
pub const DIGIT_5: u8 = 0x1D;
/// The `2` key (Shift: `"`).
pub const DIGIT_2: u8 = 0x1E;
/// The `1` key (Shift: `!`).
pub const DIGIT_1: u8 = 0x1F;
/// The `,` key (Shift: `[`).
pub const COMMA: u8 = 0x20;
/// The SPACE bar.
pub const SPACE: u8 = 0x21;
/// The `.` key (Shift: `]`).
pub const PERIOD: u8 = 0x22;
/// The N key.
pub const N: u8 = 0x23;
/// The M key.
pub const M: u8 = 0x25;
/// The `/` key (Shift: `?`).
pub const SLASH: u8 = 0x26;
/// The inverse video key.
pub const INVERSE: u8 = 0x27;
/// The R key.
pub const R: u8 = 0x28;
/// The E key.
pub const E: u8 = 0x2A;
/// The Y key.
pub const Y: u8 = 0x2B;
/// The TAB key.
pub const TAB: u8 = 0x2C;
/// The T key.
pub const T: u8 = 0x2D;
/// The W key.
pub const W: u8 = 0x2E;
/// The Q key.
pub const Q: u8 = 0x2F;
/// The `9` key (Shift: `(`).
pub const DIGIT_9: u8 = 0x30;
/// The `0` key (Shift: `)`).
pub const DIGIT_0: u8 = 0x32;
/// The `7` key (Shift: `'`).
pub const DIGIT_7: u8 = 0x33;
/// The BACK SPACE key.
pub const BACK_SPACE: u8 = 0x34;
/// The `8` key (Shift: `@`).
pub const DIGIT_8: u8 = 0x35;
/// The `<` key (Shift: CLEAR).
pub const LESS: u8 = 0x36;
/// The `>` key (Shift: INSERT).
pub const GREATER: u8 = 0x37;
/// The F key.
pub const F: u8 = 0x38;
/// The H key.
pub const H: u8 = 0x39;
/// The D key.
pub const D: u8 = 0x3A;
/// The CAPS key.
pub const CAPS: u8 = 0x3C;
/// The G key.
pub const G: u8 = 0x3D;
/// The S key.
pub const S: u8 = 0x3E;
/// The A key.
pub const A: u8 = 0x3F;

/// All 54 keys of the 65XE keyboard: (code, name), in code order (REQ-523).
///
/// START, SELECT, OPTION, RESET, SHIFT, CONTROL and BREAK have no key code. The codes
/// `$03`, `$04`, `$13` and `$14` are the 1200XL function keys, and `$09`, `$19`, `$24`,
/// `$29`, `$31` and `$3B` are not in the keyboard matrix.
pub const KEYS: [(u8, &str); 54] = [
    (L, "L"),
    (J, "J"),
    (SEMICOLON, ";"),
    (K, "K"),
    (PLUS, "+"),
    (ASTERISK, "*"),
    (O, "O"),
    (P, "P"),
    (U, "U"),
    (RETURN, "RETURN"),
    (I, "I"),
    (MINUS, "-"),
    (EQUALS, "="),
    (V, "V"),
    (HELP, "HELP"),
    (C, "C"),
    (B, "B"),
    (X, "X"),
    (Z, "Z"),
    (DIGIT_4, "4"),
    (DIGIT_3, "3"),
    (DIGIT_6, "6"),
    (ESC, "ESC"),
    (DIGIT_5, "5"),
    (DIGIT_2, "2"),
    (DIGIT_1, "1"),
    (COMMA, ","),
    (SPACE, "SPACE"),
    (PERIOD, "."),
    (N, "N"),
    (M, "M"),
    (SLASH, "/"),
    (INVERSE, "INVERSE"),
    (R, "R"),
    (E, "E"),
    (Y, "Y"),
    (TAB, "TAB"),
    (T, "T"),
    (W, "W"),
    (Q, "Q"),
    (DIGIT_9, "9"),
    (DIGIT_0, "0"),
    (DIGIT_7, "7"),
    (BACK_SPACE, "BACK SPACE"),
    (DIGIT_8, "8"),
    (LESS, "<"),
    (GREATER, ">"),
    (F, "F"),
    (H, "H"),
    (D, "D"),
    (CAPS, "CAPS"),
    (G, "G"),
    (S, "S"),
    (A, "A"),
];

/// Converts one ASCII character to a `KBCODE` value (REQ-524).
///
/// The result is the code of the key with the character as its unshifted legend, or the
/// code + [`SHIFT`] for a shifted legend (HRM Table 12). Upper and lower case letters both
/// give the unshifted letter key. `'\n'` is RETURN, `'\t'` is TAB, `'\x1B'` is ESC and
/// `'\x08'` is BACK SPACE. Returns `None` for a character that has no key legend. The
/// result never has the [`CONTROL`] bit.
#[must_use]
pub const fn ascii_to_kbcode(c: char) -> Option<u8> {
    let code = match c.to_ascii_uppercase() {
        'A' => A,
        'B' => B,
        'C' => C,
        'D' => D,
        'E' => E,
        'F' => F,
        'G' => G,
        'H' => H,
        'I' => I,
        'J' => J,
        'K' => K,
        'L' => L,
        'M' => M,
        'N' => N,
        'O' => O,
        'P' => P,
        'Q' => Q,
        'R' => R,
        'S' => S,
        'T' => T,
        'U' => U,
        'V' => V,
        'W' => W,
        'X' => X,
        'Y' => Y,
        'Z' => Z,
        '0' => DIGIT_0,
        '1' => DIGIT_1,
        '2' => DIGIT_2,
        '3' => DIGIT_3,
        '4' => DIGIT_4,
        '5' => DIGIT_5,
        '6' => DIGIT_6,
        '7' => DIGIT_7,
        '8' => DIGIT_8,
        '9' => DIGIT_9,
        ' ' => SPACE,
        '\n' => RETURN,
        '\t' => TAB,
        '\x1B' => ESC,
        '\x08' => BACK_SPACE,
        ';' => SEMICOLON,
        '+' => PLUS,
        '*' => ASTERISK,
        '-' => MINUS,
        '=' => EQUALS,
        ',' => COMMA,
        '.' => PERIOD,
        '/' => SLASH,
        '<' => LESS,
        '>' => GREATER,
        ':' => SEMICOLON | SHIFT,
        '\\' => PLUS | SHIFT,
        '^' => ASTERISK | SHIFT,
        '_' => MINUS | SHIFT,
        '|' => EQUALS | SHIFT,
        '[' => COMMA | SHIFT,
        ']' => PERIOD | SHIFT,
        '?' => SLASH | SHIFT,
        '!' => DIGIT_1 | SHIFT,
        '"' => DIGIT_2 | SHIFT,
        '#' => DIGIT_3 | SHIFT,
        '$' => DIGIT_4 | SHIFT,
        '%' => DIGIT_5 | SHIFT,
        '&' => DIGIT_6 | SHIFT,
        '\'' => DIGIT_7 | SHIFT,
        '@' => DIGIT_8 | SHIFT,
        '(' => DIGIT_9 | SHIFT,
        ')' => DIGIT_0 | SHIFT,
        _ => return None,
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::{
        A, ASTERISK, B, BACK_SPACE, C, CAPS, COMMA, CONTROL, D, DIGIT_0, DIGIT_1, DIGIT_2, DIGIT_3,
        DIGIT_4, DIGIT_5, DIGIT_6, DIGIT_7, DIGIT_8, DIGIT_9, E, EQUALS, ESC, F, G, GREATER, H,
        HELP, I, INVERSE, J, K, KEYS, L, LESS, M, MINUS, N, O, P, PERIOD, PLUS, Q, R, RETURN, S,
        SEMICOLON, SHIFT, SLASH, SPACE, T, TAB, U, V, W, X, Y, Z, ascii_to_kbcode,
    };

    /// The key table of REQ-523, independent of the module constants: (code, name).
    const TABLE: [(u8, &str); 54] = [
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

    /// Each named constant with the name of its key in [`TABLE`].
    const CONSTANTS: [(u8, &str); 54] = [
        (L, "L"),
        (J, "J"),
        (SEMICOLON, ";"),
        (K, "K"),
        (PLUS, "+"),
        (ASTERISK, "*"),
        (O, "O"),
        (P, "P"),
        (U, "U"),
        (RETURN, "RETURN"),
        (I, "I"),
        (MINUS, "-"),
        (EQUALS, "="),
        (V, "V"),
        (HELP, "HELP"),
        (C, "C"),
        (B, "B"),
        (X, "X"),
        (Z, "Z"),
        (DIGIT_4, "4"),
        (DIGIT_3, "3"),
        (DIGIT_6, "6"),
        (ESC, "ESC"),
        (DIGIT_5, "5"),
        (DIGIT_2, "2"),
        (DIGIT_1, "1"),
        (COMMA, ","),
        (SPACE, "SPACE"),
        (PERIOD, "."),
        (N, "N"),
        (M, "M"),
        (SLASH, "/"),
        (INVERSE, "INVERSE"),
        (R, "R"),
        (E, "E"),
        (Y, "Y"),
        (TAB, "TAB"),
        (T, "T"),
        (W, "W"),
        (Q, "Q"),
        (DIGIT_9, "9"),
        (DIGIT_0, "0"),
        (DIGIT_7, "7"),
        (BACK_SPACE, "BACK SPACE"),
        (DIGIT_8, "8"),
        (LESS, "<"),
        (GREATER, ">"),
        (F, "F"),
        (H, "H"),
        (D, "D"),
        (CAPS, "CAPS"),
        (G, "G"),
        (S, "S"),
        (A, "A"),
    ];

    /// The code of the key `name` in [`TABLE`].
    fn table_code(name: &str) -> u8 {
        TABLE
            .iter()
            .find(|(_, n)| *n == name)
            .map_or_else(|| panic!("no key {name} in the table"), |(code, _)| *code)
    }

    /// Asserts that each `(character, kbcode)` pair converts as given.
    fn assert_converts(pairs: &[(char, u8)]) {
        for &(c, kbcode) in pairs {
            assert_eq!(ascii_to_kbcode(c), Some(kbcode), "character {c:?}");
        }
    }

    /// Verifies REQ-523: `KEYS` has the 54 keys of the table, and each named constant has
    /// the code of its key.
    #[test]
    fn key_constants_match_table() {
        assert_eq!(KEYS.len(), 54, "number of keys");
        let mut keys = KEYS.to_vec();
        let mut table = TABLE.to_vec();
        keys.sort_unstable();
        table.sort_unstable();
        assert_eq!(keys, table, "KEYS against the table of REQ-523");
        for (code, name) in CONSTANTS {
            assert_eq!(code, table_code(name), "constant for key {name}");
        }
        let mut codes: Vec<u8> = KEYS.iter().map(|(code, _)| *code).collect();
        codes.dedup();
        assert_eq!(codes.len(), 54, "the codes in KEYS are unique");
    }

    /// Verifies REQ-523: no entry has a code without a 65XE key, and the keys without a code
    /// have no entry.
    #[test]
    fn no_key_codes_unused() {
        for unused in [0x03, 0x04, 0x09, 0x13, 0x14, 0x19, 0x24, 0x29, 0x31, 0x3B] {
            assert!(
                KEYS.iter().all(|(code, _)| *code != unused),
                "code {unused:#04X} has an entry"
            );
        }
        for name in [
            "START", "SELECT", "OPTION", "RESET", "SHIFT", "CONTROL", "BREAK",
        ] {
            assert!(
                KEYS.iter().all(|(_, key)| !key.eq_ignore_ascii_case(name)),
                "{name} has an entry"
            );
        }
        assert!(
            KEYS.iter().all(|(code, _)| *code < 0x40),
            "all codes are 6-bit codes"
        );
    }

    /// Verifies REQ-524: upper and lower case letters give the letter key.
    #[test]
    fn ascii_letters() {
        for (upper, lower) in ('A'..='Z').zip('a'..='z') {
            let code = table_code(upper.encode_utf8(&mut [0; 4]));
            assert_eq!(ascii_to_kbcode(upper), Some(code), "letter {upper}");
            assert_eq!(ascii_to_kbcode(lower), Some(code), "letter {lower}");
        }
        assert_converts(&[('A', 0x3F), ('a', 0x3F), ('Z', 0x17), ('z', 0x17)]);
    }

    /// Verifies REQ-524: digits, space and newline.
    #[test]
    fn ascii_digits_and_space() {
        for digit in '0'..='9' {
            let code = table_code(digit.encode_utf8(&mut [0; 4]));
            assert_eq!(ascii_to_kbcode(digit), Some(code), "digit {digit}");
        }
        assert_converts(&[('1', 0x1F), ('0', 0x32), (' ', 0x21), ('\n', 0x0C)]);
    }

    /// Verifies REQ-524: the shifted legends of the digit keys.
    #[test]
    fn ascii_shifted_symbols() {
        assert_converts(&[
            ('!', 0x5F),
            ('"', 0x5E),
            ('#', 0x5A),
            ('$', 0x58),
            ('%', 0x5D),
            ('&', 0x5B),
            ('\'', 0x73),
            ('@', 0x75),
            ('(', 0x70),
            (')', 0x72),
        ]);
    }

    /// Verifies REQ-524: the legends of the symbol keys, unshifted and shifted.
    #[test]
    fn ascii_unshifted_symbols() {
        assert_converts(&[
            ('<', 0x36),
            ('>', 0x37),
            ('-', 0x0E),
            ('_', 0x4E),
            ('=', 0x0F),
            ('|', 0x4F),
            (';', 0x02),
            (':', 0x42),
            ('+', 0x06),
            ('\\', 0x46),
            ('*', 0x07),
            ('^', 0x47),
        ]);
        assert_converts(&[
            (',', 0x20),
            ('[', 0x60),
            ('.', 0x22),
            (']', 0x62),
            ('/', 0x26),
            ('?', 0x66),
        ]);
    }

    /// Verifies REQ-524: tab, escape and backspace.
    #[test]
    fn ascii_control_characters() {
        assert_converts(&[('\t', 0x2C), ('\x1B', 0x1C), ('\x08', 0x34)]);
    }

    /// Verifies REQ-524: characters without a key legend give no value.
    #[test]
    fn ascii_without_key_is_none() {
        let keyed = ['\t', '\n', '\x1B', '\x08'];
        let none = ['{', '}', '~', '`', '\x7F', '\u{80}', '\u{E9}', '\u{20AC}']
            .into_iter()
            .chain(('\x00'..='\x1F').filter(|c| !keyed.contains(c)));
        for c in none {
            assert_eq!(ascii_to_kbcode(c), None, "character {c:?}");
        }
    }

    /// Verifies REQ-524: no value has the CONTROL bit, and the SHIFT bit is the only bit
    /// above the key code.
    #[test]
    fn ascii_never_sets_control() {
        for c in (0_u8..=127).map(char::from) {
            if let Some(kbcode) = ascii_to_kbcode(c) {
                assert_eq!(kbcode & CONTROL, 0, "character {c:?}");
                assert!(
                    TABLE.iter().any(|(code, _)| *code == kbcode & !SHIFT),
                    "character {c:?} gives a code without a key"
                );
            }
        }
    }
}
