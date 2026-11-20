//! Priority and multicolor player tests (REQ-424, REQ-425, REQ-427).

use super::objects::{collisions, gtia_with, only, plain_frame};
use super::{PRIOR, at};
use crate::antic::AnCode;
use crate::gtia::{Gtia, Playfield};
use crate::timing::VideoStandard;

/// The colors of REQ-424: `COLPM0-3`, `COLPF0-3` and `COLBK`.
const COLORS: [u8; 9] = [0x12, 0x24, 0x36, 0x48, 0x5A, 0x6C, 0x7E, 0x80, 0x92];

/// The objects and the playfield of one color clock.
#[derive(Debug, Clone, Copy, Default)]
struct Layers<'a> {
    /// The players at the color clock.
    players: &'a [u8],
    /// The missiles at the color clock.
    missiles: &'a [u8],
    /// The playfield code (`PFn`), if any.
    playfield: Option<u8>,
}

/// The color at HPOS `$80` with `PRIOR` = `prior`, the color registers `colors` (`COLPM0-3`,
/// `COLPF0-3`, `COLBK`) and the objects and playfield of `layers` at `$80-$81`.
fn color_at(prior: u8, colors: [u8; 9], layers: Layers<'_>) -> [u8; 2] {
    let mut gtia = gtia_with(&[(PRIOR, prior)]);
    for (reg, color) in (0x12..).zip(colors) {
        gtia.write(reg, color);
    }
    for &n in layers.players {
        gtia.write(n, 0x80);
        gtia.write(0x0D + n, 0xC0);
    }
    let mut grafm = 0;
    for &n in layers.missiles {
        gtia.write(0x04 + n, 0x80);
        grafm |= 3 << (2 * n);
    }
    gtia.write(0x11, grafm);
    let inner = |h| match layers.playfield {
        Some(pf) if (0x80..=0x81).contains(&h) => AnCode::playfield(pf),
        _ => AnCode::BACKGROUND,
    };
    plain_frame(&mut gtia, false, &inner);
    assert_eq!(
        at(&gtia, 100, 0x81),
        at(&gtia, 100, 0x80),
        "both color clocks"
    );
    at(&gtia, 100, 0x80)
}

/// One color byte with the colors of REQ-424.
fn shows(prior: u8, players: &[u8], missiles: &[u8], playfield: Option<u8>) -> u8 {
    let layers = Layers {
        players,
        missiles,
        playfield,
    };
    color_at(prior, COLORS, layers)[0]
}

/// The HRM 6.7 equations, evaluated in the test: the outputs `SP0-SP3`, `SF0-SF3`, `SB` for
/// `PRIOR` and the inputs `P0-P3` (bits 0-3) and `PF0-PF3` (bits 4-7).
#[expect(
    clippy::similar_names,
    clippy::nonminimal_bool,
    reason = "the signal names and the equations of HRM 6.7"
)]
fn equations(prior: u8, inputs: u8) -> [bool; 9] {
    let b = |value: u8, n: u8| value & (1 << n) != 0;
    let (pri0, pri1, pri2, pri3, multi) = (
        b(prior, 0),
        b(prior, 1),
        b(prior, 2),
        b(prior, 3),
        b(prior, 5),
    );
    let (p0, p1, p2, p3) = (b(inputs, 0), b(inputs, 1), b(inputs, 2), b(inputs, 3));
    let (pf0, pf1, pf2, pf3) = (b(inputs, 4), b(inputs, 5), b(inputs, 6), b(inputs, 7));
    let pri01 = pri0 || pri1;
    let pri12 = pri1 || pri2;
    let pri23 = pri2 || pri3;
    let pri03 = pri0 || pri3;
    let p01 = p0 || p1;
    let p23 = p2 || p3;
    let pf01 = pf0 || pf1;
    let pf23 = pf2 || pf3;
    let sp0 = p0 && !(pf01 && pri23) && !(pri2 && pf23);
    let sp1 = p1 && !(pf01 && pri23) && !(pri2 && pf23) && (!p0 || multi);
    let sp2 = p2 && !p01 && !(pf23 && pri12) && !(pf01 && !pri0);
    let sp3 = p3 && !p01 && !(pf23 && pri12) && !(pf01 && !pri0) && (!p2 || multi);
    let sf3 = pf3 && !(p23 && pri03) && !(p01 && !pri2);
    let sf0 = pf0 && !(p23 && pri0) && !(p01 && pri01) && !sf3;
    let sf1 = pf1 && !(p23 && pri0) && !(p01 && pri01) && !sf3;
    let sf2 = pf2 && !(p23 && pri03) && !(p01 && !pri2) && !sf3;
    let sb = !p01 && !p23 && !pf01 && !pf23;
    [sp0, sp1, sp2, sp3, sf0, sf1, sf2, sf3, sb]
}

/// Verifies REQ-424: compares the priority table with an evaluation of the HRM 6.7 equations
/// that the test code does itself, for all `PRIOR` values with bit 4 = 0 and all inputs
/// (criterion 1). `PRIOR` bit 4 = 1 gives the same colors (OI-019: GTIA ignores bit 4).
#[test]
fn priority_equations_all_inputs() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.colpm.copy_from_slice(&COLORS[0..4]);
    gtia.colpf.copy_from_slice(&COLORS[4..8]);
    gtia.colbk = COLORS[8];
    for prior in (0..0x40_u8).filter(|p| p & 0x10 == 0) {
        // Players (bits 0-3), missiles (bits 4-7) and the playfield. "Any missile" is the
        // OR of the missile bits.
        for objects in 0..=255_u8 {
            for pf in 0..16_u8 {
                let inputs = (objects & 0x0F) | (objects >> 4) | (pf << 4);
                let expected = equations(prior, inputs)
                    .iter()
                    .zip(COLORS)
                    .filter(|(on, _)| **on)
                    .fold(0, |color, (_, register)| color | register);
                let playfield = Playfield {
                    inputs: pf,
                    collide: pf,
                    ..Playfield::default()
                };
                gtia.prior = prior;
                let color = gtia.color(objects, playfield, 0);
                assert_eq!(
                    color,
                    (expected, expected),
                    "PRIOR ${prior:02X} {objects:08b} {pf:04b}"
                );
                gtia.prior = prior | 0x10;
                let fifth = gtia.color(objects, playfield, 0);
                assert_eq!(
                    fifth,
                    color,
                    "PRIOR ${:02X} {objects:08b} {pf:04b}",
                    prior | 0x10
                );
            }
        }
    }
}

/// Verifies REQ-424: `PRIOR` = `$01`, `$02`, `$04`, `$08` give the colors of criteria 2-7,
/// and a color clock with no object and no playfield shows `COLBK` (criterion 8).
#[test]
fn standard_priority_modes() {
    assert_eq!(shows(0x01, &[0], &[], Some(0)), 0x12, "2: $01, P0 on PF0");
    assert_eq!(shows(0x04, &[0], &[], Some(0)), 0x5A, "3: $04, P0 on PF0");
    assert_eq!(shows(0x08, &[0], &[], Some(2)), 0x12, "4: $08, P0 on PF2");
    assert_eq!(shows(0x08, &[0], &[], Some(0)), 0x5A, "4: $08, P0 on PF0");
    assert_eq!(shows(0x02, &[0], &[], Some(0)), 0x12, "5: $02, P0 on PF0");
    assert_eq!(shows(0x02, &[2], &[], Some(1)), 0x6C, "5: $02, P2 on PF1");
    assert_eq!(shows(0x01, &[0, 1], &[], None), 0x12, "6: $01, P0 and P1");
    assert_eq!(shows(0x01, &[], &[2], Some(0)), 0x36, "7: $01, M2 on PF0");
    for prior in [0x00, 0x01, 0x02, 0x04, 0x08] {
        assert_eq!(
            shows(prior, &[], &[], None),
            0x92,
            "8: PRIOR ${prior:02X}, no layer"
        );
    }
}

/// The color with only `COLPMn` = `pm`, `COLPFk` = `pf` and the other registers `$00`.
fn mix(prior: u8, n: u8, pm: u8, k: u8, pf: u8) -> u8 {
    let mut colors = [0; 9];
    colors[usize::from(n)] = pm;
    colors[4 + usize::from(k)] = pf;
    let layers = Layers {
        players: &[n],
        missiles: &[],
        playfield: Some(k),
    };
    color_at(prior, colors, layers)[0]
}

/// Verifies REQ-425: `PRIOR` = `$00` ORs the player and playfield colors (`$4A`, `$76`),
/// `PRIOR` = `$05` and `$03` give `$00`, and `PRIOR` = `$0C` gives `$40` (criteria 1-5).
#[test]
fn priority_mode_0_mixes() {
    assert_eq!(mix(0x00, 0, 0x0A, 0, 0x40), 0x4A, "1: $00, P0 on PF0");
    assert_eq!(mix(0x00, 2, 0x06, 2, 0x70), 0x76, "2: $00, P2 on PF2");
    assert_eq!(shows(0x05, &[0], &[], Some(0)), 0x00, "3: $05, P0 on PF0");
    assert_eq!(shows(0x03, &[2], &[], Some(2)), 0x00, "4: $03, P2 on PF2");
    assert_eq!(mix(0x0C, 0, 0x0A, 0, 0x40), 0x40, "5: $0C, P0 on PF0");
}

/// The results of HRM Table 16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    /// Player 0 or 1.
    P01,
    /// Player 2 or 3.
    P23,
    /// Playfield 0 or 1.
    Pf01,
    /// Playfield 2 or 3.
    Pf23,
    /// Color `$00`.
    Black,
}

/// The `PRIOR[3:0]` columns of HRM Table 16.
const TABLE_16_COLUMNS: [u8; 11] = [
    0b0011, 0b0101, 0b0110, 0b0111, 0b1001, 0b1010, 0b1011, 0b1100, 0b1101, 0b1110, 0b1111,
];

/// One row of HRM Table 16: the active layers `(P01, P23, PF01, PF23)` and the cells.
type Table16Row = ((bool, bool, bool, bool), [Cell; 11]);

/// The rows of HRM Table 16 without `P5`: `(P01, P23, PF01, PF23)` and the cells.
const TABLE_16: [Table16Row; 6] = {
    use Cell::{Black as B, P01, P23, Pf01, Pf23};
    let pf01_p01 = [P01, B, B, B, B, B, B, Pf01, B, B, B];
    [
        ((true, false, true, false), pf01_p01),
        ((true, true, true, false), pf01_p01),
        (
            (false, true, true, false),
            [P23, P23, Pf01, P23, P23, Pf01, P23, Pf01, P23, Pf01, P23],
        ),
        (
            (true, false, false, true),
            [P01, Pf23, Pf23, Pf23, P01, P01, P01, Pf23, Pf23, Pf23, Pf23],
        ),
        (
            (false, true, false, true),
            [B, B, Pf23, B, P23, B, B, B, B, B, B],
        ),
        (
            (true, true, false, true),
            [P01, B, Pf23, B, P01, P01, P01, B, B, B, B],
        ),
    ]
};

/// Verifies REQ-425: each cell of the rows of HRM Table 16 without `P5` for all 11
/// `PRIOR[3:0]` columns (criterion 6).
#[test]
fn priority_table_16_without_p5() {
    for ((p01, p23, pf01, pf23), cells) in TABLE_16 {
        let players: Vec<u8> = [(p01, 0), (p23, 2)]
            .into_iter()
            .filter_map(|(on, n)| on.then_some(n))
            .collect();
        // One color clock has one playfield code: PF01 and PF23 are not in the same row.
        let playfield = if pf01 { Some(0) } else { pf23.then_some(2) };
        for (prior, cell) in TABLE_16_COLUMNS.into_iter().zip(cells) {
            let expected = match cell {
                Cell::P01 => COLORS[0],
                Cell::P23 => COLORS[2],
                Cell::Pf01 => COLORS[4],
                Cell::Pf23 => COLORS[6],
                Cell::Black => 0x00,
            };
            let actual = shows(prior, &players, &[], playfield);
            assert_eq!(
                actual, expected,
                "PRIOR {prior:04b}, P01 {p01} P23 {p23} PF01 {pf01} PF23 {pf23}: {cell:?}"
            );
        }
    }
}

/// The colors of REQ-427: `COLPM0-3` = `$42`, `$94`, `$06`, `$C0`, the others `$00`.
const MULTI: [u8; 9] = [0x42, 0x94, 0x06, 0xC0, 0, 0, 0, 0, 0];

/// The color of `players` and `missiles` at one color clock with the colors of REQ-427.
fn multi(prior: u8, players: &[u8], missiles: &[u8]) -> u8 {
    let layers = Layers {
        players,
        missiles,
        playfield: None,
    };
    color_at(prior, MULTI, layers)[0]
}

/// Verifies REQ-427: with `PRIOR` = `$21`, players 0 and 1 show `$D6`, players 2 and 3 show
/// `$C6` and players 0 and 2 show `$42`; with `PRIOR` = `$01` players 0 and 1 show `$42`
/// (criteria 1, 2, 3 and 5).
#[test]
fn multicolor_players_or_colors() {
    assert_eq!(multi(0x21, &[0, 1], &[]), 0xD6, "1: $21, P0 and P1");
    assert_eq!(multi(0x01, &[0, 1], &[]), 0x42, "2: $01, P0 and P1");
    assert_eq!(multi(0x21, &[2, 3], &[]), 0xC6, "3: $21, P2 and P3");
    assert_eq!(multi(0x21, &[0, 2], &[]), 0x42, "5: $21, P0 and P2");
}

/// Verifies REQ-427: with `PRIOR` = `$21`, missiles 0 and 1 show `$D6` and missiles 2 and 3
/// show `$C6` (criterion 4).
#[test]
fn multicolor_missiles() {
    assert_eq!(multi(0x21, &[], &[0, 1]), 0xD6, "M0 and M1");
    assert_eq!(multi(0x21, &[], &[2, 3]), 0xC6, "M2 and M3");
    assert_eq!(multi(0x01, &[], &[0, 1]), 0x42, "M0 and M1 without MULTI");
}

/// Verifies REQ-427: with `PRIOR` = `$21`, players 0 and 1 at the same color clock set
/// `P0PL` = `$02` and `P1PL` = `$01` (criterion 6).
#[test]
fn multicolor_keeps_collisions() {
    let mut gtia = gtia_with(&[(PRIOR, 0x21)]);
    for n in 0..2 {
        gtia.write(n, 0x80);
        gtia.write(0x0D + n, 0xFF);
    }
    plain_frame(&mut gtia, false, &|_| AnCode::BACKGROUND);
    assert_eq!(
        collisions(&gtia),
        only(&[(0x0C, 0x02), (0x0D, 0x01)]),
        "P0PL and P1PL"
    );
}
