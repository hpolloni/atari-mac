//! Test data files (REQ-020, REQ-021).

use std::fs;
use std::path::{Path, PathBuf};

use crate::HarnessError;

/// The command that downloads the ROMs and the small test data.
pub const FETCH_ROMS: &str = "make roms";

/// The command that downloads the `SingleStepTests` suite too.
pub const FETCH_TEST_DATA: &str = "make test-data";

/// The root directory of the repository.
#[must_use]
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Reads a file below the repository root. A missing file gives
/// [`HarnessError::Missing`] with `fetch_command` (REQ-021).
///
/// # Errors
///
/// [`HarnessError::Missing`] if the file does not exist, [`HarnessError::Read`] if it
/// cannot be read.
pub fn read_test_file(relative: &str, fetch_command: &str) -> Result<Vec<u8>, HarnessError> {
    read_file(&repo_root().join(relative), fetch_command)
}

/// Reads a file at `path`. A missing file gives [`HarnessError::Missing`].
///
/// # Errors
///
/// As [`read_test_file`].
pub fn read_file(path: &Path, fetch_command: &str) -> Result<Vec<u8>, HarnessError> {
    if !path.exists() {
        return Err(HarnessError::Missing {
            path: path.to_path_buf(),
            fetch_command: fetch_command.to_owned(),
        });
    }
    fs::read(path).map_err(|error| HarnessError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

/// The `AltirraOS` ROM (`roms/altirraos-xl.rom`).
///
/// # Errors
///
/// As [`read_test_file`].
pub fn os_rom() -> Result<Vec<u8>, HarnessError> {
    read_test_file("roms/altirraos-xl.rom", FETCH_ROMS)
}

/// The Altirra BASIC ROM (`roms/altirra-basic.rom`).
///
/// # Errors
///
/// As [`read_test_file`].
pub fn basic_rom() -> Result<Vec<u8>, HarnessError> {
    read_test_file("roms/altirra-basic.rom", FETCH_ROMS)
}
