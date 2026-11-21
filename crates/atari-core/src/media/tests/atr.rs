//! Tests of the ATR disk images: loading, errors and export (`docs/design/media.md`,
//! "Tests").

use super::sio_host::{Host, atr_file, dd_file, ed_file, load, sd_file};
use crate::media::{DiskImage, Geometry, MediaError};

/// Header bytes 7-15 of the export tests (REQ-637).
const EXTRA: [u8; 9] = [1, 2, 3, 4, 5, 6, 7, 8, 9];

/// Checks that sector `n` of `image` is equal to file bytes `start..=end` of `file`.
fn check_sector(image: &DiskImage, n: u16, file: &[u8], (start, end): (usize, usize), name: &str) {
    assert_eq!(
        image.sector(n),
        Some(&file[start..=end]),
        "{name}: sector {n} is file bytes {start} to {end}"
    );
}

/// Verifies REQ-625: the SD (720 x 128), ED (1040 x 128) and DD (720 sectors, 1-3 of 128
/// bytes) headers load, and sectors 1, 3, 4 and 720 hold the file bytes of the criteria
/// (criteria 1-5).
#[test]
fn req625_atr_sd_ed_dd() {
    let sd = sd_file();
    let image = load(&sd);
    assert_eq!(image.geometry(), Geometry::SD, "1: SD");
    assert!(!image.changed(), "1: no change after the load");
    check_sector(&image, 1, &sd, (16, 143), "4: SD");
    check_sector(&image, 720, &sd, (92_048, 92_175), "4: SD");
    assert_eq!(image.sector(721), None, "SD sector 721");

    assert_eq!(load(&ed_file()).geometry(), Geometry::ED, "2: ED");

    let dd = dd_file();
    let image = load(&dd);
    assert_eq!(image.geometry(), Geometry::DD, "3: DD");
    for n in 1..=3 {
        assert_eq!(
            image.sector(n).map(<[u8]>::len),
            Some(128),
            "3: DD sector {n}"
        );
    }
    for n in 4..=720 {
        assert_eq!(
            image.sector(n).map(<[u8]>::len),
            Some(256),
            "3: DD sector {n}"
        );
    }
    check_sector(&image, 3, &dd, (272, 399), "5: DD");
    check_sector(&image, 4, &dd, (400, 655), "5: DD");
    check_sector(&image, 720, &dd, (183_696, 183_951), "5: DD");
}

/// Verifies REQ-625: the SD file with 100 more bytes after the data loads with 720 sectors,
/// and sector 720 holds file bytes 92,048 to 92,175 (criterion 6).
#[test]
fn req625_atr_extra_bytes() {
    let mut file = sd_file();
    file.extend_from_slice(&[0xEE; 100]);
    let image = load(&file);
    assert_eq!(image.geometry(), Geometry::SD, "geometry");
    check_sector(&image, 720, &file, (92_048, 92_175), "6");
}

/// Verifies REQ-625: a header with the high size byte `$02` and 2,097,152 data bytes loads
/// with 16384 sectors of 128 bytes (criterion 7).
#[test]
fn req625_atr_large() {
    let file = atr_file(
        [0x96, 0x02, 0x00, 0x00, 0x80, 0x00, 0x02],
        [0; 9],
        0x0020_0000, // 2,097,152 bytes
    );
    let image = load(&file);
    assert_eq!(
        image.geometry(),
        Geometry {
            sectors: 0x4000,
            sector_size: 128
        },
        "7"
    );
    check_sector(
        &image,
        0x4000,
        &file,
        (2_097_040, 2_097_040 + 127),
        "7: last sector",
    );
}

/// Verifies REQ-625: the full boot sector rule of "ATR loading" in `docs/design/media.md`
/// for images with 256-byte sectors: a data size of `N * 256` gives N sectors, sectors 1-3
/// keep the first 128 bytes of their 256-byte slots, and the export writes sectors 1-3 as
/// 256 bytes again with the second half `$00`.
#[test]
fn req625_atr_dd_boot_sectors() {
    // 720 * 256 = 184,320 bytes = 11,520 units = $2D00.
    let file = atr_file([0x96, 0x02, 0x00, 0x2D, 0x00, 0x01, 0x00], EXTRA, 184_320);
    let image = load(&file);
    assert_eq!(image.geometry(), Geometry::DD, "geometry");
    check_sector(&image, 1, &file, (16, 143), "sector 1");
    check_sector(&image, 2, &file, (272, 399), "sector 2");
    check_sector(&image, 3, &file, (528, 655), "sector 3");
    check_sector(&image, 4, &file, (784, 1_039), "sector 4");
    check_sector(&image, 720, &file, (184_080, 184_335), "sector 720");

    let export = image.to_atr();
    assert_eq!(export.len(), file.len(), "export length");
    assert_eq!(export[..16], file[..16], "export header");
    for slot in 0..3 {
        let start = 16 + slot * 256;
        assert_eq!(
            export[start..start + 128],
            file[start..start + 128],
            "boot {slot}"
        );
        assert!(
            export[start + 128..start + 256].iter().all(|&b| b == 0),
            "boot sector {slot}: second half $00"
        );
    }
    assert_eq!(export[784..], file[784..], "sectors 4-720");

    // A normal DD file with sectors 1-3 of 128 bytes is not this layout.
    assert!(!load(&dd_file()).to_atr().is_empty(), "normal DD export");
    // Three 256-byte slots only: 3 sectors.
    let small = atr_file([0x96, 0x02, 0x30, 0x00, 0x00, 0x01, 0x00], [0; 9], 768);
    assert_eq!(
        DiskImage::from_atr(&small).map(|image| image.geometry()),
        Ok(Geometry {
            sectors: 3,
            sector_size: 256
        }),
        "768 bytes: 3 full boot sectors"
    );
}

/// The SD file of REQ-626 with the changes of `change`.
fn changed_sd(change: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut file = sd_file();
    change(&mut file);
    file
}

/// Verifies REQ-626: the errors `AtrHeader`, `AtrSignature`, `AtrSectorSize`,
/// `AtrDataSize`, `AtrTruncated` and `AtrSectorCount` for the files of the criteria
/// (criteria 1-8).
#[test]
fn req626_atr_errors() {
    let cases: [(&str, Vec<u8>, MediaError); 8] = [
        ("1", changed_sd(|f| f.truncate(15)), MediaError::AtrHeader),
        (
            "2",
            changed_sd(|f| f[1] = 0x03),
            MediaError::AtrSignature(0x0396),
        ),
        (
            "3",
            changed_sd(|f| f[4..6].copy_from_slice(&[0x00, 0x02])),
            MediaError::AtrSectorSize(512),
        ),
        (
            "4",
            changed_sd(|f| {
                f[2] = 0x81;
                f.extend_from_slice(&[0; 16]);
            }),
            MediaError::AtrDataSize(92_176),
        ),
        (
            "5",
            changed_sd(|f| f.truncate(16 + 92_000)),
            MediaError::AtrTruncated {
                expected: 92_160,
                actual: 92_000,
            },
        ),
        (
            "6",
            changed_sd(|f| {
                f[2..4].copy_from_slice(&[0, 0]);
                f.truncate(16);
            }),
            MediaError::AtrSectorCount(0),
        ),
        (
            "7",
            changed_sd(|f| {
                f[2..4].copy_from_slice(&[0, 0]);
                f[6] = 0x08;
                f.resize(16 + 0x0080_0000, 0); // 8,388,608 data bytes
            }),
            MediaError::AtrSectorCount(0x0001_0000),
        ),
        (
            "8",
            changed_sd(|f| {
                f[4..6].copy_from_slice(&[0x00, 0x01]);
                f[2..4].copy_from_slice(&[0x19, 0x00]);
                f.truncate(16 + 400);
            }),
            MediaError::AtrDataSize(400),
        ),
    ];
    for (name, file, error) in cases {
        assert_eq!(DiskImage::from_atr(&file), Err(error), "criterion {name}");
        assert!(!error.to_string().is_empty(), "criterion {name}: message");
    }
}

/// Verifies REQ-626: a file with more than one defect gives the first error in the order of
/// the criterion (criterion 9).
#[test]
fn req626_atr_error_order() {
    let mut short = sd_file();
    short.truncate(15);
    short[1] = 0x03;
    assert_eq!(
        DiskImage::from_atr(&short),
        Err(MediaError::AtrHeader),
        "header first"
    );
    // Signature before the sector size.
    let file = changed_sd(|f| {
        f[1] = 0x03;
        f[4] = 0x00;
    });
    assert_eq!(
        DiskImage::from_atr(&file),
        Err(MediaError::AtrSignature(0x0396)),
        "signature"
    );
    // Sector size before the truncation.
    let file = changed_sd(|f| {
        f[4..6].copy_from_slice(&[0x00, 0x02]);
        f.truncate(100);
    });
    assert_eq!(
        DiskImage::from_atr(&file),
        Err(MediaError::AtrSectorSize(512)),
        "sector size"
    );
    // Truncation before the data size.
    let file = changed_sd(|f| {
        f[2] = 0x81;
        f.truncate(100);
    });
    assert_eq!(
        DiskImage::from_atr(&file),
        Err(MediaError::AtrTruncated {
            expected: 92_176,
            actual: 84
        }),
        "truncated"
    );
    // Data size before the sector count: 256-byte sectors, 16 bytes.
    let file = changed_sd(|f| {
        f[2..4].copy_from_slice(&[0x01, 0x00]);
        f[4..6].copy_from_slice(&[0x00, 0x01]);
    });
    assert_eq!(
        DiskImage::from_atr(&file),
        Err(MediaError::AtrDataSize(16)),
        "data size"
    );
}

/// Verifies REQ-637: the export of an unchanged SD file (without the extra bytes) and DD
/// file is equal to the input, the image reports no change, and export bytes 7-15 are
/// `01` to `09` (criteria 1, 2, 5).
#[test]
fn req637_export_unchanged() {
    let sd = atr_file([0x96, 0x02, 0x80, 0x16, 0x80, 0x00, 0x00], EXTRA, 92_160);
    let mut with_tail = sd.clone();
    with_tail.extend_from_slice(&[0xEE; 100]);
    let image = load(&with_tail);
    assert!(!image.changed(), "1: no change");
    assert_eq!(image.to_atr(), sd, "1: export without the 100 bytes");
    let dd = atr_file([0x96, 0x02, 0xE8, 0x2C, 0x00, 0x01, 0x00], EXTRA, 183_936);
    let image = load(&dd);
    assert!(!image.changed(), "2: no change");
    let export = image.to_atr();
    assert_eq!(export.len(), 183_952, "2: length");
    assert_eq!(export, dd, "2: export");
    assert_eq!(export[7..16], EXTRA, "5: header bytes 7-15");
}

/// Verifies REQ-637: after Put of SD sector 400, the export loads and holds the new sector,
/// and the image reports a change (criteria 3, 5).
#[test]
fn req637_export_after_write() {
    let sd = atr_file([0x96, 0x02, 0x80, 0x16, 0x80, 0x00, 0x00], EXTRA, 92_160);
    let mut host = Host::ntsc(Some(load(&sd)));
    assert_eq!(
        host.write(0x50, 400, &[0x5A; 128]),
        [0x41, 0x41, 0x43],
        "Put"
    );
    assert!(host.image().changed(), "3: change");
    let export = host.image().to_atr();
    let reloaded = DiskImage::from_atr(&export).unwrap();
    assert_eq!(
        reloaded.sector(400),
        Some(&[0x5A; 128][..]),
        "3: sector 400"
    );
    assert_eq!(export[7..16], EXTRA, "5: header bytes 7-15");
}

/// Verifies REQ-637: after Format `$21` of the ED image, the export has 92,176 bytes, the SD
/// header and all data bytes `$00` (criteria 4, 5).
#[test]
fn req637_export_after_format() {
    let ed = atr_file([0x96, 0x02, 0x80, 0x20, 0x80, 0x00, 0x00], EXTRA, 133_120);
    let mut host = Host::ntsc(Some(load(&ed)));
    let reply = host.command(0x21, 0, 0);
    assert_eq!(reply.get(..2), Some(&[0x41, 0x43][..]), "Format");
    assert!(host.image().changed(), "change");
    let export = host.image().to_atr();
    assert_eq!(export.len(), 92_176, "4: length");
    assert_eq!(
        export[..7],
        [0x96, 0x02, 0x80, 0x16, 0x80, 0x00, 0x00],
        "4: header"
    );
    assert!(export[16..].iter().all(|&b| b == 0), "4: data bytes $00");
    assert_eq!(export[7..16], EXTRA, "5: header bytes 7-15");
}
