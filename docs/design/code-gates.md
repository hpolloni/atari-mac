# Code gates design

## Scope

The gates that check the Rust code itself, not the specification: the cyclomatic complexity
limit (REQ-058), the source file length limit (REQ-059), mutation testing of changed code
(REQ-060) and the property tests of the media parsers (REQ-061). The plan is SLICE-12.

## Complexity and file length gates (REQ-058, REQ-059)

The two gates are functions of a new module `atari_harness::code`, in the same way as the
specification gates of `atari_harness::spec` (`docs/design/spec-gates.md`, "Gates").

```rust
/// One function or file that a code gate rejects.
pub struct CodeViolation {
    /// `complexity`, `lines` or `stale`.
    pub rule: &'static str,
    /// The file, relative to the repository root.
    pub path: PathBuf,
    /// The line of the function (1 for a file).
    pub line: usize,
    /// The message (REQ-058 criterion 3, REQ-059 criterion 2).
    pub message: String,
}

/// The complexity of each function of one Rust source text (REQ-058 criterion 1).
pub fn complexities(source: &str) -> Result<Vec<FunctionComplexity>, syn::Error>;

/// Runs both gates on the repository at `root` with the baseline `docs/spec/code-baseline.txt`.
pub fn check(root: &Path) -> Vec<CodeViolation>;
```

- The parser is the `syn` crate (features `full`, `visit`, `parsing`) with the
  `span-locations` feature of `proc-macro2` for the line numbers. They are dependencies of
  `atari-harness` only. The core does not change (REQ-015).
- The files are the output of `git ls-files '*.rs'` at `root` (REQ-058 criterion 2). A file
  that `syn` cannot parse fails the gate with the parse error.
- A function is each `fn` item with a body: a free function, a function in an `impl` or a
  `trait` block, and a nested `fn`. The name of a function in an `impl` block is the type
  name without generics and a `::` (for a trait impl too). A trait default method uses the
  trait name.
- A macro body (for example `matches!` or `assert!`) is a token stream for `syn`. The gate
  does not count the decisions in it.
- The baseline line format is `<rule> <path> <name> <value>` for `complexity` and
  `<rule> <path> <value>` for `lines`, separated by one space. Lines that start with `#` are
  comments. The first version of the baseline has each function and file above the limit on
  the day that the gate merges.
- `crates/harness/tests/code_gates.rs` runs `check` on the repository, and tests each rule
  on small synthetic sources and files in a temporary directory.
- `make spec-check` also runs `--test code_gates`. Thus the gates run in `make pre-commit`,
  `make check` and the CI job `Specification` (REQ-058 criterion 6, REQ-059 criterion 5).

## Mutation testing (REQ-060)

- `make tools` installs `cargo-mutants` and `cargo-nextest` with pinned versions (`--locked`).
- `make mutants` runs `scripts/mutants.sh` with the base `git merge-base origin/main HEAD`
  (`MUTANTS_BASE=<commit>` selects another base). The script writes the diff of
  `git diff <base>` (the working tree included) to `target/mutants/mutants.diff` and runs
  `cargo mutants --in-diff` with the packages of `$(PACKAGES)`: `--workspace` on macOS,
  `--package atari-core --package atari-harness` on another host (cargo-mutants reads
  `--exclude` as a file pattern). When the diff adds or modifies no line of a `.rs` file, the
  script prints that and passes (criterion 5).
- The mutants are not made in the checkout of the user. The script makes a throwaway git
  worktree `target/mutants-wt` (it removes one that an earlier run left first): HEAD detached,
  the uncommitted changes applied (`git diff HEAD --binary`, so that the tree agrees with the
  diff), and the ROMs and the `test-data/` link of `scripts/worktree-setup.sh
  --no-fast-forward`. In it, `cargo mutants --in-place` changes one file at a time (a copy of
  the tree by cargo-mutants would not have the ROMs and `test-data/`, which are not in git). A
  trap removes the worktree at the end, also on a failure. A hard kill can only damage the
  throwaway worktree. The build directory `target/mutants/build` and the results
  `target/mutants/mutants.out` are in the main tree, so the build cache stays from one run to
  the next. In CI the checkout is throwaway too; the job uses the same script.
- `.cargo/mutants.toml` holds the settings: `exclude_re` (criterion 4), the timeouts, and the
  test options. A mutant of a function in `crates/atari-core` must run the system tests of
  `crates/harness` too, because most core tests are there (`test_package`; on macOS,
  `--test-workspace=true` adds the frontend tests).
- The test binaries `spec_gates`, `code_gates` and `traceability` of `atari-harness` do not run
  for a mutant (cargo-nextest with a filter expression; `cargo test` cannot leave out one test
  binary). They test the documents and the gates, and they read the source files that the
  mutant changes: a mutant that makes a function of the complexity baseline simpler would make
  the baseline entry stale, and the gate test would count the mutant as caught.
- The test binary `media_fuzz` of `atari-harness` (REQ-061) does not run for a mutant either.
  Each mutant runs all the tests, and media_fuzz alone takes more than a minute in the dev
  profile; the error paths of the parsers are also tested in `crates/harness/tests/media.rs`.
- A `timeout` is a pass (criterion 2): a mutant that makes the emulation loop forever is
  caught. The test timeout has a lower bound of 600 s (`minimum_test_timeout`). The baseline
  run of cargo-mutants runs only the tests of the mutated package (about 1 s for the core),
  but each mutant runs all the tests above (in the dev profile about 90 s for the core and the
  harness, about 210 s with the frontend tests on macOS). With a smaller bound, a mutant that
  no test catches runs into the timeout and passes instead of failing as `missed`.
- CI: the job `Mutants (cargo-mutants)` on Linux runs the gate on the diff from
  `github.event.before` to `github.sha` (on the first push, where `before` is all zeros: the
  diff of the last commit), with the packages of `--workspace --exclude atari-mac`. A pull
  request or a merge queue entry uses the diff from its base; a scheduled or manual run passes
  with a message. The output directory `target/mutants/mutants.out` is an artifact. The
  frontend mutants run only in `make check` on macOS.

## Media parser property tests (REQ-061)

- `crates/harness/tests/media_fuzz.rs`, with the `proptest` crate as a dev-dependency of
  `atari-harness`.
- Each test uses a `proptest` configuration with at least 1000 cases, a fixed seed, and no
  failure persistence file (REQ-061 criterion 4). The configuration is one helper function
  of the test file.
- Each format has two strategies: random bytes of 0 to 4096 bytes, and a valid header with
  random bytes after it (the header fields of REQ-604, REQ-605, REQ-625, REQ-639, REQ-612 and
  REQ-018).
- For an accepted input, the test builds a machine with the free ROMs (`roms/`, as the other
  harness tests do) and the media, and runs 10 frames (criterion 3).
- The tests run in `make test`, `make pre-commit` and the CI job `Test` (criterion 5). They do
  not run in `make coverage`, the CI job `Coverage` and the mutant runs (filter expressions of
  cargo-nextest): with the coverage counters they took more than 25 minutes in CI, and each
  mutant runs all the tests.
- A panic that a test finds is a defect: the package fixes the parser in the core and keeps
  the input as a normal test case in the same file.

## Test plan

| Requirement | Test |
| --- | --- |
| REQ-058 | `crates/harness/tests/code_gates.rs` `complexity_counts_each_decision` (criterion 1), `complexity_reads_tracked_files` (2), `complexity_limit_fails` (3), `complexity_baseline_rules` (4, 5), `complexity_gate_passes_on_repository` (3, 5), `code_gates_in_spec_check` (6) |
| REQ-059 | `crates/harness/tests/code_gates.rs` `file_length_counts_lines` (criterion 1), `file_length_limit_fails` (2), `file_length_baseline_rules` (3, 4), `file_length_gate_passes_on_repository` (2, 4) |
| REQ-060 | Inspection: a review of `make mutants`, `.cargo/mutants.toml` and the CI job against the acceptance criteria, with one run that has a planted missed mutant; evidence `docs/verification/REQ-060.md` |
| REQ-061 | `crates/harness/tests/media_fuzz.rs` `cartridge_parsers_never_panic`, `atr_parser_never_panics`, `cas_parser_never_panics`, `xex_parser_never_panics`, `rom_check_never_panics`, `load_state_never_panics` (criteria 1, 2, 3), `fuzz_inputs_are_deterministic` (4); the inputs of the panics that the tests found: `state_with_bad_audio_phase_is_an_error`, `state_with_bad_antic_cycle_is_an_error`, `state_with_bad_antic_count_is_an_error`, `state_with_bad_machine_count_is_an_error`, `state_with_bad_pokey_exit_cycle_is_an_error`, `state_with_bad_serial_bit_count_is_an_error`, `state_with_bad_keyboard_counter_is_an_error` (criteria 1, 3) |

## Open issues

None.
