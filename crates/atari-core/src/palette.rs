//! GTIA color byte to RGB.
//!
//! Design: `docs/design/gtia.md`, "Palette". Requirements: REQ-409, REQ-410. The formula
//! is the YUV form of HRM D.2. The parameters are the values of REQ-409 (OI-023).

#[cfg(test)]
mod tests;

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
        let [r, g, b] = unclamped(standard, params, color);
        *slot = (channel(r) << 16) | (channel(g) << 8) | channel(b);
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

/// The R, G and B values of a color byte before the clamp, in the range 0-1 (HRM D.2).
fn unclamped(standard: VideoStandard, params: &PaletteParams, color: u8) -> [f64; 3] {
    let hue = color >> 4;
    let y = f64::from(color & 0x0F) / 15.0;
    let (u, v) = if hue == 0 {
        (0.0, 0.0)
    } else {
        let radians = angle(standard, params, hue).to_radians();
        (
            params.saturation * radians.cos(),
            params.saturation * radians.sin(),
        )
    };
    [
        1.140_f64.mul_add(v, y),
        (-0.581_f64).mul_add(v, (-0.395_f64).mul_add(u, y)),
        2.032_f64.mul_add(u, y),
    ]
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
