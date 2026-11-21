---
id: SLICE-11
title: Monitor
status: planned
depends_on: "SLICE-10 (disassembler, instruction hook)"
packages:
  - id: WP-1
    title: Stop, step and continue
    status: planned
    requirements: [REQ-088]
  - id: WP-2
    title: Breakpoints and watches
    status: planned
    requirements: [REQ-089]
  - id: WP-3
    title: Memory and registers
    status: planned
    requirements: [REQ-090]
---
# SLICE-11: Monitor

## Goal

The user stops the emulation, steps instructions, sets breakpoints, and looks at and changes the registers and the memory.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | Stop, step and continue | REQ-088 |
| WP-2 | Breakpoints and watches | REQ-089 |
| WP-3 | Memory and registers | REQ-090 |

WP-2 and WP-3 need WP-1. WP-2 and WP-3 can run in parallel.

### Open items

| Open item | Question | Affects |
| --- | --- | --- |
| OI-057 | Where the user types the monitor commands | REQ-088, REQ-089, REQ-090 |

No package starts before the user decides OI-057.

## Done when

1. The user stops the emulation, steps, sets a breakpoint and reads the memory in the monitor.
2. `make check` passes, and the gates show no requirement of the slice as `missing`.
