---
id: SLICE-03
title: Sound
status: done
depends_on: "SLICE-01"
packages:
  - id: WP-1
    title: POKEY timers and audio
    status: done
    requirements: [REQ-438, REQ-503, REQ-504, REQ-505, REQ-506, REQ-507, REQ-508, REQ-509, REQ-510, REQ-511, REQ-512, REQ-513, REQ-514, REQ-538, REQ-539, REQ-540, REQ-541]
  - id: WP-2
    title: Frontend audio and speed
    status: done
    requirements: [REQ-011]
---
# SLICE-03: Sound

## Goal

The emulator makes the POKEY sound and the console speaker sound. In BASIC, `SOUND 0,121,10,8` plays a tone in the window, and the emulator runs at real-time speed.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | POKEY timers and audio | REQ-438, REQ-503, REQ-504, REQ-505, REQ-506, REQ-507, REQ-508, REQ-509, REQ-510, REQ-511, REQ-512, REQ-513, REQ-514, REQ-538, REQ-539, REQ-540, REQ-541 |
| WP-2 | Frontend audio and speed | REQ-011 |

On 2026-11-20, at the user's request, the lead moved REQ-007 (real-time speed and the headless speed limit) to SLICE-06 WP-3.

Some requirements in this slice already have tests that run (from SLICE-01). The package must still check them against the full acceptance criteria.

### Test rule for each package (REQ-027)

Each test that the design test plans name for these requirements exists now as an ignored stub with the reason `planned: SLICE-03`. A package replaces each stub with a real test. When the package is `done`, each `test` requirement has a test that runs, and each `inspection` or `demonstration` requirement has a passed evidence record in `docs/verification/`.

## Done when

1. The audio sample tests pass for each distortion, clock and filter mode.
2. The user hears the tone of `SOUND 0,121,10,8` and the key click (evidence record for REQ-011).
3. `make check` passes, and the gates show no requirement of the slice as `missing`.
