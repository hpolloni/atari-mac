//! Planned tests of player/missile DMA and memory refresh (REQ-339 to REQ-341).

use crate::planned::planned;

/// Verifies REQ-339: with `DMACTL` = `$04`, on scan lines 8-247, `out.dma` is `Missile` on
/// cycle 0 and never `Player`.
#[test]
#[ignore = "planned: SLICE-02"]
fn missile_dma_on_cycle_0() {
    planned();
}

/// Verifies REQ-339: with `DMACTL` = `$08`, on scan lines 8-247, `out.dma` is `Missile` on
/// cycle 0 and `Player` on cycles 2-5.
#[test]
#[ignore = "planned: SLICE-02"]
fn player_dma_forces_missile_dma() {
    planned();
}

/// Verifies REQ-339: with `DMACTL` = `$0C`, `out.dma` is `Player(0)` to `Player(3)` on
/// cycles 2 to 5.
#[test]
#[ignore = "planned: SLICE-02"]
fn player_dma_cycles_2_to_5() {
    planned();
}

/// Verifies REQ-339: with `DMACTL` = `$0C`, `out.dma` is never `Missile` or `Player` on scan
/// lines 0-7 and from 248 to the end of the frame.
#[test]
#[ignore = "planned: SLICE-02"]
fn no_pm_dma_in_vblank() {
    planned();
}

/// Verifies REQ-339: with `DMACTL` = `$0C`, a write of `$00` on cycle 112 of scan line 100
/// prevents the `Missile` fetch on cycle 0 of line 101; the same write on cycle 113 does
/// not.
#[test]
#[ignore = "planned: SLICE-02"]
fn pm_dma_disable_two_cycles_ahead() {
    planned();
}

/// Verifies REQ-339: with `DMACTL` = `$00`, a write of `$04` on cycle 112 of scan line 100
/// gives a `Missile` fetch on cycle 0 of line 101; the same write on cycle 113 does not.
#[test]
#[ignore = "planned: SLICE-02"]
fn pm_dma_enable_two_cycles_ahead() {
    planned();
}

/// Verifies REQ-339: on each P/M DMA cycle, `out.halt` is set and `out.dma_data` is the byte
/// at the fetch address of REQ-340.
#[test]
#[ignore = "planned: SLICE-02"]
fn pm_dma_data_on_bus() {
    planned();
}

/// Verifies REQ-340: with `PMBASE` = `$30` and `DMACTL` = `$0C`, on scan line 100 the
/// `Missile` fetch reads `$31B2` and the `Player(2)` fetch reads `$3332`.
#[test]
#[ignore = "planned: SLICE-02"]
fn pm_address_two_line() {
    planned();
}

/// Verifies REQ-340: with `PMBASE` = `$34` and `DMACTL` = `$1C`, on scan line 100 the
/// `Missile` fetch reads `$3364` and the `Player(0)` fetch reads `$3464`.
#[test]
#[ignore = "planned: SLICE-02"]
fn pm_address_one_line() {
    planned();
}

/// Verifies REQ-340: with `DMACTL` = `$0C`, the P/M fetches on scan lines 100 and 101 read
/// the same addresses, and fetches occur on both lines.
#[test]
#[ignore = "planned: SLICE-02"]
fn pm_two_line_repeats() {
    planned();
}

/// Verifies REQ-340: with `PMBASE` = `$34`, a write of `$0C` to `DMACTL` after `$1C` makes
/// the `Missile` fetch on scan line 100 read `$35B2` (base `$3400`).
#[test]
#[ignore = "planned: SLICE-02"]
fn pmbase_bit2_latent() {
    planned();
}

/// Verifies REQ-341: with `DMACTL` = `$00`, `out.dma` is `Refresh` on cycles 25, 29, ..., 57
/// of every scan line, also in vertical blank.
#[test]
#[ignore = "planned: SLICE-02"]
fn refresh_cycles_on_blank_line() {
    planned();
}

/// Verifies REQ-341: on the first scan line of a mode 2 line with `DMACTL` = `$22`,
/// `out.dma` is `Refresh` on cycle 98 only.
#[test]
#[ignore = "planned: SLICE-02"]
fn refresh_in_mode_2_first_line() {
    planned();
}

/// Verifies REQ-341: on the first scan line of a mode 2 line with `DMACTL` = `$21`,
/// `out.dma` is `Refresh` on cycles 25 and 90 only.
#[test]
#[ignore = "planned: SLICE-02"]
fn refresh_narrow_mode_2_two_cycles() {
    planned();
}

/// Verifies REQ-341: on each `Refresh` cycle, `out.halt` is set and the fake `DmaMemory`
/// gets no read.
#[test]
#[ignore = "planned: SLICE-02"]
fn refresh_no_memory_read() {
    planned();
}
