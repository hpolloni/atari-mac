//! Priority, fifth player and multicolor player tests (REQ-424 to REQ-427).

use crate::planned::planned;

/// Verifies REQ-424: compares the priority table with an evaluation of the HRM 6.7 equations
/// that the test code does itself, for all `PRIOR` values with bit 4 = 0 and all inputs
/// (criterion 1).
#[test]
#[ignore = "planned: SLICE-02"]
fn priority_equations_all_inputs() {
    planned();
}

/// Verifies REQ-424: `PRIOR` = `$01`, `$02`, `$04`, `$08` give the colors of criteria 2-7,
/// and a color clock with no object and no playfield shows `COLBK` (criterion 8).
#[test]
#[ignore = "planned: SLICE-02"]
fn standard_priority_modes() {
    planned();
}

/// Verifies REQ-425: `PRIOR` = `$00` ORs the player and playfield colors (`$4A`, `$76`),
/// `PRIOR` = `$05` and `$03` give `$00`, and `PRIOR` = `$0C` gives `$40` (criteria 1-5).
#[test]
#[ignore = "planned: SLICE-02"]
fn priority_mode_0_mixes() {
    planned();
}

/// Verifies REQ-425: each cell of the rows of HRM Table 16 without `P5` for all 11
/// `PRIOR[3:0]` columns (criterion 6).
#[test]
#[ignore = "planned: SLICE-02"]
fn priority_table_16_without_p5() {
    planned();
}

/// Verifies REQ-426: with `PRIOR` = `$11` and `COLPF3` = `$C8`, missile 0 alone and on
/// `PF0`, `PF1` and `PF2` shows `$C8` (criteria 1 and 2).
#[test]
#[ignore = "planned: SLICE-02"]
fn fifth_player_uses_colpf3() {
    planned();
}

/// Verifies REQ-426: with `PRIOR` = `$11`, missile 0 on `PF1` sets `M0PF` = `$02`, and
/// player 0 on missile 0 sets `M0PL` = `$01` and `P0PF` = `$00` (criterion 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn fifth_player_keeps_collisions() {
    planned();
}

/// Verifies REQ-426: missiles 0 and 3 at different positions and sizes with `PRIOR` = `$10`
/// show `COLPF3` at HPOS `$40-$41` and `$90-$97` and `COLBK` at `$42` and `$98`
/// (criterion 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn fifth_player_keeps_position_and_size() {
    planned();
}

/// Verifies REQ-426: each cell of the `P5` rows of HRM Table 16 for all 11 `PRIOR[3:0]`
/// columns, except the three cells in the Notes (criterion 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn priority_table_16_p5_rows() {
    planned();
}

/// Verifies REQ-427: with `PRIOR` = `$21`, players 0 and 1 show `$D6`, players 2 and 3 show
/// `$C6` and players 0 and 2 show `$42`; with `PRIOR` = `$01` players 0 and 1 show `$42`
/// (criteria 1, 2, 3 and 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn multicolor_players_or_colors() {
    planned();
}

/// Verifies REQ-427: with `PRIOR` = `$21`, missiles 0 and 1 show `$D6` and missiles 2 and 3
/// show `$C6` (criterion 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn multicolor_missiles() {
    planned();
}

/// Verifies REQ-427: with `PRIOR` = `$21`, players 0 and 1 at the same color clock set
/// `P0PL` = `$02` and `P1PL` = `$01` (criterion 6).
#[test]
#[ignore = "planned: SLICE-02"]
fn multicolor_keeps_collisions() {
    planned();
}
