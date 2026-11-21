//! Tests of the serial port and the two-tone mode (REQ-526 to REQ-533).
//!
//! A read "on cycle N" is a `peek` after the tick of cycle N, as a CPU read in cycle N sees
//! it. A level that the test sets "on cycle N" is set before the tick of cycle N. The frames
//! on SIO DATA IN have a start bit 0, the 8 data bits from bit 0 to bit 7 and a stop bit,
//! each for one bit time.

use super::{
    AUDC1, AUDC3, AUDCTL, AUDF1, AUDF2, AUDF3, AUDF4, BIT, IRQ_DONE, IRQ_NEEDED, IRQEN, SEROUT,
    SKCTL, SKRES, STIMER, all_reads, clear_irq, fire_log, fires_of, frame_levels, irqst, levels,
    new_pokey, run, run_checking, run_to, run_until, send, serial_pokey, skstat, spacing,
    timer_pokey,
};
use crate::keyboard;
use crate::pokey::Pokey;

/// `AUDC2`.
const AUDC2: u8 = 0x03;
/// `AUDC4`.
const AUDC4: u8 = 0x07;
/// `SERIN` (read).
const SERIN: u8 = 0x0D;
/// `IRQST` bit 5: serial input ready.
const IRQ_IN: u8 = 0x20;
/// `SKSTAT` bit 1: 0 = the serial input shifts.
const SK_BUSY: u8 = 0x02;
/// `SKSTAT` bit 4: SIO DATA IN.
const SK_DATA_IN: u8 = 0x10;
/// `SKSTAT` bit 5: 0 = serial input overrun.
const SK_OVERRUN_IN: u8 = 0x20;
/// `SKSTAT` bit 7: 0 = framing error.
const SK_FRAMING: u8 = 0x80;
/// The half period of the test clock on SIO CLOCK IN (REQ-527).
const CLOCK_HALF: u64 = 100;
/// Bit 0 of `fired`: timer 1. Bit 1: timer 2. Bit 3: timer 4.
const T1: u8 = 0b0001;
/// Timer 2 in `test_fired`.
const T2: u8 = 0b0010;
/// Timer 4 in `test_fired`.
const T4: u8 = 0b1000;

/// The 10 levels of the frame of `value`, with the stop bit `stop`.
fn frame_bits(value: u8, stop: bool) -> [bool; 10] {
    let mut bits = [false; 10];
    for (n, bit) in bits.iter_mut().enumerate().skip(1).take(8) {
        *bit = (value >> (n - 1)) & 1 != 0;
    }
    bits[9] = stop;
    bits
}

/// Sends the frame of `value` on SIO DATA IN with `bit` cycles per bit. The start edge is on
/// the next cycle S. Calls `each` after each tick. After the frame, the line is 1. Returns S.
pub(super) fn receive(
    p: &mut Pokey,
    value: u8,
    stop: bool,
    bit: u64,
    mut each: impl FnMut(&mut Pokey),
) -> u64 {
    let s = p.cycle() + 1;
    for level in frame_bits(value, stop) {
        for _ in 0..bit {
            p.set_serial_in(level);
            p.tick();
            each(p);
        }
    }
    p.set_serial_in(true);
    s
}

/// The setup of REQ-526: `SKCTL` = `$13`, `AUDCTL` = `$28`, `AUDF3` = `$28`, `AUDF4` = `$00`
/// and `IRQEN` = `irqen`, with SIO DATA IN at 1 for more than 1000 cycles.
fn receive_pokey(irqen: u8) -> Pokey {
    let mut p = timer_pokey(0x13, 0x28, [0, 0, 0x28, 0]);
    p.write(IRQEN, irqen);
    run(&mut p, 1000);
    p
}

/// Receives the frame of `value` and returns `(S, E)`: the start edge and the cycle on which
/// `IRQST` bit 5 goes to 0.
fn receive_irq(p: &mut Pokey, value: u8, stop: bool) -> (u64, Option<u64>) {
    let mut e = None;
    let s = receive(p, value, stop, BIT, |p| {
        if e.is_none() && irqst(p) & IRQ_IN == 0 {
            e = Some(p.cycle());
        }
    });
    (s, e)
}

/// `SKCTL` for mode `m` (`SKCTL` bits 6-4): `$03 + $10 x m`.
const fn mode_skctl(m: u8) -> u8 {
    0x03 + 0x10 * m
}

/// `true` for the modes in which POKEY drives SIO CLOCK IN (`010` and `110`).
const fn drives_clock(m: u8) -> bool {
    m == 0b010 || m == 0b110
}

/// A POKEY in mode `m` with the timers of REQ-527 criteria 1-4.
fn mode_pokey(m: u8, audc: u8) -> Pokey {
    let mut p = timer_pokey(mode_skctl(m), 0x28, [0, 0, 0x28, 0]);
    for reg in [AUDC3, AUDC4] {
        p.write(reg, audc);
    }
    p
}

/// The test clock on SIO CLOCK IN: it toggles every 100 cycles where the line is an input.
struct TestClock {
    /// The level of the line.
    level: bool,
    /// The test drives the line.
    on: bool,
}

impl TestClock {
    /// The clock for mode `m`.
    const fn new(m: u8) -> Self {
        Self {
            level: true,
            on: !drives_clock(m),
        }
    }

    /// Sets the line for the next tick. Returns `true` if it rises in that cycle.
    fn step(&mut self, p: &mut Pokey) -> bool {
        if !self.on || !(p.cycle() + 1).is_multiple_of(CLOCK_HALF) {
            return false;
        }
        self.level = !self.level;
        p.set_serial_clock_in(self.level);
        self.level
    }
}

/// The cycles on which SIO DATA OUT changes in the next `cycles` cycles, with the test clock
/// of mode `m`.
fn out_changes(p: &mut Pokey, m: u8, cycles: u64) -> Vec<u64> {
    let mut clock = TestClock::new(m);
    let mut last = p.serial_out();
    let mut changes = Vec::new();
    for _ in 0..cycles {
        let _rose = clock.step(p);
        p.tick();
        if p.serial_out() != last {
            last = p.serial_out();
            changes.push(p.cycle());
        }
    }
    changes
}

/// Sends `$55` in mode `m` and returns the distances between the changes of SIO DATA OUT.
/// `$55` changes the level on each bit boundary of the frame.
fn output_bit_times(p: &mut Pokey, m: u8, bit: u64) -> Vec<u64> {
    p.tick();
    p.write(SEROUT, 0x55);
    let changes = out_changes(p, m, 12 * bit);
    assert_eq!(changes.len(), 10, "mode {m:03b}: changes {changes:?}");
    spacing(&changes)
}

/// Receives the frame of `value` in mode `m` (REQ-527 criterion 3). In modes `000` and
/// `100`, the bits change on the rising edges of the test clock. In modes `010` and `110`,
/// they change on the rising edges of the level that POKEY drives. In the other modes, the
/// bit time is 94 cycles.
fn receive_in_mode(p: &mut Pokey, m: u8, value: u8) {
    let bits = frame_bits(value, true);
    match m {
        0b000 | 0b100 => {
            let mut clock = TestClock::new(m);
            let mut next = 0;
            // Two idle clock periods, then one bit on each rising edge.
            let mut idle = 2;
            for _ in 0..30 * CLOCK_HALF {
                if clock.step(p) {
                    if idle > 0 {
                        idle -= 1;
                    } else {
                        p.set_serial_in(bits.get(next).copied().unwrap_or(true));
                        next += 1;
                    }
                }
                p.tick();
            }
        }
        0b010 | 0b110 => {
            let mut last = p.serial_clock_in_drive();
            let mut next = 0;
            let mut idle = 2;
            for _ in 0..30 * BIT {
                p.tick();
                let drive = p.serial_clock_in_drive();
                if drive == Some(true) && last == Some(false) {
                    if idle > 0 {
                        idle -= 1;
                    } else {
                        p.set_serial_in(bits.get(next).copied().unwrap_or(true));
                        next += 1;
                    }
                }
                last = drive;
            }
        }
        _ => {
            let _s = receive(p, value, true, BIT, |_| {});
            run(p, 1000);
        }
    }
}

/// Verifies REQ-526: a frame for `$43` makes `SERIN` read `$43` from the cycle on which
/// `IRQST` bit 5 goes to 0.
#[test]
fn serial_input_frame_43() {
    let mut p = receive_pokey(0x20);
    let mut log = Vec::new();
    let _s = receive(&mut p, 0x43, true, BIT, |p| {
        log.push((p.cycle(), irqst(p) & IRQ_IN, p.peek(SERIN)));
    });
    let e = log
        .iter()
        .find(|(_, irq, _)| *irq == 0)
        .map(|(cycle, _, _)| *cycle)
        .expect("IRQST bit 5 goes to 0");
    for (cycle, _, serin) in &log {
        if *cycle < e {
            assert_eq!(*serin, 0x00, "SERIN before E = {e}, cycle {cycle}");
        } else {
            assert_eq!(*serin, 0x43, "SERIN from E = {e}, cycle {cycle}");
        }
    }
    run_checking(&mut p, 1000, "SERIN after the frame", |p| {
        p.peek(SERIN) == 0x43
    });
}

/// Verifies REQ-526: `IRQST` bit 5 goes to 0 from S+885 to S+901.
#[test]
fn serial_input_irq_timing() {
    let mut p = receive_pokey(0x20);
    let (s, e) = receive_irq(&mut p, 0x43, true);
    let e = e.expect("IRQST bit 5 goes to 0");
    assert!(
        (s + 885..=s + 901).contains(&e),
        "IRQST bit 5 on S+{} (S = {s})",
        e - s
    );
}

/// Verifies REQ-526: `SKSTAT` bit 1 reads 1 before S+39, 0 from S+55 to S+885 and 1 from
/// S+901.
#[test]
fn serial_input_busy_bit() {
    let mut p = receive_pokey(0x20);
    assert_ne!(skstat(&p) & SK_BUSY, 0, "idle before the frame");
    let mut log = Vec::new();
    let s = receive(&mut p, 0x43, true, BIT, |p| {
        log.push((p.cycle(), skstat(p) & SK_BUSY != 0));
    });
    for _ in 0..1000 {
        p.tick();
        log.push((p.cycle(), skstat(&p) & SK_BUSY != 0));
    }
    for (cycle, idle) in log {
        let offset = cycle - s;
        if !(39..901).contains(&offset) {
            assert!(idle, "SKSTAT bit 1 = 1 on S+{offset}");
        } else if (55..=885).contains(&offset) {
            assert!(!idle, "SKSTAT bit 1 = 0 on S+{offset}");
        }
    }
}

/// Verifies REQ-526: reads of `SERIN` have no side effects.
#[test]
fn serin_read_no_side_effect() {
    let mut a = receive_pokey(0x20);
    let mut b = receive_pokey(0x20);
    let mut reads_a = Vec::new();
    let _sa = receive(&mut a, 0x43, true, BIT, |p| reads_a.push(all_reads(p)));
    let mut reads_b = Vec::new();
    let _sb = receive(&mut b, 0x43, true, BIT, |p| {
        let _serin = p.read(SERIN);
        reads_b.push(all_reads(p));
    });
    assert!(reads_a == reads_b, "the reads differ");
    assert_eq!(a, b, "state after the frame");
    assert_eq!(a.peek(SERIN), 0x43, "SERIN");
}

/// Verifies REQ-526: 256 back-to-back bytes are received in order.
#[test]
fn serial_input_256_bytes() {
    let mut p = receive_pokey(0x20);
    let mut values = Vec::new();
    for value in 0..=0xFF {
        let _s = receive(&mut p, value, true, BIT, |p| {
            if irqst(p) & IRQ_IN == 0 {
                values.push(p.read(SERIN));
                p.write(IRQEN, 0x00);
                p.write(IRQEN, 0x20);
            }
        });
    }
    let expected: Vec<u8> = (0..=0xFF).collect();
    assert_eq!(values, expected, "values in order");
    assert_eq!(
        skstat(&p) & (SK_OVERRUN_IN | SK_FRAMING),
        SK_OVERRUN_IN | SK_FRAMING,
        "no overrun and no framing error"
    );
}

/// Verifies REQ-526: with `IRQEN` = `$00`, `SERIN` gets the byte and `IRQST` bit 5 stays 1.
#[test]
fn serial_input_without_irq() {
    let mut p = receive_pokey(0x00);
    let mut low = false;
    let _s = receive(&mut p, 0x43, true, BIT, |p| low |= irqst(p) & IRQ_IN == 0);
    run_checking(&mut p, 1000, "IRQST bit 5 is 1", |p| irqst(p) & IRQ_IN != 0);
    assert!(!low, "IRQST bit 5 read 0");
    assert_eq!(p.peek(SERIN), 0x43, "SERIN");
}

/// Verifies REQ-527: the output bit times and the input of each mode (HRM Table 10).
#[test]
fn serial_clock_mode_table() {
    // Criterion 1: the output.
    for (m, bit) in [
        (0b010, 94),
        (0b100, 94),
        (0b110, 56),
        (0b111, 56),
        (0b000, 2 * CLOCK_HALF),
        (0b001, 2 * CLOCK_HALF),
    ] {
        let mut p = mode_pokey(m, 0x00);
        let times = output_bit_times(&mut p, m, bit);
        assert!(
            times.iter().all(|t| *t == bit),
            "mode {m:03b}: bit times {times:?}, expected {bit}"
        );
    }
    // Criterion 3: the input.
    for m in 0..8 {
        let mut p = mode_pokey(m, 0x00);
        receive_in_mode(&mut p, m, 0x43);
        assert_eq!(p.peek(SERIN), 0x43, "SERIN in mode {m:03b}");
        assert_ne!(skstat(&p) & SK_FRAMING, 0, "SKSTAT bit 7 in mode {m:03b}");
    }
}

/// Verifies REQ-527: in modes `011` and `101`, SIO CLOCK OUT changes only while a frame is
/// received, on each fire of timer 4.
#[test]
fn async_output_clock_stops() {
    for m in [0b011, 0b101] {
        let mut p = mode_pokey(m, 0x00);
        let idle = p.serial_clock_out();
        run_checking(&mut p, 10_000, "SIO CLOCK OUT does not change", |p| {
            p.serial_clock_out() == idle
        });
        let mut changes = Vec::new();
        let mut fires = Vec::new();
        let mut last = p.serial_clock_out();
        let _s = receive(&mut p, 0x43, true, BIT, |p| {
            if p.serial_clock_out() != last {
                last = p.serial_clock_out();
                changes.push(p.cycle());
            }
            if p.test_fired() & T4 != 0 {
                fires.push(p.cycle());
            }
        });
        assert_eq!(fires.len(), 19, "mode {m:03b}: timer 4 fires");
        assert_eq!(changes, fires, "mode {m:03b}: changes on the fires");
    }
}

/// Verifies REQ-527: POKEY drives SIO CLOCK IN in modes `010` and `110` only.
#[test]
fn bidirectional_clock_drive() {
    for m in 0..8 {
        let mut p = mode_pokey(m, 0x00);
        let mut last = p.serial_clock_in_drive();
        let mut changes = Vec::new();
        let mut fires = Vec::new();
        for _ in 0..2000 {
            p.tick();
            let drive = p.serial_clock_in_drive();
            assert_eq!(drive.is_some(), drives_clock(m), "mode {m:03b}");
            if drive != last {
                changes.push(p.cycle());
            }
            last = drive;
            if p.test_fired() & T4 != 0 {
                fires.push(p.cycle());
            }
        }
        if drives_clock(m) {
            assert!(fires.len() > 30, "mode {m:03b}: timer 4 fires");
            assert_eq!(changes, fires, "mode {m:03b}: changes on the fires");
        }
    }
}

/// Verifies REQ-527: in mode `000`, a constant SIO CLOCK IN loads and sends nothing.
#[test]
fn external_clock_shifts_on_clock_in() {
    for level in [true, false] {
        let mut p = mode_pokey(0b000, 0x00);
        p.write(IRQEN, 0x10);
        run(&mut p, 10);
        p.set_serial_clock_in(level);
        p.tick();
        p.write(SEROUT, 0x4F);
        let out = p.serial_out();
        run_checking(&mut p, 10_000, "SIO DATA OUT and IRQST bit 4", |p| {
            p.serial_out() == out && irqst(p) & IRQ_NEEDED != 0
        });
    }
}

/// Verifies REQ-527: in mode `000`, SIO DATA OUT changes on the rising edges of SIO CLOCK IN,
/// and the input takes SIO DATA IN on the cycle of a falling edge.
#[test]
fn serial_clock_edges() {
    // The output.
    let mut p = mode_pokey(0b000, 0x00);
    p.tick();
    p.write(SEROUT, 0x55);
    let mut clock = TestClock::new(0b000);
    let mut last = p.serial_out();
    let mut changes = 0;
    for _ in 0..30 * CLOCK_HALF {
        let rose = clock.step(&mut p);
        p.tick();
        if p.serial_out() != last {
            last = p.serial_out();
            changes += 1;
            assert!(rose, "change without a rising edge on cycle {}", p.cycle());
        }
    }
    assert_eq!(changes, 10, "changes of the frame of $55");
    // The input: the frame level is on the line only in the cycles of the falling edges.
    // In all other cycles the line has the opposite level.
    for value in [0x43, 0xA5] {
        let mut p = mode_pokey(0b000, 0x00);
        let bits = frame_bits(value, true);
        let mut clock = TestClock::new(0b000);
        let mut next = 0;
        let mut idle = 2;
        for _ in 0..30 * CLOCK_HALF {
            let changes = clock.on && (p.cycle() + 1).is_multiple_of(CLOCK_HALF);
            let falls = changes && clock.level;
            let _rose = clock.step(&mut p);
            let bit = if falls && idle == 0 {
                bits.get(next).copied()
            } else {
                None
            };
            if falls {
                if idle > 0 {
                    idle -= 1;
                } else {
                    next += 1;
                }
            }
            let level = bit.unwrap_or(true);
            // Between the samples, the line has the opposite of the next sampled bit.
            let other = !bits.get(next).copied().unwrap_or(false);
            p.set_serial_in(if bit.is_some() { level } else { other });
            p.tick();
        }
        assert_eq!(p.peek(SERIN), value, "SERIN");
        assert_ne!(skstat(&p) & SK_FRAMING, 0, "SKSTAT bit 7");
    }
}

/// Verifies REQ-527: the bit time is two timer 4 periods: 2982 cycles for `$05CC`.
#[test]
fn serial_bit_rate_half_fire_rate() {
    let mut p = timer_pokey(0x23, 0x28, [0, 0, 0xCC, 0x05]);
    let times = output_bit_times(&mut p, 0b010, 2982);
    assert!(times.iter().all(|t| *t == 2982), "bit times {times:?}");
}

/// Verifies REQ-527: `AUDC3` and `AUDC4` do not change the bit times.
#[test]
fn serial_clock_ignores_audc() {
    for audc in [0x00, 0x1F, 0x8F, 0xAF] {
        for (m, bit) in [(0b010, 94), (0b100, 94), (0b000, 2 * CLOCK_HALF)] {
            let mut p = mode_pokey(m, audc);
            let times = output_bit_times(&mut p, m, bit);
            assert!(
                times.iter().all(|t| *t == bit),
                "AUDC3/4 = {audc:02X}, mode {m:03b}: {times:?}"
            );
        }
        let mut p = timer_pokey(0x23, 0x28, [0, 0, 0xCC, 0x05]);
        for reg in [AUDC3, AUDC4] {
            p.write(reg, audc);
        }
        let times = output_bit_times(&mut p, 0b010, 2982);
        assert!(
            times.iter().all(|t| *t == 2982),
            "AUDC3/4 = {audc:02X}: {times:?}"
        );
    }
}

/// Verifies REQ-528: timers 3 and 4 do not run while the input waits.
#[test]
fn async_holds_timers_3_4() {
    let mut p = receive_pokey(0x04);
    run_checking(&mut p, 10_000, "no fire of timers 3 and 4", |p| {
        p.test_fired() & 0b1100 == 0 && irqst(p) & 0x04 != 0
    });
}

/// Verifies REQ-528: the first fire of timer 4 is S+39 to S+55, then every 47 cycles.
#[test]
fn async_first_timer4_fire() {
    let mut p = receive_pokey(0x00);
    let mut fires = Vec::new();
    let s = receive(&mut p, 0x43, true, BIT, |p| {
        if p.test_fired() & T4 != 0 {
            fires.push(p.cycle());
        }
    });
    let first = *fires.first().expect("timer 4 fires");
    assert!(
        (s + 39..=s + 55).contains(&first),
        "first fire on S+{}",
        first - s
    );
    assert!(
        spacing(&fires).iter().all(|d| *d == 47),
        "spacing {:?}",
        spacing(&fires)
    );
}

/// Verifies REQ-528: timer 4 fires 19 times for each byte and stops after the stop bit.
#[test]
fn async_19_timer4_fires_per_byte() {
    let mut p = receive_pokey(0x20);
    for (value, gap) in [(0x43, 0), (0x55, 0), (0xA5, 500)] {
        let mut fires = Vec::new();
        let mut e = None;
        run(&mut p, gap);
        let s = receive(&mut p, value, true, BIT, |p| {
            if p.test_fired() & T4 != 0 {
                fires.push(p.cycle());
            }
            if e.is_none() && irqst(p) & IRQ_IN == 0 {
                e = Some(p.cycle());
                clear_irq(p, 0x20, IRQ_IN);
            }
        });
        let e = e.expect("IRQST bit 5 goes to 0");
        let before: Vec<u64> = fires.iter().copied().filter(|c| *c <= e).collect();
        assert_eq!(before.len(), 19, "fires from S = {s} to E = {e}");
        assert_eq!(before.last(), Some(&e), "the 19th fire is on E");
        assert_eq!(fires.len(), 19, "no fire after E in the frame");
    }
    run_checking(&mut p, 5000, "no fire until the next start edge", |p| {
        p.test_fired() & 0b1100 == 0
    });
}

/// Verifies REQ-528: bit times of 91 and 97 cycles are received.
#[test]
fn async_tolerates_3_percent() {
    for bit in [91, 97] {
        let mut p = receive_pokey(0x20);
        for value in [0x43, 0xA5] {
            let _s = receive(&mut p, value, true, bit, |_| {});
            run(&mut p, 200);
            assert_eq!(p.peek(SERIN), value, "SERIN, bit time {bit}");
            assert_ne!(skstat(&p) & SK_FRAMING, 0, "SKSTAT bit 7, bit time {bit}");
        }
    }
}

/// Verifies REQ-528: channels 3 and 4 do not change while the input waits.
#[test]
fn async_silences_ch3_ch4() {
    let mut p = receive_pokey(0x00);
    p.write(AUDC3, 0xAF);
    p.write(AUDC4, 0xAF);
    p.tick();
    let outputs = (p.test_channel_output(2), p.test_channel_output(3));
    run_checking(&mut p, 10_000, "channel 3 and 4 outputs", |p| {
        (p.test_channel_output(2), p.test_channel_output(3)) == outputs
    });
    for (reg, value) in [(AUDC1, 0x00), (AUDC2, 0x00), (AUDC3, 0x1F), (AUDC4, 0x00)] {
        p.write(reg, value);
    }
    run_checking(&mut p, 1000, "mix level 15", |p| p.test_mix_level() == 15);
}

/// Verifies REQ-529: a stop bit of 0 is a framing error.
#[test]
fn framing_error() {
    let mut p = receive_pokey(0x20);
    let mut log = Vec::new();
    let _s = receive(&mut p, 0x43, false, BIT, |p| {
        log.push((
            p.cycle(),
            irqst(p) & IRQ_IN,
            skstat(p) & SK_FRAMING,
            p.peek(SERIN),
        ));
    });
    let e = log
        .iter()
        .find(|entry| entry.1 == 0)
        .map(|entry| entry.0)
        .expect("IRQST bit 5 goes to 0");
    for (cycle, irq, framing, serin) in log {
        if cycle < e {
            assert_eq!(
                (irq, framing),
                (IRQ_IN, SK_FRAMING),
                "cycle {cycle} before E"
            );
        } else {
            assert_eq!((irq, framing, serin), (0, 0, 0x43), "cycle {cycle} from E");
        }
    }
}

/// Sends `$43`, then `$55`, with `between` called between the frames. Returns `SKSTAT`
/// after the first frame.
fn two_frames(p: &mut Pokey, between: impl FnOnce(&mut Pokey)) -> u8 {
    let _s = receive(p, 0x43, true, BIT, |_| {});
    let after_first = skstat(p);
    between(p);
    let _s = receive(p, 0x55, true, BIT, |_| {});
    run(p, 100);
    after_first
}

/// Verifies REQ-529: a byte while `IRQST` bit 5 is 0 is an overrun.
#[test]
fn serial_overrun() {
    let mut p = receive_pokey(0x20);
    let first = two_frames(&mut p, |_| {});
    assert_ne!(first & SK_OVERRUN_IN, 0, "no overrun after the first frame");
    assert_eq!(
        skstat(&p) & SK_OVERRUN_IN,
        0,
        "overrun after the second frame"
    );
    assert_eq!(p.peek(SERIN), 0x55, "SERIN");
}

/// Verifies REQ-529: a read of `SERIN` does not prevent the overrun.
#[test]
fn serin_read_does_not_prevent_overrun() {
    let mut p = receive_pokey(0x20);
    let _first = two_frames(&mut p, |p| {
        assert_eq!(p.read(SERIN), 0x43, "SERIN between the frames");
    });
    assert_eq!(
        skstat(&p) & SK_OVERRUN_IN,
        0,
        "overrun after the second frame"
    );
    assert_eq!(p.peek(SERIN), 0x55, "SERIN");
}

/// Verifies REQ-529: with `IRQEN` = `$00`, there is no overrun.
#[test]
fn no_overrun_when_disabled() {
    let mut p = receive_pokey(0x00);
    let _first = two_frames(&mut p, |_| {});
    assert_ne!(skstat(&p) & SK_OVERRUN_IN, 0, "no overrun");
    assert_eq!(p.peek(SERIN), 0x55, "SERIN");
}

/// Verifies REQ-529: the error bits stay 0 until a write to `SKRES`.
#[test]
fn skres_clears_serial_errors() {
    let mut p = receive_pokey(0x20);
    let _s = receive(&mut p, 0x43, false, BIT, |_| {});
    run(&mut p, 200);
    let _first = two_frames(&mut p, |_| {});
    let errors = SK_OVERRUN_IN | SK_FRAMING;
    assert_eq!(skstat(&p) & errors, 0, "both errors");
    clear_irq(&mut p, 0x20, IRQ_IN);
    let _s = receive(&mut p, 0x55, true, BIT, |_| {});
    run(&mut p, 100);
    assert_eq!(skstat(&p) & errors, 0, "the errors stay after a good frame");
    p.tick();
    p.write(SKRES, 0x00);
    p.tick();
    assert_eq!(skstat(&p) & errors, errors, "SKSTAT on X+1");
}

/// Verifies REQ-530: `SKSTAT` bit 4 follows SIO DATA IN for each `SKCTL` value.
#[test]
fn skstat_direct_serial_in() {
    let mut p = new_pokey();
    for skctl in 0..=0xFF {
        p.write(SKCTL, skctl);
        for level in [false, true, false, true] {
            p.set_serial_in(level);
            p.tick();
            let bit = skstat(&p) & SK_DATA_IN != 0;
            assert_eq!(bit, level, "SKCTL = {skctl:02X}, cycle {}", p.cycle());
        }
    }
}

/// Verifies REQ-530: `SKSTAT` bit 0 always reads 1.
#[test]
fn skstat_bit0_always_1() {
    let mut p = new_pokey();
    p.set_shift(true);
    p.set_key(keyboard::A, true);
    for skctl in 0..=0xFF {
        p.write(SKCTL, skctl);
        for level in [false, true] {
            p.set_serial_in(level);
            for _ in 0..200 {
                p.tick();
                assert_eq!(skstat(&p) & 0x01, 0x01, "SKCTL = {skctl:02X}");
            }
        }
    }
    let mut p = receive_pokey(0x20);
    let _s = receive(&mut p, 0x43, true, BIT, |p| {
        assert_eq!(skstat(p) & 0x01, 0x01, "cycle {} of the frame", p.cycle());
    });
    assert_eq!(p.peek(SERIN), 0x43, "the frame was received");
}

/// Verifies REQ-531: initialization mode keeps the last level on SIO DATA OUT.
#[test]
fn init_keeps_data_out_level() {
    for (offset, level) in [(10, false), (110, true)] {
        let mut p = serial_pokey(0x10);
        let (_, l) = send(&mut p, &[0x4F]);
        run_to(&mut p, l + offset);
        p.write(SKCTL, 0x00);
        assert_eq!(p.serial_out(), level, "on L+{offset}");
        run_checking(&mut p, 10_000, "SIO DATA OUT keeps its level", |p| {
            p.serial_out() == level
        });
        assert_eq!(p.take_serial_byte(), None, "no byte");
    }
}

/// Verifies REQ-531: initialization mode discards a pending `SEROUT` byte.
#[test]
fn init_discards_pending_serout() {
    let mut p = serial_pokey(0x10);
    let (_, l) = send(&mut p, &[0x4F]);
    clear_irq(&mut p, 0x10, IRQ_NEEDED);
    p.write(SEROUT, 0x55);
    run_to(&mut p, l + 500);
    let level = p.serial_out();
    p.write(SKCTL, 0x00);
    run(&mut p, 1000);
    p.write(SKCTL, 0x23);
    run_checking(&mut p, 5000, "no frame after initialization mode", |p| {
        p.serial_out() == level && irqst(p) & (IRQ_DONE | IRQ_NEEDED) == IRQ_NEEDED
    });
    assert_eq!(p.take_serial_byte(), None, "no byte");
}

/// Verifies REQ-531: mode `000` stops the output clock, not the shift register.
#[test]
fn clock_reset_keeps_shift_registers() {
    let mut p = serial_pokey(0x10);
    let (_, l) = send(&mut p, &[0x4F]);
    run_to(&mut p, l + 300);
    assert!(p.serial_out(), "data bit 2 on L+300");
    p.write(SKCTL, 0x03);
    run_checking(&mut p, 1000, "SIO DATA OUT stays 1", Pokey::serial_out);
    p.tick();
    p.write(SKCTL, 0x23);
    let w = p.cycle();
    let mut changes = Vec::new();
    let mut fires = Vec::new();
    let mut last = p.serial_out();
    let mut out = Vec::new();
    for _ in 0..2000 {
        p.tick();
        if p.test_fired() & T4 != 0 {
            fires.push(p.cycle());
        }
        if p.serial_out() != last {
            last = p.serial_out();
            changes.push(p.cycle());
        }
        out.push((p.cycle(), p.serial_out()));
    }
    let f = *fires.first().expect("timer 4 fires");
    // `$4F`: data bits 3-7 are 1, 0, 0, 1, 0, then the stop bit 1.
    let expected = [true, false, false, true, false, true];
    for (cycle, level) in out {
        let want = if cycle < f {
            true
        } else {
            let bit = usize::try_from((cycle - f) / BIT).expect("small");
            expected.get(bit).copied().unwrap_or(true)
        };
        assert_eq!(level, want, "cycle {cycle} (W = {w}, F = {f})");
    }
    assert_eq!(
        changes,
        vec![f + BIT, f + 3 * BIT, f + 4 * BIT, f + 5 * BIT],
        "changes"
    );
    assert_eq!(
        p.take_serial_byte().map(|byte| byte.value),
        Some(0x4F),
        "sent byte"
    );
}

/// A POKEY in the two-tone setup of REQ-531 criterion 4 and REQ-533 criterion 1.
fn two_tone_pokey(skctl: u8) -> Pokey {
    timer_pokey(skctl, 0x00, [0x05, 0x07, 0, 0])
}

/// Verifies REQ-531: clearing `SKCTL` bit 3 resets the two-tone flip-flop.
#[test]
fn two_tone_off_resets_tone_ff() {
    let mut p = two_tone_pokey(0x0B);
    let _c = run_until(&mut p, 1000, Pokey::serial_out).expect("SIO DATA OUT reads 1");
    p.write(SKCTL, 0x03);
    p.tick();
    p.write(SKCTL, 0x0B);
    assert!(!p.serial_out(), "0 after the writes");
    let fire = run_until(&mut p, 1000, |p| {
        if p.test_fired() & (T1 | T2) != 0 {
            return true;
        }
        assert!(
            !p.serial_out(),
            "0 before the next fire, cycle {}",
            p.cycle()
        );
        false
    });
    assert!(fire.is_some(), "timer 1 or 2 fires");
    assert!(p.serial_out(), "the fire toggles the output");
}

/// Verifies REQ-531: an `STIMER` write does not change the two-tone output.
#[test]
fn stimer_keeps_tone_ff() {
    for skctl in [0x0B, 0x8B] {
        let mut p = two_tone_pokey(skctl);
        let mut last = p.serial_out();
        let _toggle = run_until(&mut p, 1000, |p| p.serial_out() != last);
        p.tick();
        last = p.serial_out();
        p.write(STIMER, 0x00);
        assert_eq!(p.serial_out(), last, "on the STIMER cycle");
        run_checking(&mut p, 100, "SIO DATA OUT after STIMER", |p| {
            p.serial_out() == last
        });
    }
}

/// Verifies REQ-532: force break drives SIO DATA OUT to 0.
#[test]
fn force_break_drives_low() {
    let mut p = timer_pokey(0x03, 0x28, [0, 0, 0x28, 0]);
    assert!(p.serial_out(), "idle");
    p.write(SKCTL, 0x83);
    run_checking(&mut p, 1000, "0 from X+1", |p| !p.serial_out());
    p.write(SKCTL, 0x03);
    run_checking(&mut p, 1000, "1 from Y+1", Pokey::serial_out);
}

/// The levels of SIO DATA OUT and `IRQST` bits 3 and 4 from the `SEROUT` write to L+2000.
fn traced_frame(p: &mut Pokey, change_at: Option<(u64, u8)>) -> (u64, Vec<(bool, u8)>) {
    p.tick();
    p.write(SEROUT, 0x4F);
    let x = p.cycle();
    let mut trace = Vec::new();
    for _ in 0..2200 {
        p.tick();
        if let Some((offset, skctl)) = change_at
            && let Some(l) = load_cycle(x, &trace)
            && p.cycle() == l + offset
        {
            p.write(SKCTL, skctl);
        }
        trace.push((p.serial_out(), irqst(p) & (IRQ_DONE | IRQ_NEEDED)));
    }
    (x, trace)
}

/// The load cycle L in a trace that starts on cycle X+1: the first cycle with `IRQST` bit 3.
fn load_cycle(x: u64, trace: &[(bool, u8)]) -> Option<u64> {
    trace
        .iter()
        .position(|(_, irq)| irq & IRQ_DONE != 0)
        .map(|i| x + 1 + i as u64)
}

/// Verifies REQ-532: force break keeps the timing of the frame and of the IRQs.
#[test]
fn force_break_keeps_timing() {
    let mut reference = serial_pokey(0x10);
    let (x, normal) = traced_frame(&mut reference, None);
    let l = load_cycle(x, &normal).expect("load");
    // Criterion 2.
    let mut p = serial_pokey(0x10);
    p.write(SKCTL, 0xA3);
    let (xb, broken) = traced_frame(&mut p, None);
    assert_eq!(xb, x, "same write cycle");
    assert!(broken.iter().all(|(out, _)| !*out), "SIO DATA OUT is 0");
    let irq = |trace: &[(bool, u8)]| trace.iter().map(|(_, irq)| *irq).collect::<Vec<_>>();
    assert_eq!(irq(&broken), irq(&normal), "IRQST bits 3 and 4");
    // Criterion 3.
    let mut p = serial_pokey(0x10);
    p.write(SKCTL, 0xA3);
    let (_, ended) = traced_frame(&mut p, Some((300, 0x23)));
    for (i, ((out, irq), (want_out, want_irq))) in ended.iter().zip(&normal).enumerate() {
        let cycle = x + 1 + i as u64;
        assert_eq!(irq, want_irq, "IRQST on cycle {cycle}");
        if cycle < l + 300 {
            assert!(!*out, "0 before the write, cycle {cycle}");
        } else if cycle > l + 300 {
            assert_eq!(out, want_out, "the frame from L+301, cycle {cycle}");
        }
    }
    let frame = frame_levels(0x4F);
    let from_l = usize::try_from(l - x).expect("small");
    assert_eq!(
        normal[from_l..from_l + frame.len()]
            .iter()
            .map(|(out, _)| *out)
            .collect::<Vec<_>>(),
        frame,
        "the reference has the frame of REQ-525"
    );
}

/// The cycles on which SIO DATA OUT changes in the next `cycles` cycles.
fn changes(p: &mut Pokey, cycles: u64) -> Vec<u64> {
    out_changes(p, 0b111, cycles)
}

/// Verifies REQ-533: the mark and space tones of the OS cassette values.
#[test]
fn two_tone_frequencies() {
    for (skctl, period) in [(0x0B, 168), (0x8B, 224)] {
        let mut p = two_tone_pokey(skctl);
        let found = changes(&mut p, 20 * period);
        assert!(found.len() > 15, "SKCTL = {skctl:02X}: changes {found:?}");
        assert!(
            spacing(&found).iter().all(|d| *d == period),
            "SKCTL = {skctl:02X}: {:?}",
            spacing(&found)
        );
    }
}

/// The two-tone setup of REQ-533 criterion 2 with `SKCTL` = `skctl`.
fn two_tone_byte_pokey(skctl: u8) -> Pokey {
    timer_pokey(skctl, 0x28, [0x05, 0x07, 0xCC, 0x05])
}

/// Sends `$55` and returns L and the change cycles of SIO DATA OUT up to the end of the frame.
fn two_tone_frame(p: &mut Pokey) -> (u64, Vec<u64>) {
    p.tick();
    p.write(SEROUT, 0x55);
    let l = run_until(p, 3000, |p| irqst(p) & IRQ_DONE != 0).expect("load");
    let found = changes(p, 10 * 2982);
    (l, found)
}

/// Checks the change spacing inside each bit of the frame from L, without the first change
/// after each bit boundary. `period(bit)` gives the spacing of frame bit `bit` (0-9).
fn check_bit_tones(l: u64, found: &[u64], period: impl Fn(usize) -> u64) {
    for bit in 0..10 {
        let start = l + 2982 * bit as u64;
        let inside: Vec<u64> = found
            .iter()
            .copied()
            .filter(|c| (start..start + 2982).contains(c))
            .collect();
        let want = period(bit);
        assert!(inside.len() > 5, "bit {bit}: changes {inside:?}");
        assert!(
            spacing(&inside).iter().all(|d| *d == want),
            "bit {bit}: spacing {:?}, expected {want}",
            spacing(&inside)
        );
    }
}

/// Verifies REQ-533: the byte `$55` in two-tone mode.
#[test]
fn two_tone_byte_55() {
    let mut p = two_tone_byte_pokey(0x2B);
    let (l, found) = two_tone_frame(&mut p);
    let bits = frame_bits(0x55, true);
    check_bit_tones(l, &found, |bit| if bits[bit] { 168 } else { 224 });
}

/// The setup of REQ-533 criterion 3, with `IRQEN` = `irqen`. Returns T.
fn resync_setup(p: &mut Pokey, irqen: u8) -> u64 {
    p.tick();
    p.write(SKCTL, 0x08);
    p.write(AUDF1, 0x00);
    p.write(AUDF2, 0xFF);
    p.write(IRQEN, irqen);
    run(p, 100);
    p.tick();
    p.write(AUDCTL, 0x40);
    p.write(STIMER, 0x00);
    p.write(SKCTL, 0x0B);
    p.cycle()
}

/// Verifies REQ-533: the resync makes timer 1 fire on T+8, T+14 and T+20.
#[test]
fn two_tone_resync_8_14_20() {
    let mut p = new_pokey();
    let t = resync_setup(&mut p, 0x00);
    let log = fire_log(&mut p, 1000);
    let timer1: Vec<u64> = fires_of(&log, 1)
        .into_iter()
        .filter(|c| *c <= t + 20)
        .collect();
    assert_eq!(timer1, vec![t + 8, t + 14, t + 20], "timer 1 fires");
    assert_eq!(fires_of(&log, 2), Vec::<u64>::new(), "timer 2 fires");
}

/// Verifies REQ-533: the resync reload is not a fire of timer 2.
#[test]
fn two_tone_resync_no_fire() {
    let mut p = new_pokey();
    let _t = resync_setup(&mut p, 0x03);
    let mut latched = Vec::new();
    let mut fires = Vec::new();
    for _ in 0..1000 {
        p.tick();
        assert_ne!(irqst(&p) & 0x02, 0, "IRQST bit 1 on cycle {}", p.cycle());
        if p.test_fired() & T1 != 0 {
            fires.push(p.cycle());
        }
        if irqst(&p) & 0x01 == 0 {
            latched.push(p.cycle());
            clear_irq(&mut p, 0x03, 0x01);
        }
    }
    assert!(
        latched.len() > 100,
        "IRQST bit 0 changes: {}",
        latched.len()
    );
    assert!(
        latched.iter().all(|c| fires.contains(c)),
        "IRQST bit 0 changes only on the fires of timer 1"
    );
}

/// Verifies REQ-533: with force break, the whole frame has the timer 2 tone.
#[test]
fn two_tone_force_break_uses_timer2() {
    let mut p = two_tone_byte_pokey(0xAB);
    let (l, found) = two_tone_frame(&mut p);
    let inside: Vec<u64> = found
        .iter()
        .copied()
        .filter(|c| *c >= l && *c < l + 10 * 2982)
        .collect();
    assert!(inside.len() > 100, "changes {}", inside.len());
    assert!(
        spacing(&inside).iter().all(|d| *d == 224),
        "spacing {:?}",
        spacing(&inside)
    );
}

/// Verifies REQ-533: `AUDC1` and `AUDC2` do not change the two-tone output.
#[test]
fn two_tone_ignores_audc() {
    for skctl in [0x0B, 0x8B] {
        let mut reference = two_tone_pokey(skctl);
        let want = levels(&mut reference, 5000);
        for audc in [0x00, 0x1F, 0x8F, 0xAF] {
            let mut p = new_pokey();
            p.tick();
            p.write(SKCTL, skctl);
            p.write(AUDCTL, 0x00);
            for (reg, value) in [(AUDF1, 0x05), (AUDF2, 0x07), (AUDF3, 0), (AUDF4, 0)] {
                p.write(reg, value);
            }
            p.write(AUDC1, audc);
            p.write(AUDC2, audc);
            run(&mut p, 2000);
            assert_eq!(
                levels(&mut p, 5000),
                want,
                "SKCTL = {skctl:02X}, AUDC1/2 = {audc:02X}"
            );
        }
    }
}
