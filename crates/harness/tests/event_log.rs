//! The machine event log of the core (REQ-084), with a capture logger.
//!
//! Design: `docs/design/architecture.md`, "Logging and statistics". The logger and the
//! maximum level of the `log` crate are global to the process, so these tests are in their
//! own test binary, and each test holds one lock while it runs.

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

/// Verifies REQ-084: the boot test tape of `media.rs` (`tape_boot`: NTSC, START at the cold
/// reset, RETURN after the prompt tone) gives `atari::tape` records at `Info`: `motor on`, then
/// `record <r> starts: 132 bytes at 600 baud` for each of the three data records, and
/// `motor off`, with the record index and the byte of the design. The `frame` and `cycle`
/// keys of the motor records are the counts of the cycle where the PIA CA2 output changed
/// (criteria 1, 2 and 3).
#[test]
#[ignore = "planned: SLICE-09"]
fn tape_records() {
    planned();
}

/// Verifies REQ-084: the SD boot image of `media.rs` (`sd_boot_image`) on the serial path
/// gives `atari::sio` records: `command device $31 command $52 aux1 $01 aux2 $00 checksum ok`,
/// `reply ACK` and `reply COMPLETE` with `data frame 128 bytes` at `Info`, and `sent $xx`
/// and `received $xx` at `Debug`, with the bytes of the frames. With the SIOV shortcut on,
/// the record is `SIOV device $31 command $52 aux1 $01 aux2 $00 status $01`. Each record
/// has the `frame` and `cycle` keys (criteria 1, 2 and 3).
#[test]
#[ignore = "planned: SLICE-09"]
fn sio_records() {
    planned();
}

/// Verifies REQ-084: `Machine::press_reset` gives `warm reset (RESET key)` and
/// `Machine::power_on` gives `cold reset` in `atari::reset` at `Info`. The `frame` and
/// `cycle` keys are `Machine::frame_count` and `Machine::cycle_count` before the call
/// (criteria 1, 2 and 3).
#[test]
#[ignore = "planned: SLICE-09"]
fn reset_records() {
    planned();
}

/// Verifies REQ-084: a cartridge (8 KB and 16 KB), an executable, a disk image and a tape
/// image each give one `atari::media` record at `Info` with the kind, the length in bytes
/// and, for the cartridge, the CAR type, as the design gives (criteria 1, 2 and 3).
#[test]
#[ignore = "planned: SLICE-09"]
fn media_records() {
    planned();
}

/// Verifies REQ-084: the tape boot and the disk boot of `tape_records` and `sio_records`,
/// with the same frames and the same key input, give the same frame buffer and the same RAM
/// (`$0000-$FFFF`) with the level `Debug` and with the level `Off` (criterion 4).
#[test]
#[ignore = "planned: SLICE-09"]
fn log_keeps_machine_state() {
    planned();
}

/// Verifies REQ-084: with the level `Off`, a logger that counts its calls gets no call of
/// `enabled`, `log` or `flush` during the runs of `tape_records`, `sio_records`,
/// `reset_records` and `media_records` (criterion 5).
#[test]
#[ignore = "planned: SLICE-09"]
fn no_logger_calls_when_off() {
    planned();
}
