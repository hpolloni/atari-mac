//! GTIA mode tests (REQ-428 to REQ-433).

use super::objects::{
    collisions, gtia_with, no_dma, only, pixel_codes, plain_frame, run_frame, span,
};
use super::{COLBK, COLPF1, COLPM0, PRIOR, at};
use std::ops::Range;

use crate::antic::AnCode;
use crate::gtia::Gtia;

/// The codes of a line with the hires pairs `pairs` from HPOS `$30` and `%000` elsewhere.
fn pairs_from_30(pairs: &[u8]) -> impl Fn(u32) -> AnCode {
    move |h| {
        let i = usize::try_from(h).unwrap().wrapping_sub(0x30);
        pairs
            .get(i)
            .map_or(AnCode::BACKGROUND, |&pair| AnCode::hires(pair))
    }
}

/// The color of the full GTIA mode pixel at HPOS `$80-$81` with `setup` and the codes
/// `inner`, after a check that both color clocks agree.
fn pixel_color(setup: &[(u8, u8)], inner: &dyn Fn(u32) -> AnCode) -> u8 {
    let mut gtia = gtia_with(setup);
    plain_frame(&mut gtia, false, inner);
    let [left, right] = at(&gtia, 100, 0x80);
    assert_eq!(
        [left, right],
        at(&gtia, 100, 0x81),
        "the pixel is two color clocks"
    );
    assert_eq!(left, right, "both columns");
    left
}

/// The codes of the pair (`even`, `odd`) at each even HPOS and the next HPOS.
fn code_pair(even: AnCode, odd: AnCode) -> impl Fn(u32) -> AnCode {
    move |h| if h % 2 == 0 { even } else { odd }
}

/// Verifies REQ-428: AN1-AN0 values `%00`, `%00`, `%01`, `%11`, `%11`, `%00` at HPOS
/// `$30-$35` give `$00` at HPOS `$30-$31`, `$07` at `$32-$33` and `$0C` at `$34-$35`
/// (criterion 3).
#[test]
fn gtia_pixels_pair_even_hpos() {
    let mut gtia = gtia_with(&[(PRIOR, 0x40), (COLBK, 0x00)]);
    let inner = pairs_from_30(&[0b00, 0b00, 0b01, 0b11, 0b11, 0b00]);
    plain_frame(&mut gtia, false, &inner);
    assert_eq!(span(&gtia, 100, 0x30..=0x31), [0x00; 4], "HPOS $30-$31");
    assert_eq!(span(&gtia, 100, 0x32..=0x33), [0x07; 4], "HPOS $32-$33");
    assert_eq!(span(&gtia, 100, 0x34..=0x35), [0x0C; 4], "HPOS $34-$35");
}

/// Verifies REQ-428: code pairs (`%000`, `%101`) and (`%100`, `%101`) give the same pixel
/// `$01` (criterion 4).
#[test]
fn gtia_mode_ignores_an2() {
    let setup = [(PRIOR, 0x40), (COLBK, 0x00)];
    let bak = code_pair(AnCode::BACKGROUND, AnCode::hires(1));
    let pf = code_pair(AnCode::hires(0), AnCode::hires(1));
    assert_eq!(pixel_color(&setup, &bak), 0x01, "(%000, %101)");
    assert_eq!(pixel_color(&setup, &pf), 0x01, "(%100, %101)");
}

/// Verifies REQ-428: after the horizontal blank code `%011`, with `COLBK` = `$90` and
/// `COLPF1` = `$0E`, the pair (`%110`, `%110`) shows `$9A` in all four columns, and player 0
/// on it sets no bit in `P0PF` (criterion 5).
#[test]
fn gtia_mode_ignores_hires() {
    let mut gtia = gtia_with(&[
        (PRIOR, 0x40),
        (COLBK, 0x90),
        (COLPF1, 0x0E),
        (0x00, 0xA0),
        (0x0D, 0xFF),
    ]);
    plain_frame(&mut gtia, true, &|_| AnCode::hires(0b10));
    assert_eq!(span(&gtia, 100, 0x80..=0x81), [0x9A; 4], "four columns");
    assert_eq!(collisions(&gtia), [0; 16], "no P0PF bit");
}

/// The color of player 0 (or missile 0) on each pixel value of `values` with `PRIOR` =
/// `prior` and `COLPM0` = `$46`.
fn object_on_pixels(prior: u8, missile: bool, values: Range<u8>) {
    for value in values {
        let mut setup = vec![(PRIOR, prior), (COLBK, 0x90), (COLPM0, 0x46)];
        if missile {
            setup.extend([(0x04, 0x80), (0x11, 0x03)]);
        } else {
            setup.extend([(0x00, 0x7C), (0x0D, 0xFF)]);
        }
        let color = pixel_color(&setup, &pixel_codes(value));
        assert_eq!(
            color, 0x46,
            "PRIOR ${prior:02X} missile {missile} pixel {value}"
        );
    }
}

/// The collision registers after a frame with player 0 at HPOS `$80` and missile 0 at
/// HPOS `$A0` on pixels of value `value`.
fn object_collisions(prior: u8, value: u8) -> [u8; 16] {
    let mut gtia = gtia_with(&[
        (PRIOR, prior),
        (0x00, 0x80),
        (0x0D, 0xFF),
        (0x04, 0xA0),
        (0x11, 0x03),
    ]);
    plain_frame(&mut gtia, false, &pixel_codes(value));
    collisions(&gtia)
}

/// Verifies REQ-429: with `PRIOR` = `$44` player 0 and with `PRIOR` = `$40` missile 0 on
/// pixels 1-15 show `COLPM0` (criteria 3 and 4).
#[test]
fn mode_9_players_on_top() {
    object_on_pixels(0x44, false, 1..16);
    object_on_pixels(0x40, true, 1..16);
}

/// Verifies REQ-429: with `PRIOR` = `$40`, player 0 and missile 0 on pixels 1-15 set no bit
/// in `P0PF` and `M0PF` (criterion 6).
#[test]
fn mode_9_no_playfield_collisions() {
    for value in 1..16 {
        assert_eq!(object_collisions(0x40, value), [0; 16], "pixel {value}");
    }
}

/// The colors of REQ-430: `COLPM0-3` = `$10-$40`, `COLPF0-3` = `$50-$80`, `COLBK` = `$90`,
/// and `PRIOR` = `prior`.
fn mode_10_gtia(prior: u8) -> Gtia {
    let mut gtia = gtia_with(&[(PRIOR, prior)]);
    for (n, reg) in (0x12..=0x1A).enumerate() {
        gtia.write(reg, u8::try_from(0x10 * (n + 1)).unwrap());
    }
    gtia
}

/// The codes of a mode F line of `$44` bytes on HPOS `first..=last` and `%000` elsewhere.
fn mode_f_44(first: u32, last: u32) -> impl Fn(u32) -> AnCode {
    move |h| {
        if (first..=last).contains(&h) {
            AnCode::hires(u8::from(h % 2 == 0))
        } else {
            AnCode::BACKGROUND
        }
    }
}

/// Verifies REQ-430: with `PRIOR` = `$80`, pixel values 0-15 show `COLPM0-3`, `COLPF0-3`,
/// `COLBK` four times and `COLPF0-3`, one color clock late (criterion 1).
#[test]
fn mode_10_register_map() {
    let expected = [
        0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0x90, 0x90, 0x90, 0x50, 0x60, 0x70,
        0x80,
    ];
    for (value, color) in (0..16).zip(expected) {
        let mut gtia = mode_10_gtia(0x80);
        plain_frame(&mut gtia, false, &pixel_codes(value));
        assert_eq!(span(&gtia, 100, 0x81..=0x82), [color; 4], "pixel {value}");
    }
}

/// Verifies REQ-430: a normal width mode F line of `$44` bytes shows each pixel one color
/// clock later (HPOS `$30` and `$D1` show `COLPM0`, `$31-$D0` show `COLPF0`), the border
/// shows `COLPM0`, and with `PRIOR` = `$81` and `$A1` player 1 in the border shows `$10` and
/// `$30` (criteria 2-4).
#[test]
fn mode_10_delay_and_border() {
    let mut gtia = mode_10_gtia(0x80);
    plain_frame(&mut gtia, false, &mode_f_44(0x30, 0xCF));
    assert_eq!(at(&gtia, 100, 0x30), [0x10; 2], "2: HPOS $30");
    assert_eq!(
        span(&gtia, 100, 0x31..=0xD0),
        vec![0x50; 2 * 0xA0],
        "2: HPOS $31-$D0"
    );
    assert_eq!(at(&gtia, 100, 0xD1), [0x10; 2], "2: HPOS $D1");
    assert_eq!(
        span(&gtia, 100, 0x23..=0x2F),
        vec![0x10; 2 * 13],
        "3: left border"
    );
    assert_eq!(
        span(&gtia, 100, 0xD1..=0xDD),
        vec![0x10; 2 * 13],
        "3: right border"
    );
    for (prior, color) in [(0x81, 0x10), (0xA1, 0x30)] {
        let mut gtia = mode_10_gtia(prior);
        gtia.write(0x01, 0x24);
        gtia.write(0x0E, 0xFF);
        plain_frame(&mut gtia, false, &mode_f_44(0x30, 0xCF));
        assert_eq!(
            span(&gtia, 100, 0x24..=0x2B),
            [color; 16],
            "4: PRIOR ${prior:02X}"
        );
    }
}

/// Verifies REQ-430: a wide mode F line with `PRIOR` = `$80`, checks HPOS `$2C-$DD`
/// (criterion 6).
#[test]
fn mode_10_wide_right_edge() {
    let mut gtia = mode_10_gtia(0x80);
    plain_frame(&mut gtia, false, &mode_f_44(0x2C, 0xDD));
    assert_eq!(at(&gtia, 100, 0x2C), [0x10; 2], "HPOS $2C");
    assert_eq!(
        span(&gtia, 100, 0x2D..=0xDD),
        vec![0x50; 2 * 0xB1],
        "HPOS $2D-$DD"
    );
}

/// Verifies REQ-430: with `PRIOR` = `$80`, player 0 over a pixel with value 5 sets `P0PF` =
/// `$02`, and over a pixel with value 1 sets `P0PL` = `$00` (criterion 5).
#[test]
fn mode_10_collisions() {
    let player_0 = |value| {
        let mut gtia = mode_10_gtia(0x80);
        gtia.write(0x00, 0x80);
        gtia.write(0x0D, 0xFF);
        plain_frame(&mut gtia, false, &pixel_codes(value));
        collisions(&gtia)
    };
    assert_eq!(player_0(5), only(&[(0x04, 0x02)]), "pixel 5");
    assert_eq!(player_0(1), [0; 16], "pixel 1");
}

/// Verifies REQ-431: with `PRIOR` = `$C0` and `COLPM0` = `$46`, player 0 on pixels 0-15 shows
/// `$46` (criterion 3).
#[test]
fn mode_11_players_on_top() {
    object_on_pixels(0xC0, false, 0..16);
}

/// Verifies REQ-431: with `PRIOR` = `$C0`, player 0 and missile 0 on pixels 0-15 set no bit
/// in `P0PF` and `M0PF` (criterion 5).
#[test]
fn mode_11_no_playfield_collisions() {
    for value in 0..16 {
        assert_eq!(object_collisions(0xC0, value), [0; 16], "pixel {value}");
    }
}

/// Verifies REQ-432: after a `PRIOR` write of `$40` at HPOS `$24` and `$00` at HPOS `$28` on
/// a hires line, codes `%100-%111` at HPOS `$30-$33` give `COLPF0-3` in both columns; on the
/// next scanline with `PRIOR` = `$00`, code `%101` gives `$38` and `$36` (criteria 1 and 2).
#[test]
fn pseudo_mode_e_after_mid_line_prior() {
    let mut gtia = gtia_with(&[
        (COLBK, 0x02),
        (0x16, 0x14),
        (0x17, 0x26),
        (0x18, 0x38),
        (0x19, 0x4A),
    ]);
    let inner = pairs_from_30(&[0b00, 0b01, 0b10, 0b11]);
    // `PRIOR` bits 6-7 take effect at `2c - 2 + 4`: cycle 17 gives $24, cycle 19 gives $28.
    run_frame(
        &mut gtia,
        true,
        &inner,
        &no_dma,
        &mut |gtia, l, c| match (l, c) {
            (100, 17) => gtia.write(PRIOR, 0x40),
            (100, 19) => gtia.write(PRIOR, 0x00),
            _ => {}
        },
    );
    assert_eq!(at(&gtia, 91, 0x31), [0x38, 0x36], "row 91: hires");
    assert_eq!(
        span(&gtia, 92, 0x30..=0x33),
        [0x14, 0x14, 0x26, 0x26, 0x38, 0x38, 0x4A, 0x4A],
        "1: row 92, pseudo mode E"
    );
    assert_eq!(at(&gtia, 93, 0x31), [0x38, 0x36], "2: row 93, hires again");
}

/// Verifies REQ-432: with `PRIOR` = `$40` during the horizontal blank code `%011` and a write
/// of `$00` that takes effect at HPOS `$28`, codes `%100-%111` at HPOS `$30-$33` give
/// `COLPF0-3` in both columns (criterion 3).
#[test]
fn gtia_mode_at_hblank_blocks_hires() {
    let mut gtia = gtia_with(&[
        (COLBK, 0x02),
        (0x16, 0x14),
        (0x17, 0x26),
        (0x18, 0x38),
        (0x19, 0x4A),
    ]);
    let inner = pairs_from_30(&[0b00, 0b01, 0b10, 0b11]);
    // `PRIOR` bits 6-7 take effect at `2c - 2 + 4`: cycle 100 of line 99 gives $CA, before the
    // horizontal blank of line 100; cycle 19 of line 100 gives $28.
    run_frame(
        &mut gtia,
        true,
        &inner,
        &no_dma,
        &mut |gtia, l, c| match (l, c) {
            (99, 100) => gtia.write(PRIOR, 0x40),
            (100, 19) => gtia.write(PRIOR, 0x00),
            _ => {}
        },
    );
    assert_eq!(
        span(&gtia, 92, 0x30..=0x33),
        [0x14, 0x14, 0x26, 0x26, 0x38, 0x38, 0x4A, 0x4A],
        "3: row 92, pseudo mode E"
    );
}

/// Verifies REQ-433: with `PRIOR` = `$80`, the pair (`%101`, `%000`) shows `COLBK` `$90`, the
/// pair (`%101`, `%100`) shows `COLPF0` `$50` and the pair (`%000`, `%100`) shows `COLPM0`
/// `$10` (criteria 1-3).
#[test]
fn mode_10_bak_mutes_pixel() {
    let setup = [(PRIOR, 0x80), (COLBK, 0x90), (0x16, 0x50), (COLPM0, 0x10)];
    for (even, odd, color) in [
        (AnCode::playfield(1), AnCode::BACKGROUND, 0x90),
        (AnCode::playfield(1), AnCode::playfield(0), 0x50),
        (AnCode::BACKGROUND, AnCode::playfield(0), 0x10),
    ] {
        let mut gtia = gtia_with(&setup);
        plain_frame(&mut gtia, false, &code_pair(even, odd));
        // Mode 10 shows the pixel of HPOS $80-$81 at HPOS $81-$82.
        assert_eq!(
            span(&gtia, 100, 0x81..=0x82),
            [color; 4],
            "({:03b}, {:03b})",
            even.bits(),
            odd.bits()
        );
    }
}
