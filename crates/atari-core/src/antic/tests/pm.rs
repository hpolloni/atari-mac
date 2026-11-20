//! Player/missile DMA and memory refresh (REQ-339 to REQ-341).

use super::{Rec, Rig, cycles_of, on_line, refresh_cycles};
use crate::antic::DmaKind;
use crate::timing::{VBLANK_START_LINE, VideoStandard};

/// A rig at power-on with `DMACTL` = `dmactl` and `PMBASE` = `pmbase`. Memory holds a
/// different byte at each address.
fn pm_rig(dmactl: u8, pmbase: u8) -> Rig {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    for addr in 0..=u16::MAX {
        let [lo, hi] = addr.to_le_bytes();
        rig.mem.ram[usize::from(addr)] = lo ^ hi.rotate_left(3);
    }
    rig.write(0x07, pmbase);
    rig.write(0x00, dmactl);
    rig
}

/// The kinds of P/M DMA cycles of `line`: `(cycle, kind)`.
fn pm_cycles(line: &[Rec]) -> Vec<(u32, DmaKind)> {
    line.iter()
        .filter(|rec| matches!(rec.out.dma, DmaKind::Missile | DmaKind::Player(_)))
        .map(|rec| (rec.out.cycle, rec.out.dma))
        .collect()
}

/// The DMA read address of the record of `kind` in `line`.
fn read_of(line: &[Rec], kind: DmaKind) -> Option<u16> {
    line.iter()
        .find(|rec| rec.out.dma == kind)
        .and_then(|rec| rec.read)
}

/// The missile and player 0-3 cycles.
fn all_pm() -> Vec<(u32, DmaKind)> {
    let mut out = vec![(0, DmaKind::Missile)];
    out.extend((0..4).map(|n| (u32::from(n) + 2, DmaKind::Player(n))));
    out
}

/// Verifies REQ-339: with `DMACTL` = `$04`, on scan lines 8-247, `out.dma` is `Missile` on
/// cycle 0 and never `Player`.
#[test]
fn missile_dma_on_cycle_0() {
    let mut rig = pm_rig(0x04, 0x30);
    let frame = rig.frame();
    for line in 8..VBLANK_START_LINE {
        assert_eq!(
            pm_cycles(&on_line(&frame, line)),
            [(0, DmaKind::Missile)],
            "line {line}"
        );
    }
}

/// Verifies REQ-339: with `DMACTL` = `$08`, on scan lines 8-247, `out.dma` is `Missile` on
/// cycle 0 and `Player` on cycles 2-5.
#[test]
fn player_dma_forces_missile_dma() {
    let mut rig = pm_rig(0x08, 0x30);
    let frame = rig.frame();
    for line in 8..VBLANK_START_LINE {
        assert_eq!(pm_cycles(&on_line(&frame, line)), all_pm(), "line {line}");
    }
}

/// Verifies REQ-339: with `DMACTL` = `$0C`, `out.dma` is `Player(0)` to `Player(3)` on
/// cycles 2 to 5.
#[test]
fn player_dma_cycles_2_to_5() {
    let mut rig = pm_rig(0x0C, 0x30);
    let frame = rig.frame();
    for line in 8..VBLANK_START_LINE {
        assert_eq!(pm_cycles(&on_line(&frame, line)), all_pm(), "line {line}");
    }
}

/// Verifies REQ-339: with `DMACTL` = `$0C`, `out.dma` is never `Missile` or `Player` on scan
/// lines 0-7 and from 248 to the end of the frame.
#[test]
fn no_pm_dma_in_vblank() {
    for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
        let mut rig = Rig::new(standard);
        rig.write(0x00, 0x0C);
        let frame = rig.frame();
        let lines = standard.lines_per_frame();
        for line in (0..8).chain(VBLANK_START_LINE..lines) {
            assert!(
                pm_cycles(&on_line(&frame, line)).is_empty(),
                "{standard:?} line {line}"
            );
        }
    }
}

/// Runs from (100, `cycle` + 1) to the end of line 101 after a write of `value` to `DMACTL`
/// on cycle `cycle` of scan line 100, from `DMACTL` = `from`. Returns the P/M cycles of
/// line 101.
fn pm_after_write(from: u8, value: u8, cycle: u32) -> Vec<(u32, DmaKind)> {
    let mut rig = pm_rig(from, 0x30);
    rig.seek_after(100, cycle);
    rig.write(0x00, value);
    rig.seek(101, 0);
    pm_cycles(&rig.line())
}

/// Verifies REQ-339: with `DMACTL` = `$0C`, a write of `$00` on cycle 112 of scan line 100
/// prevents the `Missile` fetch on cycle 0 of line 101; the same write on cycle 113 does
/// not.
#[test]
fn pm_dma_disable_two_cycles_ahead() {
    assert!(
        pm_after_write(0x0C, 0x00, 112).is_empty(),
        "write on cycle 112"
    );
    assert_eq!(
        pm_after_write(0x0C, 0x00, 113),
        [(0, DmaKind::Missile)],
        "write on cycle 113: the missile fetch still occurs"
    );
}

/// Verifies REQ-339: with `DMACTL` = `$00`, a write of `$04` on cycle 112 of scan line 100
/// gives a `Missile` fetch on cycle 0 of line 101; the same write on cycle 113 does not.
#[test]
fn pm_dma_enable_two_cycles_ahead() {
    assert_eq!(
        pm_after_write(0x00, 0x04, 112),
        [(0, DmaKind::Missile)],
        "write on cycle 112"
    );
    assert!(
        pm_after_write(0x00, 0x04, 113).is_empty(),
        "write on cycle 113"
    );
}

/// The fetch address of REQ-340 for a missile (`None`) or player `n`.
fn expected_address(pmbase: u8, one_line: bool, player: Option<u8>, y: u32) -> u16 {
    let y = u16::try_from(y).unwrap_or(0);
    if one_line {
        let base = u16::from(pmbase & 0xF8) << 8;
        player.map_or(base + 0x300 + y, |n| {
            base + 0x400 + u16::from(n) * 0x100 + y
        })
    } else {
        let base = u16::from(pmbase & 0xFC) << 8;
        player.map_or(base + 0x180 + (y >> 1), |n| {
            base + 0x200 + u16::from(n) * 0x80 + (y >> 1)
        })
    }
}

/// Verifies REQ-339: on each P/M DMA cycle, `out.halt` is set and `out.dma_data` is the byte
/// at the fetch address of REQ-340.
#[test]
fn pm_dma_data_on_bus() {
    for (dmactl, pmbase) in [(0x0C, 0x34), (0x1C, 0x6C)] {
        let mut rig = pm_rig(dmactl, pmbase);
        let ram = rig.mem.ram.clone();
        let frame = rig.frame();
        let mut count = 0;
        for rec in &frame {
            let player = match rec.out.dma {
                DmaKind::Missile => None,
                DmaKind::Player(n) => Some(n),
                _ => continue,
            };
            count += 1;
            let addr = expected_address(pmbase, dmactl & 0x10 != 0, player, rec.out.line);
            assert!(rec.out.halt, "halt on line {}", rec.out.line);
            assert_eq!(rec.read, Some(addr), "address on line {}", rec.out.line);
            assert_eq!(
                rec.out.dma_data,
                ram[usize::from(addr)],
                "data on line {}",
                rec.out.line
            );
        }
        assert_eq!(count, 5 * 240, "P/M fetches in the frame");
    }
}

/// Scan line 100 of a rig with `DMACTL` = `dmactl` and `PMBASE` = `pmbase`.
fn line_100(dmactl: u8, pmbase: u8) -> Vec<Rec> {
    let mut rig = pm_rig(dmactl, pmbase);
    rig.seek(100, 0);
    rig.line()
}

/// Verifies REQ-340: with `PMBASE` = `$30` and `DMACTL` = `$0C`, on scan line 100 the
/// `Missile` fetch reads `$31B2` and the `Player(2)` fetch reads `$3332`.
#[test]
fn pm_address_two_line() {
    let line = line_100(0x0C, 0x30);
    assert_eq!(read_of(&line, DmaKind::Missile), Some(0x31B2), "missile");
    assert_eq!(read_of(&line, DmaKind::Player(2)), Some(0x3332), "player 2");
    for n in 0..4 {
        assert_eq!(
            read_of(&line, DmaKind::Player(n)),
            Some(0x3032 + 0x200 + u16::from(n) * 0x80),
            "player {n}"
        );
    }
}

/// Verifies REQ-340: with `PMBASE` = `$34` and `DMACTL` = `$1C`, on scan line 100 the
/// `Missile` fetch reads `$3364` and the `Player(0)` fetch reads `$3464`.
#[test]
fn pm_address_one_line() {
    let line = line_100(0x1C, 0x34);
    assert_eq!(read_of(&line, DmaKind::Missile), Some(0x3364), "missile");
    assert_eq!(read_of(&line, DmaKind::Player(0)), Some(0x3464), "player 0");
    for n in 1..4 {
        assert_eq!(
            read_of(&line, DmaKind::Player(n)),
            Some(0x3464 + u16::from(n) * 0x100),
            "player {n}"
        );
    }
}

/// Verifies REQ-340: with `DMACTL` = `$0C`, the P/M fetches on scan lines 100 and 101 read
/// the same addresses, and fetches occur on both lines.
#[test]
fn pm_two_line_repeats() {
    let mut rig = pm_rig(0x0C, 0x30);
    rig.seek(100, 0);
    let lines = rig.lines(2);
    let reads = |line: &[Rec]| -> Vec<Option<u16>> {
        all_pm()
            .into_iter()
            .map(|(_, kind)| read_of(line, kind))
            .collect()
    };
    let first = reads(&lines[0]);
    assert!(first.iter().all(Option::is_some), "fetches on line 100");
    assert_eq!(first, reads(&lines[1]), "line 101 reads the same addresses");
}

/// Verifies REQ-340: with `PMBASE` = `$34`, a write of `$0C` to `DMACTL` after `$1C` makes
/// the `Missile` fetch on scan line 100 read `$35B2` (base `$3400`).
#[test]
fn pmbase_bit2_latent() {
    let mut rig = pm_rig(0x1C, 0x34);
    rig.seek(50, 0);
    rig.write(0x00, 0x0C);
    rig.seek(100, 0);
    let line = rig.line();
    assert_eq!(read_of(&line, DmaKind::Missile), Some(0x35B2), "missile");
}

/// Verifies REQ-341: with `DMACTL` = `$00`, `out.dma` is `Refresh` on cycles 25, 29, ..., 57
/// of every scan line, also in vertical blank.
#[test]
fn refresh_cycles_on_blank_line() {
    for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
        let mut rig = Rig::new(standard);
        let frame = rig.frame();
        for line in 0..standard.lines_per_frame() {
            assert_eq!(
                cycles_of(&on_line(&frame, line), DmaKind::Refresh),
                refresh_cycles(),
                "{standard:?} line {line}"
            );
        }
    }
}

/// The first scan line of a mode line `ir` with `DMACTL` = `dmactl` and `HSCROL` = `hscrol`.
fn first_line(dmactl: u8, ir: u8, hscrol: u8) -> Vec<Rec> {
    let mut rig = Rig::with_dl(dmactl, &[ir | 0x40, 0x00, 0x20, 0x41, 0x00, 0x10]);
    rig.write(0x04, hscrol);
    rig.line()
}

/// Verifies REQ-341: on the first scan line of a mode 2 line with `DMACTL` = `$22`,
/// `out.dma` is `Refresh` on cycle 98 only.
#[test]
fn refresh_in_mode_2_first_line() {
    let line = first_line(0x22, 0x02, 0);
    assert_eq!(cycles_of(&line, DmaKind::Refresh), [98], "refresh cycles");
}

/// Verifies REQ-341: on the first scan line of a mode 2 line with `DMACTL` = `$21`,
/// `out.dma` is `Refresh` on cycles 25 and 90 only.
#[test]
fn refresh_narrow_mode_2_two_cycles() {
    let line = first_line(0x21, 0x02, 0);
    assert_eq!(
        cycles_of(&line, DmaKind::Refresh),
        [25, 90],
        "refresh cycles"
    );
}

/// Verifies REQ-341: on each `Refresh` cycle, `out.halt` is set and the fake `DmaMemory`
/// gets no read.
#[test]
fn refresh_no_memory_read() {
    let mut dl = vec![0x42, 0x00, 0x20];
    dl.extend([0x02; 29]);
    dl.extend([0x41, 0x00, 0x10]);
    let mut rig = Rig::with_dl(0x2E, &dl);
    let frame = rig.frame();
    let refresh: Vec<&Rec> = frame
        .iter()
        .filter(|rec| rec.out.dma == DmaKind::Refresh)
        .collect();
    assert!(refresh.len() > 262, "refresh cycles in the frame");
    for rec in refresh {
        assert!(
            rec.out.halt,
            "halt on ({}, {})",
            rec.out.line, rec.out.cycle
        );
        assert_eq!(
            rec.read, None,
            "no read on ({}, {})",
            rec.out.line, rec.out.cycle
        );
    }
}

/// Verifies REQ-341: the HRM 4.14 chart cases. On the first scan line of a `$12` line with
/// `DMACTL` = `$21` and `HSCROL` = 14, `out.dma` is `Refresh` on cycles 26 and 105 only. On
/// the first scan line of a `$02` line with `DMACTL` = `$23` and `HSCROL` = 0, `out.dma` is
/// `Refresh` on cycle 106 only.
#[test]
fn refresh_chart_cases() {
    let scrolled = first_line(0x21, 0x12, 14);
    assert_eq!(
        cycles_of(&scrolled, DmaKind::Refresh),
        [26, 105],
        "$12 narrow, HSCROL = 14"
    );
    let wide = first_line(0x23, 0x02, 0);
    assert_eq!(cycles_of(&wide, DmaKind::Refresh), [106], "$02 wide");
}
