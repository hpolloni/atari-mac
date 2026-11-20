//! The NMI timing and the `WSYNC` deadline (REQ-334, REQ-337).

use super::Rig;
use crate::timing::VideoStandard;

/// A display list with a one-line DLI on scan line 40 and a JVB, with `NMIEN` = `nmien`.
/// The next tick is (39, 51), and `NMIRES` was written on scan line 39.
fn dli_40_rig(nmien: u8) -> Rig {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0x70, 0x70, 0x70, 0x80, 0x41, 0x00, 0x10]);
    rig.write(0x0E, nmien);
    rig.seek_after(39, 50);
    rig.write(0x0F, 0);
    rig
}

/// One cycle of scan line 40, after the register writes of the cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineCycle {
    /// The cycle.
    cycle: u32,
    /// `NMIST`.
    nmist: u8,
    /// `nmi_level()`.
    level: bool,
    /// `out.nmi`.
    out_nmi: bool,
}

/// Runs cycles 0-20 of scan line 40. After the tick of cycle `c`, it makes the writes
/// `(c, reg, value)` of `writes` (the writes "on cycle c"), then records the cycle.
fn line_40(rig: &mut Rig, writes: &[(u32, u8, u8)]) -> Vec<LineCycle> {
    rig.seek(40, 0);
    (0..=20)
        .map(|cycle| {
            let rec = rig.step();
            assert_eq!((rec.out.line, rec.out.cycle), (40, cycle), "position");
            for &(_, reg, value) in writes.iter().filter(|w| w.0 == cycle) {
                rig.write(reg, value);
            }
            LineCycle {
                cycle,
                nmist: rig.read(0x0F),
                level: rig.antic.nmi_level(),
                out_nmi: rec.out.nmi,
            }
        })
        .collect()
}

/// The cycles with the NMI level high.
fn high(cycles: &[LineCycle]) -> Vec<u32> {
    cycles.iter().filter(|c| c.level).map(|c| c.cycle).collect()
}

/// The cycles with `out.nmi`.
fn out_nmi(cycles: &[LineCycle]) -> Vec<u32> {
    cycles
        .iter()
        .filter(|c| c.out_nmi)
        .map(|c| c.cycle)
        .collect()
}

/// Verifies REQ-334 (criterion 1): on a DLI scan line, after a write to `NMIRES` on the line
/// before, a read of `NMIST` on cycle 6 gives bit 7 = 0, and on cycle 7 bit 7 = 1. With
/// `NMIEN` = `$C0`, the NMI level is high on cycles 8 and 9.
#[test]
fn nmist_visible_from_cycle_7() {
    let mut rig = dli_40_rig(0xC0);
    let cycles = line_40(&mut rig, &[]);
    assert_eq!(cycles[6].nmist & 0x80, 0, "cycle 6");
    assert_eq!(cycles[7].nmist & 0x80, 0x80, "cycle 7");
    assert_eq!(high(&cycles), [8, 9], "NMI level");
    assert_eq!(out_nmi(&cycles), [9], "out.nmi after the rising edge");
}

/// Verifies REQ-334 (criterion 3): a write of `$C0` to `NMIEN` on cycle 7 (from `$00`) still
/// gives the NMI of that scan line, one cycle later: the level is high on cycle 9 only, so
/// the earliest NMI sequence starts on cycle 11.
#[test]
fn nmien_enable_on_7_delays_nmi() {
    let mut rig = dli_40_rig(0x00);
    let cycles = line_40(&mut rig, &[(7, 0x0E, 0xC0)]);
    assert_eq!(high(&cycles), [9], "NMI level");
    assert_eq!(out_nmi(&cycles), [10], "out.nmi");
}

/// Verifies REQ-334 (criterion 4): a write of `$C0` to `NMIEN` on cycle 8 (from `$00`) gives
/// no NMI on that scan line, and `NMIST` bit 7 still becomes set.
#[test]
fn nmien_enable_on_8_too_late() {
    let mut rig = dli_40_rig(0x00);
    let cycles = line_40(&mut rig, &[(8, 0x0E, 0xC0)]);
    assert!(high(&cycles).is_empty(), "no NMI level");
    assert!(out_nmi(&cycles).is_empty(), "no out.nmi");
    assert_eq!(cycles[20].nmist & 0x80, 0x80, "NMIST bit 7");
}

/// Verifies REQ-334 (criterion 5): with `NMIEN` = `$C0`, a write of `$00` on cycle 8 prevents
/// the NMI of that scan line, and a write of `$00` on cycle 9 does not.
#[test]
fn nmien_disable_on_8_prevents() {
    let mut on_8 = dli_40_rig(0xC0);
    let cycles = line_40(&mut on_8, &[(8, 0x0E, 0x00)]);
    assert!(high(&cycles).is_empty(), "write on 8: no NMI level");
    assert!(out_nmi(&cycles).is_empty(), "write on 8: no out.nmi");
    let mut on_9 = dli_40_rig(0xC0);
    let cycles = line_40(&mut on_9, &[(9, 0x0E, 0x00)]);
    assert_eq!(high(&cycles), [8], "write on 9: NMI level");
    assert_eq!(out_nmi(&cycles), [9], "write on 9: out.nmi");
}

/// Verifies REQ-334 (criterion 6): a write to `NMIRES` on cycle 8 does not prevent the NMI
/// of that scan line.
#[test]
fn nmires_does_not_cancel() {
    let mut rig = dli_40_rig(0xC0);
    let cycles = line_40(&mut rig, &[(8, 0x0F, 0x00)]);
    assert_eq!(cycles[8].nmist & 0xC0, 0, "NMIST cleared");
    assert_eq!(high(&cycles), [8, 9], "NMI level");
    assert_eq!(out_nmi(&cycles), [9], "out.nmi");
}

/// The `out.rdy` values of the cycles from (100, 0) to (101, 113), after `WSYNC` writes on
/// the cycles `writes` of scan line 100.
fn rdy_after_writes(writes: &[u32]) -> Vec<((u32, u32), bool)> {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    rig.seek(100, 0);
    (0..2 * 114)
        .map(|_| {
            let rec = rig.step();
            let pos = (rec.out.line, rec.out.cycle);
            if pos.0 == 100 && writes.contains(&pos.1) {
                rig.write(0x0A, 0);
            }
            (pos, rec.out.rdy)
        })
        .collect()
}

/// The positions with RDY low.
fn rdy_low(cycles: &[((u32, u32), bool)]) -> Vec<(u32, u32)> {
    cycles.iter().filter(|c| !c.1).map(|c| c.0).collect()
}

/// The positions from (`line`, `first`) to (`line`, `last`).
fn span(line: u32, first: u32, last: u32) -> Vec<(u32, u32)> {
    (first..=last).map(|c| (line, c)).collect()
}

/// Verifies REQ-337: a `WSYNC` write on cycle 104 of scan line N keeps RDY low from cycle 106
/// of scan line N to cycle 104 of scan line N + 1; RDY is high again on cycle 105 of scan
/// line N + 1 (supports criterion 1). A write on cycle 113 gives RDY low from cycle 1 of
/// scan line N + 1 (supports criterion 4).
#[test]
fn wsync_write_on_104_waits_next_line() {
    let mut expected = span(100, 106, 113);
    expected.extend(span(101, 0, 104));
    assert_eq!(rdy_low(&rdy_after_writes(&[104])), expected, "write on 104");
    assert_eq!(
        rdy_low(&rdy_after_writes(&[113])),
        span(101, 1, 104),
        "write on 113"
    );
}

/// Verifies REQ-337: two `WSYNC` writes on consecutive cycles (as in `INC WSYNC`) give one
/// wait (supports criteria 2 and 3). Writes on cycles 102 and 103 give RDY low on cycle 104
/// only: the second wait is empty. Writes on cycles 103 and 104 give RDY low from cycle 106
/// to cycle 104 of the next scan line: the first wait is empty, the second one waits for the
/// next scan line.
#[test]
fn wsync_windows_join() {
    assert_eq!(
        rdy_low(&rdy_after_writes(&[102, 103])),
        [(100, 104)],
        "writes on 102 and 103"
    );
    assert_eq!(
        rdy_after_writes(&[103, 104]),
        rdy_after_writes(&[104]),
        "writes on 103 and 104"
    );
    // Each write starts a new wait: a write on cycle 104 during the wait of a write on
    // cycle 50 waits for the next scan line.
    let mut expected = span(100, 52, 104);
    expected.extend(span(100, 106, 113));
    expected.extend(span(101, 0, 104));
    assert_eq!(
        rdy_low(&rdy_after_writes(&[50, 104])),
        expected,
        "writes on 50 and 104"
    );
}
