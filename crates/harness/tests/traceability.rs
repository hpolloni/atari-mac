//! Requirement traceability (REQ-027).
//!
//! The rules are the `trace` gate of `atari_harness::spec` (`docs/design/spec-gates.md`).
//! Each requirement with the verification method `test` must have tests that name it in a
//! `/// Verifies REQ-NNN` documentation comment on a `#[test]` function. When the work
//! package or the slice that plans the requirement is `done` (the slice frontmatter), at
//! least one of these tests must run. A test must not name a requirement that does not exist
//! or that is withdrawn, and each test that names a requirement must be in its test plan
//! (REQ-049). The full matrix goes to `target/traceability.md`.

use std::fs;
use std::path::Path;

use atari_harness::data::repo_root;
use atari_harness::spec::{self, Baseline, MATRIX};

/// Verifies REQ-027: each requirement of a finished package has a test that runs, and each
/// test names a valid requirement. Writes the matrix.
#[test]
fn finished_packages_have_running_tests() {
    let root = repo_root();
    let spec = spec::load(&root).unwrap();
    let baseline = Baseline::read(&root).unwrap();
    let report = spec::matrix(&spec, &spec::check(&spec, &root), &baseline);
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join(MATRIX), &report).unwrap();
    println!(
        "{}",
        report
            .lines()
            .find(|l| l.starts_with("Counts: "))
            .unwrap_or_default()
    );
    let errors: Vec<String> = baseline
        .filter(&spec::trace(&spec))
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        errors.is_empty(),
        "traceability errors:\n{}",
        errors.join("\n")
    );
}

/// Verifies REQ-027: the parser finds IDs, ranges, ignored tests and stray comments.
#[test]
fn traceability_parser_finds_tests() {
    assert_eq!(
        spec::requirement_numbers("REQ-001, REQ-005 to REQ-007; REQ-2xx"),
        [1, 5, 6, 7].into(),
        "IDs and ranges"
    );
    let text = concat!(
        "/// Verifies REQ-002.\n#[test]\n#[ignore = \"x\"]\nfn a() {}\n\n",
        "/// Verifies REQ-003.\nfn helper() {}\n\n",
        "/// Verifies REQ-004.\nstruct Data;\n\n",
        "#[test]\nfn plain() {}\n",
    );
    let tests = spec::tests_in(Path::new("x.rs"), text);
    let summary: Vec<(&str, bool, bool, Vec<u16>)> = tests
        .iter()
        .map(|t| {
            (
                t.function.as_str(),
                t.is_test,
                t.ignored,
                t.verifies.clone(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("a", true, true, vec![2]),
            ("helper", false, false, vec![3]),
            ("", false, false, vec![4]),
            ("plain", true, false, vec![]),
        ],
        "tests, helpers and stray comments"
    );
    assert_eq!(tests.first().map(|t| t.line), Some(4), "line of `fn a`");
}

/// Verifies REQ-027: a `Verifies` comment belongs to the next function after other
/// attributes, empty lines and comments, the parser finds the name after `pub fn` and
/// `async fn`, and a function without a `#[test]` or a `Verifies` comment is not a test.
#[test]
fn traceability_parser_skips_attributes_and_comments() {
    let text = concat!(
        "/// Verifies REQ-002.\n#[test]\n#[expect(lint, reason = \"x\")]\npub fn first_test() {}\n\n",
        "/// Verifies REQ-003.\n\n// A comment.\n#[test]\n    pub(crate) async fn second_test() {}\n\n",
        "fn helper() {}\n\n",
        "#[test]\nfn third_test() {}\n",
    );
    let tests = spec::tests_in(Path::new("x.rs"), text);
    let summary: Vec<(&str, bool, Vec<u16>)> = tests
        .iter()
        .map(|t| (t.function.as_str(), t.is_test, t.verifies.clone()))
        .collect();
    assert_eq!(
        summary,
        [
            ("first_test", true, vec![2]),
            ("second_test", true, vec![3]),
            ("third_test", true, vec![]),
        ],
    );
}
