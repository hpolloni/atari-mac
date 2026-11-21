//! The errors of the frontend.
//!
//! Design: `docs/design/architecture.md`, "Error handling". The binary prints each error to
//! standard error. A usage or file error gives exit status 2.

use core::error::Error;
use core::fmt;
use std::io;
use std::path::PathBuf;

use atari_core::ConfigError;
use atari_core::media::MediaError;

/// An error of the frontend.
#[derive(Debug)]
pub enum FrontendError {
    /// The command line is not valid.
    Usage(String),
    /// The command line option is not supported yet.
    Unsupported(&'static str),
    /// A file cannot be read.
    Read {
        /// The file.
        path: PathBuf,
        /// The cause.
        source: io::Error,
    },
    /// A ROM file has the wrong size.
    Rom {
        /// The ROM file.
        path: PathBuf,
        /// The cause.
        error: ConfigError,
    },
    /// A media file is not valid.
    Media {
        /// The media file.
        path: PathBuf,
        /// The cause.
        error: MediaError,
    },
    /// The screenshot cannot be written.
    Screenshot {
        /// The PNG file.
        path: PathBuf,
        /// The cause.
        source: png::EncodingError,
    },
    /// The BASIC program file has a character without an Atari key (REQ-080 criterion 4).
    BasCharacter {
        /// The program file.
        path: PathBuf,
        /// The line number, from 1.
        line: usize,
        /// The character.
        character: char,
    },
    /// The BASIC `Ready` prompt did not show, so the program cannot be typed (REQ-080
    /// criterion 6).
    BasicNotStarted {
        /// The frames that the frontend waited.
        frames: u32,
    },
    /// The window cannot be opened or updated.
    Window(minifb::Error),
    /// Standard output cannot be written.
    Output(io::Error),
}

impl FrontendError {
    /// The exit status of the binary for this error: 2 for a usage or file error, else 1.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_)
            | Self::Unsupported(_)
            | Self::Read { .. }
            | Self::Rom { .. }
            | Self::Media { .. }
            | Self::Screenshot { .. }
            | Self::BasCharacter { .. } => 2,
            Self::BasicNotStarted { .. } | Self::Window(_) | Self::Output(_) => 1,
        }
    }
}

impl fmt::Display for FrontendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "{message} (see --help)"),
            Self::Unsupported(option) => write!(f, "{option} is not supported yet"),
            Self::Read { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Rom { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Media { path, error } => {
                write!(
                    f,
                    "{}: {error} ({})",
                    path.display(),
                    media_error_name(error)
                )
            }
            Self::Screenshot { path, source } => write!(f, "{}: {source}", path.display()),
            Self::BasCharacter {
                path,
                line,
                character,
            } => write!(
                f,
                "{}: line {line}: the character '{}' has no key on the Atari keyboard",
                path.display(),
                character.escape_debug()
            ),
            Self::BasicNotStarted { frames } => write!(
                f,
                "BASIC did not start: no Ready prompt after {frames} frames, so the program was not typed"
            ),
            Self::Window(error) => write!(f, "window: {error}"),
            Self::Output(error) => write!(f, "standard output: {error}"),
        }
    }
}

/// The name of the variant of `error`, for example `CasSignature`: the start of its
/// `Debug` text. A media message names it (REQ-081 criterion 2).
fn media_error_name(error: &MediaError) -> String {
    format!("{error:?}")
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect()
}

impl Error for FrontendError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Usage(_)
            | Self::Unsupported(_)
            | Self::BasCharacter { .. }
            | Self::BasicNotStarted { .. } => None,
            Self::Read { source, .. } | Self::Output(source) => Some(source),
            Self::Rom { error, .. } => Some(error),
            Self::Media { error, .. } => Some(error),
            Self::Screenshot { source, .. } => Some(source),
            Self::Window(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::error::Error as _;
    use std::io;
    use std::path::PathBuf;

    use atari_core::ConfigError;
    use atari_core::media::MediaError;

    use super::FrontendError;

    #[test]
    fn messages_name_the_file_and_exit_codes() {
        let path = PathBuf::from("x.rom");
        let cases = [
            (
                FrontendError::Usage("bad".to_owned()),
                "bad (see --help)",
                2,
            ),
            (
                FrontendError::Unsupported("--xex"),
                "--xex is not supported yet",
                2,
            ),
            (
                FrontendError::Read {
                    path: path.clone(),
                    source: io::Error::other("gone"),
                },
                "x.rom: gone",
                2,
            ),
            (
                FrontendError::Rom {
                    path: path.clone(),
                    error: ConfigError::OsRomSize(100),
                },
                "x.rom: OS ROM must be 16384 bytes, not 100 bytes",
                2,
            ),
            (
                FrontendError::Media {
                    path: path.clone(),
                    error: MediaError::BadSize(100),
                },
                "x.rom: unsupported cartridge size: 100 bytes (BadSize)",
                2,
            ),
            (
                FrontendError::Media {
                    path: PathBuf::from("t.cas"),
                    error: MediaError::CasSignature,
                },
                "t.cas: CAS file does not start with a FUJI chunk (CasSignature)",
                2,
            ),
            (
                FrontendError::Media {
                    path: PathBuf::from("t.cas"),
                    error: MediaError::CasTruncated { offset: 8 },
                },
                "t.cas: CAS chunk at offset 8 goes past the end of the file (CasTruncated)",
                2,
            ),
            (
                FrontendError::Screenshot {
                    path,
                    source: png::EncodingError::from(io::Error::other("full")),
                },
                "x.rom: full",
                2,
            ),
            (
                FrontendError::BasCharacter {
                    path: PathBuf::from("p.bas"),
                    line: 3,
                    character: '{',
                },
                "p.bas: line 3: the character '{' has no key on the Atari keyboard",
                2,
            ),
            (
                FrontendError::BasicNotStarted { frames: 600 },
                "BASIC did not start: no Ready prompt after 600 frames",
                1,
            ),
            (
                FrontendError::Window(minifb::Error::WindowCreate("no".to_owned())),
                "window: ",
                1,
            ),
            (
                FrontendError::Output(io::Error::other("closed")),
                "standard output: closed",
                1,
            ),
        ];
        for (error, message, code) in cases {
            assert!(
                error.to_string().starts_with(message),
                "{error} starts with {message}"
            );
            assert_eq!(error.exit_code(), code, "exit code of {error}");
            let has_source = !matches!(
                error,
                FrontendError::Usage(_)
                    | FrontendError::Unsupported(_)
                    | FrontendError::BasCharacter { .. }
                    | FrontendError::BasicNotStarted { .. }
            );
            assert_eq!(error.source().is_some(), has_source, "source of {error}");
        }
    }
}
