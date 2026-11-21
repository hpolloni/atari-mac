---
id: SLICE-09
title: Logging and speed statistics
status: done
depends_on: "SLICE-08 (tape events), SLICE-05 WP-1 (SIO events)"
packages:
  - id: WP-1
    title: Machine event log in the core
    status: done
    requirements: [REQ-084]
  - id: WP-2
    title: The --stats and --log options
    status: done
    requirements: [REQ-083, REQ-085]
---
# SLICE-09: Logging and speed statistics

## Goal

The user starts the emulator with `--stats` and sees, each second, how fast the emulation runs and why. With `--log tape,sio`, the user sees what the machine does while it loads a tape or a disk.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | Machine event log in the core | REQ-084 |
| WP-2 | The --stats and --log options | REQ-083, REQ-085 |

WP-1 adds the `log` dependency to the core (REQ-015, changed on 2026-11-21) and the records of REQ-084. It changes `crates/atari-core` and its tests in `crates/harness/tests/event_log.rs` (a `log` dev-dependency of the harness). WP-2 changes only the frontend (`src/`, `tests/`, the README options rows). The `log` targets and the data of each record are in REQ-084, so WP-2 can start in parallel with WP-1. The WP-2 tests of REQ-085 that need core records use a record that the test sends, or stay ignored stubs with the reason `planned: SLICE-09 WP-1` until WP-1 is merged. WP-1 merges first.

REQ-015 stays in SLICE-06 WP-3 (an inspection). WP-1 changes the dependency that it inspects.

The design goes in `docs/design/architecture.md`, a new section "Logging and statistics".

## Done when

1. The user runs the window with `--stats --log tape --cas <file>` and sees the statistics lines and the tape records.
2. `make check` passes, and the gates show no requirement of the slice as `missing`.
