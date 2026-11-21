//! Tests of the CAS tape images: loading and errors
//! (`docs/design/media.md`, "CAS loading" and "Tests").

use crate::media::{MediaError, Tape, TapeBlock};

/// One chunk: the 8-byte header (`kind`, the data length, `aux`) and `data`.
fn chunk(kind: [u8; 4], aux: u16, data: &[u8]) -> Vec<u8> {
    let mut bytes = kind.to_vec();
    let length = u16::try_from(data.len()).expect("chunk length");
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(&aux.to_le_bytes());
    bytes.extend_from_slice(data);
    bytes
}

/// The record R of REQ-639: `55 55 FC`, the bytes `$00` to `$7F`, and the checksum.
fn record_r() -> Vec<u8> {
    let mut record = vec![0x55, 0x55, 0xFC];
    record.extend(0..0x80_u8);
    let checksum = record.iter().fold(0_u8, |sum, &byte| {
        let (value, carry) = sum.overflowing_add(byte);
        value + u8::from(carry)
    });
    record.push(checksum);
    record
}

/// The 201-byte test file T of REQ-639.
fn file_t() -> Vec<u8> {
    let mut file = chunk(*b"FUJI", 0, b"Test");
    file.extend(chunk(*b"baud", 600, &[]));
    file.extend(chunk(*b"data", 20_000, &record_r()));
    file.extend(chunk(*b"baud", 300, &[]));
    file.extend(chunk(*b"data", 1_000, &[0x55, 0x55, 0xAA]));
    file.extend(chunk(*b"fsk ", 100, &[0x00, 0x01, 0x10, 0x01, 0x80, 0x00]));
    file.extend(chunk(*b"FUJI", 0, &[]));
    file
}

/// Loads `file` and panics with `name` on an error.
fn load(file: &[u8], name: &str) -> Tape {
    Tape::from_cas(file).unwrap_or_else(|error| panic!("{name}: {error:?}"))
}

/// A data block.
fn data(gap_ms: u16, baud: u16, bytes: &[u8]) -> TapeBlock {
    TapeBlock::Data {
        gap_ms,
        baud,
        bytes: bytes.to_vec(),
    }
}

/// T with `bytes` written at `offset`.
fn t_with(changes: &[(usize, &[u8])]) -> Vec<u8> {
    let mut file = file_t();
    for &(offset, bytes) in changes {
        file[offset..offset + bytes.len()].copy_from_slice(bytes);
    }
    file
}

/// Checks that `file` gives `error`.
fn check_error(file: &[u8], error: MediaError, name: &str) {
    assert_eq!(Tape::from_cas(file), Err(error), "{name}");
}

/// The file T is the file of the requirement table, byte by byte at the table offsets.
#[test]
fn file_t_layout() {
    let file = file_t();
    assert_eq!(file.len(), 201, "T has 201 bytes");
    let headers: [(usize, [u8; 8]); 7] = [
        (0, [0x46, 0x55, 0x4A, 0x49, 0x04, 0x00, 0x00, 0x00]),
        (12, [0x62, 0x61, 0x75, 0x64, 0x00, 0x00, 0x58, 0x02]),
        (20, [0x64, 0x61, 0x74, 0x61, 0x84, 0x00, 0x20, 0x4E]),
        (160, [0x62, 0x61, 0x75, 0x64, 0x00, 0x00, 0x2C, 0x01]),
        (168, [0x64, 0x61, 0x74, 0x61, 0x03, 0x00, 0xE8, 0x03]),
        (179, [0x66, 0x73, 0x6B, 0x20, 0x06, 0x00, 0x64, 0x00]),
        (193, [0x46, 0x55, 0x4A, 0x49, 0x00, 0x00, 0x00, 0x00]),
    ];
    for (offset, header) in headers {
        assert_eq!(file[offset..offset + 8], header, "header at {offset}");
    }
    assert_eq!(record_r()[131], 0x87, "the checksum of R");
}

/// Verifies REQ-639: the 201-byte file T loads with 3 blocks: a data block (gap 20000 ms,
/// 600 baud, the 132 bytes of record R), a data block (gap 1000 ms, 300 baud, `55 55 AA`) and
/// a signal block (gap 100 ms, lengths 256, 272, 128); the description is `Test`, and the
/// second `FUJI` chunk makes no block (criteria 1-5).
#[test]
fn req639_cas_blocks() {
    let tape = load(&file_t(), "T");
    let record = record_r();
    assert_eq!(record.len(), 132, "R has 132 bytes");
    assert_eq!(tape.blocks().len(), 3, "criterion 1: 3 blocks");
    assert_eq!(
        tape.blocks()[0],
        data(20_000, 600, &record),
        "criterion 2: block 1"
    );
    assert_eq!(
        tape.blocks()[1],
        data(1_000, 300, &[0x55, 0x55, 0xAA]),
        "criterion 3: block 2"
    );
    assert_eq!(
        tape.blocks()[2],
        TapeBlock::Signal {
            gap_ms: 100,
            lengths: vec![256, 272, 128],
        },
        "criterion 4: block 3"
    );
    assert_eq!(tape.description(), "Test", "criterion 5: description");
}

/// Verifies REQ-639: the 23-byte file of the `FUJI` chunk and the `data` chunk of offset 168
/// (no `baud` chunk) gives one data block of 600 baud, gap 1000 ms, `55 55 AA` (criterion 6).
#[test]
fn req639_cas_default_baud() {
    let t = file_t();
    let mut file = t[..12].to_vec();
    file.extend_from_slice(&t[168..179]);
    assert_eq!(file.len(), 23, "23 bytes");
    let tape = load(&file, "FUJI and data");
    assert_eq!(tape.blocks(), [data(1_000, 600, &[0x55, 0x55, 0xAA])]);
}

/// Verifies REQ-639: T with a 20-byte `data` chunk after it (length 12, gap 0) gives 4
/// blocks; block 4 has a gap of 0 ms, 300 baud and 12 bytes of `$00` (criterion 7).
#[test]
fn req639_cas_trailing_chunk() {
    let mut file = file_t();
    file.extend_from_slice(&[0x64, 0x61, 0x74, 0x61, 0x0C, 0x00, 0x00, 0x00]);
    file.extend_from_slice(&[0; 12]);
    assert_eq!(file.len(), 221, "T and 20 bytes");
    let tape = load(&file, "T with a data chunk after it");
    assert_eq!(tape.blocks().len(), 4, "4 blocks");
    assert_eq!(tape.blocks()[3], data(0, 300, &[0; 12]), "block 4");
}

/// Verifies REQ-640: the changed files of T give `CasHeader`, `CasSignature` (two files),
/// `CasTruncated { offset: 20 }`, `CasTruncated { offset: 201 }`, `CasChunkType` at offset
/// 160 for `pwms`, `pwmc`, `pwmd`, `pwml` and `xyzw`, `CasBaud { offset: 160 }`,
/// `CasFskLength { offset: 179, length: 5 }` and `CasEmpty`; T without bytes 20-178 loads
/// (criteria 1-8).
#[test]
fn req640_cas_errors() {
    let t = file_t();
    check_error(&t[..7], MediaError::CasHeader, "criterion 1");
    check_error(
        &t_with(&[(0, b"FUJJ")]),
        MediaError::CasSignature,
        "criterion 2: FUJJ",
    );
    check_error(
        &t[12..],
        MediaError::CasSignature,
        "criterion 2: starts with baud",
    );
    check_error(
        &t[..100],
        MediaError::CasTruncated { offset: 20 },
        "criterion 3",
    );
    let mut long = t.clone();
    long.extend_from_slice(&[0x64, 0x61, 0x74, 0x61, 0x00]);
    check_error(
        &long,
        MediaError::CasTruncated { offset: 201 },
        "criterion 4",
    );
    for kind in [b"pwms", b"pwmc", b"pwmd", b"pwml", b"xyzw"] {
        check_error(
            &t_with(&[(160, kind)]),
            MediaError::CasChunkType {
                offset: 160,
                chunk_type: *kind,
            },
            &format!("criterion 5: {}", kind.escape_ascii()),
        );
    }
    check_error(
        &t_with(&[(166, &[0x00, 0x00])]),
        MediaError::CasBaud { offset: 160 },
        "criterion 6",
    );
    check_error(
        &t_with(&[(183, &[0x05, 0x00])]),
        MediaError::CasFskLength {
            offset: 179,
            length: 5,
        },
        "criterion 7",
    );
    check_error(&t[..12], MediaError::CasEmpty, "criterion 8: FUJI only");
    let mut no_data = t[..20].to_vec();
    no_data.extend_from_slice(&t[179..]);
    let tape = load(&no_data, "criterion 8: T without bytes 20-178");
    assert_eq!(
        tape.blocks(),
        [TapeBlock::Signal {
            gap_ms: 100,
            lengths: vec![256, 272, 128],
        }],
        "criterion 8: the fsk block only"
    );
}

/// Verifies REQ-640: the check order: a 7-byte file that starts with `FUJJ` gives
/// `CasHeader`; the first 165 bytes of T with `pwms` at 160 give `CasTruncated { offset: 160
/// }`; T with `pwms` at 160 and the `fsk ` length 5 gives `CasChunkType` at 160 (criterion 9).
#[test]
fn req640_cas_error_order() {
    check_error(
        &[0x46, 0x55, 0x4A, 0x4A, 0, 0, 0],
        MediaError::CasHeader,
        "7 bytes with FUJJ",
    );
    let pwms = t_with(&[(160, b"pwms")]);
    check_error(
        &pwms[..165],
        MediaError::CasTruncated { offset: 160 },
        "165 bytes with pwms",
    );
    check_error(
        &t_with(&[(160, b"pwms"), (183, &[0x05, 0x00])]),
        MediaError::CasChunkType {
            offset: 160,
            chunk_type: *b"pwms",
        },
        "pwms and fsk length 5",
    );
}

/// The error messages name the offset and the chunk type.
#[test]
fn cas_error_messages() {
    let message = MediaError::CasChunkType {
        offset: 160,
        chunk_type: *b"pwms",
    }
    .to_string();
    assert!(
        message.contains("160") && message.contains("pwms"),
        "{message}"
    );
    let message = MediaError::CasFskLength {
        offset: 179,
        length: 5,
    }
    .to_string();
    assert!(
        message.contains("179") && message.contains('5'),
        "{message}"
    );
}
