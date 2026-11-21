//! Tests of the SIO bus: command frames, checksums and reply timing
//! (`docs/design/media.md`, "Unit test method for the SIO bus").

use super::sio_host::{
    HOST_BIT, Host, QUIET, RISE_DELAY, assert_quiet, decode_line, device_bytes, end_of,
    sd_test_image, send_bytes, send_frame, send_frame_with, values, with_checksum,
};
use crate::media::{
    CommandFrame, DiskImage, DiskTransfer, Geometry, SioBus, SioData, SioOutcome, SioRequest,
    sio_checksum,
};
use crate::timing::VideoStandard;

/// The Status frame for D1:.
const STATUS: [u8; 5] = [0x31, 0x53, 0x00, 0x00, 0x84];

/// The Status reply with the SD test image (REQ-619 criterion 1).
const STATUS_REPLY: [u8; 7] = [0x41, 0x43, 0x00, 0xFF, 0xE0, 0x00, 0xE0];

/// A bus for `standard` with the SD test image in D1:.
fn sd_bus(standard: VideoStandard) -> SioBus {
    let mut bus = SioBus::new(standard);
    bus.drive_mut().insert(sd_test_image());
    bus
}

/// Both video standards.
const STANDARDS: [VideoStandard; 2] = [VideoStandard::Ntsc, VideoStandard::Pal];

/// Verifies REQ-615: the Status frame `31 53 00 00 84`, sent while the command line is low,
/// gets `41` and the Status reply after the command line goes high (criterion 1). The line
/// level, decoded cycle by cycle, agrees with the schedule.
#[test]
fn req615_status_frame_reply() {
    let mut bus = sd_bus(VideoStandard::Ntsc);
    let rise = send_frame(&mut bus, &STATUS, 1_000, HOST_BIT);
    assert_eq!(values(&bus), STATUS_REPLY, "reply");
    let schedule = device_bytes(&bus);
    assert!(
        schedule.iter().all(|byte| byte.1 >= rise),
        "after the rising edge"
    );
    let last = schedule.last().map_or(rise, |&byte| end_of(byte));
    let decoded = decode_line(&bus, 0, last + 1_000, 91);
    let expected: Vec<(u8, u64)> = schedule.iter().map(|&(v, s, _)| (v, s)).collect();
    assert_eq!(decoded, expected, "decoded line");
}

/// Verifies REQ-615: the Status frame sent with the command line high gets no reply
/// (criterion 2).
#[test]
fn req615_frame_needs_command_line() {
    let mut bus = sd_bus(VideoStandard::Ntsc);
    let end = send_bytes(&mut bus, &STATUS, 1_000, &[HOST_BIT; 5]);
    // The line stays high: this call is no edge.
    bus.set_command_line(false, end + RISE_DELAY);
    assert!(device_bytes(&bus).is_empty(), "no scheduled byte");
    assert_quiet(&bus, end + RISE_DELAY, "line high");
}

/// Verifies REQ-615: a partial frame, a new falling edge of the command line and a full
/// Status frame give one Status reply only, after the second rising edge (criterion 3).
#[test]
fn req615_frame_restart() {
    let mut bus = sd_bus(VideoStandard::Ntsc);
    let first = send_frame(&mut bus, &STATUS[..3], 1_000, HOST_BIT);
    assert!(device_bytes(&bus).is_empty(), "no reply to three bytes");
    let second = send_frame(&mut bus, &STATUS, first + 500, HOST_BIT);
    assert_eq!(values(&bus), STATUS_REPLY, "one Status reply");
    assert!(
        device_bytes(&bus).iter().all(|byte| byte.1 >= second),
        "after the second rising edge"
    );
}

/// Verifies REQ-615: Status frames at bit periods of 90 and 98 cycles get the reply; a
/// frame with byte 2 at 89 or 99 cycles gets no reply (criteria 4, 5).
#[test]
fn req615_baud_tolerance() {
    for bit in [90, 98] {
        let mut bus = sd_bus(VideoStandard::Ntsc);
        let _rise = send_frame(&mut bus, &STATUS, 1_000, bit);
        assert_eq!(values(&bus), STATUS_REPLY, "bit period {bit}");
    }
    // "Byte 2" is checked as the second byte and as byte index 2.
    for index in [1, 2] {
        for bit in [89, 99] {
            let mut bus = sd_bus(VideoStandard::Ntsc);
            let mut bits = [HOST_BIT; 5];
            bits[index] = bit;
            let rise = send_frame_with(&mut bus, &STATUS, 1_000, &bits, RISE_DELAY);
            assert_quiet(&bus, rise, &format!("byte {index} at {bit} cycles"));
        }
    }
}

/// Verifies REQ-616: the checksums of `31 52 01 00` (`$84`), `FF 02` (`$02`), `FF FF`
/// (`$FF`), 128 bytes of `$FF` (`$FF`), 128 bytes of `$00` and the empty frame (`$00`)
/// (criteria 1-6).
#[test]
fn req616_checksum_values() {
    assert_eq!(sio_checksum(&[0x31, 0x52, 0x01, 0x00]), 0x84, "1");
    assert_eq!(sio_checksum(&[0xFF, 0x02]), 0x02, "2");
    assert_eq!(sio_checksum(&[0xFF, 0xFF]), 0xFF, "3");
    assert_eq!(sio_checksum(&[0xFF; 128]), 0xFF, "4");
    assert_eq!(sio_checksum(&[0x00; 128]), 0x00, "5");
    assert_eq!(sio_checksum(&[]), 0x00, "6");
}

/// Verifies REQ-617: a frame for device `$32`, a frame with a bad checksum, a frame with a
/// framing error and a frame of four bytes give no byte, and a Status frame after each one
/// gets the full reply (criteria 1-5).
#[test]
fn req617_ignored_frames() {
    let mut bit_99 = [HOST_BIT; 5];
    bit_99[1] = 99;
    let cases: [(&str, &[u8], [u64; 5]); 4] = [
        ("device $32", &[0x32, 0x53, 0x00, 0x00, 0x85], [HOST_BIT; 5]),
        ("checksum", &[0x31, 0x53, 0x00, 0x00, 0x85], [HOST_BIT; 5]),
        ("framing", &STATUS, bit_99),
        ("four bytes", &STATUS[..4], [HOST_BIT; 5]),
    ];
    for (name, frame, bits) in cases {
        let mut bus = sd_bus(VideoStandard::Ntsc);
        let rise = send_frame_with(&mut bus, frame, 1_000, &bits, RISE_DELAY);
        assert_quiet(&bus, rise, name);
        let _rise = send_frame(&mut bus, &STATUS, rise + QUIET, HOST_BIT);
        assert_eq!(values(&bus), STATUS_REPLY, "{name}: Status after it");
    }
}

/// The 16 ms limit in machine cycles (REQ-618 criterion 2).
const fn limit_16ms(standard: VideoStandard) -> u64 {
    match standard {
        VideoStandard::Ntsc => 28_636,
        VideoStandard::Pal => 28_375,
    }
}

/// Sends `frame` with the command line low for 20,000 cycles after the last stop bit, and
/// gives the rising edge and the first device byte.
fn ack_after_long_low(bus: &mut SioBus, frame: &[u8]) -> (u64, (u8, u64, u32)) {
    let rise = send_frame_with(bus, frame, 1_000, &[HOST_BIT; 5], 20_000);
    let first = device_bytes(bus).first().copied().unwrap();
    (rise, first)
}

/// Verifies REQ-618: the start bit of the ACK byte `41` begins at or after the rising edge
/// of the command line, for NTSC and PAL (criteria 1, 4).
#[test]
fn req618_ack_after_deassert() {
    for standard in STANDARDS {
        let mut bus = sd_bus(standard);
        let (rise, ack) = ack_after_long_low(&mut bus, &STATUS);
        assert_eq!(ack.0, 0x41, "{standard:?}: ACK");
        assert!(
            ack.1 >= rise,
            "{standard:?}: ACK at {} before {rise}",
            ack.1
        );
    }
}

/// Verifies REQ-618: the start bit of the ACK byte begins no more than 28,636 (NTSC) or
/// 28,375 (PAL) machine cycles after the command line goes high (criteria 2, 4).
#[test]
fn req618_ack_within_16ms() {
    for standard in STANDARDS {
        let mut bus = sd_bus(standard);
        let (rise, ack) = ack_after_long_low(&mut bus, &STATUS);
        assert_eq!(ack.0, 0x41, "{standard:?}: ACK");
        assert!(
            ack.1 - rise <= limit_16ms(standard),
            "{standard:?}: ACK {} cycles after the edge",
            ack.1 - rise
        );
    }
}

/// Verifies REQ-618: the NAK byte `4E` for the frame `31 00 00 00 31` has the timing of
/// criteria 1 and 2, for NTSC and PAL (criteria 3, 4).
#[test]
fn req618_nak_timing() {
    for standard in STANDARDS {
        let mut bus = sd_bus(standard);
        let (rise, nak) = ack_after_long_low(&mut bus, &[0x31, 0x00, 0x00, 0x00, 0x31]);
        assert_eq!(values(&bus), [0x4E], "{standard:?}: NAK only");
        assert!(nak.1 >= rise, "{standard:?}: NAK after the edge");
        assert!(
            nak.1 - rise <= limit_16ms(standard),
            "{standard:?}: within 16 ms"
        );
    }
}

/// The SD Percom block of REQ-634.
const SD_PERCOM: [u8; 12] = [
    0x28, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x80, 0xFF, 0x00, 0x00, 0x00,
];

/// Verifies REQ-619: the reply bytes of Status, Read sector 1 (with and without an image),
/// Format and Read Percom block with the SD test image (criteria 1-3, 5, 6).
#[test]
fn req619_read_sequence() {
    let mut host = Host::ntsc(Some(sd_test_image()));
    assert_eq!(host.command(0x53, 0, 0), STATUS_REPLY, "1: Status");

    let mut read = vec![0x41, 0x43];
    read.extend_from_slice(&[0x01; 128]);
    read.push(0x80);
    assert_eq!(host.command(0x52, 1, 0), read, "2: Read sector 1");

    let mut empty = Host::ntsc(None);
    let mut error = vec![0x41, 0x45];
    error.extend_from_slice(&[0x00; 128]);
    error.push(0x00);
    assert_eq!(empty.command(0x52, 1, 0), error, "3: Read with no image");

    let mut percom = vec![0x41, 0x43];
    percom.extend_from_slice(&SD_PERCOM);
    percom.push(0xBA);
    assert_eq!(host.command(0x4E, 0, 0), percom, "6: Read Percom block");

    let mut format = vec![0x41, 0x43];
    format.extend_from_slice(&[0xFF; 128]);
    format.push(0xFF);
    assert_eq!(host.command(0x21, 0, 0), format, "5: Format");
}

/// Verifies REQ-619: Put sector 1 and Write Percom block: `41`, the data frame from the
/// computer, then `41` and `43` (criteria 4, 7). The line level, decoded cycle by cycle,
/// agrees with the schedule.
#[test]
fn req619_write_sequence() {
    let mut host = Host::ntsc(Some(sd_test_image()));
    assert_eq!(
        host.command(0x50, 1, 0),
        [0x41],
        "4: ACK before the data frame"
    );
    let mut host = Host::ntsc(Some(sd_test_image()));
    assert_eq!(
        host.write_with(0x50, 1, &[0x5A; 128], Some(0x2D)),
        [0x41, 0x41, 0x43],
        "4: Put sector 1"
    );
    let schedule = device_bytes(&host.bus);
    let last = schedule.last().map_or(0, |&byte| end_of(byte));
    let decoded = decode_line(&host.bus, 0, last + 1_000, 91);
    let expected: Vec<(u8, u64)> = schedule.iter().map(|&(v, s, _)| (v, s)).collect();
    assert_eq!(decoded, expected, "decoded line");

    assert_eq!(
        host.write_with(0x4F, 0, &SD_PERCOM, Some(0xBA)),
        [0x41, 0x41, 0x43],
        "7: Write Percom block"
    );
}

/// Verifies REQ-620: a Put data frame with a bad checksum gets `41`, `4E` and then no byte
/// for 100 ms; the sector and the change flag stay the same, and the next Status gives
/// `02 FF E0 00` (criteria 1-3).
#[test]
fn req620_data_checksum_nak() {
    let mut host = Host::ntsc(Some(sd_test_image()));
    let reply = host.write_with(0x50, 5, &[0x5A; 128], Some(0x2C));
    assert_eq!(reply, [0x41, 0x4E], "1: ACK, NAK");
    let nak = device_bytes(&host.bus).last().copied().unwrap();
    assert_quiet(&host.bus, end_of(nak), "1: after the NAK");
    assert_eq!(
        host.image().sector(5),
        Some(&[0x05; 128][..]),
        "2: sector 5"
    );
    assert!(!host.image().changed(), "2: no change");
    assert_eq!(host.status(), [0x02, 0xFF, 0xE0, 0x00], "3: Status");
}

/// The least delay from the end of an ACK to the Complete or Error byte (REQ-621).
const fn complete_min(standard: VideoStandard) -> u64 {
    match standard {
        VideoStandard::Ntsc => 448,
        VideoStandard::Pal => 444,
    }
}

/// Checks that device byte `index` of the schedule is `value` and starts at least the least
/// delay after the end of the byte before it.
fn check_result_delay(bus: &SioBus, index: usize, value: u8, standard: VideoStandard, name: &str) {
    let schedule = device_bytes(bus);
    let ack = schedule[index - 1];
    let result = schedule[index];
    assert_eq!(ack.0, 0x41, "{standard:?} {name}: ACK");
    assert_eq!(result.0, value, "{standard:?} {name}: result byte");
    assert!(
        result.1 >= end_of(ack) + complete_min(standard),
        "{standard:?} {name}: {} cycles after the ACK",
        result.1 - end_of(ack)
    );
}

/// Verifies REQ-621: the Complete byte `43` (Status, Read, Put) and the Error byte `45`
/// (empty drive) begin at least 448 (NTSC) or 444 (PAL) cycles after the ACK byte
/// (criteria 1-3).
#[test]
fn req621_complete_delay() {
    for standard in STANDARDS {
        let mut host = Host::new(Some(sd_test_image()), standard);
        assert_eq!(host.command(0x53, 0, 0).len(), 7, "{standard:?}: Status");
        check_result_delay(&host.bus, 1, 0x43, standard, "Status");
        assert_eq!(host.command(0x52, 1, 0).len(), 131, "{standard:?}: Read");
        check_result_delay(&host.bus, 1, 0x43, standard, "Read");
        assert_eq!(
            host.write(0x50, 1, &[0x5A; 128]).len(),
            3,
            "{standard:?}: Put"
        );
        check_result_delay(&host.bus, 2, 0x43, standard, "Put");
        let mut empty = Host::new(None, standard);
        assert_eq!(
            empty.command(0x52, 1, 0).len(),
            131,
            "{standard:?}: empty Read"
        );
        check_result_delay(&empty.bus, 1, 0x45, standard, "Read with no image");
    }
}

/// Checks the byte timing of REQ-622 for `standard`: `bit` cycles for each bit and `period`
/// cycles from byte to byte.
fn check_byte_timing(standard: VideoStandard, bit: u32, period: u64) {
    for (name, image) in [("SD", Some(sd_test_image())), ("no image", None)] {
        let mut host = Host::new(image, standard);
        let reply = host.command(0x52, 1, 0);
        assert_eq!(
            reply.get(..1),
            Some(&[0x41][..]),
            "{standard:?} {name}: ACK"
        );
        let schedule = device_bytes(&host.bus);
        assert_eq!(schedule.len(), 131, "{standard:?} {name}: bytes");
        assert!(
            schedule.iter().all(|byte| byte.2 == bit),
            "{standard:?} {name}: 1: bit period"
        );
        for pair in schedule[1..].windows(2) {
            assert_eq!(
                pair[1].1 - pair[0].1,
                period,
                "{standard:?} {name}: 2, 3: period"
            );
        }
        let last = schedule.last().map_or(0, |&byte| end_of(byte));
        let decoded = decode_line(&host.bus, 0, last + 1_000, u64::from(bit));
        assert_eq!(decoded.len(), 131, "{standard:?} {name}: decoded bytes");
    }
}

/// Verifies REQ-622: NTSC: a bit period of 91 cycles, 983 cycles from start bit to start bit
/// in the data frame, and the first data byte 983 cycles after the Complete byte
/// (criteria 1-4).
#[test]
fn req622_byte_timing_ntsc() {
    check_byte_timing(VideoStandard::Ntsc, 91, 983);
}

/// Verifies REQ-622: PAL: a bit period of 90 cycles, 974 cycles from start bit to start bit
/// in the data frame, and the first data byte 974 cycles after the Complete byte
/// (criteria 1-4).
#[test]
fn req622_byte_timing_pal() {
    check_byte_timing(VideoStandard::Pal, 90, 974);
}

/// Verifies REQ-624: the Status frame with ID `$31` gets the reply, and the IDs `$00`, `$30`,
/// `$32`, `$3F`, `$40`, `$4F`, `$50`, `$5F` and `$FF` get no byte for 100 ms (criteria 1, 2).
#[test]
fn req624_only_device_31() {
    let mut bus = sd_bus(VideoStandard::Ntsc);
    let _rise = send_frame(
        &mut bus,
        &with_checksum(&[0x31, 0x53, 0, 0]),
        1_000,
        HOST_BIT,
    );
    assert_eq!(values(&bus), STATUS_REPLY, "1: ID $31");
    for id in [0x00, 0x30, 0x32, 0x3F, 0x40, 0x4F, 0x50, 0x5F, 0xFF] {
        let mut bus = sd_bus(VideoStandard::Ntsc);
        let rise = send_frame(&mut bus, &with_checksum(&[id, 0x53, 0, 0]), 1_000, HOST_BIT);
        assert_quiet(&bus, rise, &format!("2: ID ${id:02X}"));
    }
}

/// A request for D1: with `command`, the sector `aux` and `data`.
const fn request(command: u8, aux: u16, data: SioData<'_>) -> SioRequest<'_> {
    let [aux1, aux2] = aux.to_le_bytes();
    SioRequest {
        frame: CommandFrame {
            device: 0x31,
            command,
            aux1,
            aux2,
        },
        data,
    }
}

/// Verifies REQ-623: `SioBus::transact`, the part of the SIOV shortcut in the bus: the OS
/// status bytes of the "SIOV shortcut" table of the design for Complete, NAK, Error and the
/// length mismatches, the received bytes, a Put that changes the image, and `NoDevice` for
/// another bus ID. The transaction schedules no device byte.
#[test]
fn req623_transact_status() {
    let mut bus = sd_bus(VideoStandard::Ntsc);
    let done = |status, data: &[u8]| SioOutcome::Done {
        status,
        data: data.to_vec(),
    };
    let read = |aux, len| request(0x52, aux, SioData::Receive(len));
    assert_eq!(
        bus.transact(&read(2, 128)),
        done(0x01, &[0x02; 128]),
        "read"
    );
    assert_eq!(
        bus.transact(&read(0, 128)),
        done(0x8B, &[]),
        "sector 0: NAK"
    );
    assert_eq!(
        bus.transact(&read(3, 100)),
        done(0x8F, &[0x03; 100]),
        "long frame"
    );
    assert_eq!(
        bus.transact(&read(3, 200)),
        done(0x8A, &[0x03; 128]),
        "short frame"
    );
    let put = request(0x50, 5, SioData::Send(&[0x5A; 128]));
    assert_eq!(bus.transact(&put), done(0x01, &[]), "put");
    assert_eq!(
        bus.drive().image().and_then(|image| image.sector(5)),
        Some(&[0x5A; 128][..]),
        "sector 5 after the put"
    );
    let short = request(0x50, 6, SioData::Send(&[0x5A; 100]));
    assert_eq!(bus.transact(&short), done(0x8A, &[]), "send: DBYT shorter");
    let long = request(0x50, 6, SioData::Send(&[0x5A; 200]));
    assert_eq!(bus.transact(&long), done(0x8B, &[]), "send: DBYT longer");
    let mut other = request(0x52, 1, SioData::Receive(128));
    other.frame.device = 0x32;
    assert_eq!(bus.transact(&other), SioOutcome::NoDevice, "bus ID $32");
    assert!(device_bytes(&bus).is_empty(), "no scheduled byte");

    let mut empty = SioBus::new(VideoStandard::Ntsc);
    assert_eq!(
        empty.transact(&read(1, 128)),
        done(0x90, &[0x00; 128]),
        "no image"
    );
    assert_eq!(
        empty.transact(&request(0x53, 0, SioData::Receive(4))),
        done(0x01, &[0x00, 0x7F, 0xE0, 0x00]),
        "status with no image"
    );
    empty.drive_mut().insert(DiskImage::blank(Geometry::SD));
    assert_eq!(
        empty.transact(&request(0x53, 0, SioData::None)),
        done(0x01, &[]),
        "status with no data frame"
    );
}

/// The end of the last scheduled device byte.
fn reply_end(bus: &SioBus) -> u64 {
    device_bytes(bus).into_iter().map(end_of).max().unwrap()
}

/// Checks that D1: takes part in a transfer in cycles of `from..to`, and not in cycle `to`,
/// where `last_end` is `Some(to)`.
fn assert_span(bus: &SioBus, from: u64, to: u64, name: &str) {
    for now in [from, from + 1, from.midpoint(to), to - 1] {
        let transfer = bus.disk_transfer(now, None);
        assert!(transfer.active, "{name}: active in cycle {now}");
        assert_eq!(transfer.last_end, None, "{name}: no end in cycle {now}");
    }
    let after = bus.disk_transfer(to, None);
    assert!(!after.active, "{name}: not active at the end {to}");
    assert_eq!(after.last_end, Some(to), "{name}: the end");
}

/// Verifies REQ-095: `SioBus::disk_transfer` gives the transfer of D1: from the first bit
/// of a command frame for D1: until the last bit of the reply of the drive, for a read, a
/// write and a NAK (criterion 1, `docs/design/media.md`, "Disk transfer").
#[test]
fn req095_transfer_span() {
    let mut bus = sd_bus(VideoStandard::Ntsc);
    let idle = bus.disk_transfer(500, None);
    assert!(!idle.active, "no transfer before the first frame");
    assert_eq!(idle.last_end, None, "no transfer since the cold reset");

    // Read sector 1: from the start bit of the first byte.
    let read = with_checksum(&[0x31, 0x52, 0x01, 0x00]);
    bus.set_command_line(true, 1_000);
    assert!(!bus.disk_transfer(1_000, None).active, "no byte yet");
    assert!(
        !bus.disk_transfer(1_001, Some(0x32)).active,
        "a frame for D2:"
    );
    assert!(
        bus.disk_transfer(1_001, Some(0x31)).active,
        "POKEY sends the first byte, $31"
    );
    let end = send_bytes(&mut bus, &read, 1_001, &[HOST_BIT; 5]);
    bus.set_command_line(false, end + RISE_DELAY);
    let read_end = reply_end(&bus);
    assert_eq!(values(&bus).len(), 2 + 128 + 1, "ACK, Complete, data frame");
    assert_span(&bus, 1_001 + 10 * HOST_BIT, read_end, "read");

    // Put sector 1: the drive waits for the data frame after its ACK.
    let put = with_checksum(&[0x31, 0x50, 0x01, 0x00]);
    let start = read_end + 10_000;
    assert_eq!(
        bus.disk_transfer(start, None).last_end,
        Some(read_end),
        "the end of the read"
    );
    let rise = send_frame(&mut bus, &put, start, HOST_BIT);
    let ack_end = reply_end(&bus);
    assert!(
        bus.disk_transfer(ack_end + 50_000, None).active,
        "the drive waits for the data frame"
    );
    let mut data = vec![0x5A; 128];
    data.push(sio_checksum(&data));
    let _end = send_bytes(&mut bus, &data, ack_end + 60_000, &[HOST_BIT; 129]);
    let put_end = reply_end(&bus);
    assert_eq!(values(&bus), [0x41, 0x41, 0x43], "the Put reply");
    assert_span(&bus, rise, put_end, "put");

    // A NAK: sector 0.
    let nak = with_checksum(&[0x31, 0x52, 0x00, 0x00]);
    let rise = send_frame(&mut bus, &nak, put_end + 10_000, HOST_BIT);
    assert_eq!(values(&bus), [0x4E], "NAK");
    assert_span(&bus, rise, reply_end(&bus), "NAK");
}

/// Verifies REQ-095: a bad frame for D1: ends at the rising edge of the command line, a
/// frame for another device is no transfer, a new frame cuts a reply, the SIOV shortcut has
/// no transfer, a removed drive takes no part, and a cold reset clears the end (criterion 1,
/// `docs/design/media.md`, "Disk transfer").
#[test]
fn req095_transfer_edges() {
    let mut bus = sd_bus(VideoStandard::Ntsc);
    let read = with_checksum(&[0x31, 0x52, 0x01, 0x00]);

    // A frame for D1: with a bad checksum ends at the rising edge of the command line.
    let start = 10_000;
    bus.set_command_line(true, start);
    let end = send_bytes(
        &mut bus,
        &[0x31, 0x52, 0x01, 0x00, 0x00],
        start + 1,
        &[HOST_BIT; 5],
    );
    assert!(bus.disk_transfer(end, None).active, "the bad frame");
    let rise = end + RISE_DELAY;
    bus.set_command_line(false, rise);
    assert!(device_bytes(&bus).is_empty(), "no reply to a bad frame");
    let after = bus.disk_transfer(rise, None);
    assert!(!after.active, "the bad frame ended");
    assert_eq!(after.last_end, Some(rise), "the end of the bad frame");

    // A frame for D2: is no transfer of D1:.
    let other = with_checksum(&[0x32, 0x53, 0x00, 0x00]);
    let start = rise + 10_000;
    bus.set_command_line(true, start);
    let end = send_bytes(&mut bus, &other, start + 1, &[HOST_BIT; 5]);
    assert!(!bus.disk_transfer(end, None).active, "a frame for D2:");
    bus.set_command_line(false, end + RISE_DELAY);
    let after = bus.disk_transfer(end + 50_000, None);
    assert!(!after.active, "no reply for D2:");
    assert_eq!(after.last_end, Some(rise), "the end of the bad frame");

    // A new command frame cuts the reply: the transfer ends at the falling edge.
    let rise = send_frame(&mut bus, &read, end + 100_000, HOST_BIT);
    let cut = rise.midpoint(reply_end(&bus));
    bus.set_command_line(true, cut);
    bus.set_command_line(false, cut + 2_000);
    assert_eq!(
        bus.disk_transfer(cut + 3_000, None).last_end,
        Some(cut),
        "the cut reply"
    );

    // The SIOV shortcut has no transfer on the bus.
    let outcome = bus.transact(&SioRequest {
        frame: CommandFrame {
            device: 0x31,
            command: 0x52,
            aux1: 1,
            aux2: 0,
        },
        data: SioData::Receive(128),
    });
    assert!(
        matches!(outcome, SioOutcome::Done { status: 0x01, .. }),
        "{outcome:?}"
    );
    let after = bus.disk_transfer(cut + 4_000, None);
    assert!(!after.active, "transact is no transfer");
    assert_eq!(after.last_end, Some(cut), "transact keeps the end");

    // An empty drive on the bus takes part, a removed drive does not.
    let mut empty = SioBus::new(VideoStandard::Ntsc);
    let rise = send_frame(&mut empty, &read, 1_000, HOST_BIT);
    assert_span(&empty, rise, reply_end(&empty), "empty drive");
    let mut removed = SioBus::new(VideoStandard::Ntsc);
    removed.set_drive_connected(false);
    removed.set_command_line(true, 1_000);
    assert!(
        !removed.disk_transfer(1_001, Some(0x31)).active,
        "a removed drive"
    );
    let _rise = send_frame(&mut removed, &read, 2_000, HOST_BIT);
    assert_eq!(
        removed.disk_transfer(100_000, None),
        DiskTransfer {
            active: false,
            last_end: None
        },
        "no transfer of a removed drive"
    );

    // A cold reset clears the end: the cycle count starts again.
    bus.reset();
    assert_eq!(
        bus.disk_transfer(cut + 5_000, None),
        DiskTransfer {
            active: false,
            last_end: None
        },
        "after the reset"
    );
}
