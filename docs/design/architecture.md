# Architecture design

## Scope

This document gives the crate structure, the data flow between the chips, the scheduling
model and the frontend. It covers REQ-001 to REQ-019, REQ-040 to REQ-042, REQ-080 and REQ-081. The chip
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
| `state` | The byte format of the save states | This document, "Save states" |

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

## Determinism (REQ-014)

The core reads no host clock and no environment. The workspace `clippy.toml` lists
`std::time::Instant::now`, `std::time::SystemTime::now` and `std::env::var` in
`disallowed-methods`. The frontend (the frame pacing and the `--frames` time) and the test
harness (`UPDATE_GOLDEN`, the time limit of the specification gates) mark each function that
uses one with `#[expect(clippy::disallowed_methods, reason = "...")]`.

## Save states (REQ-018)

```rust
impl Machine {
    /// The complete machine state as bytes. The ROMs are not in the state.
    pub fn save_state(&self) -> Vec<u8>;
    /// Restores a state of `save_state` into a machine with the same `MachineConfig`.
    /// On an error the machine does not change.
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), StateError>;
}

pub enum StateError { Signature, Version(u16), Truncated, Checksum, Config, Invalid, TrailingBytes }
```

The module `state` gives the trait `State` (`save` to a `Writer`, `load` in place from a
`Reader`) for the number types, `bool`, `usize`, arrays, pairs, `Option`, `Vec` and
`VecDeque`, and two macros. Each chip module writes its own fields:
`impl_state!(Type { field, ... })` for a struct, with an optional `skip { ... }` list (a
constant table, for example the GTIA priority table) or `test_only { ... }` list (a field
with `#[cfg(test)]`), and `impl_state_enum!(Type { Variant, ... })` for an enum without
data. The struct macro destructures the struct without `..`, so a new field that is in no
list does not compile. An enum with data or a struct with a field from the configuration
has a manual `State` implementation (`antic::DmaKind`, `media::Cartridge`,
`machine::Memory`).

The format, all numbers little-endian:

| Part | Contents |
| --- | --- |
| Signature | 8 bytes `A65XEST` and `$1A` |
| Version | `u16`, 1 |
| Configuration hash | FNV-1a 64 of the video standard, the audio sample rate, the OS ROM, the BASIC ROM and whether a cartridge is inserted |
| Machine | The fields of `Machine` in their order: CPU, memory (RAM, then the cartridge: the FNV-1a 64 hash of its ROM, the bank and the enable flag; then `portb`), ANTIC, GTIA, POKEY, PIA, and the machine counters. A `Vec` or `VecDeque` is its length as `u64` and the elements. An enum is the index of the variant as `u64`. |
| Checksum | FNV-1a 64 of all bytes before it |

`load_state` checks the signature, the version, the checksum and the configuration hash in
this order, loads into a copy of the machine, and replaces the machine only when the load
succeeds and no byte is left. The audio samples that were not drained are in the state. When
a chip gets a new field, its `impl_state!` list gets the field, and the version increments.

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

### Emulation thread and frame pacing (REQ-007)

`window::run` runs the frame loop on an emulation thread and the window on the main thread
(macOS needs the window on the main thread). The window thread reads the host keys, gives
them to the emulation thread, and shows the last frame that the emulation thread gives. A
`minifb` window update waits for the display, and macOS can stop it for up to 1 s (measured
2026-11-20: one stop of 1.0 s in 3 of 4 runs of 300 or 600 frames). Before
this change, the loop ran in one thread, and `--frames 600` took 11.0 s instead of 10.0 s.
On its own thread, the emulation keeps the frame rate and gives the audio samples in time
while the window waits.

`Pacer` waits until the start time of the next frame. When a frame ends late by at most
`MAX_LATE` (100 ms), the next frame runs at once, and the emulation thread does not give it
to the window (at least one frame each `TURBO_PRESENT_INTERVAL` is still shown), so that the
frames catch up. When a frame ends more than `MAX_LATE` late, the pacing starts again from
the current time. `--frames` measures the time from the first frame to the end of the last
frame on the emulation thread.

### Audio output (REQ-007, REQ-011)

The module `audio` of the frontend owns the sound. `AudioOutput::open_default` opens a
`cpal` output stream on the default output device of the host, with the default sample rate,
channel count and sample format of the device. The frontend opens the stream before it
creates the machine, and `MachineConfig::audio_sample_rate` is the rate of the stream, so that
no resampling is necessary and the frontend does not change the rate of the device. Each
channel of the stream gets the same mono sample.

- `Frontend` takes the samples of each frame with `Machine::drain_audio` after the frame
  (`Frontend::audio`). The samples of the last frame replace the samples before them, also
  in headless mode, so that they do not collect. A step without a frame (pause) has no
  samples.
- `SampleRing` is the ring buffer between the frame loop and the audio callback (a
  `VecDeque<f32>` in a `Mutex`; the frame loop holds the lock only to copy one frame of
  samples). The ring is silent until it holds the target level: 50 ms of audio (`TARGET_MS`),
  or two callback requests when they are larger. Then it plays. When a callback asks for more
  samples than the ring holds, that is one underrun: the ring holds the last sample value for
  the rest of the request and is silent until it reaches the target level again. The ring
  holds at most 250 ms (`CAPACITY_MS`); more samples drop the oldest.
- Step 5: after each frame, `SampleRing::correction` moves the time of the next frame by
  `CORRECTION_GAIN` (0.05) of the level error (the level minus the target, in seconds of
  audio), at most 1 ms (`MAX_CORRECTION`). When the ring holds more than the target, the next
  frame comes later. So the frames follow the clock of the audio device, and the level stays
  near the target. The correction is 0 while the ring does not play, so a machine that makes
  no samples runs on the host clock only (`Pacer`).
- `AudioOutput` keeps the stream on the main thread; the emulation thread uses its
  `AudioSink` (`AudioOutput::sink`) for steps 4 and 5.
- While the emulation is paused or in turbo (REQ-080), the frame loop calls `AudioSink::stop`:
  the ring is emptied and is silent. This is not an underrun.
- No audio device, or a stream that cannot start: the frontend writes
  `atari-mac: warning: no sound: <reason>` to standard error and runs without sound and
  without the correction. `--headless` opens no stream.
- `--audio-stats` writes one line at exit, for example
  `audio: 48000 Hz, 2 channels, 480512 samples, 0 underruns, 0 dropped`, or
  `audio: off (<reason>)` without a stream.

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

Lower case letters (criterion 9). A letter key gives the letter in the case mode of the OS
screen editor (`SHFLOK`, `$02BE`), and the OS starts in upper case. So `bas::load` puts
CAPS (it toggles to lower case) before a lower case letter in upper case mode, and
SHIFT + CAPS (it sets upper case) before an upper case letter in lower case mode. After the
last program line, SHIFT + CAPS follows if the mode is lower case, so `RUN` and later keys
are in upper case. Other characters do not depend on the mode. Altirra BASIC does not accept
lower case statement names, so a line such as `10 print 1` is typed exactly and BASIC
shows an error for it.

`bas::Typist` types the key codes one frame at a time, so the window loop never blocks. The
frontend calls `Typist::before_frame` before each frame, in `Frontend::step` (window) and in
`Frontend::run_frames` (headless). The phases:

| Phase | Before each frame |
| --- | --- |
| `WaitReady(n)` | If a screen row is `Ready` (`screen::has_row`), go to `Typing` at key 0 in the same frame. Else, after 600 frames, stop with `FrontendError::BasicNotStarted` (exit status 1). |
| `Typing { index, tick }` | If key `index` is typed (see below), go to key `index + 1` (or to `Done`) at tick 0. At tick 0, `key_down(code)`; at tick `KEY_DOWN_FRAMES`, `key_up`. |
| `Done` | The frontend drops the typist. |

Key timing (`bas.rs`, criteria 9 and 10). Each key is down for `KEY_DOWN_FRAMES` = 1 frame
and up for `KEY_UP_FRAMES` = 1 frame, or for `SAME_KEY_UP_FRAMES` = 3 frames when the next
key code is the same. Then the typist waits until the OS variable `CH` (`$02FC`) is `$FF`
(the screen editor read the key), at most `KEY_READ_TIMEOUT_FRAMES` = 120 frames. Then the
key is typed. `examples/pong.bas` with `RUN` takes 1,161 frames after `Ready`.

The timing is measured with `AltirraOS` and Altirra BASIC (2026-11-19): a program with every
character that has a key, lines with doubled characters, and `examples/pong.bas` were typed
and compared with `LIST`. Without the `CH` wait:

| Down + up frames | Distinct keys | Same key twice | `pong.bas` |
| --- | --- | --- | --- |
| 1 + 1, 1 + 2, 2 + 1, 2 + 2 | correct | second key lost | keys lost |
| 3 + 3 (the harness timing) | correct | correct | correct, 3,300 frames |

The causes:

- POKEY (REQ-518) registers a press 64 to 128 scanlines after the key goes down, and a
  release 64 to 128 scanlines after it goes up. One frame (262 or 312 scanlines) is enough
  for both, so 1 + 1 types each distinct key once.
- Same key twice: the OS keyboard interrupt ignores the key code of the last key while the
  debounce counter `KEYDEL` (`$02F1`) is not 0. The OS sets it to 3 when it accepts a key,
  and the vertical blank interrupt decrements it only when it sees no key down. A frame
  starts at the vertical blank (scanline 248), so the first VBI after `key_up` still sees
  the key down. So the key must be up for 3 frames before the same key again (measured: 2
  frames up lose the key, 3 frames up do not; the key-down time has no effect).
- `pong.bas`: the OS has no key buffer. The interrupt writes each key to `CH`, also when
  `CH` still holds a key. After RETURN, BASIC stores the line for up to 7 frames (a long
  line), and the first digits of the next line number are lost (for example, line `110`
  became line `10`). A fixed wait after RETURN would depend on the length of the line and
  the program, so the typist waits for `CH` instead. With the `CH` wait, 1 + 1 (3 up before
  the same key) types all test programs correctly.
- The OS key repeat (`SRTIMR`, 48 frames before the first repeat) has no effect at 1 frame
  down.

The harness `type_text` (REQ-025) keeps its own timing (`keyboard::KEY_DOWN_FRAMES` and
`KEY_UP_FRAMES` = 3 + 3, without the `CH` wait). The harness tests pass with the `--bas`
timing, except `type_text_presses_return`, which checks the fixed frame count. But the
`CH` wait works only when the OS screen editor reads the keys: a program that reads
`KBCODE` itself never sets `CH` to `$FF`, and each key would wait for the timeout. The
harness types into such programs too, and tests (REQ-438) depend on its fixed frame count.

While the typist is active, `Frontend::step` ignores the host keys except quit (Command +
Escape), and pause has no effect. With `--bas`, the window title names the program file.

Turbo (criterion 7). The window loop calls `Pacing::after_frame(turbo, now)` after each
frame, with `turbo` = `Frontend::typist().is_some()`: from the start of the machine (the
wait for `Ready`) until the typing is done. In turbo, the wait is 0, and the window shows the
frame only when `TURBO_PRESENT_INTERVAL` (50 ms) of host time passed since the last shown
frame. One frame and one window update take much less than 50 ms, so the window shows a
frame at least every 100 ms. The window reads the host keys (quit) at each shown frame.
After turbo, `Pacing` starts a new `Pacer` at the current time, shows each frame and waits
as in REQ-007. Measured on the development Mac (release build, 2026-11-19): `pong.bas`
with `RUN` (196 frames to `Ready`, then 1,161 frames of typing) takes 2.15 s headless
(1.83 s for the typing) and 2.27 s in the window (about 1.9 s for the typing), against
22.6 s for 1,357 frames at real-time speed.

### Tape image from the command line (REQ-081)

`--cas <file>` sets `Options::cas`. `atari_mac::start` reads the file and calls
`media::Tape::from_cas` before it creates the machine, so a file that cannot be read
(`FrontendError::Read`) or a bad image (the `MediaError` of REQ-640, with the file name) gives
exit status 2 before the machine starts. `--cas` together with `--bas` is a usage error
(exit status 2), because both make key presses at the start. The help line is
`--cas <file>         CAS tape image: boot it (holds START at power-on)`.

The frontend then calls `Machine::start_tape` (`media.md`, "Tape boot"), which holds START
from the power-on cold reset until frame 60 (criterion 3, OI-050). It presses no OPTION and
no RETURN, so BASIC stays on. The user presses RETURN after the prompt tone, as on the real
computer. The frontend does not make key presses itself, and the host keys work as usual.

After WP-3 adds the option to `--help`, the README options table gets the row
`` | `--cas <file>` | Boot a CAS tape image (holds START at power-on; press RETURN after the tone) | ``.
Before that, the README gate (REQ-056 criterion 6) rejects the row.

### Turbo while the tape motor runs (REQ-082)

The window loop calls `Pacing::after_frame(turbo, now)` with `turbo` =
`Frontend::turbo()`: the typist is active (REQ-080 criterion 7), or
`Machine::tape_motor_on()` (`media.md`: a tape is inserted and the motor line, PIA CA2, is
low, REQ-641) at the start of the frame.
The value is taken before the frame, so a frame that starts while the motor line is low runs
in turbo, and a frame that starts while it is high runs at real-time speed. Without a tape,
the motor line has no effect. Turbo is the same as for `--bas`: no wait, a frame shown each
`TURBO_PRESENT_INTERVAL` (so at least every 100 ms of host time), the host keys read at each
shown frame, and a new `Pacer` from `now` after turbo. In turbo the frame loop calls
`AudioSink::stop` (no audio, not an underrun); the ring fills and plays again after turbo.
Turbo changes only the host pacing, so the emulated machine gives the same frames and RAM
(criterion 4).

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
| REQ-080 | `tests/cli.rs` `bas_runs_program` (1), `bas_no_run_only_types` (2), `bas_line_endings` (3), `bas_rejects_unknown_character` (4), `bas_missing_file` (5), `bas_waits_for_ready` (6), `bas_window_shows_typing` (7), `bas_help_and_no_run_alone` (8), `bas_typed_listing_equals_file` (9), `bas_typed_listing_keeps_lower_case` (9), `bas_pong_typing_frames` (10); `src/frontend.rs` unit test `bas_turbo_until_typed` (7) |
| REQ-081 | `tests/cli.rs` `cas_loads_tape` (1), `cas_rejects_bad_files` (2), `cas_key_presses` (3), `cas_boots_tape` (4, NTSC and PAL), `cas_help_lists_option` (5), `cas_with_bas_rejected` (6) |
| REQ-082 | `src/frontend.rs` unit tests `tape_turbo_follows_motor_line` (1, fake clock), `motor_line_without_tape_keeps_pacing` (2), `tape_turbo_presents_frames` (3), `tape_turbo_keeps_machine_state` (4), `tape_turbo_stops_audio` (5, fake device) |
| REQ-019 | `tests/cli.rs` `filters_off_without_flags` (1), `artifacting_flag_filters_screenshot` (2), `pal_blending_flag_filters_screenshot` (3), `help_lists_filter_flags` (4), `filter_flag_needs_matching_standard` (5); `src/cli.rs` unit test `filter_flags_and_standards` (5); `src/frontend.rs` unit test `filters_apply_to_their_standard` (2, 3) |
| REQ-007 | Demonstration: the commands in the acceptance criteria; evidence `docs/verification/REQ-007.md`. The pacing logic: `src/frontend.rs` unit tests `paced_frames_keep_the_frame_rate` (1, fake clock), `pacer_keeps_the_frame_rate` |
| REQ-011 | Demonstration: the commands in the acceptance criteria; evidence `docs/verification/REQ-011.md`. The ring and the wait correction: `src/audio.rs` unit tests `ring_plays_samples_in_order`, `ring_without_samples_is_silent`, `ring_counts_underruns`, `ring_drops_oldest_when_full`, `correction_follows_the_level`; `src/frontend.rs` unit tests `audio_ring_has_no_underruns` (1, fake clock and device), `step_hands_over_the_frame_audio` |
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
| REQ-018 | `crates/harness/tests/boot.rs` `save_state_round_trip`; `crates/atari-core/src/state.rs` unit test `values_round_trip` |
| REQ-042 | `crates/harness/tests/harness_self_test.rs` `coverage_job_needs_80_percent` (the CI workflow runs the coverage check); CI job `Coverage (cargo-llvm-cov)` |

## Open issues

1. The window crate is not decided finally. `minifb` has no game controller support.
2. When the Linux frontend becomes a goal again, it needs the X11, Wayland and ALSA
   development packages in CI, a quit key for Linux, a second fire key, and a requirement
   for a host without an audio device.
