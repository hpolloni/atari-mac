//! The machine event log of the core (REQ-084), with a capture logger.
//!
//! Design: `docs/design/architecture.md`, "Logging and statistics". The logger and the
//! maximum level of the `log` crate are global to the process, so these tests are in their
//! own test binary, and each test holds one lock while it runs.

/// The tests and their helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod event_log {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, MutexGuard, Once, PoisonError};

    use atari_core::gtia::ConsoleKeys;
    use atari_core::keyboard::RETURN;
    use atari_core::media::{Cartridge, DiskImage, Tape, Xex, sio_checksum};
    use atari_core::{Machine, MachineConfig, VideoStandard};
    use atari_harness::data::{basic_rom, os_rom};
    use atari_harness::system::TEST_SAMPLE_RATE;
    use log::kv::Key;
    use log::{Level, LevelFilter, Log, Metadata, Record};

    /// The marker address of the boot code of the test media.
    const MARKER: u16 = 0x0600;
    /// The marker value.
    const DONE: u8 = 0xA5;
    /// The frames from the cold reset during which the tape test holds START.
    const START_FRAMES: u64 = 60;
    /// The first frame of the RETURN press of the NTSC tape boot (REQ-643).
    const RETURN_FRAME: u64 = 180;
    /// The frames that RETURN is held.
    const RETURN_FRAMES: u64 = 6;
    /// The frame limit of the tape boot (REQ-643 criterion 1).
    const TAPE_FRAMES: u64 = 2_400;
    /// The frame limit of the disk boot.
    const DISK_FRAMES: u64 = 600;
    /// The frames that run after the boot code wrote the marker.
    const AFTER_DONE: u64 = 5;

    /// One record that the capture logger received.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Entry {
        /// The `log` target.
        target: String,
        /// The level.
        level: Level,
        /// The key `frame`.
        frame: Option<u64>,
        /// The key `cycle`.
        cycle: Option<u64>,
        /// The message text.
        message: String,
    }

    /// The capture logger: keeps each record and counts each call.
    #[derive(Debug)]
    struct Capture {
        /// The records since the start of the test.
        records: Mutex<Vec<Entry>>,
        /// The calls of `enabled`, `log` and `flush`.
        calls: AtomicUsize,
    }

    impl Log for Capture {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            let _calls = self.calls.fetch_add(1, Ordering::SeqCst);
            true
        }

        fn log(&self, record: &Record<'_>) {
            let _calls = self.calls.fetch_add(1, Ordering::SeqCst);
            let key = |name: &str| {
                record
                    .key_values()
                    .get(Key::from_str(name))
                    .and_then(|value| value.to_u64())
            };
            let entry = Entry {
                target: record.target().to_owned(),
                level: record.level(),
                frame: key("frame"),
                cycle: key("cycle"),
                message: record.args().to_string(),
            };
            self.records
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(entry);
        }

        fn flush(&self) {
            let _calls = self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// The one logger of the test binary.
    static CAPTURE: Capture = Capture {
        records: Mutex::new(Vec::new()),
        calls: AtomicUsize::new(0),
    };

    /// Each test holds this lock while it runs.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Installs the logger once.
    static INSTALL: Once = Once::new();

    /// Takes the test lock, installs the logger, sets `level` and clears the records and the
    /// call count.
    fn start(level: LevelFilter) -> MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        INSTALL.call_once(|| log::set_logger(&CAPTURE).unwrap());
        log::set_max_level(level);
        clear();
        guard
    }

    /// Clears the records and the call count.
    fn clear() {
        CAPTURE
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        CAPTURE.calls.store(0, Ordering::SeqCst);
    }

    /// Takes the records of `target`.
    fn take(target: &str) -> Vec<Entry> {
        let mut records = CAPTURE
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (taken, kept) = records.drain(..).partition(|entry| entry.target == target);
        *records = kept;
        taken
    }

    /// An `Info` entry of `target` with the counts and the message.
    fn info(target: &str, frame: u64, cycle: u64, message: &str) -> Entry {
        Entry {
            target: target.to_owned(),
            level: Level::Info,
            frame: Some(frame),
            cycle: Some(cycle),
            message: message.to_owned(),
        }
    }

    /// One CAS chunk: the 8-byte header (`kind`, the data length, `aux`) and `data`.
    fn chunk(kind: [u8; 4], aux: u16, data: &[u8]) -> Vec<u8> {
        let mut bytes = kind.to_vec();
        bytes.extend(u16::try_from(data.len()).unwrap().to_le_bytes());
        bytes.extend(aux.to_le_bytes());
        bytes.extend(data);
        bytes
    }

    /// One tape record: `55 55`, the control byte, the 128 bytes of `data` and the checksum.
    fn tape_record(control: u8, data: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0x55, 0x55, control];
        bytes.extend(data);
        bytes.resize(131, 0);
        bytes.push(sio_checksum(&bytes));
        bytes
    }

    /// The boot test tape of REQ-643 (the bytes of `media.rs`): two boot records and the
    /// end-of-file record, at 600 baud.
    fn boot_tape() -> Tape {
        let first = tape_record(
            0xFC,
            &[0x00, 0x02, 0x00, 0x07, 0x86, 0x07, 0x4C, 0x80, 0x07],
        );
        let second = tape_record(0xFC, &[0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x18, 0x60]);
        let mut file = chunk(*b"FUJI", 0, b"");
        file.extend(chunk(*b"baud", 600, &[]));
        file.extend(chunk(*b"data", 20_000, &first));
        file.extend(chunk(*b"data", 3_000, &second));
        file.extend(chunk(*b"data", 3_000, &tape_record(0xFE, &[])));
        Tape::from_cas(&file).unwrap()
    }

    /// An ATR file with 720 sectors of 128 bytes: `first` in sector 1, `$00` elsewhere.
    fn sd_atr(first: &[u8]) -> Vec<u8> {
        let mut data = first.to_vec();
        data.resize(720 * 128, 0);
        let paragraphs = u16::try_from(data.len() / 16).unwrap();
        let mut file = vec![0x96, 0x02];
        file.extend(paragraphs.to_le_bytes());
        file.extend(128_u16.to_le_bytes());
        file.extend([0; 10]);
        file.extend(data);
        file
    }

    /// Sector 1 of the SD boot image of REQ-636 criterion 1: the boot header and
    /// `LDA #$A5`, `STA $0600`, `JMP $070B`.
    fn boot_sector() -> Vec<u8> {
        let mut sector = vec![0x00, 0x01, 0x00, 0x07, 0x00, 0x07];
        sector.extend([0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x4C, 0x0B, 0x07]);
        sector.resize(128, 0);
        sector
    }

    /// The SD boot image of REQ-636 criterion 1 (the bytes of `media.rs`).
    fn sd_boot_image() -> DiskImage {
        DiskImage::from_atr(&sd_atr(&boot_sector())).unwrap()
    }

    /// A machine with `AltirraOS`, the BASIC ROM, `cartridge`, `disk` and `tape`, NTSC.
    fn machine(
        cartridge: Option<Cartridge>,
        disk: Option<DiskImage>,
        tape: Option<Tape>,
        fast_sio: bool,
    ) -> Machine {
        Machine::new(MachineConfig {
            os_rom: os_rom().unwrap(),
            basic_rom: Some(basic_rom().unwrap()),
            cartridge,
            disk,
            fast_sio,
            tape,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap()
    }

    /// A change of a line that the test watches: the frame count before the cycle, the
    /// cycle count after it (the number of the cycle) and the new level.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Change {
        /// `Machine::frame_count` before the cycle.
        frame: u64,
        /// The number of the cycle of the change.
        cycle: u64,
        /// The new level (`true` = motor on, or command line low).
        on: bool,
    }

    /// Runs `m` cycle by cycle for at most `max_frames` frames, until `AFTER_DONE` frames
    /// after `$0600` holds `$A5`, with `keys` called with the frame count before each frame.
    /// Gives the changes of `line` and the frame count after the frame where `$0600` held
    /// `$A5`.
    fn run(
        m: &mut Machine,
        max_frames: u64,
        mut keys: impl FnMut(&mut Machine, u64),
        line: impl Fn(&Machine) -> bool,
    ) -> (Vec<Change>, Option<u64>) {
        let mut changes = Vec::new();
        let mut done = None;
        let mut level = line(m);
        for _ in 0..max_frames {
            let frame = m.frame_count();
            keys(m, frame);
            let target = frame + 1;
            while m.frame_count() < target {
                let before = m.frame_count();
                m.run_cycle();
                let now = line(m);
                if now != level {
                    level = now;
                    changes.push(Change {
                        frame: before,
                        cycle: m.cycle_count(),
                        on: now,
                    });
                }
            }
            if done.is_none() && m.peek_ram(MARKER) == DONE {
                done = Some(target);
            }
            if done.is_some_and(|d| target >= d + AFTER_DONE) {
                break;
            }
        }
        (changes, done)
    }

    /// The tape boot of REQ-643 (NTSC, START until frame 60, RETURN in frames 180-186).
    /// The OS leaves the motor on after the boot, so the run then presses the RESET key
    /// (the PIA reset stops the motor) and runs `AFTER_DONE` more frames. Gives the
    /// machine, the motor changes and the frame of the marker.
    fn tape_boot() -> (Machine, Vec<Change>, Option<u64>) {
        let mut m = machine(None, None, Some(boot_tape()), false);
        m.set_console_keys(ConsoleKeys {
            start: true,
            ..ConsoleKeys::default()
        });
        let keys = |m: &mut Machine, frame: u64| {
            if frame == START_FRAMES {
                m.set_console_keys(ConsoleKeys::default());
            }
            if frame == RETURN_FRAME {
                m.key_down(RETURN);
            }
            if frame == RETURN_FRAME + RETURN_FRAMES {
                m.key_up();
            }
        };
        let (mut changes, done) = run(&mut m, TAPE_FRAMES, keys, Machine::tape_motor_on);
        if done.is_some() {
            m.press_reset();
            let (after, _) = run(&mut m, AFTER_DONE, |_, _| {}, Machine::tape_motor_on);
            changes.extend(after);
        }
        (m, changes, done)
    }

    /// The boot of the SD boot image on the serial path (or with the SIOV shortcut). Gives
    /// the machine, the command line changes and the frame of the marker.
    fn disk_boot(fast_sio: bool) -> (Machine, Vec<Change>, Option<u64>) {
        let mut m = machine(None, Some(sd_boot_image()), None, fast_sio);
        let (changes, done) = run(
            &mut m,
            DISK_FRAMES,
            |_, _| {},
            |m: &Machine| m.sio().command_line_low(),
        );
        (m, changes, done)
    }

    /// The reset run: 10 frames, the RESET key, 5 frames, a cold reset. Gives the counts
    /// before the RESET key and before the cold reset.
    fn reset_run() -> ((u64, u64), (u64, u64)) {
        let mut m = machine(None, None, None, false);
        for _ in 0..10 {
            m.run_frame();
        }
        let warm = (m.frame_count(), m.cycle_count());
        m.press_reset();
        for _ in 0..5 {
            m.run_frame();
        }
        let cold = (m.frame_count(), m.cycle_count());
        m.power_on();
        (warm, cold)
    }

    /// An XEX file of two segments: 8 bytes at `$3000` and RUNAD (2 bytes).
    fn xex() -> Xex {
        let mut file = vec![0xFF, 0xFF];
        for (start, data) in [
            (
                0x3000_u16,
                &[0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x4C, 0x05, 0x30][..],
            ),
            (0x02E0, &[0x00, 0x30][..]),
        ] {
            let end = start + u16::try_from(data.len() - 1).unwrap();
            file.extend(start.to_le_bytes());
            file.extend(end.to_le_bytes());
            file.extend(data);
        }
        Xex::parse(&file).unwrap()
    }

    /// The media records of each kind of media, after each step: the 8 KB and 16 KB
    /// cartridges at `Machine::new`, an executable, a disk image and a tape image on a
    /// running machine. Each item: the counts at the call and the `atari::media` records.
    fn media_run() -> Vec<((u64, u64), Vec<Entry>)> {
        let mut out = Vec::new();
        for size in [0x2000, 0x4000] {
            let _m = machine(
                Some(Cartridge::from_raw(vec![0; size]).unwrap()),
                None,
                None,
                false,
            );
            out.push(((0, 0), take(MEDIA)));
        }
        let mut m = machine(None, None, None, false);
        let _ignored = take(MEDIA);
        let mut step = |m: &mut Machine, action: &dyn Fn(&mut Machine)| {
            for _ in 0..3 {
                m.run_frame();
            }
            let counts = (m.frame_count(), m.cycle_count());
            action(m);
            out.push((counts, take(MEDIA)));
        };
        step(&mut m, &|m| m.start_xex(&xex()));
        step(&mut m, &|m| m.insert_disk(sd_boot_image()));
        step(&mut m, &|m| m.insert_tape(boot_tape()));
        out
    }

    /// The target of the tape records.
    const TAPE: &str = "atari::tape";
    /// The target of the SIO records.
    const SIO: &str = "atari::sio";
    /// The target of the reset records.
    const RESET: &str = "atari::reset";
    /// The target of the media records.
    const MEDIA: &str = "atari::media";

    /// Verifies REQ-084: the boot test tape of `media.rs` (`tape_boot`: NTSC, START at the
    /// cold reset, RETURN after the prompt tone) gives `atari::tape` records at `Info`:
    /// `motor on, record 0 byte 0`, then `record <r> starts: 132 bytes at 600 baud` for each
    /// of the three data records, `tape ends`, and `motor off, record 3 byte 0` at the RESET
    /// key after the boot (the OS leaves the motor on), with the record index and the byte
    /// of the design. The `frame` and `cycle` keys of the motor records are the counts of the
    /// cycle where the PIA CA2 output changed (criteria 1, 2 and 3).
    #[test]
    fn tape_records() {
        let _lock = start(LevelFilter::Debug);
        let (_m, motor, done) = tape_boot();
        assert!(done.is_some(), "the tape boots");
        let records = take(TAPE);
        assert!(
            records.iter().all(|e| e.level == Level::Info),
            "1: the tape records are Info"
        );
        let motor_records: Vec<&Entry> = records
            .iter()
            .filter(|e| e.message.starts_with("motor"))
            .collect();
        assert_eq!(
            motor_records.len(),
            motor.len(),
            "one record for each change"
        );
        assert_eq!(motor.len(), 2, "the motor starts and stops");
        for (entry, change) in motor_records.iter().zip(&motor) {
            let state = if change.on { "on" } else { "off" };
            assert!(
                entry
                    .message
                    .starts_with(&format!("motor {state}, record ")),
                "1: {entry:?}"
            );
            assert_eq!(
                (entry.frame, entry.cycle),
                (Some(change.frame), Some(change.cycle)),
                "2: the counts of the CA2 change"
            );
        }
        assert_eq!(
            motor_records.first().map(|e| e.message.as_str()),
            Some("motor on, record 0 byte 0"),
            "1: the motor starts at the start of the tape"
        );
        let starts: Vec<&Entry> = records
            .iter()
            .filter(|e| e.message.starts_with("record "))
            .collect();
        assert_eq!(starts.len(), 3, "1: the three data records start");
        for (index, entry) in starts.iter().enumerate() {
            assert_eq!(
                entry.message,
                format!("record {index} starts: 132 bytes at 600 baud"),
                "1: the record start"
            );
            let first_on = motor.first().unwrap();
            assert!(
                entry.cycle > Some(first_on.cycle) && entry.frame.is_some(),
                "2: the record starts while the motor runs: {entry:?}"
            );
        }
        let tail: Vec<&str> = records
            .iter()
            .rev()
            .take(2)
            .map(|e| e.message.as_str())
            .collect();
        assert_eq!(
            tail,
            ["motor off, record 3 byte 0", "tape ends"],
            "1: the tape ends, then the motor stops at the RESET key"
        );
    }

    /// Verifies REQ-084: the SD boot image of `media.rs` (`sd_boot_image`) on the serial
    /// path gives `atari::sio` records: `command device $31 command $52 aux1 $01 aux2 $00
    /// checksum ok`, `reply ACK` and `reply COMPLETE` with `data frame 128 bytes` at
    /// `Info`, and `sent $xx` and `received $xx` at `Debug`, with the bytes of the frames.
    /// With the SIOV shortcut on, the record is `SIOV device $31 command $52 aux1 $01 aux2
    /// $00 status $01`. Each record has the `frame` and `cycle` keys (criteria 1, 2 and 3).
    #[test]
    fn sio_records() {
        let _lock = start(LevelFilter::Debug);
        let (m, command_line, done) = disk_boot(false);
        assert!(done.is_some(), "the disk boots");
        let records = take(SIO);
        assert!(
            records
                .iter()
                .all(|e| e.frame.is_some() && e.cycle.is_some()),
            "2: each record has the counts"
        );
        let read = "command device $31 command $52 aux1 $01 aux2 $00 checksum ok";
        let at = records
            .iter()
            .position(|e| e.message == read)
            .unwrap_or_else(|| panic!("1: no record `{read}`: {records:?}"));
        let command = &records[at];
        assert_eq!(command.level, Level::Info, "1: the command record is Info");
        let edge = command_line
            .iter()
            .find(|c| !c.on && Some(c.cycle) == command.cycle)
            .expect("2: the cycle of the rising edge of the command line");
        assert_eq!(command.frame, Some(edge.frame), "2: the frame of the edge");
        // The bytes that the computer sent: the command frame and its checksum.
        let sent: Vec<&str> = records[..at]
            .iter()
            .rev()
            .take(5)
            .map(|e| e.message.as_str())
            .collect();
        let checksum = sio_checksum(&[0x31, 0x52, 0x01, 0x00]);
        let checksum = format!("sent ${checksum:02X}");
        let want = [
            checksum.as_str(),
            "sent $00",
            "sent $01",
            "sent $52",
            "sent $31",
        ];
        assert_eq!(
            sent, want,
            "1: the sent bytes of the command frame, last first"
        );
        assert!(
            records[..at]
                .iter()
                .rev()
                .take(5)
                .all(|e| e.level == Level::Debug),
            "1: the sent records are Debug"
        );
        // The replies, with the cycle of the decision and the start cycle of each byte.
        let cycle = command.cycle.unwrap();
        let ack = &records[at + 1];
        let ack_start = cycle + 1_790;
        assert_eq!(
            ack,
            &info(
                SIO,
                edge.frame,
                cycle,
                &format!("reply ACK at cycle {ack_start}")
            ),
            "1: the ACK"
        );
        let complete = &records[at + 2];
        assert!(
            complete.message.starts_with("reply COMPLETE at cycle ")
                && complete.message.ends_with(", data frame 128 bytes")
                && complete.level == Level::Info
                && complete.cycle == Some(cycle),
            "1: the COMPLETE: {complete:?}"
        );
        let mut want: Vec<u8> = boot_sector();
        want.push(sio_checksum(&want));
        let received: Vec<String> = records[at + 3..at + 3 + want.len()]
            .iter()
            .map(|e| {
                assert_eq!(e.level, Level::Debug, "1: the received records are Debug");
                assert_eq!(e.cycle, Some(cycle), "2: the cycle of the decision");
                e.message.split(" at cycle ").next().unwrap().to_owned()
            })
            .collect();
        let want: Vec<String> = want.iter().map(|b| format!("received ${b:02X}")).collect();
        assert_eq!(received, want, "1: the data frame and its checksum");
        assert_eq!(m.peek_ram(MARKER), DONE, "the boot code ran");

        // The SIOV shortcut.
        clear();
        let (_m, _lines, done) = disk_boot(true);
        assert!(done.is_some(), "the disk boots with the SIOV shortcut");
        let records = take(SIO);
        let siov = "SIOV device $31 command $52 aux1 $01 aux2 $00 status $01";
        let entry = records
            .iter()
            .find(|e| e.message == siov)
            .unwrap_or_else(|| panic!("1: no record `{siov}`: {records:?}"));
        assert!(
            entry.level == Level::Info && entry.frame.is_some() && entry.cycle.is_some(),
            "1, 2: {entry:?}"
        );
    }

    /// Verifies REQ-084: `Machine::press_reset` gives `warm reset (RESET key)` and
    /// `Machine::power_on` gives `cold reset` in `atari::reset` at `Info`. The `frame` and
    /// `cycle` keys are `Machine::frame_count` and `Machine::cycle_count` before the call
    /// (criteria 1, 2 and 3).
    #[test]
    fn reset_records() {
        let _lock = start(LevelFilter::Debug);
        let _new = machine(None, None, None, false);
        let _ignored = take(RESET);
        let ((wf, wc), (cf, cc)) = reset_run();
        let records = take(RESET);
        // `Machine::new` of the run sends its own cold reset at the counts 0.
        assert_eq!(
            records,
            [
                info(RESET, 0, 0, "cold reset"),
                info(RESET, wf, wc, "warm reset (RESET key)"),
                info(RESET, cf, cc, "cold reset"),
            ],
            "1, 2: the reset records"
        );
        assert!(wc > 0 && cc > 0, "the counts are not 0");
    }

    /// Verifies REQ-084: a cartridge (8 KB and 16 KB), an executable, a disk image and a
    /// tape image each give one `atari::media` record at `Info` with the kind, the length in
    /// bytes and, for the cartridge, the CAR type, as the design gives (criteria 1, 2 and 3).
    #[test]
    fn media_records() {
        let _lock = start(LevelFilter::Debug);
        let runs = media_run();
        let mut messages = Vec::new();
        for ((frame, cycle), records) in &runs {
            let first = records.first().expect("one media record");
            assert_eq!(
                (first.level, first.frame, first.cycle),
                (Level::Info, Some(*frame), Some(*cycle)),
                "1, 2: {first:?}"
            );
            messages.push(first.message.as_str());
        }
        assert_eq!(
            messages,
            [
                "cartridge, 8192 bytes, CAR type 1",
                "cartridge, 16384 bytes, CAR type 2",
                "executable, 10 bytes in 2 segments",
                "disk, 92160 bytes, 720 sectors of 128 bytes",
                "tape, 396 bytes in 3 blocks",
            ],
            "1: the media records"
        );
        // The executable puts its boot image in D1: (one disk record after it); each other
        // insert gives one record.
        let counts: Vec<usize> = runs.iter().map(|(_, records)| records.len()).collect();
        assert_eq!(counts, [1, 1, 2, 1, 1], "one record for each insert");
        assert!(
            runs[2].1[1].message.starts_with("disk, "),
            "the boot image of the executable"
        );
    }

    /// The frame buffer and the RAM (`$0000-$FFFF`) of `m`.
    fn snapshot(m: &Machine) -> (Vec<u8>, Vec<u8>) {
        let ram = (0..=u16::MAX).map(|addr| m.peek_ram(addr)).collect();
        (m.frame().to_vec(), ram)
    }

    /// Verifies REQ-084: the tape boot and the disk boot of `tape_records` and
    /// `sio_records`, with the same frames and the same key input, give the same frame
    /// buffer and the same RAM (`$0000-$FFFF`) with the level `Debug` and with the level
    /// `Off` (criterion 4).
    #[test]
    fn log_keeps_machine_state() {
        let _lock = start(LevelFilter::Debug);
        let (tape_on, tape_motor_on, tape_done) = tape_boot();
        let (disk_on, disk_line_on, disk_done) = disk_boot(false);
        assert!(
            !CAPTURE
                .records
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty(),
            "the log is on"
        );
        log::set_max_level(LevelFilter::Off);
        let (tape_off, tape_motor_off, tape_done_off) = tape_boot();
        let (disk_off, disk_line_off, disk_done_off) = disk_boot(false);
        assert!(tape_done.is_some() && disk_done.is_some(), "the boots end");
        assert_eq!(
            (tape_done, disk_done),
            (tape_done_off, disk_done_off),
            "4: the same frames"
        );
        assert_eq!(tape_motor_on, tape_motor_off, "4: the same motor changes");
        assert_eq!(disk_line_on, disk_line_off, "4: the same command line");
        assert!(
            snapshot(&tape_on) == snapshot(&tape_off),
            "4: tape boot: the same frame buffer and RAM"
        );
        assert!(
            snapshot(&disk_on) == snapshot(&disk_off),
            "4: disk boot: the same frame buffer and RAM"
        );
        assert!(tape_on == tape_off, "4: tape boot: the same machine");
        assert!(disk_on == disk_off, "4: disk boot: the same machine");
    }

    /// Verifies REQ-084: with the level `Off`, a logger that counts its calls gets no call of
    /// `enabled`, `log` or `flush` during the runs of `tape_records`, `sio_records`,
    /// `reset_records` and `media_records` (criterion 5).
    #[test]
    fn no_logger_calls_when_off() {
        let _lock = start(LevelFilter::Off);
        let (_m, _motor, done) = tape_boot();
        assert!(done.is_some(), "the tape boots");
        let (_m, _line, done) = disk_boot(false);
        assert!(done.is_some(), "the disk boots");
        let (_m, _line, done) = disk_boot(true);
        assert!(done.is_some(), "the disk boots with the SIOV shortcut");
        let _counts = reset_run();
        let _media = media_run();
        assert_eq!(CAPTURE.calls.load(Ordering::SeqCst), 0, "5: no logger call");
        assert!(
            CAPTURE
                .records
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty(),
            "5: no record"
        );
    }
}
