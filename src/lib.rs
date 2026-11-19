//! `atari-mac` frontend library crate.
//!
//! WP-4 (SLICE-01) adds the window, the key map and the headless mode here.

pub use atari_core;

/// Returns the greeting shown by the binary.
///
/// # Examples
///
/// ```
/// assert_eq!(atari_mac::greeting(), "Hello, world!");
/// ```
#[must_use]
pub const fn greeting() -> &'static str {
    "Hello, world!"
}

#[cfg(test)]
mod tests {
    use super::greeting;

    #[test]
    fn greeting_is_not_empty() {
        assert!(!greeting().is_empty(), "greeting must not be empty");
    }
}
