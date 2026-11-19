---
id: SLICE-04
title: Cartridges, executables and controllers
status: planned
depends_on: "SLICE-01"
packages:
  - id: WP-1
    title: Cartridges and executable files
    status: planned
    requirements: [REQ-209, REQ-210, REQ-232, REQ-600, REQ-604, REQ-605, REQ-606, REQ-607, REQ-608, REQ-609, REQ-610, REQ-611, REQ-612, REQ-613, REQ-614]
  - id: WP-2
    title: Joysticks, paddles and console keys
    status: planned
    requirements: [REQ-006, REQ-010, REQ-017, REQ-436, REQ-439, REQ-440, REQ-534, REQ-535, REQ-536, REQ-537]
  - id: WP-3
    title: Command line and robust input
    status: planned
    requirements: [REQ-004, REQ-012, REQ-016]
---
# SLICE-04: Cartridges, executables and controllers

## Goal

The user starts a cartridge image or an executable file from the command line and controls it with the joystick, the paddles and the console keys.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | Cartridges and executable files | REQ-209, REQ-210, REQ-232, REQ-600, REQ-604, REQ-605, REQ-606, REQ-607, REQ-608, REQ-609, REQ-610, REQ-611, REQ-612, REQ-613, REQ-614 |
| WP-2 | Joysticks, paddles and console keys | REQ-006, REQ-010, REQ-017, REQ-436, REQ-439, REQ-440, REQ-534, REQ-535, REQ-536, REQ-537 |
| WP-3 | Command line and robust input | REQ-004, REQ-012, REQ-016 |

Some requirements in this slice already have tests that run (from SLICE-01). The package must still check them against the full acceptance criteria.

### Test rule for each package (REQ-027)

Each test that the design test plans name for these requirements exists now as an ignored stub with the reason `planned: SLICE-04`. A package replaces each stub with a real test. When the package is `done`, each `test` requirement has a test that runs, and each `inspection` or `demonstration` requirement has a passed evidence record in `docs/verification/`.

## Done when

1. A cartridge image and an executable file start from the command line.
2. The joystick, the fire button and the paddles change the values that the program reads.
3. No bad file or bad command line makes the emulator panic (REQ-016).
4. `make check` passes, and the gates show no requirement of the slice as `missing`.
