//! Player and missile graphics tests: data, DMA, position, size and shifting (REQ-413 to
//! REQ-419, REQ-441).

use super::objects::{
    Dma, background, gtia_with, hpos_with, no_dma, plain_frame, pm_gtia, run_frame, span,
};
use super::{COLBK, COLPM0, GRACTL, at, row};
use crate::antic::DmaKind;
use crate::gtia::{FRAME_HEIGHT, Gtia};
use crate::timing::VideoStandard;

/// `HPOSP0`.
const HPOSP0: u8 = 0x00;
/// `HPOSM0`.
const HPOSM0: u8 = 0x04;
/// `SIZEP0`.
const SIZEP0: u8 = 0x08;
/// `SIZEM`.
const SIZEM: u8 = 0x0C;
/// `GRAFP0`.
const GRAFP0: u8 = 0x0D;
/// `GRAFM`.
const GRAFM: u8 = 0x11;
/// `VDELAY`.
const VDELAY: u8 = 0x1C;

/// A frame with the writes `setup`, `COLBK` = `$02` and no playfield.
fn frame(setup: &[(u8, u8)]) -> Gtia {
    let mut gtia = gtia_with(&[(COLBK, 0x02)]);
    for &(reg, value) in setup {
        gtia.write(reg, value);
    }
    plain_frame(&mut gtia, false, &background);
    gtia
}

/// A frame with player 0 (`COLPM0` = `$46`, `COLBK` = `$02`, `setup` first) and one CPU write
/// on `cycle` of scan line 100 (row 92).
fn frame_with_write(setup: &[(u8, u8)], cycle: u32, write: (u8, u8)) -> Gtia {
    let mut gtia = gtia_with(&[(COLBK, 0x02), (COLPM0, 0x46)]);
    for &(reg, value) in setup {
        gtia.write(reg, value);
    }
    run_frame(&mut gtia, false, &background, &no_dma, &mut |gtia, l, c| {
        if (l, c) == (100, cycle) {
            gtia.write(write.0, write.1);
        }
    });
    gtia
}

/// The HPOS values in `$22-$DD` of `row` that show `$46`.
fn player_hpos(gtia: &Gtia, row: usize) -> Vec<usize> {
    hpos_with(gtia, row, 0x46)
}

/// Verifies REQ-413: `GRAFP0` = `$81`, `HPOSP0` = `$80`, `SIZEP0` = `$00`, `COLPM0` = `$46`
/// show `$46` at HPOS `$80` and `$87` and `$02` at HPOS `$81-$86` (criterion 1).
#[test]
fn player_bits_msb_first() {
    let gtia = frame(&[
        (GRAFP0, 0x81),
        (HPOSP0, 0x80),
        (SIZEP0, 0x00),
        (COLPM0, 0x46),
    ]);
    for r in [0, 100, 239] {
        assert_eq!(at(&gtia, r, 0x80), [0x46; 2], "row {r} HPOS $80");
        assert_eq!(at(&gtia, r, 0x87), [0x46; 2], "row {r} HPOS $87");
        assert_eq!(
            span(&gtia, r, 0x81..=0x86),
            [0x02; 12],
            "row {r} HPOS $81-$86"
        );
    }
}

/// Verifies REQ-413: missile n shows bits `2n+1` and `2n` of `GRAFM` in `COLPMn`, the most
/// significant bit at the left (criteria 2 and 3).
#[test]
fn missile_bits_and_colors() {
    let gtia = frame(&[
        (GRAFM, 0x0C),
        (HPOSM0 + 1, 0x90),
        (SIZEM, 0x00),
        (0x13, 0x94),
    ]);
    assert_eq!(
        span(&gtia, 100, 0x90..=0x91),
        [0x94; 4],
        "missile 1 at $90-$91"
    );
    assert_eq!(at(&gtia, 100, 0x92), [0x02; 2], "missile 1 ends at $91");
    let gtia = frame(&[(GRAFM, 0x02), (HPOSM0, 0xA0), (SIZEM, 0x00), (COLPM0, 0x46)]);
    assert_eq!(at(&gtia, 100, 0xA0), [0x46; 2], "missile 0 bit 1 at $A0");
    assert_eq!(at(&gtia, 100, 0xA1), [0x02; 2], "missile 0 bit 0 at $A1");
}

/// Verifies REQ-413: players 1-3 and missiles 2-3 at different positions show `COLPM1-3`
/// (criterion 4).
#[test]
fn all_object_colors() {
    let gtia = frame(&[
        (0x13, 0x16),
        (0x14, 0x26),
        (0x15, 0x36),
        (GRAFP0 + 1, 0x80),
        (GRAFP0 + 2, 0x80),
        (GRAFP0 + 3, 0x80),
        (HPOSP0 + 1, 0x50),
        (HPOSP0 + 2, 0x60),
        (HPOSP0 + 3, 0x70),
        (GRAFM, 0xF0),
        (HPOSM0 + 2, 0xA0),
        (HPOSM0 + 3, 0xB0),
    ]);
    for (h, color) in [(0x50, 0x16), (0x60, 0x26), (0x70, 0x36)] {
        assert_eq!(at(&gtia, 100, h), [color; 2], "player at ${h:02X}");
        assert_eq!(
            at(&gtia, 100, h + 1),
            [0x02; 2],
            "after the player at ${h:02X}"
        );
    }
    assert_eq!(span(&gtia, 100, 0xA0..=0xA1), [0x26; 4], "missile 2");
    assert_eq!(span(&gtia, 100, 0xB0..=0xB1), [0x36; 4], "missile 3");
    assert_eq!(at(&gtia, 100, 0xA2), [0x02; 2], "after missile 2");
    assert_eq!(at(&gtia, 100, 0xB2), [0x02; 2], "after missile 3");
}

/// Verifies REQ-413: with P/M DMA off and one write of `$80` to `GRAFP0` before scanline 8,
/// columns 192-193 of all 240 rows are `COLPM0` (criterion 5).
#[test]
fn latch_repeats_without_dma() {
    let mut gtia = gtia_with(&[(COLBK, 0x02), (HPOSP0, 0x80), (COLPM0, 0x46)]);
    run_frame(&mut gtia, false, &background, &no_dma, &mut |gtia, l, c| {
        if (l, c) == (4, 50) {
            gtia.write(GRAFP0, 0x80);
        }
    });
    for r in 0..FRAME_HEIGHT {
        assert_eq!(row(&gtia, r)[192..194], [0x46; 2], "row {r}");
    }
}

/// The P/M DMA of REQ-414 criterion 1: a missile byte `$00` and a player 0 byte of `$80` on
/// even scan lines and `$40` on odd scan lines.
fn even_odd_dma(line: u32, cycle: u32) -> (DmaKind, u8) {
    match cycle {
        0 => (DmaKind::Missile, 0x00),
        2 => (
            DmaKind::Player(0),
            if line.is_multiple_of(2) { 0x80 } else { 0x40 },
        ),
        _ => (DmaKind::None, 0xFF),
    }
}

/// Verifies REQ-414: writes `$00` to `GRACTL` in cycle 110 of scanline 99 and checks that
/// rows 92-239 all show `$46` at HPOS `$81` only (criterion 2).
#[test]
fn gractl_off_keeps_latch() {
    let mut gtia = pm_gtia();
    gtia.write(GRACTL, 0x03);
    run_frame(
        &mut gtia,
        false,
        &background,
        &even_odd_dma,
        &mut |gtia, l, c| {
            if (l, c) == (99, 110) {
                gtia.write(GRACTL, 0x00);
            }
        },
    );
    assert_eq!(
        player_hpos(&gtia, 90),
        [0x80],
        "row 90: the DMA byte of an even line"
    );
    assert_eq!(
        player_hpos(&gtia, 91),
        [0x81],
        "row 91: the DMA byte of an odd line"
    );
    for r in 92..FRAME_HEIGHT {
        assert_eq!(player_hpos(&gtia, r), [0x81], "row {r}");
    }
}

/// Verifies REQ-414: with `GRACTL` = `$02`, a CPU write of `$03` to `GRAFM` before scanline 8
/// and a missile DMA byte of `$00` on each scanline, HPOS `$90-$91` show `$46` on all rows
/// (criterion 3).
#[test]
fn missile_dma_needs_gractl_bit_0() {
    let mut gtia = pm_gtia();
    gtia.write(GRACTL, 0x02);
    gtia.write(GRAFM, 0x03);
    let dma = |_, cycle| match cycle {
        0 => (DmaKind::Missile, 0x00),
        _ => (DmaKind::None, 0xFF),
    };
    run_frame(&mut gtia, false, &background, &dma, &mut |_, _, _| {});
    for r in 0..FRAME_HEIGHT {
        assert_eq!(span(&gtia, r, 0x90..=0x91), [0x46; 4], "row {r}");
    }
}

/// Verifies REQ-414: with `GRACTL` = `$00` and again with `$03`, a CPU write of `$80` to
/// `GRAFP0` in horizontal blank after the player DMA cycle shows the player on that scanline
/// (criterion 5).
#[test]
fn cpu_writes_ignore_gractl() {
    for gractl in [0x00, 0x03] {
        let mut gtia = pm_gtia();
        gtia.write(GRACTL, gractl);
        let dma = |_, cycle| match cycle {
            2 => (DmaKind::Player(0), 0x00),
            _ => (DmaKind::None, 0xFF),
        };
        run_frame(&mut gtia, false, &background, &dma, &mut |gtia, l, c| {
            if (l, c) == (100, 10) {
                gtia.write(GRAFP0, 0x80);
            }
        });
        assert_eq!(player_hpos(&gtia, 91), [], "GRACTL ${gractl:02X}: row 91");
        assert_eq!(
            player_hpos(&gtia, 92),
            [0x80],
            "GRACTL ${gractl:02X}: row 92"
        );
    }
}

/// The objects of `vdelay_bit_map`: `(VDELAY bit, HPOS of the even-line image, HPOS of the
/// odd-line image)`. Missiles 0-3 are bits 0-3, players 0-3 are bits 4-7.
const VDELAY_OBJECTS: [(u8, usize, usize); 8] = [
    (0, 0xC0, 0xC1),
    (1, 0xC4, 0xC5),
    (2, 0xC8, 0xC9),
    (3, 0xCC, 0xCD),
    (4, 0x40, 0x41),
    (5, 0x50, 0x51),
    (6, 0x60, 0x61),
    (7, 0x70, 0x71),
];

/// Verifies REQ-415: sets each `VDELAY` bit alone and checks that only the related object
/// skips the even-scanline DMA byte (criterion 1), and that with `VDELAY` = `$00` each
/// scanline shows its own DMA byte for all objects (criterion 4).
#[test]
fn vdelay_bit_map() {
    // Even lines: players `$80` and missiles `%10` (left bit); odd lines: players `$40` and
    // missiles `%01` (right bit).
    let dma = |line: u32, cycle| {
        let even = line.is_multiple_of(2);
        match cycle {
            0 => (DmaKind::Missile, if even { 0xAA } else { 0x55 }),
            2..=5 => (
                DmaKind::Player(u8::try_from(cycle - 2).unwrap()),
                if even { 0x80 } else { 0x40 },
            ),
            _ => (DmaKind::None, 0xFF),
        }
    };
    for vdelay in [0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80] {
        let mut gtia = gtia_with(&[(COLBK, 0x02), (GRACTL, 0x03), (VDELAY, vdelay)]);
        for n in 0..4 {
            gtia.write(HPOSP0 + n, 0x40 + 0x10 * n);
            gtia.write(HPOSM0 + n, 0xC0 + 4 * n);
            gtia.write(COLPM0 + n, 0x46);
        }
        run_frame(&mut gtia, false, &background, &dma, &mut |_, _, _| {});
        for line in 10..=247_usize {
            for (bit, even_h, odd_h) in VDELAY_OBJECTS {
                let delayed = vdelay & (1 << bit) != 0 && line.is_multiple_of(2);
                let own = if line.is_multiple_of(2) {
                    even_h
                } else {
                    odd_h
                };
                let shown = if delayed { odd_h } else { own };
                let hidden = if shown == even_h { odd_h } else { even_h };
                let r = line - 8;
                assert_eq!(
                    at(&gtia, r, shown),
                    [0x46; 2],
                    "VDELAY ${vdelay:02X} bit {bit} line {line}"
                );
                assert_eq!(
                    at(&gtia, r, hidden),
                    [0x02; 2],
                    "VDELAY ${vdelay:02X} bit {bit} line {line}"
                );
            }
        }
    }
}

/// Verifies REQ-415: with `VDELAY` = `$10` and a player 0 DMA byte of `$00`, a CPU write of
/// `$80` to `GRAFP0` in horizontal blank of an even scanline shows the player at HPOS `$80`
/// (criterion 3).
#[test]
fn vdelay_ignores_cpu_writes() {
    let mut gtia = pm_gtia();
    gtia.write(GRACTL, 0x02);
    gtia.write(VDELAY, 0x10);
    let dma = |_, cycle| match cycle {
        2 => (DmaKind::Player(0), 0x00),
        _ => (DmaKind::None, 0xFF),
    };
    run_frame(&mut gtia, false, &background, &dma, &mut |gtia, l, c| {
        if (l, c) == (100, 10) {
            gtia.write(GRAFP0, 0x80);
        }
    });
    assert_eq!(player_hpos(&gtia, 92), [0x80], "scan line 100");
    assert_eq!(
        player_hpos(&gtia, 93),
        [],
        "scan line 101 loads the DMA byte"
    );
}

/// Verifies REQ-416: `GRAFP0` = `$80` and `HPOSP0` = `$80` show `$46` at HPOS `$80` (columns
/// 192-193) and `$02` at HPOS `$7F` and `$81` (criterion 1).
#[test]
fn player_starts_at_hpos() {
    let gtia = frame(&[(GRAFP0, 0x80), (HPOSP0, 0x80), (COLPM0, 0x46)]);
    assert_eq!(row(&gtia, 100)[192..194], [0x46; 2], "columns 192-193");
    assert_eq!(at(&gtia, 100, 0x7F), [0x02; 2], "HPOS $7F");
    assert_eq!(at(&gtia, 100, 0x81), [0x02; 2], "HPOS $81");
}

/// Verifies REQ-416: `HPOSP0` = `$1E`, `$DA` and `$DE` clip the image at the left and right
/// edges of `$22-$DD`, and the image does not continue on the next scanline (criteria 2-4).
#[test]
fn player_clipped_at_left_and_right() {
    let setup = |h| [(GRAFP0, 0xFF), (SIZEP0, 0x00), (COLPM0, 0x46), (HPOSP0, h)];
    let left = frame(&setup(0x1E));
    let right = frame(&setup(0xDA));
    let off = frame(&setup(0xDE));
    for r in 0..FRAME_HEIGHT {
        assert_eq!(row(&left, r)[4..12], [0x46; 8], "$1E: row {r} columns 4-11");
        assert_eq!(
            player_hpos(&left, r),
            (0x22..=0x25).collect::<Vec<_>>(),
            "$1E: row {r}"
        );
        assert_eq!(
            row(&right, r)[372..380],
            [0x46; 8],
            "$DA: row {r} columns 372-379"
        );
        assert_eq!(
            player_hpos(&right, r),
            (0xDA..=0xDD).collect::<Vec<_>>(),
            "$DA: row {r}"
        );
        assert_eq!(player_hpos(&off, r), [], "$DE: row {r}");
    }
}

/// Verifies REQ-417: `SIZEP0` = `$00`, `$01`, `$03`, `$02` show each bit for 1, 2, 4 and 1
/// color clocks, and `GRAFP0` = `$FF` at quadruple size covers HPOS `$40-$5F` (criteria 1 and
/// 2).
#[test]
fn player_sizes() {
    for (size, width) in [(0x00, 1), (0x01, 2), (0x03, 4), (0x02, 1)] {
        let gtia = frame(&[
            (GRAFP0, 0x80),
            (HPOSP0, 0x80),
            (SIZEP0, size),
            (COLPM0, 0x46),
        ]);
        let expected: Vec<usize> = (0x80..0x80 + width).collect();
        assert_eq!(player_hpos(&gtia, 100), expected, "SIZEP0 ${size:02X}");
    }
    let gtia = frame(&[
        (GRAFP0, 0xFF),
        (HPOSP0, 0x40),
        (SIZEP0, 0x03),
        (COLPM0, 0x46),
    ]);
    assert_eq!(
        player_hpos(&gtia, 100),
        (0x40..=0x5F).collect::<Vec<_>>(),
        "$FF at 4x"
    );
    assert_eq!(at(&gtia, 100, 0x60), [0x02; 2], "HPOS $60");
}

/// Verifies REQ-417: `GRAFM` = `$08`, `HPOSM1` = `$90` and `SIZEM` = `$0C` show `COLPM1` at
/// HPOS `$90-$93` and `COLBK` at HPOS `$94` (criterion 3).
#[test]
fn missile_sizes() {
    let gtia = frame(&[
        (GRAFM, 0x08),
        (HPOSM0 + 1, 0x90),
        (SIZEM, 0x0C),
        (0x13, 0x94),
    ]);
    assert_eq!(
        hpos_with(&gtia, 100, 0x94),
        (0x90..=0x93).collect::<Vec<_>>(),
        "missile 1"
    );
    assert_eq!(at(&gtia, 100, 0x94), [0x02; 2], "HPOS $94");
}

/// Verifies REQ-417: compares the rows for `SIZEP0` = `$FC` and `$00`, and `$FD` and `$01`
/// (criterion 4).
#[test]
fn size_bits_2_7_ignored() {
    let line = |size| {
        let gtia = frame(&[
            (GRAFP0, 0x80),
            (HPOSP0, 0x80),
            (SIZEP0, size),
            (COLPM0, 0x46),
        ]);
        row(&gtia, 100).to_vec()
    };
    assert_eq!(line(0xFC), line(0x00), "$FC and $00");
    assert_eq!(line(0xFD), line(0x01), "$FD and $01");
    assert_ne!(line(0x00), line(0x01), "the sizes differ");
}

/// Verifies REQ-418: `HPOSP0` = `$40` and a write of `$A0` that takes effect at HPOS `$67`
/// show the player at HPOS `$40-$47` and `$A0-$A7` only (criterion 1).
#[test]
fn retrigger_shows_second_copy() {
    let gtia = frame_with_write(&[(GRAFP0, 0xFF), (HPOSP0, 0x40)], 50, (HPOSP0, 0xA0));
    let expected: Vec<usize> = (0x40..=0x47).chain(0xA0..=0xA7).collect();
    assert_eq!(player_hpos(&gtia, 92), expected, "two copies");
    assert_eq!(
        hpos_with(&gtia, 92, 0x02).len(),
        188 - 16,
        "COLBK elsewhere"
    );
}

/// Verifies REQ-418: `HPOSP0` = `$A0` and a write of `$40` that takes effect at HPOS `$67`
/// show no player on that scanline (criterion 2).
#[test]
fn move_left_suppresses_image() {
    let gtia = frame_with_write(&[(GRAFP0, 0xFF), (HPOSP0, 0xA0)], 50, (HPOSP0, 0x40));
    assert_eq!(
        player_hpos(&gtia, 91),
        (0xA0..=0xA7).collect::<Vec<_>>(),
        "row 91"
    );
    assert_eq!(hpos_with(&gtia, 92, 0x02).len(), 188, "row 92 is COLBK");
}

/// Verifies REQ-418: `HPOSP0` = `$60` and a write of `$50` that takes effect at HPOS `$63`
/// show the player at HPOS `$60-$67` and `COLBK` at HPOS `$68-$DD` (criterion 3).
#[test]
fn move_during_image_keeps_image() {
    let gtia = frame_with_write(&[(GRAFP0, 0xFF), (HPOSP0, 0x60)], 48, (HPOSP0, 0x50));
    assert_eq!(
        player_hpos(&gtia, 92),
        (0x60..=0x67).collect::<Vec<_>>(),
        "row 92"
    );
    assert_eq!(
        span(&gtia, 92, 0x68..=0xDD),
        vec![0x02; 2 * (0xDD - 0x68 + 1)],
        "COLBK"
    );
}

/// Verifies REQ-418: `GRAFP0` = `$A0`, `HPOSP0` = `$60` and a write of `$61` that takes
/// effect at HPOS `$61` show the player at HPOS `$60-$63` only: the load ORs the shifted old
/// bits with the new bits (criterion 4).
#[test]
fn reload_ors_shift_register() {
    let gtia = frame_with_write(&[(GRAFP0, 0xA0), (HPOSP0, 0x60)], 47, (HPOSP0, 0x61));
    assert_eq!(player_hpos(&gtia, 91), [0x60, 0x62], "row 91: $A0");
    assert_eq!(
        player_hpos(&gtia, 92),
        (0x60..=0x63).collect::<Vec<_>>(),
        "row 92"
    );
    assert_eq!(
        span(&gtia, 92, 0x64..=0xDD),
        vec![0x02; 2 * (0xDD - 0x64 + 1)],
        "COLBK"
    );
}

/// Verifies REQ-418: `HPOSP0` = `$1A` shows `COLBK` at HPOS `$22-$DD`: the image shifts out
/// in horizontal blank (criterion 5).
#[test]
fn image_shifts_out_in_hblank() {
    let gtia = frame(&[(GRAFP0, 0xFF), (HPOSP0, 0x1A), (COLPM0, 0x46)]);
    for r in 0..FRAME_HEIGHT {
        assert_eq!(hpos_with(&gtia, r, 0x02).len(), 188, "row {r}");
    }
}

/// The rows of HRM Table 15: `(original size, new size, last "before" state, "after"
/// states)`. Size codes: 1x = `%00`, 2x = `%01`, 4x = `%11`, 1x* = `%10`.
const TABLE_15: [(u8, u8, u8, [u8; 4]); 20] = [
    (0b00, 0b01, 0b00, [0b01, 0b00, 0b01, 0b00]),
    (0b00, 0b11, 0b00, [0b01, 0b10, 0b11, 0b00]),
    (0b01, 0b00, 0b00, [0b00, 0b00, 0b00, 0b00]),
    (0b01, 0b00, 0b01, [0b00, 0b00, 0b00, 0b00]),
    (0b01, 0b11, 0b00, [0b01, 0b10, 0b11, 0b00]),
    (0b01, 0b11, 0b01, [0b10, 0b11, 0b00, 0b01]),
    (0b11, 0b00, 0b00, [0b00, 0b00, 0b00, 0b00]),
    (0b11, 0b00, 0b01, [0b00, 0b00, 0b00, 0b00]),
    (0b11, 0b00, 0b10, [0b00, 0b00, 0b00, 0b00]),
    (0b11, 0b00, 0b11, [0b00, 0b00, 0b00, 0b00]),
    (0b11, 0b01, 0b00, [0b01, 0b00, 0b01, 0b00]),
    (0b11, 0b01, 0b01, [0b00, 0b01, 0b00, 0b01]),
    (0b11, 0b01, 0b10, [0b01, 0b00, 0b01, 0b00]),
    (0b11, 0b01, 0b11, [0b00, 0b01, 0b00, 0b01]),
    (0b01, 0b10, 0b10, [0b10, 0b10, 0b10, 0b10]),
    (0b01, 0b10, 0b00, [0b00, 0b00, 0b00, 0b00]),
    (0b11, 0b10, 0b00, [0b00, 0b00, 0b00, 0b00]),
    (0b11, 0b10, 0b01, [0b10, 0b10, 0b10, 0b10]),
    (0b11, 0b10, 0b10, [0b10, 0b10, 0b10, 0b10]),
    (0b11, 0b10, 0b11, [0b00, 0b00, 0b00, 0b00]),
];

/// Verifies REQ-419: for each of the 20 rows of HRM Table 15, the next four shift states
/// after the size write are the four "after" values of the row (criterion 1).
#[test]
fn size_change_table_15() {
    for (n, (original, new, before, after)) in TABLE_15.into_iter().enumerate() {
        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        gtia.apply(SIZEP0, original);
        gtia.hposp[0] = 0x10;
        gtia.shift[0] = 0xFF;
        gtia.shift_state[0] = before;
        gtia.apply(SIZEP0, new);
        let mut states = [0; 4];
        for (slot, h) in states.iter_mut().zip(0x80..) {
            let old = gtia.shift[0];
            let _bits = gtia.shift_objects(h);
            *slot = gtia.shift_state[0];
            let expected = if *slot == 0 { old << 1 } else { old };
            assert_eq!(
                gtia.shift[0], expected,
                "row {n}: a shift at each step to %00"
            );
        }
        assert_eq!(
            states, after,
            "row {n}: {original:02b} to {new:02b} from {before:02b}"
        );
    }
}

/// Verifies REQ-419: `SIZEP0` `$01` to `$02` at HPOS `$41` shows `COLPM0` from HPOS `$40` to
/// the end of that scanline and at HPOS `$22-$3F` of the next scanline (criterion 2, the
/// first sentence).
#[test]
fn size_10_lockup_next_line() {
    let gtia = frame_with_write(
        &[(GRAFP0, 0x80), (HPOSP0, 0x40), (SIZEP0, 0x01)],
        31,
        (SIZEP0, 0x02),
    );
    assert_eq!(player_hpos(&gtia, 91), [0x40, 0x41], "row 91: double width");
    assert_eq!(
        player_hpos(&gtia, 92),
        (0x40..=0xDD).collect::<Vec<_>>(),
        "row 92"
    );
    assert_eq!(
        player_hpos(&gtia, 93),
        (0x22..=0x40).collect::<Vec<_>>(),
        "row 93"
    );
    assert_eq!(
        player_hpos(&gtia, 94),
        [0x40],
        "row 94: the reload ends the lockup"
    );
}

/// Verifies REQ-419: changes `SIZEP0` after the image and compares the row with a row with
/// no write (criterion 3). A `SIZEP0` write takes effect at an odd HPOS (REQ-434), so the
/// test uses HPOS `$81`, the first HPOS after `$80` that a write can reach.
#[test]
fn size_change_after_image() {
    for (old, new) in [(0x00, 0x01), (0x00, 0x03), (0x01, 0x00), (0x03, 0x00)] {
        let setup = [(GRAFP0, 0xFF), (HPOSP0, 0x40), (SIZEP0, old)];
        let changed = frame_with_write(&setup, 63, (SIZEP0, new));
        let plain = frame_with_write(&setup, 63, (0x1E, 0x00));
        assert_eq!(
            row(&changed, 92),
            row(&plain, 92),
            "${old:02X} to ${new:02X}"
        );
        assert_ne!(
            row(&changed, 93),
            row(&changed, 91),
            "the new size applies later"
        );
    }
}

/// Verifies REQ-441: feeds `CycleOutput` values with a missile DMA cycle and bus values on
/// cycles 2-5, and checks the player images; with `GRACTL` = `$01` `GRAFP0-3` do not change,
/// and real player DMA bytes take precedence over the bus values (criteria 1-3).
#[test]
fn players_load_bus_without_player_dma() {
    let patterns = [0x81_u8, 0x42, 0x24, 0x18];
    let bus_only = |_, cycle| match cycle {
        0 => (DmaKind::Missile, 0x00),
        2..=5 => (DmaKind::None, patterns[usize::try_from(cycle - 2).unwrap()]),
        _ => (DmaKind::None, 0xFF),
    };
    let player_dma = |_, cycle: u32| match cycle {
        0 => (DmaKind::Missile, 0x00),
        2..=5 => (DmaKind::Player(u8::try_from(cycle - 2).unwrap()), 0xFF),
        _ => (DmaKind::None, 0x5A),
    };
    let cases: [(u8, Dma<'_>, [u8; 4]); 3] = [
        (0x03, &bus_only, patterns),
        (0x01, &bus_only, [0x00; 4]),
        (0x03, &player_dma, [0xFF; 4]),
    ];
    for (gractl, dma, expected) in cases {
        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        for n in 0..4 {
            gtia.write(HPOSP0 + n, 0x40 + 0x10 * n);
            gtia.write(COLPM0 + n, 0x46);
        }
        gtia.write(GRACTL, gractl);
        run_frame(&mut gtia, false, &background, dma, &mut |_, _, _| {});
        for r in [0, 100, 239] {
            for (n, pattern) in expected.into_iter().enumerate() {
                let h = 0x40 + 0x10 * n;
                let shown: Vec<usize> = (h..h + 8)
                    .filter(|&h| at(&gtia, r, h) == [0x46; 2])
                    .collect();
                let bits: Vec<usize> = (0..8)
                    .filter(|bit| pattern & (0x80 >> bit) != 0)
                    .map(|bit| h + bit)
                    .collect();
                assert_eq!(shown, bits, "GRACTL ${gractl:02X} row {r} player {n}");
            }
        }
    }
}
