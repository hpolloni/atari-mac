//! P/M graphics, collisions, GTIA modes and write delays.

use super::{COLBK, COLPF2, COLPM0, GRACTL, PRIOR, at, scan_line_code};
use crate::antic::{AnCode, CycleOutput, DmaKind};
use crate::gtia::Gtia;
use crate::timing::VideoStandard;

/// The P/M DMA of one cycle: `(kind, data bus)` for `(line, cycle)`.
type Dma<'a> = &'a dyn Fn(u32, u32) -> (DmaKind, u8);
/// A hook that runs before the `clock` of `(line, cycle)` (the CPU access of the cycle).
type Hook<'a> = &'a mut dyn FnMut(&mut Gtia, u32, u32);

/// No P/M DMA.
fn no_dma(_: u32, _: u32) -> (DmaKind, u8) {
    (DmaKind::None, 0xFF)
}

/// Feeds scan lines 0-248 with the codes `inner(h)` in `$22-$DD`, the P/M DMA `dma` and
/// the CPU accesses `hook`, so that `frame()` shows scan lines 8-247.
fn run_frame(
    gtia: &mut Gtia,
    hires: bool,
    inner: &dyn Fn(u32) -> AnCode,
    dma: Dma<'_>,
    hook: Hook<'_>,
) {
    for line in 0..249 {
        for cycle in 0..114 {
            hook(gtia, line, cycle);
            let h = (2 * cycle + 226) % 228;
            let (kind, bus) = dma(line, cycle);
            let out = CycleOutput {
                line,
                cycle,
                halt: kind != DmaKind::None,
                dma: kind,
                video: [
                    scan_line_code(hires, h, inner),
                    scan_line_code(hires, h + 1, inner),
                ],
                ..CycleOutput::default()
            };
            gtia.clock(&out, bus);
        }
    }
}

/// Like `run_frame` with no DMA and no CPU access.
fn plain_frame(gtia: &mut Gtia, hires: bool, inner: &dyn Fn(u32) -> AnCode) {
    run_frame(gtia, hires, inner, &no_dma, &mut |_, _, _| {});
}

/// A GTIA with the setup of REQ-414: `HPOSP0` = `$80`, `HPOSM0` = `$90`, `COLPM0` = `$46`,
/// `COLBK` = `$02`.
fn pm_gtia() -> Gtia {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    for (reg, value) in [(0x00, 0x80), (0x04, 0x90), (COLPM0, 0x46), (COLBK, 0x02)] {
        gtia.write(reg, value);
    }
    gtia
}

/// Background codes.
fn background(_: u32) -> AnCode {
    AnCode::BACKGROUND
}

/// Verifies REQ-414: `GRACTL` selects which DMA bytes go to the latches.
#[test]
fn gractl_gates_dma() {
    let mut gtia = pm_gtia();
    gtia.write(GRACTL, 0x03);
    let dma = |line: u32, cycle| match cycle {
        0 => (DmaKind::Missile, 0x00),
        2 => (
            DmaKind::Player(0),
            if line.is_multiple_of(2) { 0x80 } else { 0x40 },
        ),
        _ => (DmaKind::None, 0xFF),
    };
    run_frame(&mut gtia, false, &background, &dma, &mut |_, _, _| {});
    for r in 0..240 {
        let (on, off) = if r % 2 == 0 {
            (0x80, 0x81)
        } else {
            (0x81, 0x80)
        };
        assert_eq!(at(&gtia, r, on), [0x46; 2], "row {r} player");
        assert_eq!(at(&gtia, r, off), [0x02; 2], "row {r} background");
    }
    let mut gtia = pm_gtia();
    gtia.write(GRACTL, 0x01);
    let dma = |_, cycle| match cycle {
        0 => (DmaKind::Missile, 0x03),
        2 => (DmaKind::Player(0), 0x80),
        _ => (DmaKind::None, 0xFF),
    };
    run_frame(&mut gtia, false, &background, &dma, &mut |_, _, _| {});
    for r in [0, 100, 239] {
        assert_eq!(at(&gtia, r, 0x90), [0x46; 2], "row {r} missile 0");
        assert_eq!(at(&gtia, r, 0x91), [0x46; 2], "row {r} missile 0");
        assert_eq!(at(&gtia, r, 0x80), [0x02; 2], "row {r} no player");
    }
}

/// Verifies REQ-415: with `VDELAY` bit 4, player 0 loads DMA bytes on odd scan lines only.
#[test]
fn vdelay_accepts_odd_lines_only() {
    for vdelay in [0x10, 0x00] {
        let mut gtia = pm_gtia();
        gtia.write(GRACTL, 0x02);
        gtia.write(0x1C, vdelay);
        let dma = |line: u32, cycle| match cycle {
            2 => (DmaKind::Player(0), if line % 2 == 1 { 0x80 } else { 0x40 }),
            _ => (DmaKind::None, 0xFF),
        };
        run_frame(&mut gtia, false, &background, &dma, &mut |_, _, _| {});
        for line in 10..247_usize {
            let h = if vdelay == 0x10 || line % 2 == 1 {
                0x80
            } else {
                0x81
            };
            assert_eq!(
                at(&gtia, line - 8, h),
                [0x46; 2],
                "VDELAY ${vdelay:02X} scan line {line}"
            );
        }
    }
}

/// Verifies REQ-441: with missile DMA only, the players load the bus on cycles 2-5.
#[test]
fn players_load_bus_without_player_dma() {
    for gractl in [0x03, 0x01] {
        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        for n in 0..4 {
            gtia.write(n, 0x40 + 0x10 * n);
            gtia.write(COLPM0 + n, 0x46);
        }
        gtia.write(GRACTL, gractl);
        let dma = |_, cycle| match cycle {
            0 => (DmaKind::Missile, 0x00),
            2 => (DmaKind::None, 0x81),
            3 => (DmaKind::None, 0x42),
            4 => (DmaKind::None, 0x24),
            5 => (DmaKind::None, 0x18),
            _ => (DmaKind::None, 0xFF),
        };
        run_frame(&mut gtia, false, &background, &dma, &mut |_, _, _| {});
        for (n, pattern) in [0x81_u8, 0x42, 0x24, 0x18].into_iter().enumerate() {
            for bit in 0..8 {
                let shown = gractl == 0x03 && pattern & (0x80 >> bit) != 0;
                let expected = if shown { 0x46 } else { 0x00 };
                let h = 0x40 + 0x10 * n + bit;
                assert_eq!(
                    at(&gtia, 50, h),
                    [expected; 2],
                    "GRACTL {gractl} player {n} bit {bit}"
                );
            }
        }
    }
}

/// Verifies REQ-420: player and missile collisions with the playfield.
#[test]
fn pm_playfield_collisions() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    // Player 0 at $80 on PF1, missile 3 at $A0 on PF3, player 1 and missile 0 on
    // background.
    for (reg, value) in [(0x00, 0x80), (0x0D, 0xFF), (0x07, 0xA0), (0x01, 0x60)] {
        gtia.write(reg, value);
    }
    for (reg, value) in [(0x0E, 0xFF), (0x04, 0x30), (0x11, 0xC3)] {
        gtia.write(reg, value);
    }
    let inner = |h| match h {
        0x80..=0x87 => AnCode::playfield(1),
        0xA0..=0xA1 => AnCode::playfield(3),
        _ => AnCode::BACKGROUND,
    };
    plain_frame(&mut gtia, false, &inner);
    let mut expected = [0_u8; 16];
    expected[3] = 0x08;
    expected[4] = 0x02;
    for (reg, value) in expected.iter().enumerate() {
        let reg8 = u8::try_from(reg).unwrap();
        assert_eq!(gtia.read(reg8), *value, "collision register {reg:X}");
    }
    gtia.write(0x1E, 0);
    let hires = |h| match h {
        0x80..=0x83 => AnCode::hires(0b00),
        0x84 => AnCode::hires(0b01),
        _ => AnCode::BACKGROUND,
    };
    gtia.write(0x0D, 0xF0);
    plain_frame(&mut gtia, true, &hires);
    assert_eq!(gtia.read(0x04), 0x00, "%100 under player 0");
    gtia.write(0x0D, 0xF8);
    plain_frame(&mut gtia, true, &hires);
    assert_eq!(gtia.read(0x04), 0x04, "%101 under player 0");
}

/// Verifies REQ-421: player-player collisions.
#[test]
fn player_player_collisions() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(0x03, 0x40);
    gtia.write(0x10, 0xFF);
    plain_frame(&mut gtia, false, &background);
    assert_eq!(gtia.read(0x0F), 0x00, "player 3 alone");
    for n in 0..3 {
        gtia.write(n, 0x80);
        gtia.write(0x0D + n, 0xFF);
    }
    plain_frame(&mut gtia, false, &background);
    let pl: Vec<u8> = (0x0C..=0x0F).map(|reg| gtia.read(reg)).collect();
    assert_eq!(pl, [0x06, 0x05, 0x03, 0x00], "P0PL-P3PL");
    gtia.write(0x1E, 0);
    assert!((0..16).all(|reg| gtia.read(reg) == 0), "HITCLR clears all");
}

/// Verifies REQ-421: missile-player collisions.
#[test]
fn missile_player_collisions() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(0x03, 0x80);
    gtia.write(0x10, 0xFF);
    gtia.write(0x06, 0x82);
    gtia.write(0x11, 0x30);
    plain_frame(&mut gtia, false, &background);
    assert_eq!(gtia.read(0x0A), 0x08, "M2PL");
    assert_eq!(gtia.read(0x0F), 0x00, "P3PL");
}

/// The codes of GTIA mode pixel `p` (even HPOS gives bits 3-2, odd HPOS bits 1-0).
fn pixel_codes(p: u8) -> impl Fn(u32) -> AnCode {
    move |h| {
        let bits = if h % 2 == 0 { p >> 2 } else { p };
        AnCode::hires(bits & 3)
    }
}

/// The color of GTIA mode pixel `p` with `PRIOR` = `prior` and `COLBK` = `colbk`.
fn gtia_mode_color(prior: u8, colbk: u8, p: u8) -> [u8; 2] {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(PRIOR, prior);
    gtia.write(COLBK, colbk);
    plain_frame(&mut gtia, false, &pixel_codes(p));
    at(&gtia, 100, 0x80)
}

/// Verifies REQ-429: mode 9 gives the luminance of the pixel.
#[test]
fn mode_9_luminance() {
    assert_eq!(gtia_mode_color(0x40, 0x90, 7), [0x97; 2], "pixel 7");
    assert_eq!(gtia_mode_color(0x40, 0x00, 15), [0x0F; 2], "pixel 15");
    assert_eq!(gtia_mode_color(0x40, 0x00, 14), [0x0E; 2], "pixel 14");
}

/// Verifies REQ-431: mode 11 gives the hue of the pixel.
#[test]
fn mode_11_hues() {
    assert_eq!(gtia_mode_color(0xC0, 0x06, 3), [0x36; 2], "pixel 3");
    assert_eq!(gtia_mode_color(0xC0, 0x06, 0), [0x00; 2], "pixel 0");
    assert_eq!(
        gtia_mode_color(0xC0, 0x16, 0),
        [0x10; 2],
        "pixel 0 with COLBK $16"
    );
}

/// Verifies REQ-430: mode 10 uses the 9 color registers, one color clock late.
#[test]
fn mode_10_register_map() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    for (n, reg) in (0x12..=0x1A).enumerate() {
        gtia.write(reg, u8::try_from(0x10 * (n + 1)).unwrap());
    }
    gtia.write(PRIOR, 0x80);
    plain_frame(&mut gtia, false, &pixel_codes(5));
    assert_eq!(at(&gtia, 100, 0x81), [0x60; 2], "pixel 5 is COLPF1");
    plain_frame(&mut gtia, false, &pixel_codes(2));
    assert_eq!(at(&gtia, 100, 0x81), [0x30; 2], "pixel 2 is COLPM2");
    plain_frame(&mut gtia, false, &pixel_codes(9));
    assert_eq!(at(&gtia, 100, 0x81), [0x90; 2], "pixel 9 is COLBK");
}

/// One frame with player 0 and a CPU write of `value` to `reg` on cycle `cycle` of
/// scan line 100 (row 92). The other scan lines have no write.
fn write_on_cycle(gtia: &mut Gtia, inner: &dyn Fn(u32) -> AnCode, cycle: u32, write: (u8, u8)) {
    let (reg, value) = write;
    run_frame(gtia, false, inner, &no_dma, &mut |gtia, l, c| {
        if (l, c) == (100, cycle) {
            gtia.write(reg, value);
        }
    });
}

/// Verifies REQ-434: the write delays of HRM Table 18.
#[test]
fn write_delays_table_18() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    for (reg, value) in [
        (COLBK, 0x02),
        (0x00, 0x78),
        (0x0D, 0xFF),
        (0x08, 0x03),
        (COLPM0, 0x16),
    ] {
        gtia.write(reg, value);
    }
    write_on_cycle(&mut gtia, &background, 65, (COLPM0, 0x46));
    assert_eq!(at(&gtia, 92, 0x80), [0x16; 2], "COLPM0 at $80");
    assert_eq!(at(&gtia, 92, 0x81), [0x46; 2], "COLPM0 at $81");
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    for (reg, value) in [(COLBK, 0x90), (COLPF2, 0x38)] {
        gtia.write(reg, value);
    }
    let pf2 = |h| {
        if h >= 0x78 {
            AnCode::playfield(2)
        } else {
            AnCode::BACKGROUND
        }
    };
    write_on_cycle(&mut gtia, &pf2, 65, (PRIOR, 0x40));
    assert_eq!(at(&gtia, 92, 0x83), [0x38; 2], "PRIOR bits 6-7 at $83");
    assert_eq!(at(&gtia, 92, 0x84), [0x9A; 2], "PRIOR bits 6-7 at $84");
}

/// Verifies REQ-434: the base of the delay is HPOS 2c - 2 also on other cycles.
#[test]
fn write_delay_base_other_cycle() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.write(COLBK, 0x02);
    write_on_cycle(&mut gtia, &background, 30, (COLBK, 0x46));
    assert_eq!(at(&gtia, 92, 0x3A), [0x02; 2], "HPOS $3A");
    assert_eq!(at(&gtia, 92, 0x3B), [0x46; 2], "HPOS $3B");
}
