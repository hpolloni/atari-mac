//! Planned tests of the line buffer, virtual fetches, mid-line `DMACTL` changes, abnormal
//! DMA and the hires bug (REQ-311, REQ-343 to REQ-347).

use crate::planned::planned;

/// Verifies REQ-311: with `DMACTL` = `$23`, on the first scan line of a mode E line, the
/// `Bitmap` fetch on cycle 104 sets `out.halt` and the fetch on cycle 106 does not.
#[test]
#[ignore = "planned: SLICE-02"]
fn fetch_on_cycle_106_is_virtual() {
    planned();
}

/// Verifies REQ-311: after a wide mode E line with LMS `$3000`, the first fetch of the next
/// mode line without LMS reads `$3030` (48 fetches).
#[test]
#[ignore = "planned: SLICE-02"]
fn wide_mode_e_advances_48() {
    planned();
}

/// Verifies REQ-311 and REQ-347: with `DMACTL` = `$23`, on the first scan line of a mode E
/// line, the data bus value `$A5` of cycle 106 (the virtual `Bitmap` fetch) goes into line
/// buffer byte 47.
#[test]
#[ignore = "planned: SLICE-02"]
fn virtual_fetch_loads_data_bus() {
    planned();
}

/// Verifies REQ-311: with `DMACTL` = `$23`, on the first scan line of a mode 6 line, the
/// `CharacterData` fetch on cycle 105 sets `out.halt`.
#[test]
#[ignore = "planned: SLICE-02"]
fn fetch_on_cycle_105_halts() {
    planned();
}

/// Verifies REQ-343: the 40 bytes of a normal mode E line stay in the line buffer after 8
/// blank lines, a jump and a vertical blank.
#[test]
#[ignore = "planned: SLICE-02"]
fn line_buffer_never_cleared() {
    planned();
}

/// Verifies REQ-343: after a wide mode E line (W0-W47), a narrow line keeps bytes 32-47 and
/// a normal line keeps bytes 40-47.
#[test]
#[ignore = "planned: SLICE-02"]
fn line_buffer_partial_load_keeps_tail() {
    planned();
}

/// Verifies REQ-343: a mode A line with playfield DMA off on its first scan line and on
/// from cycle 110 shows the old bytes B0-B19 on scan lines 2-4.
#[test]
#[ignore = "planned: SLICE-02"]
fn line_buffer_replay_without_dma() {
    planned();
}

/// Verifies REQ-343: a mode 8 line with no first-line DMA shows the bytes B0-B9 of an
/// earlier mode E line as mode 8 data on scan lines 2-8 (byte k at `$30 + 16k`).
#[test]
#[ignore = "planned: SLICE-02"]
fn line_buffer_reinterpreted_by_mode() {
    planned();
}

/// Verifies REQ-343: after a write of `$21` to `DMACTL` on cycle 110 of the first scan line
/// of a normal mode A line, scan line 2 starts B0 at position `$40` and B0-B15 fill
/// `$40-$BF`.
#[test]
#[ignore = "planned: SLICE-02"]
fn line_buffer_follows_left_border() {
    planned();
}

/// Verifies REQ-344: a write of `$22` on cycle 50 in a mode E line that starts with
/// `DMACTL` = `$21` gives code `%110` on `$40-$CF` (144 color clocks), `%000` on the rest,
/// and the memory scan counter increases by 36.
#[test]
#[ignore = "planned: SLICE-02"]
fn width_change_narrow_to_normal_gives_144() {
    planned();
}

/// Verifies REQ-344: a write of `$21` on cycle 17 in a normal mode E line does not move the
/// start (`$30`), but the playfield stops after `$BF`.
#[test]
#[ignore = "planned: SLICE-02"]
fn late_start_ignored() {
    planned();
}

/// Verifies REQ-344: a write of `$22` on cycle 20 in a narrow mode E line gives no
/// playfield, no `Bitmap` fetch and no memory scan counter change on that scan line.
#[test]
#[ignore = "planned: SLICE-02"]
fn early_start_after_deadline_no_playfield() {
    planned();
}

/// Verifies REQ-344: a write of `$21` on cycle 95 in a normal mode E line gives `Bitmap`
/// fetches on cycles 100, 102 and 104, a virtual fetch on cycle 106, and a memory scan
/// counter increase of 44.
#[test]
#[ignore = "planned: SLICE-02"]
fn early_stop_extends_to_wide() {
    planned();
}

/// Verifies REQ-344: a write of `$20` on cycle 50 in a normal mode E line stops the `Bitmap`
/// fetches after cycle 51, gives `%000` on `$70-$CF`, a counter increase of 40, and line
/// buffer bytes 16-39 from the data bus of cycles 52-98.
#[test]
#[ignore = "planned: SLICE-02"]
fn playfield_off_mid_line_counts_on() {
    planned();
}

/// Verifies REQ-344: a write of `$20` on cycle 10 of the first scan line of a normal mode E
/// line changes neither the memory scan counter nor the line buffer.
#[test]
#[ignore = "planned: SLICE-02"]
fn playfield_off_before_start_no_change() {
    planned();
}

/// Verifies REQ-344: a write of `$01` to `HSCROL` on cycle 60 of a `$1F` line gives the same
/// `out.dma`, and the code at x equals the code at x - 1 with no write, for x from `$80` to
/// `$CF`.
#[test]
#[ignore = "planned: SLICE-02"]
fn hscrol_bit0_mid_line() {
    planned();
}

/// Verifies REQ-344: in IR modes 2 and 3, while playfield DMA is off, ANTIC uses the invert
/// and blink state of the last character that it saw.
#[test]
#[ignore = "planned: SLICE-02"]
fn invert_state_kept_while_dma_off() {
    planned();
}

/// Verifies REQ-345: a write of `$04` to `HSCROL` on cycle 50 of a `$18` line gives the DMA
/// clock values of the criterion on that scan line and the next two.
#[test]
#[ignore = "planned: SLICE-02"]
fn abnormal_dma_hscrol_0_to_4_mode_8() {
    planned();
}

/// Verifies REQ-345: two DMA requests on one cycle give one read at the bitwise AND of the
/// addresses, both get that byte, and no refresh occurs on that cycle.
#[test]
#[ignore = "planned: SLICE-02"]
fn overlap_uses_address_and() {
    planned();
}

/// Verifies REQ-345: after the abnormal state, the DMA clock is 0 after cycle 1 of a `$00`
/// or `$01` instruction line and after cycle 0 of scan line 248.
#[test]
#[ignore = "planned: SLICE-02"]
fn blank_line_clears_dma_clock() {
    planned();
}

/// Verifies REQ-345: a `$08` line after a `$5E $00 $30` line with `HSCROL` = 10 leaves a
/// memory scan counter that is not `$303A` (with 8 it is), and after a `$5A` line with 14
/// one that is not `$3022` (with 12 it is).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_8_after_scrolled_mode_2_bug() {
    planned();
}

/// Verifies REQ-345: with `DMACTL` = `$21` and `HSCROL` = 10, a `$08` line after a
/// `$5E $00 $30` line leaves the memory scan counter `$3030`.
#[test]
#[ignore = "planned: SLICE-02"]
fn narrow_width_no_mode_8_bug() {
    planned();
}

/// Verifies REQ-345: with more than 48 line buffer accesses on one scan line, accesses
/// 49-63 do not change the buffer and read `$FF`, and access 64 uses byte 0 again.
#[test]
#[ignore = "planned: SLICE-02"]
fn line_buffer_counter_63_states() {
    planned();
}

/// Verifies REQ-346: with IR = `$0F` at scan line 248 (NTSC) and `DMACTL` = `$22`, position
/// `$80` has code `%111` instead of `%011` on the vertical blank lines, and positions
/// `$00-$21` keep `%011`.
#[test]
#[ignore = "planned: SLICE-02"]
fn hires_ir_at_248_opens_border() {
    planned();
}

/// Verifies REQ-346: in the same state, position `$80` has code `%101` instead of `%001` on
/// scan lines 251-253.
#[test]
#[ignore = "planned: SLICE-02"]
fn hires_bug_corrupts_vsync_code() {
    planned();
}

/// Verifies REQ-346: in the same state on PAL, position `$80` has code `%101` on scan lines
/// 275-277.
#[test]
#[ignore = "planned: SLICE-02"]
fn hires_bug_pal_vsync() {
    planned();
}

/// Verifies REQ-346: with IR = `$41` at scan line 248 and `DMACTL` = `$22`, position `$80`
/// has code `%010` on lines 248-250 and `%001` on lines 251-253.
#[test]
#[ignore = "planned: SLICE-02"]
fn jvb_at_248_no_bug() {
    planned();
}

/// Verifies REQ-346: a write of `$20` to `DMACTL` on scan line 250 gives position `$80` code
/// `%001` on lines 251-253 and `%011` on lines 254-261.
#[test]
#[ignore = "planned: SLICE-02"]
fn hires_bug_ends_when_playfield_off() {
    planned();
}

/// Verifies REQ-346: with IR = `$0F` at scan line 248 and `DMACTL` = `$20`, position `$80`
/// has code `%011` on lines 248-250 and `%001` on lines 251-253.
#[test]
#[ignore = "planned: SLICE-02"]
fn hires_ir_with_playfield_off_no_bug() {
    planned();
}

/// Verifies REQ-347: on the first scan line of a `$12` line with `DMACTL` = `$23` and
/// `HSCROL` = 2, with `$5A` on the data bus in cycle 106 (refresh and virtual fetch),
/// positions `$DA-$DD` have the codes `%101`, `%101`, `%110` and `%110`.
#[test]
#[ignore = "planned: SLICE-02"]
fn virtual_fetch_with_refresh_uses_data_bus() {
    planned();
}
