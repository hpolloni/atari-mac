//! DMA timing, playfield width and mode heights (REQ-305, REQ-308 to REQ-310, REQ-312,
//! REQ-336).

use super::{Rig, SCREEN, codes, cycles_of, every, fetch_lines, refresh_cycles};
use crate::antic::DmaKind;
use crate::antic::decode::height;
use crate::timing::VBLANK_START_LINE;

/// A display list with one mode line `ir` (with LMS `SCREEN`) on scan line 8, then
/// another `ir` line and a JVB.
fn mode_line_rig(dmactl: u8, ir: u8) -> Rig {
    Rig::with_dl(
        dmactl,
        &[ir | 0x40, 0x00, 0x20, ir & !0x40, 0x41, 0x00, 0x10],
    )
}

/// The first fetch cycle, the interval and the number of fetches (REQ-309 table).
fn fetch_plan(mode: u8, width: u8) -> (u32, usize, usize) {
    let interval = match mode {
        2..=5 | 0x0D..=0x0F => 2,
        6 | 7 | 0x0A..=0x0C => 4,
        _ => 8,
    };
    let base = match width {
        1 => 26,
        2 => 18,
        _ => 10,
    };
    let first = if mode >= 8 { base + 2 } else { base };
    let window = 48 + 16 * usize::from(width);
    (first, interval, window / interval)
}

/// Verifies REQ-305: no DMA except refresh in vertical blank.
#[test]
fn no_dma_in_vertical_blank() {
    let mut dl = vec![0x4E, 0x00, 0x20];
    dl.extend([0x0E; 239]);
    dl.extend([0x41, 0x00, 0x10]);
    let mut rig = Rig::with_dl(0x3E, &dl);
    let frame = rig.frame();
    for rec in frame
        .iter()
        .filter(|r| !(8..VBLANK_START_LINE).contains(&r.out.line))
    {
        assert!(
            matches!(rec.out.dma, DmaKind::Refresh | DmaKind::None),
            "({}, {}): {:?}",
            rec.out.line,
            rec.out.cycle,
            rec.out.dma
        );
    }
    assert_eq!(
        fetch_lines(&frame).len(),
        240,
        "the display list fills scan lines 8-247"
    );
}

/// Checks a mode F line of `$FF` bytes: `%111` at `first..=last`, `%000` elsewhere.
fn check_mode_f_window(dmactl: u8, first: usize, last: usize) {
    let mut rig = mode_line_rig(dmactl, 0x0F);
    rig.fill(SCREEN, 48, 0xFF);
    let line = codes(&rig.line());
    for (x, &code) in line.iter().enumerate().take(0xDE).skip(0x22) {
        let expected = if (first..=last).contains(&x) {
            0b111
        } else {
            0b000
        };
        assert_eq!(code, expected, "DMACTL ${dmactl:02X} position ${x:02X}");
    }
}

/// Verifies REQ-308: the narrow playfield is at `$40-$BF`.
#[test]
fn narrow_playfield_at_40_to_bf() {
    check_mode_f_window(0x21, 0x40, 0xBF);
}

/// Verifies REQ-308: the normal playfield is at `$30-$CF`.
#[test]
fn normal_playfield_at_30_to_cf() {
    check_mode_f_window(0x22, 0x30, 0xCF);
}

/// Verifies REQ-308: the wide playfield shows at `$2C-$DD` (the bus is `$FF`).
#[test]
fn wide_playfield_clipped_at_22_to_2b() {
    check_mode_f_window(0x23, 0x2C, 0xDD);
}

/// Verifies REQ-308: with the playfield off, a mode F line shows background.
#[test]
fn playfield_off_shows_background() {
    let mut rig = mode_line_rig(0x20, 0x0F);
    rig.fill(SCREEN, 48, 0xFF);
    let recs = rig.lines(2);
    for line in &recs {
        let line_codes = codes(line);
        assert!(
            line_codes[0x22..=0xDD].iter().all(|&c| c == 0),
            "background"
        );
        assert!(cycles_of(line, DmaKind::Bitmap).is_empty(), "no bitmap DMA");
    }
    assert_eq!(
        rig.antic.debug_state().ir & 0x0F,
        0x0F,
        "IR is a mode F line"
    );
}

/// Verifies REQ-309: normal mode E fetches on cycles 20-98.
#[test]
fn dma_cycles_mode_e_normal() {
    let mut rig = mode_line_rig(0x22, 0x0E);
    let line = rig.line();
    assert_eq!(
        cycles_of(&line, DmaKind::Bitmap),
        every(20, 98, 2),
        "bitmap cycles"
    );
}

/// Verifies REQ-309: the first scan line of a normal mode 2 line.
#[test]
fn dma_cycles_mode_2_normal_first_line() {
    let mut rig = mode_line_rig(0x22, 0x02);
    let line = rig.line();
    assert_eq!(
        cycles_of(&line, DmaKind::CharacterName),
        every(18, 96, 2),
        "names"
    );
    assert_eq!(
        cycles_of(&line, DmaKind::CharacterData),
        every(21, 99, 2),
        "data"
    );
}

/// Verifies REQ-309: scan lines 2-8 of a mode 2 line fetch only character data.
#[test]
fn dma_cycles_mode_2_later_lines() {
    let mut rig = mode_line_rig(0x22, 0x02);
    let recs = rig.lines(8);
    for line in &recs[1..] {
        assert_eq!(
            cycles_of(line, DmaKind::CharacterData),
            every(21, 99, 2),
            "data"
        );
        assert!(
            cycles_of(line, DmaKind::CharacterName).is_empty(),
            "no names"
        );
    }
}

/// Verifies REQ-309: narrow mode 8 fetches on cycles 28-84.
#[test]
fn dma_cycles_mode_8_narrow() {
    let mut rig = mode_line_rig(0x21, 0x08);
    let line = rig.line();
    assert_eq!(
        cycles_of(&line, DmaKind::Bitmap),
        every(28, 84, 8),
        "bitmap cycles"
    );
}

/// Verifies REQ-309 and REQ-310: all modes and widths fetch on the cycles of the table.
#[test]
fn dma_cycles_all_modes_and_widths() {
    for mode in 2..=0x0F {
        for width in 1..=3 {
            let mut rig = mode_line_rig(0x20 | width, mode);
            let line = rig.line();
            let (first, interval, count) = fetch_plan(mode, width);
            let fetches: Vec<u32> = (first..).step_by(interval).take(count).collect();
            let real: Vec<u32> = fetches.iter().copied().filter(|&c| c <= 105).collect();
            let what = format!("mode {mode:X} width {width}");
            if mode < 8 {
                assert_eq!(
                    cycles_of(&line, DmaKind::CharacterName),
                    real,
                    "{what} names"
                );
                let data: Vec<u32> = fetches
                    .iter()
                    .map(|c| c + 3)
                    .filter(|&c| c <= 105)
                    .collect();
                assert_eq!(
                    cycles_of(&line, DmaKind::CharacterData),
                    data,
                    "{what} data"
                );
            } else {
                assert_eq!(cycles_of(&line, DmaKind::Bitmap), real, "{what} bitmap");
            }
            for rec in &line {
                assert_eq!(rec.out.halt, rec.out.dma != DmaKind::None, "{what} halt");
            }
        }
    }
}

/// Verifies REQ-309: the memory scan counter counts all fetches, also the virtual ones.
#[test]
fn memscan_counts_fetches() {
    for mode in 2..=0x0F {
        for width in 1..=3 {
            let mut rig = mode_line_rig(0x20 | width, mode);
            rig.skip(114 * usize::from(height(mode)));
            let (_, _, count) = fetch_plan(mode, width);
            let expected = SCREEN + u16::try_from(count).unwrap();
            assert_eq!(
                rig.antic.debug_state().memscan,
                expected,
                "mode {mode:X} width {width}"
            );
        }
    }
}

/// Verifies REQ-310: in a frame of mode 2, mode E and blank lines, `halt` marks the DMA.
#[test]
fn dma_cycles_set_halt() {
    let mut dl = vec![0x70, 0x42, 0x00, 0x20, 0x02, 0x4E, 0x00, 0x30];
    dl.extend([0x0E; 20]);
    dl.extend([0x30, 0x41, 0x00, 0x10]);
    let mut rig = Rig::with_dl(0x3E, &dl);
    let frame = rig.frame();
    for rec in &frame {
        assert_eq!(
            rec.out.halt,
            rec.out.dma != DmaKind::None,
            "({}, {})",
            rec.out.line,
            rec.out.cycle
        );
    }
    let kinds = [DmaKind::CharacterName, DmaKind::Bitmap, DmaKind::Player(3)];
    for kind in kinds {
        assert!(
            !cycles_of(&frame, kind).is_empty(),
            "the frame has {kind:?} DMA"
        );
    }
}

/// Verifies REQ-310: the halt cycles of the first scan line of a `$70` line.
#[test]
fn blank_line_halt_map() {
    let mut rig = Rig::with_dl(0x2C, &[0x70, 0x41, 0x00, 0x10]);
    let line = rig.line();
    let halts: Vec<u32> = line
        .iter()
        .filter(|r| r.out.halt)
        .map(|r| r.out.cycle)
        .collect();
    let mut expected = every(0, 5, 1);
    expected.extend(refresh_cycles());
    assert_eq!(halts, expected, "halt cycles");
}

/// Verifies REQ-312: the Graphics 0 display list fetches once per mode line.
#[test]
fn display_list_of_24_mode_2_lines() {
    let mut dl = vec![0x70, 0x70, 0x70, 0x42, 0x00, 0x30];
    dl.extend([0x02; 23]);
    dl.extend([0x41, 0x00, 0x10]);
    let mut rig = Rig::with_dl(0x22, &dl);
    let recs = rig.run_to(VBLANK_START_LINE, 0);
    assert_eq!(
        fetch_lines(&recs),
        every(8, 224, 8),
        "instruction fetch lines"
    );
    let jvb = recs
        .iter()
        .find(|r| (r.out.line, r.out.cycle) == (224, 1))
        .unwrap();
    assert_eq!(
        jvb.out.dma_data, 0x41,
        "the JVB is fetched on scan line 224"
    );
}

/// Verifies REQ-312: each mode has the height of the table.
#[test]
fn mode_heights_match_table() {
    let heights = [8, 10, 8, 16, 8, 16, 8, 4, 4, 2, 1, 2, 1, 1];
    for (mode, h) in (2..=0x0F).zip(heights) {
        let mut rig = mode_line_rig(0x22, mode);
        let recs = rig.run_to(60, 0);
        let lines = fetch_lines(&recs);
        assert_eq!(lines[..3], [8, 8 + h, 8 + 2 * h], "mode {mode:X}");
    }
}

/// Verifies REQ-312: mode 8 fetches only on its first scan line.
#[test]
fn mapped_mode_fetches_first_line_only() {
    let mut rig = mode_line_rig(0x22, 0x08);
    for (i, byte) in (0..10).zip([0x1B, 0xE4, 0x55, 0xAA, 0x0F, 0xF0, 0x33, 0xCC, 0x99, 0x66]) {
        rig.poke(SCREEN + i, &[byte]);
    }
    let recs = rig.lines(8);
    assert_eq!(
        cycles_of(&recs[0], DmaKind::Bitmap).len(),
        10,
        "first scan line"
    );
    let first = codes(&recs[0]);
    for line in &recs[1..] {
        assert!(cycles_of(line, DmaKind::Bitmap).is_empty(), "no bitmap DMA");
        assert_eq!(codes(line), first, "the same codes");
    }
    assert!(
        first[0x30..0xD0].iter().any(|&c| c != 0),
        "the line shows data"
    );
}

/// Verifies REQ-312: mode 5 fetches the font on every scan line.
#[test]
fn mode_5_fetches_font_every_line() {
    let mut rig = mode_line_rig(0x22, 0x05);
    let recs = rig.lines(16);
    for (row, line) in recs.iter().enumerate() {
        assert_eq!(
            cycles_of(line, DmaKind::CharacterData).len(),
            40,
            "row {row} data"
        );
        let names = cycles_of(line, DmaKind::CharacterName).len();
        assert_eq!(names, if row == 0 { 40 } else { 0 }, "row {row} names");
    }
}

/// Verifies REQ-312: mode A uses the line buffer on scan lines 2-4.
#[test]
fn mode_a_uses_line_buffer() {
    let mut rig = mode_line_rig(0x22, 0x0A);
    rig.fill(SCREEN, 20, 0x1B);
    let first = rig.line();
    rig.fill(SCREEN, 20, 0xE4);
    let rest = rig.lines(3);
    for line in &rest {
        assert_eq!(codes(line), codes(&first), "the same codes");
    }
    assert_eq!(
        &codes(&first)[0x30..0x38],
        [0, 0, 4, 4, 5, 5, 6, 6],
        "the codes of $1B"
    );
}

/// Verifies REQ-312: the row counter is 0 on the first scan line and H - 1 on the last.
#[test]
fn row_counter_0_to_h_minus_1() {
    for mode in 2..=0x0F {
        let mut rig = mode_line_rig(0x22, mode);
        let h = u32::from(height(mode));
        rig.seek_after(8, 50);
        assert_eq!(
            rig.antic.debug_state().row,
            0,
            "mode {mode:X} first scan line"
        );
        rig.seek_after(8 + h - 1, 50);
        let row = u32::from(rig.antic.debug_state().row);
        assert_eq!(row, h - 1, "mode {mode:X} last scan line");
    }
}

/// The halt flags of cycles 104-107 of the first scan line of a mode line.
fn halts_104_to_107(dmactl: u8, ir: u8, hscrol: u8) -> Vec<bool> {
    let mut rig = mode_line_rig(dmactl, ir);
    rig.write(0x04, hscrol);
    let line = rig.line();
    line[104..=107].iter().map(|r| r.out.halt).collect()
}

/// Verifies REQ-336: the DMA on cycles 105 and 106 of the three `WSYNC` setups.
#[test]
fn halt_on_cycles_105_106_setups() {
    assert_eq!(
        halts_104_to_107(0x23, 0x06, 0),
        [false, true, false, false],
        "wide mode 6: character data on cycle 105"
    );
    assert_eq!(
        halts_104_to_107(0x21, 0x12, 14),
        [true, true, false, false],
        "narrow $12 with HSCROL 14: refresh on cycle 105"
    );
    assert_eq!(
        halts_104_to_107(0x23, 0x02, 0),
        [true, true, true, false],
        "wide mode 2: data on 105 and refresh on 106"
    );
}
