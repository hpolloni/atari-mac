//! The line buffer, virtual fetches, mid-line `DMACTL` changes, abnormal DMA and the hi-res
//! bug (REQ-311, REQ-343 to REQ-347).

use core::iter;

use super::{FONT, Rec, Rig, SCREEN, codes, cycles_of, every, on_line};
use crate::antic::{DmaKind, LINE_BUFFER_SIZE};
use crate::planned::planned;
use crate::timing::{VBLANK_START_LINE, VideoStandard};

/// A different byte for each address, with bits 7-6 never `%11`.
fn byte_at(addr: u16) -> u8 {
    let [lo, hi] = addr.to_le_bytes();
    (lo.wrapping_mul(7) ^ hi.wrapping_mul(13)) & 0xBF
}

/// A rig with `DMACTL` = `dmactl`, the display list `dl`, and `byte_at` in `$2000-$3FFF`.
fn rig(dmactl: u8, dl: &[u8]) -> Rig {
    let mut rig = Rig::with_dl(dmactl, dl);
    for addr in 0x2000..0x4000 {
        rig.mem.ram[usize::from(addr)] = byte_at(addr);
    }
    rig
}

/// The codes of a 2-bit pixel byte, `width` color clocks per pixel (modes 8, A, D, E).
fn two_bit(byte: u8, width: usize) -> Vec<u8> {
    [6, 4, 2, 0]
        .iter()
        .flat_map(|&shift| {
            let code = match (byte >> shift) & 3 {
                0 => 0,
                pixel => 0b100 | (pixel - 1),
            };
            iter::repeat_n(code, width)
        })
        .collect()
}

/// The line buffer.
fn buffer(rig: &Rig) -> [u8; LINE_BUFFER_SIZE] {
    rig.antic.debug_state().line_buffer
}

/// The record of cycle `cycle` in `line`.
fn at(line: &[Rec], cycle: u32) -> Rec {
    line[usize::try_from(cycle).unwrap()]
}

/// Verifies REQ-311: with `DMACTL` = `$23`, on the first scan line of a mode E line, the
/// `Bitmap` fetch on cycle 104 sets `out.halt` and the fetch on cycle 106 does not.
#[test]
fn fetch_on_cycle_106_is_virtual() {
    let mut rig = rig(0x23, &[0x4E, 0x00, 0x20, 0x41, 0x00, 0x10]);
    let line = rig.line();
    assert_eq!(at(&line, 104).out.dma, DmaKind::Bitmap, "cycle 104");
    assert!(at(&line, 104).out.halt, "halt on cycle 104");
    assert_eq!(at(&line, 106).out.dma, DmaKind::None, "cycle 106");
    assert!(!at(&line, 106).out.halt, "no halt on cycle 106");
    assert_eq!(at(&line, 106).read, None, "no memory read on cycle 106");
}

/// Verifies REQ-311: after a wide mode E line with LMS `$3000`, the first fetch of the next
/// mode line without LMS reads `$3030` (48 fetches).
#[test]
fn wide_mode_e_advances_48() {
    let mut rig = rig(0x23, &[0x4E, 0x00, 0x30, 0x0E, 0x41, 0x00, 0x10]);
    let lines = rig.lines(2);
    let first = lines[1]
        .iter()
        .find(|rec| rec.out.dma == DmaKind::Bitmap)
        .and_then(|rec| rec.read);
    assert_eq!(first, Some(0x3030), "first fetch of the second line");
}

/// Runs the first scan line of the display list `dl` with `DMACTL` = `dmactl`, with the
/// data bus value `bus` on cycle `cycle` and `$FF` on the other cycles without DMA.
fn line_with_bus(rig: &mut Rig, cycle: u32, bus: u8) -> Vec<Rec> {
    let mut recs = rig.after(rig.pos().0, cycle - 1);
    rig.bus = bus;
    recs.push(rig.step());
    rig.bus = 0xFF;
    recs.extend(rig.run_to(rig.pos().0 + 1, 0));
    recs
}

/// Verifies REQ-311 and REQ-347: with `DMACTL` = `$23`, on the first scan line of a mode E
/// line, the data bus value `$A5` of cycle 106 (the virtual `Bitmap` fetch) goes into line
/// buffer byte 47.
#[test]
fn virtual_fetch_loads_data_bus() {
    let mut rig = rig(0x23, &[0x4E, 0x00, 0x20, 0x41, 0x00, 0x10]);
    let _line = line_with_bus(&mut rig, 106, 0xA5);
    let buffer = buffer(&rig);
    assert_eq!(buffer[47], 0xA5, "byte 47");
    for (k, &value) in buffer.iter().enumerate().take(47) {
        assert_eq!(
            value,
            byte_at(SCREEN + u16::try_from(k).unwrap()),
            "byte {k}"
        );
    }
}

/// Verifies REQ-311: with `DMACTL` = `$23`, on the first scan line of a mode 6 line, the
/// `CharacterData` fetch on cycle 105 sets `out.halt`.
#[test]
fn fetch_on_cycle_105_halts() {
    let mut rig = rig(0x23, &[0x46, 0x00, 0x20, 0x41, 0x00, 0x10]);
    let line = rig.line();
    assert_eq!(at(&line, 105).out.dma, DmaKind::CharacterData, "cycle 105");
    assert!(at(&line, 105).out.halt, "halt on cycle 105");
}

/// Verifies REQ-343: the 40 bytes of a normal mode E line stay in the line buffer after 8
/// blank lines, a jump and a vertical blank.
#[test]
fn line_buffer_never_cleared() {
    // A mode E line, 8 blank lines, a jump to a JVB.
    let mut rig = rig(
        0x22,
        &[0x4E, 0x00, 0x20, 0x70, 0x01, 0x07, 0x10, 0x41, 0x00, 0x10],
    );
    let _line = rig.line();
    let loaded = buffer(&rig);
    for (k, &value) in loaded.iter().enumerate().take(40) {
        assert_eq!(
            value,
            byte_at(SCREEN + u16::try_from(k).unwrap()),
            "byte {k}"
        );
    }
    let recs = rig.run_to(VBLANK_START_LINE, 0);
    assert!(
        cycles_of(&recs, DmaKind::Bitmap).is_empty(),
        "no bitmap DMA until vertical blank"
    );
    rig.seek(0, 0);
    rig.seek(8, 0);
    assert_eq!(buffer(&rig)[..40], loaded[..40], "after vertical blank");
}

/// Verifies REQ-343: after a wide mode E line (W0-W47), a narrow line keeps bytes 32-47 and
/// a normal line keeps bytes 40-47.
#[test]
fn line_buffer_partial_load_keeps_tail() {
    let dl = [
        0x4E, 0x00, 0x20, 0x4E, 0x00, 0x21, 0x4E, 0x00, 0x22, 0x41, 0x00, 0x10,
    ];
    let mut rig = rig(0x23, &dl);
    let _wide = rig.line();
    let wide = buffer(&rig);
    let bytes = |page: u16, count: usize| -> Vec<u8> {
        (0..count)
            .map(|k| byte_at(page + u16::try_from(k).unwrap()))
            .collect()
    };
    assert_eq!(wide[..47], bytes(0x2000, 47)[..], "W0-W46 from memory");
    rig.seek_after(8, 110);
    rig.write(0x00, 0x21);
    rig.seek_after(9, 110);
    let narrow = buffer(&rig);
    assert_eq!(narrow[..32], bytes(0x2100, 32)[..], "narrow: bytes 0-31");
    assert_eq!(narrow[32..], wide[32..], "narrow: W32-W47 kept");
    rig.write(0x00, 0x22);
    rig.seek(11, 0);
    let normal = buffer(&rig);
    assert_eq!(normal[..40], bytes(0x2200, 40)[..], "normal: bytes 0-39");
    assert_eq!(normal[40..], wide[40..], "normal: W40-W47 kept");
}

/// Verifies REQ-343: a mode A line with playfield DMA off on its first scan line and on
/// from cycle 110 shows the old bytes B0-B19 on scan lines 2-4.
#[test]
fn line_buffer_replay_without_dma() {
    // Mode A lines (4 scan lines): B at `$2000` on 8-11, the new line at `$3000` on 12-15.
    let mut rig = rig(
        0x22,
        &[0x4A, 0x00, 0x20, 0x4A, 0x00, 0x30, 0x41, 0x00, 0x10],
    );
    let old = rig.lines(4);
    rig.seek_after(11, 110);
    rig.write(0x00, 0x20);
    rig.seek_after(12, 110);
    rig.write(0x00, 0x22);
    rig.seek(13, 0);
    let new = rig.lines(3);
    let expected = codes(&old[1]);
    let mut b: Vec<u8> = Vec::new();
    for k in 0..20 {
        b.extend(two_bit(byte_at(SCREEN + k), 2));
    }
    assert_eq!(expected[0x30..0xD0], b[..], "B0-B19 on the old line");
    for (n, line) in new.iter().enumerate() {
        assert_eq!(
            codes(line)[0x22..0xDE],
            expected[0x22..0xDE],
            "scan line {} of the new mode line",
            n + 2
        );
    }
}

/// Verifies REQ-343: a mode 8 line with no first-line DMA shows the bytes B0-B9 of an
/// earlier mode E line as mode 8 data on scan lines 2-8 (byte k at `$30 + 16k`).
#[test]
fn line_buffer_reinterpreted_by_mode() {
    let mut rig = rig(0x22, &[0x4E, 0x00, 0x20, 0x08, 0x41, 0x00, 0x10]);
    let _mode_e = rig.line();
    let b = buffer(&rig);
    rig.seek_after(8, 110);
    rig.write(0x00, 0x20);
    rig.seek_after(9, 110);
    rig.write(0x00, 0x22);
    rig.seek(10, 0);
    for line in rig.lines(7) {
        assert!(
            cycles_of(&line, DmaKind::Bitmap).is_empty(),
            "no bitmap DMA"
        );
        let line_codes = codes(&line);
        for (k, &value) in b.iter().enumerate().take(10) {
            let x = 0x30 + 16 * k;
            assert_eq!(
                line_codes[x..x + 16],
                two_bit(value, 4)[..],
                "scan line {}, byte {k}",
                line[0].out.line
            );
        }
    }
}

/// Verifies REQ-343: after a write of `$21` to `DMACTL` on cycle 110 of the first scan line
/// of a normal mode A line, scan line 2 starts B0 at position `$40` and B0-B15 fill
/// `$40-$BF`.
#[test]
fn line_buffer_follows_left_border() {
    let mut rig = rig(0x22, &[0x4A, 0x00, 0x20, 0x41, 0x00, 0x10]);
    rig.seek_after(8, 110);
    rig.write(0x00, 0x21);
    rig.seek(9, 0);
    let line = codes(&rig.line());
    let mut expected: Vec<u8> = Vec::new();
    for k in 0..16 {
        expected.extend(two_bit(byte_at(SCREEN + k), 2));
    }
    assert_eq!(line[0x40..0xC0], expected[..], "B0-B15 at $40-$BF");
    assert!(line[0x22..0x40].iter().all(|&c| c == 0), "background left");
}

/// The first scan line of a mode E line with all bytes `$FF`, `DMACTL` = `from`, and a
/// write of `to` to `DMACTL` on cycle `cycle`. Returns the records and the increase of the
/// memory scan counter.
fn mode_e_dmactl_write(from: u8, to: u8, cycle: u32) -> (Vec<Rec>, u16, Rig) {
    let mut rig = rig(from, &[0x4E, 0x00, 0x20, 0x41, 0x00, 0x10]);
    rig.fill(SCREEN, 64, 0xFF);
    let mut recs = rig.after(8, cycle);
    rig.write(0x00, to);
    recs.extend(rig.run_to(9, 0));
    let count = rig.antic.debug_state().memscan - SCREEN;
    (recs, count, rig)
}

/// Checks that `line` has `code` on `first..=last` and `%000` on the rest of `$22-$DD`.
fn check_span(line: &[Rec], first: usize, last: usize, code: u8) {
    let line_codes = codes(line);
    for (x, &value) in line_codes.iter().enumerate().take(0xDE).skip(0x22) {
        let expected = if (first..=last).contains(&x) { code } else { 0 };
        assert_eq!(value, expected, "position ${x:02X}");
    }
}

/// Verifies REQ-344: a write of `$22` on cycle 50 in a mode E line that starts with
/// `DMACTL` = `$21` gives code `%110` on `$40-$CF` (144 color clocks), `%000` on the rest,
/// and the memory scan counter increases by 36.
#[test]
fn width_change_narrow_to_normal_gives_144() {
    let (line, count, _rig) = mode_e_dmactl_write(0x21, 0x22, 50);
    check_span(&line, 0x40, 0xCF, 0b110);
    assert_eq!(count, 36, "memory scan counter increase");
}

/// Verifies REQ-344: a write of `$21` on cycle 17 in a normal mode E line does not move the
/// start (`$30`), but the playfield stops after `$BF`.
#[test]
fn late_start_ignored() {
    let (line, _count, _rig) = mode_e_dmactl_write(0x22, 0x21, 17);
    check_span(&line, 0x30, 0xBF, 0b110);
}

/// Verifies REQ-344: a write of `$22` on cycle 20 in a narrow mode E line gives no
/// playfield, no `Bitmap` fetch and no memory scan counter change on that scan line.
#[test]
fn early_start_after_deadline_no_playfield() {
    let (line, count, _rig) = mode_e_dmactl_write(0x21, 0x22, 20);
    check_span(&line, 1, 0, 0);
    assert!(
        cycles_of(&line, DmaKind::Bitmap).is_empty(),
        "no bitmap DMA"
    );
    assert_eq!(count, 0, "memory scan counter increase");
}

/// Verifies REQ-344: a write of `$21` on cycle 95 in a normal mode E line gives `Bitmap`
/// fetches on cycles 100, 102 and 104, a virtual fetch on cycle 106, and a memory scan
/// counter increase of 44.
#[test]
fn early_stop_extends_to_wide() {
    let (line, count, _rig) = mode_e_dmactl_write(0x22, 0x21, 95);
    assert_eq!(
        cycles_of(&line, DmaKind::Bitmap),
        every(20, 104, 2),
        "bitmap DMA cycles"
    );
    assert!(!at(&line, 106).out.halt, "no halt on cycle 106");
    assert_eq!(
        count, 44,
        "memory scan counter increase (virtual fetch on 106)"
    );
}

/// Verifies REQ-344: a write of `$20` on cycle 50 in a normal mode E line stops the `Bitmap`
/// fetches after cycle 51, gives `%000` on `$70-$CF`, a counter increase of 40, and line
/// buffer bytes 16-39 from the data bus of cycles 52-98.
#[test]
fn playfield_off_mid_line_counts_on() {
    let bus = |cycle: u32| cycle.to_le_bytes()[0].wrapping_mul(3) ^ 0x5A;
    let mut rig = rig(0x22, &[0x4E, 0x00, 0x20, 0x41, 0x00, 0x10]);
    let mut line = Vec::new();
    for cycle in 0..114 {
        if cycle == 51 {
            rig.write(0x00, 0x20);
        }
        rig.bus = bus(cycle);
        line.push(rig.step());
    }
    let bitmap = cycles_of(&line, DmaKind::Bitmap);
    assert_eq!(bitmap, every(20, 50, 2), "bitmap DMA cycles");
    let line_codes = codes(&line);
    assert!(
        line_codes[0x70..0xD0].iter().all(|&c| c == 0),
        "background on $70-$CF"
    );
    assert_eq!(
        rig.antic.debug_state().memscan - SCREEN,
        40,
        "counter increase"
    );
    let lb = buffer(&rig);
    for (k, &value) in lb.iter().enumerate().take(40) {
        let cycle = 20 + 2 * u32::try_from(k).unwrap();
        let expected = if k < 16 {
            byte_at(SCREEN + u16::try_from(k).unwrap())
        } else {
            bus(cycle)
        };
        assert_eq!(value, expected, "byte {k} (cycle {cycle})");
    }
}

/// Verifies REQ-344: a write of `$20` on cycle 10 of the first scan line of a normal mode E
/// line changes neither the memory scan counter nor the line buffer.
#[test]
fn playfield_off_before_start_no_change() {
    let mut rig = rig(0x22, &[0x4E, 0x00, 0x20, 0x41, 0x00, 0x10]);
    rig.seek_after(8, 10);
    let before = rig.antic.debug_state();
    rig.write(0x00, 0x20);
    rig.seek(9, 0);
    let after = rig.antic.debug_state();
    assert_eq!(after.memscan, before.memscan, "memory scan counter");
    assert_eq!(after.memscan, SCREEN, "memory scan counter from the LMS");
    assert_eq!(after.line_buffer, before.line_buffer, "line buffer");
}

/// Verifies REQ-344: a write of `$01` to `HSCROL` on cycle 60 of a `$1F` line gives the same
/// `out.dma`, and the code at x equals the code at x - 1 with no write, for x from `$80` to
/// `$CF`.
#[test]
fn hscrol_bit0_mid_line() {
    let run = |write: bool| {
        let mut rig = rig(0x22, &[0x5F, 0x00, 0x20, 0x41, 0x00, 0x10]);
        let mut recs = rig.after(8, 60);
        if write {
            rig.write(0x04, 0x01);
        }
        recs.extend(rig.run_to(9, 0));
        recs
    };
    let (plain, written) = (run(false), run(true));
    let dma = |line: &[Rec]| line.iter().map(|r| r.out.dma).collect::<Vec<_>>();
    assert_eq!(dma(&written), dma(&plain), "the DMA cycles");
    let (plain, written) = (codes(&plain), codes(&written));
    for x in 0x80..=0xCF {
        assert_eq!(written[x], plain[x - 1], "position ${x:02X}");
    }
    assert_ne!(written[0x80..0xD0], plain[0x80..0xD0], "the codes move");
}

/// Verifies REQ-345: a write of `$04` to `HSCROL` on cycle 50 of a `$18` line gives the DMA
/// clock values of the criterion on that scan line and the next two.
#[test]
#[ignore = "planned: SLICE-02"]
fn abnormal_dma_hscrol_0_to_4_mode_8() {
    planned();
}

/// A rig in an abnormal DMA state at scan line 9: the `$5x` line `mode` (mode E or 2) on
/// scan line 8 starts with `HSCROL` = 0, and the test writes `$02` to `HSCROL` on cycle 50.
/// The stop event of scan line 8 then clears no bit. `dl` follows the first line.
fn abnormal_rig(first: u8, dl: &[u8]) -> Rig {
    let mut list = vec![first, 0x00, 0x20];
    list.extend(dl);
    list.extend([0x41, 0x00, 0x10]);
    let mut rig = rig(0x22, &list);
    rig.seek_after(8, 50);
    rig.write(0x04, 0x02);
    rig.seek(9, 0);
    rig
}

/// Verifies REQ-345: two DMA requests on one cycle give one read at the bitwise AND of the
/// addresses, both get that byte, and no refresh occurs on that cycle.
#[test]
fn overlap_uses_address_and() {
    // Scan line 8: `$72` with `VSCROL` = 7 (one scan line). Scan line 9: the first scan
    // line of a `$12` line with name and data fetches on every cycle.
    let mut rig = Rig::with_dl(0x22, &[0x72, 0x00, 0x20, 0x12, 0x41, 0x00, 0x10]);
    for k in 0..0x400_u16 {
        let low = k.to_le_bytes()[0];
        rig.mem.ram[usize::from(SCREEN + k)] = low.wrapping_mul(29) & 0x7F;
        rig.mem.ram[usize::from(FONT + k)] = low.wrapping_mul(11) ^ 0x3C;
    }
    // On cycle 1, the instruction fetch (`$1003`) and a character data request (row 7)
    // read `$0003`: put the instruction `$12` there.
    rig.mem.ram[3] = 0x12;
    rig.write(0x05, 7);
    rig.seek_after(8, 50);
    rig.write(0x04, 0x02);
    rig.seek(9, 0);
    let mut overlaps = 0;
    for _ in 0..114 {
        rig.mem.log.clear();
        let out = rig.antic.tick(&mut rig.mem);
        let requests: Vec<u16> = rig.antic.requests.iter().flatten().copied().collect();
        let log = rig.mem.log.clone();
        let data = if out.halt && out.dma != DmaKind::Refresh {
            out.dma_data
        } else {
            0xFF
        };
        rig.antic.end_cycle(data);
        if requests.len() < 2 {
            continue;
        }
        overlaps += 1;
        let addr = requests.iter().fold(0xFFFF, |acc, &a| acc & a);
        let at = format!("cycle {}: {requests:04X?}", out.cycle);
        assert_eq!(log, [addr], "{at}: one read at the AND");
        assert_eq!(out.dma_data, rig.mem.ram[usize::from(addr)], "{at}: data");
        assert_ne!(out.dma, DmaKind::Refresh, "{at}: no refresh");
        if out.cycle == 1 {
            assert_eq!(
                rig.antic.debug_state().ir,
                out.dma_data,
                "{at}: IR gets the byte"
            );
        }
    }
    assert!(overlaps > 40, "overlapping requests: {overlaps}");
}

/// Verifies REQ-345: after the abnormal state, the DMA clock is 0 after cycle 1 of a `$00`
/// or `$01` instruction line and after cycle 0 of scan line 248.
#[test]
fn blank_line_clears_dma_clock() {
    for blank in [vec![0x00], vec![0x01, 0x06, 0x10]] {
        let mut rig = abnormal_rig(0x5E, &blank);
        rig.skip(1);
        assert_ne!(
            rig.antic.debug_state().dma_clock,
            0,
            "${:02X}: tick 0",
            blank[0]
        );
        rig.skip(1);
        assert_eq!(
            rig.antic.debug_state().dma_clock,
            0,
            "${:02X}: tick 1",
            blank[0]
        );
    }
    // 239 `$5E` lines with the abnormal state from scan line 8 to 247.
    let mut rig = abnormal_rig(0x5E, &[0x1E; 239]);
    rig.seek(VBLANK_START_LINE - 1, 113);
    rig.skip(1);
    assert_ne!(rig.antic.debug_state().dma_clock, 0, "line 247, tick 113");
    rig.skip(1);
    assert_eq!(rig.antic.debug_state().dma_clock, 0, "line 248, tick 0");
}

/// The memory scan counter after the `$08` line that follows the `$5x` line `ir` (LMS
/// `$3000`) with `HSCROL` = `hscrol` and `DMACTL` = `dmactl`.
fn memscan_after_mode_8(dmactl: u8, ir: u8, hscrol: u8) -> u16 {
    let mut rig = Rig::with_dl(dmactl, &[ir, 0x00, 0x30, 0x08, 0x41, 0x00, 0x10]);
    rig.write(0x04, hscrol);
    rig.seek(17, 0);
    rig.antic.debug_state().memscan
}

/// Verifies REQ-345: a `$08` line after a `$5E $00 $30` line with `HSCROL` = 10 leaves a
/// memory scan counter that is not `$303A` (with 8 it is), and after a `$5A` line with 14
/// one that is not `$3022` (with 12 it is).
#[test]
fn mode_8_after_scrolled_mode_2_bug() {
    assert_ne!(
        memscan_after_mode_8(0x22, 0x5E, 10),
        0x303A,
        "$5E, HSCROL 10"
    );
    assert_eq!(memscan_after_mode_8(0x22, 0x5E, 8), 0x303A, "$5E, HSCROL 8");
    assert_ne!(
        memscan_after_mode_8(0x22, 0x5A, 14),
        0x3022,
        "$5A, HSCROL 14"
    );
    assert_eq!(
        memscan_after_mode_8(0x22, 0x5A, 12),
        0x3022,
        "$5A, HSCROL 12"
    );
}

/// Verifies REQ-345: with `DMACTL` = `$21` and `HSCROL` = 10, a `$08` line after a
/// `$5E $00 $30` line leaves the memory scan counter `$3030`.
#[test]
fn narrow_width_no_mode_8_bug() {
    assert_eq!(memscan_after_mode_8(0x21, 0x5E, 10), 0x3030, "narrow");
}

/// Verifies REQ-345: with more than 48 line buffer accesses on one scan line, accesses
/// 49-63 do not change the buffer and read `$FF`, and access 64 uses byte 0 again.
#[test]
fn line_buffer_counter_63_states() {
    // Scan line 9 is the first scan line of a `$1D` line (two scan lines) with a bitmap
    // access on every cycle from 12; scan line 10 replays it.
    let mut rig = abnormal_rig(0x5E, &[0x1D]);
    let before = buffer(&rig);
    let first = rig.line();
    let accesses: Vec<Rec> = first
        .iter()
        .filter(|rec| rec.out.cycle > 10 && rec.out.dma == DmaKind::Bitmap)
        .copied()
        .collect();
    let cycles: Vec<u32> = accesses.iter().map(|rec| rec.out.cycle).collect();
    assert_eq!(
        cycles,
        every(12, 105, 1),
        "a bitmap fetch on each cycle 12-105"
    );
    let lb = buffer(&rig);
    for k in 0..31 {
        assert_eq!(
            lb[k],
            accesses[63 + k].out.dma_data,
            "byte {k}: access {}",
            64 + k
        );
    }
    for k in 36..48 {
        assert_eq!(
            lb[k],
            accesses[k].out.dma_data,
            "byte {k}: access {}",
            k + 1
        );
    }
    assert_ne!(lb, before, "the first scan line loads the buffer");
    // Scan line 10: the same accesses read the line buffer. Mode D shows the first two
    // pixels of each byte at 2t + 8 before the next byte.
    let replay = codes(&rig.line());
    for (j, &cycle) in cycles.iter().enumerate().take(64) {
        let x = 2 * usize::try_from(cycle).unwrap() + 8;
        if x < 0x30 {
            continue;
        }
        let value = match j {
            0..48 => lb[j],
            48..63 => 0xFF,
            _ => lb[j - 63],
        };
        assert_eq!(
            replay[x..x + 2],
            two_bit(value, 1)[..2],
            "access {} on cycle {cycle}",
            j + 1
        );
    }
}

/// A frame with a hi-res IR at scan line 248: blank lines, then `last` from scan line 240
/// (a `$4F` line and 7 `$0F` lines, or a `$60` line and a JVB on 247).
fn vblank_rig(standard: VideoStandard, dmactl: u8, last: &[u8]) -> Rig {
    let mut dl = vec![0x70; 29];
    dl.extend(last);
    dl.extend([0x41, 0x00, 0x10]);
    let mut rig = Rig::new(standard);
    rig.start_dl(dmactl, &dl);
    for addr in 0x2000..0x2100 {
        rig.mem.ram[usize::from(addr)] = byte_at(addr);
    }
    rig
}

/// The mode F lines on scan lines 240-247.
fn mode_f_last() -> Vec<u8> {
    let mut last = vec![0x4F, 0x00, 0x20];
    last.extend([0x0F; 7]);
    last
}

/// The codes at position `x` on the scan lines of `recs`, by scan line.
fn code_at(recs: &[Rec], line: u32, x: usize) -> u8 {
    codes(&on_line(recs, line))[x]
}

/// Verifies REQ-346: with IR = `$0F` at scan line 248 (NTSC) and `DMACTL` = `$22`, position
/// `$80` has code `%111` instead of `%011` on the vertical blank lines, and positions
/// `$00-$21` keep `%011`.
#[test]
fn hires_ir_at_248_opens_border() {
    let mut rig = vblank_rig(VideoStandard::Ntsc, 0x22, &mode_f_last());
    rig.seek(VBLANK_START_LINE, 0);
    assert_eq!(rig.antic.debug_state().ir, 0x0F, "IR at scan line 248");
    let recs = rig.run_to(8, 0);
    for line in (248..=250).chain(254..262).chain(0..8) {
        assert_eq!(code_at(&recs, line, 0x80), 0b111, "line {line}");
    }
    for line in (248..=250).chain(254..262) {
        let line_codes = codes(&on_line(&recs, line));
        assert!(
            line_codes[..0x22].iter().all(|&c| c == 0b011),
            "line {line}: $00-$21"
        );
    }
}

/// Verifies REQ-346: in the same state, position `$80` has code `%101` instead of `%001` on
/// scan lines 251-253.
#[test]
fn hires_bug_corrupts_vsync_code() {
    let mut rig = vblank_rig(VideoStandard::Ntsc, 0x22, &mode_f_last());
    rig.seek(VBLANK_START_LINE, 0);
    let recs = rig.run_to(0, 0);
    for line in 251..=253 {
        assert_eq!(code_at(&recs, line, 0x80), 0b101, "line {line}");
    }
}

/// Verifies REQ-346: in the same state on PAL, position `$80` has code `%101` on scan lines
/// 275-277.
#[test]
fn hires_bug_pal_vsync() {
    let mut rig = vblank_rig(VideoStandard::Pal, 0x22, &mode_f_last());
    rig.seek(VBLANK_START_LINE, 0);
    assert_eq!(rig.antic.debug_state().ir, 0x0F, "IR at scan line 248");
    let recs = rig.run_to(0, 0);
    for line in 275..=277 {
        assert_eq!(code_at(&recs, line, 0x80), 0b101, "line {line}");
    }
    for line in (248..275).chain(278..312) {
        assert_eq!(code_at(&recs, line, 0x80), 0b111, "line {line}");
    }
}

/// Verifies REQ-346: with IR = `$41` at scan line 248 and `DMACTL` = `$22`, position `$80`
/// has code `%010` on lines 248-250 and `%001` on lines 251-253.
#[test]
fn jvb_at_248_no_bug() {
    let mut rig = vblank_rig(VideoStandard::Ntsc, 0x22, &[0x60, 0x41, 0x00, 0x10]);
    rig.seek(VBLANK_START_LINE - 1, 2);
    assert_eq!(rig.antic.debug_state().ir, 0x41, "IR on scan line 247");
    rig.seek(VBLANK_START_LINE, 0);
    let recs = rig.run_to(0, 0);
    for line in 248..=250 {
        assert_eq!(code_at(&recs, line, 0x80), 0b010, "line {line}");
    }
    for line in 251..=253 {
        assert_eq!(code_at(&recs, line, 0x80), 0b001, "line {line}");
    }
}

/// Verifies REQ-346: a write of `$20` to `DMACTL` on scan line 250 gives position `$80` code
/// `%001` on lines 251-253 and `%011` on lines 254-261.
#[test]
fn hires_bug_ends_when_playfield_off() {
    let mut rig = vblank_rig(VideoStandard::Ntsc, 0x22, &mode_f_last());
    rig.seek(250, 50);
    rig.write(0x00, 0x20);
    let recs = rig.run_to(0, 0);
    for line in 251..=253 {
        assert_eq!(code_at(&recs, line, 0x80), 0b001, "line {line}");
    }
    for line in 254..262 {
        assert_eq!(code_at(&recs, line, 0x80), 0b011, "line {line}");
    }
}

/// Verifies REQ-346: with IR = `$0F` at scan line 248 and `DMACTL` = `$20`, position `$80`
/// has code `%011` on lines 248-250 and `%001` on lines 251-253.
#[test]
fn hires_ir_with_playfield_off_no_bug() {
    let mut rig = vblank_rig(VideoStandard::Ntsc, 0x20, &mode_f_last());
    rig.seek(VBLANK_START_LINE, 0);
    assert_eq!(rig.antic.debug_state().ir, 0x0F, "IR at scan line 248");
    let recs = rig.run_to(0, 0);
    for line in 248..=250 {
        assert_eq!(code_at(&recs, line, 0x80), 0b011, "line {line}");
    }
    for line in 251..=253 {
        assert_eq!(code_at(&recs, line, 0x80), 0b001, "line {line}");
    }
}

/// Verifies REQ-346: with IR = `$0F` at scan line 248, the `%111` codes on scan line 248
/// cover the playfield positions of the `DMACTL` width: `$30-$CF` with `$22`, `$40-$BF` with
/// `$21` (and `$2C-$DD` with `$23`, OI-015). The other positions of `$22-$DD` have `%011`.
#[test]
fn hires_bug_region_follows_width() {
    for (dmactl, first, last) in [(0x22, 0x30, 0xCF), (0x21, 0x40, 0xBF), (0x23, 0x2C, 0xDD)] {
        let mut rig = vblank_rig(VideoStandard::Ntsc, dmactl, &mode_f_last());
        rig.seek(VBLANK_START_LINE, 0);
        let line = codes(&rig.line());
        for (x, &code) in line.iter().enumerate().take(0xDE).skip(0x22) {
            let expected = if (first..=last).contains(&x) {
                0b111
            } else {
                0b011
            };
            assert_eq!(code, expected, "DMACTL ${dmactl:02X} position ${x:02X}");
        }
    }
}

/// Verifies REQ-347: on the first scan line of a `$12` line with `DMACTL` = `$23` and
/// `HSCROL` = 2, with `$5A` on the data bus in cycle 106 (refresh and virtual fetch),
/// positions `$DA-$DD` have the codes `%101`, `%101`, `%110` and `%110`.
#[test]
fn virtual_fetch_with_refresh_uses_data_bus() {
    let mut rig = rig(0x23, &[0x52, 0x00, 0x20, 0x41, 0x00, 0x10]);
    rig.write(0x04, 0x02);
    let line = line_with_bus(&mut rig, 106, 0x5A);
    let rec = at(&line, 106);
    assert_eq!(rec.out.dma, DmaKind::Refresh, "refresh on cycle 106");
    assert_eq!(
        cycles_of(&line, DmaKind::Refresh),
        [106],
        "the only refresh"
    );
    assert_eq!(
        codes(&line)[0xDA..0xDE],
        [0b101, 0b101, 0b110, 0b110],
        "$DA-$DD"
    );
}
