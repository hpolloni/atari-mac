//! Planned tests of the light pen latch (REQ-342).

use crate::planned::planned;

/// Verifies REQ-342: an input change to asserted on scan line 100, cycle 50 makes `PENV`
/// read `$32` and `PENH` read `$64`.
#[test]
#[ignore = "planned: SLICE-02"]
fn light_pen_latches_on_edge() {
    planned();
}

/// Verifies REQ-342: an input change to asserted on scan line 101, cycle 112 makes `PENV`
/// read `$33`.
#[test]
#[ignore = "planned: SLICE-02"]
fn light_pen_penv_next_line_after_111() {
    planned();
}

/// Verifies REQ-342: for an input change on cycle c (0-113), `PENH` reads 2 × c.
#[test]
#[ignore = "planned: SLICE-02"]
fn light_pen_penh_is_two_times_cycle() {
    planned();
}

/// Verifies REQ-342: an input that stays asserted for 2 frames keeps the latched `PENH` and
/// `PENV` values.
#[test]
#[ignore = "planned: SLICE-02"]
fn light_pen_held_does_not_relatch() {
    planned();
}

/// Verifies REQ-342: `PENH` and `PENV` keep their values across vertical blank and across a
/// reset.
#[test]
#[ignore = "planned: SLICE-02"]
fn light_pen_kept_across_vblank_and_reset() {
    planned();
}
