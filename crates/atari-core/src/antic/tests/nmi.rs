//! NMIs and `WSYNC` (REQ-305, REQ-331 to REQ-333, REQ-336).

use super::{Rec, Rig, fetch_lines};
use crate::timing::{VBLANK_START_LINE, VideoStandard};

/// A display list with a one-line DLI on scan line 40 and a JVB.
fn dli_40_rig(nmien: u8) -> Rig {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0x70, 0x70, 0x70, 0x80, 0x41, 0x00, 0x10]);
    rig.write(0x0E, nmien);
    rig
}

/// The scan lines with `out.nmi` in `recs`.
fn nmi_lines(recs: &[Rec]) -> Vec<u32> {
    recs.iter()
        .filter(|r| r.out.nmi)
        .map(|r| r.out.line)
        .collect()
}

/// Steps to the end of the frame and returns `(line, cycle, NMIST)` after each cycle.
fn nmist_frame(rig: &mut Rig) -> Vec<(u32, u32, u8, bool)> {
    let cycles = rig.antic.standard.cycles_per_frame();
    (0..cycles)
        .map(|_| {
            let rec = rig.step();
            (rec.out.line, rec.out.cycle, rig.read(0x0F), rec.out.nmi)
        })
        .collect()
}

/// The `NMIST` value after the tick of (`line`, `cycle`) in `reads`.
fn nmist_at(reads: &[(u32, u32, u8, bool)], line: u32, cycle: u32) -> u8 {
    reads
        .iter()
        .find(|r| (r.0, r.1) == (line, cycle))
        .map(|r| r.2)
        .unwrap()
}

/// Verifies REQ-305: vertical blank truncates a mode line, and its DLI does not occur.
#[test]
fn mode_line_truncated_at_248_skips_dli() {
    let mut dl = vec![0x70; 29];
    dl.extend([0x30, 0x82]);
    let mut rig = Rig::with_dl(0x22, &dl);
    rig.write(0x0E, 0xC0);
    let reads = nmist_frame(&mut rig);
    for read in reads
        .iter()
        .filter(|r| (244..VBLANK_START_LINE).contains(&r.0))
    {
        assert_eq!(read.2 & 0x80, 0, "NMIST bit 7 on ({}, {})", read.0, read.1);
    }
    let nmis: Vec<u32> = reads.iter().filter(|r| r.3).map(|r| r.0).collect();
    assert_eq!(nmis, [VBLANK_START_LINE], "only the VBI");
}

/// Verifies REQ-331: `NMIST` bits 4-0 always read 1.
#[test]
fn nmist_low_bits_read_one() {
    let mut rig = dli_40_rig(0xC0);
    let reads = nmist_frame(&mut rig);
    assert!(reads.iter().all(|r| r.2 & 0x1F == 0x1F), "bits 4-0");
    assert_eq!(reads.iter().filter(|r| r.3).count(), 2, "a DLI and a VBI");
}

/// Verifies REQ-331: the DLI sets bit 7 and clears bit 6.
#[test]
fn dli_clears_vbi_bit() {
    let mut rig = dli_40_rig(0xC0);
    let reads = nmist_frame(&mut rig);
    assert_eq!(nmist_at(&reads, 39, 113) & 0xC0, 0x40, "after the VBI");
    assert_eq!(nmist_at(&reads, 40, 7) & 0xC0, 0x80, "DLI on cycle 7");
}

/// Verifies REQ-331: the VBI sets bit 6 and clears bit 7.
#[test]
fn vbi_clears_dli_bit() {
    let mut rig = dli_40_rig(0xC0);
    let reads = nmist_frame(&mut rig);
    assert_eq!(nmist_at(&reads, 248, 6) & 0xC0, 0x80, "before the VBI");
    assert_eq!(nmist_at(&reads, 248, 7) & 0xC0, 0x40, "VBI on cycle 7");
}

/// Verifies REQ-331: `NMIST` changes also with the NMIs disabled.
#[test]
fn nmist_set_when_disabled() {
    let mut on = dli_40_rig(0xC0);
    let mut off = dli_40_rig(0x00);
    let reads_on = nmist_frame(&mut on);
    let reads_off = nmist_frame(&mut off);
    for (a, b) in reads_on.iter().zip(&reads_off) {
        assert_eq!(a.2, b.2, "NMIST on ({}, {})", a.0, a.1);
        assert!(!b.3, "no NMI");
    }
}

/// Verifies REQ-331: a write to `NMIRES` clears bits 7-5.
#[test]
fn nmires_clears_status() {
    for value in [0x00, 0xFF] {
        let mut rig = dli_40_rig(0xC0);
        rig.seek_after(40, 50);
        assert_eq!(rig.read(0x0F) & 0xC0, 0x80, "after the DLI");
        rig.write(0xFF, value);
        assert_eq!(rig.read(0xFF), 0x1F, "after a write of ${value:02X}");
    }
}

/// Verifies REQ-331: the DLI bit stays set until the VBI.
#[test]
fn dli_bit_stays_set() {
    let mut rig = dli_40_rig(0x80);
    let reads = nmist_frame(&mut rig);
    for read in &reads {
        let pos = (read.0, read.1);
        let set = ((40, 7)..(248, 7)).contains(&pos);
        assert_eq!(read.2 & 0x80 != 0, set, "bit 7 on {pos:?}");
    }
}

/// Verifies REQ-331: bit 5 stays 0 after `NMIRES`, also after a reset.
#[test]
fn nmist_bit5_stays_clear() {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    assert_eq!(rig.read(0x0F) & 0x20, 0x20, "power-on bit 5");
    rig.write(0x0F, 0);
    for t in 0..2 * 29_868 {
        if t == 10_000 {
            rig.antic.reset();
        }
        let _rec = rig.step();
        assert_eq!(rig.read(0x0F) & 0x20, 0, "bit 5 on cycle {t}");
    }
}

/// The DL with a `$82` line on scan lines 40-47.
fn dli_mode_2_rig(nmien: u8) -> Rig {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0x70, 0x70, 0x70, 0x82, 0x41, 0x00, 0x10]);
    rig.write(0x0E, nmien);
    rig.seek_after(39, 50);
    rig.write(0x0F, 0);
    rig
}

/// Checks the `NMIST` bit 7 of the `$82` line and returns the NMI lines.
fn check_dli_mode_2(nmien: u8) -> Vec<u32> {
    let mut rig = dli_mode_2_rig(nmien);
    let mut nmis = Vec::new();
    for _ in 0..(48 * 114 - (39 * 114 + 51)) {
        let rec = rig.step();
        let pos = (rec.out.line, rec.out.cycle);
        let set = pos >= (47, 7);
        assert_eq!(
            rig.read(0x0F) & 0x80 != 0,
            set,
            "NMIEN ${nmien:02X} bit 7 on {pos:?}"
        );
        if rec.out.nmi {
            nmis.push(rec.out.line);
        }
    }
    nmis
}

/// Verifies REQ-332: the DLI is on the last scan line of the mode line.
#[test]
fn dli_on_last_line_of_mode_line() {
    assert_eq!(check_dli_mode_2(0x80), [47], "one NMI on scan line 47");
}

/// Verifies REQ-332: `NMIEN` bit 7 masks the DLI, but not `NMIST`.
#[test]
fn dli_masked_by_nmien() {
    assert!(check_dli_mode_2(0x00).is_empty(), "no NMI");
}

/// Verifies REQ-332: a `$F0` line gives one DLI on its last scan line.
#[test]
fn dli_on_blank_line() {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0xF0, 0x41, 0x00, 0x10]);
    rig.write(0x0E, 0x80);
    let recs = rig.run_to(VBLANK_START_LINE, 0);
    assert_eq!(nmi_lines(&recs), [23], "one DLI");
}

/// Verifies REQ-332: a line that vertical scrolling shortens has its DLI on its last line.
#[test]
fn dli_on_shortened_line() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x20, 0xA2, 0x41, 0x00, 0x10]);
    rig.write(0x0E, 0x80);
    rig.write(0x05, 5);
    let recs = rig.run_to(VBLANK_START_LINE, 0);
    assert_eq!(
        fetch_lines(&recs),
        [8, 16, 19],
        "the $A2 line has 3 scan lines"
    );
    assert_eq!(nmi_lines(&recs), [18], "one DLI");
}

/// Verifies REQ-332: a jump with the DLI bit gives one DLI.
#[test]
fn dli_on_jump() {
    let mut rig = Rig::with_dl(0x22, &[0x70, 0x81, 0x00, 0x30]);
    rig.poke(0x3000, &[0x41, 0x00, 0x30]);
    rig.write(0x0E, 0x80);
    let recs = rig.run_to(VBLANK_START_LINE, 0);
    assert_eq!(nmi_lines(&recs), [16], "one DLI");
}

/// Verifies REQ-333: the VBI is on scan line 248 of each frame.
#[test]
fn vbi_on_line_248() {
    for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
        let mut rig = Rig::new(standard);
        rig.write(0x0E, 0x40);
        let mut recs = Vec::new();
        for _ in 0..3 {
            recs.extend(rig.frame());
        }
        assert_eq!(nmi_lines(&recs), [248; 3], "{standard:?}");
    }
}

/// Verifies REQ-333: `NMIEN` bit 6 masks the VBI, but not `NMIST`.
#[test]
fn vbi_masked_by_nmien() {
    let mut rig = Rig::new(VideoStandard::Ntsc);
    rig.seek_after(100, 10);
    rig.write(0x0F, 0);
    let recs = rig.after(248, 6);
    assert_eq!(rig.read(0x0F) & 0x40, 0, "bit 6 before cycle 7");
    let mut all = recs;
    for _ in 0..2 * 29_868 {
        let rec = rig.step();
        if rec.out.line >= 248 {
            assert_eq!(
                rig.read(0x0F) & 0x40,
                0x40,
                "bit 6 on ({}, {})",
                rec.out.line,
                rec.out.cycle
            );
        }
        all.push(rec);
    }
    assert!(nmi_lines(&all).is_empty(), "no NMI");
}

/// Verifies REQ-336: RDY is low from cycle c + 2 to cycle 104 after a `WSYNC` write.
#[test]
fn wsync_rdy_window() {
    for c in 0..=103 {
        let mut rig = Rig::new(VideoStandard::Ntsc);
        rig.seek_after(100, c);
        rig.write(0x0A, 0);
        let recs = rig.run_to(100, 106);
        for rec in &recs {
            let cycle = rec.out.cycle;
            let expected = !(c + 2..=104).contains(&cycle);
            assert_eq!(rec.out.rdy, expected, "write on {c}: RDY on cycle {cycle}");
        }
        assert_eq!(
            recs.len(),
            usize::try_from(105 - c).unwrap(),
            "cycles {}-105",
            c + 1
        );
    }
}
