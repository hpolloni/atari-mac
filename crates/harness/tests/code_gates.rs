//! The code gates: the complexity of a function (REQ-058) and the length of a file (REQ-059).
//! The test names follow the test plan in `docs/design/code-gates.md`.

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

/// Verifies REQ-058: the complexity of a function is 1, plus 1 for each `if` (also `if let`
/// and `else if`), `while` (also `while let`), `for`, `&&`, `||` and match guard, plus the
/// number of arms of each `match` minus 1. A closure counts in the function that contains it,
/// and a nested `fn` is a function of its own (criterion 1).
#[test]
#[ignore = "planned: SLICE-12"]
fn complexity_counts_each_decision() {
    planned();
}

/// Verifies REQ-058: the gate reads each `.rs` file of the workspace that git tracks
/// (`src/`, `tests/`, `benches/`, `build.rs` and the same paths of each crate in `crates/`),
/// also the test code (criterion 2).
#[test]
#[ignore = "planned: SLICE-12"]
fn complexity_reads_tracked_files() {
    planned();
}

/// Verifies REQ-058: a function with a complexity above 15 that is not on the code baseline
/// fails the gate, and the message gives the file, the line, the function name and the
/// complexity (criterion 3).
#[test]
#[ignore = "planned: SLICE-12"]
fn complexity_limit_fails() {
    planned();
}

/// Verifies REQ-058: a baseline line `complexity`, file, function name (`Type::name` in an
/// `impl` block, `name` otherwise) and complexity allows that function above 15 (criterion 4).
/// A function above its baseline value fails the gate, and a line that matches no function
/// above 15, or that has a value above the current complexity, fails the gate as stale
/// (criterion 5).
#[test]
#[ignore = "planned: SLICE-12"]
fn complexity_baseline_rules() {
    planned();
}

/// Verifies REQ-058: the complexity gate passes on the repository with
/// `docs/spec/code-baseline.txt`: no function above 15 that is not on the baseline
/// (criterion 3), and no function above its baseline value and no stale line (criterion 5).
#[test]
#[ignore = "planned: SLICE-12"]
fn complexity_gate_passes_on_repository() {
    planned();
}

/// Verifies REQ-058: the code gates run in `make pre-commit`, in `make check` and in the CI
/// job `Specification` (criterion 6).
#[test]
#[ignore = "planned: SLICE-12"]
fn code_gates_in_spec_check() {
    planned();
}

/// Verifies REQ-059: the length of a file is its number of newline characters, plus 1 if the
/// last line has no newline (criterion 1).
#[test]
#[ignore = "planned: SLICE-12"]
fn file_length_counts_lines() {
    planned();
}

/// Verifies REQ-059: a file with more than 1500 lines that is not on the code baseline fails
/// the gate, and the message gives the file and its length (criterion 2).
#[test]
#[ignore = "planned: SLICE-12"]
fn file_length_limit_fails() {
    planned();
}

/// Verifies REQ-059: a baseline line `lines`, file and length allows that file above 1500
/// lines (criterion 3). A file longer than its baseline value fails the gate, and a line for
/// a file that does not exist, that has 1500 lines or fewer, or that is shorter than its
/// baseline value, fails the gate as stale (criterion 4).
#[test]
#[ignore = "planned: SLICE-12"]
fn file_length_baseline_rules() {
    planned();
}

/// Verifies REQ-059: the file length gate passes on the repository with
/// `docs/spec/code-baseline.txt`: no file above 1500 lines that is not on the baseline
/// (criterion 2), and no file above its baseline value and no stale line (criterion 4).
#[test]
#[ignore = "planned: SLICE-12"]
fn file_length_gate_passes_on_repository() {
    planned();
}
