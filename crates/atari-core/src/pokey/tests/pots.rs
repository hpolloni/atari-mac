//! Tests of the pot scan (REQ-534 to REQ-537).
//!
//! The reads use `peek`, which has no side effects (REQ-500). A read "on cycle N" is a
//! `peek` after the tick of cycle N, as a CPU read in cycle N sees it.

use std::collections::BTreeSet;

use super::{LINE, POTGO, SKCTL, new_pokey, run};
use crate::pokey::Pokey;

/// `ALLPOT` (read).
const ALLPOT: u8 = 0x08;
/// The cycles of a slow scan from `POTGO` to the end, with a margin: at most one scanline to
/// the first increment, 227 scanlines, and 2 cycles.
const SLOW_SCAN: u64 = 229 * LINE;
/// The cycles of a fast scan with a margin.
const FAST_SCAN: u64 = 300;
/// The slow-scan count of the full charge time (`ALLPOT` bit 0 changes at G + 114 x c + 1).
const SLOW_COUNTS: [u8; 3] = [1, 100, 227];
/// Phases of [`Scan::start`] for the tests that do not check all 114. With phase 94, the
/// 15 kHz pulse falls on cycle G+1, where the counter is cleared.
const PHASES: [u64; 8] = [0, 1, 20, 57, 93, 94, 95, 113];

/// `POTn`.
fn pot(p: &Pokey, n: u8) -> u8 {
    p.peek(n)
}

/// `ALLPOT`.
fn allpot(p: &Pokey) -> u8 {
    p.peek(ALLPOT)
}

/// `ALLPOT` bit `n` reads 1.
fn polling(p: &Pokey, n: u8) -> bool {
    allpot(p) & (1 << n) != 0
}

/// A pot scan setup: the POKEY, and the cycles W (`SKCTL` write) and G (`POTGO` write).
struct Scan {
    /// The POKEY.
    p: Pokey,
    /// The cycle of the `SKCTL` write.
    w: u64,
    /// The cycle of the last `POTGO` write.
    g: u64,
}

impl Scan {
    /// The setup of REQ-534: `SKCTL` = `skctl` on cycle W, the paddles of `pots`, then
    /// `100 + phase` cycles with `SKCTL` bit 2 = 0 and no scan (empty capacitors), then
    /// `POTGO` on cycle G. With `skctl` = `$03`, (G - W) modulo 114 = (100 + phase) modulo
    /// 114.
    fn start(skctl: u8, pots: &[(usize, u8)], phase: u64) -> Self {
        let mut p = new_pokey();
        p.tick();
        p.write(SKCTL, skctl);
        let w = p.cycle();
        for &(n, c) in pots {
            p.set_pot(n, c);
        }
        run(&mut p, 100 + phase);
        p.write(POTGO, 0x00);
        let g = p.cycle();
        Self { p, w, g }
    }

    /// A slow scan (`SKCTL` = `$03`).
    fn slow(pots: &[(usize, u8)], phase: u64) -> Self {
        Self::start(0x03, pots, phase)
    }

    /// One tick. Returns the cycle.
    fn tick(&mut self) -> u64 {
        self.p.tick();
        self.p.cycle()
    }

    /// Writes `POTGO` on the present cycle.
    fn potgo(&mut self) {
        self.p.write(POTGO, 0x00);
        self.g = self.p.cycle();
    }

    /// Runs until the tick of `cycle`.
    fn run_to(&mut self, cycle: u64) {
        assert!(self.p.cycle() <= cycle, "cycle {cycle} is in the past");
        let cycles = cycle - self.p.cycle();
        run(&mut self.p, cycles);
    }

    /// Runs until `ALLPOT` reads `$00` (the end of the scan). Returns the cycle.
    fn run_to_end(&mut self, max: u64) -> u64 {
        for _ in 0..max {
            let _cycle = self.tick();
            if allpot(&self.p) == 0 {
                return self.p.cycle();
            }
        }
        panic!("the scan did not end in {max} cycles");
    }

    /// Runs until `POT4` (no paddle) reads 1. This is the cycle P of the first pot counter
    /// increment after G, because an input without a paddle follows the counter.
    fn run_to_first_increment(&mut self) -> u64 {
        for _ in 0..2 * LINE {
            let cycle = self.tick();
            if pot(&self.p, 4) == 1 {
                return cycle;
            }
        }
        panic!("no pot counter increment after G = {}", self.g);
    }

    /// Runs until the end of the scan. Returns, for each input 0-3, the cycle on which its
    /// `ALLPOT` bit changed from 1 to 0 before the end of the scan (`None` if it did not), and
    /// checks that the bit does not change back to 1.
    fn run_recording_changes(&mut self) -> [Option<u64>; 4] {
        let mut changes = [None; 4];
        let mut was = allpot(&self.p);
        for _ in 0..SLOW_SCAN {
            let cycle = self.tick();
            let now = allpot(&self.p);
            if now == 0 {
                return changes;
            }
            assert_eq!(
                now & !was,
                0,
                "an ALLPOT bit changed back to 1 on cycle {cycle}"
            );
            for (n, change) in changes.iter_mut().enumerate() {
                if was & !now & (1 << n) != 0 {
                    *change = Some(cycle);
                }
            }
            was = now;
        }
        panic!("the scan did not end");
    }

    /// Runs until `ALLPOT` bit `n` reads 0. Returns the cycle.
    fn run_to_latch(&mut self, n: u8) -> u64 {
        for _ in 0..SLOW_SCAN {
            let cycle = self.tick();
            if !polling(&self.p, n) {
                return cycle;
            }
        }
        panic!("ALLPOT bit {n} stayed 1");
    }
}

/// Verifies REQ-534: Criterion 1: On cycle G+1, for each input below threshold, `ALLPOT` bit n
/// reads 1 and `POTn` reads 0.
#[test]
fn potgo_starts_scan() {
    for phase in 0..LINE {
        let mut s = Scan::slow(&[(0, 100), (1, 1), (2, 227), (3, 228)], phase);
        let cycle = s.tick();
        assert_eq!(cycle, s.g + 1, "cycle G+1");
        assert_eq!(allpot(&s.p), 0xFF, "ALLPOT, phase {phase}");
        for n in 0..8 {
            assert_eq!(pot(&s.p, n), 0, "POT{n}, phase {phase}");
        }
    }
}

/// Verifies REQ-534: Criterion 2: With `set_pot(0, 100)`, `POT0` reads 100 after the scan. `ALLPOT`
/// bit 0 reads 1 while `POT0` reads less than 100. It changes to 0 while the counter is 100.
#[test]
fn slow_pot_latches_count() {
    for phase in PHASES {
        let mut s = Scan::slow(&[(0, 100)], phase);
        let mut latched = false;
        for _ in 0..SLOW_SCAN {
            let cycle = s.tick();
            let bit = polling(&s.p, 0);
            if pot(&s.p, 0) < 100 {
                assert!(bit, "ALLPOT bit 0 on cycle {cycle}, POT0 < 100");
            }
            if !bit && !latched {
                latched = true;
                // POT4 has no paddle, so it shows the counter.
                assert_eq!(pot(&s.p, 4), 100, "the counter when bit 0 changes");
                assert_eq!(pot(&s.p, 0), 100, "POT0 when bit 0 changes");
            }
            if allpot(&s.p) == 0 {
                break;
            }
        }
        assert!(latched, "ALLPOT bit 0 changed to 0");
        assert_eq!(allpot(&s.p), 0, "the scan ended");
        assert_eq!(pot(&s.p, 0), 100, "POT0 after the scan, phase {phase}");
    }
}

/// Verifies REQ-534: Criterion 3: For each input that does not reach threshold, `POTn` reads 228
/// from cycle P + 227 x 114 + 1. `ALLPOT` reads `$00` first on cycle P + 227 x 114 + 2.
#[test]
fn slow_pot_scan_end() {
    for phase in PHASES {
        let mut s = Scan::slow(&[(0, 228), (1, 255)], phase);
        let p = s.run_to_first_increment();
        let last = p + 227 * LINE;
        s.run_to(last - 1);
        for n in 0..8 {
            assert_eq!(pot(&s.p, n), 227, "POT{n} before the last increment");
        }
        assert_eq!(allpot(&s.p), 0xFF, "ALLPOT before the last increment");
        // The read on the increment cycle is not checked (REQ-534 Notes).
        let _increment = s.tick();
        for offset in 1..=200 {
            let cycle = s.tick();
            assert_eq!(cycle, last + offset, "cycle");
            for n in 0..8 {
                assert_eq!(pot(&s.p, n), 228, "POT{n} on P + 227 x 114 + {offset}");
            }
            let expected = if offset == 1 { 0xFF } else { 0x00 };
            assert_eq!(
                allpot(&s.p),
                expected,
                "ALLPOT on P + 227 x 114 + {offset}, phase {phase}"
            );
        }
    }
}

/// Verifies REQ-534: Criterion 4: P is a cycle W + 81 + 114 k (k = 0, 1, 2, ...). For an input
/// without a paddle, `POTn` reads the old count on each cycle W + 80 + 114 k and the new count on
/// each cycle W + 82 + 114 k, during the scan.
#[test]
fn pot_increment_phase() {
    for phase in PHASES {
        let mut s = Scan::slow(&[], phase);
        let (w, g) = (s.w, s.g);
        // reads[i] = POT0 on cycle G + i.
        let mut reads = vec![pot(&s.p, 0)];
        for _ in 0..SLOW_SCAN {
            let _cycle = s.tick();
            reads.push(pot(&s.p, 0));
        }
        let read = |cycle: u64| reads[usize::try_from(cycle - g).unwrap()];
        let first = (g + 1..g + SLOW_SCAN).find(|&c| read(c) == 1).unwrap();
        assert_eq!(
            (first - w - 81) % LINE,
            0,
            "P = W + 81 + 114 k, phase {phase}"
        );
        let mut q = first;
        let mut count = 1;
        while count <= 228 {
            assert_eq!(read(q - 1), count - 1, "old count on W + 80 + 114 k");
            assert_eq!(read(q + 1), count, "new count on W + 82 + 114 k");
            // The count does not change between two pulses.
            if count < 228 {
                for c in q + 1..q + LINE {
                    assert_eq!(read(c), count, "count {count} on cycle {c}");
                }
            }
            q += LINE;
            count += 1;
        }
    }
}

/// Verifies REQ-534: Criterion 5: With `set_pot(0, 100)`, `ALLPOT` bit 0 changes to 0 on cycle G +
/// 100 x 114 + 1, for each of the 114 values of (G - W) modulo 114. This cycle has no fixed
/// relation to the pot counter increments.
#[test]
fn allpot_updates_each_cycle() {
    let mut phases_to_pulse = BTreeSet::new();
    for phase in 0..LINE {
        let mut s = Scan::slow(&[(0, 100)], phase);
        let latch = s.run_to_latch(0);
        assert_eq!(latch, s.g + 100 * LINE + 1, "phase {phase}");
        let _new = phases_to_pulse.insert((latch - s.w) % LINE);
    }
    assert_eq!(
        phases_to_pulse.len(),
        114,
        "the latch cycle takes each phase of the 15 kHz clock"
    );
}

/// Verifies REQ-535: Criterion 1: For each input 0-3, `set_pot(n, c)` makes `POTn` read c after the
/// scan, and `ALLPOT` bit n change to 0 on cycle G + 114 x c + 1, for c = 1, 100 and 227. The test
/// checks input 0 for each of the 114 values of (G - W) modulo 114.
#[test]
fn paddle_host_value_range() {
    for c in SLOW_COUNTS {
        for phase in 0..LINE {
            // Inputs 1-3 are checked for 3 phases; input 0 for each phase.
            let inputs: &[usize] = if phase % 50 == 0 { &[0, 1, 2, 3] } else { &[0] };
            let pots: Vec<(usize, u8)> = inputs.iter().map(|&n| (n, c)).collect();
            let mut s = Scan::slow(&pots, phase);
            let expected = s.g + 114 * u64::from(c) + 1;
            let changes = s.run_recording_changes();
            for &n in inputs {
                assert_eq!(
                    changes[n],
                    Some(expected),
                    "ALLPOT bit {n}, c = {c}, phase {phase}"
                );
                let n = u8::try_from(n).unwrap();
                assert_eq!(pot(&s.p, n), c, "POT{n} after the scan, phase {phase}");
            }
        }
    }
}

/// Verifies REQ-535: Criterion 2: `set_pot(0, 228)`, `set_pot(0, 255)`, or no call of `set_pot` for
/// input 0, makes `POT0` read 228 after the scan.
#[test]
fn no_paddle_reads_228() {
    let cases: [&[(usize, u8)]; 3] = [&[(0, 228)], &[(0, 255)], &[]];
    for pots in cases {
        let mut s = Scan::slow(pots, 7);
        let _end = s.run_to_end(SLOW_SCAN);
        assert_eq!(pot(&s.p, 0), 228, "POT0 with {pots:?}");
    }
    // A paddle that is removed again before the scan.
    let mut s = Scan::slow(&[(0, 50), (0, 228)], 7);
    let _end = s.run_to_end(SLOW_SCAN);
    assert_eq!(
        pot(&s.p, 0),
        228,
        "POT0 after set_pot(0, 50) and set_pot(0, 228)"
    );
}

/// Verifies REQ-535: Criterion 3: `POT4-POT7` read 228 after a slow scan. They read 229 after a
/// scan with `SKCTL` bit 2 = 1 (REQ-536 criterion 1).
#[test]
fn grounded_pots_read_228_and_229() {
    // Inputs 4-7 are grounded: the host cannot connect a paddle.
    let pots = [(4, 1), (5, 50), (6, 100), (7, 227)];
    let mut s = Scan::slow(&pots, 3);
    let _end = s.run_to_end(SLOW_SCAN);
    for n in 4..8 {
        assert_eq!(pot(&s.p, n), 228, "POT{n} after a slow scan");
    }
    let mut s = Scan::start(0x07, &pots, 3);
    let _end = s.run_to_end(FAST_SCAN);
    for n in 4..8 {
        assert_eq!(pot(&s.p, n), 229, "POT{n} after a fast scan");
    }
}

/// Verifies REQ-535: Criterion 4: With `set_pot(0, 200)`, a call of `set_pot(0, 50)` when `POT0`
/// reads 100 makes `ALLPOT` bit 0 read 0 from the next cycle. `POT0` reads 100 or 101 after the
/// scan.
#[test]
fn pot_change_during_scan() {
    for phase in PHASES {
        let mut s = Scan::slow(&[(0, 200)], phase);
        while pot(&s.p, 0) < 100 {
            let _cycle = s.tick();
        }
        assert!(polling(&s.p, 0), "ALLPOT bit 0 before the change");
        s.p.set_pot(0, 50);
        let mut ended = false;
        for _ in 0..SLOW_SCAN {
            let cycle = s.tick();
            assert!(!polling(&s.p, 0), "ALLPOT bit 0 on cycle {cycle}");
            if allpot(&s.p) == 0 {
                ended = true;
                break;
            }
        }
        assert!(ended, "the scan ended");
        let value = pot(&s.p, 0);
        assert!(
            value == 100 || value == 101,
            "POT0 after the scan: {value}, phase {phase}"
        );
    }
}

/// Verifies REQ-536: Criterion 1: With `SKCTL` = `$07` and a `POTGO` write on cycle G, `ALLPOT` bit
/// 4 reads 1 on cycle G+230, and `ALLPOT` reads `$00` from cycle G+231. `POT4` reads 229 from cycle
/// G+230.
#[test]
fn fast_pot_scan_end_231() {
    let mut s = Scan::start(0x07, &[], 0);
    for offset in 1..=FAST_SCAN {
        let cycle = s.tick();
        assert_eq!(cycle, s.g + offset, "cycle");
        if offset <= 230 {
            assert!(polling(&s.p, 4), "ALLPOT bit 4 on G+{offset}");
        } else {
            assert_eq!(allpot(&s.p), 0x00, "ALLPOT on G+{offset}");
        }
        if offset >= 230 {
            assert_eq!(pot(&s.p, 4), 229, "POT4 on G+{offset}");
        } else {
            // The counter is cleared on G+1 and counts on each cycle after that.
            assert_eq!(u64::from(pot(&s.p, 4)), offset - 1, "POT4 on G+{offset}");
        }
    }
}

/// Verifies REQ-536: Criterion 2: With `SKCTL` = `$03` for at least 100 cycles (no scan), a write
/// of `SKCTL` = `$07` on cycle F and a `POTGO` write on cycle F+1: `set_pot(0, 1)` gives a `POT0`
/// value from 112 to 114, `set_pot(0, 2)` gives a value from 226 to 228, and `set_pot(0, 3)` gives
/// 229.
#[test]
fn fast_pot_paddle_count() {
    for (c, range) in [(1, 112..=114), (2, 226..=228), (3, 229..=229)] {
        let mut p = new_pokey();
        p.tick();
        p.write(SKCTL, 0x03);
        p.set_pot(0, c);
        run(&mut p, 100);
        p.write(SKCTL, 0x07);
        p.tick();
        p.write(POTGO, 0x00);
        let mut ended = false;
        for _ in 0..FAST_SCAN {
            p.tick();
            if allpot(&p) == 0 {
                ended = true;
                break;
            }
        }
        assert!(ended, "the fast scan ended");
        let value = pot(&p, 0);
        assert!(range.contains(&value), "set_pot(0, {c}): POT0 = {value}");
    }
}

/// Verifies REQ-536: Criterion 3: With `SKCTL` = `$07` and `set_pot(0, 1)`, a first fast scan
/// latches `POT0` = v. A second `POTGO` write 1000 cycles later keeps `POT0` at v and `ALLPOT` bit
/// 0 at 0 on each cycle of the second scan.
#[test]
fn fast_pot_no_dump() {
    let mut s = Scan::start(0x07, &[(0, 1)], 0);
    let first = s.g;
    let _end = s.run_to_end(FAST_SCAN);
    let v = pot(&s.p, 0);
    assert!(v < 229, "the first scan latched a count: {v}");
    s.run_to(first + 1000);
    s.potgo();
    assert_eq!(pot(&s.p, 0), v, "POT0 on the POTGO cycle");
    assert!(!polling(&s.p, 0), "ALLPOT bit 0 on the POTGO cycle");
    assert!(polling(&s.p, 4), "the second scan runs");
    for _ in 0..FAST_SCAN {
        let cycle = s.tick();
        assert_eq!(pot(&s.p, 0), v, "POT0 on cycle {cycle}");
        assert!(!polling(&s.p, 0), "ALLPOT bit 0 on cycle {cycle}");
    }
    assert_eq!(allpot(&s.p), 0, "the second scan ended");
}

/// Verifies REQ-536: Criterion 4: With `SKCTL` = `$04` (initialization mode and fast pot scan) and
/// a `POTGO` write on cycle G, `ALLPOT` reads `$00` from cycle G+231 and `POT4` reads 229.
#[test]
fn fast_pot_ignores_init_mode() {
    let mut s = Scan::start(0x04, &[], 0);
    for offset in 1..=FAST_SCAN {
        let _cycle = s.tick();
        if offset < 231 {
            assert_ne!(allpot(&s.p), 0x00, "ALLPOT on G+{offset}");
        } else {
            assert_eq!(allpot(&s.p), 0x00, "ALLPOT on G+{offset}");
        }
    }
    assert_eq!(pot(&s.p, 4), 229, "POT4");
}

/// Verifies REQ-537: Criterion 1: With `set_pot(0, 150)`, a second `POTGO` write on cycle G + 64 x
/// 114 makes `POT0` read 86 +/- 1 after the second scan.
#[test]
fn truncated_pot_scan() {
    for phase in PHASES {
        let mut s = Scan::slow(&[(0, 150)], phase);
        s.run_to(s.g + 64 * LINE);
        s.potgo();
        let _end = s.run_to_end(SLOW_SCAN);
        let value = pot(&s.p, 0);
        assert!((85..=87).contains(&value), "POT0 = {value}, phase {phase}");
    }
}

/// Verifies REQ-537: Criterion 2: With `set_pot(0, 150)`, a write of `SKCTL` = `$00` on cycle G +
/// 20 x 114 and a write of `SKCTL` = `$03` on cycle G + 70 x 114 make `POT0` read 100 +/- 1 after
/// the scan.
#[test]
fn suspended_pot_scan() {
    for phase in PHASES {
        let mut s = Scan::slow(&[(0, 150)], phase);
        let g = s.g;
        s.run_to(g + 20 * LINE);
        s.p.write(SKCTL, 0x00);
        s.run_to(g + 70 * LINE);
        s.p.write(SKCTL, 0x03);
        let _end = s.run_to_end(SLOW_SCAN);
        let value = pot(&s.p, 0);
        assert!((99..=101).contains(&value), "POT0 = {value}, phase {phase}");
    }
}

/// Verifies REQ-537: Criterion 3: After the second `POTGO` write of criterion 1 on cycle G2,
/// `ALLPOT` reads `$00` first on cycle P2 + 227 x 114 + 2. P2 is the cycle of the first pot counter
/// increment after G2.
#[test]
fn restarted_scan_runs_full() {
    for phase in PHASES {
        let mut s = Scan::slow(&[(0, 150)], phase);
        s.run_to(s.g + 64 * LINE);
        s.potgo();
        assert_eq!(pot(&s.p, 4), 0, "POT4 restarts at 0");
        let p2 = s.run_to_first_increment();
        let end = s.run_to_end(SLOW_SCAN);
        assert_eq!(end, p2 + 227 * LINE + 2, "phase {phase}");
    }
}

/// Verifies REQ-537: Criterion 4: With `set_pot(0, 30)`, a second `POTGO` write on cycle G + 64 x
/// 114 keeps `POT0` at 30 and `ALLPOT` bit 0 at 0 on each cycle of the second scan. The test
/// restarts a scan after the input latched and checks `POT0` and `ALLPOT` bit 0.
#[test]
fn restarted_scan_keeps_latched_input() {
    let mut s = Scan::slow(&[(0, 30)], 9);
    s.run_to(s.g + 64 * LINE);
    assert_eq!(pot(&s.p, 0), 30, "POT0 before the second POTGO");
    s.potgo();
    assert!(polling(&s.p, 4), "the second scan runs");
    let mut ended = false;
    for _ in 0..SLOW_SCAN {
        let cycle = s.tick();
        assert_eq!(pot(&s.p, 0), 30, "POT0 on cycle {cycle}");
        assert!(!polling(&s.p, 0), "ALLPOT bit 0 on cycle {cycle}");
        if allpot(&s.p) == 0 {
            ended = true;
            break;
        }
    }
    assert!(ended, "the second scan ended");
}
