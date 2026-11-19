//! Reads the files of the repository into a [`Spec`].

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use yaml_rust2::{Yaml, YamlLoader};

use super::{
    Document, Evidence, ItemStatus, OpenItem, Outcome, Package, Priority, Requirement, SliceStatus,
    Spec, SpecError, Status, TestPlanRow, TestRef, Verification, Violation, code, text,
};
use crate::spec::Slice;

/// The keys of a requirement frontmatter (REQ-043).
const REQUIREMENT_KEYS: &[&str] = &[
    "id",
    "title",
    "status",
    "priority",
    "area",
    "source",
    "verification",
    "depends_on",
];

/// The keys of an open item frontmatter (REQ-053). `raised` is optional.
const OPEN_ITEM_KEYS: &[&str] = &["id", "title", "status", "area", "affects"];

/// The keys of a slice frontmatter (REQ-054). `depends_on` is optional.
const SLICE_KEYS: &[&str] = &["id", "title", "status", "packages"];

/// The keys of a work package (REQ-054). `title` is optional.
const PACKAGE_KEYS: &[&str] = &["id", "status", "requirements"];

/// The keys of an evidence record (REQ-051).
const EVIDENCE_KEYS: &[&str] = &["requirement", "date", "commit", "method", "result", "by"];

/// Reads a text file.
pub(super) fn read(path: &Path) -> Result<String, SpecError> {
    fs::read_to_string(path).map_err(|error| SpecError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

/// The files in `dir` whose names start with `prefix` and end with `.md`, sorted. A missing
/// directory gives no files.
fn markdown_files(dir: &Path, prefix: &str) -> Result<Vec<PathBuf>, SpecError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(dir).map_err(|error| SpecError::Io {
        path: dir.to_path_buf(),
        message: error.to_string(),
    })?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix))
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        })
        .collect();
    files.sort();
    Ok(files)
}

/// All `.md` files below `dir`, recursively, sorted.
fn markdown_tree(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SpecError> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|error| SpecError::Io {
            path: dir.to_path_buf(),
            message: error.to_string(),
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            markdown_tree(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    Ok(())
}

/// The file stem, for example `REQ-001`.
fn stem(path: &Path) -> &str {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
}

/// A path relative to `root`.
fn relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

/// A file split into its frontmatter and its body.
struct Parts<'a> {
    /// The YAML text between the `---` lines, if the file has frontmatter.
    yaml: Option<&'a str>,
    /// The body lines with their line numbers (1-based).
    body: Vec<(usize, &'a str)>,
}

/// Splits `text` into the frontmatter and the body.
fn split(text: &str) -> Parts<'_> {
    let lines: Vec<&str> = text.lines().collect();
    let end = (lines.first() == Some(&"---"))
        .then(|| lines.iter().skip(1).position(|l| *l == "---"))
        .flatten()
        .map(|i| i + 1);
    end.map_or_else(
        || Parts {
            yaml: None,
            body: lines
                .iter()
                .copied()
                .enumerate()
                .map(|(i, l)| (i + 1, l))
                .collect(),
        },
        |end| {
            let start = text.find('\n').map_or(text.len(), |i| i + 1);
            let stop = lines
                .iter()
                .take(end)
                .map(|l| l.len() + 1)
                .sum::<usize>()
                .min(text.len());
            Parts {
                yaml: text.get(start..stop),
                body: lines
                    .iter()
                    .copied()
                    .enumerate()
                    .skip(end + 1)
                    .map(|(i, l)| (i + 1, l))
                    .collect(),
            }
        },
    )
}

/// The frontmatter of one file and the problems found in it.
struct Front<'a> {
    /// The file, relative to the repository root.
    path: &'a Path,
    /// The rule that the frontmatter belongs to.
    rule: u16,
    /// The YAML text, for the line numbers.
    yaml: &'a str,
    /// The top-level mapping.
    map: Vec<(String, Yaml)>,
    /// The problems.
    problems: Vec<Violation>,
}

impl<'a> Front<'a> {
    /// Parses the frontmatter of `parts`. No frontmatter is a problem and an empty map.
    fn parse(path: &'a Path, rule: u16, parts: &Parts<'a>) -> Result<Self, SpecError> {
        let mut front = Self {
            path,
            rule,
            yaml: parts.yaml.unwrap_or_default(),
            map: Vec::new(),
            problems: Vec::new(),
        };
        let Some(yaml) = parts.yaml else {
            front.problem(None, "the file has no YAML frontmatter");
            return Ok(front);
        };
        let docs = YamlLoader::load_from_str(yaml).map_err(|error| SpecError::Yaml {
            path: path.to_path_buf(),
            line: error.marker().line() + 1,
            message: error.to_string(),
        })?;
        match docs.into_iter().next() {
            Some(Yaml::Hash(hash)) => {
                for (key, value) in hash {
                    match key {
                        Yaml::String(key) => front.map.push((key, value)),
                        _ => front.problem(Some(2), "a frontmatter key is not a string"),
                    }
                }
            }
            _ => front.problem(Some(2), "the frontmatter is not a mapping"),
        }
        Ok(front)
    }

    /// Adds a problem.
    fn problem(&mut self, line: Option<usize>, message: impl Into<String>) {
        self.problems
            .push(Violation::new(self.rule, self.path, line, message));
    }

    /// The line of `key` in the file.
    fn line_of(&self, key: &str) -> Option<usize> {
        let prefix = format!("{key}:");
        self.yaml
            .lines()
            .position(|l| l.trim_start().trim_start_matches("- ").starts_with(&prefix))
            .map(|i| i + 2)
    }

    /// Checks that the keys are exactly `required` plus some of `optional`.
    fn keys(&mut self, required: &[&str], optional: &[&str]) {
        let found: Vec<String> = self.map.iter().map(|(k, _)| k.clone()).collect();
        for key in required {
            if !found.iter().any(|k| k == key) {
                self.problem(Some(2), format!("missing key `{key}`"));
            }
        }
        for key in &found {
            if !required.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
                let line = self.line_of(key);
                self.problem(line, format!("unknown key `{key}`"));
            }
        }
    }

    /// The value of `key`.
    fn value(&self, key: &str) -> Option<&Yaml> {
        self.map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// The string value of `key`. Another value type is a problem.
    fn string(&mut self, key: &str) -> String {
        match self.value(key).map(scalar) {
            Some(Some(text)) => text,
            Some(None) => {
                let line = self.line_of(key);
                self.problem(line, format!("`{key}` is not a string"));
                String::new()
            }
            None => String::new(),
        }
    }

    /// The list value of `key`, as strings. Another value type is a problem.
    fn list(&mut self, key: &str) -> Vec<String> {
        match self.value(key) {
            Some(Yaml::Array(items)) => {
                let items: Vec<Option<String>> = items.iter().map(scalar).collect();
                if items.iter().any(Option::is_none) {
                    let line = self.line_of(key);
                    self.problem(line, format!("an item of `{key}` is not a string"));
                }
                items.into_iter().flatten().collect()
            }
            Some(_) => {
                let line = self.line_of(key);
                self.problem(line, format!("`{key}` is not a list"));
                Vec::new()
            }
            None => Vec::new(),
        }
    }

    /// The keyword value of `key`, parsed with `parse`. A bad value is a problem.
    fn keyword<T: Default>(
        &mut self,
        key: &str,
        values: &[&str],
        parse: fn(&str) -> Option<T>,
    ) -> T {
        let text = self.string(key);
        if self.value(key).is_none() {
            return T::default();
        }
        parse(&text).unwrap_or_else(|| {
            let line = self.line_of(key);
            let allowed = values.join(", ");
            self.problem(line, format!("`{key}` is `{text}`, not one of: {allowed}"));
            T::default()
        })
    }

    /// Requirement numbers from the list value of `key`. A bad ID is a problem.
    fn requirement_list(&mut self, key: &str) -> Vec<u16> {
        let items = self.list(key);
        let mut out = Vec::new();
        for item in items {
            if let Some(id) = text::parse_id(&item, "REQ-", 3) {
                out.push(id);
            } else {
                let line = self.line_of(key);
                self.problem(
                    line,
                    format!("`{key}` item `{item}` is not a requirement ID"),
                );
            }
        }
        out
    }

    /// Checks that the `id` value is the file stem.
    fn id(&mut self) -> String {
        let id = self.string("id");
        let expected = stem(self.path).to_owned();
        if self.value("id").is_some() && id != expected {
            let line = self.line_of("id");
            self.problem(
                line,
                format!("`id` is `{id}`, but the file is {expected}.md"),
            );
        }
        id
    }
}

/// A YAML scalar as a string. Numbers and booleans give their text.
fn scalar(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// The heading text after `# {id}: ` on the first body line.
fn heading(parts: &Parts<'_>, id: &str) -> String {
    parts
        .body
        .first()
        .and_then(|(_, line)| line.strip_prefix(&format!("# {id}: ")))
        .unwrap_or_default()
        .to_owned()
}

/// Reads one requirement file.
fn requirement(root: &Path, path: &Path) -> Result<(Requirement, Vec<Violation>), SpecError> {
    let text = read(path)?;
    let rel = relative(root, path);
    let parts = split(&text);
    let mut front = Front::parse(&rel, 43, &parts)?;
    front.keys(REQUIREMENT_KEYS, &[]);
    let declared_id = front.id();
    let requirement = Requirement {
        id: text::parse_id(stem(path), "REQ-", 3).unwrap_or_default(),
        title: front.string("title"),
        heading: heading(&parts, stem(path)),
        status: front.keyword("status", Status::VALUES, Status::parse),
        priority: front.keyword("priority", Priority::VALUES, Priority::parse),
        area: front.string("area"),
        source: front.string("source"),
        verification: front.keyword("verification", Verification::VALUES, Verification::parse),
        depends_on: front.requirement_list("depends_on"),
        sections: text::sections(&parts.body),
        path: rel.clone(),
        declared_id,
    };
    let mut problems = front.problems;
    if requirement.title != requirement.heading {
        problems.push(Violation::new(
            43,
            &rel,
            Some(2),
            "`title` is not the text of the first heading",
        ));
    }
    Ok((requirement, problems))
}

/// Reads one open item file.
fn open_item(root: &Path, path: &Path) -> Result<(OpenItem, Vec<Violation>), SpecError> {
    let text = read(path)?;
    let rel = relative(root, path);
    let parts = split(&text);
    let mut front = Front::parse(&rel, 53, &parts)?;
    front.keys(OPEN_ITEM_KEYS, &["raised"]);
    let _id = front.id();
    let sections = text::sections(&parts.body);
    let decision = sections.iter().find(|s| s.title == "Decision").map(|s| {
        s.lines
            .iter()
            .map(|(_, l)| l.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned()
    });
    let item = OpenItem {
        id: text::parse_id(stem(path), "OI-", 3).unwrap_or_default(),
        title: front.string("title"),
        status: front.keyword("status", ItemStatus::VALUES, ItemStatus::parse),
        area: front.list("area"),
        affects: front.list("affects"),
        decision,
        path: rel.clone(),
    };
    Ok((item, front.problems))
}

/// Reads the packages of a slice frontmatter.
fn packages(front: &mut Front<'_>) -> Vec<Package> {
    let items = match front.value("packages") {
        Some(Yaml::Array(items)) => items.clone(),
        Some(_) => {
            let line = front.line_of("packages");
            front.problem(line, "`packages` is not a list");
            Vec::new()
        }
        None => Vec::new(),
    };
    let mut out = Vec::new();
    for item in items {
        let Yaml::Hash(hash) = item else {
            let line = front.line_of("packages");
            front.problem(line, "a package is not a mapping");
            continue;
        };
        let mut package = Front {
            path: front.path,
            rule: front.rule,
            yaml: front.yaml,
            map: hash
                .into_iter()
                .filter_map(|(k, v)| scalar(&k).map(|k| (k, v)))
                .collect(),
            problems: Vec::new(),
        };
        package.keys(PACKAGE_KEYS, &["title"]);
        out.push(Package {
            id: package.string("id"),
            status: package.keyword("status", SliceStatus::VALUES, SliceStatus::parse),
            requirements: package.requirement_list("requirements"),
        });
        front.problems.extend(package.problems);
    }
    out
}

/// Reads one slice file.
fn slice(root: &Path, path: &Path) -> Result<(Slice, Vec<Violation>), SpecError> {
    let text = read(path)?;
    let rel = relative(root, path);
    let parts = split(&text);
    let mut front = Front::parse(&rel, 54, &parts)?;
    front.keys(SLICE_KEYS, &["depends_on"]);
    let _id = front.id();
    let slice = Slice {
        id: text::parse_id(stem(path), "SLICE-", 2).unwrap_or_default(),
        title: front.string("title"),
        status: front.keyword("status", SliceStatus::VALUES, SliceStatus::parse),
        packages: packages(&mut front),
        path: rel.clone(),
    };
    Ok((slice, front.problems))
}

/// Reads `product_complete` from `docs/plan/README.md` (REQ-050).
fn product_complete(root: &Path) -> Result<(bool, Vec<Violation>), SpecError> {
    let rel = PathBuf::from("docs/plan/README.md");
    let text = read(&root.join(&rel))?;
    let parts = split(&text);
    let mut front = Front::parse(&rel, 50, &parts)?;
    front.keys(&["product_complete"], &[]);
    let complete = match front.value("product_complete") {
        Some(Yaml::Boolean(b)) => *b,
        Some(_) => {
            let line = front.line_of("product_complete");
            front.problem(line, "`product_complete` is not `true` or `false`");
            false
        }
        None => false,
    };
    Ok((complete, front.problems))
}

/// True if `date` has the form `YYYY-MM-DD`.
fn is_iso_date(date: &str) -> bool {
    let parts: Vec<&str> = date.split('-').collect();
    matches!(parts.as_slice(), [y, m, d]
        if y.len() == 4 && m.len() == 2 && d.len() == 2
            && [y, m, d].iter().all(|p| text::digits(p) == **p))
}

/// Reads one evidence record.
fn evidence(root: &Path, path: &Path) -> Result<(Evidence, Vec<Violation>), SpecError> {
    let text = read(path)?;
    let rel = relative(root, path);
    let parts = split(&text);
    let mut front = Front::parse(&rel, 51, &parts)?;
    front.keys(EVIDENCE_KEYS, &[]);
    let requirement = front.string("requirement");
    let date = front.string("date");
    if !is_iso_date(&date) {
        let line = front.line_of("date");
        front.problem(line, format!("`date` `{date}` is not an ISO 8601 date"));
    }
    let record = Evidence {
        requirement: text::parse_id(&requirement, "REQ-", 3),
        date,
        commit: front.string("commit"),
        method: front.keyword("method", &["inspection", "demonstration"], |m| {
            Verification::parse(m).filter(|v| *v != Verification::Test)
        }),
        result: front.keyword("result", Outcome::VALUES, Outcome::parse),
        by: front.string("by"),
        subsections: parts
            .body
            .iter()
            .filter(|(_, l)| l.starts_with("## ") || l.starts_with("### "))
            .count(),
        path: rel.clone(),
    };
    if record.requirement.is_none() || stem(path) != requirement {
        front.problem(
            Some(2),
            format!("`requirement` `{requirement}` is not the ID in the file name"),
        );
    }
    Ok((record, front.problems))
}

/// The `area` values of the ID range table in `docs/spec/README.md`: the code spans of its
/// third column.
fn areas(root: &Path) -> Result<BTreeSet<String>, SpecError> {
    let text = read(&root.join("docs/spec/README.md"))?;
    let lines: Vec<(usize, &str)> = text.lines().enumerate().collect();
    let sections = text::sections(&lines);
    let mut out = BTreeSet::new();
    for section in sections.iter().filter(|s| s.title == "ID ranges") {
        for (_, line) in section.lines.iter().filter(|(_, l)| text::is_table_row(l)) {
            if let Some(cell) = text::cells(line).get(2) {
                out.extend(text::code_spans(cell).into_iter().map(str::to_owned));
            }
        }
    }
    Ok(out)
}

/// The lines of a data file without comments and empty lines.
fn data_lines(root: &Path, rel: &str) -> Result<Vec<String>, SpecError> {
    let text = read(&root.join(rel))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

/// The test plan rows of one design document.
fn test_plan_rows(rel: &Path, text: &str) -> Vec<TestPlanRow> {
    let lines: Vec<(usize, &str)> = text.lines().enumerate().map(|(i, l)| (i + 1, l)).collect();
    let mut rows = Vec::new();
    for section in text::sections(&lines)
        .iter()
        .filter(|s| s.title == "Test plan")
    {
        let mut in_fence = false;
        for (number, line) in &section.lines {
            in_fence ^= text::is_fence(line);
            if in_fence || !text::is_table_row(line) {
                continue;
            }
            if let Some(row) = test_plan_row(rel, *number, line) {
                rows.push(row);
            }
        }
    }
    rows
}

/// One test plan row, if its first cell names requirement IDs.
fn test_plan_row(rel: &Path, line: usize, row: &str) -> Option<TestPlanRow> {
    let cells = text::cells(row);
    let requirements: Vec<u16> = text::requirement_numbers(cells.first()?)
        .into_iter()
        .collect();
    if requirements.is_empty() {
        return None;
    }
    let mut tests = Vec::new();
    for cell in cells.iter().skip(1) {
        let mut file = None;
        for span in text::code_spans(cell) {
            if Path::new(span)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("rs"))
            {
                file = Some(span.to_owned());
            } else if text::is_test_name(span) {
                tests.push(TestRef {
                    file: file.clone(),
                    function: span.to_owned(),
                });
            }
        }
    }
    let evidence = text::references(row, "verification/REQ-")
        .into_iter()
        .next()
        .map(|(_, token)| PathBuf::from(format!("docs/{token}.md")));
    Some(TestPlanRow {
        document: rel.to_path_buf(),
        line,
        requirements,
        tests,
        evidence,
    })
}

/// Reads the whole repository. A parse error is a [`SpecError`] with the file and line.
///
/// # Errors
///
/// [`SpecError::Io`] if a file cannot be read, [`SpecError::Yaml`] if a frontmatter is not
/// valid YAML.
pub fn load(root: &Path) -> Result<Spec, SpecError> {
    let mut spec = Spec {
        areas: areas(root)?,
        hrm_sections: data_lines(root, "docs/spec/hrm-sections.txt")?
            .iter()
            .filter_map(|l| l.split('\t').next().map(str::to_owned))
            .collect(),
        vague_words: data_lines(root, "docs/spec/vague-words.txt")?,
        ..Spec::default()
    };
    for path in markdown_files(&root.join("docs/spec/requirements"), "REQ-")? {
        let (item, problems) = requirement(root, &path)?;
        spec.requirements.push(item);
        spec.problems.extend(problems);
    }
    for path in markdown_files(&root.join("docs/spec/open-items"), "OI-")? {
        let (item, problems) = open_item(root, &path)?;
        spec.open_items.push(item);
        spec.problems.extend(problems);
    }
    for path in markdown_files(&root.join("docs/plan"), "SLICE-")? {
        let (item, problems) = slice(root, &path)?;
        spec.slices.push(item);
        spec.problems.extend(problems);
    }
    let (complete, problems) = product_complete(root)?;
    spec.product_complete = complete;
    spec.problems.extend(problems);
    for path in markdown_files(&root.join("docs/verification"), "")? {
        let (item, problems) = evidence(root, &path)?;
        spec.evidence.push(item);
        spec.problems.extend(problems);
    }
    load_documents(root, &mut spec)?;
    Ok(spec)
}

/// Reads the design test plans, the markdown documents and the tests in the code.
fn load_documents(root: &Path, spec: &mut Spec) -> Result<(), SpecError> {
    let mut markdown = Vec::new();
    markdown_tree(&root.join("docs"), &mut markdown)?;
    let readme = root.join("README.md");
    if readme.exists() {
        markdown.push(readme);
    }
    for path in markdown {
        let rel = relative(root, &path);
        let text = read(&path)?;
        if rel.parent() == Some(Path::new("docs/design")) && stem(&rel) != "README" {
            spec.test_plans.extend(test_plan_rows(&rel, &text));
        }
        spec.documents.push(Document { path: rel, text });
    }
    let mut rust = Vec::new();
    for dir in ["crates", "src", "tests"] {
        code::rust_files(&root.join(dir), &mut rust)?;
    }
    rust.sort();
    for path in rust {
        let rel = relative(root, &path);
        let text = read(&path)?;
        spec.code_tests.extend(code::tests_in(&rel, &text));
        spec.documents.push(Document { path: rel, text });
    }
    Ok(())
}
