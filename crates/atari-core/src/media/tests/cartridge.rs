//! Tests of the raw cartridge images (`docs/design/media.md`, "Tests").

use crate::media::{CartKind, Cartridge, MediaError};

/// Verifies REQ-604: raw images of 8192 and 16384 bytes load with no error, with the kinds
/// `CartKind::Standard8k` and `CartKind::Standard16k` (criteria 1, 2).
#[test]
fn req604_raw_sizes() {
    let small = Cartridge::from_raw(vec![0x11; 0x2000]);
    assert_eq!(
        small.map(|cart| cart.kind()),
        Ok(CartKind::Standard8k),
        "1: 8192 bytes"
    );
    let big = Cartridge::from_raw(vec![0x22; 0x4000]);
    assert_eq!(
        big.map(|cart| cart.kind()),
        Ok(CartKind::Standard16k),
        "2: 16384 bytes"
    );
}

/// Verifies REQ-604: raw images of 0, 4096, 8193 and 32768 bytes give `BadSize` with the
/// size, and no load panics (criterion 3).
#[test]
fn req604_raw_bad_sizes() {
    for size in [0, 0x1000, 0x2001, 0x8000] {
        assert_eq!(
            Cartridge::from_raw(vec![0; size]),
            Err(MediaError::BadSize(size)),
            "3: {size} bytes"
        );
    }
}
