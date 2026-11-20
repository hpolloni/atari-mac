//! GTIA color byte to RGB, and the optional display filters.
//!
//! Design: `docs/design/gtia.md`, "Palette". Requirements: REQ-409 to REQ-412. The formula
//! is the YUV form of HRM D.2. The parameters are the values of REQ-409 (OI-023).

#[cfg(test)]
mod tests;

use std::f64::consts::FRAC_PI_2;
use std::sync::OnceLock;

use crate::timing::VideoStandard;

/// The ideal PAL UV angles of hues 1-15 in degrees (HRM 6.3 Table 14).
const PAL_ANGLES: [f64; 15] = [
    135.0, 112.5, 90.0, 67.5, 45.0, 22.5, 337.5, 315.0, 292.5, 270.0, 225.0, 202.5, 180.0, 157.5,
    135.0,
];

/// Palette parameters. `Default` gives the values of REQ-409.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaletteParams {
    /// The NTSC hue step in degrees.
    pub hue_step_deg: f64,
    /// The chroma magnitude `S`.
    pub saturation: f64,
}

impl Default for PaletteParams {
    fn default() -> Self {
        Self {
            hue_step_deg: 24.0,
            saturation: 0.233,
        }
    }
}

/// The 256 RGB values (`0x00RRGGBB`) for a standard, computed once.
#[must_use]
pub fn table(standard: VideoStandard) -> &'static [u32; 256] {
    static NTSC: OnceLock<[u32; 256]> = OnceLock::new();
    static PAL: OnceLock<[u32; 256]> = OnceLock::new();
    let cell = match standard {
        VideoStandard::Ntsc => &NTSC,
        VideoStandard::Pal => &PAL,
    };
    cell.get_or_init(|| build(standard, &PaletteParams::default()))
}

/// Converts a GTIA color byte to `0x00RRGGBB` (a lookup in [`table`]).
#[must_use]
pub fn rgb(standard: VideoStandard, color: u8) -> u32 {
    table(standard)[usize::from(color)]
}

/// The palette for other parameters (for tests and a later settings screen).
#[must_use]
pub fn build(standard: VideoStandard, params: &PaletteParams) -> [u32; 256] {
    let mut out = [0; 256];
    for (color, slot) in (0..=255_u8).zip(out.iter_mut()) {
        *slot = pack(unclamped(standard, params, color));
    }
    out
}

/// The chroma angle of hue `hue` (1-15) in degrees.
fn angle(standard: VideoStandard, params: &PaletteParams, hue: u8) -> f64 {
    match standard {
        VideoStandard::Ntsc => params.hue_step_deg.mul_add(-f64::from(hue - 1), 180.0),
        VideoStandard::Pal => PAL_ANGLES[usize::from(hue - 1)],
    }
}

/// The Y, U and V values of a color byte (HRM D.2). Y is `L / 15`.
fn yuv(standard: VideoStandard, params: &PaletteParams, color: u8) -> [f64; 3] {
    let hue = color >> 4;
    let y = f64::from(color & 0x0F) / 15.0;
    if hue == 0 {
        return [y, 0.0, 0.0];
    }
    let radians = angle(standard, params, hue).to_radians();
    [
        y,
        params.saturation * radians.cos(),
        params.saturation * radians.sin(),
    ]
}

/// The R, G and B values for Y, U and V, before the clamp (HRM D.2).
const fn yuv_to_rgb([y, u, v]: [f64; 3]) -> [f64; 3] {
    [
        1.140_f64.mul_add(v, y),
        (-0.581_f64).mul_add(v, (-0.395_f64).mul_add(u, y)),
        2.032_f64.mul_add(u, y),
    ]
}

/// The R, G and B values of a color byte before the clamp, in the range 0-1 (HRM D.2).
fn unclamped(standard: VideoStandard, params: &PaletteParams, color: u8) -> [f64; 3] {
    yuv_to_rgb(yuv(standard, params, color))
}

/// Clamps and packs R, G and B values to `0x00RRGGBB`.
fn pack([r, g, b]: [f64; 3]) -> u32 {
    (channel(r) << 16) | (channel(g) << 8) | channel(b)
}

/// The phase of the first sample of a row in the artifacting filter, relative to the color
/// burst (project decision: it gives the red and blue artifact hues of the XE, HRM 6.4).
const ARTIFACT_PHASE_DEG: f64 = 60.0;
/// The gain from the luma signal to the decoded chroma. The 130XE color burst has a
/// peak-to-peak level of about 14 luminance steps (HRM 6.4), and the decoder scales the
/// chroma so that the burst gives the palette saturation: `S / (7 / 15)`.
const ARTIFACT_GAIN: f64 = 0.233 * 15.0 / 7.0;

/// The NTSC artifacting filter (REQ-411).
///
/// `row` is one frame buffer row (one byte per hires pixel) and `out` gets one `0x00RRGGBB`
/// value for each byte (the shorter length of the two is used).
///
/// The model samples the composite signal at 4 x fsc, 2 samples for each hires pixel (HRM
/// D.3). The luma of each pixel is `L / 15`, and the decoder demodulates the luma at the
/// color subcarrier over a window of 4 samples (one color clock) centered on the pixel.
/// The result adds to the chroma of the pixel itself. A solid row gives the palette
/// colors, and alternating hires pixels give colored artifacts with the mean luma.
pub fn artifact_ntsc_row(row: &[u8], out: &mut [u32]) {
    let params = PaletteParams::default();
    let len = row.len().min(out.len());
    if len == 0 {
        return;
    }
    let luma = |sample: usize| f64::from(row[(sample / 2).min(len - 1)] & 0x0F) / 15.0;
    let phase = ARTIFACT_PHASE_DEG.to_radians();
    for (i, slot) in out.iter_mut().enumerate().take(len) {
        let [_, u, v] = yuv(VideoStandard::Ntsc, &params, row[i]);
        let (mut luma_sum, mut u_sum, mut v_sum) = (0.0, 0.0, 0.0);
        for k in 0..4_usize {
            // Samples 2i - 1 to 2i + 2. The rows repeat their first and last samples.
            let sample = (2 * i + k).saturating_sub(1);
            let value = luma(sample);
            let (sin, cos) = FRAC_PI_2
                .mul_add(f64::from(u8::try_from(sample % 4).unwrap_or(0)), phase)
                .sin_cos();
            luma_sum += value;
            u_sum = value.mul_add(cos, u_sum);
            v_sum = value.mul_add(sin, v_sum);
        }
        let artifact = ARTIFACT_GAIN * 0.5;
        let decoded = [
            luma_sum / 4.0,
            artifact.mul_add(u_sum, u),
            artifact.mul_add(v_sum, v),
        ];
        *slot = pack(yuv_to_rgb(decoded));
    }
}

/// The PAL chroma blending filter for one row (REQ-412).
///
/// `prev` is the frame buffer row above, `row` the current row, and `out` gets one
/// `0x00RRGGBB` value for each byte: for each channel `Y2 + ((RGB1 - Y1) + (RGB2 - Y2)) / 2`,
/// with the PAL colors before the clamp.
pub fn pal_blend_row(prev: &[u8], row: &[u8], out: &mut [u32]) {
    let params = PaletteParams::default();
    for ((slot, &above), &color) in out.iter_mut().zip(prev).zip(row) {
        let first = unclamped(VideoStandard::Pal, &params, above);
        let second = unclamped(VideoStandard::Pal, &params, color);
        let y1 = f64::from(above & 0x0F) / 15.0;
        let y2 = f64::from(color & 0x0F) / 15.0;
        let blend = |n: usize| 0.5_f64.mul_add((first[n] - y1) + (second[n] - y2), y2);
        *slot = pack([blend(0), blend(1), blend(2)]);
    }
}

/// The PAL chroma blending filter for a whole frame (REQ-412).
///
/// Row 0 is not blended (OI-021), and each other row is blended with the row above. `width`
/// is the row length. `out` gets one `0x00RRGGBB` value for each frame buffer byte.
pub fn pal_blend_frame(frame: &[u8], width: usize, out: &mut [u32]) {
    if width == 0 {
        return;
    }
    let mut prev: Option<&[u8]> = None;
    for (row, out_row) in frame.chunks(width).zip(out.chunks_mut(width)) {
        match prev {
            None => {
                for (slot, &color) in out_row.iter_mut().zip(row) {
                    *slot = rgb(VideoStandard::Pal, color);
                }
            }
            Some(above) => pal_blend_row(above, row, out_row),
        }
        prev = Some(row);
    }
}

/// One channel: clamped to 0-1 and converted with `round(255 x value)`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to 0-255 before the cast"
)]
fn channel(value: f64) -> u32 {
    (255.0 * value.clamp(0.0, 1.0)).round() as u32
}
