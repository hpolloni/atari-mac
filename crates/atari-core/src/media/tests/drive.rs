//! Planned tests of the disk drive commands and timing (`docs/design/media.md`, "Unit test
//! method for the SIO bus").

use crate::planned::planned;

/// Verifies REQ-627: the Status frames of the SD, ED, DD, 16384-sector, write-protected and
/// empty images, and the same frame for AUX1 = AUX2 = `$FF` (criteria 1-6, 9).
#[test]
#[ignore = "planned: SLICE-05"]
fn req627_status_frames() {
    planned();
}

/// Verifies REQ-627: after a NAK, the next Status has byte 0 bit 0 set and the one after it
/// does not; a completed command clears the bit (criteria 7, 8).
#[test]
#[ignore = "planned: SLICE-05"]
fn req627_status_error_bits() {
    planned();
}

/// Verifies REQ-628: with an SD image, each command code except `$21`, `$22`, `$4E`, `$4F`,
/// `$50`, `$52`, `$53` and `$57` gets NAK (criterion 1).
#[test]
#[ignore = "planned: SLICE-05"]
fn req628_nak_commands() {
    planned();
}

/// Verifies REQ-628: Read, Put and Write of sector 0 or of a sector after the last one get
/// NAK for the SD, ED and DD images, and the last sector gets ACK (criteria 2-6).
#[test]
#[ignore = "planned: SLICE-05"]
fn req628_nak_sector_range() {
    planned();
}

/// Verifies REQ-629: Read of SD sectors 1 and 720 and ED sector 1040 gives the file bytes of
/// the sector, with the checksum of the data frame (criteria 1-3, 6).
#[test]
#[ignore = "planned: SLICE-05"]
fn req629_read_sd_ed() {
    planned();
}

/// Verifies REQ-629: Read of DD sector 3 gives 128 bytes and of DD sector 4 gives 256 bytes
/// of the file, with the checksum of the data frame (criteria 4-6).
#[test]
#[ignore = "planned: SLICE-05"]
fn req629_read_dd() {
    planned();
}

/// Verifies REQ-630: Put and Write of SD sector 400, then Read gives the new bytes; the
/// image reports a change, and the export differs only in sector 400 (criteria 1, 2, 4, 5).
#[test]
#[ignore = "planned: SLICE-05"]
fn req630_put_write_roundtrip() {
    planned();
}

/// Verifies REQ-630: DD image: Put of sector 2 takes 128 bytes and Put of sector 10 takes
/// 256 bytes, each gets `41`, `41`, `43` and reads back (criterion 3).
#[test]
#[ignore = "planned: SLICE-05"]
fn req630_put_dd_frame_sizes() {
    planned();
}

/// Verifies REQ-631: on a write-protected image, Put, Write and both Format commands give
/// the Error byte `45` and change no data; Status then gives `08 BF E0 00`, and the image
/// reports no change (criteria 1-6).
#[test]
#[ignore = "planned: SLICE-05"]
fn req631_write_protect() {
    planned();
}

/// Verifies REQ-632: Format `$21` of a DD image with no Write Percom block gives 720 sectors
/// of 128 bytes (criterion 3).
#[test]
#[ignore = "planned: SLICE-05"]
fn req632_format_sd() {
    planned();
}

/// Verifies REQ-632: Format `$21` of the ED image gives 720 zero sectors, Format `$22` of the
/// SD image gives 1040 zero sectors and Status byte 0 = `$80`, and the image reports a
/// change (criteria 1, 2, 5).
#[test]
#[ignore = "planned: SLICE-05"]
fn req632_format_ed() {
    planned();
}

/// Verifies REQ-632: Write Percom block with the DD block, then Format `$21`: a 256-byte
/// reply frame, sectors 1-3 of 128 bytes and sectors 4-720 of 256 bytes (criterion 4).
#[test]
#[ignore = "planned: SLICE-05"]
fn req632_format_dd() {
    planned();
}

/// Verifies REQ-633: with no image, Status, Read, Put, Write and Format give the replies of
/// the criteria, and Read of sector 0 gets NAK (criteria 1-7).
#[test]
#[ignore = "planned: SLICE-05"]
fn req633_empty_drive() {
    planned();
}

/// Verifies REQ-634: the Percom blocks of the SD, ED, DD, 16384-sector and 1440-sector
/// images, and the same block for AUX1 = AUX2 = `$FF` (criteria 1-4).
#[test]
#[ignore = "planned: SLICE-05"]
fn req634_percom_blocks() {
    planned();
}

/// Verifies REQ-635: Write Percom block sets the geometry of the next Format `$21` only, a
/// new insertion or the SD block resets it, an 80-track block gives `45`, and the step
/// rate and drive-present bytes are ignored (criteria 1-6).
#[test]
#[ignore = "planned: SLICE-05"]
fn req635_write_percom() {
    planned();
}

/// Verifies REQ-638: a Read on another track takes 40 ms more for each track step and 60 ms
/// more for two steps, for the SD and ED images, and a Put on track 1 takes 40 ms more
/// (criteria 1-4, 6).
#[test]
#[ignore = "planned: SLICE-05"]
fn req638_seek_time() {
    planned();
}

/// Verifies REQ-638: Write sector 2 takes 208 ms more than Put sector 2 (criterion 5).
#[test]
#[ignore = "planned: SLICE-05"]
fn req638_write_verify_time() {
    planned();
}
