//! The code gates: the complexity of a function (REQ-058) and the length of a file (REQ-059).
//! The test names follow the test plan in `docs/design/code-gates.md`.
//!
//! Each gate runs on the real repository with `docs/spec/code-baseline.txt`, and on small
//! synthetic git repositories in a temporary directory.

/// The tests, the fixture and the helpers. The module has `cfg(test)`, so that the helpers
/// can use `unwrap` as the tests do.
#[cfg(test)]
mod gates {
    use std::fmt::Write as _;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::{env, fs, process};

    use atari_harness::code::{
        self, BASELINE, COMPLEXITY, CodeViolation, FunctionComplexity, LINES, STALE,
    };
    use atari_harness::data::repo_root;

    /// A number for each fixture, so that parallel tests use different directories.
    static FIXTURES: AtomicUsize = AtomicUsize::new(0);

    /// A git repository in a temporary directory. It is removed when the value is dropped.
    struct Fixture {
        /// The root of the repository.
        root: PathBuf,
    }

    impl Fixture {
        /// An empty git repository.
        fn new() -> Self {
            let number = FIXTURES.fetch_add(1, Ordering::Relaxed);
            let root = env::temp_dir().join(format!("atari-code-gates-{}-{number}", process::id()));
            let _removed = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            let fixture = Self { root };
            fixture.git(&["init", "-q"]);
            fixture
        }

        /// Runs git in the repository.
        fn git(&self, args: &[&str]) {
            let status = process::Command::new("git")
                .arg("-C")
                .arg(&self.root)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?} failed");
        }

        /// Writes a file below the root and does not add it to git.
        fn write_untracked(&self, path: &str, text: &str) {
            let file = self.root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }

        /// Writes a file below the root and adds it to git.
        fn write(&self, path: &str, text: &str) {
            self.write_untracked(path, text);
            self.git(&["add", "--", path]);
        }

        /// Writes the code baseline with these lines after a comment line.
        fn baseline(&self, lines: &[&str]) {
            let mut text = String::from("# The code baseline of the fixture.\n");
            for line in lines {
                writeln!(text, "{line}").unwrap();
            }
            self.write(BASELINE, &text);
        }

        /// The violations of both gates.
        fn check(&self) -> Vec<CodeViolation> {
            code::check(&self.root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _removed = fs::remove_dir_all(&self.root);
        }
    }

    /// A source file with one function `name` of complexity `complexity` (a chain of `if`).
    fn function(name: &str, complexity: usize) -> String {
        let mut text = format!("fn {name}(x: u32) -> u32 {{\n    let mut y = 0;\n");
        for value in 1..complexity {
            writeln!(text, "    if x == {value} {{ y += 1; }}").unwrap();
        }
        text.push_str("    y\n}\n");
        text
    }

    /// A text of `lines` lines, each with a newline.
    fn lines(lines: usize) -> String {
        "// line\n".repeat(lines)
    }

    /// The complexity of each function of `source` by name.
    fn by_name(source: &str) -> Vec<(String, usize)> {
        code::complexities(source)
            .unwrap()
            .into_iter()
            .map(|f| (f.name, f.complexity))
            .collect()
    }

    /// The complexity of the only function of `source`.
    fn single(source: &str) -> usize {
        let list = code::complexities(source).unwrap();
        assert_eq!(list.len(), 1, "one function in {source}");
        list[0].complexity
    }

    /// The rules and messages of a list of violations, for the assertion messages.
    fn show(violations: &[CodeViolation]) -> String {
        violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Verifies REQ-058: the complexity of a function is 1, plus 1 for each `if` (also `if let`
    /// and `else if`), `while` (also `while let`), `for`, `&&`, `||` and match guard, plus the
    /// number of arms of each `match` minus 1. A closure counts in the function that contains it,
    /// and a nested `fn` is a function of its own (criterion 1).
    #[test]
    fn complexity_counts_each_decision() {
        assert_eq!(single("fn f() {}"), 1, "a function without decisions");
        assert_eq!(single("fn f(a: bool) { if a {} }"), 2, "if");
        assert_eq!(
            single("fn f(a: Option<u8>) { if let Some(_) = a {} }"),
            2,
            "if let"
        );
        let chain = "fn f(a: u8) { if a == 1 {} else if a == 2 {} else if a == 3 {} else {} }";
        assert_eq!(single(chain), 4, "if and two else if; else counts nothing");
        assert_eq!(
            single("fn f(mut a: u8) { while a > 0 { a -= 1; } }"),
            2,
            "while"
        );
        let while_let = "fn f(mut v: Vec<u8>) { while let Some(_) = v.pop() {} }";
        assert_eq!(single(while_let), 2, "while let");
        assert_eq!(single("fn f() { for _ in 0..3 {} }"), 2, "for");
        assert_eq!(
            single("fn f() { loop { break; } }"),
            1,
            "loop counts nothing"
        );
        assert_eq!(single("fn f(a: bool, b: bool) -> bool { a && b }"), 2, "&&");
        assert_eq!(
            single("fn f(a: bool, b: bool, c: bool) -> bool { a || b || c }"),
            3,
            "||"
        );
        assert_eq!(
            single("fn f(a: u8, b: u8) -> bool { a & b == 0 }"),
            1,
            "& is no decision"
        );
        let let_chain = "fn f(a: Option<u8>) { if let Some(x) = a && x > 1 {} }";
        assert_eq!(single(let_chain), 3, "if let with a && chain");
        let three_arms = "fn f(a: u8) -> u8 { match a { 0 => 1, 1 => 2, _ => 3 } }";
        assert_eq!(single(three_arms), 3, "match with three arms");
        let one_arm = "fn f(a: u8) -> u8 { match a { _ => 1 } }";
        assert_eq!(single(one_arm), 1, "match with one arm");
        let guards =
            "fn f(a: u8) -> u8 { match a { x if x > 9 => 1, x if x > 4 && x < 7 => 2, _ => 3 } }";
        assert_eq!(single(guards), 6, "three arms, two guards and one &&");
        let question = "fn f(a: Option<u8>) -> Option<u8> { let b = a?; Some(b) }";
        assert_eq!(single(question), 1, "? counts nothing");
        let in_macro = "fn f(a: u8) { assert!(a == 1 || a == 2); let _ = matches!(a, 1 | 2); }";
        assert_eq!(
            single(in_macro),
            1,
            "decisions in a macro body count nothing"
        );
        let closure = "fn f(v: &[u8]) -> usize { v.iter().filter(|&&x| if x > 1 { x < 5 } else { false }).count() }";
        assert_eq!(
            single(closure),
            2,
            "a closure counts in the function that contains it"
        );
        let nested = "fn outer(a: bool) { fn inner(b: bool) { if b {} if b {} } if a {} }";
        assert_eq!(
            by_name(nested),
            [("outer".to_owned(), 2), ("inner".to_owned(), 3)],
            "a nested fn is a function of its own"
        );
        names_and_lines();
    }

    /// The names of the functions in `impl` and `trait` blocks, and the line numbers.
    fn names_and_lines() {
        let source = "struct S<T>(T);\n\
                      impl<T> S<T> {\n    fn new(t: T) -> Self { Self(t) }\n}\n\
                      impl core::fmt::Display for S<u8> {\n    \
                      fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { \
                      if self.0 > 1 { write!(f, \"a\") } else { write!(f, \"b\") } }\n}\n\
                      trait T {\n    fn required(&self);\n    fn provided(&self) { for _ in 0..1 {} }\n}\n\
                      mod m {\n    fn free() {}\n}\n\
                      impl S<u16> {\n    fn method(&self) { fn helper() {} }\n}\n";
        let list = code::complexities(source).unwrap();
        let expected = [
            ("S::new", 3, 1),
            ("S::fmt", 6, 2),
            ("T::provided", 10, 2),
            ("free", 13, 1),
            ("S::method", 16, 1),
            ("helper", 16, 1),
        ];
        let actual: Vec<(&str, usize, usize)> = list
            .iter()
            .map(|f: &FunctionComplexity| (f.name.as_str(), f.line, f.complexity))
            .collect();
        assert_eq!(actual, expected, "names, lines and complexities");
        assert!(code::complexities("fn broken( {").is_err(), "a parse error");
    }

    /// Verifies REQ-058: the gate reads each `.rs` file of the workspace that git tracks
    /// (`src/`, `tests/`, `benches/`, `build.rs` and the same paths of each crate in `crates/`),
    /// also the test code (criterion 2).
    #[test]
    fn complexity_reads_tracked_files() {
        let fixture = Fixture::new();
        let read = [
            "src/main.rs",
            "src/deep/module.rs",
            "tests/cli.rs",
            "benches/speed.rs",
            "build.rs",
            "crates/core/src/lib.rs",
            "crates/core/tests/unit.rs",
            "crates/core/benches/bench.rs",
            "crates/core/build.rs",
        ];
        let not_read = [
            "docs/example.rs",
            "scripts/tool.rs",
            "crates/core/examples/demo.rs",
        ];
        for path in read.iter().chain(&not_read) {
            fixture.write(path, &function("complex", 16));
        }
        fixture.write_untracked("src/untracked.rs", &function("complex", 16));
        fixture.write("src/notes.txt", &function("complex", 16));
        let mut found: Vec<String> = fixture
            .check()
            .iter()
            .map(|v| v.path.to_string_lossy().into_owned())
            .collect();
        found.sort();
        let mut expected: Vec<String> = read.iter().map(|&p| p.to_owned()).collect();
        expected.sort();
        assert_eq!(found, expected, "the tracked files in the gated paths");
        let files = code::rust_files(&fixture.root).unwrap();
        assert_eq!(files, expected, "rust_files gives the same files");
        assert!(code::is_gated("crates/a/tests/t.rs"), "a crate test");
        assert!(!code::is_gated("crates/a/src/lib.txt"), "not a Rust file");
        assert!(
            !code::is_gated("crates/build.rs"),
            "not the build script of a crate"
        );
    }

    /// Verifies REQ-058: a function with a complexity above 15 that is not on the code baseline
    /// fails the gate, and the message gives the file, the line, the function name and the
    /// complexity (criterion 3).
    #[test]
    fn complexity_limit_fails() {
        let fixture = Fixture::new();
        fixture.write("src/ok.rs", &function("at_limit", 15));
        assert!(
            fixture.check().is_empty(),
            "complexity 15 passes: {}",
            show(&fixture.check())
        );
        let source = format!(
            "// The function starts on line 3.\n\n{}",
            function("too_complex", 16)
        );
        fixture.write("crates/core/src/bad.rs", &source);
        let violations = fixture.check();
        assert_eq!(violations.len(), 1, "one violation: {}", show(&violations));
        let violation = &violations[0];
        assert_eq!(violation.rule, COMPLEXITY, "the rule");
        assert_eq!(
            violation.path,
            Path::new("crates/core/src/bad.rs"),
            "the file"
        );
        assert_eq!(violation.line, 3, "the line");
        let message = &violation.message;
        for part in ["crates/core/src/bad.rs:3", "too_complex", "16"] {
            assert!(
                message.contains(part),
                "`{part}` in the message `{message}`"
            );
        }
        fixture.write("src/broken.rs", "fn broken( {");
        let parse = fixture.check();
        assert!(
            parse.iter().any(|v| v.path == Path::new("src/broken.rs")),
            "a file that cannot be parsed fails: {}",
            show(&parse)
        );
    }

    /// Verifies REQ-058: a baseline line `complexity`, file, function name (`Type::name` in an
    /// `impl` block, `name` otherwise) and complexity allows that function above 15 (criterion 4).
    /// A function above its baseline value fails the gate, and a line that matches no function
    /// above 15, or that has a value above the current complexity, fails the gate as stale
    /// (criterion 5).
    #[test]
    fn complexity_baseline_rules() {
        let fixture = Fixture::new();
        let method = function("run", 18).replace("fn run", "    fn run");
        fixture.write("src/lib.rs", &function("free", 20));
        fixture.write(
            "src/machine.rs",
            &format!("struct M;\nimpl M {{\n{method}}}\n"),
        );
        fixture.baseline(&[
            "complexity src/lib.rs free 20",
            "complexity src/machine.rs M::run 18",
        ]);
        assert!(
            fixture.check().is_empty(),
            "allowed: {}",
            show(&fixture.check())
        );
        // The bare name does not match a function in an `impl` block.
        fixture.baseline(&[
            "complexity src/lib.rs free 20",
            "complexity src/machine.rs run 18",
        ]);
        let rules = rules_of(&fixture.check());
        assert_eq!(rules, [COMPLEXITY, STALE], "M::run is not on the baseline");
        // More complex than the baseline value.
        fixture.baseline(&[
            "complexity src/lib.rs free 19",
            "complexity src/machine.rs M::run 18",
        ]);
        let violations = fixture.check();
        assert_eq!(
            rules_of(&violations),
            [COMPLEXITY],
            "above the baseline value"
        );
        assert!(
            violations[0].message.contains("free"),
            "{}",
            show(&violations)
        );
        // Less complex than the baseline value.
        fixture.baseline(&[
            "complexity src/lib.rs free 21",
            "complexity src/machine.rs M::run 18",
        ]);
        let violations = fixture.check();
        assert_eq!(
            rules_of(&violations),
            [STALE],
            "a value above the current complexity"
        );
        assert_eq!(
            violations[0].path,
            Path::new(BASELINE),
            "the stale line is in the baseline"
        );
        assert_eq!(violations[0].line, 2, "the line of the baseline");
        // A line that matches no function, and a line of a function at 15 or below.
        fixture.write("src/small.rs", &function("small", 15));
        fixture.baseline(&[
            "complexity src/lib.rs free 20",
            "complexity src/machine.rs M::run 18",
            "complexity src/lib.rs gone 30",
            "complexity src/small.rs small 15",
            "complexity src/small.rs",
        ]);
        let violations = fixture.check();
        assert_eq!(
            rules_of(&violations),
            [STALE, STALE, STALE],
            "{}",
            show(&violations)
        );
        let mut sorted: Vec<usize> = violations.iter().map(|v| v.line).collect();
        sorted.sort_unstable();
        assert_eq!(sorted, [4, 5, 6], "the stale lines and the malformed line");
    }

    /// The rules of a list of violations, sorted.
    fn rules_of(violations: &[CodeViolation]) -> Vec<&'static str> {
        let mut rules: Vec<&'static str> = violations.iter().map(|v| v.rule).collect();
        rules.sort_unstable();
        rules
    }

    /// The violations of the real repository whose rule is `rule`, or that are stale baseline
    /// lines of `rule`.
    fn repository_violations(rule: &str) -> Vec<CodeViolation> {
        let prefix = format!("`{rule} ");
        code::check(&repo_root())
            .into_iter()
            .filter(|v| v.rule == rule || (v.rule == STALE && v.message.contains(&prefix)))
            .collect()
    }

    /// Verifies REQ-058: the complexity gate passes on the repository with
    /// `docs/spec/code-baseline.txt`: no function above 15 that is not on the baseline
    /// (criterion 3), and no function above its baseline value and no stale line (criterion 5).
    #[test]
    fn complexity_gate_passes_on_repository() {
        let violations = repository_violations(COMPLEXITY);
        assert!(
            violations.is_empty(),
            "complexity gate:\n{}",
            show(&violations)
        );
        let baseline = fs::read_to_string(repo_root().join(BASELINE)).unwrap();
        let malformed: Vec<&str> = baseline
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter(|l| !l.starts_with("complexity ") && !l.starts_with("lines "))
            .collect();
        assert!(
            malformed.is_empty(),
            "malformed baseline lines: {malformed:?}"
        );
    }

    /// The prerequisites of a `Makefile` target, with the continuation lines joined.
    fn prerequisites<'a>(makefile: &'a str, target: &str) -> Vec<&'a str> {
        let start = format!("{target}:");
        let mut words = Vec::new();
        let mut inside = false;
        for line in makefile.lines() {
            if line.starts_with(&start) {
                inside = true;
                words.extend(line[start.len()..].split_whitespace());
            } else if inside {
                words.extend(line.split_whitespace());
            }
            if inside && !line.ends_with('\\') {
                break;
            }
        }
        words.retain(|w| *w != "\\");
        words
    }

    /// The recipe lines of a `Makefile` target.
    fn recipe<'a>(makefile: &'a str, target: &str) -> Vec<&'a str> {
        let start = format!("{target}:");
        makefile
            .lines()
            .skip_while(|line| !line.starts_with(&start))
            .skip(1)
            .take_while(|line| line.starts_with('\t'))
            .collect()
    }

    /// Verifies REQ-058: the code gates run in `make pre-commit`, in `make check` and in the CI
    /// job `Specification` (criterion 6).
    #[test]
    fn code_gates_in_spec_check() {
        let root = repo_root();
        let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
        let spec_check = recipe(&makefile, "spec-check");
        assert!(
            spec_check.iter().any(|l| l.contains("--test code_gates")),
            "`make spec-check` runs the code gates: {spec_check:?}"
        );
        for target in ["pre-commit", "check"] {
            let words = prerequisites(&makefile, target);
            assert!(
                words.contains(&"spec-check"),
                "`make {target}` runs spec-check: {words:?}"
            );
        }
        let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
        let job: Vec<&str> = ci
            .lines()
            .skip_while(|l| l.trim() != "name: Specification")
            .take_while(|l| !l.trim().starts_with("# Single required status check"))
            .collect();
        assert!(!job.is_empty(), "the CI job `Specification` exists");
        assert!(
            job.iter().any(|l| l.trim() == "- run: make spec-check"),
            "the CI job `Specification` runs `make spec-check`"
        );
    }

    /// Verifies REQ-059: the length of a file is its number of newline characters, plus 1 if the
    /// last line has no newline (criterion 1).
    #[test]
    fn file_length_counts_lines() {
        let cases = [
            ("", 0),
            ("a", 1),
            ("a\n", 1),
            ("a\nb", 2),
            ("a\nb\n", 2),
            ("\n", 1),
            ("\n\n", 2),
            ("a\r\nb\r\n", 2),
        ];
        for (text, expected) in cases {
            assert_eq!(code::file_lines(text), expected, "the length of {text:?}");
        }
        assert_eq!(code::file_lines(&lines(1500)), 1500, "1500 lines");
    }

    /// Verifies REQ-059: a file with more than 1500 lines that is not on the code baseline fails
    /// the gate, and the message gives the file and its length (criterion 2).
    #[test]
    fn file_length_limit_fails() {
        let fixture = Fixture::new();
        fixture.write("src/ok.rs", &lines(1500));
        fixture.write("tests/ok.rs", &format!("{}// no newline", lines(1499)));
        assert!(
            fixture.check().is_empty(),
            "1500 lines pass: {}",
            show(&fixture.check())
        );
        fixture.write(
            "crates/core/tests/long.rs",
            &format!("{}// no newline", lines(1500)),
        );
        let violations = fixture.check();
        assert_eq!(violations.len(), 1, "one violation: {}", show(&violations));
        let violation = &violations[0];
        assert_eq!(violation.rule, LINES, "the rule");
        assert_eq!(
            violation.path,
            Path::new("crates/core/tests/long.rs"),
            "the file"
        );
        let message = &violation.message;
        for part in ["crates/core/tests/long.rs", "1501"] {
            assert!(
                message.contains(part),
                "`{part}` in the message `{message}`"
            );
        }
    }

    /// Verifies REQ-059: a baseline line `lines`, file and length allows that file above 1500
    /// lines (criterion 3). A file longer than its baseline value fails the gate, and a line for
    /// a file that does not exist, that has 1500 lines or fewer, or that is shorter than its
    /// baseline value, fails the gate as stale (criterion 4).
    #[test]
    fn file_length_baseline_rules() {
        let fixture = Fixture::new();
        fixture.write("src/long.rs", &lines(1600));
        fixture.write("src/short.rs", &lines(1500));
        fixture.baseline(&["lines src/long.rs 1600"]);
        assert!(
            fixture.check().is_empty(),
            "allowed: {}",
            show(&fixture.check())
        );
        fixture.baseline(&["lines src/long.rs 1599"]);
        let violations = fixture.check();
        assert_eq!(
            rules_of(&violations),
            [LINES],
            "longer than the baseline value"
        );
        assert!(
            violations[0].message.contains("1600"),
            "{}",
            show(&violations)
        );
        fixture.baseline(&["lines src/long.rs 1601"]);
        let violations = fixture.check();
        assert_eq!(
            rules_of(&violations),
            [STALE],
            "shorter than the baseline value"
        );
        assert_eq!(
            violations[0].path,
            Path::new(BASELINE),
            "the stale line is in the baseline"
        );
        fixture.write_untracked("src/untracked.rs", &lines(1600));
        fixture.baseline(&[
            "lines src/long.rs 1600",
            "lines src/gone.rs 1600",
            "lines src/short.rs 1600",
            "lines src/untracked.rs 1600",
            "lines src/long.rs",
        ]);
        let violations = fixture.check();
        let mut sorted: Vec<usize> = violations.iter().map(|v| v.line).collect();
        assert_eq!(rules_of(&violations), [STALE; 4], "{}", show(&violations));
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            [3, 4, 5, 6],
            "a missing file, a short file, an untracked file and a malformed line"
        );
    }

    /// Verifies REQ-059: the file length gate passes on the repository with
    /// `docs/spec/code-baseline.txt`: no file above 1500 lines that is not on the baseline
    /// (criterion 2), and no file above its baseline value and no stale line (criterion 4).
    #[test]
    fn file_length_gate_passes_on_repository() {
        let violations = repository_violations(LINES);
        assert!(
            violations.is_empty(),
            "file length gate:\n{}",
            show(&violations)
        );
        let all = code::check(&repo_root());
        assert!(all.is_empty(), "code gates:\n{}", show(&all));
    }
}
