//! Specification gates (`docs/design/spec-gates.md`, REQ-043 to REQ-055).
//!
//! Each gate runs on the real repository, where only the violations of
//! `docs/spec/gate-baseline.txt` are permitted, and on a small synthetic repository in a
//! temporary directory. The synthetic repository passes all gates; each test then breaks one
//! file and checks that the gate reports the rule.

/// The tests, the fixture and the helpers. The module has `cfg(test)`, so that the
/// helpers can use `unwrap` as the tests do.
#[cfg(test)]
mod gates {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};
    use std::{env, fs, process};

    use atari_harness::data::repo_root;
    use atari_harness::spec::{
        self, Baseline, MATRIX, OPEN_ITEM_INDEX, REQUIREMENT_INDEX, RULES, Spec, SpecError,
        Violation,
    };

    /// A gate as a function of the specification and the repository root.
    type Gate = fn(&Spec, &Path) -> Vec<Violation>;

    /// A number for each fixture, so that parallel tests use different directories.
    static FIXTURES: AtomicUsize = AtomicUsize::new(0);

    /// `REQ-NNN` for a number. The tests build IDs with this function, so that the source of
    /// this file names no requirement that does not exist (REQ-047).
    fn req(id: u16) -> String {
        format!("REQ-{id:03}")
    }

    /// Runs `gate` on the real repository. Fails for each violation that the baseline does not
    /// list, and for each baseline line of `rules` that matches no violation.
    fn repository_passes(gate: Gate, rules: &[u16]) {
        let root = repo_root();
        let spec = spec::load(&root).unwrap();
        let baseline = Baseline::read(&root).unwrap();
        let new: Vec<String> = baseline
            .filter(&gate(&spec, &root))
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(new.is_empty(), "new violations:\n{}", new.join("\n"));
        let stale = baseline.stale(&spec::check(&spec, &root), rules);
        assert!(
            stale.is_empty(),
            "baseline lines that match no violation:\n{}",
            stale.join("\n")
        );
    }

    /// The text of a requirement file of the fixture.
    fn requirement(id: u16, verification: &str, source: &str, depends_on: &str) -> String {
        let id = req(id);
        format!(
            "---\nid: {id}\ntitle: Title of {id}\nstatus: draft\npriority: must\narea: product\n\
             source: {source}\nverification: {verification}\ndepends_on: [{depends_on}]\n---\n\
             # {id}: Title of {id}\n\n## Requirement\n\nThe emulator shall do one thing.\n\n\
             ## Rationale\n\nA reason.\n\n## Acceptance criteria\n\n1. A test checks the value \
             `$FF`.\n\n## Notes\n\nA note.\n"
        )
    }

    /// A synthetic repository in a temporary directory. It is removed when dropped.
    struct Fixture {
        /// The root directory.
        root: PathBuf,
    }

    impl Fixture {
        /// A repository that passes all gates: three requirements, one open item, one slice
        /// with one `done` package, one evidence record, one design document and one test file.
        fn new() -> Self {
            let number = FIXTURES.fetch_add(1, Ordering::Relaxed);
            let name = format!("atari-spec-gates-{}-{number}", process::id());
            let root = env::temp_dir().join(name);
            let _removed = fs::remove_dir_all(&root);
            let fixture = Self { root };
            fixture.write_files();
            let _counts = spec::generate(&fixture.root).unwrap();
            fixture
        }

        /// Writes the files of the good repository.
        fn write_files(&self) {
            let (one, two, three) = (req(1), req(2), req(3));
            self.write(
                "docs/spec/README.md",
                &format!(
                    "# Specification\n\n## ID ranges\n\n| Range | Area | `area` value | Source |\n\
                     | --- | --- | --- | --- |\n| {one} to {three} | Product | `product` | Here |\n"
                ),
            );
            self.write(
                "docs/spec/vague-words.txt",
                "# Comment\ncorrect\nas expected\n",
            );
            self.write(
                "docs/spec/hrm-sections.txt",
                "# Header\n1.1\tIntroduction\n3.3\tCycle timing\n",
            );
            self.write(
                &format!("docs/spec/requirements/{one}.md"),
                &requirement(1, "test", "HRM 3.3 Cycle timing (Clock speed)", ""),
            );
            self.write(
                &format!("docs/spec/requirements/{two}.md"),
                &requirement(2, "inspection", "Project decision", &one),
            );
            self.write(
                &format!("docs/spec/requirements/{three}.md"),
                &requirement(3, "test", "HRM 1.1 Introduction; HRM 3.3 Cycle timing", ""),
            );
            self.write(
                "docs/spec/open-items/OI-001.md",
                &format!(
                    "---\nid: OI-001\ntitle: A question\nstatus: open\narea: [product]\n\
                     affects: [{one}, docs/design/demo.md]\nraised: \"2026-11-19, review\"\n---\n\
                     # OI-001: A question\n\n## Question\n\nWhat?\n\n## Decision\n\nNot decided.\n"
                ),
            );
            self.write(
                "docs/plan/README.md",
                "---\nproduct_complete: false\n---\n# Plan\n",
            );
            self.write(
                "docs/plan/SLICE-01.md",
                &format!(
                    "---\nid: SLICE-01\ntitle: First slice\nstatus: in-progress\npackages:\n  \
                     - id: WP-1\n    title: Work\n    status: done\n    requirements: [{one}, {two}]\n  \
                     - id: WP-2\n    status: planned\n    requirements: []\n---\n\
                     # SLICE-01: First slice\n\nSee [the design](../design/demo.md#test-plan).\n"
                ),
            );
            self.write(
                &format!("docs/verification/{two}.md"),
                &format!(
                    "---\nrequirement: {two}\ndate: 2026-12-01\ncommit: 0123abc\n\
                     method: inspection\nresult: pass\nby: lead\n---\n# Evidence\n\n\
                     ### Criterion 1\n\nSteps and result.\n"
                ),
            );
            self.write(
                "docs/design/demo.md",
                &format!(
                    "# Demo design\n\nIt uses [the README](../spec/README.md).\n\n## Test plan\n\n\
                     | Requirement | Test |\n| --- | --- |\n| {one} | `demo.rs` `first_test` |\n\
                     | {two} | `docs/verification/{two}.md` |\n| {three} | `demo.rs` `third_test` |\n\n\
                     ## Open issues\n\nNone.\n"
                ),
            );
            self.write(
                "crates/demo/tests/demo.rs",
                &format!(
                    "/// Verifies {one}.\n#[test]\nfn first_test() {{}}\n\n/// Verifies {three}.\n\
                     #[test]\n#[ignore = \"later\"]\nfn third_test() {{}}\n"
                ),
            );
        }

        /// Writes a file below the root.
        fn write(&self, rel: &str, text: &str) {
            let path = self.root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }

        /// Replaces `from` with `to` in a file. `from` must be in the file.
        fn replace(&self, rel: &str, from: &str, to: &str) {
            let text = fs::read_to_string(self.root.join(rel)).unwrap();
            assert!(text.contains(from), "{rel} does not contain {from:?}");
            self.write(rel, &text.replacen(from, to, 1));
        }

        /// The path of a requirement file.
        fn req_file(id: u16) -> String {
            format!("docs/spec/requirements/{}.md", req(id))
        }

        /// Loads the fixture.
        fn spec(&self) -> Spec {
            spec::load(&self.root).unwrap()
        }

        /// The violations of `gate` on the fixture.
        fn run(&self, gate: Gate) -> Vec<Violation> {
            gate(&self.spec(), &self.root)
        }

        /// The violations of all gates on the fixture.
        fn all(&self) -> Vec<Violation> {
            spec::check(&self.spec(), &self.root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _removed = fs::remove_dir_all(&self.root);
        }
    }

    /// Asserts that `violations` has one of `rule` whose message contains `needle`.
    #[track_caller]
    fn assert_reports(violations: &[Violation], rule: u16, needle: &str) {
        assert!(
            violations
                .iter()
                .any(|v| v.rule == rule && v.message.contains(needle)),
            "no REQ-{rule:03} violation with {needle:?} in:\n{}",
            violations
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// Breaks a fresh fixture with `edit`, and asserts that `gate` reports `rule` with `needle`.
    #[track_caller]
    fn rejects(gate: Gate, rule: u16, needle: &str, edit: impl FnOnce(&Fixture)) {
        let fixture = Fixture::new();
        edit(&fixture);
        assert_reports(&fixture.run(gate), rule, needle);
    }

    /// The `schema` gate.
    fn schema(spec: &Spec, _root: &Path) -> Vec<Violation> {
        spec::schema(spec)
    }

    /// The `structure` gate.
    fn structure(spec: &Spec, _root: &Path) -> Vec<Violation> {
        spec::structure(spec)
    }

    /// The `vague_words` gate.
    fn vague_words(spec: &Spec, _root: &Path) -> Vec<Violation> {
        spec::vague_words(spec)
    }

    /// The `sources` gate.
    fn sources(spec: &Spec, _root: &Path) -> Vec<Violation> {
        spec::sources(spec)
    }

    /// The `test_plans` gate.
    fn test_plans(spec: &Spec, _root: &Path) -> Vec<Violation> {
        spec::test_plans(spec)
    }

    /// The `trace` gate.
    fn trace(spec: &Spec, _root: &Path) -> Vec<Violation> {
        spec::trace(spec)
    }

    /// The `slices` gate.
    fn slices(spec: &Spec, _root: &Path) -> Vec<Violation> {
        spec::slices(spec)
    }

    /// The `evidence` gate.
    fn evidence(spec: &Spec, _root: &Path) -> Vec<Violation> {
        spec::evidence(spec)
    }

    /// The `schema` and `open_items` gates.
    fn open_items(spec: &Spec, root: &Path) -> Vec<Violation> {
        let mut all = spec::schema(spec);
        all.extend(spec::open_items(spec, root));
        all
    }

    /// The `schema` and `slices` gates.
    fn slice_rules(spec: &Spec, _root: &Path) -> Vec<Violation> {
        let mut all = spec::schema(spec);
        all.extend(spec::slices(spec));
        all
    }

    /// The good fixture passes all gates, so the bad-file tests do not find false violations.
    #[track_caller]
    fn fixture_passes() {
        let violations = Fixture::new().all();
        assert!(
            violations.is_empty(),
            "the good fixture has violations:\n{}",
            violations
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// Verifies REQ-043: the requirement frontmatter of the repository passes the schema check.
    #[test]
    fn schema_gate_passes_on_repository() {
        repository_passes(schema, &[43, 53, 54]);
    }

    /// Verifies REQ-043: each rule of the schema check rejects a bad file, with the file and
    /// the rule in the message.
    #[test]
    fn schema_gate_rejects_bad_files() {
        fixture_passes();
        let one = Fixture::req_file(1);
        rejects(schema, 43, "missing key `priority`", |f| {
            f.replace(&one, "priority: must\n", "");
        });
        rejects(schema, 43, "unknown key `owner`", |f| {
            f.replace(&one, "priority: must\n", "priority: must\nowner: lead\n");
        });
        rejects(schema, 43, "`status` is `done`, not one of", |f| {
            f.replace(&one, "status: draft", "status: done");
        });
        rejects(schema, 43, "`priority` is `high`", |f| {
            f.replace(&one, "priority: must", "priority: high");
        });
        rejects(schema, 43, "`verification` is `review`", |f| {
            f.replace(&one, "verification: test", "verification: review");
        });
        rejects(schema, 43, "`area` `space`", |f| {
            f.replace(&one, "area: product", "area: space");
        });
        rejects(schema, 43, "but the file is", |f| {
            f.replace(&one, &format!("id: {}", req(1)), &format!("id: {}", req(3)));
        });
        rejects(schema, 43, "is also the ID of", |f| {
            f.replace(&one, &format!("id: {}", req(1)), &format!("id: {}", req(3)));
        });
        rejects(
            schema,
            43,
            "`title` is not the text of the first heading",
            |f| {
                f.replace(&one, "title: Title", "title: Other title");
            },
        );
        rejects(schema, 43, "`depends_on` is not a list", |f| {
            f.replace(&one, "depends_on: []", "depends_on: none");
        });
        rejects(schema, 43, "is not a requirement ID", |f| {
            f.replace(&one, "depends_on: []", "depends_on: [REQ-1x]");
        });
        let missing = format!("`depends_on` {} does not exist", req(999));
        rejects(schema, 43, &missing, |f| {
            f.replace(
                &one,
                "depends_on: []",
                &format!("depends_on: [{}]", req(999)),
            );
        });
        rejects(schema, 43, "is the requirement itself", |f| {
            f.replace(&one, "depends_on: []", &format!("depends_on: [{}]", req(1)));
        });
        rejects(schema, 43, "is withdrawn", |f| {
            f.replace(&Fixture::req_file(3), "status: draft", "status: withdrawn");
            f.replace(&one, "depends_on: []", &format!("depends_on: [{}]", req(3)));
        });
        let cycle = format!("cycle: {} -> {} -> {}", req(1), req(2), req(1));
        rejects(schema, 43, &cycle, |f| {
            f.replace(&one, "depends_on: []", &format!("depends_on: [{}]", req(2)));
        });
        rejects(schema, 43, "no YAML frontmatter", |f| {
            f.write(&one, &format!("# {}: Title\n", req(1)));
        });
        rejects(schema, 43, "`title` is not a string", |f| {
            f.replace(&one, "title: Title of", "title: [Title]\nx: Title of");
        });
        let fixture = Fixture::new();
        fixture.replace(&one, "status: draft", "status: [draft");
        let error = spec::load(&fixture.root).unwrap_err();
        assert!(
            matches!(&error, SpecError::Yaml { path, line, .. }
                if path.ends_with(&one) && *line > 1),
            "a YAML error gives the file and the line: {error}"
        );
        assert!(error.to_string().contains(&one), "message names the file");
    }

    /// Verifies REQ-044: the requirement files of the repository have the required structure.
    #[test]
    fn structure_gate_passes_on_repository() {
        repository_passes(structure, &[44]);
    }

    /// Verifies REQ-044: a missing or misplaced section, a statement without `shall`, criteria
    /// without `1. ` and an unknown heading each fail.
    #[test]
    fn structure_gate_rejects_bad_files() {
        fixture_passes();
        let one = Fixture::req_file(1);
        rejects(structure, 44, "`## Rationale` is missing", |f| {
            f.replace(&one, "## Rationale\n\nA reason.\n\n", "");
        });
        rejects(structure, 44, "are not in this order", |f| {
            f.replace(&one, "## Rationale\n\nA reason.\n\n", "");
            f.replace(&one, "## Notes", "## Rationale\n\nA reason.\n\n## Notes");
        });
        rejects(structure, 44, "has no `shall`", |f| {
            f.replace(&one, "The emulator shall do", "The emulator does");
        });
        rejects(structure, 44, "no line that starts with `1. `", |f| {
            f.replace(&one, "1. A test", "- A test");
        });
        rejects(structure, 44, "`## Extra` is not permitted", |f| {
            f.replace(&one, "## Notes", "## Extra\n\nText.\n\n## Notes");
        });
        let fixture = Fixture::new();
        fixture.replace(&one, "status: draft", "status: withdrawn");
        fixture.replace(&one, "## Rationale\n\nA reason.\n\n", "");
        assert!(
            fixture.run(structure).is_empty(),
            "a withdrawn requirement is not checked"
        );
    }

    /// Verifies REQ-045: the requirement statements and criteria of the repository have no
    /// vague words.
    #[test]
    fn vague_words_gate_passes_on_repository() {
        repository_passes(vague_words, &[45]);
        let words = fs::read_to_string(repo_root().join("docs/spec/vague-words.txt")).unwrap();
        let words: Vec<&str> = words.lines().filter(|l| !l.starts_with('#')).collect();
        for word in [
            "correct",
            "correctly",
            "properly",
            "appropriate",
            "as expected",
            "reasonable",
            "etc",
            "and so on",
            "user-friendly",
            "fast",
            "quickly",
        ] {
            assert!(words.contains(&word), "the list has {word:?}");
        }
    }

    /// Verifies REQ-045: a vague word fails with the file, the criterion number and the word; a
    /// word in backticks or inside another word does not.
    #[test]
    fn vague_words_gate_rejects_bad_files() {
        fixture_passes();
        let one = Fixture::req_file(1);
        rejects(vague_words, 45, "criterion 2: vague word `correct`", |f| {
            f.replace(&one, "`$FF`.\n", "`$FF`.\n2. The value is Correct.\n");
        });
        rejects(
            vague_words,
            45,
            "requirement statement: vague word `as expected`",
            |f| f.replace(&one, "one thing.", "one thing as expected."),
        );
        let fixture = Fixture::new();
        fixture.replace(&one, "`$FF`.", "`correct` and incorrect values.");
        let found = fixture.run(vague_words);
        assert!(found.is_empty(), "code spans and other words: {found:?}");
        let fixture = Fixture::new();
        fixture.replace(&one, "1. A test", "1. A correct test");
        let found = fixture.run(vague_words);
        assert!(
            found
                .iter()
                .all(|v| v.file.ends_with(&one) && v.line.is_some()),
            "the message gives the file and the line: {found:?}"
        );
    }

    /// Verifies REQ-046: the sources of the repository cite sections of the HRM list, or are
    /// project decisions with notes.
    #[test]
    fn sources_gate_passes_on_repository() {
        repository_passes(sources, &[46]);
        let list = fs::read_to_string(repo_root().join("docs/spec/hrm-sections.txt")).unwrap();
        for section in [
            "2.5\tPeripheral Interface Adapter (PIA)",
            "4.9\tWSYNC",
            "D.2\t",
        ] {
            assert!(list.contains(section), "the HRM list has {section:?}");
        }
    }

    /// Verifies REQ-046: an unknown HRM section, a source that is neither HRM nor `Project
    /// decision`, and a project decision without notes each fail.
    #[test]
    fn sources_gate_rejects_bad_files() {
        fixture_passes();
        let one = Fixture::req_file(1);
        let two = Fixture::req_file(2);
        rejects(sources, 46, "cites HRM 9.9, which is not in", |f| {
            f.replace(&one, "HRM 3.3", "HRM 9.9");
        });
        rejects(
            sources,
            46,
            "names no HRM section and is not `Project decision`",
            |f| {
                f.replace(
                    &one,
                    "source: HRM 3.3 Cycle timing (Clock speed)",
                    "source: A book",
                );
            },
        );
        rejects(sources, 46, "no `Notes` section", |f| {
            f.replace(&two, "\n## Notes\n\nA note.\n", "");
        });
    }

    /// Verifies REQ-047: the references and links of the repository resolve.
    #[test]
    fn references_gate_passes_on_repository() {
        repository_passes(spec::references, &[47]);
    }

    /// Verifies REQ-047: a reference to a missing file, a range end that does not exist, a
    /// withdrawn requirement outside the records and a broken link each fail with the file, the
    /// line and the reference.
    #[test]
    fn references_gate_rejects_bad_files() {
        fixture_passes();
        let design = "docs/design/demo.md";
        let missing = format!("`{}` names no file", req(999));
        rejects(spec::references, 47, &missing, |f| {
            f.replace(
                design,
                "# Demo design\n",
                &format!("# Demo design\n\nSee {}.\n", req(999)),
            );
        });
        let range_end = format!("`{}` names no file", req(9));
        rejects(spec::references, 47, &range_end, |f| {
            let range = format!("{} to {}", req(1), req(9));
            f.replace(
                design,
                "# Demo design\n",
                &format!("# Demo design\n\n{range}.\n"),
            );
        });
        rejects(spec::references, 47, "`OI-002` names no file", |f| {
            f.replace(
                design,
                "# Demo design\n",
                "# Demo design\n\nSee OI-002 and SLICE-01.\n",
            );
        });
        let slice = format!("SLICE-{:02}", 2);
        rejects(
            spec::references,
            47,
            &format!("`{slice}` names no file"),
            |f| {
                f.write(
                    "crates/demo/src/lib.rs",
                    &format!("//! Planned in {slice}.\n"),
                );
            },
        );
        rejects(spec::references, 47, "the link `../missing.md`", |f| {
            f.replace(
                design,
                "# Demo design\n",
                "# Demo design\n\n[x](../missing.md)\n",
            );
        });
        let withdrawn = format!("`{}` is withdrawn", req(3));
        rejects(spec::references, 47, &withdrawn, |f| {
            f.replace(&Fixture::req_file(3), "status: draft", "status: withdrawn");
            f.replace(
                design,
                "# Demo design\n",
                &format!("# Demo design\n\nUse {}.\n", req(3)),
            );
        });
        let fixture = Fixture::new();
        fixture.replace(&Fixture::req_file(3), "status: draft", "status: withdrawn");
        let note = format!(
            "# Demo design\n\n{} is withdrawn. Family REQ-2xx.\n",
            req(3)
        );
        fixture.replace(design, "# Demo design\n", &note);
        let found: Vec<Violation> = fixture
            .run(spec::references)
            .into_iter()
            .filter(|v| v.line == Some(3))
            .collect();
        assert!(found.is_empty(), "permitted references: {found:?}");
        let fixture = Fixture::new();
        fixture.replace(
            design,
            "# Demo design\n",
            &format!("# Demo design\n\n{}\n", req(999)),
        );
        let found = fixture.run(spec::references);
        assert!(
            found
                .iter()
                .any(|v| v.file.ends_with(design) && v.line == Some(3)),
            "the message gives the file and the line: {found:?}"
        );
    }

    /// Verifies REQ-048: each requirement of the repository is in exactly one test plan, with
    /// a test or an evidence record.
    #[test]
    fn test_plans_gate_passes_on_repository() {
        repository_passes(test_plans, &[48]);
    }

    /// Verifies REQ-048: a requirement in no test plan or in two, a `test` row without a test
    /// and an `inspection` row without the evidence file each fail.
    #[test]
    fn test_plans_gate_rejects_bad_files() {
        fixture_passes();
        let design = "docs/design/demo.md";
        let one = req(1);
        rejects(
            test_plans,
            48,
            &format!("{one} is in no design test plan"),
            |f| {
                f.replace(design, &format!("| {one} | `demo.rs` `first_test` |\n"), "");
            },
        );
        rejects(
            test_plans,
            48,
            &format!("{one} is in the test plans of"),
            |f| {
                f.write(
                "docs/design/other.md",
                &format!("# Other\n\n## Test plan\n\n| Requirement | Test |\n| --- | --- |\n| {one} | `x.rs` `other_test` |\n"),
            );
            },
        );
        rejects(test_plans, 48, "names no test in backticks", |f| {
            f.replace(design, "`demo.rs` `first_test`", "the first test");
        });
        let two = req(2);
        rejects(
            test_plans,
            48,
            &format!("does not name docs/verification/{two}.md"),
            |f| {
                f.replace(design, &format!("`docs/verification/{two}.md`"), "a review");
            },
        );
    }

    /// Verifies REQ-027 and REQ-049: the trace from the test plans to the code and back passes
    /// on the repository.
    #[test]
    fn trace_gate_passes_on_repository() {
        repository_passes(trace, &[27, 49]);
    }

    /// Verifies REQ-027 and REQ-049: bad `Verifies` comments, a test that its test plan does
    /// not name, and the rules for a `done` package each fail.
    #[test]
    fn trace_gate_rejects_bad_files() {
        fixture_passes();
        let code = "crates/demo/tests/demo.rs";
        let design = "docs/design/demo.md";
        rejects(
            trace,
            27,
            "`helper`: a `Verifies` comment is not on a #[test]",
            |f| {
                f.write(
                    "crates/demo/src/lib.rs",
                    &format!("//! Lib.\n\n/// Verifies {}.\nfn helper() {{}}\n", req(1)),
                );
            },
        );
        let missing = format!("names {}, which does not exist", req(999));
        rejects(trace, 27, &missing, |f| {
            f.replace(
                code,
                &format!("Verifies {}.", req(1)),
                &format!("Verifies {}.", req(999)),
            );
        });
        rejects(trace, 27, "which is withdrawn", |f| {
            f.replace(&Fixture::req_file(3), "status: draft", "status: withdrawn");
        });
        let not_named = format!("`first_test` verifies {}, but the test plan", req(1));
        rejects(trace, 49, &not_named, |f| {
            f.replace(design, "`first_test`", "`first_test_renamed`");
        });
        rejects(trace, 27, "no test that runs names", |f| {
            f.replace(
                "docs/plan/SLICE-01.md",
                &format!("[{}, {}]", req(1), req(2)),
                &format!("[{}, {}, {}]", req(1), req(2), req(3)),
            );
        });
        rejects(trace, 27, "no test that runs names", |f| {
            f.replace(
                code,
                "#[test]\nfn first_test",
                "#[test]\n#[ignore = \"x\"]\nfn first_test",
            );
        });
        rejects(
            trace,
            49,
            "the test plan names `second_test` in demo.rs, which is not",
            |f| {
                f.replace(
                    design,
                    "`demo.rs` `first_test`",
                    "`demo.rs` `first_test`, `second_test`",
                );
            },
        );
        rejects(
            trace,
            49,
            "the test `second_test` in demo.rs has no `Verifies` comment",
            |f| {
                f.replace(
                    design,
                    "`demo.rs` `first_test`",
                    "`demo.rs` `first_test`, `second_test`",
                );
                f.replace(
                    code,
                    "\n\n/// Verifies",
                    "\n\n#[test]\nfn second_test() {}\n\n/// Verifies",
                );
            },
        );
        let fixture = Fixture::new();
        fixture.replace(
            "docs/plan/SLICE-01.md",
            "status: done",
            "status: in-progress",
        );
        fixture.replace(
            design,
            "`demo.rs` `first_test`",
            "`demo.rs` `first_test`, `second_test`",
        );
        assert!(
            fixture.run(trace).is_empty(),
            "the test plan rules apply only to `done` packages"
        );
    }

    /// A fixture whose test `first_test` (REQ-001, a `done` package) is in the file `code`
    /// and whose test plan names it in the file `plan`.
    fn moved_test(code: &str, plan: &str) -> Fixture {
        let fixture = Fixture::new();
        fixture.replace(
            "crates/demo/tests/demo.rs",
            &format!("/// Verifies {}.\n#[test]\nfn first_test() {{}}\n\n", req(1)),
            "",
        );
        fixture.write(
            code,
            &format!("/// Verifies {}.\n#[test]\nfn first_test() {{}}\n", req(1)),
        );
        fixture.replace(
            "docs/design/demo.md",
            "`demo.rs` `first_test`",
            &format!("`{plan}` `first_test`"),
        );
        fixture
    }

    /// Asserts that the `trace` gate reports `first_test` in both directions: the test plan
    /// names a test that is not found, and the test is not in its test plan.
    #[track_caller]
    fn first_test_not_matched(fixture: &Fixture, plan: &str) {
        let violations = fixture.run(trace);
        assert_reports(
            &violations,
            49,
            &format!("the test plan names `first_test` in {plan}, which is not"),
        );
        assert_reports(
            &violations,
            49,
            &format!("`first_test` verifies {}, but the test plan", req(1)),
        );
    }

    /// Verifies REQ-049: a file name without a directory in a test plan names a module, so a
    /// test in `x/tests/y.rs` matches the plan file `x.rs` in both directions.
    #[test]
    fn trace_gate_matches_submodule_files() {
        for code in [
            "crates/demo/src/x/tests/y.rs",
            "crates/demo/src/x.rs",
            "crates/demo/tests/x/y.rs",
        ] {
            let fixture = moved_test(code, "x.rs");
            let violations = fixture.run(trace);
            assert!(
                violations.is_empty(),
                "{code} with x.rs: {:?}",
                violations
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            );
        }
    }

    /// Verifies REQ-049: a test in `x/tests/y.rs` does not match the plan file `z.rs`, and a
    /// crate directory is not a module directory.
    #[test]
    fn trace_gate_rejects_other_modules() {
        first_test_not_matched(&moved_test("crates/demo/src/x/tests/y.rs", "z.rs"), "z.rs");
        first_test_not_matched(&moved_test("crates/x/src/lib.rs", "x.rs"), "x.rs");
        first_test_not_matched(&moved_test("crates/demo/src/zx/y.rs", "x.rs"), "x.rs");
    }

    /// Verifies REQ-049: a plan file with a directory matches that file only, so a test in
    /// `other/x.rs` does not match the plan file `crates/a/x.rs`.
    #[test]
    fn trace_gate_matches_paths_exactly() {
        let plan = "crates/a/x.rs";
        first_test_not_matched(&moved_test("crates/demo/src/other/x.rs", plan), plan);
        first_test_not_matched(&moved_test("crates/a/x/tests/y.rs", plan), plan);
        let fixture = moved_test("crates/a/x.rs", plan);
        assert!(fixture.run(trace).is_empty(), "{plan} matches itself");
    }

    /// Verifies REQ-050: the matrix lists each requirement that no slice plans.
    #[test]
    fn unplanned_requirements_are_reported() {
        let root = repo_root();
        let spec = spec::load(&root).unwrap();
        let report = spec::matrix(&spec, &[], &Baseline::default());
        let section = report.split("## Not in a slice").nth(1).unwrap();
        let section = section.split("\n## ").next().unwrap();
        for r in spec.requirements.iter().filter(|r| r.active()) {
            let listed = section.contains(&format!("- {}:", req(r.id)));
            assert_eq!(
                listed,
                spec.planned(r.id).is_none(),
                "{} in the section",
                req(r.id)
            );
        }
        let fixture = Fixture::new();
        let report = fs::read_to_string(fixture.root.join(MATRIX)).unwrap();
        assert!(
            report.contains(&format!("## Not in a slice\n\n- {}: Title of", req(3))),
            "fixture matrix:\n{report}"
        );
    }

    /// Verifies REQ-050: with `product_complete: true`, a `must` requirement that no slice
    /// plans fails.
    #[test]
    fn product_complete_needs_must_planned() {
        fixture_passes();
        let plan = "docs/plan/README.md";
        let three = req(3);
        rejects(
            slices,
            50,
            &format!("{three} (`must`) is in no slice"),
            |f| {
                f.replace(plan, "false", "true");
            },
        );
        rejects(
            slices,
            50,
            "`product_complete` is not `true` or `false`",
            |f| {
                f.replace(plan, "false", "maybe");
            },
        );
        let fixture = Fixture::new();
        fixture.replace(plan, "false", "true");
        fixture.replace(&Fixture::req_file(3), "priority: must", "priority: should");
        assert!(
            fixture.run(slices).is_empty(),
            "a `should` requirement can be unplanned"
        );
    }

    /// Verifies REQ-051: the evidence records of the repository are valid and complete.
    #[test]
    fn evidence_gate_passes_on_repository() {
        repository_passes(evidence, &[51]);
    }

    /// Verifies REQ-051: a missing record, a failed result and bad record fields each fail.
    #[test]
    fn evidence_gate_rejects_bad_files() {
        fixture_passes();
        let two = req(2);
        let record = format!("docs/verification/{two}.md");
        rejects(
            evidence,
            51,
            &format!("docs/verification/{two}.md does not exist"),
            |f| {
                fs::remove_file(f.root.join(&record)).unwrap();
            },
        );
        rejects(evidence, 51, "has `result: fail`", |f| {
            f.replace(&record, "result: pass", "result: fail");
        });
        rejects(evidence, 51, "is not an ISO 8601 date", |f| {
            f.replace(&record, "2026-12-01", "1 December 2026");
        });
        rejects(evidence, 51, "missing key `by`", |f| {
            f.replace(&record, "by: lead\n", "");
        });
        rejects(evidence, 51, "`method` is `demonstration`", |f| {
            f.replace(&record, "method: inspection", "method: demonstration");
        });
        rejects(evidence, 51, "`method` is `test`", |f| {
            f.replace(&record, "method: inspection", "method: test");
        });
        rejects(evidence, 51, "the body has 0 subsections", |f| {
            f.replace(&record, "### Criterion 1\n", "");
        });
        rejects(evidence, 51, "is not the ID in the file name", |f| {
            f.replace(
                &record,
                &format!("requirement: {two}"),
                &format!("requirement: {}", req(1)),
            );
        });
    }

    /// Verifies REQ-052: the matrix has one row for each requirement that is not withdrawn,
    /// with its slice, package, state, tests and evidence, and the counts and violations.
    #[test]
    fn matrix_has_one_row_per_requirement() {
        let root = repo_root();
        let spec = spec::load(&root).unwrap();
        let report = spec::matrix(
            &spec,
            &spec::check(&spec, &root),
            &Baseline::read(&root).unwrap(),
        );
        assert!(
            report.contains(
                "| Requirement | Verification | Slice and package | State | Tests | Evidence |"
            ),
            "header"
        );
        let table = report.split("\n## ").next().unwrap();
        for r in &spec.requirements {
            let rows = table
                .lines()
                .filter(|l| l.starts_with(&format!("| {} |", req(r.id))))
                .count();
            assert_eq!(rows, usize::from(r.active()), "rows of {}", req(r.id));
        }
        assert!(report.contains("\nCounts: "), "counts line");
        assert!(report.contains("\n## Violations\n"), "violations section");
        let fixture = Fixture::new();
        let report = fs::read_to_string(fixture.root.join(MATRIX)).unwrap();
        let (one, two, three) = (req(1), req(2), req(3));
        for row in [
            format!(
                "| {one} | test | SLICE-01 WP-1 | tested | crates/demo/tests/demo.rs `first_test` |  |"
            ),
            format!(
                "| {two} | inspection | SLICE-01 WP-1 | not a test requirement |  | docs/verification/{two}.md (pass) |"
            ),
            format!(
                "| {three} | test | not in a slice | pending | crates/demo/tests/demo.rs `third_test` (ignored) |  |"
            ),
        ] {
            assert!(report.contains(&row), "row {row} in:\n{report}");
        }
        assert!(
            report.contains("Violations: 0 new, 0 in the baseline."),
            "counts: {report}"
        );
        fixture.replace(&Fixture::req_file(1), "status: draft", "status: withdrawn");
        let spec = fixture.spec();
        let violations = spec::check(&spec, &fixture.root);
        let baseline = Baseline::parse(&violations.first().unwrap().key());
        let report = spec::matrix(&spec, &violations, &baseline);
        assert!(
            !report.contains(&format!("| {one} |")),
            "no row for a withdrawn requirement"
        );
        assert!(report.contains("in the baseline."), "baseline count");
        assert!(
            report.contains("| Rule | File | Line | Message |"),
            "violation table"
        );
    }

    /// Verifies REQ-053: an open item with bad frontmatter or a `Decision` section that does
    /// not agree with its status fails.
    #[test]
    fn open_item_gate_rejects_bad_files() {
        fixture_passes();
        repository_passes(open_items, &[53]);
        let item = "docs/spec/open-items/OI-001.md";
        rejects(open_items, 53, "`status` is `closed`", |f| {
            f.replace(item, "status: open", "status: closed");
        });
        rejects(open_items, 53, "missing key `affects`", |f| {
            f.replace(item, "affects:", "touches:");
        });
        rejects(
            open_items,
            53,
            "`affects` item `docs/design/missing.md` does not exist",
            |f| {
                f.replace(item, "docs/design/demo.md", "docs/design/missing.md");
            },
        );
        let missing = format!("`affects` item `{}` does not exist", req(999));
        rejects(open_items, 53, &missing, |f| {
            f.replace(item, &req(1), &req(999));
        });
        rejects(
            open_items,
            53,
            "is `decided`, but the `Decision` section has no decision",
            |f| {
                f.replace(item, "status: open", "status: decided");
            },
        );
        rejects(
            open_items,
            53,
            "is `open`, but the `Decision` section has a decision",
            |f| {
                f.replace(item, "Not decided.", "Option 1.");
            },
        );
        let fixture = Fixture::new();
        fixture.replace(item, "status: open", "status: decided");
        fixture.replace(item, "Not decided.", "Option 1.");
        assert!(
            fixture.run(open_items).is_empty(),
            "a decided item with a decision"
        );
    }

    /// Verifies REQ-053: the generated indexes of the repository are up to date, and a stale
    /// index fails.
    #[test]
    fn indexes_are_up_to_date() {
        let root = repo_root();
        let spec = spec::load(&root).unwrap();
        let stale: Vec<String> = spec::indexes(&spec, &root)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(stale.is_empty(), "{}", stale.join("\n"));
        for path in [REQUIREMENT_INDEX, OPEN_ITEM_INDEX] {
            let fixture = Fixture::new();
            fixture.write(path, "# Old\n");
            assert_reports(&fixture.run(spec::indexes), 53, "not up to date");
            let _counts = spec::generate(&fixture.root).unwrap();
            assert!(
                fixture.run(spec::indexes).is_empty(),
                "{path} is generated again"
            );
        }
        let fixture = Fixture::new();
        fixture.replace(
            &Fixture::req_file(1),
            "title: Title of",
            "title: New title of",
        );
        let found = fixture.run(spec::indexes);
        assert!(
            found.iter().any(|v| v.file == Path::new(REQUIREMENT_INDEX)),
            "a changed title makes the index stale: {found:?}"
        );
    }

    /// Verifies REQ-054: a slice with bad frontmatter, an unknown or duplicated requirement, or
    /// a `done` status with an unfinished package fails.
    #[test]
    fn slice_gate_rejects_bad_files() {
        fixture_passes();
        let slice = "docs/plan/SLICE-01.md";
        let (one, two) = (req(1), req(2));
        rejects(slice_rules, 54, "`status` is `started`", |f| {
            f.replace(slice, "status: in-progress", "status: started");
        });
        rejects(slice_rules, 54, "missing key `packages`", |f| {
            f.replace(slice, "packages:", "work:");
        });
        rejects(slice_rules, 54, "missing key `requirements`", |f| {
            f.replace(slice, "    requirements: []\n", "");
        });
        rejects(slice_rules, 54, "is not a requirement ID", |f| {
            f.replace(
                slice,
                &format!("[{one}, {two}]"),
                &format!("[{one} to {two}]"),
            );
        });
        let unknown = format!("SLICE-01 WP-1 names {}, which does not exist", req(999));
        rejects(slice_rules, 54, &unknown, |f| {
            f.replace(
                slice,
                &format!("[{one}, {two}]"),
                &format!("[{one}, {}]", req(999)),
            );
        });
        let twice = format!("{one} is in more than one package: SLICE-01 WP-1, SLICE-01 WP-2");
        rejects(slice_rules, 54, &twice, |f| {
            f.replace(slice, "requirements: []", &format!("requirements: [{one}]"));
        });
        rejects(
            slice_rules,
            54,
            "the slice is done, but WP-2 is `planned`",
            |f| {
                f.replace(slice, "status: in-progress", "status: done");
            },
        );
        rejects(slice_rules, 54, "`packages` is not a list", |f| {
            f.write(
                slice,
                "---\nid: SLICE-01\ntitle: T\nstatus: planned\npackages: none\n---\n# SLICE-01: T\n",
            );
        });
    }

    /// Verifies REQ-055: all gates run on the repository in no more than 10 seconds, and the
    /// baseline has no line that matches no violation.
    #[test]
    fn gates_run_within_10_seconds() {
        let root = repo_root();
        let start = Instant::now();
        let spec = spec::load(&root).unwrap();
        let violations = spec::check(&spec, &root);
        let elapsed = start.elapsed();
        assert!(
            elapsed <= Duration::from_secs(10),
            "the gates took {elapsed:?}"
        );
        let baseline = Baseline::read(&root).unwrap();
        let new: Vec<String> = baseline
            .filter(&violations)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(new.is_empty(), "new violations:\n{}", new.join("\n"));
        let stale = baseline.stale(&violations, RULES);
        assert!(
            stale.is_empty(),
            "stale baseline lines:\n{}",
            stale.join("\n")
        );
        assert!(
            baseline.unknown_rules().is_empty(),
            "baseline lines without a rule"
        );
        let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
        assert!(
            makefile.contains("spec-check:\n\tcargo test -p atari-harness --locked --test spec_gates --test traceability"),
            "make spec-check runs the gates"
        );
    }
}
