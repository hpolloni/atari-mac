//! `atari-mac` library crate.
//!
//! All program logic lives here so that it is reachable from unit tests,
//! integration tests (`tests/`) and doctests. `src/main.rs` is a thin shell.

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
