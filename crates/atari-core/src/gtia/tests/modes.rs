//! GTIA mode tests (REQ-428 to REQ-433).

use crate::planned::planned;

/// Verifies REQ-428: AN1-AN0 values `%00`, `%00`, `%01`, `%11`, `%11`, `%00` at HPOS
/// `$30-$35` give `$00` at HPOS `$30-$31`, `$07` at `$32-$33` and `$0C` at `$34-$35`
/// (criterion 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn gtia_pixels_pair_even_hpos() {
    planned();
}

/// Verifies REQ-428: code pairs (`%000`, `%101`) and (`%100`, `%101`) give the same pixel
/// `$01` (criterion 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn gtia_mode_ignores_an2() {
    planned();
}

/// Verifies REQ-428: after the horizontal blank code `%011`, with `COLBK` = `$90` and
/// `COLPF1` = `$0E`, the pair (`%110`, `%110`) shows `$9A` in all four columns, and player 0
/// on it sets no bit in `P0PF` (criterion 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn gtia_mode_ignores_hires() {
    planned();
}

/// Verifies REQ-429: with `PRIOR` = `$44` player 0 and with `PRIOR` = `$40` missile 0 on
/// pixels 1-15 show `COLPM0` (criteria 3 and 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_9_players_on_top() {
    planned();
}

/// Verifies REQ-429: with `PRIOR` = `$50` and `COLPF3` = `$40`, the fifth player on pixel 5
/// shows `$45` (criterion 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_9_fifth_player() {
    planned();
}

/// Verifies REQ-429: with `PRIOR` = `$40`, player 0 and missile 0 on pixels 1-15 set no bit
/// in `P0PF` and `M0PF` (criterion 6).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_9_no_playfield_collisions() {
    planned();
}

/// Verifies REQ-430: a normal width mode F line of `$44` bytes shows each pixel one color
/// clock later (HPOS `$30` and `$D1` show `COLPM0`, `$31-$D0` show `COLPF0`), the border
/// shows `COLPM0`, and with `PRIOR` = `$81` and `$A1` player 1 in the border shows `$10` and
/// `$30` (criteria 2-4).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_10_delay_and_border() {
    planned();
}

/// Verifies REQ-430: a wide mode F line with `PRIOR` = `$80`, checks HPOS `$2C-$DD`
/// (criterion 6).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_10_wide_right_edge() {
    planned();
}

/// Verifies REQ-430: with `PRIOR` = `$80`, player 0 over a pixel with value 5 sets `P0PF` =
/// `$02`, and over a pixel with value 1 sets `P0PL` = `$00` (criterion 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_10_collisions() {
    planned();
}

/// Verifies REQ-431: with `PRIOR` = `$C0` and `COLPM0` = `$46`, player 0 on pixels 0-15 shows
/// `$46` (criterion 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_11_players_on_top() {
    planned();
}

/// Verifies REQ-431: with `PRIOR` = `$D0`, `COLBK` = `$06` and `COLPF3` = `$08`, the fifth
/// player on pixel 2 shows `$28` and on pixel 0 shows `$00` (criterion 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_11_fifth_player() {
    planned();
}

/// Verifies REQ-431: with `PRIOR` = `$C0`, player 0 and missile 0 on pixels 0-15 set no bit
/// in `P0PF` and `M0PF` (criterion 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_11_no_playfield_collisions() {
    planned();
}

/// Verifies REQ-432: after a `PRIOR` write of `$40` at HPOS `$24` and `$00` at HPOS `$28` on
/// a hires line, codes `%100-%111` at HPOS `$30-$33` give `COLPF0-3` in both columns; on the
/// next scanline with `PRIOR` = `$00`, code `%101` gives `$38` and `$36` (criteria 1 and 2).
#[test]
#[ignore = "planned: SLICE-02"]
fn pseudo_mode_e_after_mid_line_prior() {
    planned();
}

/// Verifies REQ-433: with `PRIOR` = `$80`, the pair (`%101`, `%000`) shows `COLBK` `$90`, the
/// pair (`%101`, `%100`) shows `COLPF0` `$50` and the pair (`%000`, `%100`) shows `COLPM0`
/// `$10` (criteria 1-3).
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_10_bak_mutes_pixel() {
    planned();
}
