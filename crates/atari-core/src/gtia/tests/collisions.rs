//! Collision tests (REQ-420 to REQ-423).

use super::objects::{
    background, collisions, gtia_with, no_dma, only, pixel_codes, plain_frame, run_lines,
};
use super::{COLPM0, PRIOR};
use crate::antic::AnCode;
use crate::gtia::Gtia;
use crate::timing::VideoStandard;

/// `HITCLR`.
const HITCLR: u8 = 0x1E;
/// The index of `M0PF` in the collision registers.
const M0PF: usize = 0x00;
/// The index of `P0PF`.
const P0PF: usize = 0x04;
/// The index of `M0PL`.
const M0PL: usize = 0x08;
/// The index of `P0PL`.
const P0PL: usize = 0x0C;

/// Player 0 (`$FF`) at HPOS `$80` and, if `missile`, missile 0 (`%11`) at HPOS `$A0`.
fn object_0(missile: bool) -> Vec<(u8, u8)> {
    let mut setup = vec![(0x00, 0x80), (0x0D, 0xFF)];
    if missile {
        setup.extend([(0x04, 0xA0), (0x11, 0x03)]);
    }
    setup
}

/// The collision registers after one frame with `setup` and the codes `inner`.
fn after_frame(setup: &[(u8, u8)], hires: bool, inner: &dyn Fn(u32) -> AnCode) -> [u8; 16] {
    let mut gtia = gtia_with(setup);
    plain_frame(&mut gtia, hires, inner);
    collisions(&gtia)
}

/// The code `code` at HPOS `$80-$87` and background elsewhere.
fn under_object(code: AnCode) -> impl Fn(u32) -> AnCode {
    move |h| {
        if (0x80..=0x87).contains(&h) {
            code
        } else {
            AnCode::BACKGROUND
        }
    }
}

/// Verifies REQ-420: on a hires line, player 0 on code `%101`, `%110` or `%111` sets `P0PF`
/// to `$04`, and on code `%100` sets no bit (criterion 3).
#[test]
fn hires_collisions_use_pf2() {
    for pair in 1..4 {
        let regs = after_frame(&object_0(false), true, &under_object(AnCode::hires(pair)));
        assert_eq!(regs, only(&[(P0PF, 0x04)]), "pair %{pair:02b}");
    }
    let regs = after_frame(&object_0(false), true, &under_object(AnCode::hires(0)));
    assert_eq!(regs, [0; 16], "pair %00");
}

/// Verifies REQ-420: with `PRIOR` = `$40` or `$C0` players and missiles set no playfield
/// bit; with `PRIOR` = `$80` player 0 over pixel values 4-7 and 12-15 sets
/// `1 << (value AND 3)` and over values 0-3 and 8-11 sets no bit (criteria 4 and 5).
#[test]
fn gtia_mode_playfield_collisions() {
    for prior in [0x40, 0xC0] {
        for value in 1..16 {
            let mut setup = object_0(true);
            setup.push((PRIOR, prior));
            let regs = after_frame(&setup, false, &pixel_codes(value));
            assert_eq!(regs, [0; 16], "PRIOR ${prior:02X} pixel {value}");
        }
    }
    for value in 0..16_u8 {
        let mut setup = object_0(false);
        setup.push((PRIOR, 0x80));
        let regs = after_frame(&setup, false, &pixel_codes(value));
        let expected = match value {
            4..=7 | 12..=15 => only(&[(P0PF, 1 << (value & 3))]),
            _ => [0; 16],
        };
        assert_eq!(regs, expected, "PRIOR $80 pixel {value}");
    }
}

/// Verifies REQ-420: all color registers `$00`, `PRIOR` = `$04`, player 0 on `PF0`, checks
/// `P0PF` = `$01` (criterion 6).
#[test]
fn collisions_ignore_colors_and_priority() {
    let mut setup = object_0(false);
    setup.extend((0x12..=0x1A).map(|reg| (reg, 0x00)));
    setup.push((PRIOR, 0x04));
    let regs = after_frame(&setup, false, &under_object(AnCode::playfield(0)));
    assert_eq!(regs, only(&[(P0PF, 0x01)]), "player 0 on PF0");
}

/// Verifies REQ-421: missiles 0 and 1 at the same pixels, with no player, set no collision
/// bit (criterion 3).
#[test]
fn no_missile_missile_collisions() {
    let setup = [(0x04, 0x80), (0x05, 0x80), (0x11, 0x0F)];
    assert_eq!(
        after_frame(&setup, false, &background),
        [0; 16],
        "missiles 0 and 1"
    );
}

/// Verifies REQ-421: player 0 alone sets no collision bit, `P0PL` = `$00` (criterion 4).
#[test]
fn no_self_collision() {
    assert_eq!(
        after_frame(&object_0(false), false, &background),
        [0; 16],
        "player 0"
    );
}

/// Verifies REQ-421: with `COLPM0` = `COLPM1` = `$46`, players 0 and 1 at the same pixels set
/// `P0PL` = `$02` and `P1PL` = `$01` (criterion 5).
#[test]
fn same_color_players_collide() {
    let mut setup = object_0(false);
    setup.extend([
        (0x01, 0x80),
        (0x0E, 0xFF),
        (COLPM0, 0x46),
        (COLPM0 + 1, 0x46),
    ]);
    let regs = after_frame(&setup, false, &background);
    assert_eq!(
        regs,
        only(&[(P0PL, 0x02), (P0PL + 1, 0x01)]),
        "players 0 and 1"
    );
}

/// Players 0 and 1 (`$FF`, single width) at `h0` and `h1`.
fn two_players(h0: u8, h1: u8) -> [(u8, u8); 4] {
    [(0x00, h0), (0x01, h1), (0x0D, 0xFF), (0x0E, 0xFF)]
}

/// Writes `$FF` to `GRAFP0-1` at the start of scan line 248 and `$00` at the start of scan
/// line 8.
fn load_in_blank_lines(gtia: &mut Gtia, line: u32, cycle: u32) {
    let value = match (line, cycle) {
        (248, 0) => 0xFF,
        (8, 0) => 0x00,
        _ => return,
    };
    gtia.write(0x0D, value);
    gtia.write(0x0E, value);
}

/// The collision registers after two frames of `standard` with players 0 and 1 at HPOS
/// `$80`, whose graphics are `$FF` only on scan lines 248 and later and 0-7.
fn blank_line_overlap(standard: VideoStandard) -> [u8; 16] {
    let lines = standard.lines_per_frame();
    let mut gtia = Gtia::new(standard);
    for (reg, value) in [(0x00, 0x80), (0x01, 0x80)] {
        gtia.write(reg, value);
    }
    let codes = |line: u32| -> Box<dyn Fn(u32) -> AnCode> {
        if (8..248).contains(&line) {
            Box::new(background)
        } else {
            Box::new(|_| AnCode::HBLANK_LORES)
        }
    };
    for _ in 0..2 {
        for line in 0..lines {
            run_lines(
                &mut gtia,
                line..=line,
                false,
                &codes(line),
                &no_dma,
                &mut load_in_blank_lines,
            );
        }
    }
    collisions(&gtia)
}

/// Verifies REQ-422: players that overlap only outside HPOS `$22-$DD`, or only on scanlines
/// with blank codes, set no collision bit; an overlap inside the visible area sets `P0PL` =
/// `$02` and `P1PL` = `$01` (criteria 1-4).
#[test]
fn collisions_only_in_visible_area() {
    let left = after_frame(&two_players(0x1A, 0x1C), false, &background);
    assert_eq!(left, [0; 16], "1: overlap at $1C-$21");
    let edge = after_frame(&two_players(0x1C, 0x20), false, &background);
    assert_eq!(
        edge,
        only(&[(P0PL, 0x02), (P0PL + 1, 0x01)]),
        "2: overlap at $20-$23"
    );
    let right = after_frame(&two_players(0xDA, 0xDE), false, &background);
    assert_eq!(right, [0; 16], "3: overlap at $DE-$E1");
    for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
        let regs = blank_line_overlap(standard);
        assert_eq!(
            regs, [0; 16],
            "4: overlap on blank scan lines, {standard:?}"
        );
    }
}

/// The objects that set all 16 collision registers: players 0-3 and missiles 0-3 at HPOS
/// `$80` on `PF0`.
fn all_collisions(gtia: &mut Gtia) {
    for n in 0..4 {
        gtia.write(n, 0x80);
        gtia.write(0x04 + n, 0x80);
        gtia.write(0x0D + n, 0xFF);
    }
    gtia.write(0x11, 0xFF);
    plain_frame(gtia, false, &under_object(AnCode::playfield(0)));
}

/// Verifies REQ-423: after a collision, a write of `$00` or `$FF` to `HITCLR` makes
/// `$D000-$D00F` read `$00`, and a later collision sets the bit again (criteria 1 and 3).
#[test]
fn hitclr_clears_all() {
    for value in [0x00, 0xFF] {
        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        all_collisions(&mut gtia);
        assert!(
            collisions(&gtia).iter().all(|&r| r != 0),
            "all registers set"
        );
        gtia.write(HITCLR, value);
        assert_eq!(collisions(&gtia), [0; 16], "HITCLR ${value:02X}");
        all_collisions(&mut gtia);
        assert!(
            collisions(&gtia).iter().all(|&r| r != 0),
            "set again after HITCLR ${value:02X}"
        );
        assert_eq!(
            collisions(&gtia)[M0PF],
            0x01,
            "M0PF after HITCLR ${value:02X}"
        );
        assert_eq!(
            collisions(&gtia)[M0PL],
            0x0F,
            "M0PL after HITCLR ${value:02X}"
        );
    }
}

/// Verifies REQ-423: with no `HITCLR` write, a collision bit set on scanline 50 still reads
/// 1 on scanline 200 and in the next two frames, with no further collision (criterion 2).
#[test]
fn collisions_latch_until_hitclr() {
    let mut gtia = gtia_with(&two_players(0x80, 0x80));
    gtia.write(0x0D, 0x00);
    let overlap_on_50 = &mut |g: &mut Gtia, l, c| {
        if c == 0 && (l == 50 || l == 51) {
            let value = if l == 50 { 0xFF } else { 0x00 };
            g.write(0x0D, value);
        }
    };
    run_lines(&mut gtia, 0..50, false, &background, &no_dma, overlap_on_50);
    assert_eq!(collisions(&gtia), [0; 16], "before scan line 50");
    run_lines(
        &mut gtia,
        50..=200,
        false,
        &background,
        &no_dma,
        overlap_on_50,
    );
    let expected = only(&[(P0PL, 0x02), (P0PL + 1, 0x01)]);
    assert_eq!(collisions(&gtia), expected, "scan line 200");
    run_lines(
        &mut gtia,
        201..262,
        false,
        &background,
        &no_dma,
        &mut |_, _, _| {},
    );
    for frame in 1..=2 {
        run_lines(
            &mut gtia,
            0..262,
            false,
            &background,
            &no_dma,
            &mut |_, _, _| {},
        );
        assert_eq!(
            collisions(&gtia),
            expected,
            "frame {frame} after the collision"
        );
    }
}
