//! The gates of `docs/design/spec-gates.md`. Each gate gives its violations; an empty list passes.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use super::{
    CodeTest, ItemStatus, OPEN_ITEM_INDEX, Outcome, Priority, REQUIREMENT_INDEX, Requirement,
    Section, SliceStatus, Spec, Verification, Violation, report, text,
};

/// The frontmatter problems of `rules` that [`super::load()`] found.
fn problems(spec: &Spec, rules: &[u16]) -> Vec<Violation> {
    spec.problems
        .iter()
        .filter(|p| rules.contains(&p.rule))
        .cloned()
        .collect()
}

/// `REQ-NNN` for a requirement number.
fn req(id: u16) -> String {
    format!("REQ-{id:03}")
}

/// REQ-043, REQ-053, REQ-054: the frontmatter keys and values of the requirements, open
/// items and slices, the `area` values, unique IDs, `depends_on` and its cycles.
#[must_use]
pub fn schema(spec: &Spec) -> Vec<Violation> {
    let mut out = problems(spec, &[43, 53, 54]);
    let mut seen: BTreeMap<&str, &Path> = BTreeMap::new();
    for r in &spec.requirements {
        if !spec.areas.contains(&r.area) {
            let message = format!("`area` `{}` is not an area of docs/spec/README.md", r.area);
            out.push(Violation::new(43, &r.path, None, message));
        }
        if let Some(first) = seen.insert(&r.declared_id, &r.path) {
            let message = format!(
                "`id` {} is also the ID of {}",
                r.declared_id,
                first.display()
            );
            out.push(Violation::new(43, &r.path, None, message));
        }
        out.extend(depends_on(spec, r));
    }
    out.extend(cycles(spec));
    out
}

/// REQ-043 criterion 4: each `depends_on` ID exists, is not withdrawn and is not the file's
/// own ID.
fn depends_on(spec: &Spec, r: &Requirement) -> Vec<Violation> {
    let mut out = Vec::new();
    for &id in &r.depends_on {
        let problem = match spec.requirement(id) {
            _ if id == r.id => Some("is the requirement itself"),
            None => Some("does not exist"),
            Some(other) if !other.active() => Some("is withdrawn"),
            Some(_) => None,
        };
        if let Some(problem) = problem {
            let message = format!("`depends_on` {} {problem}", req(id));
            out.push(Violation::new(43, &r.path, None, message));
        }
    }
    out
}

/// REQ-043 criterion 5: the `depends_on` relation has no cycle. Each requirement in a cycle
/// gives one violation.
fn cycles(spec: &Spec) -> Vec<Violation> {
    let graph: BTreeMap<u16, &[u16]> = spec
        .requirements
        .iter()
        .map(|r| (r.id, r.depends_on.as_slice()))
        .collect();
    let mut out = Vec::new();
    for r in &spec.requirements {
        if let Some(path) = path_back(&graph, r.id) {
            let chain: Vec<String> = path.iter().map(|&id| req(id)).collect();
            let message = format!("`depends_on` has a cycle: {}", chain.join(" -> "));
            out.push(Violation::new(43, &r.path, None, message));
        }
    }
    out
}

/// A path from `start` back to `start` in `graph`, if any (breadth first, so the shortest).
fn path_back(graph: &BTreeMap<u16, &[u16]>, start: u16) -> Option<Vec<u16>> {
    let mut parent: BTreeMap<u16, u16> = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        for &next in graph.get(&node).copied().unwrap_or_default() {
            if next == start {
                return Some(unwind(&parent, start, node));
            }
            if let Entry::Vacant(entry) = parent.entry(next) {
                let _node = entry.insert(node);
                queue.push_back(next);
            }
        }
    }
    None
}

/// The path `start -> ... -> node -> start` from the parent links of a search.
fn unwind(parent: &BTreeMap<u16, u16>, start: u16, node: u16) -> Vec<u16> {
    let mut path = vec![start];
    let mut at = node;
    while at != start {
        path.push(at);
        let Some(&up) = parent.get(&at) else {
            break;
        };
        at = up;
    }
    path.push(start);
    path.reverse();
    path
}

/// The section titles that a requirement must have, in this order (REQ-044).
const REQUIRED_SECTIONS: [&str; 3] = ["Requirement", "Rationale", "Acceptance criteria"];

/// REQ-044: the sections, the word `shall` and the numbered acceptance criteria of each
/// requirement that is not withdrawn.
#[must_use]
pub fn structure(spec: &Spec) -> Vec<Violation> {
    let mut out = Vec::new();
    for r in spec.requirements.iter().filter(|r| r.active()) {
        for s in &r.sections {
            if !REQUIRED_SECTIONS.contains(&s.title.as_str()) && s.title != "Notes" {
                let message = format!("the section heading `## {}` is not permitted", s.title);
                out.push(Violation::new(44, &r.path, Some(s.line), message));
            }
        }
        let positions: Vec<Option<usize>> = REQUIRED_SECTIONS
            .iter()
            .map(|t| r.sections.iter().position(|s| s.title == *t))
            .collect();
        for (title, position) in REQUIRED_SECTIONS.iter().zip(&positions) {
            if position.is_none() {
                let message = format!("the section `## {title}` is missing");
                out.push(Violation::new(44, &r.path, None, message));
            }
        }
        let found: Vec<usize> = positions.iter().flatten().copied().collect();
        if found.windows(2).any(|w| w.first() > w.get(1)) {
            let message = "the sections `Requirement`, `Rationale` and `Acceptance criteria` \
                           are not in this order";
            out.push(Violation::new(44, &r.path, None, message));
        }
        out.extend(statement_and_criteria(r));
    }
    out
}

/// REQ-044 criteria 2 and 3: `shall` in the statement, and a criterion `1. `.
fn statement_and_criteria(r: &Requirement) -> Vec<Violation> {
    let mut out = Vec::new();
    if let Some(s) = r.section("Requirement")
        && !s.lines.iter().any(|(_, l)| text::contains_word(l, "shall"))
    {
        let message = "the requirement statement has no `shall`";
        out.push(Violation::new(44, &r.path, Some(s.line), message));
    }
    if let Some(s) = r.section("Acceptance criteria")
        && !s.lines.iter().any(|(_, l)| l.starts_with("1. "))
    {
        let message = "`Acceptance criteria` has no line that starts with `1. `";
        out.push(Violation::new(44, &r.path, Some(s.line), message));
    }
    out
}

/// REQ-045: the requirement statement and the acceptance criteria contain no word of
/// `docs/spec/vague-words.txt`. Text in backticks is not checked.
#[must_use]
pub fn vague_words(spec: &Spec) -> Vec<Violation> {
    let mut out = Vec::new();
    for r in spec.requirements.iter().filter(|r| r.active()) {
        let mut seen = BTreeSet::new();
        for (title, place) in [
            ("Requirement", "requirement statement"),
            ("Acceptance criteria", "criterion"),
        ] {
            if let Some(section) = r.section(title) {
                out.extend(vague_in(spec, r, section, place, &mut seen));
            }
        }
    }
    out
}

/// The vague words in one section. `place` names the section in the message; for the
/// criteria, the message also gives the criterion number.
fn vague_in(
    spec: &Spec,
    r: &Requirement,
    section: &Section,
    place: &str,
    seen: &mut BTreeSet<String>,
) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut criterion = None;
    for (line, content) in &section.lines {
        criterion = text::list_number(content).or(criterion);
        let plain = text::without_code(content);
        let at = criterion
            .filter(|_| place == "criterion")
            .map_or_else(|| place.to_owned(), |n| format!("{place} {n}"));
        for word in spec
            .vague_words
            .iter()
            .filter(|w| text::contains_word(&plain, w))
        {
            let message = format!("{at}: vague word `{word}`");
            if seen.insert(message.clone()) {
                out.push(Violation::new(45, &r.path, Some(*line), message));
            }
        }
    }
    out
}

/// The HRM section numbers in a source, for example `3.3` for `HRM 3.3 Cycle timing`.
fn hrm_sections(source: &str) -> Vec<String> {
    source
        .match_indices("HRM ")
        .filter_map(|(start, _)| {
            let rest = source.get(start + 4..)?;
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '.')
                .unwrap_or(rest.len());
            let token = rest.get(..end)?.trim_end_matches('.');
            let starts_ok = token.chars().next().is_some_and(|c| c.is_ascii_digit())
                || token.chars().next().is_some_and(|c| c.is_ascii_uppercase());
            (starts_ok && token.contains('.')).then(|| token.to_owned())
        })
        .collect()
}

/// REQ-046: each `HRM n.m` of a source is in `docs/spec/hrm-sections.txt`, a source without
/// an HRM section is `Project decision`, and a project decision has a `Notes` section.
#[must_use]
pub fn sources(spec: &Spec) -> Vec<Violation> {
    let mut out = Vec::new();
    for r in &spec.requirements {
        let cited = hrm_sections(&r.source);
        for section in &cited {
            if !spec.hrm_sections.contains(section) {
                let message = format!(
                    "`source` cites HRM {section}, which is not in docs/spec/hrm-sections.txt"
                );
                out.push(Violation::new(46, &r.path, None, message));
            }
        }
        if cited.is_empty() && r.source != "Project decision" {
            let message = format!(
                "`source` `{}` names no HRM section and is not `Project decision`",
                r.source
            );
            out.push(Violation::new(46, &r.path, None, message));
        }
        let decision = r
            .source
            .split(';')
            .any(|part| part.trim().to_lowercase().starts_with("project decision"));
        if decision && r.section("Notes").is_none() {
            let message = "`source` is a project decision, but there is no `Notes` section";
            out.push(Violation::new(46, &r.path, None, message));
        }
    }
    out
}

/// The IDs of the files that the references can name: `REQ-NNN`, `OI-NNN` and `SLICE-NN`.
fn known_ids(spec: &Spec) -> BTreeSet<String> {
    let requirements = spec.requirements.iter().map(|r| req(r.id));
    let items = spec.open_items.iter().map(|i| format!("OI-{:03}", i.id));
    let slices = spec.slices.iter().map(|s| format!("SLICE-{:02}", s.id));
    requirements.chain(items).chain(slices).collect()
}

/// True if a link target is outside the repository or in the same file.
fn is_external(target: &str) -> bool {
    target.starts_with('#') || target.contains("://") || target.starts_with("mailto:")
}

/// The context of the `references` gate.
struct Refs<'a> {
    /// The repository root.
    root: &'a Path,
    /// The IDs that name a file.
    known: BTreeSet<String>,
    /// The IDs of the withdrawn requirements.
    withdrawn: BTreeSet<String>,
}

impl Refs<'_> {
    /// The broken ID references of one line. `record` is true in a requirement or an open
    /// item, where a withdrawn requirement can be named.
    fn ids(&self, doc: &Path, number: usize, line: &str, record: bool) -> Vec<Violation> {
        let lower = line.to_lowercase();
        let excused = record || lower.contains("withdrawn") || lower.contains("merged");
        let mut out = Vec::new();
        for prefix in ["REQ-", "OI-", "SLICE-"] {
            for (_, token) in text::references(line, prefix) {
                let message = if !self.known.contains(token) {
                    format!("`{token}` names no file")
                } else if self.withdrawn.contains(token) && !excused {
                    format!("`{token}` is withdrawn")
                } else {
                    continue;
                };
                out.push(Violation::new(47, doc, Some(number), message));
            }
        }
        out
    }

    /// The broken relative links of one markdown line.
    fn links(&self, doc: &Path, number: usize, line: &str) -> Vec<Violation> {
        text::links(line)
            .into_iter()
            .filter(|target| !is_external(target) && !link_exists(self.root, doc, target))
            .map(|target| {
                let message = format!("the link `{target}` points to no file");
                Violation::new(47, doc, Some(number), message)
            })
            .collect()
    }
}

/// REQ-047: the references to requirements, open items, slices and files resolve.
///
/// Each `REQ-NNN`, `OI-NNN` and `SLICE-NN` in `docs/` and in `.rs` files names an existing
/// file, a withdrawn requirement is named only where permitted, and each relative markdown
/// link in `docs/` and in `README.md` points to an existing file.
#[must_use]
pub fn references(spec: &Spec, root: &Path) -> Vec<Violation> {
    let refs = Refs {
        root,
        known: known_ids(spec),
        withdrawn: spec
            .requirements
            .iter()
            .filter(|r| !r.active())
            .map(|r| req(r.id))
            .collect(),
    };
    let mut out = Vec::new();
    for doc in &spec.documents {
        let record = doc.path.starts_with("docs/spec/requirements")
            || doc.path.starts_with("docs/spec/open-items");
        let markdown = doc.path.extension().is_some_and(|e| e == "md");
        let mut in_fence = false;
        for (index, line) in doc.text.lines().enumerate() {
            out.extend(refs.ids(&doc.path, index + 1, line, record));
            in_fence ^= markdown && text::is_fence(line);
            if markdown && !in_fence {
                out.extend(refs.links(&doc.path, index + 1, line));
            }
        }
    }
    out
}

/// True if the relative link `target` in the file `from` points to an existing file.
fn link_exists(root: &Path, from: &Path, target: &str) -> bool {
    let path = target.split('#').next().unwrap_or_default();
    let base = from.parent().unwrap_or_else(|| Path::new(""));
    let full = path
        .strip_prefix('/')
        .map_or_else(|| root.join(base).join(path), |p| root.join(p));
    full.exists()
}

/// REQ-048: each requirement that is not withdrawn is in the test plan of exactly one
/// design document, with a test (`test`) or an evidence record (the other methods).
#[must_use]
pub fn test_plans(spec: &Spec) -> Vec<Violation> {
    let mut out = Vec::new();
    for r in spec.requirements.iter().filter(|r| r.active()) {
        let rows: Vec<_> = spec.rows(r.id).collect();
        let documents: BTreeSet<&PathBuf> = rows.iter().map(|row| &row.document).collect();
        let id = req(r.id);
        if documents.len() != 1 {
            let names: Vec<String> = documents.iter().map(|d| d.display().to_string()).collect();
            let message = if names.is_empty() {
                format!("{id} is in no design test plan")
            } else {
                format!("{id} is in the test plans of {}", names.join(", "))
            };
            out.push(Violation::new(48, &r.path, None, message));
            if names.is_empty() {
                continue;
            }
        }
        let Some(first) = rows.first() else {
            continue;
        };
        let evidence = PathBuf::from(format!("docs/verification/{id}.md"));
        let message = if r.verification == Verification::Test {
            (!rows.iter().any(|row| !row.tests.is_empty()))
                .then(|| format!("the test plan row of {id} names no test in backticks"))
        } else {
            (!rows
                .iter()
                .any(|row| row.evidence.as_ref() == Some(&evidence)))
            .then(|| {
                format!(
                    "the test plan row of {id} ({}) does not name {}",
                    r.verification.as_str(),
                    evidence.display()
                )
            })
        };
        if let Some(message) = message {
            out.push(Violation::new(
                48,
                &first.document,
                Some(first.line),
                message,
            ));
        }
    }
    out
}

/// REQ-027 and REQ-049: the trace from the test plans to the code, and back.
///
/// The `Verifies` comments name valid requirements and are on tests, each test that names a
/// requirement is in its test plan, and an ignored test is a planned stub of a package that
/// is not `done`. Each `test` requirement has a test, ignored or not, and each test of a
/// test plan exists with a `Verifies` comment for its requirement. For a `done` package,
/// each `test` requirement has a test that runs.
#[must_use]
pub fn trace(spec: &Spec) -> Vec<Violation> {
    let mut out = Vec::new();
    for test in spec.code_tests.iter().filter(|t| !t.verifies.is_empty()) {
        out.extend(trace_comment(spec, test));
    }
    for test in spec.code_tests.iter().filter(|t| t.is_test && t.ignored) {
        out.extend(trace_ignored(spec, test));
    }
    for r in spec.requirements.iter().filter(|r| r.active()) {
        out.extend(trace_requirement(spec, r));
    }
    out
}

/// The rules of REQ-027 and REQ-049 criterion 3 for one function with a `Verifies` comment.
fn trace_comment(spec: &Spec, test: &CodeTest) -> Vec<Violation> {
    let at = Some(test.line);
    let name = &test.function;
    if !test.is_test {
        let message = format!("`{name}`: a `Verifies` comment is not on a #[test] function");
        return vec![Violation::new(27, &test.file, at, message)];
    }
    let mut out = Vec::new();
    for &id in &test.verifies {
        let id_text = req(id);
        match spec.requirement(id) {
            None => {
                let message = format!("`{name}` names {id_text}, which does not exist");
                out.push(Violation::new(27, &test.file, at, message));
            }
            Some(r) if !r.active() => {
                let message = format!("`{name}` names {id_text}, which is withdrawn");
                out.push(Violation::new(27, &test.file, at, message));
            }
            Some(_) => {
                if !spec
                    .rows(id)
                    .any(|row| row.tests.iter().any(|t| t.matches(test)))
                {
                    let message = format!(
                        "`{name}` verifies {id_text}, but the test plan of {id_text} does not \
                         name it"
                    );
                    out.push(Violation::new(49, &test.file, at, message));
                }
            }
        }
    }
    out
}

/// The slice number of an ignore reason `planned: SLICE-NN`, if the reason has exactly this
/// form.
fn planned_slice(reason: &str) -> Option<u16> {
    let number = reason.strip_prefix("planned: SLICE-")?;
    if number.len() != 2 || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

/// REQ-027 criterion 5 for one ignored test: the reason is `planned: SLICE-NN`, the slice
/// exists and plans each requirement of the test, and no package of these requirements is
/// `done`.
fn trace_ignored(spec: &Spec, test: &CodeTest) -> Vec<Violation> {
    let at = Some(test.line);
    let name = &test.function;
    let reason = test.ignore_reason.as_deref();
    let Some(number) = reason.and_then(planned_slice) else {
        let message = reason.map_or_else(
            || format!("`{name}`: the test is ignored without the reason `planned: SLICE-NN`"),
            |reason| format!("`{name}`: the ignore reason is `{reason}`, not `planned: SLICE-NN`"),
        );
        return vec![Violation::new(27, &test.file, at, message)];
    };
    let slice_id = format!("SLICE-{number:02}");
    if !spec.slices.iter().any(|s| s.id == number) {
        let message = format!("`{name}`: the ignore reason names {slice_id}, which does not exist");
        return vec![Violation::new(27, &test.file, at, message)];
    }
    let mut out = Vec::new();
    for &id in &test.verifies {
        let id_text = req(id);
        let message = match spec.planned(id) {
            Some((slice, _)) if slice.id != number => format!(
                "`{name}` is planned in {slice_id}, but SLICE-{:02} plans {id_text}",
                slice.id
            ),
            None => format!("`{name}` is planned in {slice_id}, but no slice plans {id_text}"),
            Some(_) if spec.done(id) => {
                format!("`{name}` is ignored, but the package of {id_text} is done")
            }
            Some(_) => continue,
        };
        out.push(Violation::new(27, &test.file, at, message));
    }
    out
}

/// The rules of REQ-027 criteria 1 and 2 and REQ-049 criteria 1 and 2 for one requirement
/// that is not withdrawn.
fn trace_requirement(spec: &Spec, r: &Requirement) -> Vec<Violation> {
    let id = req(r.id);
    let mut out = Vec::new();
    if r.verification == Verification::Test {
        if spec.tests_of(r.id).next().is_none() {
            let message = format!("{id}: no test names {id}, ignored or not");
            out.push(Violation::new(27, &r.path, None, message));
        } else if spec.done(r.id) && !spec.tests_of(r.id).any(|t| !t.ignored) {
            let message = format!("{id}: the package is done, but no test that runs names {id}");
            out.push(Violation::new(27, &r.path, None, message));
        }
    }
    for row in spec.rows(r.id) {
        for test_ref in &row.tests {
            let found: Vec<&CodeTest> = spec
                .code_tests
                .iter()
                .filter(|t| t.is_test && test_ref.matches(t))
                .collect();
            let file = test_ref.file.as_deref().unwrap_or("any file");
            let function = &test_ref.function;
            let message = if found.is_empty() {
                format!(
                    "{id}: the test plan names `{function}` in {file}, which is not a #[test] function"
                )
            } else if !found.iter().any(|t| t.verifies.contains(&r.id)) {
                format!("{id}: the test `{function}` in {file} has no `Verifies` comment for {id}")
            } else {
                continue;
            };
            out.push(Violation::new(49, &row.document, Some(row.line), message));
        }
    }
    out
}

/// REQ-050 and REQ-054: the slices and their packages.
///
/// Each requirement that is not withdrawn is in a package, each planned requirement exists
/// and is in at most one package, a `done` slice has only `done` packages, and
/// `product_complete: true` needs each `must` requirement in a `done` package.
#[must_use]
pub fn slices(spec: &Spec) -> Vec<Violation> {
    let mut out = problems(spec, &[50]);
    let mut owners: BTreeMap<u16, Vec<String>> = BTreeMap::new();
    for slice in &spec.slices {
        for package in &slice.packages {
            let owner = format!("SLICE-{:02} {}", slice.id, package.id);
            for &id in &package.requirements {
                if spec.requirement(id).is_none() {
                    let message = format!("{owner} names {}, which does not exist", req(id));
                    out.push(Violation::new(54, &slice.path, None, message));
                }
                owners.entry(id).or_default().push(owner.clone());
            }
            if slice.status == SliceStatus::Done && package.status != SliceStatus::Done {
                let message = format!(
                    "the slice is done, but {} is `{}`",
                    package.id,
                    package.status.as_str()
                );
                out.push(Violation::new(54, &slice.path, None, message));
            }
        }
    }
    for (id, list) in owners.iter().filter(|(_, list)| list.len() > 1) {
        let path = spec
            .slices
            .iter()
            .find(|s| s.packages.iter().any(|p| p.requirements.contains(id)))
            .map_or_else(PathBuf::new, |s| s.path.clone());
        let message = format!(
            "{} is in more than one package: {}",
            req(*id),
            list.join(", ")
        );
        out.push(Violation::new(54, &path, None, message));
    }
    for r in spec.requirements.iter().filter(|r| r.active()) {
        if spec.planned(r.id).is_none() {
            let message = format!("{} is in no slice", req(r.id));
            out.push(Violation::new(50, &r.path, None, message));
        }
    }
    if spec.product_complete {
        for r in spec
            .requirements
            .iter()
            .filter(|r| r.active() && r.priority == Priority::Must && !spec.done(r.id))
        {
            let place = spec.planned(r.id).map_or_else(
                || "is in no slice".to_owned(),
                |(slice, package)| {
                    format!(
                        "is in SLICE-{:02} {}, which is not done",
                        slice.id, package.id
                    )
                },
            );
            let message = format!(
                "`product_complete` is true, but {} (`must`) {place}",
                req(r.id)
            );
            out.push(Violation::new(
                50,
                Path::new("docs/plan/README.md"),
                None,
                message,
            ));
        }
    }
    out
}

/// REQ-051: the evidence records are valid, and each `inspection` or `demonstration`
/// requirement of a `done` package has a record with `result: pass`.
#[must_use]
pub fn evidence(spec: &Spec) -> Vec<Violation> {
    let mut out = problems(spec, &[51]);
    for record in &spec.evidence {
        let Some(r) = record.requirement.and_then(|id| spec.requirement(id)) else {
            continue;
        };
        if record.method != r.verification {
            let message = format!(
                "`method` is `{}`, but {} has the verification `{}`",
                record.method.as_str(),
                req(r.id),
                r.verification.as_str()
            );
            out.push(Violation::new(51, &record.path, None, message));
        }
        if record.subsections < r.criteria() {
            let message = format!(
                "the body has {} subsections, but {} has {} acceptance criteria",
                record.subsections,
                req(r.id),
                r.criteria()
            );
            out.push(Violation::new(51, &record.path, None, message));
        }
    }
    for r in spec
        .requirements
        .iter()
        .filter(|r| r.active() && r.verification != Verification::Test && spec.done(r.id))
    {
        let id = req(r.id);
        let record = spec.evidence.iter().find(|e| e.requirement == Some(r.id));
        let message = match record {
            None => {
                format!("{id}: the package is done, but docs/verification/{id}.md does not exist")
            }
            Some(e) if e.result == Outcome::Fail => {
                format!("{id}: the package is done, but the evidence record has `result: fail`")
            }
            Some(_) => continue,
        };
        out.push(Violation::new(51, &r.path, None, message));
    }
    out
}

/// REQ-053: the `affects` values exist, and the `Decision` section agrees with the status.
#[must_use]
pub fn open_items(spec: &Spec, root: &Path) -> Vec<Violation> {
    let known = known_ids(spec);
    let mut out = Vec::new();
    for item in &spec.open_items {
        for target in &item.affects {
            let is_id = target.starts_with("REQ-") || target.starts_with("OI-");
            let exists = if is_id {
                known.contains(target)
            } else {
                root.join(target.split('#').next().unwrap_or_default())
                    .exists()
            };
            if !exists {
                let message = format!("`affects` item `{target}` does not exist");
                out.push(Violation::new(53, &item.path, None, message));
            }
        }
        let text = item.decision.as_deref().unwrap_or_default();
        let decided = !text.is_empty() && text != "Not decided.";
        let message = match item.status {
            ItemStatus::Decided if !decided => {
                "the status is `decided`, but the `Decision` section has no decision"
            }
            ItemStatus::Open if decided => {
                "the status is `open`, but the `Decision` section has a decision"
            }
            ItemStatus::Decided | ItemStatus::Open => continue,
        };
        out.push(Violation::new(53, &item.path, None, message));
    }
    out
}

/// REQ-053: the generated indexes are up to date.
#[must_use]
pub fn indexes(spec: &Spec, root: &Path) -> Vec<Violation> {
    let mut out = Vec::new();
    for (path, expected) in [
        (REQUIREMENT_INDEX, report::requirement_index(spec)),
        (OPEN_ITEM_INDEX, report::open_item_index(spec)),
    ] {
        let actual = fs::read_to_string(root.join(path)).unwrap_or_default();
        if actual != expected {
            let message = "the file is not up to date: run `cargo run -p atari-harness --bin \
                           spec-index`";
            out.push(Violation::new(53, Path::new(path), None, message));
        }
    }
    out
}
