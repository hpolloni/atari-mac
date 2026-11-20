# Golden reference images

The golden frame tests (REQ-026, `crates/harness/tests/golden.rs`) compare frames with these
files. The emulator made them with `UPDATE_GOLDEN=1`, so a person reviews each new or changed
image once, and this table records the review.

| File | Content | Reviewed | Commit | Result |
| --- | --- | --- | --- | --- |
| `boot-ready-ntsc.png` | BASIC screen after 240 frames, NTSC | 2026-11-19, user | `73df071` | Pass: shows `Ready` |
| `boot-ready-pal.png` | BASIC screen after 240 frames, PAL | 2026-11-19, user | `73df071` | Pass: shows `Ready`. The hue differs from NTSC, as expected for `$94` with the PAL hue angles (HRM D.4). |

The PAL hue depends on the palette parameters (OI-023). When OI-023 is decided, check
`boot-ready-pal.png` again.
