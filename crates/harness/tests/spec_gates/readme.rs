//! The tests of the README status block, the `readme` gate and the status rules of the
//! plan (REQ-056, REQ-057). A submodule of `spec_gates.rs`, so that the file stays below
//! its line limit.

use super::*;

/// Asserts that `violations` has no violation of `rule` whose message contains `needle`.
#[track_caller]
fn assert_not_reported(violations: &[Violation], rule: u16, needle: &str) {
    let found: Vec<String> = violations
        .iter()
        .filter(|v| v.rule == rule && v.message.contains(needle))
        .map(ToString::to_string)
        .collect();
    assert!(
        found.is_empty(),
        "unexpected violations:\n{}",
        found.join("\n")
    );
}

/// The text of the README of the fixture.
fn readme_of(fixture: &Fixture) -> String {
    fs::read_to_string(fixture.root.join(README)).unwrap()
}

/// Verifies REQ-056, REQ-057: the README of the repository has one status block with a row
/// for each slice file, and the block gives the number of requirements with the status
/// `implemented` (REQ-057 criterion 4).
#[test]
fn readme_status_block_is_current_on_repository() {
    let root = repo_root();
    let spec = spec::load(&root).unwrap();
    let stale: Vec<String> = spec::indexes(&spec, &root)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(stale.is_empty(), "{}", stale.join("\n"));
    let text = fs::read_to_string(root.join(README)).unwrap();
    let block = format!(
        "{STATUS_BEGIN}\n{}{STATUS_END}\n",
        spec::status_block(&spec)
    );
    assert_eq!(
        text.matches(&block).count(),
        1,
        "README.md has the block once"
    );
    let slice_files = fs::read_dir(root.join("docs/plan"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("SLICE-")
        })
        .count();
    assert_eq!(block.matches("| [SLICE-").count(), slice_files);
    for slice in &spec.slices {
        let link = format!("[SLICE-{0:02}](docs/plan/SLICE-{0:02}.md)", slice.id);
        assert!(block.contains(&link), "a row links {link}");
    }
    let implemented = spec
        .requirements
        .iter()
        .filter(|r| r.status == Status::Implemented)
        .count();
    assert!(
        block.contains(&format!("Implemented: {implemented}. Open items")),
        "the README of the repository gives the number of implemented requirements"
    );
}

/// Verifies REQ-056, REQ-057: `spec-index` writes the status block and keeps the text outside
/// it, the `indexes` gate fails when the block differs from a new generation, and the block
/// gives the number of requirements with the status `implemented` (REQ-057 criterion 4).
#[test]
fn readme_status_block_is_current() {
    let fixture = Fixture::new();
    let text = readme_of(&fixture);
    assert!(
        text.contains(
            "| [SLICE-01](docs/plan/SLICE-01.md) | First slice | in-progress | 1 | 1 | 0 | 1 |"
        ),
        "the row has the counts of the states:\n{text}"
    );
    assert!(
        text.contains(
            "All 3 requirements: 1 tested, 1 pending, 0 missing, 1 not a test requirement. \
             Implemented: 2. Open items with the status `open`: 1."
        ),
        "the counts of the matrix:\n{text}"
    );
    assert!(
        fixture.run(spec::indexes).is_empty(),
        "the block is current"
    );

    let above = "# Demo\n\nText | above.\n\n";
    let below = "\nText below.\n<!-- other -->\n";
    fixture.write(
        README,
        &format!("{above}{STATUS_BEGIN}\nold\n{STATUS_END}\n{below}"),
    );
    assert_reports(
        &fixture.run(spec::indexes),
        56,
        "status block is not up to date",
    );
    let _counts = spec::generate(&fixture.root).unwrap();
    let text = readme_of(&fixture);
    assert!(
        text.starts_with(&format!("{above}{STATUS_BEGIN}\n")),
        "{text}"
    );
    assert!(
        text.ends_with(&format!("\n{STATUS_END}\n{below}")),
        "{text}"
    );
    assert!(!text.contains("old"), "{text}");
    assert!(
        fixture.run(spec::indexes).is_empty(),
        "the block is current"
    );

    fixture.replace(
        "docs/plan/SLICE-01.md",
        "title: First slice",
        "title: New slice",
    );
    assert_reports(
        &fixture.run(spec::indexes),
        56,
        "status block is not up to date",
    );
    assert_reports(&fixture.all(), 56, "status block is not up to date");
    let _counts = spec::generate(&fixture.root).unwrap();
    assert!(readme_of(&fixture).contains("| New slice |"));
    assert!(fixture.all().is_empty(), "all gates pass");

    fixture.replace(
        &Fixture::req_file(2),
        "status: implemented",
        "status: withdrawn",
    );
    let _counts = spec::generate(&fixture.root).unwrap();
    assert!(
        readme_of(&fixture).contains("Implemented: 1. Open items"),
        "the count follows the status `implemented`"
    );
}

/// Verifies REQ-057: a requirement in a `done` package or slice without the status
/// `implemented` fails (criterion 1), a requirement with the status `implemented` in no
/// package or in a package that is not `done` fails (criterion 2), and a withdrawn
/// requirement makes no failure of these rules (criterion 3).
#[test]
fn status_follows_the_plan() {
    fixture_passes();
    let slice = "docs/plan/SLICE-01.md";
    let (one, two, three) = (req(1), req(2), req(3));
    let one_file = Fixture::req_file(1);
    let three_file = Fixture::req_file(3);
    for status in ["draft", "accepted"] {
        let needle = format!(
            "{one} is in SLICE-01 WP-1, which is done, but its status is `{status}`, not \
             `implemented`"
        );
        rejects(slice_rules, 57, &needle, |f| {
            f.replace(
                &one_file,
                "status: implemented",
                &format!("status: {status}"),
            );
        });
    }
    let in_done_slice = format!(
        "{three} is in SLICE-01 WP-2, which is done, but its status is `draft`, not \
         `implemented`"
    );
    rejects(slice_rules, 57, &in_done_slice, |f| {
        f.replace(slice, "status: in-progress", "status: done");
    });
    let not_done = format!("{three} has the status `implemented`, but SLICE-01 WP-2 is not done");
    rejects(slice_rules, 57, &not_done, |f| {
        f.replace(&three_file, "status: draft", "status: implemented");
    });
    let package_not_done =
        format!("{one} has the status `implemented`, but SLICE-01 WP-1 is not done");
    rejects(slice_rules, 57, &package_not_done, |f| {
        f.replace(
            slice,
            "    title: Work\n    status: done",
            "    title: Work\n    status: in-progress",
        );
    });
    let unplanned = format!("{one} has the status `implemented`, but it is in no package");
    rejects(slice_rules, 57, &unplanned, |f| {
        f.replace(slice, &format!("[{one}, {two}]"), &format!("[{two}]"));
    });

    let fixture = Fixture::new();
    fixture.replace(&one_file, "status: implemented", "status: withdrawn");
    fixture.replace(&three_file, "status: draft", "status: withdrawn");
    fixture.replace(slice, "status: in-progress", "status: done");
    fixture.replace(slice, "    status: planned", "    status: done");
    assert_not_reported(&fixture.run(slice_rules), 57, "");
    let fixture = Fixture::new();
    fixture.replace(&one_file, "status: implemented", "status: withdrawn");
    fixture.replace(slice, &format!("[{one}, {two}]"), &format!("[{two}]"));
    assert_not_reported(&fixture.run(slice_rules), 57, "");
    let fixture = Fixture::new();
    fixture.replace(&three_file, "status: draft", "status: implemented");
    fixture.replace(slice, "status: in-progress", "status: done");
    fixture.replace(slice, "    status: planned", "    status: done");
    assert_not_reported(&fixture.run(slice_rules), 57, "");
}

/// Verifies REQ-056: a missing, duplicated or unclosed status marker fails the `indexes`
/// gate, and `spec-index` does not change the README.
#[test]
fn readme_status_block_rejects_bad_markers() {
    let begin = format!("{STATUS_BEGIN}\n");
    let end = format!("{STATUS_END}\n");
    let cases: [(&str, String); 5] = [
        ("no line `<!-- BEGIN", String::new()),
        (
            "BEGIN GENERATED: status -->` is duplicated",
            format!("{begin}{begin}"),
        ),
        (
            "END GENERATED: status -->` is duplicated",
            format!("{end}{end}"),
        ),
        ("is not closed", String::new()),
        ("is before", String::new()),
    ];
    for (index, (needle, replacement)) in cases.iter().enumerate() {
        let fixture = Fixture::new();
        let good = readme_of(&fixture);
        let bad = match index {
            0 | 1 => good.replacen(&begin, replacement, 1),
            2 | 3 => good.replacen(&end, replacement, 1),
            _ => format!("{end}{}", good.replacen(&end, "", 1)),
        };
        fixture.write(README, &bad);
        assert_reports(&fixture.run(spec::indexes), 56, needle);
        let error = spec::generate(&fixture.root).unwrap_err();
        assert!(
            matches!(&error, SpecError::Markers { message, .. } if message.contains(needle)),
            "{error}"
        );
        assert!(error.to_string().contains(README), "{error}");
        assert_eq!(readme_of(&fixture), bad, "the README does not change");
    }
    rejects(spec::indexes, 56, "the file does not exist", |f| {
        fs::remove_file(f.root.join(README)).unwrap();
    });
    rejects(spec::indexes, 56, "no line", |f| {
        f.replace(README, &begin, &format!("  {begin}"));
    });
}

/// Verifies REQ-056: the links and the IDs of the `README.md` of the repository pass the
/// `references` gate.
#[test]
fn readme_links_and_ids_are_checked_on_repository() {
    let root = repo_root();
    let spec = spec::load(&root).unwrap();
    let found: Vec<String> = spec::references(&spec, &root)
        .iter()
        .filter(|v| v.file == Path::new(README))
        .map(ToString::to_string)
        .collect();
    assert!(found.is_empty(), "{}", found.join("\n"));
    assert!(
        spec.documents.iter().any(|d| d.path == Path::new(README)),
        "the gates read README.md"
    );
}

/// Verifies REQ-056: the `references` gate checks the links and the IDs of `README.md`.
#[test]
fn readme_links_and_ids_are_checked() {
    let fixture = Fixture::new();
    fixture.replace(
        README,
        "Text above.\n",
        &format!(
            "Text above.\n\n[x](docs/missing.md), [y](docs/plan/) and {}.\n",
            req(999)
        ),
    );
    let found = fixture.run(spec::references);
    assert_reports(&found, 47, "the link `docs/missing.md` points to no file");
    assert_reports(&found, 47, &format!("`{}` names no file", req(999)));
    assert_not_reported(&found, 47, "docs/plan/");
    assert!(
        found
            .iter()
            .all(|v| v.file == Path::new(README) && v.line == Some(5)),
        "the file and the line: {found:?}"
    );
    let item = format!("OI-{:03}", 7);
    rejects(
        spec::references,
        47,
        &format!("`{item}` names no file"),
        |f| {
            f.replace(README, "SLICE-01.\n", &format!("{item}.\n"));
        },
    );
    let slice = format!("SLICE-{:02}", 9);
    rejects(
        spec::references,
        47,
        &format!("`{slice}` names no file"),
        |f| {
            f.replace(README, "SLICE-01.\n", &format!("{slice}.\n"));
        },
    );
}

/// Verifies REQ-056: the `make` targets and the command-line options of the `README.md` of the
/// repository exist.
#[test]
fn readme_gate_passes_on_repository() {
    repository_passes(spec::readme, &[56]);
}

/// Verifies REQ-056: each `make <target>` in a code span or a code block of `README.md` is
/// a target in `Makefile`.
#[test]
fn readme_make_targets_exist() {
    fixture_passes();
    let needle = "`nothing` is not a target in Makefile";
    rejects(spec::readme, 56, needle, |f| {
        f.replace(README, "`make check`", "`make nothing`");
    });
    rejects(spec::readme, 56, needle, |f| {
        f.replace(README, "make test ", "make -k nothing ");
    });
    rejects(spec::readme, 56, "`FLAGS` is not a target", |f| {
        f.replace(README, "`make check`", "`make FLAGS`");
    });
    let fixture = Fixture::new();
    fixture.replace(README, "`make check`", "`make nothing`");
    assert_reports(&fixture.all(), 56, needle);
    let found: Vec<Violation> = fixture.run(spec::readme);
    assert_eq!(found.len(), 1, "{found:?}");
    let line = readme_of(&fixture)
        .lines()
        .position(|l| l.contains("`make nothing`"))
        .map(|index| index + 1);
    assert_eq!(found.first().and_then(|v| v.line), line, "the line");
    let fixture = Fixture::new();
    fixture.replace(README, "Text above.", "Text above: make nothing, `make`.");
    assert!(fixture.run(spec::readme).is_empty(), "prose is not checked");
    rejects(spec::readme, 56, "`check` is not a target", |f| {
        fs::remove_file(f.root.join("Makefile")).unwrap();
    });
}

/// Verifies REQ-056: each command-line option of `README.md` is in the help text of
/// `src/cli.rs`, except options of other programs in a code span that starts with `cargo`.
#[test]
fn readme_options_exist() {
    fixture_passes();
    let needle = "`--ntsc` is not an option in the help text of src/cli.rs";
    rejects(spec::readme, 56, needle, |f| {
        f.replace(README, "`demo --pal`", "`demo --ntsc`");
    });
    rejects(spec::readme, 56, needle, |f| {
        f.replace(README, "Text above.", "Text above, with --ntsc.");
    });
    rejects(spec::readme, 56, needle, |f| {
        f.replace(README, "-- --pal\n", "-- --pal --ntsc\n");
    });
    rejects(spec::readme, 56, needle, |f| {
        f.replace(
            README,
            "Text above.",
            "Use `cargo run --release -- --ntsc`.",
        );
    });
    rejects(spec::readme, 56, "`--pal-blending` is not an option", |f| {
        f.replace(README, "`demo --pal`", "`demo --pal-blending`");
    });
    let fixture = Fixture::new();
    fixture.replace(
        README,
        "Text above.",
        "Use `cargo test --all-targets -- --include-ignored`, `cargo run -p x --bin y -- --z` \
         and `demo -h --help`. A table | --- | and <!-- a comment -->.",
    );
    let found = fixture.run(spec::readme);
    assert!(found.is_empty(), "{found:?}");
    rejects(spec::readme, 56, "src/cli.rs has no help text", |f| {
        f.replace("src/cli.rs", "const USAGE", "const HELP");
    });
}
