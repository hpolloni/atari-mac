//! Tests of the harness helpers themselves.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::{fs, io};

use atari_core::cpu::Bus as _;
use atari_core::gtia::ConsoleKeys;
use atari_core::{ConfigError, Machine, MachineConfig, VideoStandard};
use atari_harness::input::{KEY_DOWN_FRAMES, KEY_UP_FRAMES, type_text};
use atari_harness::screen::{COLUMNS, ROWS, SAVMSC, has_row, internal_to_ascii, screen_text};
use atari_harness::single_step::{
    Access, Case, DOCUMENTED_OPCODES, FileReport, FlatBus, KIL_OPCODES, Mismatch, State,
    failure_summary, file_path, load_cases, run_case, run_opcodes,
};
use atari_harness::system::{BootOptions, boot_program, test_os_rom};
use atari_harness::{HarnessError, data, sha256, system};

/// Verifies REQ-021: a missing file names the file and the fetch command.
#[test]
fn missing_file_names_fetch_command() {
    let error = data::read_test_file("test-data/does-not-exist.bin", "make test-data").unwrap_err();
    assert!(
        matches!(error, HarnessError::Missing { .. }),
        "error is {error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("does-not-exist.bin"), "message: {message}");
    assert!(message.contains("make test-data"), "message: {message}");
}

/// Verifies REQ-003: the free ROMs are downloaded with the expected sizes.
#[test]
fn free_roms_have_expected_sizes() {
    assert_eq!(data::os_rom().unwrap().len(), 0x4000, "AltirraOS size");
    assert_eq!(
        data::basic_rom().unwrap().len(),
        8_192,
        "Altirra BASIC size"
    );
}

/// Verifies REQ-003: git tracks no ROM file.
#[test]
fn no_rom_is_tracked_by_git() {
    let output = Command::new("git")
        .args(["ls-files", "*.rom"])
        .current_dir(data::repo_root())
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git ls-files failed");
    assert!(
        output.stdout.is_empty(),
        "tracked ROM files: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// Verifies REQ-024: internal character codes decode to ASCII.
#[test]
fn internal_to_ascii_values() {
    assert_eq!(internal_to_ascii(0x00), ' ', "space");
    assert_eq!(internal_to_ascii(0x10), '0', "digit 0");
    assert_eq!(internal_to_ascii(0x21), 'A', "letter A");
    assert_eq!(internal_to_ascii(0xA1), 'A', "inverse letter A");
    assert_eq!(internal_to_ascii(0x40), '?', "graphics character");
}

/// Verifies REQ-024: `screen_text` reads 24 rows of 40 characters at `SAVMSC`.
#[test]
fn screen_text_has_24_rows_of_40() {
    let mut machine = Machine::new(MachineConfig {
        os_rom: test_os_rom(&[]),
        basic_rom: None,
        cartridge: None,
        standard: VideoStandard::Ntsc,
        audio_sample_rate: system::TEST_SAMPLE_RATE,
    })
    .unwrap();
    machine.poke_ram(SAVMSC, 0x00);
    machine.poke_ram(SAVMSC + 1, 0x40);
    for (offset, code) in [0x32_u8, 0x25, 0x21, 0x24, 0x39].into_iter().enumerate() {
        machine.poke_ram(0x4000 + u16::try_from(40 + offset).unwrap(), code);
    }
    let text = screen_text(&machine);
    assert_eq!(text.len(), ROWS, "row count");
    assert!(
        text.iter().all(|row| row.chars().count() == COLUMNS),
        "column count"
    );
    assert_eq!(text[1].trim_end(), "READY", "row 1");
    assert!(has_row(&machine, "READY"), "has_row");
}

/// Verifies REQ-023: the opcode lists of the single-step suite are complete.
#[test]
fn opcode_lists_are_consistent() {
    assert_eq!(DOCUMENTED_OPCODES.len(), 151, "documented opcodes");
    assert!(
        KIL_OPCODES
            .iter()
            .all(|op| !DOCUMENTED_OPCODES.contains(op)),
        "KIL is undocumented"
    );
}

/// Verifies REQ-023: `FlatBus` logs each access in order.
#[test]
fn flat_bus_logs_accesses() {
    let mut bus = FlatBus::new();
    bus.write(0x1234, 0x56);
    assert_eq!(bus.read(0x1234), 0x56, "read back");
    assert_eq!(
        bus.log,
        vec![(0x1234, 0x56, Access::Write), (0x1234, 0x56, Access::Read)],
        "log"
    );
}

/// Verifies REQ-023: `FlatBus` starts with all RAM `$00`, and `set_byte` does not log.
#[test]
fn flat_bus_default_is_empty() {
    let mut bus = FlatBus::default();
    assert_eq!(bus.ram.len(), 0x1_0000, "64 KB of RAM");
    assert!(bus.ram.iter().all(|&byte| byte == 0), "RAM is $00");
    bus.set_byte(0xFFFF, 0x77);
    assert_eq!(bus.byte(0xFFFF), 0x77, "last byte");
    assert!(bus.log.is_empty(), "set_byte is not a bus access");
}

/// A CPU state with the given program counter and memory, and the other registers fixed.
const fn state(pc: u16, ram: Vec<(u16, u8)>) -> State {
    State {
        pc,
        s: 0xFD,
        a: 0x12,
        x: 0x34,
        y: 0x56,
        p: 0x24,
        ram,
    }
}

/// A case with no bus cycles: the CPU does not run, so the result depends only on the
/// harness and not on the CPU.
fn zero_cycle_case(initial: State, expected: State) -> Case {
    Case {
        name: "synthetic".to_owned(),
        initial,
        r#final: expected,
        cycles: Vec::new(),
    }
}

/// Verifies REQ-023: `run_case` passes a case whose end state equals its start state, and
/// clears the memory that the case used.
#[test]
fn run_case_passes_and_clears_memory() {
    let ram = vec![(0x0200, 0xEA), (0x1234, 0x99)];
    let case = zero_cycle_case(state(0x0200, ram.clone()), state(0x0200, ram));
    let mut bus = FlatBus::new();
    assert_eq!(run_case(&case, &mut bus), Ok(()), "case passes");
    assert_eq!(bus.byte(0x0200), 0x00, "$0200 cleared");
    assert_eq!(bus.byte(0x1234), 0x00, "$1234 cleared");
}

/// Verifies REQ-023: a register difference fails the case, with the case name and the
/// registers in the message.
#[test]
fn run_case_reports_register_difference() {
    let mut expected = state(0x0200, Vec::new());
    expected.a = 0x13;
    let case = zero_cycle_case(state(0x0200, Vec::new()), expected);
    let error = run_case(&case, &mut FlatBus::new()).unwrap_err();
    assert!(
        error.0.starts_with("synthetic: registers"),
        "message: {error}"
    );
    assert!(
        error.0.contains("13"),
        "message names the expected A: {error}"
    );
}

/// Verifies REQ-023: a memory difference fails the case, with the address and both values.
#[test]
fn run_case_reports_memory_difference() {
    let case = zero_cycle_case(
        state(0x0200, Vec::new()),
        state(0x0200, vec![(0x0300, 0x55)]),
    );
    let error = run_case(&case, &mut FlatBus::new()).unwrap_err();
    assert_eq!(
        error,
        Mismatch("synthetic: memory $0300 is $00, expected $55".to_owned()),
        "mismatch"
    );
    assert_eq!(error.to_string(), error.0, "Display");
}

/// Verifies REQ-023: a case with bus cycles that no 6502 makes fails, and the bus is clean
/// after the case. The first cycle of each instruction reads the opcode at `PC`.
#[test]
fn run_case_fails_impossible_case() {
    let case = Case {
        name: "impossible".to_owned(),
        initial: state(0x0200, vec![(0x0200, 0xEA)]),
        r#final: state(0x0201, vec![(0x0200, 0xEA)]),
        cycles: vec![(0x0200, 0x00, Access::Write), (0x0201, 0x00, Access::Write)],
    };
    let mut bus = FlatBus::new();
    let error = run_case(&case, &mut bus).unwrap_err();
    assert!(error.0.starts_with("impossible: "), "message: {error}");
    assert!(
        bus.ram.iter().all(|&byte| byte == 0),
        "bus is clean after the case"
    );
}

/// Verifies REQ-023: the `NOP` file has 10,000 cases, and the report and the summary agree
/// with the number of failures.
#[test]
fn run_opcodes_reports_nop_file() {
    let reports = run_opcodes(&[0xEA]).unwrap();
    assert_eq!(reports.len(), 1, "one report");
    let report = &reports[0];
    assert_eq!(report.opcode, 0xEA, "opcode");
    assert_eq!(report.cases, 10_000, "cases in the file");
    assert!(
        report.failures <= report.cases,
        "failures: {}",
        report.failures
    );
    assert_eq!(
        report.first.is_some(),
        report.failures > 0,
        "first failure is given exactly when a case fails"
    );
    let summary = failure_summary(&reports);
    if report.failures == 0 {
        assert!(summary.is_empty(), "summary: {summary}");
    } else {
        let first = report.first.as_ref().map_or("", |m| m.0.as_str());
        let start = format!(
            "opcode $EA: {} of 10000 cases fail; first: ",
            report.failures
        );
        assert!(summary.starts_with(&start), "summary: {summary}");
        assert!(summary.contains(first), "summary names the first failure");
    }
}

/// Verifies REQ-023: `failure_summary` lists only the opcodes with failures.
#[test]
fn failure_summary_lists_failed_opcodes() {
    let reports = [
        FileReport {
            opcode: 0xA9,
            cases: 10,
            failures: 0,
            first: None,
        },
        FileReport {
            opcode: 0x69,
            cases: 10,
            failures: 2,
            first: Some(Mismatch("69 01 02: registers".to_owned())),
        },
    ];
    assert_eq!(
        failure_summary(&reports),
        "opcode $69: 2 of 10 cases fail; first: 69 01 02: registers\n",
        "summary"
    );
    assert_eq!(failure_summary(&reports[..1]), "", "no failure");
}

/// Verifies REQ-023: `load_cases` reads the case data of a file.
#[test]
fn load_cases_reads_nop_file() {
    let path = file_path(0xEA);
    assert!(
        path.ends_with("test-data/single-step/6502/ea.json"),
        "path {}",
        path.display()
    );
    let cases = load_cases(&path).unwrap();
    assert_eq!(cases.len(), 10_000, "cases");
    let case = &cases[0];
    assert_eq!(case.cycles.len(), 2, "NOP takes 2 cycles");
    assert_eq!(
        case.r#final.pc,
        case.initial.pc.wrapping_add(1),
        "NOP is 1 byte"
    );
    assert_eq!(
        case.cycles[0],
        (case.initial.pc, 0xEA, Access::Read),
        "opcode fetch"
    );
}

/// Verifies REQ-021 and REQ-023: a missing suite file names `make test-data`, and a file
/// that does not parse gives `HarnessError::Read`.
#[test]
fn load_cases_errors() {
    let missing = load_cases(Path::new("/does/not/exist.json")).unwrap_err();
    assert!(
        missing.to_string().contains("make test-data"),
        "message: {missing}"
    );
    let bad = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bad-single-step.json");
    fs::write(&bad, b"[{").unwrap();
    let error = load_cases(&bad).unwrap_err();
    assert!(
        matches!(error, HarnessError::Read { .. }),
        "error is {error:?}"
    );
    let start = format!("cannot read {}: ", bad.display());
    assert!(error.to_string().starts_with(&start), "message: {error}");
}

/// Verifies REQ-021: a path that exists but cannot be read gives `HarnessError::Read`.
#[test]
fn unreadable_file_gives_read_error() {
    let error = data::read_test_file("crates", "make roms").unwrap_err();
    assert!(
        matches!(error, HarnessError::Read { .. }),
        "error is {error:?}"
    );
}

/// Verifies REQ-024 and REQ-025: the error messages of the harness.
#[test]
fn harness_error_messages() {
    assert_eq!(
        HarnessError::from(ConfigError::OsRomSize(5)).to_string(),
        "machine configuration: OS ROM must be 16384 bytes, not 5 bytes",
        "Config"
    );
    assert_eq!(
        HarnessError::Config(ConfigError::BasicRomSize(7)).to_string(),
        "machine configuration: BASIC ROM must be 8192 bytes, not 7 bytes",
        "Config BASIC"
    );
    assert_eq!(
        HarnessError::NoKey('€').to_string(),
        "no Atari key types '€'",
        "NoKey"
    );
    assert_eq!(
        HarnessError::Read {
            path: PathBuf::from("a/b"),
            message: "bad".to_owned(),
        }
        .to_string(),
        "cannot read a/b: bad",
        "Read"
    );
}

/// Verifies REQ-024: `test_os_rom` puts the program at `$C000`, the vectors at the end, and
/// `NOP` everywhere else.
#[test]
fn test_os_rom_layout() {
    let rom = test_os_rom(&[0xA9, 0x01]);
    assert_eq!(rom.len(), 0x4000, "size: 16 KB");
    assert_eq!(&rom[..3], &[0xA9, 0x01, 0xEA], "program, then NOP");
    assert!(rom[2..0x3FF0].iter().all(|&byte| byte == 0xEA), "NOP fill");
    assert_eq!(rom[0x3FF0], 0x40, "RTI at $FFF0");
    assert_eq!(
        &rom[0x3FFA..],
        &[0xF0, 0xFF, 0x00, 0xC0, 0xF0, 0xFF],
        "NMI, RESET and IRQ vectors"
    );
    let long = test_os_rom(&[0x11; 0x4000]);
    assert!(
        long[..0x3FF0].iter().all(|&byte| byte == 0x11),
        "program fills $C000-$FFEF"
    );
    assert_eq!(
        &long[0x3FF0..],
        &rom[0x3FF0..],
        "a long program does not overwrite the vectors"
    );
}

/// Verifies REQ-024: `boot_program` makes a machine with the test ROM, and with the standard
/// and BASIC of the options.
#[test]
fn boot_program_uses_options() {
    let pal = boot_program(
        &[0x4C, 0x00, 0xC0],
        BootOptions {
            basic: false,
            standard: VideoStandard::Pal,
        },
    )
    .unwrap();
    assert_eq!(pal.standard(), VideoStandard::Pal, "standard");
    assert_eq!(pal.peek(0xC000), 0x4C, "program at $C000");
    assert_eq!(pal.peek(0xFFFC), 0x00, "RESET vector low byte");
    assert_eq!(pal.peek(0xFFFD), 0xC0, "RESET vector high byte");
    assert_eq!(pal.cycle_count(), 0, "no cycle has run");
    let options = BootOptions {
        basic: false,
        standard: VideoStandard::Ntsc,
    };
    let without = boot_program(&[], options).unwrap();
    let with = boot_program(
        &[],
        BootOptions {
            basic: true,
            ..options
        },
    )
    .unwrap();
    assert_eq!(without.standard(), VideoStandard::Ntsc, "NTSC");
    assert_ne!(with, without, "BASIC is in the configuration");
}

/// Verifies REQ-024: `run_frames` runs whole frames.
#[test]
fn run_frames_counts_frames() {
    let mut machine = boot_program(&[], BootOptions::default()).unwrap();
    system::run_frames(&mut machine, 1);
    assert_eq!(machine.frame_count(), 1, "one frame");
    let start = machine.cycle_count();
    system::run_frames(&mut machine, 2);
    assert_eq!(machine.frame_count(), 3, "three frames");
    assert_eq!(machine.cycle_count() - start, 2 * 29_868, "two NTSC frames");
    system::run_frames(&mut machine, 0);
    assert_eq!(machine.frame_count(), 3, "zero frames");
}

/// Verifies REQ-024: `run_until` gives the frames that ran before the condition was true,
/// or `None` after `max_frames`.
#[test]
fn run_until_counts_frames() {
    let mut machine = boot_program(&[], BootOptions::default()).unwrap();
    assert_eq!(
        system::run_until(&mut machine, 5, |_| true),
        Some(0),
        "true at once"
    );
    assert_eq!(machine.frame_count(), 0, "no frame ran");
    assert_eq!(
        system::run_until(&mut machine, 5, |m| m.frame_count() >= 2),
        Some(2),
        "true after 2 frames"
    );
    assert_eq!(
        system::run_until(&mut machine, 3, |_| false),
        None,
        "never true"
    );
    assert_eq!(machine.frame_count(), 5, "max_frames ran");
}

/// Verifies REQ-024 and REQ-437: `hold_console_keys` runs the frames and then releases the
/// keys (`CONSOL` reads `$0F` with no key and no write).
#[test]
fn hold_console_keys_releases_keys() {
    let mut machine = boot_program(&[], BootOptions::default()).unwrap();
    let keys = ConsoleKeys {
        start: true,
        ..ConsoleKeys::default()
    };
    machine.set_console_keys(keys);
    assert_eq!(machine.peek(0xD01F), 0x0E, "START pressed");
    system::hold_console_keys(&mut machine, keys, 2);
    assert_eq!(machine.frame_count(), 2, "frames held");
    assert_eq!(machine.peek(0xD01F), 0x0F, "keys released");
}

/// Verifies REQ-025: `type_text` holds each key down, then up, and releases it at the end.
#[test]
fn type_text_presses_return() {
    let mut machine = boot_program(&[], BootOptions::default()).unwrap();
    type_text(&mut machine, "\n\n").unwrap();
    assert_eq!(
        machine.frame_count(),
        u64::from(2 * (KEY_DOWN_FRAMES + KEY_UP_FRAMES)),
        "frames for two keys"
    );
    assert_eq!(
        machine.peek(0xD20F) & 0x04,
        0x04,
        "SKSTAT: no key pressed at the end"
    );
    type_text(&mut machine, "").unwrap();
    assert_eq!(machine.frame_count(), 12, "no frame for empty text");
}

/// Verifies REQ-025: `type_text` presses no key when a character has no key.
#[test]
fn type_text_checks_all_characters_first() {
    let mut machine = boot_program(&[], BootOptions::default()).unwrap();
    assert_eq!(
        type_text(&mut machine, "\n€"),
        Err(HarnessError::NoKey('€')),
        "euro sign"
    );
    assert_eq!(machine.frame_count(), 0, "no frame ran");
}

/// The text of `scripts/fetch-roms.sh`.
fn fetch_script() -> io::Result<String> {
    fs::read_to_string(data::repo_root().join("scripts/fetch-roms.sh"))
}

/// `true` if `text` is `len` lower-case hexadecimal digits.
fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Verifies REQ-020: each URL in `scripts/fetch-roms.sh` contains a commit hash (criterion 1).
/// Each `fetch` and `fetch_c_array` call has a SHA-256 checksum, except the call for the
/// pinned references, which takes the `sha256` of the registry entry.
#[test]
fn fetch_urls_are_pinned() {
    let script = fetch_script().unwrap().replace("\\\n", " ");
    let commits: Vec<(String, String)> = script
        .lines()
        .filter_map(|line| line.strip_prefix("readonly "))
        .filter_map(|rest| rest.split_once('='))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    assert!(commits.len() >= 3, "commit variables: {commits:?}");
    for (name, value) in &commits {
        assert!(is_hex(value, 40), "{name} = {value} is not a commit hash");
    }
    let expand = |url: &str| {
        commits.iter().fold(url.to_owned(), |url, (name, value)| {
            url.replace(&format!("${name}"), value)
        })
    };
    let mut urls = 0;
    for (index, _) in script.match_indices("https://") {
        let rest = &script[index..];
        let end = rest
            .find(|c: char| c == '"' || c.is_whitespace())
            .unwrap_or(rest.len());
        let url = expand(&rest[..end]);
        urls += 1;
        assert!(
            url.split('/').any(|part| is_hex(part, 40)),
            "URL without a commit hash: {url}"
        );
    }
    assert_eq!(urls, 3, "atari800, Klaus Dormann and SingleStepTests URLs");
    let calls: Vec<&str> = script
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("fetch ") || line.starts_with("fetch_c_array "))
        .collect();
    assert_eq!(calls.len(), 4, "fetch calls: {calls:?}");
    for call in calls {
        let checksum = call.split_whitespace().nth(3).unwrap_or_default();
        assert!(
            is_hex(checksum, 64) || call == r#"fetch "$url" "$file" "$sha""#,
            "no SHA-256 checksum: {call}"
        );
    }
}

/// A copy of `scripts/fetch-roms.sh` in a new temporary directory `name`.
fn script_copy(name: &str) -> io::Result<PathBuf> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    fs::create_dir_all(root.join("scripts"))?;
    fs::write(root.join("scripts/fetch-roms.sh"), fetch_script()?)?;
    Ok(root)
}

/// Runs the script copy in `root` with `args`.
fn run_script(root: &Path, args: &[&str]) -> io::Result<Output> {
    Command::new("bash")
        .arg(root.join("scripts/fetch-roms.sh"))
        .args(args)
        .output()
}

/// Verifies REQ-020: when a downloaded file has a different checksum, the script deletes the
/// file, prints `checksum mismatch` and exits with a status that is not 0 (criterion 2).
///
/// The test runs a copy of the script in a temporary directory. The copy has the valid ROMs,
/// so that it downloads nothing for them, and its Klaus Dormann URL is a local `file://` URL
/// to a file with a wrong checksum.
#[test]
fn fetch_rejects_bad_checksum() {
    let root = script_copy("fetch-rejects-bad-checksum").unwrap();
    fs::create_dir_all(root.join("roms")).unwrap();
    for rom in ["altirraos-xl.rom", "altirra-basic.rom"] {
        let bytes = data::read_test_file(&format!("roms/{rom}"), data::FETCH_ROMS).unwrap();
        fs::write(root.join("roms").join(rom), bytes).unwrap();
    }
    let source = root.join("wrong.bin");
    fs::write(&source, b"not the Klaus Dormann test").unwrap();
    let script = fetch_script().unwrap();
    let start = script
        .find("https://raw.githubusercontent.com/Klaus2m5")
        .unwrap();
    let end = start + script[start..].find('"').unwrap();
    let copy = format!(
        "{}file://{}{}",
        &script[..start],
        source.display(),
        &script[end..]
    );
    fs::write(root.join("scripts/fetch-roms.sh"), copy).unwrap();
    let output = run_script(&root, &[]).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the script failed: {stderr}");
    assert!(stderr.contains("checksum mismatch"), "stderr: {stderr}");
    let dest = root.join("test-data/klaus/6502_functional_test.bin");
    assert!(!dest.exists(), "the file is not kept");
    assert!(
        !dest.with_extension("bin.part").exists(),
        "the partial download is deleted"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("ok      roms/altirraos-xl.rom"),
        "the valid ROMs are kept"
    );
}

/// Verifies REQ-020: `scripts/fetch-roms.sh --single-step` gives 256 files in
/// `test-data/single-step/6502/`, each with the git blob SHA-1 in
/// `scripts/single-step-6502.sha1` (criterion 3).
#[test]
fn single_step_files_match_blob_list() {
    let root = data::repo_root();
    let list = fs::read_to_string(root.join("scripts/single-step-6502.sha1")).unwrap();
    let entries: Vec<(&str, &str)> = list
        .lines()
        .filter_map(|line| line.split_once(' '))
        .collect();
    assert_eq!(entries.len(), 256, "one blob SHA-1 per opcode");
    let paths: Vec<String> = entries
        .iter()
        .map(|(_, file)| format!("test-data/single-step/6502/{file}"))
        .collect();
    for path in &paths {
        assert!(
            root.join(path).is_file(),
            "{path} is missing: run `{}`",
            data::FETCH_TEST_DATA
        );
    }
    let output = Command::new("git")
        .args(["hash-object", "--stdin-paths"])
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write as _;
                stdin.write_all(paths.join("\n").as_bytes())?;
            }
            child.wait_with_output()
        })
        .expect("git runs");
    assert!(output.status.success(), "git hash-object failed");
    let blobs = String::from_utf8_lossy(&output.stdout);
    for ((blob, file), actual) in entries.iter().zip(blobs.lines()) {
        assert_eq!(actual, *blob, "blob SHA-1 of {file}");
    }
    assert_eq!(blobs.lines().count(), 256, "git hashed each file");
}

/// Writes a registry entry `docs/spec/references/<id>.md` below `root` for the tests of the
/// `--references` option. Its `file` is `reference/pinned/<id>.bin` for the storage `pin`,
/// and `reference/<id>.bin` for the storage `commit`.
fn write_entry(root: &Path, id: &str, storage: &str, url: &str, sha256: &str) -> io::Result<()> {
    let file = if storage == "pin" {
        format!("reference/pinned/{id}.bin")
    } else {
        format!("reference/{id}.bin")
    };
    let dir = root.join("docs/spec/references");
    fs::create_dir_all(&dir)?;
    let text = format!(
        "---\nid: {id}\ntitle: Title\nedition: One\nauthor: An author\nlicense: none found\n\
         storage: {storage}\nurl: \"{url}\"\nsha256: {sha256}\nfile: {file}\n---\n\
         # {id}: Title\n\n## License\n\nNone.\n"
    );
    fs::write(dir.join(format!("{id}.md")), text)
}

/// Verifies REQ-020: the script without `--references` downloads no reference (criterion 4).
/// With `--references`, it downloads each registry entry with the storage `pin` from its
/// `url` to its `file` and verifies its `sha256`; a different checksum deletes the file,
/// prints `checksum mismatch` and exits with a status that is not 0 (criterion 5). A `commit`
/// entry is not downloaded.
///
/// The test runs a copy of the script in a temporary directory, with local `file://` URLs,
/// so that it uses no network.
#[test]
fn fetch_references_only_with_option() {
    let root = script_copy("fetch-references-only-with-option").unwrap();
    fs::create_dir_all(root.join("roms")).unwrap();
    for rom in ["roms/altirraos-xl.rom", "roms/altirra-basic.rom"] {
        let bytes = data::read_test_file(rom, data::FETCH_ROMS).unwrap();
        fs::write(root.join(rom), bytes).unwrap();
    }
    let klaus = "test-data/klaus/6502_functional_test.bin";
    let bytes = data::read_test_file(klaus, data::FETCH_ROMS).unwrap();
    fs::create_dir_all(root.join("test-data/klaus")).unwrap();
    fs::write(root.join(klaus), bytes).unwrap();
    let source = root.join("source.bin");
    fs::write(&source, b"abc").unwrap();
    let url = format!("file://{}", source.display());
    let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    write_entry(&root, "PINNED", "pin", &url, abc).unwrap();
    write_entry(&root, "COMMITTED", "commit", &url, abc).unwrap();

    let output = run_script(&root, &[]).unwrap();
    assert!(
        output.status.success(),
        "the script without options: {output:?}"
    );
    assert!(
        !root.join("reference").exists(),
        "no reference without --references"
    );

    let output = run_script(&root, &["--references"]).unwrap();
    assert!(output.status.success(), "--references: {output:?}");
    assert_eq!(
        fs::read(root.join("reference/pinned/PINNED.bin")).unwrap(),
        b"abc",
        "the pinned file is downloaded"
    );
    assert!(
        !root.join("reference/COMMITTED.bin").exists(),
        "a commit entry is not downloaded"
    );
    let output = run_script(&root, &["--references"]).unwrap();
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("ok      reference/pinned/PINNED.bin"),
        "a valid file is kept: {output:?}"
    );

    fs::remove_file(root.join("reference/pinned/PINNED.bin")).unwrap();
    let wrong = "0000000000000000000000000000000000000000000000000000000000000000";
    write_entry(&root, "PINNED", "pin", &url, wrong).unwrap();
    let output = run_script(&root, &["--references"]).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "a wrong checksum fails: {stderr}");
    assert!(stderr.contains("checksum mismatch"), "stderr: {stderr}");
    assert!(
        !root.join("reference/pinned/PINNED.bin").exists()
            && !root.join("reference/pinned/PINNED.bin.part").exists(),
        "the file is deleted"
    );
}

/// Verifies REQ-020: only the `make reference` target runs `--references`, no other target
/// depends on it, and CI does not download a reference (criterion 4). `make roms`, `make test`
/// and `make pre-commit` therefore download no reference.
#[test]
fn make_and_ci_fetch_no_reference() {
    let root = data::repo_root();
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    let mut target = String::new();
    for line in makefile.lines() {
        if !line.starts_with('\t')
            && !line.starts_with('#')
            && let Some((name, prerequisites)) = line.split_once(':')
            && !name.contains('=')
            && !prerequisites.starts_with('=')
        {
            name.trim().clone_into(&mut target);
            assert!(
                target == ".PHONY" || !prerequisites.split_whitespace().any(|p| p == "reference"),
                "target {target} depends on reference"
            );
        }
        if line.starts_with('\t') && line.contains("--references") {
            assert_eq!(target, "reference", "{line}");
        }
    }
    assert!(
        makefile.contains("reference:\n\tscripts/fetch-roms.sh --references\n"),
        "make reference runs the script with --references"
    );
    for (file, text) in [
        ("ci.yml", ".github/workflows/ci.yml"),
        ("pre-commit", ".githooks/pre-commit"),
    ] {
        let text = fs::read_to_string(root.join(text)).unwrap();
        assert!(
            !text.contains("--references") && !text.contains("make reference"),
            "{file} downloads a reference"
        );
    }
}

/// Verifies REQ-046: SHA-256 gives the values of the FIPS 180-4 examples for the empty
/// message, `abc`, the 56-byte message (the padding needs a second block) and one million
/// times `a` (the checksum of criterion 7).
#[test]
fn sha256_matches_fips_vectors() {
    assert_eq!(
        sha256::hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "empty message"
    );
    assert_eq!(
        sha256::hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "abc"
    );
    assert_eq!(
        sha256::hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        "two-block message"
    );
    assert_eq!(
        sha256::hex(&vec![b'a'; 1_000_000]),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
        "one million times a"
    );
}

/// Verifies REQ-046: git ignores `reference/pinned/`, the place of the `pin` references
/// (criterion 8), and `reference/text/`, but not the committed references.
#[test]
fn pinned_references_are_ignored() {
    let ignored = |path: &str| {
        Command::new("git")
            .args(["check-ignore", "--quiet", "--no-index", path])
            .current_dir(data::repo_root())
            .status()
            .expect("git runs")
            .success()
    };
    assert!(ignored("reference/pinned/a.pdf"), "reference/pinned/");
    assert!(ignored("reference/text/a.txt"), "reference/text/");
    assert!(
        !ignored("reference/altirra-hardware-reference.pdf"),
        "the committed HRM"
    );
}

/// The jobs of a GitHub workflow: the key of each job and its lines (the lines after
/// `jobs:` with an indent of 2 spaces start a job).
fn workflow_jobs(workflow: &str) -> Vec<(String, Vec<&str>)> {
    let mut jobs: Vec<(String, Vec<&str>)> = Vec::new();
    let mut in_jobs = false;
    for line in workflow.lines() {
        if !line.starts_with(' ') && !line.is_empty() {
            in_jobs = line == "jobs:";
            continue;
        }
        if !in_jobs {
            continue;
        }
        if let Some(key) = line.strip_prefix("  ").and_then(|l| l.strip_suffix(':'))
            && !key.starts_with([' ', '#'])
        {
            jobs.push((key.to_owned(), Vec::new()));
        } else if let Some((_, lines)) = jobs.last_mut() {
            lines.push(line);
        }
    }
    jobs
}

/// Verifies REQ-042: the CI job `Coverage (cargo-llvm-cov)` in `.github/workflows/ci.yml`
/// runs `cargo llvm-cov` on the workspace with `--fail-under-lines 80` (criterion 1). The
/// job `CI success` needs it, and `make coverage` uses the same threshold.
#[test]
fn coverage_job_needs_80_percent() {
    let root = data::repo_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let jobs = workflow_jobs(&workflow);
    let named = |name: &str| {
        jobs.iter()
            .find(|(_, lines)| lines.iter().any(|l| l.trim() == format!("name: {name}")))
            .unwrap_or_else(|| panic!("no CI job {name}"))
    };
    let (coverage_key, coverage) = named("Coverage (cargo-llvm-cov)");
    let run = coverage
        .iter()
        .filter_map(|l| l.trim().strip_prefix("- run: "))
        .find(|command| command.starts_with("cargo llvm-cov"))
        .expect("the coverage job runs cargo llvm-cov");
    let words: Vec<&str> = run.split_whitespace().collect();
    assert!(words.contains(&"--workspace"), "{run}: --workspace");
    assert!(
        words.windows(2).any(|w| w == ["--fail-under-lines", "80"]),
        "{run}: --fail-under-lines 80"
    );
    let (_, success) = named("CI success");
    let needs = success
        .iter()
        .find_map(|l| l.trim().strip_prefix("needs: "))
        .expect("CI success needs the other jobs");
    assert!(
        needs
            .trim_matches(['[', ']'])
            .split(',')
            .any(|job| job.trim() == coverage_key),
        "CI success needs {coverage_key}: {needs}"
    );
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    assert!(
        makefile.lines().any(|l| l == "COVERAGE_MIN := 80"),
        "make coverage uses 80 percent"
    );
}
