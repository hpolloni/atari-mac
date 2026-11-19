//! Requirement traceability (REQ-027).
//!
//! Each requirement with the verification method `Test` must have tests that name it in a
//! `/// Verifies REQ-NNN` documentation comment on a `#[test]` function. A slice
//! (`docs/plan/SLICE-NN.md`) gives each requirement to a work package. When the package
//! status is `Done`, or the slice status is `Done`, each `Test` requirement of the package
//! must have at least one test that is not ignored.
//!
//! A test must not name a requirement that does not exist or that is withdrawn. The full
//! matrix goes to `target/traceability.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::mem;
use std::path::{Path, PathBuf};

use atari_harness::data::repo_root;

/// One requirement file.
#[derive(Debug)]
struct Requirement {
    /// For example `Draft` or `Withdrawn`.
    status: String,
    /// `Test`, `Inspection` or `Demonstration`.
    verification: String,
}

/// The tests that name one requirement.
#[derive(Debug, Default)]
struct Coverage {
    /// Tests that run.
    active: Vec<String>,
    /// Tests with `#[ignore]`.
    ignored: Vec<String>,
}

/// One requirement in a slice.
#[derive(Debug)]
struct Planned {
    /// For example `SLICE-01`.
    slice: String,
    /// For example `WP-1`.
    package: String,
    /// The package or the slice is `Done`.
    done: bool,
}

/// One function with a `Verifies` comment. `name` is empty if it is not a `#[test]` function.
#[derive(Debug, PartialEq, Eq)]
struct FoundTest {
    /// The function name.
    name: String,
    /// The function has `#[ignore]`.
    ignored: bool,
    /// The requirement IDs in the comment.
    ids: BTreeSet<u16>,
}

/// The value of a `| Field | Value |` row of a markdown table.
fn field(text: &str, name: &str) -> Option<String> {
    let prefix = format!("| {name} |");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|rest| rest.trim().trim_end_matches('|').trim().to_owned())
}

/// The leading ASCII digits of `text`.
fn digits(text: &str) -> &str {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    text.get(..end).unwrap_or_default()
}

/// All `REQ-NNN` IDs in `text`, including the ranges `REQ-NNN to REQ-NNN`.
fn requirement_ids(text: &str) -> BTreeSet<u16> {
    let parts: Vec<&str> = text.split("REQ-").skip(1).collect();
    let mut ids = BTreeSet::new();
    for (index, part) in parts.iter().enumerate() {
        let Ok(first) = digits(part).parse::<u16>() else {
            continue;
        };
        let _new = ids.insert(first);
        let after = part.get(digits(part).len()..).unwrap_or_default();
        let next = parts.get(index + 1).copied().unwrap_or_default();
        if after == " to "
            && let Ok(last) = digits(next).parse::<u16>()
        {
            ids.extend(first..=last);
        }
    }
    ids
}

/// The stem of a file name, for example `REQ-001`.
fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

/// The requirement files.
fn requirements(root: &Path) -> io::Result<BTreeMap<u16, Requirement>> {
    let mut map = BTreeMap::new();
    for entry in fs::read_dir(root.join("docs/spec/requirements"))? {
        let path = entry?.path();
        let Some(Ok(number)) = stem(&path).strip_prefix("REQ-").map(str::parse) else {
            continue;
        };
        let text = fs::read_to_string(&path)?;
        let requirement = Requirement {
            status: field(&text, "Status").unwrap_or_default(),
            verification: field(&text, "Verification").unwrap_or_default(),
        };
        let _previous = map.insert(number, requirement);
    }
    Ok(map)
}

/// The cells of the markdown table rows in `section`, without the header rows.
fn table_rows(section: &str) -> Vec<Vec<String>> {
    section
        .lines()
        .filter(|line| line.starts_with('|') && !line.starts_with("| ---"))
        .skip(1)
        .map(|line| {
            line.trim_matches('|')
                .split(" | ")
                .map(|c| c.trim().to_owned())
                .collect()
        })
        .collect()
}

/// The section of `text` that starts with the heading `## {title}`.
fn section<'a>(text: &'a str, title: &str) -> &'a str {
    text.split("\n## ")
        .find(|part| part.starts_with(title))
        .unwrap_or_default()
}

/// The requirements that the slices plan, with their package and the rule state.
fn planned(root: &Path) -> io::Result<BTreeMap<u16, Planned>> {
    let mut map = BTreeMap::new();
    for entry in fs::read_dir(root.join("docs/plan"))? {
        let path = entry?.path();
        let slice = stem(&path);
        if !slice.starts_with("SLICE-") {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let slice_done = field(&text, "Status").as_deref() == Some("Done");
        let packages_done: BTreeSet<String> = table_rows(section(&text, "Work packages"))
            .into_iter()
            .filter(|row| row.get(1).is_some_and(|status| status == "Done"))
            .filter_map(|row| row.first()?.split(' ').next().map(str::to_owned))
            .collect();
        for row in table_rows(section(&text, "Requirements in the slice")) {
            let (Some(package), Some(ids)) = (row.get(1), row.get(2)) else {
                continue;
            };
            for id in requirement_ids(ids) {
                let done = slice_done || packages_done.contains(package);
                let entry = Planned {
                    slice: slice.clone(),
                    package: package.clone(),
                    done,
                };
                let _previous = map.insert(id, entry);
            }
        }
    }
    Ok(map)
}

/// All `.rs` files below `dir`, except `target` and hidden directories.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        if path.is_dir() {
            if name != "target" && !name.starts_with('.') {
                rust_files(&path, out)?;
            }
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// The name of the function that `line` declares, if any.
fn function_name(line: &str) -> Option<&str> {
    let rest = line.get(line.find("fn ")? + 3..)?;
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    rest.get(..end)
}

/// The functions with a `/// Verifies` comment in the text of one file.
fn tests_in(text: &str) -> Vec<FoundTest> {
    let mut found = Vec::new();
    let mut ids = BTreeSet::new();
    let (mut is_test, mut ignored) = (false, false);
    for line in text.lines().map(str::trim) {
        if let Some(doc) = line.strip_prefix("///") {
            if doc.trim_start().starts_with("Verifies") {
                ids.extend(requirement_ids(doc));
            }
        } else if line.starts_with("#[test]") {
            is_test = true;
        } else if line.starts_with("#[ignore") {
            ignored = true;
        } else if !line.starts_with("#[") && !line.is_empty() && !line.starts_with("//") {
            if let Some(name) = function_name(line)
                && !ids.is_empty()
            {
                let name = if is_test {
                    name.to_owned()
                } else {
                    String::new()
                };
                found.push(FoundTest {
                    name,
                    ignored,
                    ids: mem::take(&mut ids),
                });
            }
            ids.clear();
            (is_test, ignored) = (false, false);
        }
    }
    found
}

/// Adds one test to the coverage map, or an error for an invalid reference.
fn record(
    test: FoundTest,
    file: &str,
    reqs: &BTreeMap<u16, Requirement>,
    map: &mut BTreeMap<u16, Coverage>,
    errors: &mut Vec<String>,
) {
    if test.name.is_empty() {
        errors.push(format!(
            "{file}: a `Verifies` comment is not on a #[test] fn"
        ));
        return;
    }
    for id in test.ids {
        match reqs.get(&id) {
            None => errors.push(format!("{file} {}: REQ-{id:03} does not exist", test.name)),
            Some(req) if req.status == "Withdrawn" => {
                errors.push(format!("{file} {}: REQ-{id:03} is withdrawn", test.name));
            }
            Some(_) => {
                let entry = map.entry(id).or_default();
                let label = format!("{file} `{}`", test.name);
                if test.ignored {
                    entry.ignored.push(label);
                } else {
                    entry.active.push(label);
                }
            }
        }
    }
}

/// The tests of the workspace by requirement. Adds an error for each invalid reference.
fn coverage(
    root: &Path,
    reqs: &BTreeMap<u16, Requirement>,
    errors: &mut Vec<String>,
) -> io::Result<BTreeMap<u16, Coverage>> {
    let mut files = Vec::new();
    for dir in ["crates", "src", "tests"] {
        rust_files(&root.join(dir), &mut files)?;
    }
    let mut map = BTreeMap::new();
    for file in &files {
        let short = file
            .strip_prefix(root)
            .unwrap_or(file)
            .display()
            .to_string();
        for test in tests_in(&fs::read_to_string(file)?) {
            record(test, &short, reqs, &mut map, errors);
        }
    }
    Ok(map)
}

/// The traceability matrix as markdown.
fn matrix(
    reqs: &BTreeMap<u16, Requirement>,
    cov: &BTreeMap<u16, Coverage>,
    plan: &BTreeMap<u16, Planned>,
) -> String {
    let mut out = String::from(
        "# Traceability matrix\n\nGenerated by `crates/harness/tests/traceability.rs`.\n\n\
         | Requirement | Verification | Slice and package | State | Tests |\n\
         | --- | --- | --- | --- | --- |\n",
    );
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (id, req) in reqs.iter().filter(|(_, req)| req.status != "Withdrawn") {
        let c = cov.get(id);
        let state = match (
            c.map_or(0, |c| c.active.len()),
            c.map_or(0, |c| c.ignored.len()),
        ) {
            (0, 0) if req.verification != "Test" => "not a test requirement",
            (0, 0) => "missing",
            (0, _) => "pending",
            _ => "tested",
        };
        *counts.entry(state).or_default() += 1;
        let slice = plan.get(id).map_or_else(
            || "not in a slice".to_owned(),
            |p| format!("{} {}", p.slice, p.package),
        );
        let tests = c.map_or_else(String::new, |c| {
            let ignored = c.ignored.iter().map(|t| format!("{t} (ignored)"));
            c.active
                .iter()
                .cloned()
                .chain(ignored)
                .collect::<Vec<_>>()
                .join("<br>")
        });
        let row = format!(
            "| REQ-{id:03} | {} | {slice} | {state} | {tests} |",
            req.verification
        );
        out.push_str(&row);
        out.push('\n');
    }
    let summary: Vec<String> = counts
        .iter()
        .map(|(state, n)| format!("{state}: {n}"))
        .collect();
    out.push_str("\nCounts: ");
    out.push_str(&summary.join(", "));
    out.push('\n');
    out
}

/// Verifies REQ-027: each requirement of a finished package has a test that runs, and each
/// test names a valid requirement.
#[test]
fn finished_packages_have_running_tests() {
    let root = repo_root();
    let reqs = requirements(&root).unwrap();
    let mut errors = Vec::new();
    let cov = coverage(&root, &reqs, &mut errors).unwrap();
    let plan = planned(&root).unwrap();
    for (id, p) in &plan {
        let Some(req) = reqs.get(id) else {
            errors.push(format!("{}: REQ-{id:03} does not exist", p.slice));
            continue;
        };
        let runs = cov.get(id).is_some_and(|c| !c.active.is_empty());
        if p.done && req.verification == "Test" && req.status != "Withdrawn" && !runs {
            let (slice, package) = (&p.slice, &p.package);
            errors.push(format!(
                "{slice} {package} is done: REQ-{id:03} has no test that runs"
            ));
        }
    }
    let report = matrix(&reqs, &cov, &plan);
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("target/traceability.md"), &report).unwrap();
    println!("{}", report.lines().last().unwrap_or_default());
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
        requirement_ids("REQ-001, REQ-005 to REQ-007"),
        BTreeSet::from([1, 5, 6, 7]),
        "IDs and ranges"
    );
    let text = "/// Verifies REQ-002.\n#[test]\n#[ignore = \"x\"]\nfn a() {}\n\n\
                /// Verifies REQ-003.\nfn helper() {}\n";
    let tests = tests_in(text);
    let expected = FoundTest {
        name: "a".to_owned(),
        ignored: true,
        ids: BTreeSet::from([2]),
    };
    assert_eq!(tests.first(), Some(&expected), "ignored test");
    assert!(
        tests.get(1).is_some_and(|t| t.name.is_empty()),
        "a helper is not a test"
    );
}
