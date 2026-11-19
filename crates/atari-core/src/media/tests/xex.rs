//! Planned tests of the executable file parser and boot image (`docs/design/media.md`,
//! "Tests").

use crate::planned::planned;

/// Verifies REQ-610: the six example files of the acceptance criteria give the segments of
/// the criteria, in file order: optional `FF FF` headers between segments, a repeated
/// signature, a trailing `FF FF`, and a segment of 65536 bytes (criteria 1-6).
#[test]
#[ignore = "planned: SLICE-04"]
fn req610_parse_segments() {
    planned();
}

/// Verifies REQ-611: an empty file, the file `FF` and a file without the `FF FF` signature
/// give `XexSignature` (criteria 1-3).
#[test]
#[ignore = "planned: SLICE-04"]
fn req611_xex_signature_errors() {
    planned();
}

/// Verifies REQ-611: a file with no complete segment gives `XexNoSegment`, with no defect
/// (`FF FF`), or with `EndBeforeStart` or `ShortData` at offset 2 (criteria 4-6).
#[test]
#[ignore = "planned: SLICE-04"]
fn req611_xex_no_segment() {
    planned();
}

/// Verifies REQ-611: a defect after a complete segment (`ShortHeader`, `ShortData` or
/// `EndBeforeStart` at offset 7) keeps the segments before it and drops the segments after
/// it (criteria 7-9).
#[test]
#[ignore = "planned: SLICE-04"]
fn req611_xex_defects_keep_segments() {
    planned();
}

/// Verifies REQ-612: `Xex::boot_image` gives the loader records for the segments and the
/// disk geometry of "XEX boot image" in `docs/design/media.md`.
#[test]
#[ignore = "planned: SLICE-04"]
fn req612_boot_image_records() {
    planned();
}

/// Verifies REQ-613: with `INITAD` segments after segments C and E, the boot image has an
/// Init record after segments C and E only.
#[test]
#[ignore = "planned: SLICE-04"]
fn req613_init_records() {
    planned();
}

/// Verifies REQ-614: the boot image ends with a Run indirect record (`RUNAD` segment) or a
/// Run direct record to `$2000` (no `RUNAD` segment, first segment at `$2000`).
#[test]
#[ignore = "planned: SLICE-04"]
fn req614_run_records() {
    planned();
}
