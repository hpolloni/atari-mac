//! The code gates: the cyclomatic complexity limit (REQ-058) and the source file length limit
//! (REQ-059).
//!
//! Design: `docs/design/code-gates.md`, "Complexity and file length gates". [`check`] reads
//! each Rust file that git tracks, measures each function and each file, and compares the
//! results with the code baseline [`BASELINE`].

use core::cmp::Reverse;
use core::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use syn::visit::{self, Visit};
use syn::{BinOp, Block, ExprBinary, ExprForLoop, ExprIf, ExprMatch, ExprWhile, Ident, PatGuard};
use syn::{ImplItemFn, ItemFn, ItemImpl, ItemTrait, TraitItemFn, Type};

/// The path of the code baseline, relative to the repository root (REQ-058 criterion 4).
pub const BASELINE: &str = "docs/spec/code-baseline.txt";

/// The highest complexity of a function that is not on the baseline (REQ-058).
pub const COMPLEXITY_LIMIT: usize = 15;

/// The highest number of lines of a file that is not on the baseline (REQ-059).
pub const LINES_LIMIT: usize = 1500;

/// The rule of a function that is too complex (REQ-058).
pub const COMPLEXITY: &str = "complexity";

/// The rule of a file that is too long (REQ-059).
pub const LINES: &str = "lines";

/// The rule of a baseline line that is not correct any more (REQ-058 criterion 5, REQ-059
/// criterion 4).
pub const STALE: &str = "stale";

/// One function or file that a code gate rejects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeViolation {
    /// [`COMPLEXITY`], [`LINES`] or [`STALE`].
    pub rule: &'static str,
    /// The file, relative to the repository root.
    pub path: PathBuf,
    /// The line of the function (1 for a file, the baseline line for a stale line).
    pub line: usize,
    /// The message (REQ-058 criterion 3, REQ-059 criterion 2).
    pub message: String,
}

impl fmt::Display for CodeViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.rule, self.message)
    }
}

/// The complexity of one function (REQ-058 criterion 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionComplexity {
    /// `Type::name` for a function in an `impl` or `trait` block, `name` otherwise.
    pub name: String,
    /// The line of the function name in the source text.
    pub line: usize,
    /// The cyclomatic complexity.
    pub complexity: usize,
}

/// The complexity of each function of one Rust source text (REQ-058 criterion 1), in the
/// order of the source.
///
/// # Errors
///
/// The parse error of `syn` when the text is not a Rust file.
pub fn complexities(source: &str) -> Result<Vec<FunctionComplexity>, syn::Error> {
    let file = syn::parse_file(source)?;
    let mut counter = Counter::default();
    counter.visit_file(&file);
    Ok(counter.functions)
}

/// The number of lines of a text: the number of newline characters, plus 1 if the last line
/// has no newline (REQ-059 criterion 1).
#[must_use]
pub fn file_lines(text: &str) -> usize {
    let newlines = text.bytes().filter(|&b| b == b'\n').count();
    newlines + usize::from(!text.is_empty() && !text.ends_with('\n'))
}

/// Whether the gates read a file at `path` (relative to the repository root, with `/`): a
/// file in `src/`, `tests/` or `benches/`, or `build.rs`, of the root or of a crate in
/// `crates/` (REQ-058 criterion 2).
#[must_use]
pub fn is_gated(path: &str) -> bool {
    if Path::new(path)
        .extension()
        .is_none_or(|extension| extension != "rs")
    {
        return false;
    }
    let local = path
        .strip_prefix("crates/")
        .and_then(|rest| rest.split_once('/'))
        .map_or(path, |(_, rest)| rest);
    local == "build.rs"
        || ["src/", "tests/", "benches/"]
            .iter()
            .any(|dir| local.starts_with(dir))
}

/// The Rust files of the repository at `root` that the gates read: the output of
/// `git ls-files '*.rs'` that [`is_gated`] accepts (REQ-058 criterion 2), sorted.
///
/// # Errors
///
/// A message when git does not run or fails.
pub fn rust_files(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--", "*.rs"])
        .output()
        .map_err(|error| format!("cannot run git ls-files: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut files: Vec<String> = text
        .split('\0')
        .filter(|path| is_gated(path))
        .map(str::to_owned)
        .collect();
    files.sort();
    Ok(files)
}

/// Runs both gates on the repository at `root` with the baseline [`BASELINE`].
#[must_use]
pub fn check(root: &Path) -> Vec<CodeViolation> {
    let mut violations = Vec::new();
    let files = match rust_files(root) {
        Ok(files) => files,
        Err(message) => {
            violations.push(violation(COMPLEXITY, ".", 1, message));
            return violations;
        }
    };
    // A repository without the baseline file has an empty baseline.
    let baseline = fs::read_to_string(root.join(BASELINE)).map_or_else(
        |_| Baseline::default(),
        |text| Baseline::parse(&text, &mut violations),
    );
    let mut lengths = BTreeMap::new();
    let mut functions = Vec::new();
    for file in &files {
        let text = match fs::read_to_string(root.join(file)) {
            Ok(text) => text,
            Err(error) => {
                violations.push(violation(COMPLEXITY, file, 1, format!("{file}: {error}")));
                continue;
            }
        };
        let _previous = lengths.insert(file.clone(), file_lines(&text));
        match complexities(&text) {
            Ok(list) => functions.extend(list.into_iter().map(|f| (file.clone(), f))),
            Err(error) => {
                let line = line_of(error.span());
                let message = format!("{file}:{line}: cannot parse: {error}");
                violations.push(violation(COMPLEXITY, file, line, message));
            }
        }
    }
    check_complexity(&functions, &baseline, &mut violations);
    check_lines(&lengths, &baseline, &mut violations);
    violations
}

/// A violation with its fields.
fn violation(rule: &'static str, path: &str, line: usize, message: String) -> CodeViolation {
    CodeViolation {
        rule,
        path: PathBuf::from(path),
        line,
        message,
    }
}

/// The complexity gate (REQ-058 criteria 3 and 5) on the functions of all files.
fn check_complexity(
    functions: &[(String, FunctionComplexity)],
    baseline: &Baseline,
    violations: &mut Vec<CodeViolation>,
) {
    // The functions above the limit and the baseline lines, by file and name. Two functions
    // of one file can have the same name (for example in two test modules): the highest
    // complexity pairs with the highest baseline value.
    let mut above: BTreeMap<(&str, &str), Vec<&FunctionComplexity>> = BTreeMap::new();
    for (file, function) in functions {
        if function.complexity > COMPLEXITY_LIMIT {
            above
                .entry((file.as_str(), function.name.as_str()))
                .or_default()
                .push(function);
        }
    }
    let mut allowed: BTreeMap<(&str, &str), Vec<&BaselineLine>> = BTreeMap::new();
    for line in &baseline.complexity {
        allowed
            .entry((line.path.as_str(), line.name.as_str()))
            .or_default()
            .push(line);
    }
    let keys: BTreeSet<(&str, &str)> = above.keys().chain(allowed.keys()).copied().collect();
    for key in keys {
        let mut found = above.remove(&key).unwrap_or_default();
        let mut lines = allowed.remove(&key).unwrap_or_default();
        found.sort_by_key(|f| Reverse(f.complexity));
        lines.sort_by_key(|l| Reverse(l.value));
        for (index, function) in found.iter().enumerate() {
            let (file, name) = key;
            let line = function.line;
            let value = function.complexity;
            match lines.get(index) {
                None => violations.push(violation(
                    COMPLEXITY,
                    file,
                    line,
                    format!(
                        "{file}:{line}: function `{name}` has complexity {value} \
                         (limit {COMPLEXITY_LIMIT}, not on {BASELINE})"
                    ),
                )),
                Some(allowed) if value > allowed.value => violations.push(violation(
                    COMPLEXITY,
                    file,
                    line,
                    format!(
                        "{file}:{line}: function `{name}` has complexity {value} \
                         (baseline {})",
                        allowed.value
                    ),
                )),
                Some(allowed) if value < allowed.value => {
                    violations.push(stale(allowed, &format!("the complexity is now {value}")));
                }
                Some(_) => {}
            }
        }
        for unused in lines.iter().skip(found.len()) {
            let reason = format!("no function above {COMPLEXITY_LIMIT} matches it");
            violations.push(stale(unused, &reason));
        }
    }
}

/// The file length gate (REQ-059 criteria 2 and 4) on the lengths of all files.
fn check_lines(
    lengths: &BTreeMap<String, usize>,
    baseline: &Baseline,
    violations: &mut Vec<CodeViolation>,
) {
    let allowed: BTreeMap<&str, &BaselineLine> = baseline
        .lines
        .iter()
        .map(|line| (line.path.as_str(), line))
        .collect();
    for (file, &length) in lengths {
        if length <= LINES_LIMIT {
            continue;
        }
        match allowed.get(file.as_str()) {
            None => violations.push(violation(
                LINES,
                file,
                1,
                format!("{file}: {length} lines (limit {LINES_LIMIT}, not on {BASELINE})"),
            )),
            Some(line) if length > line.value => violations.push(violation(
                LINES,
                file,
                1,
                format!("{file}: {length} lines (baseline {})", line.value),
            )),
            Some(_) => {}
        }
    }
    for line in &baseline.lines {
        match lengths.get(&line.path) {
            None => violations.push(stale(line, "the file does not exist")),
            Some(&length) if length <= LINES_LIMIT => {
                let reason = format!("the file has {length} lines, not more than {LINES_LIMIT}");
                violations.push(stale(line, &reason));
            }
            Some(&length) if length < line.value => {
                violations.push(stale(line, &format!("the file has {length} lines now")));
            }
            Some(_) => {}
        }
    }
}

/// The stale violation of one baseline line.
fn stale(line: &BaselineLine, reason: &str) -> CodeViolation {
    violation(
        STALE,
        BASELINE,
        line.number,
        format!("{BASELINE}:{}: `{}`: {reason}", line.number, line.text),
    )
}

/// One line of the code baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BaselineLine {
    /// The line number in the baseline file.
    number: usize,
    /// The text of the line.
    text: String,
    /// The file.
    path: String,
    /// The function name (empty for a `lines` line).
    name: String,
    /// The complexity or the length.
    value: usize,
}

/// The code baseline: the functions and files that may be above the limits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Baseline {
    /// The `complexity` lines.
    complexity: Vec<BaselineLine>,
    /// The `lines` lines.
    lines: Vec<BaselineLine>,
}

impl Baseline {
    /// Parses the baseline text. A line that is not in the format of the design is stale.
    fn parse(text: &str, violations: &mut Vec<CodeViolation>) -> Self {
        let mut baseline = Self::default();
        for (index, raw) in text.lines().enumerate() {
            let number = index + 1;
            if raw.is_empty() || raw.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = raw.split(' ').collect();
            let entry = |path: &str, name: &str, value: &str| {
                value.parse().ok().map(|value| BaselineLine {
                    number,
                    text: raw.to_owned(),
                    path: path.to_owned(),
                    name: name.to_owned(),
                    value,
                })
            };
            match fields.as_slice() {
                [COMPLEXITY, path, name, value] => {
                    if let Some(line) = entry(path, name, value) {
                        baseline.complexity.push(line);
                        continue;
                    }
                }
                [LINES, path, value] => {
                    if let Some(line) = entry(path, "", value) {
                        baseline.lines.push(line);
                        continue;
                    }
                }
                _ => {}
            }
            violations.push(violation(
                STALE,
                BASELINE,
                number,
                format!(
                    "{BASELINE}:{number}: `{raw}`: not `complexity <path> <name> <value>` \
                     or `lines <path> <value>`"
                ),
            ));
        }
        baseline
    }
}

/// The line of a span (1-based).
fn line_of(span: proc_macro2::Span) -> usize {
    span.start().line
}

/// The name of the type of an `impl` block, without generics.
fn type_name(ty: &Type) -> String {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map_or_else(|| "_".to_owned(), |segment| segment.ident.to_string()),
        Type::Reference(reference) => type_name(&reference.elem),
        Type::Paren(paren) => type_name(&paren.elem),
        Type::Group(group) => type_name(&group.elem),
        _ => "_".to_owned(),
    }
}

/// The syntax tree visitor that counts the decisions of each function.
#[derive(Debug, Default)]
struct Counter {
    /// The functions found so far.
    functions: Vec<FunctionComplexity>,
    /// The index of the function that the visitor is in.
    current: Option<usize>,
    /// The name of the `impl` or `trait` block that the visitor is in, outside a function.
    owner: Option<String>,
}

impl Counter {
    /// Adds `count` decisions to the current function.
    fn add(&mut self, count: usize) {
        if let Some(function) = self.current.and_then(|i| self.functions.get_mut(i)) {
            function.complexity += count;
        }
    }

    /// Visits the body of the function `ident` as a function of its own.
    fn function(&mut self, ident: &Ident, body: &Block) {
        let name = self
            .owner
            .as_ref()
            .map_or_else(|| ident.to_string(), |owner| format!("{owner}::{ident}"));
        self.functions.push(FunctionComplexity {
            name,
            line: line_of(ident.span()),
            complexity: 1,
        });
        let current = self.current.replace(self.functions.len() - 1);
        let owner = self.owner.take();
        self.visit_block(body);
        self.current = current;
        self.owner = owner;
    }

    /// Visits the items of an `impl` or `trait` block named `name`.
    fn block_of(&mut self, name: String, visit: impl FnOnce(&mut Self)) {
        let owner = self.owner.replace(name);
        visit(self);
        self.owner = owner;
    }
}

impl<'ast> Visit<'ast> for Counter {
    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        self.function(&i.sig.ident, &i.block);
    }

    fn visit_impl_item_fn(&mut self, i: &'ast ImplItemFn) {
        self.function(&i.sig.ident, &i.block);
    }

    fn visit_trait_item_fn(&mut self, i: &'ast TraitItemFn) {
        if let Some(body) = &i.default {
            self.function(&i.sig.ident, body);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        self.block_of(type_name(&i.self_ty), |counter| {
            visit::visit_item_impl(counter, i);
        });
    }

    fn visit_item_trait(&mut self, i: &'ast ItemTrait) {
        self.block_of(i.ident.to_string(), |counter| {
            visit::visit_item_trait(counter, i);
        });
    }

    fn visit_expr_if(&mut self, i: &'ast ExprIf) {
        self.add(1);
        visit::visit_expr_if(self, i);
    }

    fn visit_expr_while(&mut self, i: &'ast ExprWhile) {
        self.add(1);
        visit::visit_expr_while(self, i);
    }

    fn visit_expr_for_loop(&mut self, i: &'ast ExprForLoop) {
        self.add(1);
        visit::visit_expr_for_loop(self, i);
    }

    fn visit_expr_binary(&mut self, i: &'ast ExprBinary) {
        if matches!(i.op, BinOp::And(_) | BinOp::Or(_)) {
            self.add(1);
        }
        visit::visit_expr_binary(self, i);
    }

    fn visit_pat_guard(&mut self, i: &'ast PatGuard) {
        self.add(1);
        visit::visit_pat_guard(self, i);
    }

    fn visit_expr_match(&mut self, i: &'ast ExprMatch) {
        self.add(i.arms.len().saturating_sub(1));
        visit::visit_expr_match(self, i);
    }
}
