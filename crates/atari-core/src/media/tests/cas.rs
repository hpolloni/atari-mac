//! Planned tests of the CAS tape images: loading and errors
//! (`docs/design/media.md`, "CAS loading" and "Tests").

use crate::planned::planned;

/// Verifies REQ-639: the 201-byte file T loads with 3 blocks: a data block (gap 20000 ms,
/// 600 baud, the 132 bytes of record R), a data block (gap 1000 ms, 300 baud, `55 55 AA`) and
/// a signal block (gap 100 ms, lengths 256, 272, 128); the description is `Test`, and the
/// second `FUJI` chunk makes no block (criteria 1-5).
#[test]
#[ignore = "planned: SLICE-08"]
fn req639_cas_blocks() {
    planned();
}

/// Verifies REQ-639: the 23-byte file of the `FUJI` chunk and the `data` chunk of offset 168
/// (no `baud` chunk) gives one data block of 600 baud, gap 1000 ms, `55 55 AA` (criterion 6).
#[test]
#[ignore = "planned: SLICE-08"]
fn req639_cas_default_baud() {
    planned();
}

/// Verifies REQ-639: T with a 20-byte `data` chunk after it (length 12, gap 0) gives 4
/// blocks; block 4 has a gap of 0 ms, 300 baud and 12 bytes of `$00` (criterion 7).
#[test]
#[ignore = "planned: SLICE-08"]
fn req639_cas_trailing_chunk() {
    planned();
}

/// Verifies REQ-640: the changed files of T give `CasHeader`, `CasSignature` (two files),
/// `CasTruncated { offset: 20 }`, `CasTruncated { offset: 201 }`, `CasChunkType` at offset
/// 160 for `pwms`, `pwmc`, `pwmd`, `pwml` and `xyzw`, `CasBaud { offset: 160 }`,
/// `CasFskLength { offset: 179, length: 5 }` and `CasEmpty`; T without bytes 20-178 loads
/// (criteria 1-8).
#[test]
#[ignore = "planned: SLICE-08"]
fn req640_cas_errors() {
    planned();
}

/// Verifies REQ-640: the check order: a 7-byte file that starts with `FUJJ` gives
/// `CasHeader`; the first 165 bytes of T with `pwms` at 160 give `CasTruncated { offset: 160
/// }`; T with `pwms` at 160 and the `fsk ` length 5 gives `CasChunkType` at 160 (criterion 9).
#[test]
#[ignore = "planned: SLICE-08"]
fn req640_cas_error_order() {
    planned();
}
