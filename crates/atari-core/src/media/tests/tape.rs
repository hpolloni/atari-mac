//! Tests of the cassette deck: the motor and the tape signal
//! (`docs/design/media.md`, "Test method for the cassette deck").

use super::cas::{chunk, file_t};
use super::sio_host::{HOST_BIT, device_bytes, sd_test_image, send_frame};
use crate::media::{SioBus, Tape, TapeDeck};
use crate::state::{Reader, State as _, Writer};
use crate::timing::VideoStandard;

/// The Status frame for D1:.
const STATUS: [u8; 5] = [0x31, 0x53, 0x00, 0x00, 0x84];

/// The bit levels of tape A, bits k = 0 to 19 (REQ-642 criterion 2).
const TAPE_A_BITS: [bool; 20] = {
    let (o, l) = (false, true);
    [
        o, l, o, l, o, l, o, l, o, l, // $55
        o, l, o, l, o, o, l, o, l, l, // $A5
    ]
};

/// The cycle where the tests start the motor.
const M: u64 = 1_000;

/// Loads the CAS file `file`.
fn tape(file: &[u8]) -> Tape {
    Tape::from_cas(file).expect("test tape")
}

/// Tape A: one `data` chunk, gap 100 ms, 600 baud (the default), the bytes `55 A5`.
fn tape_a() -> Tape {
    let mut file = chunk(*b"FUJI", 0, b"A");
    file.extend(chunk(*b"data", 100, &[0x55, 0xA5]));
    tape(&file)
}

/// Tape B: one `fsk ` chunk, gap 0 ms, the lengths 10 and 20 (in 0.1 ms).
fn tape_b() -> Tape {
    let mut file = chunk(*b"FUJI", 0, b"B");
    file.extend(chunk(*b"fsk ", 0, &[10, 0, 20, 0]));
    tape(&file)
}

/// A deck for `standard` with `tape` inserted at cycle 0, motor off.
fn deck_with(standard: VideoStandard, tape: Tape) -> TapeDeck {
    let mut deck = TapeDeck::new(standard);
    deck.insert(tape, 0);
    deck
}

/// A bus for `standard` with `tape` in the deck, motor off.
fn bus_with(standard: VideoStandard, tape: Tape) -> SioBus {
    let mut bus = SioBus::new(standard);
    bus.tape_deck_mut().insert(tape, 0);
    bus
}

/// Checks that `signal_at(p)` is `level` for each `p` in `from..=to`.
fn check_signal(deck: &TapeDeck, from: u64, to: u64, level: bool, name: &str) {
    if let Some(p) = (from..=to).find(|&p| deck.signal_at(p) != level) {
        panic!("{name}: signal at {p} is not {}", u8::from(level));
    }
}

/// `round(x)` of REQ-642 for a positive `x` below 2^52.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the test values are positive and below 2^52"
)]
fn round_cycles(x: f64) -> u64 {
    x.round() as u64
}

/// Checks the data block of tape A cycle by cycle through the motor: the motor starts at
/// the end of cycle `M`, so tape time `p` is on SIO DATA IN in cycle `M + 1 + p`. The gap is
/// `gap` cycles and bit `k` starts at `gap + round(k * f / 600)`.
fn decode_tape_a(standard: VideoStandard, gap: u64, name: &str) {
    let clock = standard.cpu_clock_hz();
    let mut bus = bus_with(standard, tape_a());
    bus.set_motor_line(true, M);
    let start = |k: u32| gap + round_cycles(f64::from(k) * clock / 600.0);
    for p in 0..gap {
        assert!(bus.data_in_level(M + 1 + p), "{name}: gap at {p}");
    }
    for (k, &level) in (0_u32..).zip(&TAPE_A_BITS) {
        for p in start(k)..start(k + 1) {
            assert_eq!(
                bus.data_in_level(M + 1 + p),
                level,
                "{name}: bit {k} at {p}"
            );
        }
    }
    let end = start(20);
    for p in end..end + 100_000 {
        assert!(
            bus.data_in_level(M + 1 + p),
            "{name}: mark after the block at {p}"
        );
    }
}

/// Verifies REQ-641: with a tape inserted and the motor line high, SIO DATA IN is 1 on each
/// of 100,000 cycles, and the tape position stays 0 (criterion 1).
#[test]
fn req641_motor_off_no_motion() {
    let mut bus = bus_with(VideoStandard::Ntsc, tape_a());
    // The tape signal is 0 from tape time 178,977: a moving tape would show it.
    bus.set_motor_line(false, 0);
    for now in 0..200_000 {
        assert!(bus.data_in_level(now), "DATA IN at {now}");
    }
    for now in [0, 99_999, 100_000, 199_999] {
        assert_eq!(bus.tape_deck().position(now), 0, "position at {now}");
    }
    assert!(!bus.tape_deck().motor_on(), "motor off");
}

/// Verifies REQ-641: the motor line goes low at the end of cycle M; SIO DATA IN in cycle
/// `M + 1 + p` is the tape signal at tape time p, for p from 0 to 200,000 (criterion 2).
#[test]
fn req641_motor_start() {
    let mut bus = bus_with(VideoStandard::Ntsc, tape_a());
    bus.set_motor_line(true, M);
    assert_eq!(bus.tape_deck().position(M), 0, "no motion in cycle M");
    let deck = bus.tape_deck().clone();
    for p in 0..=200_000 {
        assert_eq!(
            bus.data_in_level(M + 1 + p),
            deck.signal_at(p),
            "DATA IN at p = {p}"
        );
        assert_eq!(deck.position(M + 1 + p), p, "position at p = {p}");
    }
    assert!(!deck.signal_at(178_977), "the range has a 0 of the tape");
}

/// Verifies REQ-641: the motor line goes high at the end of cycle H: SIO DATA IN is 1 from
/// H + 1 and the position is H - M; it goes low again at the end of cycle M2: SIO DATA IN in
/// cycle `M2 + 1 + k` is the tape signal at `H - M + k`, for k from 0 to 200,000
/// (criterion 3).
#[test]
fn req641_motor_stop_keeps_position() {
    let mut bus = bus_with(VideoStandard::Ntsc, tape_a());
    bus.set_motor_line(true, M);
    // H in the start bit of `$55`: the tape signal is 0 in cycle H.
    let h = M + 1 + 180_000;
    assert!(!bus.data_in_level(h), "tape 0 in cycle H");
    bus.set_motor_line(false, h);
    for now in h + 1..h + 100_000 {
        assert!(bus.data_in_level(now), "DATA IN at {now}");
        assert_eq!(bus.tape_deck().position(now), h - M, "position at {now}");
    }
    let m2 = h + 100_000;
    bus.set_motor_line(true, m2);
    let deck = bus.tape_deck().clone();
    for k in 0..=200_000 {
        assert_eq!(
            bus.data_in_level(m2 + 1 + k),
            deck.signal_at(h - M + k),
            "DATA IN at k = {k}"
        );
    }
    // The same line setting again is no change.
    bus.set_motor_line(true, m2 + 10);
    assert_eq!(bus.tape_deck().position(m2 + 11), h - M + 10, "no restart");
}

/// Verifies REQ-641: after the eject of the tape, SIO DATA IN is 1 on each cycle, also while
/// the motor line is low (criterion 6).
#[test]
fn req641_eject() {
    let mut bus = bus_with(VideoStandard::Ntsc, tape_a());
    bus.set_motor_line(true, M);
    assert!(
        !bus.data_in_level(M + 1 + 178_977),
        "tape 0 before the eject"
    );
    let ejected = bus.tape_deck_mut().eject();
    assert_eq!(ejected, Some(tape_a()), "the tape comes back");
    assert!(bus.tape_deck().tape().is_none(), "no tape");
    for now in 0..300_000 {
        assert!(bus.data_in_level(now), "DATA IN at {now}");
    }
    assert_eq!(bus.tape_deck_mut().eject(), None, "second eject");
}

/// Verifies REQ-642: NTSC, tape A (gap 100 ms, 600 baud, `55 A5`): the gap ends at 178,976,
/// bit 1 starts at 181,960, the stop bit of `$55` is 205,824-208,806, `$A5` starts at
/// 208,807, the block ends at 238,636, the 20 bit levels are `0101010101` and
/// `0101001011`, and the line is 1 after the block; block 1 of file T ends at 39,732,950
/// (criteria 1, 2, 5).
#[test]
fn req642_data_block_ntsc() {
    let deck = deck_with(VideoStandard::Ntsc, tape_a());
    check_signal(&deck, 0, 178_976, true, "gap");
    check_signal(&deck, 178_977, 181_959, false, "start bit of $55");
    assert!(deck.signal_at(181_960), "bit 1 (data bit 0 of $55) is 1");
    check_signal(&deck, 205_824, 208_806, true, "stop bit of $55");
    assert!(!deck.signal_at(205_823), "data bit 7 of $55 is 0");
    assert!(!deck.signal_at(208_807), "start bit of $A5");
    assert!(deck.signal_at(238_635), "stop bit of $A5");
    check_signal(&deck, 238_636, 238_636 + 1_000_000, true, "after the block");
    assert_eq!(deck.block_starts(), vec![0, 238_636], "block end");
    // Criterion 2, cycle by cycle on SIO DATA IN.
    decode_tape_a(VideoStandard::Ntsc, 178_977, "NTSC");
    // Criterion 5: block 2 of T starts where block 1 ends.
    let t = deck_with(VideoStandard::Ntsc, tape(&file_t()));
    let starts = t.block_starts();
    assert_eq!(starts.get(1), Some(&39_732_950), "block 1 of T ends");
    assert!(t.signal_at(35_795_449), "gap of block 1");
    assert!(!t.signal_at(35_795_450), "start bit of record R");
    assert!(t.signal_at(39_732_949), "stop bit of the last byte of R");
    assert!(t.signal_at(39_732_950), "gap of block 2");
}

/// Verifies REQ-642: PAL, tape A: the gap ends at 177,344, bit 1 starts at 180,301, `$A5`
/// starts at 206,902, the block ends at 236,460, and the bit levels are those of NTSC
/// (criterion 3).
#[test]
fn req642_data_block_pal() {
    let deck = deck_with(VideoStandard::Pal, tape_a());
    check_signal(&deck, 0, 177_344, true, "gap");
    check_signal(&deck, 177_345, 180_300, false, "start bit of $55");
    assert!(deck.signal_at(180_301), "bit 1 is 1");
    assert!(deck.signal_at(206_901), "stop bit of $55");
    assert!(!deck.signal_at(206_902), "start bit of $A5");
    assert!(deck.signal_at(236_459), "stop bit of $A5");
    check_signal(&deck, 236_460, 236_460 + 1_000_000, true, "after the block");
    assert_eq!(deck.block_starts(), vec![0, 236_460], "block end");
    decode_tape_a(VideoStandard::Pal, 177_345, "PAL");
}

/// Verifies REQ-642: tape B (lengths 10 and 20): NTSC 0 for 0-1,789, 1 for 1,790-5,368 and
/// after; PAL 0 for 0-1,772, then 1 (criterion 4).
#[test]
fn req642_signal_block() {
    let ntsc = deck_with(VideoStandard::Ntsc, tape_b());
    check_signal(&ntsc, 0, 1_789, false, "NTSC signal 1");
    check_signal(&ntsc, 1_790, 5_368, true, "NTSC signal 2");
    check_signal(&ntsc, 5_369, 100_000, true, "NTSC after the block");
    assert_eq!(ntsc.block_starts(), vec![0, 5_369], "NTSC block end");
    let pal = deck_with(VideoStandard::Pal, tape_b());
    check_signal(&pal, 0, 1_772, false, "PAL signal 1");
    check_signal(&pal, 1_773, 100_000, true, "PAL after signal 1");
    // A signal block after a gap, and a signal of length 0: the signals alternate.
    let mut file = chunk(*b"FUJI", 0, &[]);
    file.extend(chunk(*b"fsk ", 1, &[10, 0, 0, 0, 10, 0, 10, 0]));
    let gapped = deck_with(VideoStandard::Ntsc, tape(&file));
    check_signal(&gapped, 0, 1_789, true, "gap of 1 ms");
    check_signal(&gapped, 1_790, 3_579, false, "signal 1 (0)");
    check_signal(
        &gapped,
        3_580,
        5_369,
        false,
        "signal 3 (0) after the empty signal 2",
    );
    check_signal(&gapped, 5_370, 7_159, true, "signal 4 (1)");
}

/// Verifies REQ-642: `SioBus::data_in_level` is the logical AND of the tape level and the
/// disk drive line: a drive stop bit during a tape 0 gives 0, a drive start bit during a
/// tape gap gives 0 (criterion 6).
#[test]
fn req642_and_with_drive() {
    let mut drive_only = SioBus::new(VideoStandard::Ntsc);
    drive_only.drive_mut().insert(sd_test_image());
    let _rise = send_frame(&mut drive_only, &STATUS, 300_000, HOST_BIT);
    let schedule = device_bytes(&drive_only);
    let &(ack, ack_start, bit) = schedule.first().expect("ACK");
    assert_eq!(ack, 0x41, "ACK first");
    let bit = u64::from(bit);
    let stop_bit = ack_start + 9 * bit..ack_start + 10 * bit;
    let last = schedule
        .last()
        .map_or(ack_start, |&(_, start, b)| start + 10 * u64::from(b));
    // Motor starts such that the tape start bit (tape times 178,977 to 181,959) covers the
    // stop bit of the ACK, and the start bit of the ACK is in the gap.
    let motor = stop_bit.start - 100 - 178_977 - 1;
    let mut bus = drive_only.clone();
    bus.tape_deck_mut().insert(tape_a(), 0);
    bus.set_motor_line(true, motor);
    for now in ack_start..ack_start + bit {
        assert!(bus.tape_deck().level(now), "tape gap at {now}");
        assert!(
            !bus.data_in_level(now),
            "drive start bit in the gap at {now}"
        );
    }
    for now in stop_bit {
        assert!(drive_only.data_in_level(now), "drive stop bit at {now}");
        assert!(!bus.tape_deck().level(now), "tape 0 at {now}");
        assert!(!bus.data_in_level(now), "tape 0 wins at {now}");
    }
    for now in 0..last + 10_000 {
        assert_eq!(
            bus.data_in_level(now),
            drive_only.data_in_level(now) && bus.tape_deck().level(now),
            "AND at {now}"
        );
    }
}

/// The deck state goes into a save state and comes back with the tape, the position and
/// the motor (REQ-018).
#[test]
fn deck_save_state_round_trip() {
    let mut deck = deck_with(VideoStandard::Ntsc, tape(&file_t()));
    deck.set_motor_line(true, M);
    deck.set_motor_line(false, M + 5_000);
    deck.set_motor_line(true, M + 9_000);
    let mut w = Writer::default();
    deck.save(&mut w);
    let bytes = w.into_bytes();
    let mut loaded = TapeDeck::new(VideoStandard::Ntsc);
    loaded.load(&mut Reader::new(&bytes)).expect("load");
    assert_eq!(loaded, deck, "same deck");
    assert_eq!(loaded.position(M + 10_001), 6_000, "position");
    let mut empty = TapeDeck::new(VideoStandard::Ntsc);
    let mut w = Writer::default();
    empty.save(&mut w);
    let bytes = w.into_bytes();
    empty.insert(tape_a(), 0);
    empty.load(&mut Reader::new(&bytes)).expect("load");
    assert_eq!(empty, TapeDeck::new(VideoStandard::Ntsc), "no tape");
}

/// Verifies REQ-644: `next_record` gives the first data block whose gap ends at the tape
/// time or later, with the tape time where it ends, and skips a signal block; `seek` puts
/// the position at the tape time in the cycle that it names, with the motor on and off
/// (criterion 4).
#[test]
fn req644_next_record_and_seek() {
    let deck = deck_with(VideoStandard::Ntsc, tape(&file_t()));
    let starts = deck.block_starts();
    let (end_0, end_1, end_2) = (starts[1], starts[2], starts[3]);
    let gap_r = round_cycles(20_000.0 * VideoStandard::Ntsc.cpu_clock_hz() / 1000.0);
    assert_eq!(deck.next_record(0), Some((0, end_0)), "in the first gap");
    assert_eq!(
        deck.next_record(gap_r),
        Some((0, end_0)),
        "at the first start bit"
    );
    assert_eq!(
        deck.next_record(gap_r + 1),
        Some((1, end_1)),
        "a record that started is skipped"
    );
    assert_eq!(
        deck.next_record(end_1),
        None,
        "the signal block is skipped, and no record remains"
    );
    assert_eq!(
        TapeDeck::new(VideoStandard::Ntsc).next_record(0),
        None,
        "no tape"
    );

    let mut running = deck.clone();
    running.set_motor_line(true, M);
    running.seek(end_0, M + 100);
    assert_eq!(running.position(M + 100), end_0, "motor on: in the cycle");
    assert_eq!(
        running.position(M + 105),
        end_0 + 5,
        "motor on: the tape moves on"
    );
    assert!(running.motor_on(), "the motor state does not change");

    let mut stopped = deck;
    stopped.seek(end_2, M);
    assert_eq!(stopped.position(M + 1_000), end_2, "motor off: no motion");
    assert!(!stopped.motor_on(), "the motor state does not change");
}
