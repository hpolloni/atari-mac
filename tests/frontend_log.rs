//! The records of the category `frontend` (REQ-085 criterion 3), with a capture logger.
//!
//! Design: `docs/design/architecture.md`, "Frontend records". The logger and the maximum
//! level of the `log` crate are global to the process, so these tests are in their own test
//! binary. The capture logger keeps one record list for each thread.

use std::cell::RefCell;
use std::{fs, mem};

use atari_mac::atari_core::keyboard::ascii_to_kbcode;
use atari_mac::atari_core::media::{DiskImage, Tape};
use atari_mac::atari_core::{Machine, MachineConfig, VideoStandard};
use atari_mac::bas::Typist;
use atari_mac::frontend::{Flow, Frontend, TurboCause};
use atari_mac::logging::format_line;
use log::kv::Key as KvKey;
use log::{LevelFilter, Log, Metadata, Record};
use minifb::Key;
// The binary uses these dependencies; this test does not.
use cpal as _;
use png as _;

/// The free OS ROM (fetched by `scripts/fetch-roms.sh`).
const OS_ROM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/roms/altirraos-xl.rom");
/// The free BASIC ROM.
const BASIC_ROM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/roms/altirra-basic.rom");

/// One record that the capture logger saw.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Captured {
    /// The `frame` key-value.
    frame: Option<u64>,
    /// The `cycle` key-value.
    cycle: Option<u64>,
    /// The line of `format_line`.
    line: Option<String>,
}

thread_local! {
    /// The records of the thread.
    static RECORDS: RefCell<Vec<Captured>> = const { RefCell::new(Vec::new()) };
}

/// The capture logger: keeps each record of the category `frontend` of the thread. The
/// records of the core (REQ-084, for example the cold reset) are not in this test.
#[derive(Debug, Clone, Copy)]
struct Capture;

impl Log for Capture {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.target() == "atari::frontend"
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let count = |key| {
            record
                .key_values()
                .get(KvKey::from_str(key))
                .and_then(|value| value.to_u64())
        };
        let captured = Captured {
            frame: count("frame"),
            cycle: count("cycle"),
            line: format_line(record),
        };
        RECORDS.with_borrow_mut(|records| records.push(captured));
    }

    fn flush(&self) {}
}

/// The capture logger, installed once for the process.
static CAPTURE: Capture = Capture;

/// Installs the capture logger at `Info` and clears the records of the thread.
fn capture() {
    // The second call in the process keeps the logger of the first.
    log::set_logger(&CAPTURE).unwrap_or(());
    log::set_max_level(LevelFilter::Info);
    RECORDS.with_borrow_mut(Vec::clear);
}

/// The records of the thread since the last call, and clears them.
fn take() -> Vec<Captured> {
    RECORDS.with_borrow_mut(mem::take)
}

/// The counts of the machine of `frontend`: the counts at the start of its next frame.
fn counts(frontend: &Frontend) -> (u64, u64) {
    let machine = frontend.machine();
    (machine.frame_count(), machine.cycle_count())
}

/// One step of `frontend` with `keys`. Each record of the step has the counts of the
/// machine before the step and the format of REQ-085 criterion 2. Returns the messages.
fn step(frontend: &mut Frontend, keys: &[Key]) -> Vec<String> {
    let (frame, cycle) = counts(frontend);
    assert_eq!(
        frontend.step(keys).expect("step"),
        Flow::Continue,
        "no quit"
    );
    take()
        .into_iter()
        .map(|record| {
            assert_eq!(
                (record.frame, record.cycle),
                (Some(frame), Some(cycle)),
                "the counts at the start of the frame: {record:?}"
            );
            let prefix = format!("[frame {frame} cycle {cycle}] frontend: ");
            let line = record.line.expect("a line of the category frontend");
            line.strip_prefix(&prefix)
                .unwrap_or_else(|| panic!("the line {line:?} starts with {prefix:?}"))
                .to_owned()
        })
        .collect()
}

/// Steps `frontend` with no keys until a step gives a record, at most `limit` steps.
/// Returns the number of steps and the messages of the last step.
fn step_until_record(frontend: &mut Frontend, limit: u32) -> (u32, Vec<String>) {
    for steps in 1..=limit {
        let messages = step(frontend, &[]);
        if !messages.is_empty() {
            return (steps, messages);
        }
    }
    panic!("no record in {limit} steps");
}

/// A machine with the free ROMs and BASIC.
fn basic_machine() -> Machine {
    Machine::new(MachineConfig {
        os_rom: fs::read(OS_ROM).expect("OS ROM"),
        basic_rom: Some(fs::read(BASIC_ROM).expect("BASIC ROM")),
        cartridge: None,
        disk: None,
        fast_sio: false,
        fast_tape: false,
        tape: None,
        standard: VideoStandard::Ntsc,
        audio_sample_rate: 48_000,
    })
    .expect("machine")
}

/// The RAM byte that the motor test program copies to `PACTL`.
const MOTOR: u16 = 0x0080;
/// `PACTL` with the motor line low (motor on).
const MOTOR_ON: u8 = 0x34;
/// `PACTL` with the motor line high (motor off).
const MOTOR_OFF: u8 = 0x3C;

/// A machine whose OS ROM copies the RAM byte [`MOTOR`] to `PACTL` in a loop, with a tape
/// of one data chunk in the deck. The motor is off after the first frame.
fn motor_machine() -> Machine {
    let mut rom = vec![0xEA; 0x4000];
    let program = [
        0xA9, MOTOR_OFF, 0x85, 0x80, // LDA #$3C, STA $80
        0xA5, 0x80, 0x8D, 0x02, 0xD3, // loop: LDA $80, STA PACTL
        0x4C, 0x04, 0xC0, // JMP loop
    ];
    rom[..program.len()].copy_from_slice(&program);
    rom[0x3FF0] = 0x40; // RTI
    rom[0x3FFA..].copy_from_slice(&[0xF0, 0xFF, 0x00, 0xC0, 0xF0, 0xFF]);
    let mut file = b"FUJI".to_vec();
    file.extend([0; 4]);
    file.extend(b"data");
    file.extend(64_u16.to_le_bytes());
    file.extend(100_u16.to_le_bytes());
    file.extend([0x55; 64]);
    let mut machine = Machine::new(MachineConfig {
        os_rom: rom,
        basic_rom: None,
        cartridge: None,
        disk: None,
        fast_sio: false,
        fast_tape: false,
        tape: Some(Tape::from_cas(&file).expect("tape")),
        standard: VideoStandard::Ntsc,
        audio_sample_rate: 48_000,
    })
    .expect("machine");
    machine.run_frame();
    machine
}

/// Verifies REQ-085: with a capture logger at `Info`, the `atari::frontend` records are
/// `turbo on (BASIC typing)` at the first frame of a `--bas` typist and `turbo off` after
/// the typing, `turbo on (tape motor)` and `turbo off` with the motor line of a tape,
/// `paused` and `continued` at F9, and `cold reset (F10)` at F10. Each record has the frame
/// count and the cycle count of the machine at the start of its frame (criteria 2 and 3).
#[test]
fn frontend_records() {
    capture();

    // BASIC typing.
    let codes = "10 REM A\n"
        .chars()
        .map(|c| ascii_to_kbcode(c).expect("key"))
        .collect();
    let mut frontend = Frontend::new(basic_machine()).with_typist(Typist::new(codes));
    assert_eq!(
        step(&mut frontend, &[]),
        ["turbo on (BASIC typing)"],
        "the first frame of the typist"
    );
    assert_eq!(
        frontend.turbo_cause(),
        Some(TurboCause::Typing),
        "the cause"
    );
    let (steps, messages) = step_until_record(&mut frontend, 2_000);
    assert_eq!(messages, ["turbo off"], "after the typing");
    assert!(frontend.typist().is_none(), "the typist is done");
    assert!(steps > 1, "the typing takes more than one frame");
    assert_eq!(frontend.turbo_cause(), None, "real time after the typing");

    // The tape motor.
    let mut frontend = Frontend::new(motor_machine());
    for _ in 0..5 {
        assert!(step(&mut frontend, &[]).is_empty(), "the motor is off");
    }
    frontend.machine_mut().poke_ram(MOTOR, MOTOR_ON);
    // The program sets the motor line in this frame; the next frame starts with it low.
    assert!(
        step(&mut frontend, &[]).is_empty(),
        "the motor line goes low"
    );
    assert_eq!(
        step(&mut frontend, &[]),
        ["turbo on (tape motor)"],
        "the first frame with the motor line low"
    );
    assert_eq!(
        frontend.turbo_cause(),
        Some(TurboCause::TapeMotor),
        "the cause"
    );
    for _ in 0..5 {
        assert!(step(&mut frontend, &[]).is_empty(), "the motor runs");
    }
    frontend.machine_mut().poke_ram(MOTOR, MOTOR_OFF);
    assert!(
        step(&mut frontend, &[]).is_empty(),
        "the motor line goes high"
    );
    assert_eq!(
        step(&mut frontend, &[]),
        ["turbo off"],
        "the first frame with the motor line high"
    );

    // Pause and cold reset.
    assert_eq!(step(&mut frontend, &[Key::F9]), ["paused"], "F9 pauses");
    assert!(step(&mut frontend, &[Key::F9]).is_empty(), "F9 held");
    assert!(step(&mut frontend, &[]).is_empty(), "paused, F9 up");
    assert_eq!(
        step(&mut frontend, &[Key::F9]),
        ["continued"],
        "F9 continues"
    );
    assert!(step(&mut frontend, &[]).is_empty(), "F9 up");
    assert_eq!(
        step(&mut frontend, &[Key::F10]),
        ["cold reset (F10)"],
        "F10"
    );
    assert!(step(&mut frontend, &[Key::F10]).is_empty(), "F10 held");
    assert!(step(&mut frontend, &[]).is_empty(), "F10 up");
}

/// The boot test disk of REQ-636 criterion 1: 720 sectors of 128 bytes, sector 1 has the
/// boot header `00 01 00 07 00 07` and `LDA #$A5`, `STA $0600`, `JMP $070B`.
fn boot_disk() -> DiskImage {
    let data_len: usize = 720 * 128;
    let mut atr = vec![0_u8; 16 + data_len];
    atr[..2].copy_from_slice(&[0x96, 0x02]);
    let paragraphs = u16::try_from(data_len / 16).expect("paragraphs");
    atr[2..4].copy_from_slice(&paragraphs.to_le_bytes());
    atr[4..6].copy_from_slice(&128_u16.to_le_bytes());
    let sector_1 = [
        0x00, 0x01, 0x00, 0x07, 0x00, 0x07, 0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x4C, 0x0B, 0x07,
    ];
    atr[16..16 + sector_1.len()].copy_from_slice(&sector_1);
    DiskImage::from_atr(&atr).expect("ATR image")
}

/// Verifies REQ-095: with the boot test disk ([`boot_disk`]) and the SIOV shortcut off, the
/// `atari::frontend` records give `turbo on (disk drive)` when the disk turbo starts and
/// `turbo off` when it stops, each with the counts at the start of its frame (criterion 6).
#[test]
fn disk_turbo_records() {
    capture();
    let machine = Machine::new(MachineConfig {
        os_rom: fs::read(OS_ROM).expect("OS ROM"),
        basic_rom: Some(fs::read(BASIC_ROM).expect("BASIC ROM")),
        cartridge: None,
        disk: Some(boot_disk()),
        fast_sio: false,
        fast_tape: false,
        tape: None,
        standard: VideoStandard::Ntsc,
        audio_sample_rate: 48_000,
    })
    .expect("machine");
    let mut frontend = Frontend::new(machine);
    let (_, messages) = step_until_record(&mut frontend, 300);
    assert_eq!(messages, ["turbo on (disk drive)"], "the disk turbo starts");
    assert_eq!(
        frontend.turbo_cause(),
        Some(TurboCause::DiskDrive),
        "the cause"
    );
    let (steps, messages) = step_until_record(&mut frontend, 300);
    assert_eq!(messages, ["turbo off"], "the disk turbo stops");
    assert!(
        steps > 30,
        "30 frames after the last transfer: {steps} steps"
    );
    assert_eq!(frontend.machine().peek_ram(0x0600), 0xA5, "the disk booted");
    assert_eq!(frontend.turbo_cause(), None, "real time after the load");
}
