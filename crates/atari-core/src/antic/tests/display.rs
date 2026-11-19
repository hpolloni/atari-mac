//! Display list, character modes and `CHACTL` (REQ-305, REQ-313, REQ-314, REQ-319 to
//! REQ-323).

use super::{DL, FONT, Rec, Rig, SCREEN, codes, cycles_of, every, fetch_lines, on_line};
use crate::antic::DmaKind;
use crate::timing::{VBLANK_START_LINE, VideoStandard};

/// One scan line of a character mode line: the address of the first character data
/// fetch and the codes at positions `$30-$33` (the first character).
type CharRow = (Option<u16>, [u8; 4]);

/// Runs `rows` scan lines of a character mode line with the names `names` and the font
/// rows `font` for each name (all names use the same 16 font rows).
fn char_rows(mode: u8, names: &[u8], chactl: u8, rows: usize) -> Vec<CharRow> {
    let mut rig = Rig::with_dl(0x22, &[0x40 | mode, 0x00, 0x20, 0x41, 0x00, 0x10]);
    rig.write(0x01, chactl);
    rig.poke(SCREEN, names);
    rig.lines(rows)
        .iter()
        .map(|line| {
            let read = line
                .iter()
                .find(|r| r.out.dma == DmaKind::CharacterData)
                .and_then(|r| r.read);
            let c = codes(line);
            (read, [c[0x30], c[0x31], c[0x32], c[0x33]])
        })
        .collect()
}

/// Like `char_rows` with the font of name `name` set to `byte` on all rows.
fn char_rows_font(mode: u8, name: u8, byte: u8, chactl: u8, rows: usize) -> Vec<CharRow> {
    let mut rig = Rig::with_dl(0x22, &[0x40 | mode, 0x00, 0x20, 0x41, 0x00, 0x10]);
    rig.write(0x01, chactl);
    rig.poke(SCREEN, &[name]);
    let base = if matches!(mode, 6 | 7) {
        FONT | (u16::from(name & 0x3F) << 3)
    } else {
        FONT | (u16::from(name & 0x7F) << 3)
    };
    rig.fill(base, 8, byte);
    rig.lines(rows)
        .iter()
        .map(|line| {
            let read = line
                .iter()
                .find(|r| r.out.dma == DmaKind::CharacterData)
                .and_then(|r| r.read);
            let c = codes(line);
            (read, [c[0x30], c[0x31], c[0x32], c[0x33]])
        })
        .collect()
}

/// The 4 hi-res codes of a font byte.
fn hires(byte: u8) -> [u8; 4] {
    [6, 4, 2, 0].map(|shift| 0b100 | ((byte >> shift) & 3))
}

/// Verifies REQ-305: a 480-line display list alternates between two frames.
#[test]
fn long_display_list_alternates_frames() {
    let mut dl = vec![0x70; 59];
    dl.extend([0x60, 0x01, 0x00, 0x10]);
    let mut rig = Rig::with_dl(0x22, &dl);
    let mut reads = Vec::new();
    for _ in 0..3 {
        let frame = rig.frame();
        let fetch = frame.iter().find(|r| (r.out.line, r.out.cycle) == (8, 1));
        reads.push(fetch.and_then(|r| r.read));
    }
    assert_eq!(
        reads,
        [Some(0x1000), Some(0x101E), Some(0x1000)],
        "fetch addresses on scan line 8"
    );
}

/// Verifies REQ-305: a display list address written on scan line 0 is used on line 8.
#[test]
fn display_list_resumes_on_line_8() {
    let mut rig = Rig::with_dl(0x22, &[0x41, 0x00, 0x10]);
    rig.seek_after(0, 20);
    rig.poke(0x1234, &[0x41, 0x34, 0x12]);
    rig.write(0x02, 0x34);
    rig.write(0x03, 0x12);
    let recs = rig.run_to(8, 0);
    assert!(fetch_lines(&recs).is_empty(), "no fetch on scan lines 0-7");
    let line = rig.line();
    assert_eq!(line[1].out.dma, DmaKind::DisplayList, "fetch on cycle 1");
    assert_eq!(line[1].read, Some(0x1234), "fetch address");
}

/// Verifies REQ-305: vertical blank has no playfield or background codes.
#[test]
fn vblank_has_no_playfield_codes() {
    let mut rig = Rig::with_dl(0x22, &[0x41, 0x00, 0x10]);
    let frame = rig.frame();
    let lines = (VBLANK_START_LINE..262).chain(0..8);
    for line in lines {
        let line_codes = codes(&on_line(&frame, line));
        assert!(
            line_codes.iter().all(|c| matches!(c, 1..=3)),
            "scan line {line}: {line_codes:?}"
        );
    }
}

/// Verifies REQ-313: the font bits give hi-res pairs, left first, in modes 2 and 3.
#[test]
fn mode_2_3_font_bit_order() {
    for mode in [2, 3] {
        let rows = char_rows_font(mode, 0x01, 0x9C, 0x00, 1);
        assert_eq!(rows[0].1, [0b110, 0b101, 0b111, 0b100], "mode {mode}");
    }
}

/// Verifies REQ-313: a normal mode 2 line shows 40 characters at `$30 + 4k`.
#[test]
fn mode_2_forty_characters() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x20, 0x41, 0x00, 0x10]);
    let names: Vec<u8> = (0..40).collect();
    rig.poke(SCREEN, &names);
    let font = |k: u8| k.wrapping_mul(37).wrapping_add(11);
    for k in 0..40 {
        rig.poke(FONT + u16::from(k) * 8, &[font(k)]);
    }
    let line = codes(&rig.line());
    for k in 0..40_u8 {
        let x = 0x30 + 4 * usize::from(k);
        assert_eq!(line[x..x + 4], hires(font(k)), "character {k}");
    }
}

/// Verifies REQ-313: the mode 2 font address.
#[test]
fn mode_2_font_address() {
    let rows = char_rows(2, &[0x41], 0x00, 8);
    assert_eq!(rows[0].0, Some(0xE208), "row 0 of $41");
    assert_eq!(rows[5].0, Some(0xE20D), "row 5 of $41");
    let rows = char_rows(2, &[0xC1], 0x00, 1);
    assert_eq!(rows[0].0, Some(0xE208), "row 0 of $C1");
}

/// Verifies REQ-313: mode 3 rows 8-9 are blank for names `$00-$5F`.
#[test]
fn mode_3_lower_rows_blank() {
    let rows = char_rows_font(3, 0x41, 0xFF, 0x00, 10);
    for (r, (read, line_codes)) in rows.iter().enumerate() {
        let r16 = u16::try_from(r).unwrap();
        if r < 8 {
            assert_eq!(*read, Some(0xE208 + r16), "row {r} font row");
            assert_eq!(*line_codes, [7; 4], "row {r} codes");
        } else {
            assert_eq!(*line_codes, [4; 4], "row {r} is blank");
        }
    }
}

/// Verifies REQ-313: mode 3 descenders for names with bits 6-5 = `%11`.
#[test]
fn mode_3_descender_rows() {
    for name in [0x61, 0xE1] {
        let rows = char_rows_font(3, name, 0xFF, 0x00, 10);
        for (r, (read, line_codes)) in rows.iter().enumerate() {
            let font_row = u16::try_from(r % 8).unwrap();
            if r < 2 {
                assert_eq!(*line_codes, [4; 4], "${name:02X} row {r} is blank");
            } else {
                assert_eq!(*read, Some(0xE308 + font_row), "${name:02X} row {r}");
                assert_eq!(*line_codes, [7; 4], "${name:02X} row {r} codes");
            }
        }
    }
}

/// Verifies REQ-313: the blank rows of mode 3 are inverted by `CHACTL` bit 1.
#[test]
fn mode_3_blank_rows_inverted() {
    let rows = char_rows_font(3, 0xC1, 0x00, 0x02, 10);
    for row in &rows[8..] {
        assert_eq!(row.1, [7; 4], "rows 8-9");
    }
}

/// Verifies REQ-314: `CHACTL` bit 0 blanks characters with name bit 7.
#[test]
fn chactl_blink_blanks_bit7_chars() {
    let rows = char_rows_font(2, 0xC1, 0x9C, 0x01, 1);
    assert_eq!(rows[0].1, [4; 4], "blank");
}

/// Verifies REQ-314: `CHACTL` bit 1 inverts characters with name bit 7.
#[test]
fn chactl_invert() {
    let rows = char_rows_font(2, 0xC1, 0x9C, 0x02, 1);
    assert_eq!(rows[0].1, [0b101, 0b110, 0b100, 0b111], "inverted $63");
}

/// Verifies REQ-314: blink and invert give a solid character.
#[test]
fn chactl_blink_and_invert_gives_solid() {
    let rows = char_rows_font(2, 0xC1, 0x9C, 0x03, 1);
    assert_eq!(rows[0].1, [7; 4], "solid");
}

/// Verifies REQ-314: `CHACTL` bits 0-1 do not change names with bit 7 clear.
#[test]
fn chactl_bit7_clear_unchanged() {
    for chactl in 1..=3 {
        let rows = char_rows_font(2, 0x41, 0x9C, chactl, 1);
        assert_eq!(rows[0].1, [0b110, 0b101, 0b111, 0b100], "CHACTL {chactl}");
    }
}

/// The codes of a full mode line of the names `$81` and `$C1` with font bytes `$5A`.
fn mode_line_codes(mode: u8, chactl: u8) -> Vec<[u8; 228]> {
    let mut rig = Rig::with_dl(0x22, &[0x40 | mode, 0x00, 0x20, 0x41, 0x00, 0x10]);
    rig.write(0x01, chactl);
    rig.poke(SCREEN, &[0x81, 0xC1, 0x81, 0xC1]);
    rig.fill(FONT, 0x400, 0x5A);
    rig.lines(16).iter().map(|line| codes(line)).collect()
}

/// Verifies REQ-314: `CHACTL` bits 0-1 have no effect in modes 4-7.
#[test]
fn chactl_no_effect_modes_4_to_7() {
    for mode in 4..=7 {
        assert_eq!(
            mode_line_codes(mode, 0x03),
            mode_line_codes(mode, 0x00),
            "mode {mode}"
        );
    }
}

/// Verifies REQ-314: `CHACTL` bit 2 reflects the font rows of all character modes.
#[test]
fn chactl_reflect_all_char_modes() {
    for mode in [2, 4, 6] {
        let rows = char_rows(mode, &[0x01], 0x04, 8);
        for (r, row) in (0..8_u16).zip(&rows) {
            assert_eq!(row.0, Some(0xE008 + 7 - r), "mode {mode} row {r}");
        }
    }
    for mode in [5, 7] {
        let rows = char_rows(mode, &[0x01], 0x04, 16);
        for (r, row) in (0..16_u16).zip(&rows) {
            assert_eq!(row.0, Some(0xE008 + 7 - (r >> 1)), "mode {mode} row {r}");
        }
    }
}

/// Verifies REQ-314: reflection keeps the blank rows of mode 3.
#[test]
fn chactl_reflect_keeps_mode_3_blank_rows() {
    let rows = char_rows_font(3, 0x41, 0xFF, 0x04, 10);
    for (r, row) in (0..10_u16).zip(&rows) {
        if r < 8 {
            assert_eq!(row.0, Some(0xE208 + 7 - r), "$41 row {r}");
        } else {
            assert_eq!(row.1, [4; 4], "$41 row {r} is blank");
        }
    }
    let rows = char_rows_font(3, 0x61, 0xFF, 0x04, 10);
    for (r, row) in (0..10_u16).zip(&rows) {
        if r < 2 {
            assert_eq!(row.1, [4; 4], "$61 row {r} is blank");
        } else {
            assert_eq!(row.0, Some(0xE308 + 7 - (r % 8)), "$61 row {r}");
        }
    }
}

/// A display list with 4 `$70` lines (scan lines 8-39), then `rest`.
fn after_32_lines(rest: &[u8]) -> Vec<u8> {
    let mut dl = vec![0x70; 4];
    dl.extend(rest);
    dl
}

/// Verifies REQ-319: the instruction fetch is on cycle 1 of the first scan line.
#[test]
fn instruction_fetch_on_cycle_1() {
    let mut rig = Rig::with_dl(0x22, &after_32_lines(&[0x42, 0x00, 0x20, 0x41, 0x00, 0x10]));
    let recs = rig.after(40, 1);
    let state = rig.antic.debug_state();
    assert_eq!(state.ir, 0x42, "IR");
    assert_eq!(state.dlist, DL + 5, "the address is one higher");
    let recs: Vec<Rec> = recs
        .into_iter()
        .chain(rig.run_to(VBLANK_START_LINE, 0))
        .collect();
    let fetches: Vec<(u32, u32)> = recs
        .iter()
        .filter(|r| r.out.dma == DmaKind::DisplayList)
        .map(|r| (r.out.line, r.out.cycle))
        .collect();
    assert_eq!(
        fetches,
        [(8, 1), (16, 1), (24, 1), (32, 1), (40, 1), (48, 1)],
        "fetch positions"
    );
}

/// Verifies REQ-319 and REQ-323: the LMS address bytes are read on cycles 6 and 7.
#[test]
fn address_fetch_on_cycles_6_7() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x40, 0x41, 0x00, 0x10]);
    let recs = rig.lines(9);
    let first = &recs[0];
    assert_eq!(first[1].read, Some(0x1000), "instruction");
    assert_eq!(
        (first[6].out.dma, first[6].read),
        (DmaKind::DisplayListAddress, Some(0x1001)),
        "cycle 6"
    );
    assert_eq!(
        (first[7].out.dma, first[7].read),
        (DmaKind::DisplayListAddress, Some(0x1002)),
        "cycle 7"
    );
    assert_eq!(recs[8][1].read, Some(0x1003), "next instruction");
}

/// Verifies REQ-319: a display list address write changes the next fetch.
#[test]
fn dlist_write_changes_next_fetch() {
    let mut rig = Rig::with_dl(0x22, &after_32_lines(&[0x42, 0x00, 0x20, 0x41, 0x00, 0x10]));
    rig.seek_after(44, 60);
    rig.write(0x02, 0x00);
    rig.write(0x03, 0x30);
    let recs = rig.run_to(49, 0);
    let fetch = recs
        .iter()
        .find(|r| r.out.dma == DmaKind::DisplayList)
        .unwrap();
    assert_eq!(
        (fetch.out.line, fetch.read),
        (48, Some(0x3000)),
        "next fetch"
    );
}

/// Verifies REQ-320: `$n0` is a blank line of n + 1 scan lines.
#[test]
fn blank_line_counts_1_to_8() {
    for n in 0..8 {
        let mut rig = Rig::with_dl(0x22, &[n << 4, 0x41, 0x00, 0x10]);
        let recs = rig.run_to(20, 0);
        assert_eq!(
            fetch_lines(&recs),
            [8, 8 + u32::from(n) + 1],
            "${:02X}",
            n << 4
        );
    }
}

/// Verifies REQ-320: a blank line shows background and has no playfield DMA.
#[test]
fn blank_line_has_no_playfield_dma() {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0x41, 0x00, 0x10]);
    for line in rig.lines(8) {
        assert!(
            codes(&line)[0x22..=0xDD].iter().all(|&c| c == 0),
            "background"
        );
        for kind in [
            DmaKind::CharacterName,
            DmaKind::CharacterData,
            DmaKind::Bitmap,
        ] {
            assert!(cycles_of(&line, kind).is_empty(), "no {kind:?}");
        }
    }
}

/// Verifies REQ-320: `$70` has no address fetch and starts no scroll region.
#[test]
fn blank_line_ignores_lms_and_scroll_bits() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x20, 0x70, 0x41, 0x00, 0x10]);
    rig.write(0x05, 4);
    let recs = rig.run_to(30, 0);
    assert_eq!(fetch_lines(&recs), [8, 16, 24], "fetch lines");
    let blank = on_line(&recs, 16);
    assert!(
        cycles_of(&blank, DmaKind::DisplayListAddress).is_empty(),
        "no address"
    );
}

/// Verifies REQ-320: a blank line with the DLI bit gives one DLI on its last scan line.
#[test]
fn blank_line_dli() {
    let mut rig = Rig::with_dl(0x22, &[0xF0, 0x41, 0x00, 0x10]);
    rig.write(0x0E, 0x80);
    let mut nmi_lines = Vec::new();
    for line in 8..16 {
        let recs = rig.line();
        let bit7 = rig.read(0x0F) & 0x80 != 0;
        assert_eq!(bit7, line == 15, "NMIST bit 7 after scan line {line}");
        nmi_lines.extend(recs.iter().filter(|r| r.out.nmi).map(|r| r.out.line));
    }
    assert_eq!(nmi_lines, [15], "one NMI");
}

/// Verifies REQ-320: a blank line ends a vertical scroll region.
#[test]
fn blank_line_ends_scroll_region() {
    let mut rig = Rig::with_dl(0x22, &[0x62, 0x00, 0x20, 0x70, 0x41, 0x00, 0x10]);
    rig.write(0x05, 2);
    let recs = rig.run_to(30, 0);
    assert_eq!(
        fetch_lines(&recs),
        [8, 14, 17],
        "the $70 line has 3 scan lines"
    );
}

/// Verifies REQ-321: a jump loads the display list address.
#[test]
fn jump_loads_dlist() {
    let mut rig = Rig::with_dl(0x22, &[0x01, 0x00, 0x30]);
    rig.poke(0x3000, &[0x41, 0x00, 0x10]);
    let recs = rig.run_to(12, 0);
    let fetches: Vec<Option<u16>> = recs
        .iter()
        .filter(|r| r.out.dma == DmaKind::DisplayList)
        .map(|r| r.read)
        .collect();
    assert_eq!(fetches, [Some(0x1000), Some(0x3000)], "fetch addresses");
}

/// Verifies REQ-321: a jump is one blank scan line.
#[test]
fn jump_shows_one_blank_line() {
    let mut rig = Rig::with_dl(0x22, &[0x01, 0x00, 0x30]);
    rig.poke(0x3000, &[0x41, 0x00, 0x10]);
    let recs = rig.run_to(12, 0);
    assert_eq!(fetch_lines(&recs), [8, 9], "fetch lines");
    let jump = codes(&on_line(&recs, 8));
    assert!(jump[0x22..=0xDD].iter().all(|&c| c == 0), "background");
}

/// Verifies REQ-321: a jump ignores bits 4-5.
#[test]
fn jump_ignores_scroll_bits() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x20, 0x31, 0x00, 0x30]);
    rig.poke(0x3000, &[0x41, 0x00, 0x10]);
    rig.write(0x05, 4);
    let recs = rig.run_to(20, 0);
    assert_eq!(fetch_lines(&recs), [8, 16, 17], "the jump has 1 scan line");
    let fetch = on_line(&recs, 17)[1];
    assert_eq!(fetch.read, Some(0x3000), "the jump target");
}

/// Verifies REQ-321: a jump target is not limited to the 1K block.
/// Verifies REQ-324: a jump at `$07FD` loads all 16 bits of the next instruction address.
#[test]
fn jump_crosses_1k() {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    rig.seek(VBLANK_START_LINE, 0);
    rig.poke(0x07FD, &[0x01, 0x00, 0x44]);
    rig.poke(0x4400, &[0x41, 0x00, 0x44]);
    rig.write(0x02, 0xFD);
    rig.write(0x03, 0x07);
    rig.write(0x00, 0x22);
    rig.seek(8, 0);
    let recs = rig.lines(2);
    assert_eq!(recs[0][6].read, Some(0x07FE), "low byte");
    assert_eq!(recs[0][7].read, Some(0x07FF), "high byte");
    assert_eq!(recs[1][1].read, Some(0x4400), "next fetch");
}

/// Verifies REQ-321: a jump with the DLI bit gives a DLI on its scan line.
#[test]
fn jump_dli() {
    let mut rig = Rig::with_dl(0x22, &[0x81, 0x00, 0x30]);
    rig.poke(0x3000, &[0x41, 0x00, 0x30]);
    rig.write(0x0E, 0x80);
    rig.seek_after(8, 6);
    assert_eq!(rig.read(0x0F) & 0x80, 0, "NMIST bit 7 before cycle 7");
    let recs = rig.after(8, 7);
    assert_eq!(rig.read(0x0F) & 0x80, 0x80, "NMIST bit 7 on cycle 7");
    let recs: Vec<Rec> = recs.into_iter().chain(rig.run_to(9, 0)).collect();
    assert_eq!(recs.iter().filter(|r| r.out.nmi).count(), 1, "one NMI");
}

/// Verifies REQ-322: a JVB stops the display list fetches until vertical blank.
#[test]
fn jvb_stops_fetch_until_vblank() {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0x41, 0x00, 0x10]);
    let recs = rig.run_to(VBLANK_START_LINE, 0);
    for rec in recs.iter().filter(|r| r.out.line > 16) {
        assert!(
            !matches!(
                rec.out.dma,
                DmaKind::DisplayList | DmaKind::DisplayListAddress
            ),
            "scan line {}",
            rec.out.line
        );
    }
    assert_eq!(fetch_lines(&recs), [8, 16], "fetch lines");
}

/// Verifies REQ-322: the JVB loads its address on its first scan line.
#[test]
fn jvb_loads_address_on_first_line() {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0x41, 0x00, 0x10]);
    rig.seek_after(16, 7);
    for _ in 0..1000 {
        assert_eq!(
            rig.antic.debug_state().dlist,
            0x1000,
            "display list address"
        );
        let _rec = rig.step();
    }
}

/// Verifies REQ-322: the display list starts again on scan line 8.
#[test]
fn jvb_restarts_on_line_8() {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0x41, 0x00, 0x10]);
    let frame = rig.frame();
    assert_eq!(
        fetch_lines(&frame),
        [8, 16],
        "fetch lines of the first frame"
    );
    let next = rig.line();
    assert_eq!(
        next[1].read,
        Some(0x1000),
        "fetch on scan line 8 of the next frame"
    );
}

/// Verifies REQ-322: a write during the JVB wait sets the next start address.
#[test]
fn jvb_dlist_write_replaces_address() {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0x41, 0x00, 0x10]);
    rig.poke(0x4000, &[0x41, 0x00, 0x40]);
    rig.seek_after(21, 30);
    rig.write(0x02, 0x00);
    rig.write(0x03, 0x40);
    rig.seek(8, 0);
    assert_eq!(rig.line()[1].read, Some(0x4000), "fetch address");
}

/// Verifies REQ-322: a JVB with the DLI bit gives a DLI on each scan line.
#[test]
fn jvb_with_dli_fires_every_line() {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0xC1, 0x00, 0x10]);
    rig.write(0x0E, 0x80);
    rig.seek(16, 0);
    let mut nmi_lines = Vec::new();
    for _ in 0..262 {
        let line = rig.line();
        let set = line.iter().filter(|r| r.out.nmi).map(|r| r.out.line);
        nmi_lines.extend(set);
    }
    let expected: Vec<u32> = (16..VBLANK_START_LINE).collect();
    assert_eq!(
        nmi_lines, expected,
        "one DLI per scan line, none in vertical blank"
    );
}

/// Verifies REQ-322: a 240-line display list with a JVB alternates frames.
#[test]
fn jvb_after_240_lines_alternates_frames() {
    let mut dl = vec![0x42, 0x00, 0x30];
    dl.extend([0x02; 29]);
    dl.extend([0x41, 0x00, 0x10]);
    let mut rig = Rig::with_dl(0x22, &dl);
    for pass in 0..2 {
        let a = rig.frame();
        assert_eq!(
            fetch_lines(&a),
            every(8, 240, 8),
            "pass {pass}: mode 2 frame"
        );
        let b = rig.frame();
        let dl_dma: Vec<(u32, u32)> = b
            .iter()
            .filter(|r| {
                matches!(
                    r.out.dma,
                    DmaKind::DisplayList | DmaKind::DisplayListAddress
                )
            })
            .map(|r| (r.out.line, r.out.cycle))
            .collect();
        assert_eq!(
            dl_dma,
            [(8, 1), (8, 6), (8, 7)],
            "pass {pass}: the JVB frame"
        );
        for line in 8..VBLANK_START_LINE {
            let line_codes = codes(&on_line(&b, line));
            assert!(
                line_codes[0x22..=0xDD].iter().all(|&c| c == 0),
                "blank line {line}"
            );
        }
    }
}

/// Verifies REQ-323: LMS loads the memory scan counter.
#[test]
fn lms_loads_memscan() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x40, 0x41, 0x00, 0x10]);
    let line = rig.line();
    let name = line
        .iter()
        .find(|r| r.out.dma == DmaKind::CharacterName)
        .unwrap();
    assert_eq!(name.read, Some(0x4000), "first name");
}

/// Verifies REQ-323: a line without LMS continues the memory scan counter.
#[test]
fn no_lms_continues_memscan() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x40, 0x02, 0x41, 0x00, 0x10]);
    let recs = rig.lines(9);
    let name = recs[8]
        .iter()
        .find(|r| r.out.dma == DmaKind::CharacterName)
        .unwrap();
    assert_eq!(name.read, Some(0x4028), "first name of the second line");
}

/// Verifies REQ-323: LMS adds no scan line.
#[test]
fn lms_adds_no_blank_line() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x40, 0x02, 0x41, 0x00, 0x10]);
    let recs = rig.run_to(30, 0);
    assert_eq!(fetch_lines(&recs), [8, 16, 24], "fetch lines");
}

/// Verifies REQ-323: the LMS bytes come on cycles 6 (low) and 7 (high).
#[test]
fn lms_address_on_cycles_6_7() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x34, 0x40, 0x41, 0x00, 0x10]);
    let line = rig.line();
    let address: Vec<(u32, u8)> = line
        .iter()
        .filter(|r| r.out.dma == DmaKind::DisplayListAddress)
        .map(|r| (r.out.cycle, r.out.dma_data))
        .collect();
    assert_eq!(address, [(6, 0x34), (7, 0x40)], "address bytes");
    assert_eq!(
        rig.antic.debug_state().memscan,
        0x4034 + 40,
        "memory scan counter"
    );
}
