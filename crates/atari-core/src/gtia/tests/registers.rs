//! The read and write register map (REQ-401).

use core::array;

use super::{COLPF0, COLPM0, CONSOL, GRACTL, PRIOR, at, background, feed_frame, feed_lines};
use crate::antic::AnCode;
use crate::gtia::{ConsoleKeys, Gtia};
use crate::timing::VideoStandard;

/// `HITCLR`.
const HITCLR: u8 = 0x1E;

/// The 16 collision registers `$D000-$D00F`.
fn collisions(gtia: &Gtia) -> [u8; 16] {
    array::from_fn(|reg| gtia.read(u8::try_from(reg).unwrap()))
}

/// The collision registers that read `value` at `reg` and `$00` elsewhere.
fn only(reg: u8, value: u8) -> [u8; 16] {
    let mut expected = [0; 16];
    expected[usize::from(reg)] = value;
    expected
}

/// A frame of `PF0` codes at HPOS `$80-$87` and background elsewhere.
fn pf0_at_80(h: u32) -> AnCode {
    if (0x80..=0x87).contains(&h) {
        AnCode::playfield(0)
    } else {
        AnCode::BACKGROUND
    }
}

/// Puts player `n` at HPOS `$80` with `GRAFPn` = `$FF`.
fn player_at_80(gtia: &mut Gtia, n: u8) {
    gtia.write(n, 0x80);
    gtia.write(0x0D + n, 0xFF);
}

/// Puts missile `n` at HPOS `$80` with both of its `GRAFM` bits set.
fn missile_at_80(gtia: &mut Gtia, n: u8) {
    gtia.write(0x04 + n, 0x80);
    gtia.write(0x11, 0x03 << (2 * n));
}

/// Verifies REQ-401: each read address gives the read register of HRM Table 13
/// (criterion 1).
#[test]
fn reads_use_read_registers() {
    for n in 0..4 {
        let m = (n + 1) % 4;
        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        missile_at_80(&mut gtia, n);
        feed_frame(&mut gtia, false, &pf0_at_80);
        assert_eq!(collisions(&gtia), only(n, 0x01), "M{n}PF");

        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        player_at_80(&mut gtia, n);
        feed_frame(&mut gtia, false, &pf0_at_80);
        assert_eq!(collisions(&gtia), only(0x04 + n, 0x01), "P{n}PF");

        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        missile_at_80(&mut gtia, n);
        player_at_80(&mut gtia, m);
        feed_frame(&mut gtia, false, &background);
        assert_eq!(collisions(&gtia), only(0x08 + n, 1 << m), "M{n}PL, P{m}PF");

        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        player_at_80(&mut gtia, n);
        player_at_80(&mut gtia, m);
        feed_frame(&mut gtia, false, &background);
        let mut expected = only(0x0C + n, 1 << m);
        expected[usize::from(0x0C + m)] = 1 << n;
        assert_eq!(collisions(&gtia), expected, "P{n}PL and P{m}PL");
    }
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    for n in [0, 1, 3] {
        let reg = 0x10 + u8::try_from(n).unwrap();
        gtia.set_trigger(n, true);
        assert_eq!(gtia.read(reg), 0x00, "TRIG{n} active");
        gtia.set_trigger(n, false);
        assert_eq!(gtia.read(reg), 0x01, "TRIG{n} inactive");
    }
    assert_eq!(gtia.read(0x14), 0x0F, "PAL on NTSC (REQ-403)");
    assert_eq!(
        Gtia::new(VideoStandard::Pal).read(0x14),
        0x01,
        "PAL on PAL (REQ-403)"
    );
    assert_eq!(gtia.read(CONSOL), 0x0F, "CONSOL, no key (REQ-437)");
    gtia.set_console_keys(ConsoleKeys {
        start: true,
        ..ConsoleKeys::default()
    });
    assert_eq!(gtia.read(CONSOL), 0x0E, "CONSOL, START (REQ-437)");
}

/// A GTIA in the power-on state with `COLPM0-3` = `$0E` and `setup` written.
fn write_gtia(setup: &[(u8, u8)]) -> Gtia {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    for reg in 0x12..=0x15 {
        gtia.write(reg, 0x0E);
    }
    for &(reg, value) in setup {
        gtia.write(reg, value);
    }
    gtia
}

/// Feeds scan lines 0-99 with `inner`, writes `value` to `reg`, then feeds scan lines
/// 100-248, and returns the colors of HPOS `hpos` on scan line 100 (row 92).
fn after_write(
    gtia: &mut Gtia,
    inner: &dyn Fn(u32) -> AnCode,
    (reg, value): (u8, u8),
    hpos: &[usize],
) -> Vec<[u8; 2]> {
    feed_lines(gtia, 0..100, false, inner);
    gtia.write(reg, value);
    feed_lines(gtia, 100..249, false, inner);
    hpos.iter().map(|&h| at(gtia, 92, h)).collect()
}

/// Verifies REQ-401: each write address changes the write register of HRM Table 13, with
/// effect on the next scan line (criterion 2).
#[test]
fn writes_use_write_registers() {
    let on = [0x0E; 2];
    let off = [0x00; 2];
    for n in 0..4_u8 {
        let mut gtia = write_gtia(&[(0x0D + n, 0x80)]);
        let got = after_write(&mut gtia, &background, (n, 0x80), &[0x80, 0x81]);
        assert_eq!(got, [on, off], "HPOSP{n}");

        let mut gtia = write_gtia(&[(0x11, 0xFF)]);
        let got = after_write(
            &mut gtia,
            &background,
            (0x04 + n, 0x80),
            &[0x80, 0x81, 0x82],
        );
        assert_eq!(got, [on, on, off], "HPOSM{n}");

        let mut gtia = write_gtia(&[(n, 0x80), (0x0D + n, 0x80)]);
        let got = after_write(
            &mut gtia,
            &background,
            (0x08 + n, 0x01),
            &[0x80, 0x81, 0x82],
        );
        assert_eq!(got, [on, on, off], "SIZEP{n}");

        let mut gtia = write_gtia(&[(n, 0x80)]);
        let got = after_write(&mut gtia, &background, (0x0D + n, 0x80), &[0x80, 0x81]);
        assert_eq!(got, [on, off], "GRAFP{n}");
    }
    let mut gtia = write_gtia(&[(0x04, 0x80), (0x11, 0x02)]);
    let got = after_write(&mut gtia, &background, (0x0C, 0x01), &[0x80, 0x81, 0x82]);
    assert_eq!(got, [on, on, off], "SIZEM");

    let mut gtia = write_gtia(&[(0x04, 0x80)]);
    let got = after_write(&mut gtia, &background, (0x11, 0x02), &[0x80, 0x81]);
    assert_eq!(got, [on, off], "GRAFM");

    // $D012-$D01A: players 0-3 at $40, $50, $60, $70; PF0-PF3 at $90, $A0, $B0, $C0.
    let layers = |h: u32| match h {
        0x90 => AnCode::playfield(0),
        0xA0 => AnCode::playfield(1),
        0xB0 => AnCode::playfield(2),
        0xC0 => AnCode::playfield(3),
        _ => AnCode::BACKGROUND,
    };
    for (i, (reg, h)) in (0x12..=0x1A)
        .zip([0x40, 0x50, 0x60, 0x70, 0x90, 0xA0, 0xB0, 0xC0, 0xD0])
        .enumerate()
    {
        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        for n in 0..4 {
            gtia.write(n, 0x40 + 0x10 * n);
            gtia.write(0x0D + n, 0x80);
        }
        let got = after_write(&mut gtia, &layers, (reg, 0x46), &[h]);
        assert_eq!(
            got,
            [[0x46; 2]],
            "${:04X} (layer {i})",
            0xD000 + u16::from(reg)
        );
    }

    let mut gtia = write_gtia(&[(0x00, 0x80), (0x0D, 0xFF), (COLPM0, 0x40), (COLPF0, 0x0A)]);
    feed_frame(&mut gtia, false, &pf0_at_80);
    assert_eq!(at(&gtia, 92, 0x80), [0x4A; 2], "PRIOR before the write");
    let got = after_write(&mut gtia, &pf0_at_80, (PRIOR, 0x04), &[0x80]);
    assert_eq!(got, [[0x0A; 2]], "PRIOR = $04");

    // $D01C-$D01F: the tests of REQ-415 (VDELAY), REQ-414 (GRACTL), REQ-423 (HITCLR) and
    // REQ-437 (CONSOL) write these addresses. A short check of the last three:
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.set_trigger(0, true);
    gtia.write(GRACTL, 0x04);
    gtia.set_trigger(0, false);
    assert_eq!(gtia.read(0x10), 0x00, "GRACTL bit 2 latches TRIG0");
    player_at_80(&mut gtia, 0);
    feed_frame(&mut gtia, false, &pf0_at_80);
    assert_eq!(gtia.read(0x04), 0x01, "P0PF before HITCLR");
    gtia.write(HITCLR, 0x00);
    assert_eq!(collisions(&gtia), [0; 16], "HITCLR clears the collisions");
    gtia.write(CONSOL, 0x08);
    assert_eq!(gtia.read(CONSOL), 0x07, "CONSOL = $08");
}

/// The trigger inputs, `GRACTL` and console key states of the read checks.
fn input_states() -> Vec<([bool; 4], u8, ConsoleKeys)> {
    let mut states = Vec::new();
    for triggers in 0..16_u8 {
        for gractl in [0x00, 0x04] {
            let keys = ConsoleKeys {
                start: triggers & 1 != 0,
                select: triggers & 2 != 0,
                option: triggers & 4 != 0,
            };
            let bits = array::from_fn(|n| triggers & (1 << n) != 0);
            states.push((bits, gractl, keys));
        }
    }
    states
}

/// Verifies REQ-401: unused read bits are 0 (criteria 3 and 4).
#[test]
fn unused_read_bits_are_zero() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(0x00, 0x80);
    feed_frame(&mut gtia, false, &background);
    assert_eq!(gtia.read(0x00), 0x00, "3: $D000 after a write of $80");

    // All players and missiles at $80, over PF0-PF3 on scan lines that follow each other.
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    for n in 0..4 {
        player_at_80(&mut gtia, n);
        gtia.write(0x04 + n, 0x80);
    }
    gtia.write(0x11, 0xFF);
    for line in 0..249_u32 {
        let code = AnCode::playfield(u8::try_from(line % 4).unwrap());
        feed_lines(&mut gtia, line..=line, false, &|_| code);
    }
    let mut expected = [0x0F; 16];
    for n in 0..4 {
        expected[12 + n] = 0x0F & !(1 << n);
    }
    assert_eq!(collisions(&gtia), expected, "4: all 60 collision bits");
    for (triggers, gractl, keys) in input_states() {
        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        gtia.write(GRACTL, gractl);
        for (n, &active) in triggers.iter().enumerate() {
            gtia.set_trigger(n, active);
        }
        gtia.set_console_keys(keys);
        for reg in 0x10..=0x13 {
            assert!(gtia.read(reg) <= 0x01, "4: TRIG{} {triggers:?}", reg - 0x10);
        }
        for value in 0..=0xFF {
            gtia.write(CONSOL, value);
            assert!(gtia.read(CONSOL) <= 0x0F, "4: CONSOL after ${value:02X}");
        }
    }
}

/// Verifies REQ-401: `$D015-$D01E` read `$0F` with NTSC and with PAL (criterion 5).
#[test]
fn unassigned_reads_return_0f() {
    for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
        let gtia = Gtia::new(standard);
        for reg in 0x15..=0x1E {
            assert_eq!(
                gtia.read(reg),
                0x0F,
                "{standard:?} ${:04X}",
                0xD000 + u16::from(reg)
            );
        }
    }
}

/// Verifies REQ-401: two reads of the same address with nothing between them give the same
/// value, and reads do not change the GTIA state (criterion 6).
#[test]
fn reads_have_no_side_effects() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    player_at_80(&mut gtia, 0);
    player_at_80(&mut gtia, 1);
    missile_at_80(&mut gtia, 2);
    feed_frame(&mut gtia, false, &pf0_at_80);
    gtia.write(GRACTL, 0x04);
    gtia.set_trigger(1, true);
    gtia.set_console_keys(ConsoleKeys {
        select: true,
        ..ConsoleKeys::default()
    });
    let before = gtia.clone();
    for reg in 0x00..=0x1F {
        let first = gtia.read(reg);
        assert_eq!(gtia.read(reg), first, "${:04X}", 0xD000 + u16::from(reg));
    }
    assert!(gtia == before, "the reads did not change the state");
}
