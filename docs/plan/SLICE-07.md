---
id: SLICE-07
title: Run a BASIC program from a file
status: in-progress
depends_on: "SLICE-01"
packages:
  - id: WP-1
    title: The --bas option
    status: in-progress
    requirements: [REQ-080]
---
# SLICE-07: Run a BASIC program from a file

## Goal

The user starts the emulator with `--bas game.bas`. The emulator boots to `Ready`, types the program from the text file, and runs it.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | The --bas option | REQ-080 |

## Done when

1. Each acceptance criterion of REQ-080 has a test that runs.
2. The user runs a BASIC program from a file in the window (for example the pong program from this session).
3. `make check` passes.
