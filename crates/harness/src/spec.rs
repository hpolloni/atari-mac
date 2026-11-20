//! Specification gates and two-way traceability (REQ-027, REQ-043 to REQ-056).
//!
//! Design: `docs/design/spec-gates.md`. [`load()`] reads the requirements, the open items, the
//! slices, the evidence records, the test plans of the design documents and the tests in
//! the code. [`check`] runs every gate and gives the violations. The violations in
//! `docs/spec/gate-baseline.txt` are known: [`Baseline`] removes them from a result.

mod code;
mod gates;
mod load;
mod readme;
mod report;
mod text;

use core::error::Error;
use core::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub use code::tests_in;
pub use gates::{
    evidence, indexes, open_items, references, schema, slices, sources, structure, test_plans,
    trace, vague_words,
};
pub use load::load;
pub use readme::{STATUS_BEGIN, STATUS_END, readme, status_block};
pub use report::{generate, matrix, open_item_index, requirement_index};
pub use text::requirement_numbers;

/// The path of the baseline file, relative to the repository root.
pub const BASELINE: &str = "docs/spec/gate-baseline.txt";

/// The path of the traceability matrix, relative to the repository root (REQ-052).
pub const MATRIX: &str = "target/traceability.md";

/// The path of the requirement index, relative to the repository root.
pub const REQUIREMENT_INDEX: &str = "docs/spec/requirements/INDEX.md";

/// The path of the open item index, relative to the repository root (REQ-053).
pub const OPEN_ITEM_INDEX: &str = "docs/spec/open-items/README.md";

/// The path of the README, relative to the repository root (REQ-056).
pub const README: &str = "README.md";

/// Defines a metadata enum with its frontmatter values.
macro_rules! keyword_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub enum $name {
            #[default]
            $($(#[$vdoc])* $variant),+
        }

        impl $name {
            /// All frontmatter values, in order.
            pub const VALUES: &[&str] = &[$($text),+];

            /// Parses a frontmatter value.
            #[must_use]
            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }

            /// The frontmatter value.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }
    };
}

keyword_enum! {
    /// The status of a requirement.
    Status {
        /// Written, not yet accepted.
        Draft = "draft",
        /// Accepted.
        Accepted = "accepted",
        /// Removed. The file stays, because IDs never change.
        Withdrawn = "withdrawn",
    }
}

keyword_enum! {
    /// The priority of a requirement.
    Priority {
        /// Necessary to boot and to run ordinary software.
        Must = "must",
        /// Necessary for accurate timing or less common software.
        Should = "should",
        /// Rare behavior.
        Could = "could",
    }
}

keyword_enum! {
    /// The verification method of a requirement.
    Verification {
        /// An automated test.
        Test = "test",
        /// A review of the code or the documents.
        Inspection = "inspection",
        /// A person runs the emulator and looks at the result.
        Demonstration = "demonstration",
    }
}

keyword_enum! {
    /// The status of a slice or a work package.
    SliceStatus {
        /// Not started.
        Planned = "planned",
        /// Started.
        InProgress = "in-progress",
        /// Finished. The gates apply their `done` rules.
        Done = "done",
    }
}

keyword_enum! {
    /// The status of an open item.
    ItemStatus {
        /// Not decided.
        Open = "open",
        /// Decided. The `Decision` section has the decision.
        Decided = "decided",
    }
}

keyword_enum! {
    /// The result of an evidence record.
    Outcome {
        /// The check passed.
        Pass = "pass",
        /// The check failed.
        Fail = "fail",
    }
}

keyword_enum! {
    /// The storage of a reference (REQ-046).
    Storage {
        /// The file is in git, in `reference/`.
        Commit = "commit",
        /// The file is not in git. Its URL and its SHA-256 freeze it, and `make reference`
        /// downloads it to `reference/pinned/`.
        Pin = "pin",
    }
}

/// A registry entry (`docs/spec/references/<ID>.md`) and its section index (REQ-046).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// The `id` value, for example `HRM`.
    pub id: String,
    /// The `storage` value.
    pub storage: Storage,
    /// The `url` value.
    pub url: String,
    /// The `sha256` value.
    pub sha256: String,
    /// The `file` value, relative to the repository root.
    pub file: String,
    /// The section index (`<ID>.sections.txt`): each key and its title. Empty for a
    /// reference without sections.
    pub sections: BTreeMap<String, String>,
    /// The registry file, relative to the repository root.
    pub path: PathBuf,
}

/// A level-2 section (`## Title`) of a markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The heading text.
    pub title: String,
    /// The line number of the heading (1-based).
    pub line: usize,
    /// The lines of the section after the heading, with their line numbers.
    pub lines: Vec<(usize, String)>,
}

/// One requirement file (`docs/spec/requirements/REQ-NNN.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// The number in the file name.
    pub id: u16,
    /// The `id` value of the frontmatter.
    pub declared_id: String,
    /// The `title` value.
    pub title: String,
    /// The text after `# REQ-NNN: ` in the first heading.
    pub heading: String,
    /// The `status` value.
    pub status: Status,
    /// The `priority` value.
    pub priority: Priority,
    /// The `area` value.
    pub area: String,
    /// The `source` value.
    pub source: String,
    /// The `verification` value.
    pub verification: Verification,
    /// The `depends_on` values that are requirement IDs.
    pub depends_on: Vec<u16>,
    /// The level-2 sections of the body.
    pub sections: Vec<Section>,
    /// The path relative to the repository root.
    pub path: PathBuf,
}

impl Requirement {
    /// The section with the heading `title`.
    #[must_use]
    pub fn section(&self, title: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.title == title)
    }

    /// The number of acceptance criteria (lines `N. ` in `Acceptance criteria`).
    #[must_use]
    pub fn criteria(&self) -> usize {
        self.section("Acceptance criteria").map_or(0, |s| {
            s.lines
                .iter()
                .filter(|(_, line)| text::list_number(line).is_some())
                .count()
        })
    }

    /// False if the status is `withdrawn`.
    #[must_use]
    pub fn active(&self) -> bool {
        self.status != Status::Withdrawn
    }
}

/// One open item (`docs/spec/open-items/OI-NNN.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenItem {
    /// The number in the file name.
    pub id: u16,
    /// The `title` value.
    pub title: String,
    /// The `status` value.
    pub status: ItemStatus,
    /// The `area` values.
    pub area: Vec<String>,
    /// The `affects` values: requirement IDs and file paths.
    pub affects: Vec<String>,
    /// The text of the `Decision` section, trimmed, if the section exists.
    pub decision: Option<String>,
    /// The path relative to the repository root.
    pub path: PathBuf,
}

/// One work package of a slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// For example `WP-1`.
    pub id: String,
    /// The `status` value.
    pub status: SliceStatus,
    /// The requirements of the package.
    pub requirements: Vec<u16>,
}

/// One slice (`docs/plan/SLICE-NN.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slice {
    /// The number in the file name.
    pub id: u16,
    /// The `title` value.
    pub title: String,
    /// The `status` value.
    pub status: SliceStatus,
    /// The work packages.
    pub packages: Vec<Package>,
    /// The path relative to the repository root.
    pub path: PathBuf,
}

/// One evidence record (`docs/verification/REQ-NNN.md`, REQ-051).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    /// The `requirement` value, if it is a requirement ID.
    pub requirement: Option<u16>,
    /// The `date` value.
    pub date: String,
    /// The `commit` value.
    pub commit: String,
    /// The `method` value.
    pub method: Verification,
    /// The `result` value.
    pub result: Outcome,
    /// The `by` value.
    pub by: String,
    /// The number of subsection headings (`##` or `###`) in the body.
    pub subsections: usize,
    /// The path relative to the repository root.
    pub path: PathBuf,
}

/// A test that a test plan names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TestRef {
    /// The nearest `.rs` name in backticks before the test in the same cell, if any.
    pub file: Option<String>,
    /// The function name.
    pub function: String,
}

impl TestRef {
    /// True if `test` is this test: the same function, in a file of the module that `file`
    /// names (if a file is given). See [`file_matches`].
    #[must_use]
    pub fn matches(&self, test: &CodeTest) -> bool {
        test.function == self.function
            && self
                .file
                .as_ref()
                .is_none_or(|file| file_matches(file, &test.file))
    }
}

/// True if the file `path` is in the module that the test plan file `file` names
/// (`docs/design/spec-gates.md`).
///
/// A name without a directory, for example `antic.rs`, matches each file `antic.rs` and each
/// file below a directory `antic/` of a crate source tree (below a `src` or `tests`
/// directory), for example `crates/atari-core/src/antic/tests/dma.rs`. A path with a
/// directory, for example `crates/harness/tests/boot.rs`, matches only a path that ends with
/// all its components.
#[must_use]
pub fn file_matches(file: &str, path: &Path) -> bool {
    let plan = Path::new(file);
    if path.ends_with(plan) {
        return true;
    }
    if plan.parent().is_some_and(|p| !p.as_os_str().is_empty()) {
        return false;
    }
    let Some(stem) = plan.file_stem() else {
        return false;
    };
    let Some(dirs) = path.parent() else {
        return false;
    };
    let mut in_source = false;
    for dir in dirs {
        if in_source && dir == stem {
            return true;
        }
        in_source |= dir == "src" || dir == "tests";
    }
    false
}

/// One table row of the `## Test plan` section of a design document whose first cell names
/// requirement IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestPlanRow {
    /// The design document, relative to the repository root.
    pub document: PathBuf,
    /// The line number of the row.
    pub line: usize,
    /// The requirements in the first cell (ranges expanded).
    pub requirements: Vec<u16>,
    /// The tests in backticks in the other cells.
    pub tests: Vec<TestRef>,
    /// The evidence record that the row names (`docs/verification/REQ-NNN.md`), if any.
    pub evidence: Option<PathBuf>,
}

/// A function in the code that is a `#[test]` function or that has a `Verifies` comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeTest {
    /// The file, relative to the repository root.
    pub file: PathBuf,
    /// The line number of the `fn` line.
    pub line: usize,
    /// The function name.
    pub function: String,
    /// The function has `#[test]`.
    pub is_test: bool,
    /// The function has `#[ignore]`.
    pub ignored: bool,
    /// The reason of `#[ignore = "reason"]`, if the attribute has one. A planned test has
    /// the reason `planned: SLICE-NN` (REQ-027).
    pub ignore_reason: Option<String>,
    /// The requirements in its `/// Verifies` comment.
    pub verifies: Vec<u16>,
}

/// A text file that the `references` gate reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The path relative to the repository root.
    pub path: PathBuf,
    /// The text.
    pub text: String,
}

/// The whole specification of the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spec {
    /// The requirements, ordered by file name.
    pub requirements: Vec<Requirement>,
    /// The open items, ordered by file name.
    pub open_items: Vec<OpenItem>,
    /// The slices, ordered by file name.
    pub slices: Vec<Slice>,
    /// The `product_complete` value of `docs/plan/README.md`.
    pub product_complete: bool,
    /// The evidence records.
    pub evidence: Vec<Evidence>,
    /// The test plan rows of all design documents.
    pub test_plans: Vec<TestPlanRow>,
    /// The tests in the code.
    pub code_tests: Vec<CodeTest>,
    /// The markdown and Rust files that can hold references.
    pub documents: Vec<Document>,
    /// The `area` values of the ID range table in `docs/spec/README.md`.
    pub areas: BTreeSet<String>,
    /// The registry entries of `docs/spec/references/`, ordered by file name.
    pub registry: Vec<Reference>,
    /// The phrases of `docs/spec/vague-words.txt`.
    pub vague_words: Vec<String>,
    /// The frontmatter problems that [`load()`] found. The gates report them.
    pub problems: Vec<Violation>,
}

impl Spec {
    /// The requirement with the number `id`.
    #[must_use]
    pub fn requirement(&self, id: u16) -> Option<&Requirement> {
        self.requirements.iter().find(|r| r.id == id)
    }

    /// The slice and the package that plan the requirement `id`, if any.
    #[must_use]
    pub fn planned(&self, id: u16) -> Option<(&Slice, &Package)> {
        self.slices.iter().find_map(|slice| {
            slice
                .packages
                .iter()
                .find(|p| p.requirements.contains(&id))
                .map(|p| (slice, p))
        })
    }

    /// True if the package or the slice that plans the requirement `id` is `done`.
    #[must_use]
    pub fn done(&self, id: u16) -> bool {
        self.planned(id).is_some_and(|(slice, package)| {
            slice.status == SliceStatus::Done || package.status == SliceStatus::Done
        })
    }

    /// The test plan rows that name the requirement `id`.
    pub fn rows(&self, id: u16) -> impl Iterator<Item = &TestPlanRow> {
        self.test_plans
            .iter()
            .filter(move |row| row.requirements.contains(&id))
    }

    /// The `#[test]` functions whose `Verifies` comment names the requirement `id`.
    pub fn tests_of(&self, id: u16) -> impl Iterator<Item = &CodeTest> {
        self.code_tests
            .iter()
            .filter(move |t| t.is_test && t.verifies.contains(&id))
    }
}

/// An error that stops [`load()`]: a file that cannot be read, or YAML that cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecError {
    /// A file or a directory cannot be read.
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        message: String,
    },
    /// The YAML frontmatter of a file cannot be parsed.
    Yaml {
        /// The path.
        path: PathBuf,
        /// The line in the file (1-based).
        line: usize,
        /// The cause.
        message: String,
    },
    /// The status markers of `README.md` are missing, duplicated or unclosed (REQ-056).
    Markers {
        /// The path.
        path: PathBuf,
        /// The cause.
        message: String,
    },
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } | Self::Markers { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            Self::Yaml {
                path,
                line,
                message,
            } => write!(f, "{}:{line}: YAML: {message}", path.display()),
        }
    }
}

impl Error for SpecError {}

/// One rule violation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    /// The requirement that states the rule, for example 43 for REQ-043.
    pub rule: u16,
    /// The file, relative to the repository root.
    pub file: PathBuf,
    /// The line, if known.
    pub line: Option<usize>,
    /// What is wrong.
    pub message: String,
}

impl Violation {
    /// A violation of `rule` in `file`.
    pub fn new(rule: u16, file: &Path, line: Option<usize>, message: impl Into<String>) -> Self {
        Self {
            rule,
            file: file.to_path_buf(),
            line,
            message: message.into(),
        }
    }

    /// The baseline form: `REQ-NNN file: message`, without the line number, so that an
    /// edit above the line does not change it.
    #[must_use]
    pub fn key(&self) -> String {
        format!(
            "REQ-{:03} {}: {}",
            self.rule,
            self.file.display(),
            self.message
        )
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "REQ-{:03} {}", self.rule, self.file.display())?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }
        write!(f, ": {}", self.message)
    }
}

/// Runs every gate. An empty list means that all gates pass.
#[must_use]
pub fn check(spec: &Spec, root: &Path) -> Vec<Violation> {
    let mut all = schema(spec);
    all.extend(structure(spec));
    all.extend(vague_words(spec));
    all.extend(sources(spec, root));
    all.extend(references(spec, root));
    all.extend(test_plans(spec));
    all.extend(trace(spec));
    all.extend(slices(spec));
    all.extend(evidence(spec));
    all.extend(open_items(spec, root));
    all.extend(indexes(spec, root));
    all.extend(readme(spec, root));
    all.sort();
    all.dedup();
    all
}

/// The known violations of `docs/spec/gate-baseline.txt`: one [`Violation::key`] per line.
/// Empty lines and lines that start with `#` are ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Baseline {
    /// The keys.
    keys: BTreeSet<String>,
}

impl Baseline {
    /// Reads the baseline file below `root`. A missing file is an empty baseline.
    ///
    /// # Errors
    ///
    /// [`SpecError::Io`] if the file exists but cannot be read.
    pub fn read(root: &Path) -> Result<Self, SpecError> {
        let path = root.join(BASELINE);
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = load::read(&path)?;
        Ok(Self::parse(&text))
    }

    /// Parses the text of a baseline file.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let keys = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .collect();
        Self { keys }
    }

    /// True if the baseline lists `violation`.
    #[must_use]
    pub fn contains(&self, violation: &Violation) -> bool {
        self.keys.contains(&violation.key())
    }

    /// The violations that the baseline does not list.
    #[must_use]
    pub fn filter(&self, violations: &[Violation]) -> Vec<Violation> {
        violations
            .iter()
            .filter(|v| !self.contains(v))
            .cloned()
            .collect()
    }

    /// The baseline lines of the rules `rules` that match none of `violations`.
    #[must_use]
    pub fn stale(&self, violations: &[Violation], rules: &[u16]) -> Vec<String> {
        let found: BTreeSet<String> = violations.iter().map(Violation::key).collect();
        self.keys
            .iter()
            .filter(|key| {
                rules
                    .iter()
                    .any(|rule| key.starts_with(&format!("REQ-{rule:03} ")))
            })
            .filter(|key| !found.contains(*key))
            .cloned()
            .collect()
    }

    /// The baseline lines that name no rule of the gates.
    #[must_use]
    pub fn unknown_rules(&self) -> Vec<String> {
        self.keys
            .iter()
            .filter(|key| {
                !RULES
                    .iter()
                    .any(|rule| key.starts_with(&format!("REQ-{rule:03} ")))
            })
            .cloned()
            .collect()
    }

    /// The number of lines.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// True if the baseline has no lines.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// The rules that the gates check.
pub const RULES: &[u16] = &[27, 43, 44, 45, 46, 47, 48, 49, 50, 51, 53, 54, 56];
