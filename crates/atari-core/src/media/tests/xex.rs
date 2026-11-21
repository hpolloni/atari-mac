//! Tests of the executable file parser and boot image (`docs/design/media.md`, "Tests").

use crate::media::{MediaError, SegmentDefect, Xex, XexDefect, XexSegment};

/// A segment at `start` with `data`.
fn segment(start: u16, data: &[u8]) -> XexSegment {
    XexSegment {
        start,
        data: data.to_vec(),
    }
}

/// Parses `file` and checks that it gives `segments` and `defect`.
fn check(file: &[u8], segments: &[XexSegment], defect: Option<XexDefect>, name: &str) {
    let xex = Xex::parse(file).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    assert_eq!(xex.segments(), segments, "{name}: segments");
    assert_eq!(xex.defect(), defect, "{name}: defect");
}

/// A defect of `kind` at `offset`.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the parser reports the defect as an `Option`"
)]
const fn defect(kind: SegmentDefect, offset: usize) -> Option<XexDefect> {
    Some(XexDefect { offset, kind })
}

/// Verifies REQ-610: the six example files of the acceptance criteria give the segments of
/// the criteria, in file order: optional `FF FF` headers between segments, a repeated
/// signature, a trailing `FF FF`, and a segment of 65536 bytes (criteria 1-6).
#[test]
fn req610_parse_segments() {
    check(
        &[0xFF, 0xFF, 0x00, 0x20, 0x02, 0x20, 0xA9, 0x00, 0x60],
        &[segment(0x2000, &[0xA9, 0x00, 0x60])],
        None,
        "1",
    );
    let two = [segment(0x2000, &[0x11]), segment(0x3000, &[0x22])];
    check(
        &[
            0xFF, 0xFF, 0x00, 0x20, 0x00, 0x20, 0x11, 0xFF, 0xFF, 0x00, 0x30, 0x00, 0x30, 0x22,
        ],
        &two,
        None,
        "2",
    );
    check(
        &[
            0xFF, 0xFF, 0x00, 0x20, 0x00, 0x20, 0x11, 0x00, 0x30, 0x00, 0x30, 0x22,
        ],
        &two,
        None,
        "3",
    );
    check(
        &[0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x20, 0x00, 0x20, 0x11],
        &[segment(0x2000, &[0x11])],
        None,
        "4",
    );
    check(
        &[0xFF, 0xFF, 0x00, 0x20, 0x00, 0x20, 0x11, 0xFF, 0xFF],
        &[segment(0x2000, &[0x11])],
        None,
        "5",
    );
    let data: Vec<u8> = (0..=0xFFFF_u16).map(|i| i.to_le_bytes()[0]).collect();
    let mut file = vec![0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF];
    file.extend(&data);
    check(&file, &[segment(0x0000, &data)], None, "6: 65536 bytes");
}

/// Verifies REQ-611: an empty file, the file `FF` and a file without the `FF FF` signature
/// give `XexSignature` (criteria 1-3).
#[test]
fn req611_xex_signature_errors() {
    for (file, name) in [
        (&[][..], "1: empty"),
        (&[0xFF][..], "2: FF"),
        (&[0x00, 0x00, 0x00, 0x20, 0x00, 0x20, 0x11][..], "3: 00 00"),
    ] {
        assert_eq!(Xex::parse(file), Err(MediaError::XexSignature), "{name}");
    }
    assert_eq!(
        MediaError::XexSignature.to_string(),
        "XEX file does not start with $FF $FF"
    );
}

/// Verifies REQ-611: a file with no complete segment gives `XexNoSegment`, with no defect
/// (`FF FF`), or with `EndBeforeStart` or `ShortData` at offset 2 (criteria 4-6).
#[test]
fn req611_xex_no_segment() {
    assert_eq!(
        Xex::parse(&[0xFF, 0xFF]),
        Err(MediaError::XexNoSegment(None)),
        "4: FF FF"
    );
    let end_before_start = defect(SegmentDefect::EndBeforeStart, 2);
    assert_eq!(
        Xex::parse(&[0xFF, 0xFF, 0x00, 0x20, 0xFF, 0x1F]),
        Err(MediaError::XexNoSegment(end_before_start)),
        "5: end $1FFF before start $2000"
    );
    assert_eq!(
        Xex::parse(&[0xFF, 0xFF, 0x00, 0x20, 0x05, 0x20, 0x11]),
        Err(MediaError::XexNoSegment(defect(
            SegmentDefect::ShortData,
            2
        ))),
        "6: 1 of 6 data bytes"
    );
    assert_eq!(
        MediaError::XexNoSegment(end_before_start).to_string(),
        "XEX file has no complete segment: end address before start address at offset 2"
    );
    assert_eq!(
        MediaError::XexNoSegment(None).to_string(),
        "XEX file has no segment"
    );
}

/// Verifies REQ-611: a defect after a complete segment (`ShortHeader`, `ShortData` or
/// `EndBeforeStart` at offset 7) keeps the segments before it and drops the segments after
/// it (criteria 7-9).
#[test]
fn req611_xex_defects_keep_segments() {
    let first = [segment(0x2000, &[0x11])];
    check(
        &[0xFF, 0xFF, 0x00, 0x20, 0x00, 0x20, 0x11, 0x00, 0x00, 0x00],
        &first,
        defect(SegmentDefect::ShortHeader, 7),
        "7",
    );
    check(
        &[
            0xFF, 0xFF, 0x00, 0x20, 0x00, 0x20, 0x11, 0x00, 0x30, 0x05, 0x30, 0x22,
        ],
        &first,
        defect(SegmentDefect::ShortData, 7),
        "8",
    );
    check(
        &[
            0xFF, 0xFF, 0x00, 0x20, 0x00, 0x20, 0x11, 0x00, 0x30, 0xFF, 0x2F, 0x22, 0x00, 0x40,
            0x00, 0x40, 0x33,
        ],
        &first,
        defect(SegmentDefect::EndBeforeStart, 7),
        "9",
    );
    assert_eq!(
        SegmentDefect::ShortHeader.describe(),
        "short segment header"
    );
    assert_eq!(SegmentDefect::ShortData.describe(), "short segment data");
}

/// A record of the load stream (`docs/design/media.md`, "XEX boot image").
#[derive(Debug, Clone, PartialEq, Eq)]
enum Record {
    /// Copy `data` to `start`.
    Data(u16, Vec<u8>),
    /// `JSR` to a `JMP ($02E2)`.
    Init,
    /// `JMP ($02E0)`.
    RunIndirect,
    /// `JMP address`.
    RunDirect(u16),
}

/// The records in sectors 4 to N of the boot image of `xex`. Checks the geometry, the
/// loader in sectors 1-3 and the `$00` padding after the last record.
fn records(xex: &Xex) -> Vec<Record> {
    let image = xex.boot_image();
    let geometry = image.geometry();
    assert_eq!(geometry.sector_size, 128, "128-byte sectors");
    assert!(!image.changed(), "a new image is not changed");
    let stream = xex.load_stream();
    let expected_sectors = 3 + stream.len().div_ceil(128);
    assert_eq!(
        usize::from(geometry.sectors),
        expected_sectors,
        "sectors = 3 + ceil(stream / 128)"
    );
    let header = image.sector(1).unwrap();
    assert_eq!(
        header[..6],
        [0x00, 0x03, 0x00, 0x07, 0x17, 0x07],
        "boot header: 3 sectors at $0700, DOSINI = $0717"
    );
    let mut bytes: Vec<u8> = Vec::new();
    for n in 4..=geometry.sectors {
        let sector = image.sector(n).unwrap();
        assert_eq!(sector.len(), 128, "sector {n} length");
        bytes.extend(sector);
    }
    assert_eq!(
        image.sector(geometry.sectors + 1),
        None,
        "no sector after N"
    );
    assert_eq!(
        bytes[..stream.len()],
        stream[..],
        "sectors 4 to N = the stream"
    );
    assert!(
        bytes[stream.len()..].iter().all(|&byte| byte == 0),
        "the last sector is padded with $00"
    );
    let mut list = Vec::new();
    let mut p = 0;
    loop {
        let word = |at: usize| u16::from_le_bytes([stream[at], stream[at + 1]]);
        match stream[p] {
            0x00 => {
                let start = word(p + 1);
                let len = usize::from(word(p + 3)) + 1;
                list.push(Record::Data(start, stream[p + 5..p + 5 + len].to_vec()));
                p += 5 + len;
            }
            0x01 => {
                list.push(Record::Init);
                p += 1;
            }
            0x02 => {
                list.push(Record::RunIndirect);
                p += 1;
                break;
            }
            0x03 => {
                list.push(Record::RunDirect(word(p + 1)));
                p += 3;
                break;
            }
            other => panic!("record type {other:#04X} at {p}"),
        }
    }
    assert_eq!(p, stream.len(), "the Run record is the last record");
    list
}

/// An XEX file of `segments`.
fn xex_file(segments: &[(u16, &[u8])]) -> Vec<u8> {
    let mut file = vec![0xFF, 0xFF];
    for &(start, data) in segments {
        let end = start + u16::try_from(data.len() - 1).unwrap();
        file.extend(start.to_le_bytes());
        file.extend(end.to_le_bytes());
        file.extend(data);
    }
    file
}

/// Verifies REQ-612: `Xex::boot_image` gives the loader records for the segments and the
/// disk geometry of "XEX boot image" in `docs/design/media.md`.
#[test]
fn req612_boot_image_records() {
    let page: Vec<u8> = (0..=0xFF).collect();
    let high: Vec<u8> = (0xF0..=0xFF).collect();
    let run = [0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x4C, 0x05, 0x30];
    let xex = Xex::parse(&xex_file(&[
        (0x2000, &page),
        (0xA000, &high),
        (0x3000, &run),
        (0x02E0, &[0x00, 0x30]),
    ]))
    .unwrap();
    assert_eq!(
        records(&xex),
        [
            Record::Data(0x2000, page),
            Record::Data(0xA000, high),
            Record::Data(0x3000, run.to_vec()),
            Record::Data(0x02E0, vec![0x00, 0x30]),
            Record::RunIndirect,
        ],
        "one Data record for each segment, in file order"
    );
    assert_eq!(
        xex.boot_image().geometry().sectors,
        6,
        "3 + ceil(300 / 128)"
    );
    assert_eq!(xex.loader_conflict(), None, "no segment in $0700-$08FF");

    // Two segments at the same address keep their file order (REQ-612 criterion 4).
    let order = Xex::parse(&xex_file(&[(0x4000, &[0x11]), (0x4000, &[0x22])])).unwrap();
    assert_eq!(
        records(&order),
        [
            Record::Data(0x4000, vec![0x11]),
            Record::Data(0x4000, vec![0x22]),
            Record::RunDirect(0x4000),
        ],
        "file order"
    );

    // The largest segment. It includes INITAD and RUNAD.
    let full: Vec<u8> = vec![0x5A; 0x1_0000];
    let big = Xex::parse(&xex_file(&[(0x0000, &full)])).unwrap();
    assert_eq!(
        records(&big),
        [
            Record::Data(0x0000, full),
            Record::Init,
            Record::RunIndirect
        ],
        "a segment of 65536 bytes"
    );
    assert_eq!(
        big.loader_conflict(),
        Some(0x0000),
        "it overlaps the loader"
    );
    for (start, len, conflict) in [
        (0x06FF_u16, 1, None),
        (0x06FF, 2, Some(0x06FF)),
        (0x08FF, 1, Some(0x08FF)),
        (0x0900, 1, None),
    ] {
        let xex = Xex::parse(&xex_file(&[(start, &vec![0; len])])).unwrap();
        assert_eq!(xex.loader_conflict(), conflict, "${start:04X}, {len} bytes");
    }
}

/// Verifies REQ-613: with `INITAD` segments after segments C and E, the boot image has an
/// Init record after segments C and E only.
#[test]
fn req613_init_records() {
    let init = [0xEE, 0x00, 0x06, 0x60];
    let run = [0xA9, 0xA5, 0x8D, 0x01, 0x06, 0x4C, 0x05, 0x30];
    let xex = Xex::parse(&xex_file(&[
        (0x0600, &[0x00]),
        (0x2000, &init),
        (0x02E2, &[0x00, 0x20]),
        (0x0600, &[0x10]),
        (0x02E2, &[0x00, 0x20]),
        (0x3000, &run),
        (0x02E0, &[0x00, 0x30]),
    ]))
    .unwrap();
    assert_eq!(
        records(&xex),
        [
            Record::Data(0x0600, vec![0x00]),
            Record::Data(0x2000, init.to_vec()),
            Record::Data(0x02E2, vec![0x00, 0x20]),
            Record::Init,
            Record::Data(0x0600, vec![0x10]),
            Record::Data(0x02E2, vec![0x00, 0x20]),
            Record::Init,
            Record::Data(0x3000, run.to_vec()),
            Record::Data(0x02E0, vec![0x00, 0x30]),
            Record::RunIndirect,
        ],
        "Init after segments C and E only"
    );
    // A segment that includes only `$02E3`, or `$02E2` inside a longer range, also calls
    // INITAD. A segment that ends at `$02E1` does not.
    for (start, data, init) in [
        (0x02E3_u16, &[0x20][..], true),
        (0x02E0, &[0x00, 0x30, 0x00, 0x20][..], true),
        (0x02DE, &[0, 0, 0, 0x30][..], false),
    ] {
        let xex = Xex::parse(&xex_file(&[(start, data)])).unwrap();
        assert_eq!(
            records(&xex).contains(&Record::Init),
            init,
            "${start:04X}, {} bytes",
            data.len()
        );
    }
}

/// Verifies REQ-614: the boot image ends with a Run indirect record (`RUNAD` segment) or a
/// Run direct record to `$2000` (no `RUNAD` segment, first segment at `$2000`).
#[test]
fn req614_run_records() {
    let first = [0xA9, 0x5A, 0x8D, 0x00, 0x06, 0x4C, 0x05, 0x20];
    let second = [0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x4C, 0x05, 0x30];
    let with_runad = Xex::parse(&xex_file(&[
        (0x2000, &first),
        (0x3000, &second),
        (0x02E0, &[0x00, 0x30]),
    ]))
    .unwrap();
    assert_eq!(
        records(&with_runad).last(),
        Some(&Record::RunIndirect),
        "1: RUNAD segment"
    );
    let without = Xex::parse(&xex_file(&[(0x2000, &first), (0x3000, &second)])).unwrap();
    assert_eq!(
        records(&without).last(),
        Some(&Record::RunDirect(0x2000)),
        "2: no RUNAD segment"
    );
    // A segment that writes only `$02E1` also counts as a RUNAD segment.
    let high_byte = Xex::parse(&xex_file(&[(0x2000, &first), (0x02E1, &[0x20])])).unwrap();
    assert_eq!(
        records(&high_byte).last(),
        Some(&Record::RunIndirect),
        "$02E1 only"
    );
}
