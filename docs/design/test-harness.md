# Test harness design

## Scope

This document gives the design of the test harness: the `atari-harness` crate, the test
data, and the test suites. It covers REQ-020 to REQ-027. REQ-022 is withdrawn; the Klaus Dormann test verifies REQ-110.

## Test data

`scripts/fetch-roms.sh` downloads all test data. Each source is pinned to a commit, and each
file has a checksum (REQ-020). Git ignores the output directories.

| Path | Contents | Source | License | Checksum |
| --- | --- | --- | --- | --- |
| `roms/altirraos-xl.rom` | AltirraOS 3.11 | atari800 `src/roms/altirraos_xl.c` | Permissive (Avery Lee) | SHA-256 |
| `roms/altirra-basic.rom` | Altirra BASIC 1.58 | atari800 `src/roms/altirra_basic.c` | Permissive (Avery Lee) | SHA-256 |
| `test-data/klaus/6502_functional_test.bin` | 6502 functional test | Klaus2m5/6502_65C02_functional_tests | GPL-3.0 | SHA-256 |
| `test-data/single-step/6502/*.json` | 256 files, 10,000 cases each (1.1 GB) | SingleStepTests/65x02 `6502/v1` | MIT | Git blob SHA-1 |
| `reference/altirra-hardware-reference.pdf` | Hardware specification | virtualdub.org | Copyright Avery Lee | SHA-256 |

`make roms` downloads the small files. `make test-data` also downloads the single-step suite.
`make test` and CI download the single-step suite, because REQ-023 is a Must requirement.

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
```

`screen_text` reads the screen address from `SAVMSC` (`$58-$59`) and decodes 24 rows of 40
ANTIC internal character codes. Bit 7 (inverse video) is ignored. Codes `$00-$3F` are
ATASCII `$20-$5F`, codes `$40-$5F` are ATASCII `$00-$1F`, and codes `$60-$7F` are the same
in both sets. A character that is not printable ASCII becomes `?`. This depends only on the
CPU, the memory map and the OS, not on ANTIC or GTIA. So the boot tests can pass before the
video chips are complete.

### Module `input`

```rust
pub fn type_text(machine: &mut Machine, text: &str) -> Result<(), HarnessError>;
```

For each character, `type_text` presses the key for 3 frames and releases it for 3 frames.
It uses `keyboard::ascii_to_kbcode`. A character without a key gives
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
pub fn compare_golden(name: &str, machine: &Machine) -> Result<(), HarnessError>;
```

`compare_golden` compares the frame with `crates/harness/golden/<name>.png`. On a
difference, it writes the actual frame and a difference image to `target/golden-failures/`.
With `UPDATE_GOLDEN=1`, it writes the reference file instead (REQ-026).

## Test suites

| File | Tests | Requirements |
| --- | --- | --- |
| `tests/cpu_functional.rs` | Klaus Dormann functional test | REQ-110 |
| `tests/cpu_single_step.rs` | SingleStepTests 6502, in parallel threads: `single_step_documented_opcodes`, `single_step_bus_cycles`, `single_step_decimal`, `single_step_brk`, `single_step_kil`, `single_step_undocumented_stable`, `single_step_undocumented_unstable` (see [cpu.md](cpu.md)) | REQ-023, REQ-100 to REQ-113 |
| `tests/boot.rs` | Boot to `READY`, OPTION disables BASIC, ROM sizes, frame length, determinism, save states | REQ-002, REQ-004 to REQ-006, REQ-014, REQ-018, REQ-024 |
| `tests/robustness.rs` | Random media files and random programs | REQ-016 |
| `tests/keyboard.rs` | Typing in BASIC | REQ-025 |
| `tests/golden.rs` | Frame comparisons | REQ-026 |
| `tests/pokey_audio.rs` | POKEY tones and filters through the machine and BASIC | REQ-503, REQ-511, REQ-539 |
| `tests/pokey_serial.rs` | OS SIO receive through the POKEY serial port | REQ-526 |
| `tests/pokey_paddle.rs` | Paddle values read by BASIC | REQ-534 |
| `tests/harness_self_test.rs` | Tests of the helpers themselves | REQ-003, REQ-020, REQ-021, REQ-024 |

A test that waits for a chip that is not implemented yet has the attribute
`#[ignore = "needs <module>: REQ-NNN"]`. The teammate that implements the module removes the
attribute. A test is never skipped silently (REQ-021).

### Traceability (REQ-027)

Each test has a documentation comment that names its requirements, for example
`/// Verifies REQ-110.`. The script `scripts/trace.sh` lists the requirements with the
verification method `Test` that no test names. CI runs the script.

## Test plan

| Requirement | Test |
| --- | --- |
| REQ-020 | `tests/harness_self_test.rs` `fetch_urls_are_pinned`, `fetch_rejects_bad_checksum` (runs the script on a temporary copy with a wrong checksum); CI runs the script |
| REQ-021 | `tests/harness_self_test.rs` `missing_file_names_fetch_command`; inspection of the test files for early returns |
| REQ-023 | `tests/cpu_single_step.rs`: the seven `single_step_*` tests |
| REQ-024 | `tests/harness_self_test.rs` `screen_text_has_24_rows_of_40`, `internal_to_ascii_values` |
| REQ-025 | `tests/keyboard.rs` `basic_prints_sum`, `type_text_rejects_unknown_character` |
| REQ-026 | `tests/golden.rs` `boot_screen_matches_golden`, `difference_writes_failure_images`, `update_mode_writes_reference` |
| REQ-027 | `scripts/trace.sh` in CI |

## Open issues

1. The single-step suite takes about 1.1 GB on each CI runner. The CI cache keeps it.
2. Altirra has an acid test suite for ANTIC and GTIA. Its license and build steps are not
   checked yet.
