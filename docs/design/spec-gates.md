# Specification gates design

## Scope

This document gives the design of the gates that check the specification, the plan and the
trace from each requirement to its verification. It covers REQ-027 and REQ-043 to REQ-055.

## File formats

All metadata moves from markdown tables into YAML frontmatter (REQ-043, REQ-053, REQ-054).
The text below the frontmatter does not change, except that the metadata table is removed.

### Requirement (`docs/spec/requirements/REQ-NNN.md`)

```yaml
---
id: REQ-110
title: Klaus Dormann functional test
status: draft            # draft | accepted | withdrawn
priority: must           # must | should | could
area: cpu                # product | frontend | test-harness | quality | cpu | system |
                         # antic | gtia | pokey | media
source: HRM 3.3 Cycle timing; HRM 3.4 Interrupts   # or: Project decision
verification: test       # test | inspection | demonstration
depends_on: [REQ-100]    # [] if none
---
```

The `title` is the text after `# REQ-NNN: ` in the first heading. The heading stays.

### Open item (`docs/spec/open-items/OI-NNN.md`)

```yaml
---
id: OI-007
title: IRQ after PLP followed by SEI
status: open             # open | decided
area: [cpu]
affects: [REQ-119, REQ-120, docs/design/cpu.md]
---
```

### Slice (`docs/plan/SLICE-NN.md`)

```yaml
---
id: SLICE-01
title: Boot to BASIC and type a command
status: in-progress      # planned | in-progress | done
packages:
  - id: WP-1
    title: CPU
    status: planned
    requirements: [REQ-100, REQ-101, ...]
---
```

A range `REQ-NNN to REQ-NNN` is not permitted in frontmatter. The list names each ID. The
markdown tables of the slice stay as text for the reader, but no gate reads them.

`docs/plan/README.md` gets the frontmatter `product_complete: false` (REQ-050).

### Evidence record (`docs/verification/REQ-NNN.md`, REQ-051)

```yaml
---
requirement: REQ-008
date: 2026-12-01
commit: 0123abc
method: demonstration    # inspection | demonstration
result: pass             # pass | fail
by: lead                 # a role, not a person's name
---
```

The body has one subsection for each acceptance criterion: the steps and what was seen.

## Parser

Module `atari_harness::spec` reads all these files. It uses the crate `yaml-rust2` (YAML 1.2,
maintained). If `cargo-deny` rejects it, the fallback is `saphyr`. `serde_yaml` is
deprecated, so it is not used.

```rust
pub struct Requirement { pub id: u16, pub title: String, pub status: Status,
                         pub priority: Priority, pub area: Area, pub source: Source,
                         pub verification: Verification, pub depends_on: Vec<u16>,
                         pub sections: Vec<Section>, pub path: PathBuf }
pub struct OpenItem { /* id, title, status, area, affects, decision text */ }
pub struct Slice { /* id, title, status, packages */ }
pub struct Package { pub id: String, pub status: SliceStatus, pub requirements: Vec<u16> }
pub struct Evidence { /* requirement, date, commit, method, result, by */ }
pub struct TestPlanRow { pub document: PathBuf, pub requirements: Vec<u16>,
                         pub tests: Vec<TestRef>, pub evidence: Option<PathBuf> }
pub struct TestRef { pub file: Option<String>, pub function: String }
pub struct CodeTest { pub file: PathBuf, pub function: String, pub ignored: bool,
                      pub verifies: Vec<u16> }

pub struct Spec { pub requirements: ..., pub open_items: ..., pub slices: ...,
                  pub evidence: ..., pub test_plans: Vec<TestPlanRow>,
                  pub code_tests: Vec<CodeTest> }

/// Reads the whole repository. A parse error is a `SpecError` with the file and line.
pub fn load(root: &Path) -> Result<Spec, SpecError>;
/// One rule violation: the rule (requirement ID), the file, the line and a message.
pub struct Violation { pub rule: u16, pub file: PathBuf, pub line: Option<usize>,
                       pub message: String }
/// Runs every gate. An empty list means that all gates pass.
pub fn check(spec: &Spec, root: &Path) -> Vec<Violation>;
```

Test plan rows are the table rows in the `## Test plan` section of each design document whose
first cell names requirement IDs. A test in backticks, for example
`` `antic.rs` `nmist_visible_from_cycle_7` ``, is a `TestRef`: the file is the nearest
`.rs` name in backticks before it in the same cell, if any.

A file name in a test plan names a Rust module, not one file: `antic.rs` matches
`antic.rs` and every file below the directory `antic/` of the same crate (for example
`antic/tests/dma.rs`). A path with a directory, for example `crates/harness/tests/boot.rs`,
matches that file only. The `trace` gate uses this rule in both directions (REQ-049). It was
decided at the WP-2 merge, because the chip packages keep their tests in submodule files.

## Gates

Each gate is a function in `atari_harness::spec` and has its own test in
`crates/harness/tests/spec_gates.rs`. Each test runs the gate on the real repository and
also on small synthetic files in a temporary directory, so that each failure rule is tested.

| Gate | Rule |
| --- | --- |
| `schema` | REQ-043, REQ-053, REQ-054: frontmatter keys, values, IDs, `depends_on`, cycles |
| `structure` | REQ-044: sections, `shall`, numbered criteria |
| `vague_words` | REQ-045: the words in `docs/spec/vague-words.txt` |
| `sources` | REQ-046: `HRM n.m` against `docs/spec/hrm-sections.txt` |
| `references` | REQ-047: `REQ-`, `OI-`, `SLICE-` references and relative links |
| `test_plans` | REQ-048: each requirement in exactly one test plan, with tests or evidence |
| `trace` | REQ-027, REQ-049: test plan to code and code to test plan, for `done` packages |
| `slices` | REQ-050, REQ-054: planned requirements, package status |
| `evidence` | REQ-051: evidence records for `done` packages |
| `indexes` | REQ-053: the generated indexes are up to date |

`tests/traceability.rs` moves into this design: its rules become the `trace` gate.

## Generated files

The binary `cargo run -p atari-harness --bin spec-index` writes:

- `docs/spec/requirements/INDEX.md` (replaces `scripts/req-index.sh`, which is deleted),
- `docs/spec/open-items/README.md` (the table of open items),
- `target/traceability.md` (the matrix, REQ-052).

The `indexes` gate compares the committed files with a new generation.

## Data files

- `docs/spec/vague-words.txt`: one word or phrase per line. Lines that start with `#` are
  comments.
- `docs/spec/hrm-sections.txt`: one line per HRM section, `n.m<TAB>title`, from the table of
  contents of the HRM edition of 2026-01-03. The file header names the edition and the
  command that made the list.

## Make and CI

- `make spec-check` runs `cargo test -p atari-harness --test spec_gates --test traceability`.
- `make check` and `make pre-commit` call `spec-check`.
- CI gets a job `Specification` (Linux). It runs `make spec-check`, uploads
  `target/traceability.md` as the artifact `traceability`, and appends the counts and the
  violations to `$GITHUB_STEP_SUMMARY`. `CI success` needs the job.

## Migration

A one-time converter changes all requirement files, open items and slice files to
frontmatter. The converter is not committed. The migration commit must show that:

1. Each file has the same text below the frontmatter, except for the removed metadata table.
2. The values in the frontmatter equal the values of the old table (lower case, ranges
   expanded).
3. All gates pass, or each violation that remains is listed in the report of the work package
   for the lead to decide.

## Test plan

| Requirement | Test |
| --- | --- |
| REQ-043 | `spec_gates.rs` `schema_gate_passes_on_repository`, `schema_gate_rejects_bad_files` |
| REQ-044 | `spec_gates.rs` `structure_gate_passes_on_repository`, `structure_gate_rejects_bad_files` |
| REQ-045 | `spec_gates.rs` `vague_words_gate_passes_on_repository`, `vague_words_gate_rejects_bad_files` |
| REQ-046 | `spec_gates.rs` `sources_gate_passes_on_repository`, `sources_gate_rejects_bad_files` |
| REQ-047 | `spec_gates.rs` `references_gate_passes_on_repository`, `references_gate_rejects_bad_files` |
| REQ-048 | `spec_gates.rs` `test_plans_gate_passes_on_repository`, `test_plans_gate_rejects_bad_files` |
| REQ-027, REQ-049 | `spec_gates.rs` `trace_gate_passes_on_repository`, `trace_gate_rejects_bad_files`, `trace_gate_matches_submodule_files`, `trace_gate_rejects_other_modules`, `trace_gate_matches_paths_exactly` |
| REQ-050 | `spec_gates.rs` `unplanned_requirements_are_reported`, `product_complete_needs_must_planned` |
| REQ-051 | `spec_gates.rs` `evidence_gate_passes_on_repository`, `evidence_gate_rejects_bad_files` |
| REQ-052 | `spec_gates.rs` `matrix_has_one_row_per_requirement`; CI job `Specification` (artifact and summary) |
| REQ-053 | `spec_gates.rs` `open_item_gate_rejects_bad_files`, `indexes_are_up_to_date` |
| REQ-054 | `spec_gates.rs` `slice_gate_rejects_bad_files` |
| REQ-055 | `spec_gates.rs` `gates_run_within_10_seconds`; CI job `Specification` |

## Open issues

1. A Simplified Technical English gate (sentence length, approved words) is not in this
   design. Most criteria have exact values in long sentences. A length gate would need a
   decision on what counts as a sentence in a criterion.
