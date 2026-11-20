//! Screen text from the OS screen memory (REQ-024).
//!
//! The functions are in the core module `atari_core::screen`, so that the frontend can use
//! them too (REQ-080). This module keeps the harness API.

pub use atari_core::screen::{COLUMNS, ROWS, SAVMSC, has_row, internal_to_ascii, screen_text};
