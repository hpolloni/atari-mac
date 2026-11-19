# SLICE-01: Boot to BASIC and type a command

| Field | Value |
| --- | --- |
| Status | In progress |
| Depends on | The committed specification (`docs/spec/`, `docs/design/`) |

## Goal

The emulator boots AltirraOS and Altirra BASIC to the `READY` prompt and shows it in a
window. The user types `PRINT 2+2` and RETURN, and the screen shows `4`.

This is the smallest path that uses all layers: the CPU, the memory map, ANTIC, GTIA, POKEY,
the frontend and the test harness.

## Requirements in the slice

| Layer | Package | Requirements | Scope in this slice |
| --- | --- | --- | --- |
| CPU | WP-1 | REQ-100 to REQ-108, REQ-110, REQ-113 to REQ-117, REQ-124 | All opcodes, exact to the cycle. The Klaus Dormann test and the SingleStepTests suite pass. |
| ANTIC | WP-2 | REQ-300 to REQ-305, REQ-308 to REQ-310, REQ-312 to REQ-314, REQ-319 to REQ-323, REQ-331 to REQ-333, REQ-336, REQ-338 | Display list, mode 2, DMA halts, VBI and DLI, `WSYNC`, `VCOUNT`. |
| GTIA | WP-2 | REQ-400 to REQ-406, REQ-408, REQ-409, REQ-435, REQ-437 | Color registers, high-resolution playfield, NTSC palette, triggers, console keys. |
| POKEY | WP-3 | REQ-500, REQ-501, REQ-515, REQ-518, REQ-519, REQ-522 to REQ-524 | Keyboard scan, keyboard IRQ, BREAK, `IRQEN` and `IRQST`, the ASCII table. |
| Frontend | WP-4 | REQ-008, REQ-009, REQ-013 | Window, keyboard mapping, headless screenshot. |
| System | WP-5 | REQ-201 to REQ-208, REQ-211, REQ-216 to REQ-222, REQ-224, REQ-229, REQ-230 | Memory map, `PORTB` banking, PIA data and direction registers, reset. |
| Test harness | WP-5 | REQ-003, REQ-005, REQ-020, REQ-021, REQ-023 to REQ-025, REQ-027 | Boot test, typing test, screen text, the CPU test suites, traceability. |
| Quality | WP-6 | REQ-043 to REQ-055 | Specification gates, frontmatter, two-way traceability, evidence records, the published matrix. |

## Not in this slice

- Audio and the POKEY timers.
- Player/missile graphics.
- Scrolling and the ANTIC modes other than mode 2.
- The PAL machine.
- Cartridges, executables and disk images.

## Work packages

Each teammate works in its own git worktree (see `CLAUDE.md`). Each package must pass
`make pre-commit` before it is merged.

| Package | Status | Owner | Contents | Checked by |
| --- | --- | --- | --- | --- |
| WP-0 Interfaces | Done | Teammate | The workspace, the decided interfaces as stubs that compile, the `atari-harness` crate with its helpers, the traceability check | `tests/harness_self_test.rs`, `tests/traceability.rs` |
| WP-1 CPU | In progress | Teammate | `crates/atari-core/src/cpu.rs` and submodules, from [cpu.md](../design/cpu.md) | `tests/cpu_functional.rs`, `tests/cpu_single_step.rs`, CPU unit tests |
| WP-2 Video | In progress | Teammate | `antic.rs`, `gtia.rs`, `palette.rs`, from [antic.md](../design/antic.md) and [gtia.md](../design/gtia.md) | ANTIC and GTIA unit tests |
| WP-3 Keyboard | In progress | Teammate | `pokey.rs` (keyboard, IRQ) and `keyboard.rs`, from [pokey.md](../design/pokey.md) | POKEY and keyboard unit tests |
| WP-4 Frontend | In progress | Teammate | `src/` of the root crate: window, key map, headless mode, from [architecture.md](../design/architecture.md). macOS only: it also changes the Linux CI jobs to `--workspace --exclude atari-mac` and adds a macOS clippy step for the frontend. | `src/keymap.rs` unit tests, `tests/cli.rs` |
| WP-5 Machine and harness | In progress | Teammate | `machine.rs`, `pia.rs`, `timing.rs`, the `atari-harness` crate, and the integration code for WP-1 to WP-4. The lead merges and runs the final checks. | `tests/boot.rs`, `tests/keyboard.rs`, `tests/harness_self_test.rs` |
| WP-6 Specification gates | In progress | Teammate | The frontmatter migration, `atari_harness::spec`, the gates of [spec-gates.md](../design/spec-gates.md), `spec-index`, the CI job `Specification`. It starts after WP-0 is merged. | `tests/spec_gates.rs`, `tests/traceability.rs` |

### Test rule for each package (REQ-027)

`crates/harness/tests/traceability.rs` checks the tests of each package:

| Package status | Rule |
| --- | --- |
| `Planned` or `In progress` | No rule. The matrix in `target/traceability.md` shows the state. |
| `Done` | Each requirement of the package with the verification method `Test` has at least one test that names it (`/// Verifies REQ-NNN`) and that is not ignored. |

When the slice status is `Done`, the same rule applies to all requirements of the slice. A
package sets its own status to `Done` in its last commit, so the check runs on that commit.

WP-1 to WP-4 do not depend on each other. Each one works against the interfaces in the
design documents. WP-5 does the integration after the other packages are merged.

## Done when

1. `make check` passes.
2. In CI, `boots_to_basic_ready` and `basic_prints_sum` pass.
3. `atari-mac` shows the `READY` screen in a window on macOS. Typing `PRINT 2+2` and RETURN
   shows `4`.
4. Each requirement in the slice has a test that names its ID (REQ-027), and the test passes.
