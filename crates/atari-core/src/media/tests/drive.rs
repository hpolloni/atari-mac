//! Tests of the disk drive commands and timing (`docs/design/media.md`, "Unit test method
//! for the SIO bus").

use core::iter;

use super::sio_host::{
    Host, assert_quiet, dd_file, device_bytes, ed_file, end_of, load, sd_file, sd_test_image,
};
use crate::media::{DiskImage, Geometry, sio_checksum};

/// The SD, ED and DD Percom blocks of REQ-634.
const SD_BLOCK: [u8; 12] = [
    0x28, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x80, 0xFF, 0x00, 0x00, 0x00,
];
/// The ED Percom block of REQ-634.
const ED_BLOCK: [u8; 12] = [
    0x28, 0x00, 0x00, 0x1A, 0x00, 0x04, 0x00, 0x80, 0xFF, 0x00, 0x00, 0x00,
];
/// The DD Percom block of REQ-634.
const DD_BLOCK: [u8; 12] = [
    0x28, 0x00, 0x00, 0x12, 0x00, 0x04, 0x01, 0x00, 0xFF, 0x00, 0x00, 0x00,
];

/// A write-protected copy of `image`.
fn protected(mut image: DiskImage) -> DiskImage {
    image.set_write_protected(true);
    image
}

/// Checks that `reply` is `41`, `result`, `len` bytes of `fill` and the checksum.
fn check_frame_reply(reply: &[u8], result: u8, fill: u8, len: usize, name: &str) {
    let mut expected = vec![0x41, result];
    expected.extend(iter::repeat_n(fill, len));
    expected.push(sio_checksum(&vec![fill; len]));
    assert_eq!(reply, expected, "{name}");
}

/// Checks that the last reply is only NAK, with no byte for 100 ms after it (REQ-628).
fn check_nak(host: &Host, reply: &[u8], name: &str) {
    assert_eq!(reply, [0x4E], "{name}: NAK");
    let nak = device_bytes(&host.bus).last().copied().unwrap();
    assert_quiet(&host.bus, end_of(nak), name);
}

/// Verifies REQ-627: the Status frames of the SD, ED, DD, 16384-sector, write-protected and
/// empty images, and the same frame for AUX1 = AUX2 = `$FF` (criteria 1-6, 9).
#[test]
fn req627_status_frames() {
    let cases = [
        ("1: SD", Some(load(&sd_file())), [0x00, 0xFF, 0xE0, 0x00]),
        ("2: ED", Some(load(&ed_file())), [0x80, 0xFF, 0xE0, 0x00]),
        ("3: DD", Some(load(&dd_file())), [0x20, 0xFF, 0xE0, 0x00]),
        (
            "4: 16384 sectors",
            Some(DiskImage::blank(Geometry {
                sectors: 0x4000,
                sector_size: 128,
            })),
            [0x00, 0xFF, 0xE0, 0x00],
        ),
        (
            "5: write-protected",
            Some(protected(load(&sd_file()))),
            [0x00, 0xBF, 0xE0, 0x00],
        ),
        ("6: no image", None, [0x00, 0x7F, 0xE0, 0x00]),
    ];
    for (name, image, frame) in cases {
        let with_aux = matches!(name, "1: SD" | "3: DD" | "6: no image");
        let mut host = Host::ntsc(image);
        assert_eq!(host.status(), frame, "{name}");
        if with_aux {
            let reply = host.command(0x53, 0xFF, 0xFF);
            assert_eq!(reply.get(2..6), Some(&frame[..]), "9: {name} with AUX $FF");
        }
    }
}

/// Verifies REQ-627: after a NAK, the next Status has byte 0 bit 0 set and the one after it
/// does not; a completed command clears the bit (criteria 7, 8).
#[test]
fn req627_status_error_bits() {
    let mut host = Host::ntsc(Some(load(&sd_file())));
    assert_eq!(host.command(0x52, 0, 0), [0x4E], "7: NAK");
    assert_eq!(host.status(), [0x01, 0xFF, 0xE0, 0x00], "7: first Status");
    assert_eq!(host.status(), [0x00, 0xFF, 0xE0, 0x00], "7: second Status");

    let mut host = Host::ntsc(Some(load(&sd_file())));
    assert_eq!(host.command(0x52, 0, 0), [0x4E], "8: NAK");
    assert_eq!(host.read_sector(1).len(), 128, "8: Read sector 1");
    assert_eq!(host.status(), [0x00, 0xFF, 0xE0, 0x00], "8: Status");
}

/// Verifies REQ-628: with an SD image, each command code except `$21`, `$22`, `$4E`, `$4F`,
/// `$50`, `$52`, `$53` and `$57` gets NAK (criterion 1).
#[test]
fn req628_nak_commands() {
    let supported = [0x21, 0x22, 0x4E, 0x4F, 0x50, 0x52, 0x53, 0x57];
    for command in 0..=0xFF_u8 {
        let mut host = Host::ntsc(Some(load(&sd_file())));
        let reply = host.command(command, 0x01, 0x00);
        if supported.contains(&command) {
            assert_eq!(reply.first(), Some(&0x41), "command ${command:02X}: ACK");
        } else {
            check_nak(&host, &reply, &format!("command ${command:02X}"));
        }
    }
}

/// Verifies REQ-628: Read, Put and Write of sector 0 or of a sector after the last one get
/// NAK for the SD, ED and DD images, and the last sector gets ACK (criteria 2-6).
#[test]
fn req628_nak_sector_range() {
    let mut host = Host::ntsc(Some(load(&sd_file())));
    let reply = host.command(0x52, 0x00, 0x00);
    check_nak(&host, &reply, "2: SD sector 0");
    for (name, file, first_bad) in [
        ("3: SD", sd_file(), 721_u16),
        ("4: ED", ed_file(), 1041),
        ("5: DD", dd_file(), 721),
    ] {
        let mut host = Host::ntsc(Some(load(&file)));
        let [aux1, aux2] = first_bad.to_le_bytes();
        let reply = host.command(0x52, aux1, aux2);
        check_nak(&host, &reply, &format!("{name}: sector {first_bad}"));
        let [aux1, aux2] = (first_bad - 1).to_le_bytes();
        let reply = host.command(0x52, aux1, aux2);
        assert_eq!(reply.first(), Some(&0x41), "{name}: last sector gets ACK");
    }
    for command in [0x50, 0x57] {
        for sector in [0_u16, 721] {
            let mut host = Host::ntsc(Some(load(&sd_file())));
            let [aux1, aux2] = sector.to_le_bytes();
            let reply = host.command(command, aux1, aux2);
            let name = format!("6: command ${command:02X} sector {sector}");
            check_nak(&host, &reply, &name);
            assert!(!host.image().changed(), "{name}: no change");
        }
    }
}

/// Checks that `data` is equal to file bytes `start..=end` of `file`.
fn check_file_bytes(data: &[u8], file: &[u8], start: usize, end: usize, name: &str) {
    assert_eq!(data.len(), end - start + 1, "{name}: length");
    assert_eq!(data, &file[start..=end], "{name}: bytes");
}

/// Verifies REQ-629: Read of SD sectors 1 and 720 and ED sector 1040 gives the file bytes of
/// the sector, with the checksum of the data frame (criteria 1-3, 6).
#[test]
fn req629_read_sd_ed() {
    let sd = sd_file();
    let mut host = Host::ntsc(Some(load(&sd)));
    check_file_bytes(&host.read_sector(1), &sd, 16, 143, "1: SD sector 1");
    check_file_bytes(
        &host.read_sector(720),
        &sd,
        92_048,
        92_175,
        "2: SD sector 720",
    );
    let ed = ed_file();
    let mut host = Host::ntsc(Some(load(&ed)));
    check_file_bytes(
        &host.read_sector(1040),
        &ed,
        133_008,
        133_135,
        "3: ED sector 1040",
    );
}

/// Verifies REQ-629: Read of DD sector 3 gives 128 bytes and of DD sector 4 gives 256 bytes
/// of the file, with the checksum of the data frame (criteria 4-6).
#[test]
fn req629_read_dd() {
    let dd = dd_file();
    let mut host = Host::ntsc(Some(load(&dd)));
    check_file_bytes(&host.read_sector(3), &dd, 272, 399, "4: DD sector 3");
    check_file_bytes(&host.read_sector(4), &dd, 400, 655, "5: DD sector 4");
}

/// Verifies REQ-630: Put and Write of SD sector 400, then Read gives the new bytes; the
/// image reports a change, and the export differs only in sector 400 (criteria 1, 2, 4, 5).
#[test]
fn req630_put_write_roundtrip() {
    let sd = sd_file();
    let mut host = Host::ntsc(Some(load(&sd)));
    assert!(!host.image().changed(), "4: no change before the Put");
    assert_eq!(
        host.write(0x50, 400, &[0x5A; 128]),
        [0x41, 0x41, 0x43],
        "1: Put"
    );
    assert_eq!(host.read_sector(400), [0x5A; 128], "1: Read after Put");
    assert!(host.image().changed(), "4: change after the Put");
    let export = host.image().to_atr();
    assert_eq!(export.len(), sd.len(), "5: export length");
    for (i, (&a, &b)) in export.iter().zip(&sd).enumerate() {
        if (51_088..=51_215).contains(&i) {
            assert_eq!(a, 0x5A, "5: export byte {i}");
        } else {
            assert_eq!(a, b, "5: export byte {i}");
        }
    }
    assert_eq!(
        host.write(0x57, 400, &[0xA5; 128]),
        [0x41, 0x41, 0x43],
        "2: Write"
    );
    assert_eq!(host.read_sector(400), [0xA5; 128], "2: Read after Write");
}

/// Verifies REQ-630: DD image: Put of sector 2 takes 128 bytes and Put of sector 10 takes
/// 256 bytes, each gets `41`, `41`, `43` and reads back (criterion 3).
#[test]
fn req630_put_dd_frame_sizes() {
    let mut host = Host::ntsc(Some(load(&dd_file())));
    assert_eq!(
        host.write(0x50, 2, &[0x5A; 128]),
        [0x41, 0x41, 0x43],
        "sector 2"
    );
    assert_eq!(host.read_sector(2), [0x5A; 128], "read sector 2");
    assert_eq!(
        host.write(0x50, 10, &[0x5A; 256]),
        [0x41, 0x41, 0x43],
        "sector 10"
    );
    assert_eq!(host.read_sector(10), [0x5A; 256], "read sector 10");
}

/// Verifies REQ-631: on a write-protected image, Put, Write and both Format commands give
/// the Error byte `45` and change no data; Status then gives `08 BF E0 00`, and the image
/// reports no change (criteria 1-6).
#[test]
fn req631_write_protect() {
    let mut host = Host::ntsc(Some(protected(sd_test_image())));
    assert_eq!(
        host.write(0x50, 5, &[0x5A; 128]),
        [0x41, 0x41, 0x45],
        "1: Put"
    );
    assert_eq!(
        host.image().sector(5),
        Some(&[0x05; 128][..]),
        "1: sector 5"
    );
    assert_eq!(
        host.status(),
        [0x08, 0xBF, 0xE0, 0x00],
        "5: Status after the Put"
    );
    assert_eq!(
        host.write(0x57, 5, &[0x5A; 128]),
        [0x41, 0x41, 0x45],
        "2: Write"
    );
    assert_eq!(
        host.image().sector(5),
        Some(&[0x05; 128][..]),
        "2: sector 5"
    );
    let reply = host.command(0x21, 0, 0);
    check_frame_reply(&reply, 0x45, 0xFF, 128, "3: Format");
    assert_eq!(host.image().geometry(), Geometry::SD, "3: geometry");
    assert_eq!(
        host.image().sector(5),
        Some(&[0x05; 128][..]),
        "3: sector 5"
    );
    let reply = host.command(0x22, 0, 0);
    check_frame_reply(&reply, 0x45, 0xFF, 128, "4: Format medium");
    assert_eq!(host.image().geometry(), Geometry::SD, "4: geometry");
    assert!(!host.image().changed(), "6: no change");
}

/// Checks that each sector of the image of `host` reads as zeros of its length.
fn check_zero_sectors(host: &Host, name: &str) {
    let geometry = host.image().geometry();
    for n in 1..=geometry.sectors {
        let len = geometry.sector_len(n).unwrap();
        assert_eq!(
            host.image().sector(n),
            Some(vec![0; len].as_slice()),
            "{name}: sector {n}"
        );
    }
}

/// Verifies REQ-632: Format `$21` of a DD image with no Write Percom block gives 720 sectors
/// of 128 bytes (criterion 3).
#[test]
fn req632_format_sd() {
    let mut host = Host::ntsc(Some(load(&dd_file())));
    let reply = host.command(0x21, 0, 0);
    check_frame_reply(&reply, 0x43, 0xFF, 128, "3: reply");
    assert_eq!(host.image().geometry(), Geometry::SD, "3: geometry");
    check_zero_sectors(&host, "3");
}

/// Verifies REQ-632: Format `$21` of the ED image gives 720 zero sectors, Format `$22` of the
/// SD image gives 1040 zero sectors and Status byte 0 = `$80`, and the image reports a
/// change (criteria 1, 2, 5).
#[test]
fn req632_format_ed() {
    let mut host = Host::ntsc(Some(load(&ed_file())));
    assert!(!host.image().changed(), "5: no change before the format");
    let reply = host.command(0x21, 0, 0);
    check_frame_reply(&reply, 0x43, 0xFF, 128, "1: reply");
    assert_eq!(host.image().geometry(), Geometry::SD, "1: geometry");
    check_zero_sectors(&host, "1");
    assert!(host.image().changed(), "5: change after the format");

    let mut host = Host::ntsc(Some(load(&sd_file())));
    let reply = host.command(0x22, 0, 0);
    check_frame_reply(&reply, 0x43, 0xFF, 128, "2: reply");
    assert_eq!(host.image().geometry(), Geometry::ED, "2: geometry");
    check_zero_sectors(&host, "2");
    assert_eq!(host.status().first(), Some(&0x80), "2: Status byte 0");
}

/// Verifies REQ-632: Write Percom block with the DD block, then Format `$21`: a 256-byte
/// reply frame, sectors 1-3 of 128 bytes and sectors 4-720 of 256 bytes (criterion 4).
#[test]
fn req632_format_dd() {
    let mut host = Host::ntsc(Some(load(&sd_file())));
    assert_eq!(
        host.write(0x4F, 0, &DD_BLOCK),
        [0x41, 0x41, 0x43],
        "Write Percom"
    );
    let reply = host.command(0x21, 0, 0);
    check_frame_reply(&reply, 0x43, 0xFF, 256, "4: reply");
    assert_eq!(host.image().geometry(), Geometry::DD, "4: geometry");
    assert_eq!(
        host.image().sector(3).map(<[u8]>::len),
        Some(128),
        "4: sector 3"
    );
    assert_eq!(
        host.image().sector(4).map(<[u8]>::len),
        Some(256),
        "4: sector 4"
    );
    check_zero_sectors(&host, "4");
}

/// Verifies REQ-633: with no image, Status, Read, Put, Write and Format give the replies of
/// the criteria, and Read of sector 0 gets NAK (criteria 1-7).
#[test]
fn req633_empty_drive() {
    let mut host = Host::ntsc(None);
    assert_eq!(
        host.command(0x53, 0, 0),
        [0x41, 0x43, 0x00, 0x7F, 0xE0, 0x00, 0x60],
        "1: Status"
    );
    check_frame_reply(&host.command(0x52, 1, 0), 0x45, 0x00, 128, "2: Read 1");
    check_frame_reply(
        &host.command(0x52, 0xFF, 0xFF),
        0x45,
        0x00,
        128,
        "3: Read 65535",
    );
    assert_eq!(
        host.write(0x50, 1, &[0x5A; 128]),
        [0x41, 0x41, 0x45],
        "4: Put"
    );
    assert_eq!(
        host.write(0x57, 1, &[0x5A; 128]),
        [0x41, 0x41, 0x45],
        "5: Write"
    );
    check_frame_reply(&host.command(0x21, 0, 0), 0x45, 0xFF, 128, "6: Format");
    let reply = host.command(0x52, 0, 0);
    check_nak(&host, &reply, "7: Read 0");
}

/// Verifies REQ-634: the Percom blocks of the SD, ED, DD, 16384-sector and 1440-sector
/// images, and the same block for AUX1 = AUX2 = `$FF` (criteria 1-4).
#[test]
fn req634_percom_blocks() {
    let geometry = |sectors, sector_size| Geometry {
        sectors,
        sector_size,
    };
    let cases = [
        ("1: SD", load(&sd_file()), SD_BLOCK),
        ("1: ED", load(&ed_file()), ED_BLOCK),
        ("1: DD", load(&dd_file()), DD_BLOCK),
        (
            "2: 16384 x 128",
            DiskImage::blank(geometry(0x4000, 128)),
            [
                0x01, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x80, 0xFF, 0x00, 0x00, 0x00,
            ],
        ),
        (
            "3: 1440 x 256",
            DiskImage::blank(geometry(1440, 256)),
            [
                0x01, 0x00, 0x05, 0xA0, 0x00, 0x04, 0x01, 0x00, 0xFF, 0x00, 0x00, 0x00,
            ],
        ),
    ];
    for (name, image, block) in cases {
        let mut host = Host::ntsc(Some(image));
        let mut expected = vec![0x41, 0x43];
        expected.extend_from_slice(&block);
        expected.push(sio_checksum(&block));
        assert_eq!(host.command(0x4E, 0, 0), expected, "{name}");
        assert_eq!(
            host.command(0x4E, 0xFF, 0xFF),
            expected,
            "4: {name} with AUX $FF"
        );
    }
}

/// Verifies REQ-635: Write Percom block sets the geometry of the next Format `$21` only, a
/// new insertion or the SD block resets it, an 80-track block gives `45`, and the step
/// rate and drive-present bytes are ignored (criteria 1-6).
#[test]
fn req635_write_percom() {
    let sd = sd_file();
    let format = |host: &mut Host| {
        let reply = host.command(0x21, 0, 0);
        assert_eq!(reply.get(..2), Some(&[0x41, 0x43][..]), "format reply");
        host.image().geometry()
    };

    let mut host = Host::ntsc(Some(load(&sd)));
    assert_eq!(
        host.write(0x4F, 0, &DD_BLOCK),
        [0x41, 0x41, 0x43],
        "1: reply"
    );
    // Criterion 4: the image does not change before the format.
    assert_eq!(host.image(), &load(&sd), "4: image unchanged");
    assert!(!host.image().changed(), "4: no change");
    assert_eq!(format(&mut host), Geometry::DD, "1: format");

    let mut host = Host::ntsc(Some(load(&sd)));
    assert_eq!(
        host.write(0x4F, 0, &DD_BLOCK),
        [0x41, 0x41, 0x43],
        "DD block"
    );
    assert_eq!(
        host.write(0x4F, 0, &SD_BLOCK),
        [0x41, 0x41, 0x43],
        "2: SD block"
    );
    assert_eq!(format(&mut host), Geometry::SD, "2: format");

    let mut host = Host::ntsc(Some(load(&sd)));
    let mut tracks_80 = DD_BLOCK;
    tracks_80[0] = 0x50;
    assert_eq!(
        host.write(0x4F, 0, &tracks_80),
        [0x41, 0x41, 0x45],
        "3: 80 tracks"
    );
    assert_eq!(format(&mut host), Geometry::SD, "3: format");

    let mut host = Host::ntsc(Some(load(&sd)));
    assert_eq!(
        host.write(0x4F, 0, &DD_BLOCK),
        [0x41, 0x41, 0x43],
        "DD block"
    );
    host.bus.drive_mut().insert(load(&sd));
    assert_eq!(
        format(&mut host),
        Geometry::SD,
        "5: format after the insertion"
    );

    let mut host = Host::ntsc(Some(load(&sd)));
    let mut other_bytes = DD_BLOCK;
    other_bytes[1] = 0x03;
    other_bytes[8] = 0x00;
    assert_eq!(
        host.write(0x4F, 0, &other_bytes),
        [0x41, 0x41, 0x43],
        "6: reply"
    );
    assert_eq!(format(&mut host), Geometry::DD, "6: format");
}

/// The time of the last command: from the end of the stop bit of the last ACK byte to the
/// start of the Complete or Error byte (REQ-638).
fn op_time(host: &Host) -> u64 {
    let schedule = device_bytes(&host.bus);
    let index = schedule
        .iter()
        .position(|byte| byte.0 == 0x43 || byte.0 == 0x45)
        .unwrap();
    schedule[index].1 - end_of(schedule[index - 1])
}

/// One run of REQ-638: a new drive with `file`, the first command Read sector `first`, and
/// then `command` on `sector`. Gives the time of the second command.
fn run_time(file: &[u8], first: u16, command: u8, sector: u16) -> u64 {
    let mut host = Host::ntsc(Some(load(file)));
    assert_eq!(host.read_sector(first).len(), 128, "first Read {first}");
    let [aux1, aux2] = sector.to_le_bytes();
    let reply = if command == 0x52 {
        host.command(command, aux1, aux2)
    } else {
        host.write(command, sector, &[0x5A; 128])
    };
    assert!(
        reply.contains(&0x43),
        "command ${command:02X} {sector}: Complete"
    );
    op_time(&host)
}

/// Checks that `a` is `ms` ms more than `b`, within 0.1 ms (179 cycles at NTSC).
fn check_more(a: u64, b: u64, ms: u64, name: &str) {
    let expected = ms * 1_789_772 / 1_000;
    let diff = a.abs_diff(b + expected);
    assert!(a > b && diff <= 179, "{name}: {a} - {b} is not {ms} ms");
}

/// Verifies REQ-638: a Read on another track takes 40 ms more for each track step and 60 ms
/// more for two steps, for the SD and ED images, and a Put on track 1 takes 40 ms more
/// (criteria 1-4, 6).
#[test]
fn req638_seek_time() {
    let sd = sd_file();
    let ed = ed_file();
    check_more(
        run_time(&sd, 1, 0x52, 19),
        run_time(&sd, 1, 0x52, 2),
        40,
        "1",
    );
    check_more(
        run_time(&sd, 1, 0x52, 37),
        run_time(&sd, 1, 0x52, 2),
        60,
        "2",
    );
    check_more(
        run_time(&sd, 37, 0x52, 1),
        run_time(&sd, 37, 0x52, 38),
        60,
        "3",
    );
    check_more(
        run_time(&ed, 1, 0x52, 27),
        run_time(&ed, 1, 0x52, 2),
        40,
        "4",
    );
    check_more(
        run_time(&sd, 1, 0x50, 19),
        run_time(&sd, 1, 0x50, 2),
        40,
        "6",
    );
}

/// Verifies REQ-638: Write sector 2 takes 208 ms more than Put sector 2 (criterion 5).
#[test]
fn req638_write_verify_time() {
    let sd = sd_file();
    check_more(
        run_time(&sd, 1, 0x57, 2),
        run_time(&sd, 1, 0x50, 2),
        208,
        "5",
    );
}
