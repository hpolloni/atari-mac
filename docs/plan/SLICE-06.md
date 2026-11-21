---
id: SLICE-06
title: Exact timing and product qualities
status: planned
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
    status: planned
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
