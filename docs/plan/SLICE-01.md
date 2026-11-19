---
id: SLICE-01
title: Boot to BASIC and type a command
status: done
depends_on: "The committed specification (`docs/spec/`, `docs/design/`)"
packages:
  - id: WP-0
    title: Interfaces
    status: done
    requirements: []
  - id: WP-1
    title: CPU
    status: done
    requirements: [REQ-100, REQ-101, REQ-102, REQ-103, REQ-104, REQ-105, REQ-106, REQ-107, REQ-108, REQ-110, REQ-113, REQ-114, REQ-115, REQ-116, REQ-117, REQ-124]
  - id: WP-2
    title: Video
    status: done
    requirements: [REQ-300, REQ-301, REQ-302, REQ-303, REQ-304, REQ-305, REQ-308, REQ-309, REQ-310, REQ-312, REQ-313, REQ-314, REQ-319, REQ-320, REQ-321, REQ-322, REQ-323, REQ-331, REQ-332, REQ-333, REQ-336, REQ-338, REQ-400, REQ-401, REQ-402, REQ-403, REQ-404, REQ-405, REQ-406, REQ-408, REQ-409, REQ-435, REQ-437]
  - id: WP-3
    title: Keyboard
    status: done
    requirements: [REQ-500, REQ-501, REQ-515, REQ-518, REQ-519, REQ-522, REQ-523, REQ-524]
  - id: WP-4
    title: Frontend
    status: done
    requirements: [REQ-008, REQ-009, REQ-013]
  - id: WP-5
    title: Machine and harness
    status: done
    requirements: [REQ-201, REQ-202, REQ-203, REQ-204, REQ-205, REQ-206, REQ-207, REQ-208, REQ-211, REQ-216, REQ-217, REQ-218, REQ-219, REQ-220, REQ-221, REQ-222, REQ-224, REQ-229, REQ-230, REQ-003, REQ-005, REQ-020, REQ-021, REQ-023, REQ-024, REQ-025, REQ-027]
  - id: WP-6
    title: Specification gates
    status: done
    requirements: [REQ-043, REQ-044, REQ-045, REQ-046, REQ-047, REQ-048, REQ-049, REQ-050, REQ-051, REQ-052, REQ-053, REQ-054, REQ-055]
---
# SLICE-01: Boot to BASIC and type a command

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
| WP-1 CPU | Done | Teammate | `crates/atari-core/src/cpu.rs` and submodules, from [cpu.md](../design/cpu.md) | `tests/cpu_functional.rs`, `tests/cpu_single_step.rs`, CPU unit tests |
| WP-2 Video | Done | Teammate | `antic.rs`, `gtia.rs`, `palette.rs`, from [antic.md](../design/antic.md) and [gtia.md](../design/gtia.md) | ANTIC and GTIA unit tests |
| WP-3 Keyboard | Done | Teammate | `pokey.rs` (keyboard, IRQ) and `keyboard.rs`, from [pokey.md](../design/pokey.md) | POKEY and keyboard unit tests |
| WP-4 Frontend | Done | Teammate | `src/` of the root crate: window, key map, headless mode, from [architecture.md](../design/architecture.md). macOS only: it also changes the Linux CI jobs to `--workspace --exclude atari-mac` and adds a macOS clippy step for the frontend. | `src/keymap.rs` unit tests, `tests/cli.rs` |
| WP-5 Machine and harness | Done | Teammate | `machine.rs`, `pia.rs`, `timing.rs`, the `atari-harness` crate, and the integration code for WP-1 to WP-4. The lead merges and runs the final checks. | `tests/boot.rs`, `tests/keyboard.rs`, `tests/harness_self_test.rs` |
| WP-6 Specification gates | Done | Teammate | The frontmatter migration, `atari_harness::spec`, the gates of [spec-gates.md](../design/spec-gates.md), `spec-index`, the CI job `Specification`. It starts after WP-0 is merged. | `tests/spec_gates.rs`, `tests/traceability.rs` |

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

## Follow-up items

Items that a work package found and that belong to a later package or slice.

| From | Item | For |
| --- | --- | --- |
| WP-3 | REQ-500 criterion 1 checks the audio mix level. Audio is not in SLICE-01, so the test checks the stored `AUDC1` instead. | The audio package |
| WP-3 | `pokey.md` functions that are not in the code yet: `serial_clock_out`, `set_serial_clock_in`, `serial_clock_in_drive`, `set_nonlinear_audio`. | The serial and audio packages |
| WP-3 | `Machine::drain_audio` is `const fn`, so `Pokey::drain_samples` must be `const` and has an `expect`. Remove both when audio exists. | The audio package |
| WP-3 | The machine calls `set_speaker(!gtia.speaker())`. Check the polarity against `pokey.md` and `gtia.md`. | The audio package |
| WP-3 | Linked timers, two-tone resync and the asynchronous receive hold are not implemented. With `AUDCTL` bits 3 or 4 set, the timers count as unlinked. | The timer and serial packages |
| WP-1 | REQ-120 criterion 3 (`PLP`+`SEI`, `PLP`+`PLP`) is not met; it waits for OI-007. | The lead, after OI-007 |
| WP-4 | `every_atari_key_has_a_host_key` uses its own copy of the 54 keys. Switch it to `keyboard::KEYS`, which WP-3 added. | A frontend teammate |
| WP-4 | `Machine` has no `set_key` or `set_control` wrappers, so CONTROL alone does nothing in the frontend. | WP-5 part 2 |
| WP-4 | `flate2` is held at 1.1.9: 1.1.10 adds a second `miniz_oxide`, which `cargo-deny` rejects. | Dependabot review |
| WP-4 | REQ-014 adds `Instant::now` to `disallowed-methods`. The frontend needs it for frame timing, so it needs a scoped exception. | The REQ-014 package |
| WP-4 | CI does not measure the frontend's coverage. Only a local `make check` on macOS does. | Known; matches `architecture.md` |
| WP-5 | `machine.rs` calls `pokey.set_speaker(gtia.speaker())`. HRM 6.11 gives `!gtia.speaker()`. `system.md` and `pokey.md` are corrected. | WP-5 part 2 |
| WP-5 | The CPU gives no signal that an IRQ or BRK sequence started, so the machine cannot call `Antic::irq_sequence_started` (REQ-335). | A CPU teammate and WP-5 part 2 |
| WP-5 | REQ-203 criteria 3 and 4 and REQ-206 criterion 6 need a test cartridge that counts `$D5xx` accesses or has RD5 off. `media::Cartridge` has no test hook. | The media package |
| WP-5 | There is no `SioBus` yet, so the machine keeps SIO DATA IN high and drops the serial bytes. | The media package |
| WP-6 merge | `kil_machine_keeps_running` (REQ-113, `machine.rs`) is in the `cpu.md` test plan but is not written. It is in `docs/spec/gate-baseline.txt`. | WP-5 part 2 |
| WP-6 merge | `break_stops_basic_program` (REQ-522, `crates/harness/tests/keyboard.rs`) is in the `pokey.md` test plan but is not written. It is in the baseline. | WP-5 part 2 |
| WP-6 merge | `basic_prints_sum` is in the REQ-518 test plan, but its `Verifies` comment names only REQ-025. It is in the baseline. | WP-5 part 2 |
| WP-6 | The baseline holds about 130 violations in the existing specification: `depends_on` cycles, vague words, project decisions without notes, requirements in no test plan or in two. | The lead |
| WP-2 | Criteria that need a system test in `crates/harness/tests/` are not checked yet: REQ-300 criterion 1, REQ-302 `reset_key_keeps_vcount`, REQ-310 criterion 2, REQ-333 criterion 3 (`RTCLOK`), REQ-336 criteria 1-4, REQ-400 `registers_mirror_at_d020_d0ff`, REQ-402 criteria 6-7, REQ-435 criteria 3-5. | A system-test package |
| WP-2 | The display filters of REQ-411 and REQ-412 (Could) are not implemented. | A later slice |
| WP-2 | With the power-on values of the design, the display list starts on scan line 9 in the first frame, and on line 8 after that. | The lead: check `antic.md` |
| WP-5 | OI-017 is open again: with the real CPU, the NMI start cycles of REQ-334 and the BRK takeover of REQ-335 fail. | The user (OI-017) |
| WP-5 | The frontend does not call `Machine::set_control` yet, so CONTROL alone does nothing. | A frontend teammate |
| WP-7 | REQ-203 criterion 4 needs code that runs from `$D500`, so a cartridge must drive the bus at `$D5xx`. No cartridge kind in `media.md` does. `rdy_reads_reach_cartridge_control` checks the part that is possible. Change the criterion, or add such a cartridge kind. | The lead |
| WP-7 | `press_reset` in the REQ-302 test plan row was the name of a function, not a test. WP-7 wrote a real test with that name. Rename it to a test name (for example `press_reset_keeps_position`). | The lead and a teammate |
