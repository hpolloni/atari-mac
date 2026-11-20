//! The README rules (REQ-056): the generated status block, and the `make` targets and the
//! command-line options that `README.md` names.

use core::ops::Range;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::report::{self, COMMAND, STATES};
use super::{ItemStatus, README, Spec, Violation, text};

/// The first line of the status block.
pub const STATUS_BEGIN: &str = "<!-- BEGIN GENERATED: status -->";

/// The last line of the status block.
pub const STATUS_END: &str = "<!-- END GENERATED: status -->";

/// The rule of the README checks.
const RULE: u16 = 56;

/// The text between the two marker lines of the status block: one table row for each slice,
/// then the counts of the whole matrix and the number of open items with the status `open`.
#[must_use]
pub fn status_block(spec: &Spec) -> String {
    let mut lines = vec![
        String::new(),
        format!("| Slice | Title | Status | {} |", STATES.join(" | ")),
        format!("| --- | --- | --- |{}", " --- |".repeat(STATES.len())),
    ];
    for slice in &spec.slices {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        let ids: BTreeSet<u16> = slice
            .packages
            .iter()
            .flat_map(|p| p.requirements.iter().copied())
            .collect();
        for r in ids.iter().filter_map(|&id| spec.requirement(id)) {
            if r.active() {
                *counts.entry(report::state(spec, r)).or_default() += 1;
            }
        }
        let numbers: Vec<String> = STATES
            .iter()
            .map(|s| counts.get(s).copied().unwrap_or_default().to_string())
            .collect();
        lines.push(format!(
            "| [SLICE-{id:02}](docs/plan/SLICE-{id:02}.md) | {} | {} | {} |",
            report::cell(&slice.title),
            slice.status.as_str(),
            numbers.join(" | "),
            id = slice.id,
        ));
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for r in spec.requirements.iter().filter(|r| r.active()) {
        *counts.entry(report::state(spec, r)).or_default() += 1;
    }
    let total: usize = counts.values().sum();
    let summary: Vec<String> = STATES
        .iter()
        .map(|s| format!("{} {s}", counts.get(s).copied().unwrap_or_default()))
        .collect();
    let open = spec
        .open_items
        .iter()
        .filter(|i| i.status == ItemStatus::Open)
        .count();
    lines.extend([
        String::new(),
        format!(
            "All {total} requirements: {}. Open items with the status `open`: {open}.",
            summary.join(", ")
        ),
        String::new(),
        String::new(),
    ]);
    lines.join("\n")
}

/// The byte range between the marker lines of the status block of `readme`. The error gives
/// the line and the message of a missing, duplicated or unclosed marker.
fn block_range(readme: &str) -> Result<Range<usize>, (Option<usize>, String)> {
    let mut begins = Vec::new();
    let mut ends = Vec::new();
    let mut offset = 0;
    for (index, line) in readme.split_inclusive('\n').enumerate() {
        let content = line.trim_end_matches(['\n', '\r']);
        if content == STATUS_BEGIN {
            begins.push((index + 1, offset + line.len()));
        } else if content == STATUS_END {
            ends.push((index + 1, offset));
        }
        offset += line.len();
    }
    match (begins.as_slice(), ends.as_slice()) {
        ([], _) => Err((None, format!("no line `{STATUS_BEGIN}`"))),
        ([_, (line, _), ..], _) => Err((Some(*line), format!("`{STATUS_BEGIN}` is duplicated"))),
        (_, [_, (line, _), ..]) => Err((Some(*line), format!("`{STATUS_END}` is duplicated"))),
        ([(line, _)], []) => Err((
            Some(*line),
            format!("the status block is not closed: no line `{STATUS_END}`"),
        )),
        ([(line, start)], [(_, end)]) if end < start => Err((
            Some(*line),
            format!("`{STATUS_END}` is before `{STATUS_BEGIN}`"),
        )),
        ([(_, start)], [(_, end)]) => Ok(*start..*end),
    }
}

/// `readme` with a new status block. The text outside the block does not change.
///
/// # Errors
///
/// The message of a missing, duplicated or unclosed marker.
pub(super) fn with_status(readme: &str, spec: &Spec) -> Result<String, String> {
    let range = block_range(readme).map_err(|(_, message)| message)?;
    let mut out = readme.to_owned();
    out.replace_range(range, &status_block(spec));
    Ok(out)
}

/// The text of `README.md` in the loaded documents, if the file exists.
fn readme_text(spec: &Spec) -> Option<&str> {
    spec.documents
        .iter()
        .find(|d| d.path == Path::new(README))
        .map(|d| d.text.as_str())
}

/// REQ-056: the status block of `README.md` exists once and is up to date.
pub(super) fn block(spec: &Spec) -> Vec<Violation> {
    let file = Path::new(README);
    let Some(readme) = readme_text(spec) else {
        return vec![Violation::new(RULE, file, None, "the file does not exist")];
    };
    match block_range(readme) {
        Err((line, message)) => vec![Violation::new(RULE, file, line, message)],
        Ok(range)
            if readme
                .get(range.clone())
                .is_some_and(|b| b == status_block(spec)) =>
        {
            Vec::new()
        }
        Ok(_) => {
            let message = format!("the status block is not up to date: run `{COMMAND}`");
            vec![Violation::new(RULE, file, None, message)]
        }
    }
}

/// The targets of the rules of a `Makefile`: the names before `:` of the lines that do not
/// start with a space, a tab or `#`, and that are not variable assignments.
fn make_targets(makefile: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in makefile.lines() {
        if line.starts_with([' ', '\t', '#', '.']) {
            continue;
        }
        let Some((names, rest)) = line.split_once(':') else {
            continue;
        };
        if rest.starts_with('=') || names.contains('=') {
            continue;
        }
        out.extend(names.split_whitespace().map(str::to_owned));
    }
    out
}

/// The command-line options (`--name`) in `text`. The name starts with a letter, and the
/// character before `--` is not part of a word.
fn options(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (start, _) in text.match_indices("--") {
        let before = text.get(..start).and_then(|t| t.chars().next_back());
        if before.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        let rest = text.get(start + 2..).unwrap_or_default();
        if !rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .unwrap_or(rest.len());
        let name = rest.get(..end).unwrap_or_default().trim_end_matches('-');
        out.push(format!("--{name}"));
    }
    out
}

/// The help text of the frontend: the string of `const USAGE` in `src/cli.rs`.
fn usage(cli: &str) -> Option<&str> {
    let start = cli.find("const USAGE: &str = \"")?;
    let rest = cli.get(start..)?;
    let open = rest.find('"')? + 1;
    let body = rest.get(open..)?;
    body.get(..body.find("\";")?)
}

/// The part of a command whose options are options of the frontend. A command that starts
/// with `cargo` has options of other programs: only the arguments after ` -- ` of a
/// `cargo run` of the default binary are options of the frontend.
fn frontend_part(command: &str) -> &str {
    let command = command.trim();
    let mut words = command.split_whitespace();
    if words.next() != Some("cargo") {
        return command;
    }
    let other = ["-p", "--package", "--bin", "--example", "--manifest-path"];
    let default_run = words.next() == Some("run")
        && !command
            .split_whitespace()
            .take_while(|w| *w != "--")
            .any(|w| other.contains(&w));
    match command.split_once(" -- ") {
        Some((_, args)) if default_run => args,
        _ => "",
    }
}

/// The targets of each `make <target>` in a command.
fn make_target(command: &str) -> Vec<String> {
    let words: Vec<&str> = command.split_whitespace().collect();
    let mut out = Vec::new();
    for (index, word) in words.iter().enumerate() {
        if *word != "make" {
            continue;
        }
        let target = words
            .iter()
            .skip(index + 1)
            .find(|w| !w.starts_with('-') && !w.contains('='));
        if let Some(target) = target {
            out.push((*target).to_owned());
        }
    }
    out
}

/// A line of a fenced code block without its shell comment.
fn without_comment(line: &str) -> &str {
    let mut end = line.len();
    for (index, _) in line.match_indices('#') {
        let before = line.get(..index).and_then(|t| t.chars().next_back());
        if before.is_none_or(char::is_whitespace) {
            end = index;
            break;
        }
    }
    line.get(..end).unwrap_or_default()
}

/// The commands and the text of one README line in which options and `make` targets count.
struct Parts<'a> {
    /// The code spans, or the line of a code block.
    commands: Vec<&'a str>,
    /// The text outside the code spans.
    prose: String,
}

/// Splits one line of `README.md` into its commands and its prose.
fn parts(line: &str, in_fence: bool) -> Parts<'_> {
    if in_fence {
        Parts {
            commands: vec![without_comment(line)],
            prose: String::new(),
        }
    } else {
        Parts {
            commands: text::code_spans(line),
            prose: text::without_code(line),
        }
    }
}

/// REQ-056: the `make` targets and the command-line options of `README.md` exist.
///
/// Each `make <target>` in a code span or a code block names a target in `Makefile`, and
/// each command-line option (`--name`) is in the help text of the frontend (`src/cli.rs`).
/// Options in a command that starts with `cargo` are options of other programs, except the
/// arguments after ` -- ` of `cargo run`.
#[must_use]
pub fn readme(spec: &Spec, root: &Path) -> Vec<Violation> {
    let file = Path::new(README);
    let Some(readme) = readme_text(spec) else {
        return vec![Violation::new(RULE, file, None, "the file does not exist")];
    };
    let mut out = Vec::new();
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap_or_default();
    let targets = make_targets(&makefile);
    let cli = spec
        .documents
        .iter()
        .find(|d| d.path == Path::new("src/cli.rs"));
    let help = cli.and_then(|d| usage(&d.text));
    if help.is_none() {
        let message = "src/cli.rs has no help text (`const USAGE: &str`)";
        out.push(Violation::new(RULE, file, None, message));
    }
    let known: BTreeSet<String> = options(help.unwrap_or_default()).into_iter().collect();
    let mut in_fence = false;
    for (index, line) in readme.lines().enumerate() {
        let number = Some(index + 1);
        if text::is_fence(line) {
            in_fence = !in_fence;
            continue;
        }
        let parts = parts(line, in_fence);
        for command in &parts.commands {
            for target in make_target(command) {
                if !targets.contains(&target) {
                    let message =
                        format!("`make {target}`: `{target}` is not a target in Makefile");
                    out.push(Violation::new(RULE, file, number, message));
                }
            }
        }
        let texts = parts
            .commands
            .iter()
            .map(|c| frontend_part(c))
            .chain([parts.prose.as_str()]);
        for option in texts.flat_map(options) {
            if !known.contains(&option) {
                let message = format!("`{option}` is not an option in the help text of src/cli.rs");
                out.push(Violation::new(RULE, file, number, message));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makefile_targets_skip_assignments() {
        let targets = make_targets("A := 1\nB=2\n.PHONY: x\nx y: z\n\tcmd: no\n# c: no\n");
        let names: Vec<&str> = targets.iter().map(String::as_str).collect();
        assert_eq!(names, ["x", "y"]);
    }

    #[test]
    fn options_need_a_word_boundary_and_a_letter() {
        assert_eq!(
            options("a --pal-blending, x--no, --- | <!-- c --> `--9` --scale"),
            ["--pal-blending", "--scale"]
        );
    }

    #[test]
    fn cargo_commands_check_only_the_frontend_arguments() {
        assert_eq!(frontend_part("cargo test --all-targets"), "");
        assert_eq!(frontend_part("cargo run --release -- --pal"), "--pal");
        assert_eq!(frontend_part("cargo run -p x --bin y -- --z"), "");
        assert_eq!(frontend_part("demo --pal"), "demo --pal");
    }

    #[test]
    fn make_targets_and_comments_of_commands() {
        assert_eq!(make_target("make -j4 V=1 check && make"), ["check"]);
        assert_eq!(without_comment("make roms  # make x"), "make roms  ");
        assert_eq!(without_comment("a#b"), "a#b");
        assert_eq!(usage("x"), None);
    }
}
