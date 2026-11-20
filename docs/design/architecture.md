# Architecture design

## Scope

This document gives the crate structure, the data flow between the chips, the scheduling
model and the frontend. It covers REQ-001 to REQ-019, REQ-040 to REQ-042 and REQ-080. The chip
designs are in the other documents in this directory.

## Crates

| Crate | Path | Contents | Dependencies |
| --- | --- | --- | --- |
| `atari-core` | `crates/atari-core` | All emulation: chips, memory map, media formats | None (REQ-015) |
| `atari-harness` | `crates/harness` | Test helpers and the system test suites | `atari-core`, `serde`, `serde_json`, `png` |
| `atari-mac` | `.` (root) | The frontend binary: window, audio, input, command line | `atari-core`, window and audio crates |

The workspace lints apply to all three crates. The core has no I/O. The frontend owns all
files, the clock of the host, the window and the audio device (REQ-014).

## Modules of `atari-core`

| Module | Owns | Design |
| --- | --- | --- |
| `cpu` | 6502C registers, instruction execution, interrupt sequences | [cpu.md](cpu.md) |
| `machine` | Memory map, `PORTB` banking, bus scheduling, interrupt routing | [system.md](system.md) |
| `pia` | 6520 PIA | [system.md](system.md) |
| `antic` | ANTIC, DMA, display list, NMIs | [antic.md](antic.md) |
| `gtia` | GTIA, frame buffer | [gtia.md](gtia.md) |
| `palette` | Color byte to RGB | [gtia.md](gtia.md) |
| `pokey` | POKEY | [pokey.md](pokey.md) |
| `keyboard` | `KBCODE` values, the ASCII table and the key timing of typed text | [pokey.md](pokey.md) |
| `screen` | The text of the OS text screen in memory (REQ-024) | [test-harness.md](test-harness.md) |
| `media` | Cartridges, executables, disk images, SIO devices | [media.md](media.md) |
| `timing` | Clock and frame constants | This document |

A module talks to another module only through the public interface in its design document.

## Scheduling model

The unit of time is one machine cycle (one CPU clock, 114 in each scanline). The machine
runs all chips one cycle at a time. The exact order of the calls in one machine cycle is in
[system.md](system.md), "Timing". In short:

1. ANTIC calculates the cycle: its DMA access, the halt signal (DMA), the RDY signal
   (`WSYNC`) and the NMI line.
2. POKEY runs the cycle, with the SIO DATA IN level from the SIO bus.
3. The CPU gets the NMI and IRQ line levels. If ANTIC does not halt it, the CPU makes
   exactly one bus access. When RDY is low, the 6502 repeats a read cycle, but it completes
   a write cycle (HRM 4.9).
4. ANTIC and GTIA get the data bus value of the cycle. GTIA makes two color clocks.
5. The PIA runs the cycle. The serial bytes from POKEY and the SIO command line go to the
   SIO bus.

Each chip type derives `Debug`, `Clone` and `PartialEq`, so that a test can copy a chip and
compare it (OI-005).

The CPU polls the interrupt lines in the cycles where the real 6502 polls them, so the
interrupt timing of HRM 3.4 and 4.8 is exact. A jammed CPU still makes its bus reads, and
the other chips continue to run.

## Frame and audio output

ANTIC reports `frame_done` on the first cycle of scanline 248. At that time the GTIA frame
buffer holds a complete frame: 384 x 240 color bytes. `Machine::run_frame` runs until the
next `frame_done`. The frontend converts the color bytes with `palette::rgb`.

POKEY produces mono `f32` samples at the sample rate in `MachineConfig`. The frontend takes
them with `Machine::drain_audio` after each frame.

## Frontend

The frontend runs on macOS only. Linux and Windows frontends are non-goals (see
`docs/spec/README.md`). The frontend uses `minifb` for the window and the keyboard, and
`cpal` for audio. Both have permissive licenses. If `cargo-deny` rejects a transitive
dependency, the decision is reviewed here.

### CI targets

| Crate | macOS job | Linux jobs |
| --- | --- | --- |
| `atari-core` | Build and test | Build, test, clippy, docs, coverage, cargo-deny, cargo-hack |
| `atari-harness` | Build and test | Build, test, clippy, docs, coverage, cargo-deny, cargo-hack |
| `atari-mac` (frontend) | Build, test, clippy | Not built: the Linux jobs use `--workspace --exclude atari-mac` after the frontend gets its macOS dependencies |

The Linux runners are cheaper. On a private repository, GitHub bills a macOS minute at 10
times a Linux minute.

The frontend loop:

1. Read the host keys and send them to the machine.
2. Run one frame.
3. Convert the frame to RGB and show it.
4. Send the audio samples to a ring buffer. The audio callback reads from the ring buffer.
5. Wait until the time of the next frame. The audio buffer level corrects the wait time,
   so that the audio does not have gaps (REQ-011).

### Key mapping (REQ-009, REQ-010, REQ-017)

| Host key | Atari key |
| --- | --- |
| Letters, digits, punctuation | The same character |
| Return | RETURN |
| Delete (Backspace) | DELETE/BACK SPACE |
| Tab | TAB |
| Escape | ESC |
| Left Shift, Right Shift | SHIFT |
| Control | CONTROL |
| Caps Lock | CAPS |
| F1 | Inverse video key (Atari logo key) |
| F2, F3, F4 | START, SELECT, OPTION |
| F5 | RESET |
| F6 | HELP |
| F7 | BREAK |
| F9 | Pause or continue |
| F10 | Cold reset |
| Arrow keys | Joystick 1 directions |
| Left Alt (Option) | Joystick 1 trigger |
| Command + Escape | Quit |

### Display filters (REQ-019)

`--artifacting` and `--pal-blending` set `Options::filters` (`frontend::DisplayFilters`, both
off by default). `frontend::convert_row` converts one frame buffer row: with artifacting on an
NTSC machine it calls `palette::artifact_ntsc_row`, with PAL blending on a PAL machine it calls
`palette::pal_blend_row` for rows 1-239 (row 0 is not blended, OI-021), and else
`palette::rgb`. The window (`render`) and the screenshot (`write_png`) both use it.
`cli::check_filters` rejects a filter for the other video standard with a usage error (exit
status 2) that names the flag and the standard. `--pal` sets `Options::standard` to PAL
(REQ-002 criterion 4); without it the machine is NTSC. With `--frames`, the run time line also
names the video standard and the cycles of the last frame, for example
`50 frames in 0.120 s (PAL, last frame 35568 cycles)`.

### BASIC program from a text file (REQ-080)

`--bas <file>` sets `Options::bas`, and `--no-run` sets `Options::no_run` (a usage error
without `--bas`). `atari_mac::start` creates the frontend: it first reads and checks the
whole file with `bas::load`, and only then creates the machine. So a file that cannot be read
or is not UTF-8 (`FrontendError::Read`), or a file with a character without a key
(`FrontendError::BasCharacter`: file, line number from 1, the character), gives exit status 2
before the machine starts. `bas::load` splits the text with
`str::lines` (LF or CR LF, the last newline is optional), skips each line that is empty or
has only spaces, and converts each character with `keyboard::ascii_to_kbcode`. Each line
ends with RETURN; then `RUN` and RETURN follow, unless `--no-run`.

`bas::Typist` types the key codes one frame at a time, so the window loop never blocks. The
frontend calls `Typist::before_frame` before each frame, in `Frontend::step` (window) and in
`Frontend::run_frames` (headless). The phases:

| Phase | Before each frame |
| --- | --- |
| `WaitReady(n)` | If a screen row is `Ready` (`screen::has_row`), go to `Typing` at key 0 in the same frame. Else, after 600 frames, stop with `FrontendError::BasicNotStarted` (exit status 1). |
| `Typing { index, tick }` | At tick 0, `key_down(code)`; at tick `KEY_DOWN_FRAMES`, `key_up`. After `KEY_DOWN_FRAMES + KEY_UP_FRAMES` ticks, go to the next key, or to `Done`. |
| `Done` | The frontend drops the typist. |

The key timing is the timing of the harness `type_text` (3 frames down, 3 frames up). While
the typist is active, `Frontend::step` ignores the host keys except quit (Command + Escape),
and pause has no effect. With `--bas`, the window title names the program file.

## Error handling

The core returns errors as values (`ConfigError`, `MediaError`). It never panics (REQ-016).
The frontend prints each error to standard error and exits with status 2 for a usage or
file error.

## Test plan

| Requirement | Test |
| --- | --- |
| REQ-002 | `crates/harness/tests/boot.rs` `frame_length_matches_standard` (criteria 1, 2), `default_standard_is_ntsc` (criterion 3); `tests/cli.rs` `pal_flag_selects_pal` (criterion 4) |
| REQ-003 | `crates/harness/tests/harness_self_test.rs` `free_roms_have_expected_sizes`, `no_rom_is_tracked_by_git` |
| REQ-004 | `crates/harness/tests/boot.rs` `accepts_user_roms`, `rejects_rom_with_wrong_size`; `tests/cli.rs` `rejects_bad_os_rom` |
| REQ-005 | `crates/harness/tests/boot.rs` `boots_to_basic_ready` |
| REQ-006 | `crates/harness/tests/boot.rs` `option_key_disables_basic`, `basic_is_mapped_without_option` |
| REQ-012 | `tests/cli.rs` `loads_cart_xex_and_disk` (criterion 1), `rejects_bad_cart_xex_and_disk` (criterion 2) |
| REQ-013 | `tests/cli.rs` `headless_writes_screenshot` |
| REQ-080 | `tests/cli.rs` `bas_runs_program` (1), `bas_no_run_only_types` (2), `bas_line_endings` (3), `bas_rejects_unknown_character` (4), `bas_missing_file` (5), `bas_waits_for_ready` (6), `bas_window_shows_typing` (7), `bas_help_and_no_run_alone` (8), `bas_turbo_until_typed` (7, frontend unit test), `bas_typed_listing_equals_file` (9), `bas_pong_typing_frames` (10) |
| REQ-019 | `tests/cli.rs` `filters_off_without_flags` (1), `artifacting_flag_filters_screenshot` (2), `pal_blending_flag_filters_screenshot` (3), `help_lists_filter_flags` (4), `filter_flag_needs_matching_standard` (5); `src/cli.rs` unit test `filter_flags_and_standards` (5); `src/frontend.rs` unit test `filters_apply_to_their_standard` (2, 3) |
| REQ-007 | Demonstration: the commands in the acceptance criteria; evidence `docs/verification/REQ-007.md` |
| REQ-011 | Demonstration: the commands in the acceptance criteria; evidence `docs/verification/REQ-011.md` |
| REQ-008 | Demonstration: the commands in the acceptance criteria; evidence `docs/verification/REQ-008.md` |
| REQ-001 | Inspection: a review of the repository against the acceptance criteria; evidence `docs/verification/REQ-001.md` |
| REQ-015 | Inspection: a review of the repository against the acceptance criteria; evidence `docs/verification/REQ-015.md` |
| REQ-040 | Inspection: a review of the repository against the acceptance criteria; evidence `docs/verification/REQ-040.md` |
| REQ-041 | Inspection: a review of the repository against the acceptance criteria; evidence `docs/verification/REQ-041.md` |
| REQ-009 | `src/keymap.rs` unit tests `every_table_entry_maps`, `every_atari_key_has_a_host_key` |
| REQ-010 | `src/keymap.rs` unit test `joystick_keys` |
| REQ-017 | `src/keymap.rs` unit test `control_keys` |
| REQ-017 | `src/frontend.rs` unit test `pause_stops_frames` |
| REQ-014 | `crates/harness/tests/boot.rs` `emulation_is_deterministic`; clippy `disallowed-methods` |
| REQ-016 | `crates/harness/tests/robustness.rs` `random_files_do_not_panic`, `random_programs_do_not_panic` |
| REQ-018 | `crates/harness/tests/boot.rs` `save_state_round_trip` |
| REQ-042 | `crates/harness/tests/harness_self_test.rs` `coverage_job_needs_80_percent` (the CI workflow runs the coverage check); CI job `Coverage (cargo-llvm-cov)` |

## Open issues

1. The window crate is not decided finally. `minifb` has no game controller support.
2. When the Linux frontend becomes a goal again, it needs the X11, Wayland and ALSA
   development packages in CI, a quit key for Linux, a second fire key, and a requirement
   for a host without an audio device.
