# Architecture design

## Scope

This document gives the crate structure, the data flow between the chips, the scheduling
model, the frontend, and the logging and the statistics. It covers REQ-001 to REQ-019, REQ-040 to
REQ-042 and REQ-080 to REQ-085. The chip designs are in the other documents in this directory.

## Crates

| Crate | Path | Contents | Dependencies |
| --- | --- | --- | --- |
| `atari-core` | `crates/atari-core` | All emulation: chips, memory map, media formats | `log` only (REQ-015, "Logging and statistics") |
| `atari-harness` | `crates/harness` | Test helpers and the system test suites | `atari-core`, `serde`, `serde_json`, `png`, `yaml-rust2`; `log` for the tests only |
| `atari-mac` | `.` (root) | The frontend binary: window, audio, input, command line, log output | `atari-core`, `log`, window and audio crates |

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
| `events` (crate-private) | The `log` targets and the record macro of the machine event log (REQ-084) | This document, "Logging and statistics" |
| `disasm`, `debug` | The disassembler, the CPU hook types of the trace and the monitor | [debugger.md](debugger.md) |

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
`machine::Memory`, `media::DiskDrive`, the SIO bus phase).

The format, all numbers little-endian:

| Part | Contents |
| --- | --- |
| Signature | 8 bytes `A65XEST` and `$1A` |
| Version | `u16`, 2 (version 2 adds the SIO bus) |
| Configuration hash | FNV-1a 64 of the video standard, the audio sample rate, the OS ROM, the BASIC ROM and whether a cartridge is inserted |
| Machine | The fields of `Machine` in their order: CPU, memory (RAM, then the cartridge: the FNV-1a 64 hash of its ROM, the bank and the enable flag; then `portb`), ANTIC, GTIA, POKEY, PIA, the SIO bus (`media.md`, "State": all `SioBus` fields except `timing`, with drive D1: and its complete disk image), the PIA CB2 level, and the machine counters. A `Vec` or `VecDeque` is its length as `u64` and the elements. An enum is the index of the variant as `u64`. |
| Checksum | FNV-1a 64 of all bytes before it |

`load_state` checks the signature, the version, the checksum and the configuration hash in
this order, loads into a copy of the machine, and replaces the machine only when the load
succeeds, no byte is left, and the loaded values are valid (REQ-061): the video standard and
the audio rates of the chips are those of the configuration; the cycle counters and the
times of the machine, ANTIC, GTIA, POKEY, the SIO bus and the cassette deck are below
2^62 (`state::TIME_LIMIT`); the ANTIC scan line and cycle are in the frame; the GTIA write
queue and frame buffers have their sizes; and the POKEY volumes, serial bit counts,
keyboard scan counter and resampler phase are in range. A damaged state with a valid
checksum then gives `StateError::Invalid`, not a panic. The audio samples that were not drained are in the state. When
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

### Command-line errors (REQ-096, REQ-097)

`cli::parse` returns `FrontendError::Usage` for each error of REQ-096 and REQ-097. `main`
prints `atari-mac: <message>` on standard error and exits with `FrontendError::exit_code`
(2 for `Usage`). The checks run in this order:

1. The pre-scan (`known_len`) finds the first argument that is not an option name or the
   value of one (REQ-096 criterion 1). `clap` reads only the arguments before it, so a value
   error before that argument comes first (criterion 6).
2. The `clap` value parsers check the numbers and the `--scale` range in the order of the
   arguments (criteria 3, 4). A missing value can only be the last argument (criterion 2).
3. `TraceArgs::options`, then `check_combinations` (one table in the order of REQ-097
   criterion 8), then `check_filters` (REQ-019 criterion 5).

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

### Automatic RETURN for a tape boot (REQ-092)

`--auto-return` sets `Options::auto_return`. It needs `--cas`: without it, the parser gives
the usage error `--auto-return needs --cas: it presses RETURN for a tape boot` (exit status
2). `start_at` then calls `Frontend::with_auto_return`. Before each frame,
`Frontend::run_frame` presses RETURN (`Machine::key_down(keyboard::RETURN)`) when
`Machine::frame_count` is `auto_return_frame(standard)` (180 NTSC, 150 PAL), and releases it
(`key_up`) when the count is `AUTO_RETURN_FRAMES` (6) frames later. Then the frontend stops
the check. These are the frames of REQ-081 criterion 4, so RETURN is down in frames 180-185
(NTSC) or 150-155 (PAL). The press is in `run_frame`, so it works in the window and in the
headless run (`run_frames`). A host key change in `Frontend::step` can release RETURN early,
as on the real keyboard. The help line is
`--auto-return        With --cas: press RETURN after the prompt tone`.

### Boot with BASIC off (REQ-093)

`--no-basic` sets `Options::no_basic`. `--no-basic` together with `--bas` is a usage error
(exit status 2): `--no-basic and --bas cannot be used together: --bas types the program into
BASIC`. With `--xex`, the option is accepted and has no effect, because `start_xex` holds
OPTION itself (criterion 5).

The machine has one hold mechanism for console keys at the start:

```rust
/// The frames from the cold reset during which a start hold holds its keys.
pub const START_HOLD_FRAMES: u64 = 60;
/// Holds the pressed keys of `keys`, with the keys of an earlier call, until the end of
/// frame START_HOLD_FRAMES after the cold reset. No effect from that frame on. A cold reset
/// (`power_on`) ends the hold.
pub const fn hold_start_keys(&mut self, keys: ConsoleKeys);
```

`start_tape` holds START with it (REQ-081 criterion 3). After `start_xex` and `start_tape`,
`load_machine` calls `hold_start_keys` with OPTION when `no_basic` is set and there is no
`--xex`. The machine is then at frame 0 of the power-on cold reset (`Machine::new` or
`start_tape`), so OPTION is down in frames 0-59 (criterion 1), together with START for
`--cas` (criterion 3). The XL OS reads OPTION at the cold start and sets `PORTB` bit 1, so
BASIC is off (REQ-006). The hold is not in the save state (a key press of the frontend
start, as the other holds). `power_on` ends the hold, so the F10 cold reset holds no key:
the user holds F4 (OPTION) during F10. The help line is
`--no-basic           Hold OPTION at power-on: boot with BASIC off`.

### Tape shortcut option (REQ-091)

`Options::fast_tape` is `true` by default, and `--no-fast-tape` sets it to `false`.
`load_machine` gives it to `MachineConfig::fast_tape`, so the machine of the frontend has
the tape shortcut of REQ-644 (`media.md`, "Tape shortcut") unless the command line has
`--no-fast-tape`. The help line is
`--no-fast-tape       Read the tape records at the real speed (no tape shortcut)`, and the
README options table gets the row
`` | `--no-fast-tape` | Read the tape records at the real speed, with no tape shortcut (REQ-644) | ``.
With the option, a tape loads on the serial path as in REQ-643, with the turbo of REQ-082.

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

### Disk shortcut option (REQ-094)

`Options::fast_disk` is `true` by default, and `--no-fast-disk` sets it to `false`.
`load_machine` gives it to `MachineConfig::fast_sio`, so the machine of the frontend has the
SIOV shortcut of REQ-623 (`media.md`, "SIOV shortcut") unless the command line has
`--no-fast-disk`, also with `--xex` and with no media. The help line is
`--no-fast-disk       Read the disk at the real speed (no SIOV shortcut)`, and the README
options table gets the row
`` | `--no-fast-disk` | Read the disk at the real speed, with no SIOV shortcut (REQ-623) | ``.
With the option, a disk loads through the POKEY serial port as in REQ-636, with the turbo
of REQ-095.

### Turbo while the disk drive transfers data (REQ-095)

This is one more cause of the turbo of the tape (REQ-082), with the same `Pacing`, frame
display and audio rules. Before each frame, `Frontend::run_frame` takes
`Machine::disk_transfer()` (`media.md`, "Disk transfer"). The frame runs in turbo when D1:
has a disk (`Machine::disk().is_some()`) and

- `active` is `true` (criterion 1, a frame that starts while D1: takes part in a transfer),
  or
- `start - last_end <= DISK_TURBO_FRAMES * cycles_per_frame`, with `start` =
  `cycle_count() + 1`, the first cycle of the frame (criterion 1, a frame that starts within
  30 frames after the end of the last transfer).

```rust
/// The frames after the end of the last disk transfer that still run in turbo (REQ-095).
pub const DISK_TURBO_FRAMES: u64 = 30;
```

Else the disk gives no turbo (criteria 2 and 3). Without a disk, the bus has no effect. With
the SIOV shortcut on, a request through SIOV has no transfer on the bus, so it gives no
turbo. The field `tape_turbo` of `Frontend` becomes `media_turbo`: the last frame started
with the tape motor or the disk turbo. `Frontend::turbo()` is `typist.is_some() ||
media_turbo`, so the window loop, `after_step`, the audio stop and the statistics (REQ-083)
need no change (criterion 4). Turbo changes only the host pacing (criterion 5). The turbo
cause gets the variant `TurboCause::DiskDrive` ("Frontend records", criterion 6).

## Logging and statistics

This section is the design of SLICE-09: the machine event log in the core (REQ-084, WP-1),
the `--log` option (REQ-085, WP-2) and the `--stats` option (REQ-083, WP-2). WP-1 changes
`crates/atari-core` and adds the test file `crates/harness/tests/event_log.rs`. WP-2 changes
only the frontend (`src/`, `tests/`, the root `Cargo.toml` and the README options table).
WP-2 uses no item of WP-1: it uses the target names and the key names below as strings. So
the two packages can run in parallel.

### The `log` dependency (REQ-015)

The `[dependencies]` table of `crates/atari-core/Cargo.toml` has one entry:

```toml
# The machine event log (REQ-084). `kv` gives the frame and cycle counts to the logger.
log = { version = "0.4.35", default-features = false, features = ["kv"] }
```

- The `log` crate has no default features. The core does not turn on `std`, because the
  core uses only the `info!` and `debug!` macros.
- The `kv` feature has no dependency and does no I/O. The core does not use `kv_std`,
  `kv_serde`, `kv_sval` or a `max_level_*` feature. The frontend sets the level at run time.
- Version 0.4.35 is already in `Cargo.lock` (an Android dependency of the audio crates), so
  `Cargo.lock` gets no new crate version. Its license is MIT OR Apache-2.0, which
  `cargo-deny` accepts.
- The frontend (`[dependencies]` of the root `Cargo.toml`, WP-2) and the harness
  (`[dev-dependencies]` of `crates/harness/Cargo.toml`, WP-1) use the same line. The
  frontend installs its logger with `log::set_logger` and `Box::leak`, so it does not need
  the `std` feature either.

### Record format (REQ-084 criteria 1 and 2, REQ-085 criterion 2)

| Part | Value |
| --- | --- |
| Target | `atari::tape`, `atari::sio`, `atari::reset`, `atari::media` (core), `atari::frontend` (frontend) |
| Level | `Info`. The SIO data byte records are `Debug`. |
| Key `frame` | A `u64`: the frame count at the event (`Machine::frame_count`, the frames that completed before the event) |
| Key `cycle` | A `u64`: the number of the machine cycle of the event (see below) |
| Message | The text of the event tables below. The message does not contain the counts. |

The cycle number:

- Outside `Machine::run_cycle` (an insert, a reset, a frontend record), `cycle` is
  `Machine::cycle_count()` at the call.
- In `Machine::run_cycle`, `cycle` is `now` = `cycles + 1`, the number of the cycle in
  progress. This is the number that the SIO bus and POKEY use (`media.md`, "SIO bus and disk
  drive", contract 3). After the cycle, `Machine::cycle_count` gives the same number.

The counts are `log` key-values, not text in the message. The frontend reads them with
`record.key_values().get(Key::from_str("frame"))` and `Value::to_u64`, so it parses no text.
Another frontend can show the counts in its own format.

### The module `events` of the core

A new crate-private module `crates/atari-core/src/events.rs` (`mod events;` in `lib.rs`)
holds the targets and the record macro. The name is not `log`, because a module `log` makes
the path `log::info!` ambiguous.

```rust
pub(crate) const TAPE: &str = "atari::tape";
pub(crate) const SIO: &str = "atari::sio";
pub(crate) const RESET: &str = "atari::reset";
pub(crate) const MEDIA: &str = "atari::media";

/// Sends one record with the frame count and the cycle count (REQ-084 criterion 2).
/// `$level` is `info` or `debug`. `$frame` and `$cycle` are `u64` values.
macro_rules! record {
    ($level:ident, $target:expr, $frame:expr, $cycle:expr, $($message:tt)+) => {
        ::log::$level!(target: $target, frame = $frame, cycle = $cycle; $($message)+)
    };
}
pub(crate) use record;
```

Each record of the core uses `record!`. The core calls no other item of the `log` crate.

### Where the core sends each record (REQ-084 criterion 1)

`$xx` is a byte in hexadecimal with two digits. `<r>` is the index of a block in
`Tape::blocks`, from 0. `<b>` is the index of the data byte in block `<r>` at the tape
position (bit index / 10). It is 0 in the gap of a block and in a signal block.

| Category | Event | Module and function | Level | Message |
| --- | --- | --- | --- | --- |
| `tape` | The motor starts | `machine.rs`, the new private `Machine::motor_line(low, now)`. `run_cycle` step 6 and `power_on` call it at each change of CA2. | Info | `motor on, record <r> byte <b>`, or `motor on, no tape` |
| `tape` | The motor stops | `Machine::motor_line` | Info | `motor off, record <r> byte <b>`, or `motor off, no tape` |
| `tape` | A data record starts | `Machine::run_cycle` step 2, when `now` is the cycle of the next tape mark (see below) | Info | `record <r> starts: <n> bytes at <baud> baud` |
| `tape` | The tape ends | `Machine::run_cycle` step 2, as above | Info | `tape ends` |
| `tape` | The tape shortcut reads a record | `machine/siov.rs`, `tape_read` (`media.md`, "Tape shortcut") | Info | `tape shortcut: record <r>, <n> bytes, status $01` (`<n>` is the length of block `<r>`), or `tape shortcut: no record, status $8A` |
| `sio` | A command frame ends | `media/sio.rs`, `SioBus::set_command_line`, at the rising edge in `BusPhase::Command` | Info | `command device $31 command $52 aux1 $01 aux2 $00 checksum ok` (or `checksum bad`). A frame with a bad bit time or not 5 bytes: `command frame not valid: <n> bytes`. |
| `sio` | A device replies ACK or NAK | `SioBus::end_command_frame` (the command) and `SioBus::end_data_frame` (the data frame) | Info | `reply ACK at cycle <c>` or `reply NAK at cycle <c>` |
| `sio` | A device replies COMPLETE or ERROR | `SioBus::schedule_result` | Info | `reply COMPLETE at cycle <c>, data frame <n> bytes` (or `ERROR`). `<n>` is 0 without a data frame. |
| `sio` | The SIOV shortcut does a request | `machine/siov.rs`, `shortcut`, after `SioBus::transact` | Info | `SIOV device $31 command $52 aux1 $01 aux2 $00 status $01`, or the same text with `no device` in place of the status |
| `sio` | The computer sends a byte | `SioBus::host_byte` | Debug | `sent $xx` |
| `sio` | A device sends a data frame byte | `SioBus::schedule_result`, for each byte of the data frame and its checksum | Debug | `received $xx at cycle <c>` |
| `reset` | A cold reset | `Machine::power_on`, at its start | Info | `cold reset` |
| `reset` | A warm reset | `Machine::press_reset` | Info | `warm reset (RESET key)` |
| `media` | A cartridge is inserted | `Machine::new`, for the cartridge of the configuration | Info | `cartridge, <n> bytes, CAR type <t>` |
| `media` | An executable is inserted | `Machine::start_xex`, before `insert_disk` | Info | `executable, <n> bytes in <s> segments` |
| `media` | A disk image is inserted | `Machine::insert_disk` (also from `Machine::new` and `start_xex`) | Info | `disk, <n> bytes, <k> sectors of <z> bytes` |
| `media` | A tape image is inserted | `Machine::insert_tape` (also from `Machine::new` and `start_tape`) | Info | `tape, <n> bytes in <k> blocks` |

The data of the records:

- Cartridge: `<n>` is `CartKind::rom_size`, and `<t>` is the CAR type number. WP-1 adds
  `CartKind::car_type(self) -> u32` (1, 2, 41, 42). The message does not use `Debug`
  (the lint `clippy::use_debug`).
- Executable: `<n>` is the sum of the data lengths of the segments, and `<s>` is the
  number of segments.
- Disk: `<n>` is the sum of the sector lengths (`Geometry::sector_len`). `<k>` and `<z>` are
  the fields of `Geometry`.
- Tape: `<n>` is the sum of the byte counts of the data blocks, and `<k>` is the number of
  blocks.
- SIOV: `status $xx` is the status byte of `SioOutcome::Done`. `no device` is
  `SioOutcome::NoDevice`. A cassette request (`DDEVIC` = `$60`) does not reach `transact` and
  has no record.
- `Machine::new`, `start_xex` and `start_tape` call `power_on`, so each of them also sends
  `cold reset`. The reset records have the counts before the reset.

The counts of the SIO bus records:

- `SioBus` gets the field `log_frame: u64` and the function
  `pub(crate) const fn set_log_frame(&mut self, frame: u64)`. The machine calls it in
  `power_on`, at each frame end (`run_cycle` step 7) and after `load_state`. The field is in
  the `skip` list of `impl_state!`, so the save state does not change.
- The `cycle` of a record of the bus is the `now` argument of the call. For `host_byte`, it
  is `byte.end_cycle`.
- The bus schedules the reply bytes before they start on SIO DATA IN. So a reply record and
  a received byte record have the cycle of the decision (the end of the command frame or
  the data frame), and the message gives the start cycle `<c>` of the byte. A record at the
  start of each byte would need a check in each cycle.

The tape marks. A data record and the tape end start at a tape time, not at a call. So the
machine calculates the cycle of the next mark:

```rust
// crates/atari-core/src/media/deck.rs
pub enum TapeMark {
    /// A data block starts its first start bit (after its gap).
    Record { index: usize, len: usize, baud: u16 },
    /// The end of the last block.
    End,
}

impl TapeDeck {
    /// The block and the data byte at tape time `p` (`<r>` and `<b>`). `None` with no tape.
    /// After the end of the tape: the number of blocks and 0.
    pub fn locate(&self, p: u64) -> Option<(usize, usize)>;
    /// The first mark at tape time `p` or later, with its tape time. `None` with no tape and
    /// after the end.
    pub fn next_mark(&self, p: u64) -> Option<(u64, TapeMark)>;
}
```

- The machine gets the fields `tape_mark_at: u64` and `tape_mark: TapeMark`. `tape_mark_at`
  is the cycle of the next mark while the motor runs with a tape, else `u64::MAX`.
- The private function `Machine::plan_tape_mark(n)` sets the fields. `n` is the next cycle:
  `now + 1` in `run_cycle`, or `cycle_count() + 1` outside. For the mark at tape time `t`,
  `tape_mark_at = n + t - deck.position(n)`.
- The machine calls `plan_tape_mark` after each motor change, after `insert_tape`,
  `eject_tape` and `power_on`, after each mark, and after `load_state`.
- `run_cycle` step 2 compares `now` with `tape_mark_at`. When they are equal, it sends the
  record of the mark and calls `plan_tape_mark(now + 1)`. This is one comparison in each
  cycle.
- The fields are not in the save state: `load_state` calculates them again. They do not
  depend on the log level, so the machine is the same with the log on and off (criterion 4).

### No formatting when the log is off (REQ-084 criterion 5)

- The `log` macros compare the level with `log::STATIC_MAX_LEVEL` and `log::max_level()`
  first. Only then do they make the `format_args!` and call the logger. With the level `Off`,
  the core calls no logger function and formats nothing.
- Each message argument of `record!` is a value or a field (`u8`, `u16`, `u64`, `usize`,
  `&str`). An argument never calls `format!`, `to_string` or a function that allocates.
  `format_args!` only borrows the arguments.
- So the cost with the log off is one atomic load for each record call (a few for each SIO
  command) and the `tape_mark_at` comparison in each cycle. WP-1 measures the headless
  command of REQ-007 before and after the change and gives the two times in its report
  (REQ-084 Notes).
- The test `no_logger_calls_when_off` installs a logger that counts the calls of `enabled`,
  `log` and `flush`. With the level `Off`, it does the runs of criterion 3, and each count
  stays 0.

### Test logger of the core tests

The tests of REQ-084 are in `crates/harness/tests/event_log.rs`, because they need the free
ROMs and the boot media. The file makes the boot test tape of REQ-643 and the SD boot image
of REQ-636 criterion 1 in code (the same bytes as `crates/harness/tests/media.rs`).

- The logger and the maximum level are global to the process. So all tests of the file
  share one capture logger (installed once with `log::set_logger`), and each test holds one
  static `Mutex` while it runs. Each test sets the level that it needs (`Debug` or `Off`)
  after it gets the lock.
- The capture logger keeps each record as (target, level, frame, cycle, message text).
- No other test binary of the core and the harness installs a logger. So the other tests
  run with the level `Off`.

### Frontend logger (REQ-085)

The new module `src/logging.rs` holds the logger. The frontend writes its own small
`log::Log` implementation and uses no logger crate (for example `env_logger`). REQ-085 fixes
the line format and the category list, and a logger crate brings its own format, its own
filter syntax and more dependencies.

```rust
/// A category of `--log` (REQ-085 criterion 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category { Tape, Sio, Reset, Media, Frontend }

impl Category {
    pub const ALL: [Self; 5];
    /// `tape`, `sio`, `reset`, `media`, `frontend`.
    pub const fn name(self) -> &'static str;
    /// The category of the target `atari::<name>`. `None` for another target.
    pub fn from_target(target: &str) -> Option<Self>;
}

/// The categories that `--log` turns on, each at `Info` or `Debug`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Categories { /* one level for each category, or off */ }

impl Categories {
    /// Parses the value of `--log`.
    ///
    /// # Errors
    ///
    /// `FrontendError::Usage` that names the first item that is not valid.
    pub fn parse(text: &str) -> Result<Self, FrontendError>;
    /// A record of `target` at `level` is written.
    pub fn enabled(&self, target: &str, level: log::Level) -> bool;
    /// `Debug` when a category has `:debug`, else `Info`.
    pub fn max_level(&self) -> log::LevelFilter;
}

/// The line of a record: `[frame <f> cycle <c>] <category>: <message>`. A count that the
/// record does not have is `-`. `None` for a target that is not a category.
pub fn format_line(record: &log::Record<'_>) -> Option<String>;

/// The logger of `--log`: writes the line of each enabled record to standard error.
#[derive(Debug)]
pub struct StderrLogger { /* categories: Categories */ }

impl log::Log for StderrLogger { /* enabled, log, flush */ }

/// Installs a `StderrLogger` (`log::set_logger` with `Box::leak`) and sets the maximum
/// level to `categories.max_level()`. A second call keeps the first logger.
pub fn install(categories: Categories);
```

The parser of `--log`:

1. Split the value at each `,`.
2. Each item is `<name>` or `<name>:debug`. `<name>` is a category name or `all`. `all` is
   each category.
3. A category without `:debug` writes its `Info` records. With `:debug`, it also writes its
   `Debug` records. When a category is in more than one item, the highest level applies.
4. An item that is not valid (also an empty item) gives
   `FrontendError::Usage("unknown --log category \"<item>\" (known: tape, sio, reset, media, frontend, all, each with an optional :debug)")`.
   The exit status is 2 (REQ-085 criterion 4). `cli::parse` calls the parser, so the error
   comes before the machine starts.

The command line and the run:

- `cli::Options` gets `pub log: Option<Categories>` (`--log <categories>`) and
  `pub stats: bool` (`--stats`). A second `--log` replaces the first.
- The help lines:
  `--log <categories>   Write log records to standard error: tape,sio,reset,media,frontend,all`
  and `--stats              Each second, write the speed statistics to standard error`.
  WP-2 adds the two rows to the README options table, because the README gate (REQ-056
  criterion 6) compares the table with `--help`.
- `run_machine` calls `logging::install` before `start_at` when `options.log` is `Some`.
  Without `--log`, the frontend does not call `log::set_logger` or `log::set_max_level`, so
  the level stays `Off`, the default of the `log` crate (criterion 5).
- `StderrLogger::log` writes the line and `\n` with one `write_all` to the locked standard
  error, so the lines of two threads do not mix. A logger cannot give an error, so it
  ignores a write error.

### Frontend records (REQ-085 criterion 3)

The frontend sends its records with
`log::info!(target: "atari::frontend", frame = f, cycle = c; "...")`. `f` and `c` are
`Machine::frame_count()` and `Machine::cycle_count()` at the call, so they are the counts at
the start of the frame that comes next (criterion 2).

| Event | Function in `src/frontend.rs` | Message |
| --- | --- | --- |
| Turbo starts | `Frontend::run_frame`, before the frame, when the turbo cause changes | `turbo on (BASIC typing)`, `turbo on (tape motor)` or `turbo on (disk drive)` (REQ-095) |
| Turbo stops | `Frontend::run_frame`, as above | `turbo off` |
| Pause | `Frontend::apply`, at the F9 edge, when the emulation stops | `paused` |
| Continue | `Frontend::apply`, at the F9 edge, when the emulation continues | `continued` |
| Cold reset from F10 | `Frontend::apply`, at the F10 edge, before `Machine::power_on` | `cold reset (F10)` |

The turbo cause:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurboCause {
    /// The typist types the BASIC program (REQ-080 criterion 7).
    Typing,
    /// A tape is inserted and the motor line is low (REQ-082).
    TapeMotor,
    /// D1: has a disk and takes part in a transfer, or did within 30 frames (REQ-095).
    DiskDrive,
}

impl Frontend {
    /// The turbo cause of the last frame that ran, or `None` for real-time speed.
    pub const fn turbo_cause(&self) -> Option<TurboCause>;
}
```

Before each frame, `run_frame` calculates the cause: `Typing` while the typist is active,
else `TapeMotor` when `Machine::tape_motor_on()`, else `DiskDrive` with the disk turbo of
REQ-095, else `None`. When the cause is not the
cause of the last frame, it sends the record. So a change from `Typing` to `TapeMotor` sends
`turbo on (tape motor)` and no `turbo off`. A pause runs no frame, so the cause does not
change. `Frontend::turbo` does not change.

### Speed statistics (REQ-083)

The new module `src/stats.rs` holds the accumulator. It takes the host time as an argument,
as `Pacing` does, so the unit tests use a fake clock.

```rust
/// One second of statistics.
pub const INTERVAL: Duration = Duration::from_secs(1);

/// The cassette motor at a frame (REQ-083 criterion 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motor { On, Off, NoTape }

impl Motor {
    /// `NoTape` when `Machine::tape_position()` is `None`, else `On` or `Off` from the deck.
    pub fn of(machine: &Machine) -> Self;
    /// `on`, `off`, `-`.
    pub const fn name(self) -> &'static str;
}

/// The totals that other parts of the frontend count since the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    /// The frames that the window showed. 0 with `--headless`.
    pub shown: u64,
    /// The audio underruns (REQ-011). 0 with `--headless`.
    pub underruns: u64,
}

/// One frame that ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    /// The host time of the emulation of the frame: `Frontend::step` or one frame of
    /// `Frontend::run_frames`, without the wait and without the window update.
    pub emulation: Duration,
    /// `Frontend::turbo()` after the frame.
    pub turbo: bool,
    /// `Motor::of` the machine after the frame.
    pub motor: Motor,
}

#[derive(Debug, Clone)]
pub struct Stats { /* see below */ }

impl Stats {
    /// Starts the first second at `now`, the start of the first frame. `counters` are the
    /// totals at `now`, and `motor` is the motor before the first frame.
    pub fn new(standard: VideoStandard, now: Instant, counters: Counters, motor: Motor) -> Self;
    /// Counts one frame that ran.
    pub fn frame(&mut self, frame: FrameStats);
    /// Called at `now` after each step. When `INTERVAL` or more passed since the start of
    /// the current second, gives the line and starts the next second at `now`.
    pub fn tick(&mut self, now: Instant, counters: Counters) -> Option<String>;
}
```

The line of `tick`:
`stats: frame <n> fps <f> speed <p>% shown <s> emu <m> ms turbo <on|off> motor <on|off|-> underruns <u>`.

| Value | Calculation |
| --- | --- |
| `<n>` | The count of `frame` calls since `new` |
| `<f>` | The frames of this second divided by `(now - start of the second).as_secs_f64()`, `{:.1}` |
| `<p>` | `<f>` / `standard.frames_per_second()` x 100, `f64::round`, as an integer |
| `<s>` | `counters.shown` minus the total at the start of the second |
| `<m>` | The mean `emulation` of the frames of this second in ms, `{:.2}`. `0.00` when no frame ran. |
| `turbo`, `motor` | The values of the last frame that ran. Before the first frame: `off` and the `motor` of `new`. |
| `<u>` | `counters.underruns` minus the total at the start of the second |

The next second starts at `now`, not at the start time plus `INTERVAL`. So after a stop of
the host, the frontend writes one line and not a series of lines.

The window loop (`src/window.rs`, `emulate`), with `--stats`:

1. `Shared` gets `shown: AtomicU64`. The window thread adds 1 after each
   `update_with_buffer` of a new frame that succeeds.
2. `FrameAudio` gets `fn underruns(&self) -> u64`. `AudioSink::underrun_count` (the lint `same_name_method` rejects the name `underruns`) gives the count of
   its ring, or 0 without a stream. The fake devices of the tests give their own count.
3. `emulate` makes the `Stats` at `start` (before the first step).
4. For each step: `t0 = Instant::now()`, `Frontend::step`, `t1 = Instant::now()`. When
   `frames_run` increased, it calls `stats.frame` with `t1 - t0`. Then it calls `after_step`
   at `t1`, and `stats.tick(t1, counters)` with `shared.shown` and `audio.underruns()`.
5. A line from `tick` goes to standard error with `writeln!` (an error is
   `FrontendError::Output`). Then the thread waits (`after.wait`).

The headless loop (`src/lib.rs`, `run_machine`), with `--stats`: it runs the frames one at a
time with `Frontend::run_frames(1)`, measures each frame as in step 4, and calls `tick` with
`Counters::default()` after each frame. Without `--stats`, the loops do not change and make
no `Stats`.

### Tests of the slice

The rows for REQ-083 to REQ-085 are in "Test plan" below.

- `src/stats.rs` and `src/logging.rs`: unit tests with a fake clock, and records that the
  test makes with `log::Record::builder` and `key_values`. They install no logger.
- `tests/frontend_log.rs`: a separate test binary with a capture logger (one record list for
  each thread), so that the level of the other frontend tests stays `Off`.
- `tests/cli.rs`: the binary, in a child process. `no_log_without_option` also calls
  `atari_mac::run` in the test process and checks that `log::max_level()` is `Off` after it.
- `crates/harness/tests/event_log.rs`: the core records, with the free ROMs.
- `log_writes_tape_lines`: the test runs the headless binary with `--headless --frames 2400
  --log tape --no-fast-tape --auto-return --cas`: `--auto-return` (REQ-092) presses RETURN
  in frames 180-186, and `--no-fast-tape` keeps the serial path, so that each record start
  has a line. The test checks the
  lines that come: `motor on`, the three record lines and `tape ends`, with counts that
  increase. The OS does not stop the motor after this boot, so no `motor off` line comes;
  `event_log.rs` `tape_records` presses RESET to get it.

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
| REQ-092 | `tests/cli.rs` `auto_return_presses_return` (1, NTSC and PAL), `auto_return_headless_boot` (2, NTSC and PAL), `auto_return_needs_cas` (3), `cas_without_auto_return` (4), `auto_return_help` (5) |
| REQ-093 | `tests/cli.rs` `no_basic_holds_option` (1), `no_basic_ram_at_a000` (2), `no_basic_with_cas` (3), `no_basic_with_disk` (4), `no_basic_with_bas_and_xex` (5), `no_basic_help` (6) |
| REQ-096 | `src/cli.rs` unit tests `unknown_argument_forms` (criterion 1), `missing_value_is_last` (2), `bad_number_values` (3), `scale_out_of_range` (4), `first_syntax_error_wins` (6); `tests/cli_errors.rs` `syntax_error_output` (5) |
| REQ-097 | `src/cli.rs` unit tests `combination_messages` (criteria 1 to 6), `combination_order` (8); `tests/cli_errors.rs` `combination_error_output` (7) |
| REQ-091 | `tests/cli.rs` `fast_tape_default_on` (1), `no_fast_tape_option` (2), `no_fast_tape_help` (3); `src/cli.rs` unit test `no_fast_tape_parses` |
| REQ-094 | `tests/cli.rs` `fast_disk_default_on` (1, with `--disk`, `--xex` and no media), `no_fast_disk_option` (2: the boot test disk of REQ-636 boots on the serial path, with the disk turbo), `fast_disk_boots_faster` (3), `no_fast_disk_help` (4); `src/cli.rs` unit test `no_fast_disk_parses` |
| REQ-095 | `src/frontend.rs` unit tests `disk_turbo_follows_transfers` (1, 2, fake clock, the boot test disk of REQ-636 with `AltirraOS` and the shortcut off), `empty_drive_keeps_pacing` (3), `disk_turbo_presents_frames` (4, fake clock and device), `disk_turbo_keeps_machine_state` (5); `tests/frontend_log.rs` `disk_turbo_records` (6); the core query of `media.md`, "Disk transfer": `crates/atari-core/src/media/tests/sio.rs` `req095_transfer_span` (1: a read, a write and a NAK), `req095_transfer_edges` (1: a bad frame, a frame for another device, a cut reply, `transact`, a removed drive and `reset`), each with a host that drives `SioBus`, `crates/harness/tests/media.rs` `req095_transfer_on_serial_boot` (1: with `AltirraOS` and the shortcut off, `Machine::disk_transfer` is active from the start bit of the first command byte, `IRQST` bit 3, to the end of the last reply byte, cycle by cycle) |
| REQ-082 | `src/frontend.rs` unit tests `tape_turbo_follows_motor_line` (1, fake clock), `motor_line_without_tape_keeps_pacing` (2), `tape_turbo_presents_frames` (3), `tape_turbo_keeps_machine_state` (4), `tape_turbo_stops_audio` (5, fake device) |
| REQ-083 | `src/stats.rs` unit tests `stats_line_each_second` (1, fake clock), `stats_frame_rate_and_speed` (2, 3), `stats_shown_and_underruns` (4, 7), `stats_emulation_time` (5), `stats_turbo_and_motor` (6); `tests/cli.rs` `stats_headless_lines` (1, 4, 7), `no_stats_without_option` (8), `stats_help_lists_option` (9) |
| REQ-084 | `crates/harness/tests/event_log.rs` `tape_records` (1, 2, 3), `sio_records` (1, 2, 3, the serial path and the SIOV shortcut), `reset_records` (1, 2, 3), `media_records` (1, 2, 3), `log_keeps_machine_state` (4), `no_logger_calls_when_off` (5) |
| REQ-085 | `src/logging.rs` unit tests `log_categories_parse` (1, 4), `logger_filters_category_and_level` (1), `log_line_format` (2); `tests/frontend_log.rs` `frontend_records` (2, 3); `tests/cli.rs` `log_writes_tape_lines` (2, the tape lines of the headless binary), `log_rejects_unknown_category` (4), `no_log_without_option` (5), `log_help_lists_option` (6) |
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
