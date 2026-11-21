---
id: SLICE-05
title: Disk drive
status: planned
depends_on: "SLICE-04"
packages:
  - id: WP-1
    title: POKEY serial port
    status: planned
    requirements: [REQ-502, REQ-516, REQ-517, REQ-525, REQ-526, REQ-527, REQ-528, REQ-529, REQ-530, REQ-531, REQ-532, REQ-533]
  - id: WP-2
    title: SIO disk drive and ATR images
    status: planned
    requirements: [REQ-012, REQ-615, REQ-616, REQ-617, REQ-618, REQ-619, REQ-620, REQ-621, REQ-622, REQ-623, REQ-624, REQ-625, REQ-626, REQ-627, REQ-628, REQ-629, REQ-630, REQ-631, REQ-632, REQ-633, REQ-634, REQ-635, REQ-636, REQ-637, REQ-638]
---
# SLICE-05: Disk drive

## Goal

The emulator has an emulated disk drive D1: on the SIO bus. It boots an ATR disk image, and a program can write sectors to the image.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | POKEY serial port | REQ-502, REQ-516, REQ-517, REQ-525, REQ-526, REQ-527, REQ-528, REQ-529, REQ-530, REQ-531, REQ-532, REQ-533 |
| WP-2 | SIO disk drive and ATR images | REQ-012, REQ-615, REQ-616, REQ-617, REQ-618, REQ-619, REQ-620, REQ-621, REQ-622, REQ-623, REQ-624, REQ-625, REQ-626, REQ-627, REQ-628, REQ-629, REQ-630, REQ-631, REQ-632, REQ-633, REQ-634, REQ-635, REQ-636, REQ-637, REQ-638 |

Some requirements in this slice already have tests that run (from SLICE-01). The package must still check them against the full acceptance criteria.

### Test rule for each package (REQ-027)

Each test that the design test plans name for these requirements exists now as an ignored stub with the reason `planned: SLICE-05`. A package replaces each stub with a real test. When the package is `done`, each `test` requirement has a test that runs, and each `inspection` or `demonstration` requirement has a passed evidence record in `docs/verification/`.

REQ-502 (initialization mode) and REQ-533 (two-tone mode) are in this slice, not in SLICE-03, because some of their criteria need the pot scan (SLICE-04), serial input and force break.

## Done when

1. The OS boots a disk image from D1: (REQ-636).
2. A program writes sectors, and the exported image has the new data (REQ-637).
3. `make check` passes, and the gates show no requirement of the slice as `missing`.
