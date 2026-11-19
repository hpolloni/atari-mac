//! Planned tests of horizontal and vertical scrolling (REQ-327 to REQ-330).

use crate::planned::planned;

/// Verifies REQ-327: with `HSCROL` = 0 and IR = `$1E`, `Bitmap` fetches occur on cycles
/// 20-98 (`DMACTL` = `$21`) and on cycles 12-106 (`DMACTL` = `$22` and `$23`), every second
/// cycle.
#[test]
#[ignore = "planned: SLICE-02"]
fn hscrol_widens_fetch() {
    planned();
}

/// Verifies REQ-327: with `HSCROL` = 0, a `$52` line with LMS address A shows the name at
/// A + 4 at positions `$30-$33`, and the memory scan counter is A + 48 after the line.
#[test]
#[ignore = "planned: SLICE-02"]
fn hscrol_0_windows_wider_playfield() {
    planned();
}

/// Verifies REQ-327: with `HSCROL` = 4, the same `$52` line shows the name at A + 3 at
/// positions `$30-$33`.
#[test]
#[ignore = "planned: SLICE-02"]
fn hscrol_4_shifts_one_char() {
    planned();
}

/// Verifies REQ-327: with `DMACTL` = `$22` and `HSCROL` = 6, the first `CharacterName`
/// fetch of a `$12` line is on cycle 13.
#[test]
#[ignore = "planned: SLICE-02"]
fn hscrol_delays_fetch() {
    planned();
}

/// Verifies REQ-327: `HSCROL` = 5 and 4 give the same `out.dma` on a `$12` line, and the
/// code at x with 5 equals the code at x - 1 with 4, for x from `$31` to `$CF`.
#[test]
#[ignore = "planned: SLICE-02"]
fn hscrol_odd_adds_one_color_clock() {
    planned();
}

/// Verifies REQ-327: for each `HSCROL` 0-15, a `$1F` line with bytes `$FF` has code `%111`
/// on exactly `$30-$CF` (`DMACTL` = `$22`) or `$40-$BF` (`DMACTL` = `$21`), and `%000` on the
/// rest of `$22-$DD`.
#[test]
#[ignore = "planned: SLICE-02"]
fn hscrol_window_width_unchanged() {
    planned();
}

/// Verifies REQ-327: a write of `$02` to `HSCROL` on cycle 110 of the first scan line of a
/// `$1A` line moves the second scan line: its code at x equals the code at x - 2 of the
/// first scan line, for x from `$32` to `$CF`.
#[test]
#[ignore = "planned: SLICE-02"]
fn hscrol_change_between_rows() {
    planned();
}

/// Verifies REQ-328: with `VSCROL` = 3, the instructions `$22`, `$22`, `$02` start on lines
/// N, N + 5 and N + 13, the next fetch is on line N + 17, and the second `$22` line has
/// rows 0-7.
#[test]
#[ignore = "planned: SLICE-02"]
fn vscroll_region_17_lines() {
    planned();
}

/// Verifies REQ-328: a write of `$F3` to `VSCROL` gives the same result as `$03`.
#[test]
#[ignore = "planned: SLICE-02"]
fn vscrol_high_bits_ignored() {
    planned();
}

/// Verifies REQ-328: with `VSCROL` = 13, a `$2F` line after a line with bit 5 clear has rows
/// 13, 14, 15 and 0, `Bitmap` DMA only on its first scan line, and the next fetch 4 scan
/// lines later.
#[test]
#[ignore = "planned: SLICE-02"]
fn vscroll_4bit_row_wrap_mode_f() {
    planned();
}

/// Verifies REQ-328: a `$70` line after a `$22` line (`VSCROL` = 2) has 3 scan lines, after
/// a `$02` line (`VSCROL` = 4) 8 scan lines, and a jump after a `$22` line (`VSCROL` = 2) 3
/// scan lines.
#[test]
#[ignore = "planned: SLICE-02"]
fn vscroll_region_ends_on_blank_line() {
    planned();
}

/// Verifies REQ-328: with `DMACTL` = `$02`, IR = `$22` and `VSCROL` = 4, the mode 2 line on
/// scan line 8 has 8 scan lines (bit 5 of the line before vertical blank is kept).
#[test]
#[ignore = "planned: SLICE-02"]
fn vscroll_bit_kept_across_vblank() {
    planned();
}

/// Verifies REQ-329: a write of `$05` to `VSCROL` on cycle 0 of the first line of a `$22`
/// line gives rows 5-7; the same write on cycle 1 gives rows 0-7.
#[test]
#[ignore = "planned: SLICE-02"]
fn vscrol_start_deadline_cycle_0() {
    planned();
}

/// Verifies REQ-329: a write of `$03` to `VSCROL` on cycle 108 of line N + 3 of a `$02` line
/// (`VSCROL` = 7) makes it the last scan line; the same write on cycle 109 does not.
#[test]
#[ignore = "planned: SLICE-02"]
fn vscrol_end_deadline_cycle_108() {
    planned();
}

/// Verifies REQ-329: in a `$82` line with `VSCROL` = 7, a write of `$03` on cycle 5 of line
/// N + 3 gives a DLI there and a write on cycle 6 does not; with `VSCROL` = 3, a write of
/// `$07` on cycle 5 prevents the DLI.
#[test]
#[ignore = "planned: SLICE-02"]
fn vscrol_dli_deadline_cycle_5() {
    planned();
}

/// Verifies REQ-330: with `VSCROL` = 2, a jump after a `$22` line has 3 scan lines and
/// fetches its address again on each, reading D + 1/D + 2, `$3000/$3001` and `$3100/$3101`,
/// and the next fetch reads `$3200`.
#[test]
#[ignore = "planned: SLICE-02"]
fn extended_jump_refetches_each_line() {
    planned();
}

/// Verifies REQ-330: with `VSCROL` = 15, a `$04` line after a `$24` line (and a `$06` line
/// after a `$26` line) has rows 0-15, and rows 8-15 read font rows 0-7.
#[test]
#[ignore = "planned: SLICE-02"]
fn extended_mode_4_repeats_rows() {
    planned();
}

/// Verifies REQ-330: with `VSCROL` = 15, a `$02` line after a `$22` line has rows 0-15,
/// rows 8 and 9 are `%100` for name `$41`, rows 10-15 read font rows 2-7; the same for a
/// `$03` line after a `$23` line.
#[test]
#[ignore = "planned: SLICE-02"]
fn extended_mode_2_blank_rows_8_9() {
    planned();
}

/// Verifies REQ-330: with `VSCROL` = 3, a `$0F` line after a `$2F` line has 4 scan lines,
/// `Bitmap` DMA only on the first, and scan lines 2-4 have the codes of scan line 1.
#[test]
#[ignore = "planned: SLICE-02"]
fn extended_mode_f_repeats_data() {
    planned();
}

/// Verifies REQ-330: with `VSCROL` = 15, a `$00` line after a `$22` line has 16 scan lines
/// with code `%000` at positions `$22-$DD`.
#[test]
#[ignore = "planned: SLICE-02"]
fn extended_blank_line_16() {
    planned();
}

/// Verifies REQ-330: a write of `$01` to `VSCROL` on cycle 50 of row 2 of a `$02` line
/// after a `$22` line (`VSCROL` = 3) gives rows 0, 1, 2, ..., 15, 0, 1 (18 scan lines).
#[test]
#[ignore = "planned: SLICE-02"]
fn vscrol_change_mid_line_repeats_rows() {
    planned();
}
