---
id: SLICE-02
title: Graphics modes and player/missile graphics
status: in-progress
depends_on: "SLICE-01"
packages:
  - id: WP-1
    title: ANTIC modes, scrolling and P/M DMA
    status: in-progress
    requirements: [REQ-306, REQ-307, REQ-311, REQ-315, REQ-316, REQ-317, REQ-318, REQ-324, REQ-325, REQ-326, REQ-327, REQ-328, REQ-329, REQ-330, REQ-334, REQ-335, REQ-337, REQ-339, REQ-340, REQ-341, REQ-343, REQ-344, REQ-345, REQ-346, REQ-347]
  - id: WP-2
    title: GTIA playfield colors, players, priority and GTIA modes
    status: in-progress
    requirements: [REQ-407, REQ-410, REQ-411, REQ-412, REQ-413, REQ-414, REQ-415, REQ-416, REQ-417, REQ-418, REQ-419, REQ-420, REQ-421, REQ-422, REQ-423, REQ-424, REQ-425, REQ-427, REQ-428, REQ-429, REQ-430, REQ-431, REQ-432, REQ-433, REQ-434, REQ-441]
  - id: WP-3
    title: CPU halt, machine video and golden frames
    status: in-progress
    requirements: [REQ-002, REQ-019, REQ-026, REQ-125, REQ-126, REQ-127, REQ-213]
---
# SLICE-02: Graphics modes and player/missile graphics

## Goal

The emulator shows every ANTIC playfield mode, fine scrolling and player/missile graphics exactly to the color clock. Golden frame tests compare whole frames with committed reference images.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | ANTIC modes, scrolling and P/M DMA | REQ-306, REQ-307, REQ-311, REQ-315, REQ-316, REQ-317, REQ-318, REQ-324, REQ-325, REQ-326, REQ-327, REQ-328, REQ-329, REQ-330, REQ-334, REQ-335, REQ-337, REQ-339, REQ-340, REQ-341, REQ-343, REQ-344, REQ-345, REQ-346, REQ-347 |
| WP-2 | GTIA playfield colors, players, priority and GTIA modes | REQ-407, REQ-410, REQ-411, REQ-412, REQ-413, REQ-414, REQ-415, REQ-416, REQ-417, REQ-418, REQ-419, REQ-420, REQ-421, REQ-422, REQ-423, REQ-424, REQ-425, REQ-427, REQ-428, REQ-429, REQ-430, REQ-431, REQ-432, REQ-433, REQ-434, REQ-441 |
| WP-3 | CPU halt, machine video and golden frames | REQ-002, REQ-019, REQ-026, REQ-125, REQ-126, REQ-127, REQ-213 |

Some requirements in this slice already have tests that run (from SLICE-01). The package must still check them against the full acceptance criteria.

### Test rule for each package (REQ-027)

Each test that the design test plans name for these requirements exists now as an ignored stub with the reason `planned: SLICE-02`. A package replaces each stub with a real test. When the package is `done`, each `test` requirement has a test that runs, and each `inspection` or `demonstration` requirement has a passed evidence record in `docs/verification/`.

## Done when

1. Each ANTIC mode 2 to F shows on the screen as the HRM gives it, with and without fine scrolling.
2. Players and missiles show with all sizes, priorities and collisions.
3. The golden frame tests (REQ-026) pass in NTSC and in PAL.
4. `make check` passes, and the gates show no requirement of the slice as `missing`.
