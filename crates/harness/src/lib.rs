//! Test harness for the Atari 65XE emulator.
//!
//! Design: `docs/design/test-harness.md`. Requirements: REQ-020 to REQ-027. The module
//! [`spec`] holds the specification gates (`docs/design/spec-gates.md`, REQ-043 to REQ-055).
//!
//! The library holds the helpers. The test suites are in `tests/`. Each test names the
//! requirements that it verifies in its documentation comment (REQ-027).

pub mod data;
pub mod image;
pub mod input;
pub mod screen;
pub mod single_step;
pub mod spec;
pub mod system;

use core::error::Error;
use core::fmt;
use std::path::PathBuf;

/// An error of a harness helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessError {
    /// A test data file does not exist (REQ-021).
    Missing {
        /// The path of the file.
        path: PathBuf,
        /// The command that downloads the file.
        fetch_command: String,
    },
    /// A file exists but cannot be read or parsed.
    Read {
        /// The path of the file.
        path: PathBuf,
        /// The cause.
        message: String,
    },
    /// A file cannot be written.
    Write {
        /// The path of the file.
        path: PathBuf,
        /// The cause.
        message: String,
    },
    /// A frame differs from its reference PNG file (REQ-026).
    GoldenMismatch {
        /// The name of the reference file, without `.png`.
        name: String,
        /// The number of differing pixels.
        pixels: usize,
        /// The actual frame that the comparison wrote.
        actual: PathBuf,
        /// The difference image that the comparison wrote.
        diff: PathBuf,
    },
    /// The machine rejected its configuration.
    Config(atari_core::ConfigError),
    /// A character has no key on the Atari keyboard (REQ-025).
    NoKey(char),
}

impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing {
                path,
                fetch_command,
            } => write!(
                f,
                "test data file {} is missing: run `{fetch_command}`",
                path.display()
            ),
            Self::Read { path, message } => write!(f, "cannot read {}: {message}", path.display()),
            Self::Write { path, message } => {
                write!(f, "cannot write {}: {message}", path.display())
            }
            Self::GoldenMismatch {
                name,
                pixels,
                actual,
                diff,
            } => write!(
                f,
                "frame {name}: {pixels} pixels differ from the reference; see {} and {}",
                actual.display(),
                diff.display()
            ),
            Self::Config(error) => write!(f, "machine configuration: {error}"),
            Self::NoKey(c) => write!(f, "no Atari key types '{c}'"),
        }
    }
}

impl Error for HarnessError {}

impl From<atari_core::ConfigError> for HarnessError {
    fn from(error: atari_core::ConfigError) -> Self {
        Self::Config(error)
    }
}
