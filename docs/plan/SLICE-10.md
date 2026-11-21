---
id: SLICE-10
title: CPU instruction trace
status: in-progress
depends_on: "SLICE-09"
packages:
  - id: WP-1
    title: Disassembler
    status: done
    requirements: [REQ-087]
  - id: WP-2
    title: The --trace option
    status: planned
    requirements: [REQ-086]
---
# SLICE-10: CPU instruction trace

## Goal

The user writes a trace of each CPU instruction of some frames to a file, with the beam position and the cycle of each instruction, and compares it with the trace of another emulator.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | Disassembler | REQ-087 |
| WP-2 | The --trace option | REQ-086 |

WP-1 is in the core only. WP-2 needs a hook in the CPU or the machine at each instruction start and each interrupt sequence, and the disassembler. The hook must not slow down the emulation when no trace is on (REQ-007).

## Done when

1. The user writes a trace of one frame with `--trace` and reads it.
2. `make check` passes, and the gates show no requirement of the slice as `missing`.
