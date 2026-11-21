---
id: SLICE-08
title: Cassette tape
status: in-progress
depends_on: "SLICE-05 WP-1 (POKEY serial input)"
packages:
  - id: WP-1
    title: CAS tape images
    status: done
    requirements: [REQ-639, REQ-640]
  - id: WP-2
    title: Tape deck, motor and signal
    status: done
    requirements: [REQ-641, REQ-642, REQ-227]
  - id: WP-3
    title: Tape boot and the --cas option
    status: done
    requirements: [REQ-643, REQ-081, REQ-082]
  - id: WP-4
    title: Tape shortcut and automatic RETURN
    status: done
    requirements: [REQ-644, REQ-091, REQ-092]
---
# SLICE-08: Cassette tape

## Goal

The user starts the emulator with `--cas game.cas`. The emulator inserts the CAS tape image in an emulated cassette deck, holds START at the cold reset. The user presses RETURN after the prompt tone. The tape loads in turbo while the motor runs. The OS reads the tape through the POKEY serial input and runs the boot-tape game.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | CAS tape images | REQ-639, REQ-640 |
| WP-2 | Tape deck, motor and signal | REQ-641, REQ-642, REQ-227 |
| WP-3 | Tape boot and the --cas option | REQ-643, REQ-081, REQ-082 |
| WP-4 | Tape shortcut and automatic RETURN | REQ-644, REQ-091, REQ-092 |

WP-1 is in the core only (`Tape::from_cas`). WP-2 needs WP-1, the POKEY serial input and `SKSTAT` bit 4 (SLICE-05 WP-1: REQ-526, REQ-530), and the PIA CA2 output (REQ-227, moved here from SLICE-06 WP-2 on 2026-11-20). WP-3 needs WP-2. WP-4 (added on 2026-11-21, a decision of the user) needs WP-3 and the SIOV hook of SLICE-05 (`machine/siov.rs`). WP-1 can start at once, in parallel with SLICE-05 WP-1.

REQ-227 criteria 3 to 5 (the pulse modes) follow the recommended option 1 of OI-010, which is open. The lead records this here. The tape uses only criteria 1 and 2.

The design is in `docs/design/media.md`, "Cassette deck", and in `docs/design/architecture.md`, "Tape image from the command line (REQ-081)".

### Open items

These open items change the requirements of this slice. The requirements follow the recommended option of each item now. A package does not start the work of a criterion that an open item changes until the user decides the item, or it uses the recommended option and the lead records this.

| Open item | Question | Affects |
| --- | --- | --- |
| OI-048 | The CAS chunk types that the emulator supports (decided: option 2) | REQ-639, REQ-640, REQ-642 |
| OI-049 | Tape writing (decided: option 1, out of scope, a non-goal) | Scope of the slice |
| OI-050 | The key presses for a tape boot (decided: option 2, START only) | REQ-081, REQ-643 |
| OI-051 | The tape load speed (decided: option 2, turbo) | REQ-082 |
| OI-052 | The tape position at a cold reset (decided: option 2) | REQ-641 criterion 5 |
| OI-046 | The cassette device code that SIO intercepts (decided: option 2, `$60`) | REQ-644, REQ-643 criterion 5 |

### Test rule for each package (REQ-027)

Each test that the design test plans name for these requirements exists now as an ignored stub with the reason `planned: SLICE-08`. A package replaces each stub with a real test. When the package is `done`, each `test` requirement has a test that runs.

The tests make all tape images in code, so no test data is downloaded, except the AltirraOS and BASIC ROM files of `scripts/fetch-roms.sh`. The README options table gets the row for `--cas` in WP-3, when `--help` lists the option (REQ-056 criterion 6).

## Done when

1. The OS boots the boot test tape of REQ-643 for NTSC and PAL.
2. The user boots a boot-tape game from a CAS file in the window with `--cas`. With the tape shortcut, a tape of standard OS records loads in less than 10 s.
3. `make check` passes, and the gates show no requirement of the slice as `missing`.
