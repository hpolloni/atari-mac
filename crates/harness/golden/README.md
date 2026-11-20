# Golden reference images

The golden frame tests (REQ-026, `crates/harness/tests/golden.rs`) compare frames with these
files. The emulator made them with `UPDATE_GOLDEN=1`, so a person reviews each new or changed
image once, and this table records the review.

| File | Content | Reviewed | Commit | Result |
| --- | --- | --- | --- | --- |
| `boot-ready-ntsc.png` | BASIC screen after 240 frames, NTSC | 2026-11-19, user | `73df071` | Pass: shows `Ready` |
| `boot-ready-pal.png` | BASIC screen after 240 frames, PAL | 2026-11-19, user | `73df071` | Pass: shows `Ready`. The hue differs from NTSC, as expected for `$94` with the PAL hue angles (HRM D.4). |
| `antic-modes-ntsc.png` | `golden_antic_modes_ntsc`: one band of each ANTIC mode 2 to F (text, inverse, mode 3 descenders, multicolor glyphs, map mode color steps and checkerboards), NTSC | pending user review | | |
| `antic-modes-pal.png` | `golden_antic_modes_pal`: the screen of `antic-modes-ntsc.png` in PAL | pending user review | | |
| `fine-scroll.png` | `golden_fine_scroll`: modes 2 and D, each without scrolling, with `HSCROL` = 2 and with `VSCROL` = 1, NTSC | pending user review | | |
| `players-missiles.png` | `golden_players_missiles`: four players (normal, double, quad, double width), each a box with a cross, and four missiles (normal, double, quad, normal), in four `PRIOR` bands `$01`, `$02`, `$04`, `$08`; in each band one row of boxes over the background and one over mode D stripes; single line resolution (8 lines high) above, double line (16 lines high) below, NTSC | pending user review | | |
| `gtia-modes.png` | `golden_gtia_modes`: mode F pixel values 0 to 15 with `PRIOR` `$00`, `$40` (mode 9), `$80` (mode 10), `$C0` (mode 11), NTSC | pending user review | | |

The PAL hue depends on the palette parameters (OI-023). When OI-023 is decided, check
`boot-ready-pal.png` again.
