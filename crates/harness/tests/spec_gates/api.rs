//! The tests of the helpers of `atari_harness::spec` on synthetic input: the baseline of the
//! gates (`docs/spec/gate-baseline.txt`), the forms of a violation and the criteria count of a
//! requirement. A submodule of `spec_gates.rs`.

use super::*;

/// Verifies REQ-044: the number of acceptance criteria is the number of lines `N. ` in the
/// section `Acceptance criteria`, and 0 without the section.
#[test]
fn requirement_counts_its_criteria() {
    let criteria = |fixture: &Fixture| {
        let spec = fixture.spec();
        spec.requirements.first().map(spec::Requirement::criteria)
    };
    let fixture = Fixture::new();
    assert_eq!(criteria(&fixture), Some(1));
    let file = Fixture::req_file(1);
    fixture.replace(
        &file,
        "`$FF`.\n",
        "`$FF`.\n2. A second criterion.\nNot 3. a criterion.\n",
    );
    assert_eq!(criteria(&fixture), Some(2));
    fixture.replace(&file, "## Acceptance criteria", "## Other");
    assert_eq!(criteria(&fixture), Some(0));
}

/// A violation of `rule` in `file`, at `line`.
fn violation(rule: u16, file: &str, line: Option<usize>, message: &str) -> Violation {
    Violation::new(rule, Path::new(file), line, message)
}

/// Verifies REQ-055: a violation has the baseline form `REQ-NNN file: message` without the
/// line, and the display form with the line, if known.
#[test]
fn violation_has_a_key_and_a_display_form() {
    let with_line = violation(46, "docs/a.md", Some(7), "bad");
    assert_eq!(with_line.key(), "REQ-046 docs/a.md: bad");
    assert_eq!(with_line.to_string(), "REQ-046 docs/a.md:7: bad");
    let without_line = violation(46, "docs/a.md", None, "bad");
    assert_eq!(without_line.to_string(), "REQ-046 docs/a.md: bad");
    assert_eq!(without_line.key(), with_line.key(), "the key has no line");
}

/// Verifies REQ-055: the baseline ignores empty lines and comments, permits the violations
/// that it lists, and finds the lines that match no violation and the lines without a rule of
/// the gates.
#[test]
fn baseline_permits_listed_violations_and_finds_stale_lines() {
    let both = [
        violation(46, "docs/a.md", Some(3), "bad"),
        violation(47, "docs/b.md", None, "worse"),
    ];
    let (listed, other) = (&both[..1], &both[1..]);
    let none: [String; 0] = [];
    let baseline = Baseline::parse("# A comment\n\n  REQ-046 docs/a.md: bad  \nREQ-052 x.md: y\n");
    assert_eq!(baseline.len(), 2, "{baseline:?}");
    assert!(!baseline.is_empty());
    assert!(baseline.contains(&both[0]));
    assert!(!baseline.contains(&both[1]));
    assert_eq!(baseline.filter(&both), other);
    assert_eq!(baseline.stale(listed, &[46]), none);
    assert_eq!(baseline.stale(other, &[46, 47]), ["REQ-046 docs/a.md: bad"]);
    assert_eq!(baseline.stale(other, &[47]), none, "only the rules");
    assert_eq!(baseline.unknown_rules(), ["REQ-052 x.md: y"]);
    let empty = Baseline::parse("# Only a comment\n\n");
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.stale(listed, RULES), none);
    assert_eq!(empty.unknown_rules(), none);
    assert_eq!(empty.filter(listed), listed);
}

/// Verifies REQ-055: the baseline is read from `docs/spec/gate-baseline.txt` below the root;
/// without the file, the baseline is empty, and `check` reports a violation of the fixture.
#[test]
fn baseline_is_read_from_the_repository_root() {
    let fixture = Fixture::new();
    assert!(Baseline::read(&fixture.root).unwrap().is_empty());
    fixture.write(spec::BASELINE, "# Old violations\nREQ-046 docs/a.md: bad\n");
    let baseline = Baseline::read(&fixture.root).unwrap();
    assert_eq!(baseline.len(), 1);
    assert!(baseline.contains(&violation(46, "docs/a.md", None, "bad")));
    fixture.replace(&Fixture::req_file(1), "priority: must", "priority: maybe");
    assert_reports(&fixture.all(), 43, "priority");
}

/// Verifies REQ-049: the `trace` gate does not read the Rust files below a `target` directory
/// or a hidden directory.
#[test]
fn trace_gate_skips_target_and_hidden_directories() {
    let code = format!(
        "/// Verifies {}.\n#[test]\nfn unplanned_test() {{}}\n",
        req(1)
    );
    let needle = "`unplanned_test` verifies";
    for dir in ["crates/demo/target", "src/.hidden"] {
        let fixture = Fixture::new();
        fixture.write(&format!("{dir}/other.rs"), &code);
        let found = fixture.run(trace);
        assert!(found.is_empty(), "{dir}: {found:?}");
    }
    rejects(trace, 49, needle, |f| {
        f.write("crates/demo/tests/other.rs", &code);
    });
}
