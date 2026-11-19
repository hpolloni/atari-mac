//! Planned tests of the ATR disk images: loading, errors and export
//! (`docs/design/media.md`, "Tests").

use crate::planned::planned;

/// Verifies REQ-625: the SD (720 x 128), ED (1040 x 128) and DD (720 sectors, 1-3 of 128
/// bytes) headers load, and sectors 1, 3, 4 and 720 hold the file bytes of the criteria
/// (criteria 1-5).
#[test]
#[ignore = "planned: SLICE-05"]
fn req625_atr_sd_ed_dd() {
    planned();
}

/// Verifies REQ-625: the SD file with 100 more bytes after the data loads with 720 sectors,
/// and sector 720 holds file bytes 92,048 to 92,175 (criterion 6).
#[test]
#[ignore = "planned: SLICE-05"]
fn req625_atr_extra_bytes() {
    planned();
}

/// Verifies REQ-625: a header with the high size byte `$02` and 2,097,152 data bytes loads
/// with 16384 sectors of 128 bytes (criterion 7).
#[test]
#[ignore = "planned: SLICE-05"]
fn req625_atr_large() {
    planned();
}

/// Verifies REQ-625: the full boot sector rule of "ATR loading" in `docs/design/media.md`
/// for images with 256-byte sectors.
#[test]
#[ignore = "planned: SLICE-05"]
fn req625_atr_dd_boot_sectors() {
    planned();
}

/// Verifies REQ-626: the errors `AtrHeader`, `AtrSignature`, `AtrSectorSize`,
/// `AtrDataSize`, `AtrTruncated` and `AtrSectorCount` for the files of the criteria
/// (criteria 1-8).
#[test]
#[ignore = "planned: SLICE-05"]
fn req626_atr_errors() {
    planned();
}

/// Verifies REQ-626: a file with more than one defect gives the first error in the order of
/// the criterion (criterion 9).
#[test]
#[ignore = "planned: SLICE-05"]
fn req626_atr_error_order() {
    planned();
}

/// Verifies REQ-637: the export of an unchanged SD file (without the extra bytes) and DD
/// file is equal to the input, the image reports no change, and export bytes 7-15 are
/// `01` to `09` (criteria 1, 2, 5).
#[test]
#[ignore = "planned: SLICE-05"]
fn req637_export_unchanged() {
    planned();
}

/// Verifies REQ-637: after Put of SD sector 400, the export loads and holds the new sector,
/// and the image reports a change (criteria 3, 5).
#[test]
#[ignore = "planned: SLICE-05"]
fn req637_export_after_write() {
    planned();
}

/// Verifies REQ-637: after Format `$21` of the ED image, the export has 92,176 bytes, the SD
/// header and all data bytes `$00` (criteria 4, 5).
#[test]
#[ignore = "planned: SLICE-05"]
fn req637_export_after_format() {
    planned();
}
