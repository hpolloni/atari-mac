//! Finds the tests and the `/// Verifies REQ-NNN` comments in Rust files (REQ-027).

use std::fs;
use std::mem;
use std::path::{Path, PathBuf};

use super::{CodeTest, SpecError, text};

/// All `.rs` files below `dir`, except `target` and hidden directories. A missing directory
/// gives no files.
pub(super) fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SpecError> {
    if !dir.is_dir() {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|error| SpecError::Io {
        path: dir.to_path_buf(),
        message: error.to_string(),
    })?;
    for path in entries.filter_map(Result::ok).map(|e| e.path()) {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        if path.is_dir() {
            if name != "target" && !name.starts_with('.') {
                rust_files(&path, out)?;
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// The name of the function that `line` declares, if any.
fn function_name(line: &str) -> Option<&str> {
    let start = if line.starts_with("fn ") {
        0
    } else {
        line.find(" fn ")? + 1
    };
    let rest = line.get(start + 3..)?;
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    rest.get(..end).filter(|name| !name.is_empty())
}

/// The state of the scan between two items.
#[derive(Debug, Default)]
struct Pending {
    /// The requirements of the `Verifies` comment.
    verifies: Vec<u16>,
    /// A `#[test]` attribute was seen.
    is_test: bool,
    /// An `#[ignore]` attribute was seen.
    ignored: bool,
}

/// The `#[test]` functions and the functions with a `/// Verifies` comment in `text`.
///
/// A `Verifies` comment is a documentation line that starts with `Verifies`. Its
/// requirement IDs (ranges expanded) belong to the next function. A `Verifies` comment on an
/// item that is not a function gives a [`CodeTest`] with an empty function name. `file` is
/// the path that the result records.
#[must_use]
pub fn tests_in(file: &Path, text: &str) -> Vec<CodeTest> {
    let mut found = Vec::new();
    let mut pending = Pending::default();
    for (index, line) in text.lines().map(str::trim).enumerate() {
        if let Some(doc) = line.strip_prefix("///") {
            if doc.trim_start().starts_with("Verifies") {
                pending.verifies.extend(text::requirement_numbers(doc));
            }
        } else if line.starts_with("#[test]") {
            pending.is_test = true;
        } else if line.starts_with("#[ignore") {
            pending.ignored = true;
        } else if !line.starts_with("#[") && !line.is_empty() && !line.starts_with("//") {
            let item = mem::take(&mut pending);
            let name = function_name(line);
            if !item.verifies.is_empty() || (item.is_test && name.is_some()) {
                found.push(CodeTest {
                    file: file.to_path_buf(),
                    line: index + 1,
                    function: name.unwrap_or_default().to_owned(),
                    is_test: item.is_test,
                    ignored: item.ignored,
                    verifies: item.verifies,
                });
            }
        }
    }
    found
}
