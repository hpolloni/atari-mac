//! Trigger latch and console speaker tests (REQ-436, REQ-438).

use super::{CONSOL, Gtia, VideoStandard};
use crate::planned::planned;

/// Verifies REQ-436: with `GRACTL` = `$04`, a press and release of the port 1 button makes
/// `TRIG0` read `$00`; a write of `$00` to `GRACTL` makes it read `$01`; with `GRACTL` =
/// `$00`, `TRIG0` reads `$01` at the first read after the release (criteria 1-3).
#[test]
#[ignore = "planned: SLICE-04"]
fn trigger_latch_holds_until_cleared() {
    planned();
}

/// Verifies REQ-436: latches triggers 0, 1 and 3, and latches an input that is active when
/// `GRACTL` bit 2 is set (criteria 4 and 5).
#[test]
#[ignore = "planned: SLICE-04"]
fn trigger_latch_all_triggers() {
    planned();
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
