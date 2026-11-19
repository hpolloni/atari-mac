//! Palette unit tests (`docs/design/gtia.md`, "Test plan").

use super::{PAL_ANGLES, PaletteParams, build, rgb, table, unclamped};
use crate::planned::planned;
use crate::timing::VideoStandard;

/// The R, G and B bytes of a color.
fn bytes(standard: VideoStandard, color: u8) -> [u32; 3] {
    let value = rgb(standard, color);
    [(value >> 16) & 0xFF, (value >> 8) & 0xFF, value & 0xFF]
}

/// Checks a color with a tolerance of 1 for each channel.
fn check_near(standard: VideoStandard, color: u8, expected: [u32; 3]) {
    let actual = bytes(standard, color);
    for (a, e) in actual.iter().zip(expected) {
        assert!(a.abs_diff(e) <= 1, "{standard:?} ${color:02X}: {actual:?}");
    }
}

/// Checks the round trip of all colors without clamping: Y, the angle and the magnitude.
fn check_round_trip(standard: VideoStandard, expected_angle: impl Fn(u8) -> f64) {
    let params = PaletteParams::default();
    for color in 0..=255_u8 {
        if unclamped(standard, &params, color)
            .iter()
            .any(|c| !(0.0..=1.0).contains(c))
        {
            continue;
        }
        let [red, green, blue] = bytes(standard, color).map(|c| f64::from(c) / 255.0);
        let luma = 0.114_f64.mul_add(blue, 0.299_f64.mul_add(red, 0.587 * green));
        let u = 0.436_f64.mul_add(blue, (-0.147_f64).mul_add(red, -0.289 * green));
        let v = (-0.100_f64).mul_add(blue, 0.615_f64.mul_add(red, -0.515 * green));
        let lum = f64::from(color & 0x0F) / 15.0;
        assert!(
            (luma - lum).abs() < 0.01,
            "{standard:?} ${color:02X} Y = {luma}"
        );
        let hue = color >> 4;
        if hue == 0 {
            continue;
        }
        let angle = v.atan2(u).to_degrees().rem_euclid(360.0);
        let diff =
            (angle - expected_angle(hue).rem_euclid(360.0) + 540.0).rem_euclid(360.0) - 180.0;
        assert!(diff.abs() < 2.0, "{standard:?} ${color:02X} angle {angle}");
        let magnitude = u.hypot(v);
        assert!(
            (magnitude - 0.233).abs() < 0.01,
            "{standard:?} ${color:02X} S = {magnitude}"
        );
    }
}

/// Verifies REQ-409: the NTSC grays.
#[test]
fn ntsc_grays() {
    for (color, value) in [(0x00, 0), (0x08, 136), (0x0E, 238), (0x0F, 255)] {
        assert_eq!(
            bytes(VideoStandard::Ntsc, color),
            [value; 3],
            "${color:02X}"
        );
    }
    for color in 0..16 {
        let [r, g, b] = bytes(VideoStandard::Ntsc, color);
        assert!(r == g && g == b, "${color:02X} is gray");
    }
}

/// Verifies REQ-409: three NTSC reference colors.
#[test]
fn ntsc_reference_colors() {
    check_near(VideoStandard::Ntsc, 0x94, [54, 52, 186]);
    check_near(VideoStandard::Ntsc, 0x1A, [170, 193, 49]);
    check_near(VideoStandard::Ntsc, 0x48, [200, 110, 99]);
}

/// Verifies REQ-409: the inverse matrix gives the luminance, angle and saturation back.
#[test]
fn ntsc_round_trip_yuv() {
    check_round_trip(VideoStandard::Ntsc, |hue| {
        24.0_f64.mul_add(-f64::from(hue - 1), 180.0)
    });
}

/// Verifies REQ-409: `build` with the default parameters gives `table`.
#[test]
fn ntsc_build_matches_table() {
    let built = build(VideoStandard::Ntsc, &PaletteParams::default());
    assert_eq!(&built, table(VideoStandard::Ntsc), "NTSC table");
    let other = build(
        VideoStandard::Ntsc,
        &PaletteParams {
            hue_step_deg: 25.0,
            saturation: 0.3,
        },
    );
    assert_ne!(&other, table(VideoStandard::Ntsc), "other parameters");
}

/// Verifies REQ-410: the PAL grays are the NTSC grays.
#[test]
fn pal_grays() {
    for (color, value) in [(0x00, 0), (0x08, 136), (0x0F, 255)] {
        assert_eq!(bytes(VideoStandard::Pal, color), [value; 3], "${color:02X}");
    }
    for color in 0..16 {
        assert_eq!(
            rgb(VideoStandard::Pal, color),
            rgb(VideoStandard::Ntsc, color),
            "${color:02X}"
        );
    }
}

/// Verifies REQ-410: three PAL reference colors.
#[test]
fn pal_reference_colors() {
    check_near(VideoStandard::Pal, 0x94, [5, 91, 114]);
    check_near(VideoStandard::Pal, 0x1A, [218, 162, 85]);
    check_near(VideoStandard::Pal, 0x48, [199, 95, 182]);
}

/// Verifies REQ-410: the PAL angles are the ideal angles of HRM Table 14.
#[test]
fn pal_round_trip_angles() {
    check_round_trip(VideoStandard::Pal, |hue| PAL_ANGLES[usize::from(hue - 1)]);
}

/// Verifies REQ-410: PAL hues 1 and 15 are the same.
#[test]
fn pal_hue_1_equals_hue_15() {
    for lum in 0..16 {
        assert_eq!(
            rgb(VideoStandard::Pal, 0x10 | lum),
            rgb(VideoStandard::Pal, 0xF0 | lum),
            "luminance {lum}"
        );
    }
}

/// Verifies REQ-411: `artifact_ntsc_row` on row A (`$0E`, `$00`, ...) and on row B (`$00`,
/// `$0E`, ...) gives mean colors (columns 16-367) with `max(R, G, B) - min(R, G, B)` of 32 or
/// more, and the two mean colors differ by 32 or more in at least one channel (criteria 2 and
/// 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn artifact_alternating_rows_are_colored() {
    planned();
}

/// Verifies REQ-411: the luma `0.299 R + 0.587 G + 0.114 B` of the mean color of row A and of
/// row B after `artifact_ntsc_row` is 119 ± 34 (criterion 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn artifact_luma_is_average() {
    planned();
}

/// Verifies REQ-411: `artifact_ntsc_row` on a row of 384 bytes `$94` gives
/// `rgb(NTSC, $94)` in columns 16-367, with a tolerance of 2 for each channel (criterion 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn artifact_solid_row_unchanged() {
    planned();
}

/// Verifies REQ-412: `pal_blend_row` gives `Y2 + ((RGB1 - Y1) + (RGB2 - Y2)) / 2` for each
/// channel of each pixel of rows 1-239, clamped to 0-1 and converted with `round(255 x
/// value)` (criterion 2).
#[test]
#[ignore = "planned: SLICE-02"]
fn pal_blend_formula() {
    planned();
}

/// Verifies REQ-412: `pal_blend_row` gives (167, 116, 159) for `$08` above `$48` and for `$48`
/// above `$08`, and (154, 133, 103) for `$48` above `$C8`, with a tolerance of 1 for each
/// channel (criteria 3-5).
#[test]
#[ignore = "planned: SLICE-02"]
fn pal_blend_reference_rows() {
    planned();
}
