//! Planned tests of the display list and memory scan counters and of display list DMA off
//! (REQ-324 to REQ-326).

use crate::planned::planned;

/// Verifies REQ-324: an instruction `$02` at `$07FF` makes the next instruction fetch read
/// `$0400`.
#[test]
#[ignore = "planned: SLICE-02"]
fn dlist_wraps_in_1k_block() {
    planned();
}

/// Verifies REQ-324: an instruction `$42` at `$07FE` reads the low address byte from
/// `$07FF` and the high address byte from `$0400`, and the next instruction fetch reads
/// `$0401`.
#[test]
#[ignore = "planned: SLICE-02"]
fn dlist_wraps_inside_lms() {
    planned();
}

/// Verifies REQ-325: with `DMACTL` = `$22`, a mode E line with LMS `$2FF0` reads `$2FFF` on
/// its 16th fetch and `$2000` on its 17th fetch, and the next mode E line without LMS
/// starts at `$2018`.
#[test]
#[ignore = "planned: SLICE-02"]
fn memscan_wraps_in_4k_block() {
    planned();
}

/// Verifies REQ-325: an LMS of `$3FF0` after the wrapped lines makes the first fetch read
/// `$3FF0` (an LMS loads all 16 bits).
#[test]
#[ignore = "planned: SLICE-02"]
fn lms_loads_16_bits() {
    planned();
}

/// Verifies REQ-326: with `DMACTL` = `$02` and IR = `$02`, the display list address does not
/// change for a full frame, and `out.dma` is never `DisplayList` or `DisplayListAddress`.
#[test]
#[ignore = "planned: SLICE-02"]
fn dl_dma_off_keeps_address() {
    planned();
}

/// Verifies REQ-326: after a write of `$02` to `DMACTL` in a `$42 $00 $40` line, the next
/// mode lines repeat mode 2 with 8 scan lines, have no `DisplayListAddress` fetch, and each
/// first name fetch is `$28` higher than the one before.
#[test]
#[ignore = "planned: SLICE-02"]
fn dl_dma_off_repeats_ir() {
    planned();
}

/// Verifies REQ-326: with `DMACTL` = `$02`, IR = `$82` and `NMIEN` = `$80`, `NMIST` bit 7 is
/// set and one NMI occurs on the 8th scan line of each repeated mode 2 line only.
#[test]
#[ignore = "planned: SLICE-02"]
fn dl_dma_off_repeated_dli() {
    planned();
}

/// Verifies REQ-326: after a JVB on scan line N and a write of `$02` to `DMACTL`, positions
/// `$22-$DD` have code `%000` on lines N + 1 to 247 and on lines 8-247 of the next frame,
/// with no display list DMA.
#[test]
#[ignore = "planned: SLICE-02"]
fn dl_dma_off_after_jvb_is_blank() {
    planned();
}

/// Verifies REQ-326: a write of `$22` to `DMACTL` on cycle 113 of scan line N makes a
/// `DisplayList` fetch occur on cycle 1 of scan line N + 1.
#[test]
#[ignore = "planned: SLICE-02"]
fn dl_dma_enable_on_113_works() {
    planned();
}

/// Verifies REQ-326: a write of `$22` to `DMACTL` on cycle 0 of scan line N + 1 does not
/// make a `DisplayList` fetch occur on cycle 1 of that scan line.
#[test]
#[ignore = "planned: SLICE-02"]
fn dl_dma_enable_on_0_too_late() {
    planned();
}
