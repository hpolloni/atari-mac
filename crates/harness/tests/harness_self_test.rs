//! Tests of the harness helpers themselves.

use std::process::Command;

use atari_core::cpu::Bus as _;
use atari_core::{Machine, MachineConfig, VideoStandard};
use atari_harness::screen::{COLUMNS, ROWS, SAVMSC, has_row, internal_to_ascii, screen_text};
use atari_harness::single_step::{Access, DOCUMENTED_OPCODES, FlatBus, KIL_OPCODES};
use atari_harness::{HarnessError, data, system};

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
        os_rom: system::test_os_rom(&[]),
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
