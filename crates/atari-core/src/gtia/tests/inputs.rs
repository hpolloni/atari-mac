//! Trigger latch and console speaker tests (REQ-436, REQ-438).

use super::{CONSOL, GRACTL, Gtia, VideoStandard};

/// `TRIG0` (read). `TRIGn` is `TRIG0 + n`.
const TRIG0: u8 = 0x10;

/// `TRIGn`.
fn trig(gtia: &Gtia, n: u8) -> u8 {
    gtia.read(TRIG0 + n)
}

/// Verifies REQ-436: with `GRACTL` = `$04`, a press and release of the port 1 button makes
/// `TRIG0` read `$00`; a write of `$00` to `GRACTL` makes it read `$01`; with `GRACTL` =
/// `$00`, `TRIG0` reads `$01` at the first read after the release (criteria 1-3).
#[test]
fn trigger_latch_holds_until_cleared() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    // Criterion 1.
    gtia.write(GRACTL, 0x04);
    assert_eq!(trig(&gtia, 0), 0x01, "released before the press");
    gtia.set_trigger(0, true);
    assert_eq!(trig(&gtia, 0), 0x00, "pressed");
    gtia.set_trigger(0, false);
    assert_eq!(trig(&gtia, 0), 0x00, "latched after the release");
    // Criterion 2.
    gtia.write(GRACTL, 0x00);
    assert_eq!(trig(&gtia, 0), 0x01, "GRACTL = $00 clears the latch");
    // Criterion 3.
    gtia.set_trigger(0, true);
    assert_eq!(trig(&gtia, 0), 0x00, "pressed without the latch");
    gtia.set_trigger(0, false);
    assert_eq!(trig(&gtia, 0), 0x01, "first read after the release");
}

/// Verifies REQ-436: latches triggers 0, 1 and 3, and latches an input that is active when
/// `GRACTL` bit 2 is set (criteria 4 and 5).
#[test]
fn trigger_latch_all_triggers() {
    // Criterion 4.
    for n in [0_u8, 1, 3] {
        let mut gtia = Gtia::new(VideoStandard::Ntsc);
        gtia.write(GRACTL, 0x04);
        gtia.set_trigger(usize::from(n), true);
        gtia.set_trigger(usize::from(n), false);
        assert_eq!(trig(&gtia, n), 0x00, "TRIG{n} latched");
        for other in (0..4).filter(|&m| m != n) {
            assert_eq!(trig(&gtia, other), 0x01, "TRIG{other} after input {n}");
        }
        gtia.write(GRACTL, 0x00);
        assert_eq!(trig(&gtia, n), 0x01, "TRIG{n} after GRACTL = $00");
    }
    // Criterion 5.
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    gtia.set_trigger(0, true);
    gtia.write(GRACTL, 0x04);
    gtia.set_trigger(0, false);
    assert_eq!(trig(&gtia, 0), 0x00, "input active at the GRACTL write");
}

/// Verifies REQ-438: the GTIA speaker state follows `CONSOL` bit 3 of the last write: `$08`
/// and `$0F` give 1, `$00` and `$07` give 0 (criteria 1 and 2).
#[test]
fn speaker_follows_consol_bit_3() {
    let mut gtia = Gtia::new(VideoStandard::Ntsc);
    // Criterion 1.
    gtia.write(CONSOL, 0x08);
    assert!(gtia.speaker(), "after $08");
    gtia.write(CONSOL, 0x00);
    assert!(!gtia.speaker(), "after $00");
    // Criterion 2: only bit 3 of the last write counts.
    gtia.write(CONSOL, 0x08);
    gtia.write(CONSOL, 0x0F);
    assert!(gtia.speaker(), "after $08 and $0F");
    gtia.write(CONSOL, 0x00);
    gtia.write(CONSOL, 0x07);
    assert!(!gtia.speaker(), "after $00 and $07");
}
