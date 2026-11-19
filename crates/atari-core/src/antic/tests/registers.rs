//! Registers, reset, frame timing and `VCOUNT` (REQ-300 to REQ-304, REQ-338).

use super::{Rig, every, refresh_cycles};
use crate::antic::DmaKind;
use crate::timing::{CYCLES_PER_LINE, VBLANK_START_LINE, VideoStandard};

/// A display list of mode 2 lines with DLI bits, and a JVB.
fn dli_mode_2_rig() -> Rig {
    let mut dl = vec![0xC2, 0x00, 0x20];
    dl.extend([0x82; 29]);
    dl.extend([0x41, 0x00, 0x10]);
    Rig::with_dl(0x3E, &dl)
}

/// Verifies REQ-300: each offset `$XY` reads the same value as `$0Y`.
#[test]
fn registers_mirror_every_16_bytes() {
    let mut rig = dli_mode_2_rig();
    rig.write(0x0E, 0xC0);
    for (line, cycle) in [(8, 3), (100, 50), (247, 111), (248, 8), (261, 113)] {
        rig.seek_after(line, cycle);
        for addr in 0..=0xFF_u8 {
            assert_eq!(
                rig.read(addr),
                rig.read(addr & 0x0F),
                "${addr:02X} on ({line}, {cycle})"
            );
        }
    }
}

/// Verifies REQ-300: a write to `$D4F0` is a write to `DMACTL`.
#[test]
fn dmactl_mirror_write_gives_same_output() {
    let mut dl = vec![0x4F, 0x00, 0x20];
    dl.extend([0x0F; 10]);
    dl.extend([0x41, 0x00, 0x10]);
    let mut a = Rig::with_dl(0x00, &dl);
    a.fill(0x2000, 0x400, 0x5A);
    let mut b = a.clone();
    a.seek_after(VBLANK_START_LINE, 20);
    b.seek_after(VBLANK_START_LINE, 20);
    a.write(0xF0, 0x22);
    b.write(0x00, 0x22);
    let ra = a.frame();
    assert!(ra.iter().any(|r| r.out.dma == DmaKind::Bitmap), "DMA runs");
    assert_eq!(ra, b.frame(), "the two instances give the same output");
}

/// Verifies REQ-300: `$D4FF` reads `$7F` after power-on, as `$D40F` does.
#[test]
fn nmist_mirror_d4ff_reads_7f() {
    let rig = Rig::new(VideoStandard::Ntsc);
    assert_eq!(rig.read(0xFF), 0x7F, "$D4FF");
    assert_eq!(rig.read(0x0F), 0x7F, "$D40F");
}

/// Verifies REQ-301: offsets `$0-$A` and `$E` read `$FF` after any write.
#[test]
fn unassigned_reads_return_ff() {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    for reg in 0..16 {
        for value in [0x00, 0x55, 0xAA, 0xFF] {
            rig.write(reg, value);
            let _rec = rig.step();
            for offset in (0..=0x0A).chain([0x0E]) {
                assert_eq!(
                    rig.read(offset),
                    0xFF,
                    "offset {offset:X} after ${value:02X} to {reg:X}"
                );
            }
        }
    }
}

/// Verifies REQ-301: the read offsets `$B`, `$C`, `$D` and `$F`.
#[test]
fn vcount_penh_penv_nmist_offsets() {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    assert_eq!(rig.read(0x0C), 0x00, "PENH at power-on");
    assert_eq!(rig.read(0x0D), 0xFF, "PENV at power-on");
    assert_eq!(rig.read(0x0F), 0x7F, "NMIST at power-on");
    rig.seek_after(100, 50);
    assert_eq!(rig.read(0x0B), 0x32, "VCOUNT on scan line 100, cycle 50");
    rig.write(0x0F, 0x00);
    assert_eq!(rig.read(0x0F), 0x1F, "NMIST after NMIRES");
}

/// Verifies REQ-301: writes to `$6`, `$8`, `$B`, `$C` and `$D` change nothing.
#[test]
fn writes_to_unused_offsets_are_ignored() {
    let mut a = dli_mode_2_rig();
    a.write(0x0E, 0xC0);
    let mut b = a.clone();
    a.seek_after(100, 50);
    b.seek_after(100, 50);
    for reg in [0x06, 0x08, 0x0B, 0x0C, 0x0D] {
        a.write(reg, 0xFF);
    }
    for _ in 0..2 * 29_868 {
        assert_eq!(a.step(), b.step(), "same output");
        for reg in [0x0B, 0x0C, 0x0D, 0x0F] {
            assert_eq!(a.read(reg), b.read(reg), "same read of {reg:X}");
        }
    }
}

/// Verifies REQ-301: reads have no side effects.
#[test]
fn reads_have_no_side_effects() {
    let mut a = dli_mode_2_rig();
    a.write(0x0E, 0xC0);
    let mut b = a.clone();
    for _ in 0..2 * 29_868 {
        let rec = a.step();
        for reg in 0..16 {
            let _value = a.read(reg);
        }
        assert_eq!(rec, b.step(), "same output");
    }
}

/// Verifies REQ-302: reset clears `NMIEN`, `DMACTL` and the DMA clock.
#[test]
fn reset_clears_nmien_dmactl_and_dma_clock() {
    let mut rig = dli_mode_2_rig();
    rig.write(0x0E, 0xC0);
    rig.seek_after(100, 50);
    assert_ne!(rig.antic.debug_state().dma_clock, 0, "the DMA clock runs");
    rig.antic.reset();
    let state = rig.antic.debug_state();
    assert_eq!(state.dmactl, 0, "DMACTL");
    assert_eq!(state.nmien, 0, "NMIEN");
    assert_eq!(state.dma_clock, 0, "DMA clock");
    let rest = rig.run_to(VBLANK_START_LINE, 0);
    let next = rig.frame();
    for rec in rest.iter().chain(&next) {
        assert!(
            matches!(rec.out.dma, DmaKind::Refresh | DmaKind::None),
            "only refresh on ({}, {})",
            rec.out.line,
            rec.out.cycle
        );
        assert!(!rec.out.nmi, "no NMI");
    }
}

/// Verifies REQ-302: reset keeps the other registers and counters.
#[test]
fn reset_keeps_other_state() {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    rig.seek_after(64, 32);
    for (reg, value) in [(4, 0x05), (5, 0x03), (7, 0x30), (9, 0xE0), (1, 0x02)] {
        rig.write(reg, value);
    }
    rig.write(2, 0x00);
    rig.write(3, 0x30);
    rig.seek_after(100, 50);
    let before = rig.antic.debug_state();
    let reads = [rig.read(0x0C), rig.read(0x0D), rig.read(0x0F)];
    rig.antic.reset();
    let after = rig.antic.debug_state();
    assert_eq!(
        (
            after.hscrol,
            after.vscrol,
            after.pmbase,
            after.chbase,
            after.chactl
        ),
        (0x05, 0x03, 0x30, 0xE0, 0x02),
        "registers"
    );
    assert_eq!(after.dlist, 0x3000, "display list address");
    assert_eq!(after.memscan, before.memscan, "memory scan counter");
    assert_eq!(after.refresh_row, before.refresh_row, "refresh row counter");
    assert_eq!(
        [rig.read(0x0C), rig.read(0x0D), rig.read(0x0F)],
        reads,
        "PENH, PENV, NMIST"
    );
    assert_eq!(
        reads[0..2],
        [0x00, 0xFF],
        "PENH and PENV keep their power-on values"
    );
}

/// Verifies REQ-302: reset keeps the scan line, the cycle and `VCOUNT`.
#[test]
fn reset_keeps_position_and_vcount() {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    rig.seek(128, 50);
    rig.antic.reset();
    assert_eq!(rig.pos(), (128, 50), "position");
    assert_eq!(rig.read(0x0B), 0x40, "VCOUNT");
}

/// Verifies REQ-302: reset keeps a `WSYNC` wait.
#[test]
fn reset_keeps_wsync_wait() {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    rig.seek_after(100, 50);
    rig.write(0x0A, 0x00);
    let _rec = rig.step();
    rig.antic.reset();
    let recs = rig.run_to(100, 106);
    for rec in &recs {
        let waits = rec.out.cycle <= 104;
        assert_eq!(rec.out.rdy, !waits, "RDY on cycle {}", rec.out.cycle);
    }
}

/// Verifies REQ-303: `PENH` = `$00`, `PENV` = `$FF`, `NMIST` = `$7F` after power-on.
#[test]
fn power_on_values_penh_penv_nmist() {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    assert_eq!(rig.read(0x0C), 0x00, "PENH");
    assert_eq!(rig.read(0x0D), 0xFF, "PENV");
    for _ in 0..2 * 29_868 {
        assert_eq!(rig.read(0x0F), 0x7F, "NMIST");
        let _rec = rig.step();
    }
}

/// Checks the scan line and cycle counters and `frame_done` for one standard.
fn check_frame_length(standard: VideoStandard, lines: u32) {
    let mut rig = Rig::new(standard);
    assert_eq!(rig.pos(), (0, 0), "power-on position");
    for line in 0..lines {
        for cycle in 0..CYCLES_PER_LINE {
            assert_eq!(rig.pos(), (line, cycle), "{standard:?} position");
            let _rec = rig.step();
        }
    }
    assert_eq!(
        rig.pos(),
        (0, 0),
        "{standard:?} wraps after scan line {}",
        lines - 1
    );
    let mut done = Vec::new();
    for t in 0..10 * u64::from(lines * CYCLES_PER_LINE) {
        if rig.step().out.frame_done {
            done.push(t);
        }
    }
    let gaps: Vec<u64> = done.windows(2).map(|w| w[1] - w[0]).collect();
    assert_eq!(done.len(), 10, "{standard:?}: 10 frames");
    assert!(
        gaps.iter()
            .all(|&g| g == u64::from(lines * CYCLES_PER_LINE)),
        "{standard:?}: frame lengths {gaps:?}"
    );
}

/// Verifies REQ-304: NTSC has 262 scan lines of 114 cycles.
#[test]
fn ntsc_frame_is_262_lines_of_114_cycles() {
    check_frame_length(VideoStandard::Ntsc, 262);
}

/// Verifies REQ-304: PAL has 312 scan lines of 114 cycles.
#[test]
fn pal_frame_is_312_lines_of_114_cycles() {
    check_frame_length(VideoStandard::Pal, 312);
}

/// Verifies REQ-304: the VBI NMIs are exactly one frame apart.
#[test]
fn vbi_nmi_period_ntsc_pal() {
    for (standard, period) in [(VideoStandard::Ntsc, 29_868), (VideoStandard::Pal, 35_568)] {
        let mut rig = Rig::new(standard);
        rig.write(0x0E, 0x40);
        let nmis: Vec<u64> = (0..10 * period).filter(|_| rig.step().out.nmi).collect();
        assert_eq!(nmis.len(), 10, "{standard:?}: one NMI per frame");
        assert!(
            nmis.windows(2).all(|w| w[1] - w[0] == period),
            "{standard:?}: NMI cycles {nmis:?}"
        );
    }
}

/// Reads `VCOUNT` on each cycle of one frame: `(line, cycle, value)`.
fn vcount_frame(standard: VideoStandard) -> Vec<(u32, u32, u8)> {
    let mut rig = Rig::new(standard);
    let cycles = standard.cycles_per_frame();
    (0..cycles)
        .map(|_| {
            let rec = rig.step();
            (rec.out.line, rec.out.cycle, rig.read(0x0B))
        })
        .collect()
}

/// Verifies REQ-338: `VCOUNT` shows the next scan line from cycle 111.
#[test]
fn vcount_changes_on_cycle_111() {
    for (line, cycle, value) in vcount_frame(VideoStandard::Ntsc) {
        if line == 261 {
            continue;
        }
        let expected = if cycle < 111 {
            line >> 1
        } else {
            (line + 1) >> 1
        };
        assert_eq!(u32::from(value), expected, "({line}, {cycle})");
    }
}

/// Verifies REQ-338: scan line 7 reads `$03` on cycle 110 and `$04` from cycle 111.
#[test]
fn vcount_line_7_reads_04() {
    let reads = vcount_frame(VideoStandard::Ntsc);
    let line_7: Vec<u8> = reads
        .iter()
        .filter(|r| r.0 == 7 && r.1 >= 110)
        .map(|r| r.2)
        .collect();
    assert_eq!(line_7, [0x03, 0x04, 0x04, 0x04], "cycles 110-113");
}

/// Verifies REQ-338: the first `$7C` is on cycle 111 of scan line 247.
#[test]
fn vcount_7c_for_13_cycles() {
    let reads = vcount_frame(VideoStandard::Ntsc);
    let first = reads.iter().position(|r| r.2 == 0x7C).unwrap();
    assert_eq!((reads[first].0, reads[first].1), (247, 111), "first $7C");
    let vbi = reads.iter().position(|r| (r.0, r.1) == (248, 10)).unwrap();
    assert_eq!(
        vbi - first,
        13,
        "13 cycles before cycle 10 of scan line 248"
    );
}

/// Checks the last scan line of a frame.
fn check_end_of_frame(standard: VideoStandard, last: u32, expected: [u8; 4]) {
    let reads = vcount_frame(standard);
    let values: Vec<u8> = reads
        .iter()
        .filter(|r| r.0 == last && r.1 >= 110)
        .map(|r| r.2)
        .collect();
    assert_eq!(
        values, expected,
        "{standard:?} cycles 110-113 of scan line {last}"
    );
}

/// Verifies REQ-338: the NTSC end-of-frame values.
#[test]
fn vcount_end_of_frame_anomaly_ntsc() {
    check_end_of_frame(VideoStandard::Ntsc, 261, [0x82, 0x83, 0x00, 0x00]);
}

/// Verifies REQ-338: the PAL end-of-frame values.
#[test]
fn vcount_end_of_frame_anomaly_pal() {
    check_end_of_frame(VideoStandard::Pal, 311, [0x9B, 0x9C, 0x00, 0x00]);
}

/// Verifies REQ-338: one frame gives every value up to the maximum, the maximum once.
#[test]
fn vcount_range() {
    for (standard, max) in [(VideoStandard::Ntsc, 0x83), (VideoStandard::Pal, 0x9C)] {
        let reads = vcount_frame(standard);
        for value in 0..=max {
            assert!(
                reads.iter().any(|r| r.2 == value),
                "{standard:?} ${value:02X}"
            );
        }
        let count = reads.iter().filter(|r| r.2 == max).count();
        assert_eq!(count, 1, "{standard:?} maximum on one cycle");
        assert!(reads.iter().all(|r| r.2 <= max), "{standard:?} range");
    }
}

/// Verifies REQ-310: with `DMACTL` = `$00`, only the refresh cycles halt.
#[test]
fn dmactl_zero_halts_only_refresh() {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    let frame = rig.frame();
    for rec in &frame {
        let refresh = refresh_cycles().contains(&rec.out.cycle);
        assert_eq!(
            rec.out.halt, refresh,
            "halt on ({}, {})",
            rec.out.line, rec.out.cycle
        );
        let kind = if refresh {
            DmaKind::Refresh
        } else {
            DmaKind::None
        };
        assert_eq!(rec.out.dma, kind, "DMA kind");
    }
    assert_eq!(every(25, 57, 4).len(), 9, "nine refresh cycles");
}
