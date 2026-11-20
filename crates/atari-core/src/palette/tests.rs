//! Palette unit tests (`docs/design/gtia.md`, "Test plan").

use super::{
    PAL_ANGLES, PaletteParams, artifact_ntsc_row, build, pal_blend_frame, pal_blend_row, rgb,
    table, unclamped,
};
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

/// The mean R, G and B of columns 16-367 of an output row.
fn mean_color(out: &[u32]) -> [f64; 3] {
    let columns = &out[16..368];
    let count = f64::from(u32::try_from(columns.len()).unwrap());
    [16, 8, 0].map(|shift| {
        columns
            .iter()
            .map(|&value| f64::from((value >> shift) & 0xFF))
            .sum::<f64>()
            / count
    })
}

/// The artifacting filter output for a row of 384 bytes `pattern[i % pattern.len()]`.
fn artifact(pattern: &[u8]) -> Vec<u32> {
    let row: Vec<u8> = pattern.iter().copied().cycle().take(384).collect();
    let mut out = vec![0; 384];
    artifact_ntsc_row(&row, &mut out);
    out
}

/// Row A (`$0E`, `$00`, ...) and row B (`$00`, `$0E`, ...) of REQ-411: their mean colors.
fn artifact_rows() -> [[f64; 3]; 2] {
    [
        mean_color(&artifact(&[0x0E, 0x00])),
        mean_color(&artifact(&[0x00, 0x0E])),
    ]
}

/// Verifies REQ-411: `artifact_ntsc_row` on row A (`$0E`, `$00`, ...) and on row B (`$00`,
/// `$0E`, ...) gives mean colors (columns 16-367) with `max(R, G, B) - min(R, G, B)` of 32 or
/// more, and the two mean colors differ by 32 or more in at least one channel (criteria 2 and
/// 3).
#[test]
fn artifact_alternating_rows_are_colored() {
    let [a, b] = artifact_rows();
    for (name, color) in [("A", a), ("B", b)] {
        let max = color.iter().copied().fold(f64::MIN, f64::max);
        let min = color.iter().copied().fold(f64::MAX, f64::min);
        assert!(max - min >= 32.0, "row {name}: {color:?}");
    }
    let differ = a.iter().zip(b).any(|(x, y)| (x - y).abs() >= 32.0);
    assert!(differ, "rows A and B: {a:?} {b:?}");
}

/// Verifies REQ-411: the luma `0.299 R + 0.587 G + 0.114 B` of the mean color of row A and of
/// row B after `artifact_ntsc_row` is 119 ± 34 (criterion 4).
#[test]
fn artifact_luma_is_average() {
    for (name, [r, g, b]) in ["A", "B"].into_iter().zip(artifact_rows()) {
        let luma = 0.114_f64.mul_add(b, 0.299_f64.mul_add(r, 0.587 * g));
        assert!((luma - 119.0).abs() <= 34.0, "row {name}: luma {luma}");
    }
}

/// Verifies REQ-411: `artifact_ntsc_row` on a row of 384 bytes `$94` gives
/// `rgb(NTSC, $94)` in columns 16-367, with a tolerance of 2 for each channel (criterion 5).
#[test]
fn artifact_solid_row_unchanged() {
    let expected = rgb(VideoStandard::Ntsc, 0x94);
    for (column, value) in artifact(&[0x94]).into_iter().enumerate().take(368).skip(16) {
        for shift in [16, 8, 0] {
            let (a, e) = ((value >> shift) & 0xFF, (expected >> shift) & 0xFF);
            assert!(
                a.abs_diff(e) <= 2,
                "column {column}: {value:06X} {expected:06X}"
            );
        }
    }
}

/// The blended color of `color` below `above`, by the formula of REQ-412 criterion 2.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the channel is clamped and rounded to 0-255 before the cast"
)]
fn blend_formula(above: u8, color: u8) -> u32 {
    let params = PaletteParams::default();
    let first = unclamped(VideoStandard::Pal, &params, above);
    let second = unclamped(VideoStandard::Pal, &params, color);
    let y1 = f64::from(above & 0x0F) / 15.0;
    let y2 = f64::from(color & 0x0F) / 15.0;
    let channel = |n: usize| {
        let value = y2 + f64::midpoint(first[n] - y1, second[n] - y2);
        (255.0 * value.clamp(0.0, 1.0)).round()
    };
    let [r, g, b] = [channel(0), channel(1), channel(2)].map(|c| c as u32);
    (r << 16) | (g << 8) | b
}

/// Verifies REQ-412: `pal_blend_row` gives `Y2 + ((RGB1 - Y1) + (RGB2 - Y2)) / 2` for each
/// channel of each pixel of rows 1-239, clamped to 0-1 and converted with `round(255 x
/// value)` (criterion 2).
#[test]
fn pal_blend_formula() {
    let colors: Vec<u8> = (0..=255).collect();
    for above in (0..=255_u8).step_by(3) {
        let prev = vec![above; 256];
        let mut out = vec![0; 256];
        pal_blend_row(&prev, &colors, &mut out);
        for (&color, &value) in colors.iter().zip(&out) {
            assert_eq!(
                value,
                blend_formula(above, color),
                "${above:02X} above ${color:02X}"
            );
        }
    }
    let frame: Vec<u8> = (0..240 * 384)
        .map(|i| u8::try_from(i * 7 % 256).unwrap())
        .collect();
    let mut out = vec![0; 240 * 384];
    pal_blend_frame(&frame, 384, &mut out);
    for row in 1..240 {
        for column in (0..384).step_by(5) {
            let i = row * 384 + column;
            assert_eq!(
                out[i],
                blend_formula(frame[i - 384], frame[i]),
                "row {row} column {column}"
            );
        }
    }
}

/// The blended color of the second row of a row of `above` above a row of `color`.
fn blended(above: u8, color: u8) -> [u32; 3] {
    let mut out = [0; 384];
    pal_blend_row(&[above; 384], &[color; 384], &mut out);
    assert!(out.iter().all(|&value| value == out[0]), "one color");
    [(out[0] >> 16) & 0xFF, (out[0] >> 8) & 0xFF, out[0] & 0xFF]
}

/// Verifies REQ-412: `pal_blend_row` gives (167, 116, 159) for `$08` above `$48` and for `$48`
/// above `$08`, and (154, 133, 103) for `$48` above `$C8`, with a tolerance of 1 for each
/// channel (criteria 3-5).
#[test]
fn pal_blend_reference_rows() {
    for (above, color, expected) in [
        (0x08, 0x48, [167, 116, 159]),
        (0x48, 0x08, [167, 116, 159]),
        (0x48, 0xC8, [154, 133, 103]),
    ] {
        let actual = blended(above, color);
        for (a, e) in actual.iter().zip(expected) {
            assert!(
                a.abs_diff(e) <= 1,
                "${above:02X} above ${color:02X}: {actual:?}"
            );
        }
    }
}

/// Verifies REQ-412: with the filter on, the output of each pixel of row 0 is
/// `rgb(PAL, b)` of its own byte b (criterion 6).
#[test]
fn pal_blend_row_0_unchanged() {
    let frame: Vec<u8> = (0..240 * 384)
        .map(|i| u8::try_from(i % 256).unwrap())
        .collect();
    let mut out = vec![0; 240 * 384];
    pal_blend_frame(&frame, 384, &mut out);
    for column in 0..384 {
        assert_eq!(
            out[column],
            rgb(VideoStandard::Pal, frame[column]),
            "column {column}"
        );
    }
}
