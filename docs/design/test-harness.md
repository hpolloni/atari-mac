# Test harness design

## Scope

This document gives the design of the test harness: the `atari-harness` crate, the test
data, and the test suites. It covers REQ-020 to REQ-027. REQ-022 is withdrawn; the Klaus Dormann test verifies REQ-110.

## Test data

`scripts/fetch-roms.sh` downloads all test data and the pinned references. Each source is
pinned to a commit or an archive snapshot, and each file has a checksum (REQ-020). Git
ignores the output directories.

| Path | Contents | Source | License | Checksum |
| --- | --- | --- | --- | --- |
| `roms/altirraos-xl.rom` | AltirraOS 3.11 | atari800 `src/roms/altirraos_xl.c` | Permissive (Avery Lee) | SHA-256 |
| `roms/altirra-basic.rom` | Altirra BASIC 1.59 | atari800 `src/roms/altirra_basic.c` | Permissive (Avery Lee) | SHA-256 |
| `test-data/klaus/6502_functional_test.bin` | 6502 functional test | Klaus2m5/6502_65C02_functional_tests | GPL-3.0 | SHA-256 |
| `test-data/single-step/6502/*.json` | 256 files, 10,000 cases each (1.1 GB) | SingleStepTests/65x02 `6502/v1` | MIT | Git blob SHA-1 |
| `reference/pinned/<file>` | The references with `storage: pin` (REQ-046) | The `url` of each registry entry in `docs/spec/references/` | See each registry entry | SHA-256 (`sha256` of the entry) |

`make roms` downloads the small files. `make test-data` also downloads the single-step suite.
`make test` and CI download the single-step suite, because REQ-023 is a Must requirement.

`make reference` (`scripts/fetch-roms.sh --references`) downloads only the pinned
references. It reads the `storage`, `url`, `sha256` and `file` of each registry entry and
uses the same checksum check as the other files. No other `make` target and no CI job
downloads a reference, so a change at a reference site cannot stop the tests. The HRM and
the FAQ are in git (`reference/`, storage `commit`), so no download is necessary for them.

## Crate `atari-harness`

The crate is in `crates/harness`. Its library holds the helpers. Its `tests/` directory
holds the system test suites. The helpers are a library, so that all test files use the
same code, and so that the lints apply to the helpers.

### Module `data`

```rust
pub fn repo_root() -> PathBuf;
pub fn read_test_file(relative: &str, fetch_command: &str) -> Result<Vec<u8>, HarnessError>;
pub fn os_rom() -> Result<Vec<u8>, HarnessError>;      // roms/altirraos-xl.rom
pub fn basic_rom() -> Result<Vec<u8>, HarnessError>;   // roms/altirra-basic.rom
```

A missing file gives `HarnessError::Missing`. Its message names the file and the command
that downloads it, for example `make test-data` (REQ-021).

### Module `system`

```rust
pub struct BootOptions { pub basic: bool, pub standard: VideoStandard }
pub fn boot(options: BootOptions) -> Result<Machine, HarnessError>;
pub fn run_frames(machine: &mut Machine, frames: u32);
pub fn run_until(machine: &mut Machine, max_frames: u32,
                 condition: impl FnMut(&Machine) -> bool) -> Option<u32>;
```

`run_until` returns the number of frames that ran before the condition became true, or
`None` after `max_frames`.

```rust
pub fn test_os_rom(program: &[u8]) -> Vec<u8>;
pub fn boot_program(program: &[u8], options: BootOptions) -> Result<Machine, HarnessError>;
```

System tests of a chip often need their own 6502 program, without AltirraOS. `test_os_rom`
builds a 16 KB OS ROM image: `program` at `$C000`, the RESET vector at `$C000`, and the NMI
and IRQ vectors at an `RTI` at the end of the program area. `boot_program` creates a machine
with this ROM and runs the power-on reset. The tests then use `run_until`, `peek` and
`cycle_count`.

### Module `screen`

```rust
pub fn screen_text(machine: &Machine) -> Vec<String>;
pub fn internal_to_ascii(code: u8) -> char;
pub fn has_row(machine: &Machine, text: &str) -> bool;
```

The functions and the constants `ROWS`, `COLUMNS` and `SAVMSC` are in the core module
`atari_core::screen`, because the frontend also needs `has_row` to wait for the BASIC
`Ready` prompt (REQ-080). The harness module `screen` re-exports them, so the harness API
does not change.

`has_row` compares a row after it removes leading and trailing spaces, because the OS text
screen has a left margin of 2 columns. `screen_text` reads the screen address from `SAVMSC` (`$58-$59`) and decodes 24 rows of 40
ANTIC internal character codes. Bit 7 (inverse video) is ignored. Codes `$00-$3F` are
ATASCII `$20-$5F`, codes `$40-$5F` are ATASCII `$00-$1F`, and codes `$60-$7F` are the same
in both sets. A character that is not printable ASCII becomes `?`. This depends only on the
CPU, the memory map and the OS, not on ANTIC or GTIA. So the boot tests can pass before the
video chips are complete.

### Module `input`

```rust
pub fn type_text(machine: &mut Machine, text: &str) -> Result<(), HarnessError>;
```

For each character, `type_text` presses the key for 3 frames and releases it for 3 frames
(`KEY_DOWN_FRAMES` and `KEY_UP_FRAMES`, which are in the core module `keyboard` and which
`input` re-exports). It uses `keyboard::ascii_to_kbcode`. A character without a key gives
`HarnessError::NoKey`.

### Module `single_step`

```rust
pub struct Case { pub name: String, pub initial: State, pub r#final: State,
                  pub cycles: Vec<(u16, u8, Access)> }
pub struct FlatBus { /* 64 KB RAM, a log of bus cycles */ }
pub fn load_cases(path: &Path) -> Result<Vec<Case>, HarnessError>;
pub fn run_case(case: &Case, bus: &mut FlatBus) -> Result<(), Mismatch>;
```

`FlatBus` implements `cpu::Bus` with 64 KB of RAM. It records each access as
`(address, value, read or write)`. `run_case` sets the registers and the memory from
`initial`, calls `Cpu::tick(bus, true)` once for each entry of `cycles` (see the call
contract in [cpu.md](cpu.md)), checks that the CPU is then at an instruction boundary, and
compares the registers, the memory and the bus log with `final` and `cycles`. After the case, it clears only the addresses that the case used,
so that a case takes no more time than its size.

### Module `image`

```rust
pub fn frame_to_rgb(machine: &Machine) -> Vec<u8>;
pub fn save_png(path: &Path, rgb: &[u8]) -> Result<(), HarnessError>;
pub fn load_png(path: &Path) -> Result<(u32, u32, Vec<u8>), HarnessError>;
pub struct GoldenPaths { pub reference_dir: PathBuf, pub failure_dir: PathBuf, pub update: bool }
pub fn compare_rgb(name: &str, rgb: &[u8], paths: &GoldenPaths) -> Result<(), HarnessError>;
pub fn compare_golden(name: &str, machine: &Machine) -> Result<(), HarnessError>;
```

`frame_to_rgb` converts the 384 x 240 frame buffer with `palette::rgb` of the machine
standard. `compare_golden` compares the frame with `crates/harness/golden/<name>.png`. A frame
passes only when every pixel is equal. On a difference, it writes the actual frame and a
difference image to `target/golden-failures/<name>-actual.png` and `<name>-diff.png`, and
returns `HarnessError::GoldenMismatch`. In the difference image, a differing pixel is red and
an equal pixel is the actual pixel at a quarter of its brightness. A missing reference file
gives `HarnessError::Missing` with the update command. With `UPDATE_GOLDEN=1`, it writes the
reference file instead and passes (REQ-026).

`compare_golden` is `compare_rgb` with `GoldenPaths::from_env()` (the repository directories
and the `UPDATE_GOLDEN` variable). The tests of the comparison itself call `compare_rgb` with
temporary directories, so that they never change the reference files.

The reference files come from this emulator (`UPDATE_GOLDEN=1 cargo test -p atari-harness
--test golden`). The user reviews each new or changed reference image once, and
`crates/harness/golden/README.md` records the review. An evidence record in
`docs/verification/` is not used, because REQ-026 has the verification method `test`.

## Test suites

| File | Tests | Requirements |
| --- | --- | --- |
| `tests/cpu_functional.rs` | Klaus Dormann functional test | REQ-110 |
| `tests/cpu_single_step.rs` | SingleStepTests 6502, in parallel threads: `single_step_documented_opcodes`, `single_step_bus_cycles`, `single_step_decimal`, `single_step_brk`, `single_step_kil`, `single_step_undocumented_stable`, `single_step_undocumented_unstable` (see [cpu.md](cpu.md)) | REQ-023, REQ-100 to REQ-113 |
| `tests/boot.rs` | Boot to the `Ready` prompt (no more than `BOOT_FRAMES` = 240 frames), OPTION disables BASIC, ROM sizes, frame length, determinism, save states | REQ-002, REQ-004 to REQ-006, REQ-014, REQ-018, REQ-024 |
| `tests/robustness.rs` | Random media files and random programs | REQ-016 |
| `tests/keyboard.rs` | Typing in BASIC | REQ-025 |
| `tests/golden.rs` | Frame comparisons | REQ-026 |
| `tests/pokey_audio.rs` | POKEY tones and filters through the machine and BASIC | REQ-503, REQ-511, REQ-539 |
| `tests/pokey_serial.rs` | OS SIO receive through the POKEY serial port | REQ-526 |
| `tests/pokey_paddle.rs` | Paddle values read by BASIC | REQ-534 |
| `tests/antic_cpu.rs` | ANTIC and the real CPU together: the NMI start cycles of REQ-334 with the CPU interrupt polling of REQ-116 to REQ-118 | REQ-334 (OI-017) |
| `tests/harness_self_test.rs` | Tests of the helpers themselves | REQ-003, REQ-020, REQ-021, REQ-024, REQ-046 (SHA-256, ignored `reference/pinned/`) |

A test that waits for a chip that is not implemented yet has the attribute
`#[ignore = "needs <module>: REQ-NNN"]`. The teammate that implements the module removes the
attribute. A test is never skipped silently (REQ-021).

### Traceability (REQ-027)

Each test has a documentation comment that names its requirements, for example
`/// Verifies REQ-110.`. The test `tests/traceability.rs` `finished_packages_have_running_tests`
reads the requirement files, the slice plans and every `.rs` file of the workspace. It applies
the rule for each work package in `docs/plan/SLICE-NN.md`:

| Package or slice status | Rule |
| --- | --- |
| `Planned`, `In progress` | No rule. The matrix shows the state. |
| `Done` | Each `Test` requirement of the package has a test that is not ignored. |

It always fails for a comment that names a requirement that does not exist or that is
withdrawn. It writes the matrix to `target/traceability.md`. A work package sets its status to
`Done` in the slice plan in its last commit, so the check runs on that commit.

## Test plan

| Requirement | Test |
| --- | --- |
| REQ-020 | `tests/harness_self_test.rs` `fetch_urls_are_pinned` (criterion 1), `fetch_rejects_bad_checksum` (criterion 2: runs the script on a temporary copy with a wrong checksum), `single_step_files_match_blob_list` (criterion 3), `fetch_references_only_with_option` (criteria 4 and 5: a temporary copy with local `file://` URLs, no network), `make_and_ci_fetch_no_reference` (criterion 4); CI runs the script |
| REQ-021 | `tests/harness_self_test.rs` `missing_file_names_fetch_command`; inspection of the test files for early returns; `harness_self_test.rs` `load_cases_errors`, `unreadable_file_gives_read_error` |
| REQ-023 | `cpu_single_step.rs` `single_step_bus_cycles`. The other single-step tests in that file also use the runner, but they verify CPU requirements (`docs/design/cpu.md`).; `harness_self_test.rs` `failure_summary_lists_failed_opcodes`, `flat_bus_default_is_empty`, `flat_bus_logs_accesses`, `load_cases_errors`, `load_cases_reads_nop_file`, `opcode_lists_are_consistent`, `run_case_fails_impossible_case`, `run_case_passes_and_clears_memory`, `run_case_reports_memory_difference`, `run_case_reports_register_difference`, `run_opcodes_reports_nop_file` |
| REQ-024 | `tests/harness_self_test.rs` `screen_text_has_24_rows_of_40`, `internal_to_ascii_values`; `harness_self_test.rs` `boot_program_uses_options`, `harness_error_messages`, `hold_console_keys_releases_keys`, `run_frames_counts_frames`, `run_until_counts_frames`, `test_os_rom_layout`; `boot.rs` `boots_to_basic_ready` |
| REQ-025 | `tests/keyboard.rs` `basic_prints_sum`, `type_text_rejects_unknown_character`; `harness_self_test.rs` `harness_error_messages`, `type_text_checks_all_characters_first`, `type_text_presses_return` |
| REQ-026 | `tests/golden.rs` `boot_screen_matches_golden` (the BASIC `Ready` screen in NTSC and in PAL), `golden_paths_and_reference_files`, `difference_writes_failure_images`, `wrong_size_or_missing_reference_fails`, `update_mode_writes_reference`, `frame_to_rgb_uses_palette`, `golden_antic_modes_ntsc`, `golden_antic_modes_pal`, `golden_fine_scroll`, `golden_players_missiles`, `golden_gtia_modes` (SLICE-02 test screens), `golden_pong_scene` (two paddles and a ball) |

## Open issues

1. The single-step suite takes about 1.1 GB on each CI runner. The CI cache keeps it.
2. Altirra has an acid test suite for ANTIC and GTIA. Its license and build steps are not
   checked yet.
