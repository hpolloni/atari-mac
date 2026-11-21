---
id: SLICE-06
title: Exact timing and product qualities
status: in-progress
depends_on: "SLICE-01"
packages:
  - id: WP-1
    title: CPU interrupt timing and undocumented opcodes
    status: planned
    requirements: [REQ-109, REQ-111, REQ-112, REQ-118, REQ-119, REQ-120, REQ-121, REQ-122, REQ-123, REQ-128]
  - id: WP-2
    title: System, PIA and keyboard timing
    status: planned
    requirements: [REQ-200, REQ-212, REQ-214, REQ-215, REQ-223, REQ-225, REQ-226, REQ-228, REQ-231, REQ-520, REQ-521]
  - id: WP-3
    title: Product qualities and save states
    status: in-progress
    requirements: [REQ-001, REQ-007, REQ-014, REQ-015, REQ-018, REQ-040, REQ-041, REQ-042]
---
# SLICE-06: Exact timing and product qualities

## Goal

The CPU, the PIA and the machine meet the remaining timing and power-on requirements. The core is deterministic and portable, and the emulator can save and restore its state.

## Work packages

| Package | Title | Requirements |
| --- | --- | --- |
| WP-1 | CPU interrupt timing and undocumented opcodes | REQ-109, REQ-111, REQ-112, REQ-118, REQ-119, REQ-120, REQ-121, REQ-122, REQ-123, REQ-128 |
| WP-2 | System, PIA and keyboard timing | REQ-200, REQ-212, REQ-214, REQ-215, REQ-223, REQ-225, REQ-226, REQ-228, REQ-231, REQ-520, REQ-521 |

On 2026-11-20, at the user's request, the lead moved REQ-007 (real-time speed and the headless speed limit) from SLICE-03 WP-2 to WP-3.

Speed baseline for REQ-007 criterion 2 (`--headless --frames 6000`, release build, Apple M4), from the speed work that was merged on 2026-11-20:

| Build | Time (3 runs) |
| --- | --- |
| Before the POKEY audio (`42f7b17`) | 10.05 s (1 run) |
| After the POKEY audio (`96517b2`) | 12.19 s, 11.92 s, 12.04 s |
| Speed branch on `96517b2` | 7.84 s, 7.57 s, 7.61 s |
| `main` after the merge (with the pot scan of SLICE-04) | 8.38 s, 8.32 s, 8.21 s |

The profile before the work: `Pokey::tick` 29 %, `Machine::run_cycle` (CPU and ANTIC) 26 %, `Gtia::color_clock` 22 %. The largest gains: fat LTO (about 10 %), GTIA shifters as one 64-bit word (about 12 %), and GTIA work that is skipped when it has no effect (about 4 %). After boot, the OS leaves `AUDCTL` = `$28` and `AUDF` = 0, so the linked timers 3 and 4 have an event every 3 or 4 cycles. One more option is not taken: `overflow-checks = false` in the release profile gives about 4 %, but `Cargo.toml` keeps the overflow checks. The rest of the speed work for REQ-007 is in this package.

On 2026-11-20 the lead moved REQ-227 (the CA2 and CB2 output modes) to SLICE-08 WP-2, because the cassette motor needs CA2.
| WP-3 | Product qualities and save states | REQ-001, REQ-007, REQ-014, REQ-015, REQ-018, REQ-040, REQ-041, REQ-042 |

Some requirements in this slice already have tests that run (from SLICE-01). The package must still check them against the full acceptance criteria.

### Test rule for each package (REQ-027)

Each test that the design test plans name for these requirements exists now as an ignored stub with the reason `planned: SLICE-06`. A package replaces each stub with a real test. When the package is `done`, each `test` requirement has a test that runs, and each `inspection` or `demonstration` requirement has a passed evidence record in `docs/verification/`.

## Done when

1. The interrupt timing tests pass, and OI-007 and OI-017 are decided.
2. A saved state restores to the same frames as the original run (REQ-018).
3. The evidence records for REQ-001, REQ-015, REQ-040 and REQ-041 pass.
4. `make check` passes, and the gates show no requirement of the slice as `missing`.
