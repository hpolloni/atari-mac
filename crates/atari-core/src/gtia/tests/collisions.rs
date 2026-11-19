//! Collision tests (REQ-420 to REQ-423).

use crate::planned::planned;

/// Verifies REQ-420: on a hires line, player 0 on code `%101`, `%110` or `%111` sets `P0PF`
/// to `$04`, and on code `%100` sets no bit (criterion 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn hires_collisions_use_pf2() {
    planned();
}

/// Verifies REQ-420: with `PRIOR` = `$40` or `$C0` players and missiles set no playfield
/// bit; with `PRIOR` = `$80` player 0 over pixel values 4-7 and 12-15 sets
/// `1 << (value AND 3)` and over values 0-3 and 8-11 sets no bit (criteria 4 and 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn gtia_mode_playfield_collisions() {
    planned();
}

/// Verifies REQ-420: all color registers `$00`, `PRIOR` = `$04`, player 0 on `PF0`, checks
/// `P0PF` = `$01` (criterion 6).
#[test]
#[ignore = "planned: SLICE-02"]
fn collisions_ignore_colors_and_priority() {
    planned();
}

/// Verifies REQ-421: missiles 0 and 1 at the same pixels, with no player, set no collision
/// bit (criterion 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn no_missile_missile_collisions() {
    planned();
}

/// Verifies REQ-421: player 0 alone sets no collision bit, `P0PL` = `$00` (criterion 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn no_self_collision() {
    planned();
}

/// Verifies REQ-421: with `COLPM0` = `COLPM1` = `$46`, players 0 and 1 at the same pixels set
/// `P0PL` = `$02` and `P1PL` = `$01` (criterion 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn same_color_players_collide() {
    planned();
}

/// Verifies REQ-422: players that overlap only outside HPOS `$22-$DD`, or only on scanlines
/// with blank codes, set no collision bit; an overlap inside the visible area sets `P0PL` =
/// `$02` and `P1PL` = `$01` (criteria 1-4).
#[test]
#[ignore = "planned: SLICE-02"]
fn collisions_only_in_visible_area() {
    planned();
}

/// Verifies REQ-423: after a collision, a write of `$00` or `$FF` to `HITCLR` makes
/// `$D000-$D00F` read `$00`, and a later collision sets the bit again (criteria 1 and 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn hitclr_clears_all() {
    planned();
}

/// Verifies REQ-423: with no `HITCLR` write, a collision bit set on scanline 50 still reads
/// 1 on scanline 200 and in the next two frames, with no further collision (criterion 2).
#[test]
#[ignore = "planned: SLICE-02"]
fn collisions_latch_until_hitclr() {
    planned();
}
