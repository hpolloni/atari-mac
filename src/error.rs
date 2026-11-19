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
            | Self::Screenshot { .. } => 2,
            Self::Window(_) | Self::Output(_) => 1,
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
            Self::Media { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Screenshot { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Window(error) => write!(f, "window: {error}"),
            Self::Output(error) => write!(f, "standard output: {error}"),
        }
    }
}

impl Error for FrontendError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Usage(_) | Self::Unsupported(_) => None,
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
                "x.rom: unsupported cartridge size: 100 bytes",
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
                FrontendError::Usage(_) | FrontendError::Unsupported(_)
            );
            assert_eq!(error.source().is_some(), has_source, "source of {error}");
        }
    }
}
