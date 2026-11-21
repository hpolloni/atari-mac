//! Test helpers of the SIO bus tests (`docs/design/media.md`, "Unit test method for the SIO
//! bus" and "Test images"): a host that drives `SioBus` directly, and the test images.

use crate::media::{DiskImage, Geometry, SioBus, sio_checksum};
use crate::pokey::SerialByte;
use crate::timing::VideoStandard;

/// The nominal host bit period: POKEY divisor 40 (REQ-615).
pub(super) const HOST_BIT: u64 = 94;

/// The default delay from the end of the last frame byte to the rising edge of the command
/// line ("Unit test method for the SIO bus").
pub(super) const RISE_DELAY: u64 = 1_600;

/// 100 ms at the NTSC clock: the window of "no reply" (REQ-615, REQ-617).
pub(super) const QUIET: u64 = 178_977;

/// The SD test image: 720 x 128, each byte of sector `n` holds `n & $FF` (REQ-619).
pub(super) fn sd_test_image() -> DiskImage {
    let mut image = DiskImage::blank(Geometry::SD);
    for n in 1..=720_u16 {
        let fill = n.to_le_bytes()[0];
        assert!(image.write_sector(n, &[fill; 128]), "sector {n}");
    }
    // The image is the input of the test, not a change.
    DiskImage::from_atr(&image.to_atr()).unwrap()
}

/// An ATR file with the header start `header` (bytes 0-6), header bytes 7-15 `extra`, and
/// `data_len` data bytes where data byte `i` holds `i & $FF` (REQ-625).
pub(super) fn atr_file(header: [u8; 7], extra: [u8; 9], data_len: usize) -> Vec<u8> {
    let mut file = header.to_vec();
    file.extend_from_slice(&extra);
    file.extend((0..data_len).map(|i| i.to_le_bytes()[0]));
    file
}

/// The SD pattern file of REQ-625 criterion 1.
pub(super) fn sd_file() -> Vec<u8> {
    atr_file([0x96, 0x02, 0x80, 0x16, 0x80, 0x00, 0x00], [0; 9], 92_160)
}

/// The ED pattern file of REQ-625 criterion 2.
pub(super) fn ed_file() -> Vec<u8> {
    atr_file([0x96, 0x02, 0x80, 0x20, 0x80, 0x00, 0x00], [0; 9], 133_120)
}

/// The DD pattern file of REQ-625 criterion 3.
pub(super) fn dd_file() -> Vec<u8> {
    atr_file([0x96, 0x02, 0xE8, 0x2C, 0x00, 0x01, 0x00], [0; 9], 183_936)
}

/// Loads `file`, which must be valid.
pub(super) fn load(file: &[u8]) -> DiskImage {
    DiskImage::from_atr(file).unwrap()
}

/// A host byte of `value` that starts at `start` with `bit` cycles for each bit.
pub(super) const fn host(value: u8, start: u64, bit: u64) -> SerialByte {
    SerialByte {
        value,
        start_cycle: start,
        end_cycle: start + 10 * bit,
    }
}

/// Sends `bytes` with no gap from cycle `start`, byte `i` with the bit period `bits[i]`, and
/// gives the end of the last stop bit. The command line does not change.
pub(super) fn send_bytes(bus: &mut SioBus, bytes: &[u8], start: u64, bits: &[u64]) -> u64 {
    let mut at = start;
    for (&value, &bit) in bytes.iter().zip(bits) {
        bus.host_byte(host(value, at, bit));
        at += 10 * bit;
    }
    at
}

/// `send_frame` of the design: lowers the command line at `t`, sends `bytes` from `t + 1`
/// with the bit periods `bits`, and raises the command line `delay` cycles after the last
/// stop bit. Gives the cycle of the rising edge.
pub(super) fn send_frame_with(
    bus: &mut SioBus,
    bytes: &[u8],
    t: u64,
    bits: &[u64],
    delay: u64,
) -> u64 {
    bus.set_command_line(true, t);
    let end = send_bytes(bus, bytes, t + 1, bits);
    bus.set_command_line(false, end + delay);
    end + delay
}

/// `send_frame` with all bytes at `bit_cycles` and the default delay.
pub(super) fn send_frame(bus: &mut SioBus, bytes: &[u8], t: u64, bit_cycles: u64) -> u64 {
    send_frame_with(bus, bytes, t, &[bit_cycles; 16], RISE_DELAY)
}

/// The four frame bytes and their checksum.
pub(super) fn with_checksum(bytes: &[u8]) -> Vec<u8> {
    let mut frame = bytes.to_vec();
    frame.push(sio_checksum(bytes));
    frame
}

/// `device_bytes` of the design: the scheduled device bytes `(value, start, bit_cycles)`.
pub(super) fn device_bytes(bus: &SioBus) -> Vec<(u8, u64, u32)> {
    bus.scheduled_device_bytes()
}

/// The values of the scheduled device bytes.
pub(super) fn values(bus: &SioBus) -> Vec<u8> {
    device_bytes(bus)
        .iter()
        .map(|&(value, _, _)| value)
        .collect()
}

/// The first cycle after the stop bit of a scheduled device byte.
pub(super) fn end_of((_, start, bit): (u8, u64, u32)) -> u64 {
    start + 10 * u64::from(bit)
}

/// Checks that the drive sends no byte in the `QUIET` cycles from `from`: no scheduled byte
/// starts in the window, and the line is high in each cycle of it.
pub(super) fn assert_quiet(bus: &SioBus, from: u64, name: &str) {
    for byte in device_bytes(bus) {
        assert!(
            byte.1 < from || byte.1 >= from + QUIET,
            "{name}: byte {byte:?} in the quiet window from {from}"
        );
        assert!(
            byte.1 >= from || end_of(byte) <= from,
            "{name}: byte {byte:?} still in progress at {from}"
        );
    }
    assert!(
        (from..from + QUIET).all(|now| bus.data_in_level(now)),
        "{name}: the line is low in the quiet window"
    );
}

/// Decodes the SIO DATA IN line cycle by cycle from `from` to `to`, as a receiver with the
/// bit period `bit` does: a falling edge starts a byte, each bit is sampled in its middle,
/// and the stop bit must be high. Checks also that the level is constant in each bit cell.
/// Gives `(value, start)` of each byte.
pub(super) fn decode_line(bus: &SioBus, from: u64, to: u64, bit: u64) -> Vec<(u8, u64)> {
    let mut bytes = Vec::new();
    let mut now = from;
    let mut previous = bus.data_in_level(now);
    while now < to {
        let level = bus.data_in_level(now);
        if previous && !level {
            let start = now;
            let mut value = 0_u8;
            for cell in 0..10 {
                let cell_start = start + cell * bit;
                let cell_level = bus.data_in_level(cell_start);
                assert!(
                    (cell_start..cell_start + bit).all(|c| bus.data_in_level(c) == cell_level),
                    "bit cell {cell} of the byte at {start} is not constant"
                );
                match cell {
                    0 => assert!(!cell_level, "start bit at {start}"),
                    9 => assert!(cell_level, "stop bit of the byte at {start}"),
                    _ => value |= u8::from(cell_level) << (cell - 1),
                }
                assert_eq!(
                    bus.data_in_level(cell_start + bit / 2),
                    cell_level,
                    "middle of cell {cell} at {start}"
                );
            }
            bytes.push((value, start));
            now = start + 10 * bit;
            previous = true;
            continue;
        }
        previous = level;
        now += 1;
    }
    bytes
}

/// A host that drives one `SioBus` with a running cycle counter.
pub(super) struct Host {
    /// The bus under test.
    pub(super) bus: SioBus,
    /// The next free cycle.
    pub(super) now: u64,
}

impl Host {
    /// A bus for `standard` with `image` in D1:.
    pub(super) fn new(image: Option<DiskImage>, standard: VideoStandard) -> Self {
        let mut bus = SioBus::new(standard);
        if let Some(image) = image {
            bus.drive_mut().insert(image);
        }
        Self { bus, now: 1_000 }
    }

    /// A bus for NTSC with `image` in D1:.
    pub(super) fn ntsc(image: Option<DiskImage>) -> Self {
        Self::new(image, VideoStandard::Ntsc)
    }

    /// Moves `now` after the last scheduled byte and 100 ms more.
    fn settle(&mut self) {
        let last = device_bytes(&self.bus)
            .into_iter()
            .map(end_of)
            .max()
            .unwrap_or(0);
        self.now = self.now.max(last) + QUIET;
    }

    /// Sends the command frame `bytes` (with its checksum) and gives the cycle of the rising
    /// edge of the command line.
    pub(super) fn frame(&mut self, bytes: &[u8]) -> u64 {
        self.settle();
        let rise = send_frame(&mut self.bus, bytes, self.now, HOST_BIT);
        self.now = rise;
        rise
    }

    /// Sends the D1: command `command` with AUX1 and AUX2, and gives all device bytes of the
    /// reply. A write-type command gets no data frame.
    pub(super) fn command(&mut self, command: u8, aux1: u8, aux2: u8) -> Vec<u8> {
        let frame = with_checksum(&[0x31, command, aux1, aux2]);
        let _rise = self.frame(&frame);
        values(&self.bus)
    }

    /// Sends the write-type D1: command `command` for `sector`, and after its ACK the data frame `data`
    /// with `checksum` (or the correct checksum). Gives all device bytes.
    pub(super) fn write_with(
        &mut self,
        command: u8,
        sector: u16,
        data: &[u8],
        checksum: Option<u8>,
    ) -> Vec<u8> {
        let [aux1, aux2] = sector.to_le_bytes();
        let frame = with_checksum(&[0x31, command, aux1, aux2]);
        let _rise = self.frame(&frame);
        let reply = device_bytes(&self.bus);
        if reply.first().map(|byte| byte.0) == Some(0x41) {
            let ack_end = reply.first().map_or(self.now, |&byte| end_of(byte));
            let mut bytes = data.to_vec();
            bytes.push(checksum.unwrap_or_else(|| sio_checksum(data)));
            let end = send_bytes(
                &mut self.bus,
                &bytes,
                ack_end + 500,
                &vec![HOST_BIT; bytes.len()],
            );
            self.now = end;
        }
        values(&self.bus)
    }

    /// `write_with` with the correct checksum.
    pub(super) fn write(&mut self, command: u8, sector: u16, data: &[u8]) -> Vec<u8> {
        self.write_with(command, sector, data, None)
    }

    /// Read sector `n`: the data frame, with the checks of the reply bytes `41 43 data cs`.
    pub(super) fn read_sector(&mut self, n: u16) -> Vec<u8> {
        let [aux1, aux2] = n.to_le_bytes();
        let reply = self.command(0x52, aux1, aux2);
        assert_eq!(
            reply.get(..2),
            Some(&[0x41, 0x43][..]),
            "read {n}: ACK, Complete"
        );
        let data = reply.get(2..reply.len() - 1).unwrap_or_default().to_vec();
        assert_eq!(
            reply.last().copied(),
            Some(sio_checksum(&data)),
            "read {n}: checksum"
        );
        data
    }

    /// The four bytes of the status frame, with the checks of `41 43` and the checksum.
    pub(super) fn status(&mut self) -> Vec<u8> {
        let reply = self.command(0x53, 0, 0);
        assert_eq!(reply.len(), 7, "status reply {reply:02X?}");
        assert_eq!(
            reply.get(..2),
            Some(&[0x41, 0x43][..]),
            "status: ACK, Complete"
        );
        let data = reply.get(2..6).unwrap_or_default().to_vec();
        assert_eq!(
            reply.get(6).copied(),
            Some(sio_checksum(&data)),
            "status checksum"
        );
        data
    }

    /// The inserted image.
    pub(super) fn image(&self) -> &DiskImage {
        self.bus.drive().image().unwrap()
    }
}
