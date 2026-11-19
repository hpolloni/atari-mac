//! Planned tests of the raw cartridge images (`docs/design/media.md`, "Tests").

use crate::planned::planned;

/// Verifies REQ-604: raw images of 8192 and 16384 bytes load with no error, with the kinds
/// `CartKind::Standard8k` and `CartKind::Standard16k` (criteria 1, 2).
#[test]
#[ignore = "planned: SLICE-04"]
fn req604_raw_sizes() {
    planned();
}

/// Verifies REQ-604: raw images of 0, 4096, 8193 and 32768 bytes give `BadSize` with the
/// size, and no load panics (criterion 3).
#[test]
#[ignore = "planned: SLICE-04"]
fn req604_raw_bad_sizes() {
    planned();
}
