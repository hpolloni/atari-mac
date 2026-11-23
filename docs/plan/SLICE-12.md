---
id: SLICE-12
title: Code quality gates
status: in-progress
depends_on: "none"
packages:
  - id: WP-1
    title: Complexity and file length gates
    status: done
    requirements: [REQ-058, REQ-059]
  - id: WP-2
    title: Mutation testing of changed code
    status: planned
    requirements: [REQ-060]
  - id: WP-3
    title: Media parser property tests
    status: done
    requirements: [REQ-061]
  - id: WP-4
    title: Command-line rules found by the mutation run
    status: done
    requirements: [REQ-096, REQ-097]
---
# SLICE-12: Code quality gates

## Goal

The gates find code that is too complex, files that are too long, tests that check nothing, and media parsers that panic on a damaged file.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | Complexity and file length gates | REQ-058, REQ-059 |
| WP-2 | Mutation testing of changed code | REQ-060 |
| WP-3 | Media parser property tests | REQ-061 |
| WP-4 | Command-line rules found by the mutation run | REQ-096, REQ-097 |

The design is `docs/design/code-gates.md`. The three packages run in parallel. WP-1 changes `crates/harness` (a new module `code`, `syn`), the `spec-check` target of the `Makefile` and `docs/spec/code-baseline.txt`. WP-2 changes the `tools` target and a new `mutants` target of the `Makefile`, `.cargo/mutants.toml` and `.github/workflows/ci.yml`. WP-3 changes `crates/harness/tests/media_fuzz.rs`, the dev-dependencies of `crates/harness/Cargo.toml`, and the core parsers if a test finds a panic. WP-1 and WP-3 both change `crates/harness/Cargo.toml`; WP-1 and WP-2 both change the `Makefile`. Each change is small.

Merge order: WP-3 first, because it can change core functions that the baseline of WP-1 lists. Then WP-1, with the baseline made again on the merged code. WP-2 merges last, because it makes the gates stricter for each later change.

WP-4 came from the mutation run on `src/cli.rs`: the parser had rules that no requirement stated. It changes only tests: unit tests in `src/cli.rs` and a new `tests/cli_errors.rs` (`tests/cli.rs` is on the code baseline and must not grow). The parser does not change.

## Done when

1. `make check` passes with the four gates, and the gates show no requirement of the slice as `missing`.
2. The user confirms the inspection of REQ-060.
