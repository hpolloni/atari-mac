//! Horizontal and vertical scrolling and mode lines with unusual height (REQ-327 to
//! REQ-330).

use super::{FONT, Rec, Rig, SCREEN, codes, cycles_of, every, fetch_lines, on_line};
use crate::antic::DmaKind;
use crate::timing::VBLANK_START_LINE;

/// A different byte for each index `k`.
fn byte(k: u8) -> u8 {
    k.wrapping_mul(37).wrapping_add(11)
}

/// A rig with the display list `dl`, `DMACTL` = `dmactl`, `HSCROL` = `hscrol` and the
/// screen bytes `byte(k)` at `SCREEN + k`.
fn hscrol_rig(dmactl: u8, hscrol: u8, dl: &[u8]) -> Rig {
    let mut rig = Rig::with_dl(dmactl, dl);
    rig.write(0x04, hscrol);
    let screen: Vec<u8> = (0..=255).map(byte).collect();
    rig.poke(SCREEN, &screen);
    rig
}

/// Like `hscrol_rig` with a mode 2 line: name k at `SCREEN + k`, and the row 0 font byte
/// `byte(k)` for name k.
fn names_rig(dmactl: u8, hscrol: u8, dl: &[u8]) -> Rig {
    let mut rig = Rig::with_dl(dmactl, dl);
    rig.write(0x04, hscrol);
    let names: Vec<u8> = (0..48).collect();
    rig.poke(SCREEN, &names);
    for k in 0..48 {
        rig.poke(FONT + u16::from(k) * 8, &[byte(k)]);
    }
    rig
}

/// The 4 hi-res codes of a byte.
fn hires(value: u8) -> [u8; 4] {
    [6, 4, 2, 0].map(|shift| 0b100 | ((value >> shift) & 3))
}

/// The row counter on cycle 50 of each of the next `count` scan lines (from cycle 0).
fn rows(rig: &mut Rig, count: usize) -> Vec<u8> {
    (0..count)
        .map(|_| {
            rig.skip(51);
            let row = rig.antic.debug_state().row;
            rig.skip(63);
            row
        })
        .collect()
}

/// The display list `dl` (with `VSCROL` = `vscrol`) runs to scan line 40. Returns the
/// records.
fn vscrol_run(vscrol: u8, dl: &[u8]) -> Vec<Rec> {
    let mut rig = Rig::with_dl(0x22, dl);
    rig.write(0x05, vscrol);
    rig.run_to(40, 0)
}

/// Verifies REQ-327: with `HSCROL` = 0 and IR = `$1E`, `Bitmap` fetches occur on cycles
/// 20-98 (`DMACTL` = `$21`) and on cycles 12-106 (`DMACTL` = `$22` and `$23`), every second
/// cycle. The fetch on cycle 106 is virtual (REQ-311): it has no DMA cycle, and it
/// increments the memory scan counter.
#[test]
fn hscrol_widens_fetch() {
    for (dmactl, real, count) in [
        (0x21, every(20, 98, 2), 40),
        (0x22, every(12, 104, 2), 48),
        (0x23, every(12, 104, 2), 48),
    ] {
        let mut rig = hscrol_rig(dmactl, 0, &[0x5E, 0x00, 0x20, 0x41, 0x00, 0x10]);
        let line = rig.line();
        assert_eq!(
            cycles_of(&line, DmaKind::Bitmap),
            real,
            "DMACTL ${dmactl:02X} bitmap cycles"
        );
        assert_eq!(
            rig.antic.debug_state().memscan,
            SCREEN + count,
            "DMACTL ${dmactl:02X} fetches"
        );
    }
}

/// The codes of the first scan line of a `$52` line (LMS `SCREEN`) with `HSCROL` =
/// `hscrol`, and the memory scan counter after the mode line.
fn scrolled_mode_2(hscrol: u8) -> ([u8; 228], u16) {
    let mut rig = names_rig(0x22, hscrol, &[0x52, 0x00, 0x20, 0x41, 0x00, 0x10]);
    let first = codes(&rig.line());
    rig.seek(16, 0);
    (first, rig.antic.debug_state().memscan)
}

/// Verifies REQ-327: with `HSCROL` = 0, a `$52` line with LMS address A shows the name at
/// A + 4 at `$30-$33`, and the memory scan counter is A + 48 after the mode line.
#[test]
fn hscrol_0_windows_wider_playfield() {
    let (line, memscan) = scrolled_mode_2(0);
    assert_eq!(line[0x30..0x34], hires(byte(4)), "the name at A + 4");
    assert_eq!(memscan, SCREEN + 48, "memory scan counter");
}

/// Verifies REQ-327: with `HSCROL` = 4, the same `$52` line shows the name at A + 3 at
/// `$30-$33`.
#[test]
fn hscrol_4_shifts_one_char() {
    let (line, _) = scrolled_mode_2(4);
    assert_eq!(line[0x30..0x34], hires(byte(3)), "the name at A + 3");
}

/// Verifies REQ-327: with `DMACTL` = `$22` and `HSCROL` = 6, the first `CharacterName`
/// fetch of a `$12` line is on cycle 13.
#[test]
fn hscrol_delays_fetch() {
    let mut rig = names_rig(0x22, 6, &[0x12, 0x41, 0x00, 0x10]);
    let line = rig.line();
    assert_eq!(
        cycles_of(&line, DmaKind::CharacterName).first(),
        Some(&13),
        "first name fetch"
    );
}

/// Verifies REQ-327: `HSCROL` = 5 and 4 give the same `out.dma` on a `$12` line, and the
/// code at x (`$31-$CF`) with 5 is the code at x - 1 with 4.
#[test]
fn hscrol_odd_adds_one_color_clock() {
    let run = |hscrol| {
        let mut rig = names_rig(0x22, hscrol, &[0x52, 0x00, 0x20, 0x41, 0x00, 0x10]);
        rig.line()
    };
    let (four, five) = (run(4), run(5));
    let dma = |line: &[Rec]| line.iter().map(|r| r.out.dma).collect::<Vec<_>>();
    assert_eq!(dma(&five), dma(&four), "the DMA cycles");
    let (four, five) = (codes(&four), codes(&five));
    for x in 0x31..=0xCF {
        assert_eq!(five[x], four[x - 1], "position ${x:02X}");
    }
}

/// Verifies REQ-327: for each `HSCROL` 0-15, a `$1F` line with bytes `$FF` has code `%111`
/// on exactly `$30-$CF` (`DMACTL` = `$22`) or `$40-$BF` (`DMACTL` = `$21`), and `%000` on
/// the other positions of `$22-$DD`.
#[test]
fn hscrol_window_width_unchanged() {
    for (dmactl, first, last) in [(0x22, 0x30, 0xCF), (0x21, 0x40, 0xBF)] {
        for hscrol in 0..16 {
            let mut rig = Rig::with_dl(dmactl, &[0x5F, 0x00, 0x20, 0x41, 0x00, 0x10]);
            rig.write(0x04, hscrol);
            rig.fill(SCREEN, 48, 0xFF);
            let line = codes(&rig.line());
            for (x, &code) in line.iter().enumerate().take(0xDE).skip(0x22) {
                let expected = if (first..=last).contains(&x) {
                    0b111
                } else {
                    0
                };
                assert_eq!(
                    code, expected,
                    "DMACTL ${dmactl:02X} HSCROL {hscrol} position ${x:02X}"
                );
            }
        }
    }
}

/// Verifies REQ-327: a write of `$02` to `HSCROL` on cycle 110 of the first scan line of a
/// `$1A` line moves the second scan line: the code at x (`$32-$CF`) on the second scan line
/// is the code at x - 2 on the first.
#[test]
fn hscrol_change_between_rows() {
    let mut rig = hscrol_rig(0x22, 0, &[0x5A, 0x00, 0x20, 0x41, 0x00, 0x10]);
    let mut first = rig.after(8, 110);
    rig.write(0x04, 0x02);
    first.extend(rig.run_to(9, 0));
    let second = rig.line();
    assert!(
        cycles_of(&second, DmaKind::Bitmap).is_empty(),
        "the second scan line uses the line buffer"
    );
    let (first, second) = (codes(&first), codes(&second));
    for x in 0x32..=0xCF {
        assert_eq!(second[x], first[x - 2], "position ${x:02X}");
    }
}

/// Verifies REQ-328: with `VSCROL` = 3, the instructions `$22`, `$22`, `$02` start on lines
/// N, N + 5 and N + 13, the next fetch is on line N + 17, and the second `$22` line has
/// rows 0-7.
#[test]
fn vscroll_region_17_lines() {
    let dl = [0x62, 0x00, 0x20, 0x22, 0x02, 0x41, 0x00, 0x10];
    let recs = vscrol_run(3, &dl);
    assert_eq!(fetch_lines(&recs)[..4], [8, 13, 21, 25], "fetch lines");
    let mut rig = Rig::with_dl(0x22, &dl);
    rig.write(0x05, 3);
    let all = rows(&mut rig, 17);
    assert_eq!(
        all[..5],
        [3, 4, 5, 6, 7],
        "the first line starts the region"
    );
    assert_eq!(all[5..13], [0, 1, 2, 3, 4, 5, 6, 7], "the second $22 line");
    assert_eq!(all[13..], [0, 1, 2, 3], "the $02 line ends the region");
}

/// Verifies REQ-328: a write of `$F3` to `VSCROL` gives the same result as `$03`.
#[test]
fn vscrol_high_bits_ignored() {
    let dl = [0x62, 0x00, 0x20, 0x22, 0x02, 0x41, 0x00, 0x10];
    let low = vscrol_run(0x03, &dl);
    let high = vscrol_run(0xF3, &dl);
    assert!(low == high, "the records of scan lines 8-39 are equal");
}

/// Verifies REQ-328: with `VSCROL` = 13, a `$2F` line after a line with bit 5 clear has rows
/// 13, 14, 15 and 0, `Bitmap` DMA only on its first scan line, and the next fetch 4 scan
/// lines later.
#[test]
fn vscroll_4bit_row_wrap_mode_f() {
    let dl = [0x6F, 0x00, 0x20, 0x41, 0x00, 0x10];
    let recs = vscrol_run(13, &dl);
    assert_eq!(fetch_lines(&recs)[..2], [8, 12], "fetch lines");
    for line in 8..12 {
        let bitmap = cycles_of(&on_line(&recs, line), DmaKind::Bitmap);
        assert_eq!(
            bitmap.is_empty(),
            line != 8,
            "bitmap DMA on scan line {line}"
        );
    }
    let mut rig = Rig::with_dl(0x22, &dl);
    rig.write(0x05, 13);
    assert_eq!(rows(&mut rig, 4), [13, 14, 15, 0], "rows");
}

/// Verifies REQ-328: a `$70` line after a `$22` line (`VSCROL` = 2) has 3 scan lines, after
/// a `$02` line (`VSCROL` = 4) 8 scan lines, and a jump after a `$22` line (`VSCROL` = 2) 3
/// scan lines.
#[test]
fn vscroll_region_ends_on_blank_line() {
    let blank_after_22 = vscrol_run(2, &[0x62, 0x00, 0x20, 0x70, 0x41, 0x00, 0x10]);
    assert_eq!(
        fetch_lines(&blank_after_22)[..3],
        [8, 14, 17],
        "$70 after $22"
    );
    let blank_after_02 = vscrol_run(4, &[0x42, 0x00, 0x20, 0x70, 0x41, 0x00, 0x10]);
    assert_eq!(
        fetch_lines(&blank_after_02)[..3],
        [8, 16, 24],
        "$70 after $02"
    );
    let jump_after_22 = jump_run();
    assert_eq!(
        fetch_lines(&jump_after_22)[..3],
        [8, 14, 17],
        "$01 after $22"
    );
}

/// Verifies REQ-328: with `DMACTL` = `$02`, IR = `$22` and `VSCROL` = 4, the mode 2 line on
/// scan line 8 has 8 scan lines (bit 5 of the line before vertical blank is kept).
#[test]
fn vscroll_bit_kept_across_vblank() {
    let mut rig = Rig::with_dl(0x22, &[0x22, 0x41, 0x00, 0x10]);
    rig.write(0x05, 4);
    rig.seek_after(8, 10);
    rig.write(0x00, 0x02);
    rig.seek(VBLANK_START_LINE, 0);
    rig.seek(8, 0);
    assert_eq!(rig.antic.debug_state().ir, 0x22, "IR");
    let recs = rig.clone().run_to(17, 0);
    let names: Vec<u32> = (8..17)
        .filter(|&line| !cycles_of(&on_line(&recs, line), DmaKind::CharacterName).is_empty())
        .collect();
    assert_eq!(
        names,
        [8, 16],
        "the mode line on scan line 8 has 8 scan lines"
    );
    assert_eq!(rows(&mut rig, 9), [0, 1, 2, 3, 4, 5, 6, 7, 0], "rows");
}

/// Runs the display list `dl` with `VSCROL` = `vscrol` and `NMIEN` = `nmien`, writes
/// `value` to `VSCROL` on cycle `cycle` of scan line `line`, and runs to scan line 40.
/// Returns the records from scan line 8.
fn vscrol_write_run(
    vscrol: u8,
    nmien: u8,
    dl: &[u8],
    (line, cycle): (u32, u32),
    value: u8,
) -> Vec<Rec> {
    let mut rig = Rig::with_dl(0x22, dl);
    rig.write(0x05, vscrol);
    rig.write(0x0E, nmien);
    let mut recs = rig.after(line, cycle);
    rig.write(0x05, value);
    recs.extend(rig.run_to(40, 0));
    recs
}

/// The scan lines with `out.nmi` in `recs`.
fn nmi_lines(recs: &[Rec]) -> Vec<u32> {
    recs.iter()
        .filter(|rec| rec.out.nmi)
        .map(|rec| rec.out.line)
        .collect()
}

/// Verifies REQ-329: a write of `$05` to `VSCROL` on cycle 0 of the first line of a `$22`
/// line gives rows 5-7; the same write on cycle 1 gives rows 0-7.
#[test]
fn vscrol_start_deadline_cycle_0() {
    // A `$02` line on scan lines 8-15, the `$22` line from N = 16, then a `$02` line.
    let dl = [0x42, 0x00, 0x20, 0x22, 0x02, 0x41, 0x00, 0x10];
    let on_0 = vscrol_write_run(0, 0, &dl, (16, 0), 5);
    assert_eq!(
        fetch_lines(&on_0)[..3],
        [8, 16, 19],
        "write on cycle 0: 3 scan lines"
    );
    let on_1 = vscrol_write_run(0, 0, &dl, (16, 1), 5);
    assert_eq!(
        fetch_lines(&on_1)[..3],
        [8, 16, 24],
        "write on cycle 1: 8 scan lines"
    );
}

/// Verifies REQ-329: a write of `$03` to `VSCROL` on cycle 108 of line N + 3 of a `$02` line
/// (`VSCROL` = 7) makes it the last scan line; the same write on cycle 109 does not.
#[test]
fn vscrol_end_deadline_cycle_108() {
    // A `$42` line on 8-15, a `$22` line (row 7 only) on 16, and the `$02` line from N = 17.
    let dl = [0x42, 0x00, 0x20, 0x22, 0x02, 0x41, 0x00, 0x10];
    let on_108 = vscrol_write_run(7, 0, &dl, (20, 108), 3);
    assert_eq!(
        fetch_lines(&on_108)[..4],
        [8, 16, 17, 21],
        "write on cycle 108: N + 3 is the last scan line"
    );
    let on_109 = vscrol_write_run(7, 0, &dl, (20, 109), 3);
    let lines = fetch_lines(&on_109);
    assert_eq!(lines[..3], [8, 16, 17], "the first fetches");
    assert!(
        lines.get(3).is_none_or(|&line| line != 21),
        "write on cycle 109: N + 3 is not the last scan line: {lines:?}"
    );
}

/// Verifies REQ-329: in a `$82` line with `VSCROL` = 7, a write of `$03` on cycle 5 of line
/// N + 3 gives a DLI there and a write on cycle 6 does not; with `VSCROL` = 3, a write of
/// `$07` on cycle 5 prevents the DLI.
#[test]
fn vscrol_dli_deadline_cycle_5() {
    // A `$42` line on 8-15, a `$22` line on 16, and the `$82` line from N = 17.
    let dl = [0x42, 0x00, 0x20, 0x22, 0x82, 0x41, 0x00, 0x10];
    let mut rig = Rig::with_dl(0x22, &dl);
    rig.write(0x05, 7);
    rig.write(0x0E, 0x80);
    rig.seek_after(20, 5);
    rig.write(0x05, 3);
    rig.seek_after(20, 7);
    assert_eq!(rig.read(0x0F) & 0x80, 0x80, "write on cycle 5: NMIST bit 7");
    let on_5 = vscrol_write_run(7, 0x80, &dl, (20, 5), 3);
    assert_eq!(nmi_lines(&on_5), [20], "write on cycle 5: DLI on N + 3");
    assert_eq!(
        fetch_lines(&on_5)[..4],
        [8, 16, 17, 21],
        "write on cycle 5: N + 3 is the last scan line"
    );
    let on_6 = vscrol_write_run(7, 0x80, &dl, (20, 6), 3);
    assert!(
        !nmi_lines(&on_6).contains(&20),
        "write on cycle 6: no DLI on N + 3"
    );
    assert_eq!(
        fetch_lines(&on_6)[..4],
        [8, 16, 17, 21],
        "write on cycle 6: N + 3 is still the last scan line"
    );
    // With `VSCROL` = 3, the `$22` line has 5 scan lines (16-20), so N = 21.
    let control = vscrol_write_run(3, 0x80, &dl, (24, 50), 3);
    assert_eq!(
        nmi_lines(&control)[..1],
        [24],
        "VSCROL = 3 without a change: DLI on N + 3"
    );
    let prevent = vscrol_write_run(3, 0x80, &dl, (24, 5), 7);
    assert!(
        !nmi_lines(&prevent).contains(&24),
        "VSCROL = 3, write of $07 on cycle 5: no DLI on N + 3"
    );
}

/// The display list of REQ-330 criterion 1 (`VSCROL` = 2): a `$22` line, then a jump
/// `$01 $00 $30` at D = `$1003`. `$3000` holds `$00 $31`, `$3100` holds `$00 $32`, and
/// `$3200` holds a JVB.
fn jump_run() -> Vec<Rec> {
    let mut rig = Rig::with_dl(0x22, &[0x62, 0x00, 0x20, 0x01, 0x00, 0x30]);
    rig.poke(0x3000, &[0x00, 0x31]);
    rig.poke(0x3100, &[0x00, 0x32]);
    rig.poke(0x3200, &[0x41, 0x00, 0x10]);
    rig.write(0x05, 2);
    rig.run_to(40, 0)
}

/// Verifies REQ-330: with `VSCROL` = 2, a jump after a `$22` line has 3 scan lines and
/// fetches its address again on each, reading D + 1/D + 2, `$3000/$3001` and `$3100/$3101`,
/// and the next fetch reads `$3200`.
#[test]
fn extended_jump_refetches_each_line() {
    let recs = jump_run();
    assert_eq!(
        fetch_lines(&recs)[..3],
        [8, 14, 17],
        "the jump has 3 scan lines"
    );
    for (line, reads) in [
        (14, [0x1004, 0x1005]),
        (15, [0x3000, 0x3001]),
        (16, [0x3100, 0x3101]),
    ] {
        let line_recs = on_line(&recs, line);
        let address: Vec<(u32, Option<u16>)> = line_recs
            .iter()
            .filter(|r| r.out.dma == DmaKind::DisplayListAddress)
            .map(|r| (r.out.cycle, r.read))
            .collect();
        assert_eq!(
            address,
            [(6, Some(reads[0])), (7, Some(reads[1]))],
            "scan line {line}"
        );
    }
    assert_eq!(on_line(&recs, 17)[1].read, Some(0x3200), "the next fetch");
}

/// The font row of the first `CharacterData` fetch of each scan line, for the name `name`
/// of a mode 2-5 line.
fn font_rows(lines: &[Vec<Rec>], name: u8) -> Vec<Option<u16>> {
    let base = FONT + u16::from(name & 0x7F) * 8;
    lines
        .iter()
        .map(|line| {
            line.iter()
                .find(|r| r.out.dma == DmaKind::CharacterData)
                .and_then(|r| r.read)
                .map(|addr| addr - base)
        })
        .collect()
}

/// The display list `$60 | mode` (`$62` for mode 0), LMS `SCREEN`, then `mode` and a JVB,
/// with `VSCROL` = `vscrol` and all names `name`. Returns the rig at the start of the
/// `mode` line, which starts on scan line `start`.
fn extended_rig(mode: u8, vscrol: u8, name: u8, start: u32) -> Rig {
    let first = 0x60 | mode.max(2);
    let mut rig = Rig::with_dl(0x22, &[first, 0x00, 0x20, mode, 0x41, 0x00, 0x10]);
    rig.write(0x05, vscrol);
    rig.fill(SCREEN, 0x200, name);
    rig.fill(FONT + u16::from(name & 0x7F) * 8, 8, 0xFF);
    let recs = rig.run_to(start, 0);
    assert_eq!(fetch_lines(&recs), [8], "the first line");
    rig
}

/// Verifies REQ-330: with `VSCROL` = 15, a `$04` line after a `$24` line (and a `$06` line
/// after a `$26` line) has rows 0-15, and rows 8-15 read font rows 0-7.
#[test]
fn extended_mode_4_repeats_rows() {
    for mode in [4, 6] {
        let mut rig = extended_rig(mode, 15, 0x01, 17);
        let lines = rig.lines(17);
        let expected: Vec<Option<u16>> = (0..16).map(|r| Some(r & 7)).collect();
        assert_eq!(font_rows(&lines[..16], 0x01), expected, "mode {mode}");
        assert_eq!(
            fetch_lines(&lines.concat()),
            [17, 33],
            "mode {mode}: 16 scan lines"
        );
    }
}

/// Verifies REQ-330: with `VSCROL` = 15, a `$02` line after a `$22` line has rows 0-15,
/// rows 8 and 9 are `%100` for name `$41`, rows 10-15 read font rows 2-7; the same for a
/// `$03` line after a `$23` line.
#[test]
fn extended_mode_2_blank_rows_8_9() {
    for (mode, start) in [(2, 17), (3, 19)] {
        let mut rig = extended_rig(mode, 15, 0x41, start);
        let lines = rig.lines(17);
        assert_eq!(
            fetch_lines(&lines.concat()),
            [start, start + 16],
            "mode {mode}: 16 scan lines"
        );
        let reads = font_rows(&lines[..16], 0x41);
        for (r, line) in lines.iter().take(16).enumerate() {
            let line_codes = codes(line);
            if (8..10).contains(&r) {
                assert!(
                    line_codes[0x30..=0xCF].iter().all(|&c| c == 0b100),
                    "mode {mode} row {r}: font data $00"
                );
            } else {
                let frow = u16::try_from(r % 8).unwrap();
                assert_eq!(reads[r], Some(frow), "mode {mode} row {r} font row");
                assert!(
                    line_codes[0x30..=0xCF].iter().all(|&c| c == 0b111),
                    "mode {mode} row {r}: font data $FF"
                );
            }
        }
    }
}

/// Verifies REQ-330: with `VSCROL` = 3, a `$0F` line after a `$2F` line has 4 scan lines,
/// `Bitmap` DMA only on the first, and scan lines 2-4 have the codes of scan line 1.
#[test]
fn extended_mode_f_repeats_data() {
    let mut rig = Rig::with_dl(0x22, &[0x6F, 0x00, 0x20, 0x0F, 0x41, 0x00, 0x10]);
    rig.write(0x05, 3);
    let screen: Vec<u8> = (0..=255).map(byte).collect();
    rig.poke(SCREEN, &screen);
    let recs = rig.run_to(22, 0);
    assert_eq!(fetch_lines(&recs), [8], "the $2F line has rows 3-15 and 0");
    let lines = rig.lines(5);
    assert_eq!(fetch_lines(&lines.concat()), [22, 26], "4 scan lines");
    let first = codes(&lines[0]);
    assert!(first[0x30..=0xCF].iter().any(|&c| c != 0b100), "data shows");
    for (k, line) in lines.iter().enumerate().take(4) {
        let bitmap = cycles_of(line, DmaKind::Bitmap);
        assert_eq!(
            bitmap.is_empty(),
            k != 0,
            "bitmap DMA on scan line {}",
            k + 1
        );
        assert_eq!(codes(line), first, "scan line {}", k + 1);
    }
}

/// Verifies REQ-330: with `VSCROL` = 15, a `$00` line after a `$22` line has 16 scan lines
/// with code `%000` at positions `$22-$DD`.
#[test]
fn extended_blank_line_16() {
    let mut rig = extended_rig(0, 15, 0xFF, 17);
    let lines = rig.lines(17);
    assert_eq!(fetch_lines(&lines.concat()), [17, 33], "16 scan lines");
    for (k, line) in lines.iter().take(16).enumerate() {
        assert!(
            codes(line)[0x22..=0xDD].iter().all(|&c| c == 0),
            "scan line {k}: background"
        );
    }
}

/// Verifies REQ-330: a write of `$01` to `VSCROL` on cycle 50 of row 2 of a `$02` line
/// after a `$22` line (`VSCROL` = 3) gives rows 0, 1, 2, ..., 15, 0, 1 (18 scan lines).
#[test]
fn vscrol_change_mid_line_repeats_rows() {
    let mut rig = extended_rig(2, 3, 0x01, 13);
    let mut all = rows(&mut rig, 2);
    rig.seek_after(15, 50);
    all.push(rig.antic.debug_state().row);
    rig.write(0x05, 0x01);
    rig.seek(16, 0);
    all.extend(rows(&mut rig, 15));
    let expected: Vec<u8> = (0..16).chain(0..2).collect();
    assert_eq!(all, expected, "rows");
    let recs = rig.run_to(40, 0);
    assert_eq!(
        fetch_lines(&recs)[0],
        31,
        "the next fetch after 18 scan lines"
    );
}
