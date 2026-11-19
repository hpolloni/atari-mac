//! Planned tests of the NMI timing, the lost NMI and `WSYNC` (REQ-334, REQ-335, REQ-337).

use crate::planned::planned;

/// Verifies REQ-334: on a DLI scan line, after a write to `NMIRES` on the line before, a
/// read of `NMIST` on cycle 6 gives bit 7 = 0, and on cycle 7 bit 7 = 1.
#[test]
#[ignore = "planned: SLICE-02"]
fn nmist_visible_from_cycle_7() {
    planned();
}

/// Verifies REQ-334: a write of `$C0` to `NMIEN` on cycle 7 (from `$00`) still gives the NMI
/// of that scan line, with the start delayed (the earliest start is cycle 11).
#[test]
#[ignore = "planned: SLICE-02"]
fn nmien_enable_on_7_delays_nmi() {
    planned();
}

/// Verifies REQ-334: a write of `$C0` to `NMIEN` on cycle 8 (from `$00`) gives no NMI on that
/// scan line, and `NMIST` bit 7 still becomes set.
#[test]
#[ignore = "planned: SLICE-02"]
fn nmien_enable_on_8_too_late() {
    planned();
}

/// Verifies REQ-334: with `NMIEN` = `$C0`, a write of `$00` on cycle 8 prevents the NMI of
/// that scan line, and a write of `$00` on cycle 9 does not.
#[test]
#[ignore = "planned: SLICE-02"]
fn nmien_disable_on_8_prevents() {
    planned();
}

/// Verifies REQ-334: a write to `NMIRES` on cycle 8 does not prevent the NMI of that scan
/// line.
#[test]
#[ignore = "planned: SLICE-02"]
fn nmires_does_not_cancel() {
    planned();
}

/// Verifies REQ-335: a call of `irq_sequence_started` in cycle 4 of a DLI or VBI scan line
/// keeps `out.nmi` clear for that scan line while `NMIST` bit 7 or 6 is still set; a call
/// in cycle 3 or 5 does not change `out.nmi`.
#[test]
#[ignore = "planned: SLICE-02"]
fn irq_on_cycle_4_loses_nmi() {
    planned();
}

/// Verifies REQ-337: a `WSYNC` write on cycle 104 of scan line N keeps RDY low until the
/// `WSYNC` window of scan line N + 1 (supports criterion 1).
#[test]
#[ignore = "planned: SLICE-02"]
fn wsync_write_on_104_waits_next_line() {
    planned();
}

/// Verifies REQ-337: two `WSYNC` writes on consecutive cycles (as in `INC WSYNC`) give one
/// joined wait (supports criteria 2 and 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn wsync_windows_join() {
    planned();
}
