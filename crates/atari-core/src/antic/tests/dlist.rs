//! The display list and memory scan counters and display list DMA off (REQ-324 to
//! REQ-326).

use super::{Rec, Rig, codes, cycles_of, every, fetch_lines, on_line};
use crate::antic::DmaKind;
use crate::timing::{VBLANK_START_LINE, VideoStandard};

/// The addresses of the reads of `kind` in `recs`.
fn reads_of(recs: &[Rec], kind: DmaKind) -> Vec<u16> {
    recs.iter()
        .filter(|r| r.out.dma == kind)
        .filter_map(|r| r.read)
        .collect()
}

/// A rig with the display list at `addr` and `DMACTL` = `$22`. The next tick is (8, 0).
fn rig_at(addr: u16, dl: &[u8]) -> Rig {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    rig.start_dl_at(0x22, addr, dl);
    rig
}

/// Verifies REQ-324: an instruction `$02` at `$07FF` makes the next instruction fetch read
/// `$0400`.
#[test]
fn dlist_wraps_in_1k_block() {
    let mut rig = rig_at(0x07FF, &[0x02]);
    rig.poke(0x0400, &[0x41, 0x00, 0x10]);
    let recs = rig.run_to(20, 0);
    assert_eq!(
        reads_of(&recs, DmaKind::DisplayList)[..2],
        [0x07FF, 0x0400],
        "instruction fetches"
    );
}

/// Verifies REQ-324: an instruction `$42` at `$07FE` reads the low address byte from
/// `$07FF` and the high address byte from `$0400`, and the next instruction fetch reads
/// `$0401`.
#[test]
fn dlist_wraps_inside_lms() {
    let mut rig = rig_at(0x07FE, &[0x42, 0x00]);
    rig.poke(0x0400, &[0x20, 0x41, 0x00, 0x10]);
    let recs = rig.run_to(20, 0);
    assert_eq!(
        reads_of(&recs, DmaKind::DisplayListAddress)[..2],
        [0x07FF, 0x0400],
        "LMS address bytes"
    );
    assert_eq!(
        reads_of(&recs, DmaKind::DisplayList)[..2],
        [0x07FE, 0x0401],
        "instruction fetches"
    );
    assert_eq!(
        reads_of(&on_line(&recs, 8), DmaKind::CharacterName)[0],
        0x2000,
        "the LMS address"
    );
}

/// The display list of REQ-325: a mode E line with LMS `$2FF0`, a mode E line, and a mode
/// E line with LMS `$3FF0`. Returns the `Bitmap` fetch addresses of the three lines.
fn memscan_lines() -> [Vec<u16>; 3] {
    let mut rig = Rig::with_dl(
        0x22,
        &[0x4E, 0xF0, 0x2F, 0x0E, 0x4E, 0xF0, 0x3F, 0x41, 0x00, 0x10],
    );
    [rig.line(), rig.line(), rig.line()].map(|line| reads_of(&line, DmaKind::Bitmap))
}

/// Verifies REQ-325: with `DMACTL` = `$22`, a mode E line with LMS `$2FF0` reads `$2FFF` on
/// its 16th fetch and `$2000` on its 17th fetch, and the next mode E line without LMS
/// starts at `$2018`.
#[test]
fn memscan_wraps_in_4k_block() {
    let [first, second, _] = memscan_lines();
    assert_eq!(first.len(), 40, "40 fetches");
    assert_eq!(first[0], 0x2FF0, "1st fetch");
    assert_eq!(first[15], 0x2FFF, "16th fetch");
    assert_eq!(first[16], 0x2000, "17th fetch");
    assert_eq!(first[39], 0x2017, "40th fetch");
    assert_eq!(second[0], 0x2018, "first fetch of the next line");
}

/// Verifies REQ-325: an LMS of `$3FF0` after the wrapped lines makes the first fetch read
/// `$3FF0` (an LMS loads all 16 bits).
#[test]
fn lms_loads_16_bits() {
    let [_, _, third] = memscan_lines();
    assert_eq!(third[0], 0x3FF0, "first fetch");
    assert_eq!(third[16], 0x3000, "17th fetch wraps in the new 4K block");
}

/// A rig that fetched the instruction `dl[0]` on scan line 8 and then turned display list
/// DMA off (`DMACTL` = `$02`) on cycle 10. The next tick is (8, 11).
fn dl_off_rig(dl: &[u8]) -> Rig {
    let mut rig = Rig::with_dl(0x22, dl);
    rig.seek_after(8, 10);
    rig.write(0x00, 0x02);
    rig
}

/// Checks that `recs` have no display list DMA.
fn check_no_dl_dma(recs: &[Rec]) {
    for kind in [DmaKind::DisplayList, DmaKind::DisplayListAddress] {
        assert!(cycles_of(recs, kind).is_empty(), "no {kind:?}");
    }
}

/// Verifies REQ-326: with `DMACTL` = `$02` and IR = `$02`, the display list address does not
/// change for a full frame, and `out.dma` is never `DisplayList` or `DisplayListAddress`.
#[test]
fn dl_dma_off_keeps_address() {
    let mut rig = dl_off_rig(&[0x02, 0x41, 0x00, 0x10]);
    rig.seek(9, 0);
    assert_eq!(rig.antic.debug_state().ir, 0x02, "IR");
    let dlist = rig.antic.debug_state().dlist;
    let cycles = rig.antic.standard.cycles_per_frame();
    let recs: Vec<Rec> = (0..cycles)
        .map(|_| {
            let rec = rig.step();
            assert_eq!(rig.antic.debug_state().dlist, dlist, "display list address");
            rec
        })
        .collect();
    check_no_dl_dma(&recs);
}

/// Verifies REQ-326: after a write of `$02` to `DMACTL` in a `$42 $00 $40` line, the next
/// mode lines repeat mode 2 with 8 scan lines, have no `DisplayListAddress` fetch, and each
/// first name fetch is `$28` higher than the one before.
#[test]
fn dl_dma_off_repeats_ir() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x40, 0x41, 0x00, 0x10]);
    let first = rig.line();
    rig.seek_after(9, 50);
    rig.write(0x00, 0x02);
    let recs = rig.run_to(VBLANK_START_LINE, 0);
    check_no_dl_dma(&recs);
    let mut names = vec![reads_of(&first, DmaKind::CharacterName)[0]];
    for line in 10..VBLANK_START_LINE {
        let line_recs = on_line(&recs, line);
        let line_names = reads_of(&line_recs, DmaKind::CharacterName);
        if line % 8 == 0 {
            assert_eq!(line_names.len(), 40, "names on scan line {line}");
            names.push(line_names[0]);
        } else {
            assert!(line_names.is_empty(), "no names on scan line {line}");
        }
    }
    assert_eq!(names.len(), 30, "30 mode lines");
    for (k, pair) in names.windows(2).enumerate() {
        assert_eq!(pair[1], pair[0] + 0x28, "mode line {}", k + 1);
    }
    assert_eq!(names[0], 0x4000, "the LMS address");
}

/// Verifies REQ-326: with `DMACTL` = `$02`, IR = `$82` and `NMIEN` = `$80`, `NMIST` bit 7 is
/// set and one NMI occurs on the 8th scan line of each repeated mode 2 line only.
#[test]
fn dl_dma_off_repeated_dli() {
    let mut rig = dl_off_rig(&[0x82, 0x41, 0x00, 0x10]);
    rig.write(0x0E, 0x80);
    rig.seek(9, 0);
    let mut nmi_lines = Vec::new();
    for line in 9..VBLANK_START_LINE {
        rig.write(0x0F, 0x00);
        let recs = rig.line();
        let bit7 = rig.read(0x0F) & 0x80 != 0;
        assert_eq!(bit7, line % 8 == 7, "NMIST bit 7 after scan line {line}");
        nmi_lines.extend(recs.iter().filter(|r| r.out.nmi).map(|r| r.out.line));
    }
    assert_eq!(nmi_lines, every(15, 247, 8), "the NMI scan lines");
}

/// Verifies REQ-326: after a JVB on scan line N and a write of `$02` to `DMACTL`, positions
/// `$22-$DD` have code `%000` on lines N + 1 to 247 and on lines 8-247 of the next frame,
/// with no display list DMA.
#[test]
fn dl_dma_off_after_jvb_is_blank() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x20, 0x41, 0x00, 0x10]);
    rig.fill(0x2000, 48, 0x01);
    rig.fill(0xE000, 0x400, 0xFF);
    let recs = rig.run_to(17, 0);
    assert_eq!(fetch_lines(&recs), [8, 16], "the JVB is on scan line 16");
    rig.write(0x00, 0x02);
    let recs = rig.run_to(VBLANK_START_LINE, 0);
    let next = rig.frame();
    check_no_dl_dma(&recs);
    check_no_dl_dma(&next);
    for (all, lines) in [
        (&recs, 17..VBLANK_START_LINE),
        (&next, 8..VBLANK_START_LINE),
    ] {
        for line in lines {
            let line_codes = codes(&on_line(all, line));
            assert!(
                line_codes[0x22..=0xDD].iter().all(|&c| c == 0),
                "scan line {line}"
            );
        }
    }
}

/// A rig with IR = `$02` and `DMACTL` = `$02` on scan line 15, the last scan line of the
/// mode line. Writes `$22` to `DMACTL` on `cycle` of scan line `line` and returns the DMA
/// kind of cycle 1 of scan line 16.
fn dl_enable(line: u32, cycle: u32) -> DmaKind {
    let mut rig = dl_off_rig(&[0x02, 0x02, 0x41, 0x00, 0x10]);
    rig.seek_after(line, cycle);
    rig.write(0x00, 0x22);
    let recs = rig.run_to(17, 0);
    on_line(&recs, 16)[1].out.dma
}

/// Verifies REQ-326: a write of `$22` to `DMACTL` on cycle 113 of scan line N makes a
/// `DisplayList` fetch occur on cycle 1 of scan line N + 1.
#[test]
fn dl_dma_enable_on_113_works() {
    assert_eq!(dl_enable(15, 113), DmaKind::DisplayList);
}

/// Verifies REQ-326: a write of `$22` to `DMACTL` on cycle 0 of scan line N + 1 does not
/// make a `DisplayList` fetch occur on cycle 1 of that scan line.
#[test]
fn dl_dma_enable_on_0_too_late() {
    assert_ne!(dl_enable(16, 0), DmaKind::DisplayList);
}
