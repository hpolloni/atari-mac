---
id: SLICE-08
title: Cassette tape
status: planned
depends_on: "SLICE-05 (POKEY serial input); the CA2 output modes of REQ-227 (SLICE-06 WP-2)"
packages:
  - id: WP-1
    title: CAS tape images
    status: planned
    requirements: [REQ-639, REQ-640]
  - id: WP-2
    title: Tape deck, motor and signal
    status: planned
    requirements: [REQ-641, REQ-642]
  - id: WP-3
    title: Tape boot and the --cas option
    status: planned
    requirements: [REQ-643, REQ-081]
---
# SLICE-08: Cassette tape

## Goal

The user starts the emulator with `--cas game.cas`. The emulator inserts the CAS tape image in an emulated cassette deck, holds START and OPTION at the cold reset, and presses RETURN after the prompt tone. The OS reads the tape through the POKEY serial input and runs the boot-tape game.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | CAS tape images | REQ-639, REQ-640 |
| WP-2 | Tape deck, motor and signal | REQ-641, REQ-642 |
| WP-3 | Tape boot and the --cas option | REQ-643, REQ-081 |

WP-1 is in the core only (`Tape::from_cas`). WP-2 needs WP-1, the POKEY serial input and `SKSTAT` bit 4 (SLICE-05 WP-1: REQ-526, REQ-530), and the PIA CA2 output (REQ-227). WP-3 needs WP-2.

The design is in `docs/design/media.md`, "Cassette deck", and in `docs/design/architecture.md`, "Tape image from the command line (REQ-081)".

### Open items

These open items change the requirements of this slice. The requirements follow the recommended option of each item now. A package does not start the work of a criterion that an open item changes until the user decides the item, or it uses the recommended option and the lead records this.

| Open item | Question | Affects |
| --- | --- | --- |
| OI-048 | The CAS chunk types that the emulator supports | REQ-639, REQ-640, REQ-642 |
| OI-049 | Tape writing | Scope of the slice (not planned now) |
| OI-050 | The key presses for a tape boot | REQ-081, REQ-643 |
| OI-051 | The tape load speed | REQ-081 (not planned now) |
| OI-052 | The tape position at a cold reset | REQ-641 criterion 5 |
| OI-046 | The cassette device code that SIO intercepts | REQ-643 criterion 5 does not depend on it |

### Test rule for each package (REQ-027)

Each test that the design test plans name for these requirements exists now as an ignored stub with the reason `planned: SLICE-08`. A package replaces each stub with a real test. When the package is `done`, each `test` requirement has a test that runs.

The tests make all tape images in code, so no test data is downloaded, except the AltirraOS and BASIC ROM files of `scripts/fetch-roms.sh`. The README options table gets the row for `--cas` in WP-3, when `--help` lists the option (REQ-056 criterion 6).

## Done when

1. The OS boots the boot test tape of REQ-643 for NTSC and PAL.
2. The user boots a boot-tape game from a CAS file in the window with `--cas`.
3. `make check` passes, and the gates show no requirement of the slice as `missing`.
