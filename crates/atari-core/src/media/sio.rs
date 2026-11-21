//! The SIO bus: the line levels between POKEY and the devices (REQ-600 and later).
//!
//! Design: `docs/design/media.md`, "SIO bus and disk drive", and `docs/design/system.md`,
//! "Timing". The machine calls [`SioBus::data_in_level`] before each POKEY tick and gives
//! each byte that POKEY sent to [`SioBus::host_byte`] (OI-038).
//!
//! This is the part of the bus that SLICE-05 WP-1 needs: the device bytes on SIO DATA IN
//! and the command line level. The disk drive protocol (SLICE-05 WP-2) and the cassette deck
//! (SLICE-08 WP-2) schedule their bytes or levels through this type. Until then, no device
//! answers, and the bytes from the computer are ignored.

use std::collections::VecDeque;

use crate::pokey::SerialByte;
use crate::timing::VideoStandard;

/// The number of bit cells in one byte on the line: start bit, 8 data bits, stop bit.
const FRAME_BITS: u64 = 10;

/// The number of bytes from the computer that the bus keeps.
const HOST_BYTES: usize = 300;

/// A byte that a device sends on SIO DATA IN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeviceByte {
    /// The 8 data bits.
    value: u8,
    /// The cycle of the start bit edge.
    start: u64,
    /// The cycles of one bit cell.
    bit_cycles: u32,
}

impl DeviceByte {
    /// The first cycle after the stop bit.
    const fn end(&self) -> u64 {
        self.start + FRAME_BITS * self.bit_cycles as u64
    }

    /// The line level in cycle `now`, which is in `start..end()`.
    const fn level(&self, now: u64) -> bool {
        match (now - self.start) / self.bit_cycles as u64 {
            0 => false,
            bit @ 1..=8 => (self.value >> (bit - 1)) & 1 != 0,
            _ => true,
        }
    }
}

/// The SIO bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SioBus {
    /// The command line is asserted (low).
    command_low: bool,
    /// The bytes from the computer since the command line went low (at most
    /// `HOST_BYTES`).
    host: Vec<u8>,
    /// The scheduled device bytes, in the order of their start cycles.
    out: VecDeque<DeviceByte>,
}

impl SioBus {
    /// The idle bus: no device byte and the command line high.
    #[must_use]
    pub const fn new(_standard: VideoStandard) -> Self {
        Self {
            command_low: false,
            host: Vec::new(),
            out: VecDeque::new(),
        }
    }

    /// The PIA CB2 output changed in cycle `now`. `low` = command line asserted.
    /// The bytes that ended before `now` are removed.
    pub fn set_command_line(&mut self, low: bool, now: u64) {
        if low && !self.command_low {
            self.host.clear();
        }
        self.command_low = low;
        while self.out.front().is_some_and(|byte| byte.end() <= now) {
            let _done = self.out.pop_front();
        }
    }

    /// The command line is asserted (low).
    #[must_use]
    pub const fn command_line_low(&self) -> bool {
        self.command_low
    }

    /// POKEY finished sending `byte` (from `Pokey::take_serial_byte`). No device answers
    /// yet: the bus only keeps the byte values (see `host_bytes`).
    pub fn host_byte(&mut self, byte: SerialByte) {
        if self.host.len() < HOST_BYTES {
            self.host.push(byte.value);
        }
    }

    /// The values of the bytes from the computer since the command line last went low (the
    /// first 300), for tests.
    #[must_use]
    pub fn host_bytes(&self) -> &[u8] {
        &self.host
    }

    /// The level of the SIO DATA IN line in cycle `now` (`true` = mark): the start bit, the
    /// 8 data bits (bit 0 first) and the stop bit of each scheduled device byte. The line is
    /// high when no byte is in progress.
    #[must_use]
    pub fn data_in_level(&self, now: u64) -> bool {
        if self.out.is_empty() {
            return true;
        }
        let next = self.out.partition_point(|byte| byte.end() <= now);
        match self.out.get(next) {
            Some(byte) if byte.start <= now => byte.level(now),
            _ => true,
        }
    }

    /// Schedules a device byte on SIO DATA IN: its start bit begins in cycle `start`, and
    /// each bit cell has `bit_cycles` cycles. The start must not be before the end of the
    /// last scheduled byte. `set_command_line` removes the bytes that ended.
    ///
    /// # Panics
    ///
    /// If `bit_cycles` is 0 or the byte overlaps the last scheduled byte.
    pub fn send_device_byte(&mut self, value: u8, start: u64, bit_cycles: u32) {
        assert!(bit_cycles > 0, "a bit cell needs at least one cycle");
        if let Some(last) = self.out.back() {
            assert!(start >= last.end(), "device bytes must not overlap");
        }
        self.out.push_back(DeviceByte {
            value,
            start,
            bit_cycles,
        });
    }

    /// The start cycle of the first scheduled device byte, for tests.
    #[must_use]
    pub fn next_device_byte_start(&self) -> Option<u64> {
        self.out.front().map(|byte| byte.start)
    }

    /// The scheduled device bytes as `(value, start, bit_cycles)`, for tests.
    #[must_use]
    pub fn scheduled_device_bytes(&self) -> Vec<(u8, u64, u32)> {
        self.out
            .iter()
            .map(|byte| (byte.value, byte.start, byte.bit_cycles))
            .collect()
    }

    /// Bus state to idle (cold reset).
    pub fn reset(&mut self) {
        self.command_low = false;
        self.host.clear();
        self.out.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::SioBus;
    use crate::timing::VideoStandard;

    /// Verifies REQ-526: the bus makes the frame of each device byte on SIO DATA IN, which
    /// POKEY receives through `set_serial_in` (the machine contract of OI-038).
    #[test]
    fn device_bytes_on_data_in() {
        let mut bus = SioBus::new(VideoStandard::Ntsc);
        assert!(bus.data_in_level(0), "idle");
        bus.send_device_byte(0x43, 100, 94);
        bus.send_device_byte(0xA5, 1040, 94);
        let mut levels = Vec::new();
        for now in 0..2100 {
            levels.push(bus.data_in_level(now));
        }
        for (value, start) in [(0x43_u8, 100_usize), (0xA5, 1040)] {
            for bit in 0..10 {
                let want = match bit {
                    0 => false,
                    9 => true,
                    n => (value >> (n - 1)) & 1 != 0,
                };
                let cells = &levels[start + 94 * bit..start + 94 * (bit + 1)];
                assert!(cells.iter().all(|l| *l == want), "{value:02X} bit {bit}");
            }
        }
        assert!(levels[..100].iter().all(|l| *l), "idle before");
        assert!(levels[2000..].iter().all(|l| *l), "idle after");
        assert_eq!(bus.next_device_byte_start(), Some(100), "first byte");
        assert_eq!(
            bus.scheduled_device_bytes(),
            vec![(0x43, 100, 94), (0xA5, 1040, 94)],
            "schedule"
        );
        // A command line edge removes the bytes that ended.
        bus.set_command_line(true, 1500);
        assert_eq!(bus.next_device_byte_start(), Some(1040), "after the edge");
        assert!(bus.host_bytes().is_empty(), "no host bytes");
        bus.reset();
        assert_eq!(bus.scheduled_device_bytes(), Vec::new(), "reset");
    }
}
