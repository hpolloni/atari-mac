//! GTIA unit tests (`docs/design/gtia.md`, "Test plan").
//!
//! A test makes a `Gtia`, writes registers and calls `clock` with synthetic `CycleOutput`
//! values. The code of HPOS `h` goes into the cycle `c` with `h = (2c - 2) mod 228`.

mod collisions;
mod inputs;
mod modes;
mod objects;
mod players;
mod playfield;
mod priority;
mod registers;

use super::{ConsoleKeys, FRAME_HEIGHT, FRAME_WIDTH, Gtia};
use crate::antic::{AnCode, CycleOutput};
use crate::timing::VideoStandard;

/// `COLPM0`.
const COLPM0: u8 = 0x12;
/// `COLPF0`.
const COLPF0: u8 = 0x16;
/// `COLPF1`.
const COLPF1: u8 = 0x17;
/// `COLPF2`.
const COLPF2: u8 = 0x18;
/// `COLBK`.
const COLBK: u8 = 0x1A;
/// `PRIOR`.
const PRIOR: u8 = 0x1B;
/// `GRACTL`.
const GRACTL: u8 = 0x1D;
/// `CONSOL`.
const CONSOL: u8 = 0x1F;

/// The code at HPOS `h` of a normal scan line: horizontal blank outside `$22-$DD`
/// (`%011` if `hires`), else `inner(h)`.
fn scan_line_code(hires: bool, h: u32, inner: &dyn Fn(u32) -> AnCode) -> AnCode {
    if (0x22..=0xDD).contains(&h) {
        inner(h)
    } else if hires {
        AnCode::HBLANK_HIRES
    } else {
        AnCode::HBLANK_LORES
    }
}

/// Feeds one scan line. `code(h)` gives the code of HPOS `h` (all 228).
fn feed_line(gtia: &mut Gtia, line: u32, code: &dyn Fn(u32) -> AnCode) {
    for cycle in 0..114 {
        let h = (2 * cycle + 226) % 228;
        let out = CycleOutput {
            line,
            cycle,
            video: [code(h), code(h + 1)],
            ..CycleOutput::default()
        };
        gtia.clock(&out, 0xFF);
    }
}

/// Feeds scan lines `lines` with the same playfield codes `inner(h)` in `$22-$DD`.
fn feed_lines(
    gtia: &mut Gtia,
    lines: impl Iterator<Item = u32>,
    hires: bool,
    inner: &dyn Fn(u32) -> AnCode,
) {
    for line in lines {
        feed_line(gtia, line, &|h| scan_line_code(hires, h, inner));
    }
}

/// Feeds a full NTSC frame (scan lines 0-261), so that `frame()` shows it.
fn feed_frame(gtia: &mut Gtia, hires: bool, inner: &dyn Fn(u32) -> AnCode) {
    feed_lines(gtia, 0..249, hires, inner);
    feed_lines(gtia, 249..262, hires, &|_| AnCode::HBLANK_LORES);
}

/// One row of the last frame.
fn row(gtia: &Gtia, row: usize) -> &[u8] {
    &gtia.frame()[row * FRAME_WIDTH..(row + 1) * FRAME_WIDTH]
}

/// The two columns of HPOS `h` in `row`.
fn at(gtia: &Gtia, row_index: usize, h: usize) -> [u8; 2] {
    let column = 2 * (h - 0x20);
    let r = row(gtia, row_index);
    [r[column], r[column + 1]]
}

/// Code `%000` everywhere.
fn background(_: u32) -> AnCode {
    AnCode::BACKGROUND
}

/// Verifies REQ-400: the registers repeat every 32 bytes in `$D000-$D0FF`.
#[test]
fn registers_mirror_every_32_bytes() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(0xFA, 0x34);
    gtia.collisions = [
        0x0F, 0x0E, 0x0D, 0x0C, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    ];
    for keys in 0..8_u8 {
        gtia.set_console_keys(ConsoleKeys {
            start: keys & 1 != 0,
            select: keys & 2 != 0,
            option: keys & 4 != 0,
        });
        for pressed in [false, true] {
            gtia.set_trigger(0, pressed);
            for addr in 0..=0xFF_u8 {
                assert_eq!(gtia.read(addr), gtia.read(addr & 0x1F), "${addr:02X}");
            }
        }
    }
    feed_frame(&mut gtia, false, &background);
    assert_eq!(at(&gtia, 100, 0x80), [0x34; 2], "COLBK written at $D0FA");
}

/// Verifies REQ-402: GTIA starts with zero state.
#[test]
fn power_on_state_is_zero() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    for reg in 0..16 {
        assert_eq!(gtia.read(reg), 0, "collision register {reg:X}");
    }
    assert_eq!(gtia.read(CONSOL), 0x0F, "CONSOL");
    feed_frame(&mut gtia, false, &|h| {
        AnCode::playfield(u8::try_from(h % 4).unwrap())
    });
    assert!(
        gtia.frame().iter().all(|&b| b == 0),
        "the first frame is $00"
    );
    for reg in 0x0D..=0x11 {
        gtia.write(reg, 0xFF);
    }
    for reg in 0x12..=0x15 {
        gtia.write(reg, 0x0E);
    }
    feed_frame(&mut gtia, false, &background);
    assert!(
        gtia.frame().iter().all(|&b| b != 0x0E),
        "the objects shift out in hblank"
    );
}

/// Verifies REQ-402: the trigger latches are clear after power-on.
#[test]
fn power_on_trigger_latches_clear() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(GRACTL, 0x04);
    for reg in 0x10..=0x13 {
        assert_eq!(gtia.read(reg), 0x01, "TRIG{}", reg - 0x10);
    }
}

/// Verifies REQ-403: `PAL` reads `$0F` on NTSC and `$01` on PAL.
#[test]
fn pal_register_ntsc_0f_pal_01() {
    assert_eq!(Gtia::new(VideoStandard::Ntsc).read(0x14), 0x0F, "NTSC");
    assert_eq!(Gtia::new(VideoStandard::Pal).read(0x14), 0x01, "PAL");
}

/// Puts player 0-3 at HPOS `$40 + $20 n` with `GRAFPn` = `$FF` (one-time latch writes).
fn place_players(gtia: &mut Gtia) {
    for n in 0..4 {
        gtia.write(n, 0x40 + 0x20 * n);
        gtia.write(0x0D + n, 0xFF);
    }
}

/// Verifies REQ-404: the color registers store the value with bit 0 cleared.
#[test]
fn color_registers_ignore_bit_0() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    place_players(&mut gtia);
    for reg in 0x12..=0x1A {
        gtia.write(reg, 0xFF);
    }
    gtia.write(PRIOR, 0x01);
    let inner = |h: u32| match h {
        0xB0..=0xB3 => AnCode::playfield(0),
        0xB4..=0xB7 => AnCode::playfield(1),
        0xB8..=0xBB => AnCode::playfield(2),
        0xBC..=0xBF => AnCode::playfield(3),
        _ => AnCode::BACKGROUND,
    };
    feed_frame(&mut gtia, false, &inner);
    for h in [0x40, 0x60, 0x80, 0xA0, 0xB0, 0xB4, 0xB8, 0xBC, 0xD0] {
        assert_eq!(at(&gtia, 0, h), [0xFE; 2], "layer at ${h:02X}");
    }
    gtia.write(COLPF2, 0x95);
    feed_frame(&mut gtia, false, &inner);
    assert_eq!(at(&gtia, 0, 0xB8), [0x94; 2], "COLPF2 = $95");
}

/// Verifies REQ-404: a color change in horizontal blank applies from that scan line.
#[test]
fn color_change_applies_to_later_lines() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(COLPF0, 0x28);
    let pf0 = |_: u32| AnCode::playfield(0);
    feed_lines(&mut gtia, 0..100, false, &pf0);
    let line_100 = |h| scan_line_code(false, h, &pf0);
    for cycle in 0..114 {
        if cycle == 5 {
            gtia.write(COLPF0, 0xC6);
        }
        let h = (2 * cycle + 226) % 228;
        let out = CycleOutput {
            line: 100,
            cycle,
            video: [line_100(h), line_100(h + 1)],
            ..CycleOutput::default()
        };
        gtia.clock(&out, 0xFF);
    }
    feed_lines(&mut gtia, 101..249, false, &pf0);
    for r in 0..FRAME_HEIGHT {
        let expected = if r < 92 { 0x28 } else { 0xC6 };
        assert!(
            row(&gtia, r)[4..380].iter().all(|&b| b == expected),
            "row {r}"
        );
    }
}

/// Verifies REQ-405: the frame buffer layout.
#[test]
fn frame_buffer_size_and_mapping() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    assert_eq!(gtia.frame().len(), 92_160, "size");
    gtia.write(COLPM0, 0x46);
    gtia.write(COLPF1, 0x3C);
    feed_lines(&mut gtia, 0..100, false, &background);
    gtia.write(0x00, 0x80);
    gtia.write(0x0D, 0x80);
    feed_lines(&mut gtia, 100..101, false, &|h| {
        if h == 0x50 {
            AnCode::playfield(1)
        } else {
            AnCode::BACKGROUND
        }
    });
    gtia.write(0x0D, 0x00);
    feed_lines(&mut gtia, 101..249, false, &background);
    let r = row(&gtia, 92);
    assert_eq!(r[192..194], [0x46, 0x46], "player at HPOS $80");
    assert_eq!(r[96..98], [0x3C, 0x3C], "PF1 at HPOS $50");
    assert_eq!(row(&gtia, 93)[192], 0x00, "one scan line only");
}

/// Verifies REQ-405: the front buffer changes on the first tick of scan line 248.
#[test]
fn frame_swaps_at_line_248() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(COLBK, 0x02);
    feed_frame(&mut gtia, false, &background);
    gtia.write(COLBK, 0x04);
    for line in 0..248 {
        feed_lines(&mut gtia, line..=line, false, &background);
        let all_02 = (0..FRAME_HEIGHT).all(|r| row(&gtia, r)[4..380].iter().all(|&b| b == 0x02));
        assert!(all_02, "frame N during scan line {line}");
    }
    let first = CycleOutput {
        line: 248,
        video: [AnCode::HBLANK_LORES; 2],
        ..CycleOutput::default()
    };
    gtia.clock(&first, 0xFF);
    let all_04 = (0..FRAME_HEIGHT).all(|r| row(&gtia, r)[4..380].iter().all(|&b| b == 0x04));
    assert!(all_04, "frame N + 1 after the first tick of scan line 248");
}

/// Checks that columns 0-3 and 380-383 are `$00` and columns 4-379 are `$46`.
fn check_blank_columns(gtia: &Gtia) {
    for r in 0..FRAME_HEIGHT {
        let line = row(gtia, r);
        assert!(
            line[..4].iter().chain(&line[380..]).all(|&b| b == 0),
            "row {r} edges"
        );
        assert!(line[4..380].iter().all(|&b| b == 0x46), "row {r} visible");
    }
}

/// Verifies REQ-406: HPOS `$20-$21` and `$DE-$DF` are `$00`.
#[test]
fn blank_columns_are_zero() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(COLBK, 0x46);
    feed_lines(&mut gtia, 0..249, false, &background);
    check_blank_columns(&gtia);
}

/// Verifies REQ-406: playfield codes in HPOS `$20-$21` and `$DE-$DF` still give `$00`.
#[test]
fn blank_columns_ignore_playfield_codes() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(COLPF0, 0x46);
    for line in 0..249 {
        feed_line(&mut gtia, line, &|h| {
            if (0x20..=0xDF).contains(&h) {
                AnCode::playfield(0)
            } else {
                AnCode::HBLANK_LORES
            }
        });
    }
    check_blank_columns(&gtia);
}

/// Verifies REQ-406: players show from HPOS `$22`.
#[test]
fn players_show_from_hpos_22() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(0x00, 0x20);
    gtia.write(0x0D, 0xFF);
    gtia.write(COLPM0, 0x1A);
    feed_lines(&mut gtia, 0..249, false, &background);
    let line = row(&gtia, 50);
    assert_eq!(line[..4], [0; 4], "columns 0-3");
    assert!(line[4..16].iter().all(|&b| b == 0x1A), "columns 4-15");
    assert_eq!(line[16], 0, "column 16");
}

/// A GTIA with the colors of REQ-408 and one scan line 100 of `codes` at `$40-$4F`.
fn hires_line(hires: bool, prior: u8, player: bool, codes: &[AnCode]) -> Gtia {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    for (reg, value) in [
        (COLBK, 0x02),
        (COLPF1, 0xCA),
        (COLPF2, 0x94),
        (COLPM0, 0x46),
    ] {
        gtia.write(reg, value);
    }
    gtia.write(PRIOR, prior);
    if player {
        gtia.write(0x00, 0x40);
        gtia.write(0x0D, 0xFF);
    }
    let inner = |h: u32| {
        let i = usize::try_from(h).unwrap();
        codes.get(i.wrapping_sub(0x40)).copied().unwrap_or_default()
    };
    feed_lines(&mut gtia, 0..249, hires, &inner);
    gtia
}

/// Verifies REQ-408: the hi-res pixel order and colors.
#[test]
fn hires_pixel_order() {
    let codes = [
        AnCode::hires(0b01),
        AnCode::hires(0b10),
        AnCode::hires(0b11),
        AnCode::hires(0b00),
        AnCode::BACKGROUND,
    ];
    let gtia = hires_line(true, 0x00, false, &codes);
    assert_eq!(at(&gtia, 92, 0x40), [0x94, 0x9A], "%101");
    assert_eq!(at(&gtia, 92, 0x41), [0x9A, 0x94], "%110");
    assert_eq!(at(&gtia, 92, 0x42), [0x9A, 0x9A], "%111");
    assert_eq!(at(&gtia, 92, 0x43), [0x94, 0x94], "%100");
    assert_eq!(at(&gtia, 92, 0x44), [0x02, 0x02], "%000");
}

/// Verifies REQ-408: the hi-res luminance is applied on top of a player.
#[test]
fn hires_luminance_over_players() {
    let gtia = hires_line(true, 0x01, true, &[AnCode::hires(0b10)]);
    assert_eq!(at(&gtia, 92, 0x40), [0x4A, 0x46], "player 0 on %110");
}

/// Verifies REQ-408: the priority logic sees `PF2` for hi-res codes.
#[test]
fn hires_priority_sees_pf2() {
    let gtia = hires_line(true, 0x04, true, &[AnCode::hires(0b00)]);
    assert_eq!(at(&gtia, 92, 0x40), [0x94, 0x94], "playfield over player 0");
}

/// Verifies REQ-408: the horizontal blank code sets the hires state.
#[test]
fn hblank_code_sets_hires_state() {
    let codes = [AnCode::playfield(1), AnCode::playfield(2)];
    let lores = hires_line(false, 0x00, false, &codes);
    assert_eq!(at(&lores, 92, 0x40), [0xCA, 0xCA], "%101 after %010");
    assert_eq!(at(&lores, 92, 0x41), [0x94, 0x94], "%110 after %010");
    let hires = hires_line(true, 0x00, false, &codes);
    assert_eq!(at(&hires, 92, 0x40), [0x94, 0x9A], "%101 after %011");
}

/// Verifies REQ-435: the trigger registers.
#[test]
fn trigger_reads() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    assert_eq!(
        [gtia.read(0x10), gtia.read(0x11)],
        [0x01, 0x01],
        "no button"
    );
    gtia.set_trigger(0, true);
    assert_eq!(
        [gtia.read(0x10), gtia.read(0x11)],
        [0x00, 0x01],
        "trigger 0"
    );
    gtia.set_trigger(0, false);
    gtia.set_trigger(1, true);
    assert_eq!(
        [gtia.read(0x10), gtia.read(0x11)],
        [0x01, 0x00],
        "trigger 1"
    );
    assert_eq!(gtia.read(0x12), 0x01, "TRIG2 with the buttons");
    gtia.set_trigger(2, true);
    gtia.write(GRACTL, 0x04);
    assert_eq!(gtia.read(0x12), 0x01, "TRIG2 with GRACTL = $04");
    gtia.set_trigger(3, true);
    assert_eq!(gtia.read(0x13), 0x00, "TRIG3 active: no cartridge");
    gtia.write(GRACTL, 0x00);
    gtia.set_trigger(3, false);
    assert_eq!(gtia.read(0x13), 0x01, "TRIG3 inactive: cartridge");
}

/// Verifies REQ-437: `CONSOL` reads the keys and the written bits.
#[test]
fn consol_keys_and_outputs() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    let read = |gtia: &mut Gtia, start, select, option| {
        gtia.set_console_keys(ConsoleKeys {
            start,
            select,
            option,
        });
        gtia.read(CONSOL)
    };
    gtia.write(CONSOL, 0x08);
    assert_eq!(read(&mut gtia, false, false, false), 0x07, "$08, no key");
    assert_eq!(read(&mut gtia, true, false, false), 0x06, "START");
    assert_eq!(read(&mut gtia, false, true, false), 0x05, "SELECT");
    assert_eq!(read(&mut gtia, false, false, true), 0x03, "OPTION");
    assert_eq!(
        read(&mut gtia, false, true, true),
        0x01,
        "SELECT and OPTION"
    );
    gtia.write(CONSOL, 0x0F);
    for keys in 0..8_u8 {
        let value = read(&mut gtia, keys & 1 != 0, keys & 2 != 0, keys & 4 != 0);
        assert_eq!(value, 0x00, "$0F, keys {keys}");
    }
    gtia.write(CONSOL, 0x00);
    assert_eq!(read(&mut gtia, false, false, false), 0x0F, "$00, no key");
    assert!(!gtia.speaker(), "speaker after $00");
}
