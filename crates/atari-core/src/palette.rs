//! GTIA color byte to RGB.
//!
//! Design: `docs/design/gtia.md`, "Palette". Requirements: REQ-409, REQ-410.

use crate::timing::VideoStandard;

/// Converts a GTIA color byte to `0x00RRGGBB`.
#[must_use]
pub fn rgb(standard: VideoStandard, color: u8) -> u32 {
    // WP-0 STUB: gray scale from the luminance bits. WP-2 implements the palettes.
    let _: VideoStandard = standard;
    let lum = u32::from(color & 0x0E) * 17;
    (lum << 16) | (lum << 8) | lum
}
