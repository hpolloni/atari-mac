//! The tests of the reading of the data files, the registry and the test plans, on the
//! fixture. A submodule of `spec_gates.rs`.

use super::*;

/// Asserts that `violations` has a violation of `rule` at `line` whose message contains
/// `needle`.
#[track_caller]
fn assert_reports_at(violations: &[Violation], rule: u16, line: usize, needle: &str) {
    assert!(
        violations
            .iter()
            .any(|v| v.rule == rule && v.line == Some(line) && v.message.contains(needle)),
        "no REQ-{rule:03} violation at line {line} with {needle:?} in: {violations:?}"
    );
}

/// Verifies REQ-046 criterion 2: a line of a section index with an empty title fails with its
/// line, and a `## License` section with only empty lines fails.
#[test]
fn sources_gate_checks_section_index_lines_and_license_text() {
    fixture_passes();
    let index = "docs/spec/references/HRM.sections.txt";
    let fixture = Fixture::new();
    fixture.replace(index, "3.3\tCycle timing\n", "3.3\tCycle timing\n9.9\t \n");
    assert_reports_at(&fixture.run(sources), 46, 4, "the title is empty");
    let fixture = Fixture::new();
    fixture.replace(index, "1.1\tIntroduction", "1.1 Introduction");
    assert_reports_at(&fixture.run(sources), 46, 2, "no TAB");
    rejects(sources, 46, "no `## License` section with text", |f| {
        f.replace(
            "docs/spec/references/DEMO.md",
            "The reference has no license text.\n",
            "\n",
        );
    });
}

/// Verifies REQ-045: the empty lines and the comments of `docs/spec/vague-words.txt` are not
/// vague words.
#[test]
fn vague_words_file_skips_empty_lines_and_comments() {
    let fixture = Fixture::new();
    fixture.write(
        "docs/spec/vague-words.txt",
        "# Comment\n\ncorrect\n\n# The emulator shall\nas expected\n",
    );
    let found = fixture.run(vague_words);
    assert!(found.is_empty(), "{found:?}");
    rejects(vague_words, 45, "vague word `correct`", |f| {
        f.replace(&Fixture::req_file(1), "one thing.", "one correct thing.");
    });
}

/// Verifies REQ-048: a test plan row has its line in the design document, a table row in a
/// fenced code block of a test plan is not a row, and `docs/design/README.md` has no test
/// plan.
#[test]
fn test_plans_skip_fenced_rows_and_the_design_readme() {
    fixture_passes();
    let design = "docs/design/demo.md";
    let one = req(1);
    let fixture = Fixture::new();
    fixture.replace(design, "`demo.rs` `first_test`", "the first test");
    assert_reports_at(
        &fixture.run(test_plans),
        48,
        9,
        "names no test in backticks",
    );
    let fenced = format!("```\n| {one} | `demo.rs` `fenced_test` |\n```\n\n| Requirement");
    let fixture = Fixture::new();
    fixture.replace(design, "| Requirement", &fenced);
    let found = fixture.all();
    assert!(found.is_empty(), "a fenced row: {found:?}");
    let fixture = Fixture::new();
    fixture.write(
        "docs/design/README.md",
        &format!(
            "# Designs\n\n## Test plan\n\n| Requirement | Test |\n| --- | --- |\n\
             | {one} | `demo.rs` `other_test` |\n"
        ),
    );
    let found = fixture.all();
    assert!(found.is_empty(), "the design README: {found:?}");
}
