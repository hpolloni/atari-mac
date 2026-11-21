//! The SIO bus: command frame reception, reply scheduling and the SIO DATA IN line level
//! (`docs/design/media.md`, "SIO bus and disk drive", "Command frame reception", "Reply
//! scheduling" and "Timing").
//!
//! `SioBus` has the byte timing. [`DiskDrive`] has the command semantics. `SioBus` never
//! reads a clock: all times come from the `now` arguments and the cycles of the host bytes,
//! so the core stays deterministic (REQ-014).

use core::mem;
use std::collections::VecDeque;

use super::drive::{Accept, Completion, DiskDrive};
use crate::pokey::SerialByte;
use crate::timing::VideoStandard;

/// The bus ID of drive D1: (REQ-624).
const DEVICE_D1: u8 = 0x31;

/// The length of a command frame: device ID, command, AUX1, AUX2 and checksum (REQ-615).
const FRAME_LEN: usize = 5;

/// The bits of one serial byte: start bit, 8 data bits and stop bit (REQ-615 Notes).
const BITS_PER_BYTE: u64 = 10;

/// The shortest accepted host bit period: 94 cycles - 5 percent, rounded inward (REQ-615).
const HOST_BIT_MIN: u64 = 90;
/// The longest accepted host bit period: 94 cycles + 5 percent, rounded inward (REQ-615).
const HOST_BIT_MAX: u64 = 98;

/// Command acknowledge (HRM 9.1).
const ACK: u8 = 0x41;
/// Negative acknowledge (HRM 9.1).
const NAK: u8 = 0x4E;
/// Complete (HRM 9.1).
const COMPLETE: u8 = 0x43;
/// Error (HRM 9.1).
const ERROR: u8 = 0x45;

/// The OS status of a completed request (REQ-623).
const STATUS_COMPLETE: u8 = 0x01;
/// The OS status of a timeout (REQ-623).
const STATUS_TIMEOUT: u8 = 0x8A;
/// The OS status of a device NAK (REQ-623).
const STATUS_NAK: u8 = 0x8B;
/// The OS status of a checksum error (REQ-623).
const STATUS_CHECKSUM: u8 = 0x8F;
/// The OS status of a device Error (REQ-623).
const STATUS_ERROR: u8 = 0x90;

/// The delay from the rising edge of the command line to the ACK or NAK (Timing section,
/// project decision in the 0-16 ms window of HRM 9.1).
const ACK_DELAY_US: u32 = 1_000;
/// The delay from the end of the data frame to its ACK or NAK (Timing section, project
/// decision in the 850 µs-16 ms window of HRM 9.1).
const DATA_ACK_DELAY_US: u32 = 1_000;
/// The least time from the end of an ACK byte to the Complete or Error byte (HRM 9.1 step 5,
/// REQ-621).
const COMPLETE_MIN_US: u32 = 250;
/// The bit period of the drive: 51 µs, as the 1050 (HRM 10.15, REQ-622).
const BIT_PERIOD_US: u32 = 51;
/// The time from the start of one drive byte to the start of the next byte of the same frame:
/// 549 µs, as the 1050 (HRM 10.15, REQ-622).
const BYTE_PERIOD_US: u32 = 549;

/// The carry wrap-around checksum of SIO frames (REQ-616): the 8-bit sum of the bytes, with
/// each carry out of bit 7 added back into the sum.
#[must_use]
pub fn sio_checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0_u8, |sum, &byte| {
        let (value, carry) = sum.overflowing_add(byte);
        value + u8::from(carry)
    })
}

/// A command frame without its checksum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandFrame {
    /// The bus ID of the device.
    pub device: u8,
    /// The command code.
    pub command: u8,
    /// AUX1: the low byte of the sector number for the sector commands.
    pub aux1: u8,
    /// AUX2: the high byte of the sector number for the sector commands.
    pub aux2: u8,
}

/// The data frame of an SIOV request (REQ-623).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SioData<'a> {
    /// No data frame.
    None,
    /// The computer receives a data frame of this length.
    Receive(usize),
    /// The computer sends these bytes.
    Send(&'a [u8]),
}

/// A complete SIO request from the device control block (REQ-623).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SioRequest<'a> {
    /// The command frame.
    pub frame: CommandFrame,
    /// The direction and the data of the data frame.
    pub data: SioData<'a>,
}

/// The result of [`SioBus::transact`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SioOutcome {
    /// No emulated device has this ID. The OS routine must run.
    NoDevice,
    /// The OS status byte and, for a receive, the bytes for the buffer.
    Done {
        /// The OS status byte (the table of REQ-623).
        status: u8,
        /// The received bytes, at most the requested length.
        data: Vec<u8>,
    },
}

/// The cycle counts of the SIO timing for one video standard (Timing section).
#[derive(Debug, Clone, Copy, PartialEq)]
struct SioTiming {
    /// The CPU clock in Hz.
    clock_hz: f64,
    /// `ACK_DELAY` in machine cycles.
    ack_delay: u64,
    /// `DATA_ACK_DELAY` in machine cycles.
    data_ack_delay: u64,
    /// `COMPLETE_MIN` in machine cycles, rounded up.
    complete_min: u64,
    /// `BIT_PERIOD` in machine cycles.
    bit_period: u32,
    /// `BYTE_PERIOD` in machine cycles.
    byte_period: u64,
}

impl SioTiming {
    /// The timing for `standard`.
    fn new(standard: VideoStandard) -> Self {
        let clock_hz = standard.cpu_clock_hz();
        let cycles = |us| Self::to_cycles(clock_hz, us, false);
        Self {
            clock_hz,
            ack_delay: cycles(ACK_DELAY_US),
            data_ack_delay: cycles(DATA_ACK_DELAY_US),
            complete_min: Self::to_cycles(clock_hz, COMPLETE_MIN_US, true),
            bit_period: u32::try_from(cycles(BIT_PERIOD_US)).unwrap_or(u32::MAX),
            byte_period: cycles(BYTE_PERIOD_US),
        }
    }

    /// `us` µs in machine cycles: `round(us * clock / 1e6)`, or rounded up if `up`.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is at most 2^32 µs of a 2 MHz clock, positive and far below 2^64"
    )]
    fn to_cycles(clock_hz: f64, us: u32, up: bool) -> u64 {
        let exact = f64::from(us) * clock_hz / 1e6;
        let value = if up { exact.ceil() } else { exact.round() };
        value.max(0.0) as u64
    }

    /// `us` µs in machine cycles, rounded.
    fn cycles(&self, us: u32) -> u64 {
        Self::to_cycles(self.clock_hz, us, false)
    }
}

/// A byte that the device sends on SIO DATA IN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeviceByte {
    /// The 8 data bits.
    value: u8,
    /// The machine cycle where the start bit begins.
    start: u64,
    /// The cycles of each bit.
    bit_cycles: u32,
}

impl DeviceByte {
    /// The first machine cycle after the stop bit.
    fn end(self) -> u64 {
        self.start + BITS_PER_BYTE * u64::from(self.bit_cycles)
    }

    /// The line level in machine cycle `now` (`true` = mark). The caller checks that `now`
    /// is in the byte.
    fn level(self, now: u64) -> bool {
        let bit = (now - self.start) / u64::from(self.bit_cycles.max(1));
        match bit {
            0 => false,
            1..=8 => (self.value >> (bit - 1)) & 1 != 0,
            _ => true,
        }
    }
}

/// The protocol state of the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    variant_size_differences,
    reason = "the largest variant has 16 bytes, and the bus has one phase"
)]
enum BusPhase {
    /// No transaction.
    Idle,
    /// The command line is low. Bytes go into `frame`.
    Command {
        /// A byte had a framing error, or the frame has more than five bytes.
        bad: bool,
    },
    /// ACK sent for a write-type command. Waiting for `len + 1` bytes.
    DataIn {
        /// The accepted command frame.
        frame: CommandFrame,
        /// The length of the data frame.
        len: usize,
    },
    /// Bytes are scheduled in `out`. New host bytes are ignored.
    Reply,
}

/// The SIO bus with the disk drive D1: (REQ-615 to REQ-624).
#[derive(Debug, Clone)]
pub struct SioBus {
    /// Drive D1:.
    drive: DiskDrive,
    /// The cycle counts of the Timing section.
    timing: SioTiming,
    /// The command line level: `true` = low (asserted).
    command_low: bool,
    /// The protocol state.
    phase: BusPhase,
    /// The command frame bytes.
    frame: [u8; FRAME_LEN],
    /// The number of bytes in `frame`.
    frame_len: u8,
    /// The data frame from the computer.
    data_in: Vec<u8>,
    /// The scheduled device bytes of the current transaction, in start order.
    out: VecDeque<DeviceByte>,
}

impl SioBus {
    /// An idle bus with an empty drive D1:, with the timing of `standard`.
    #[must_use]
    pub fn new(standard: VideoStandard) -> Self {
        Self {
            drive: DiskDrive::default(),
            timing: SioTiming::new(standard),
            command_low: false,
            phase: BusPhase::Idle,
            frame: [0; FRAME_LEN],
            frame_len: 0,
            data_in: Vec::new(),
            out: VecDeque::new(),
        }
    }

    /// Drive D1:.
    #[must_use]
    pub const fn drive(&self) -> &DiskDrive {
        &self.drive
    }

    /// Drive D1:, for insert and eject.
    pub const fn drive_mut(&mut self) -> &mut DiskDrive {
        &mut self.drive
    }

    /// The PIA CB2 output changed. `low` = command line asserted. A falling edge starts a
    /// new command frame and stops the current transaction (HRM 9.1). A rising edge ends the
    /// command frame: a valid frame for D1: gets its reply.
    pub fn set_command_line(&mut self, low: bool, now: u64) {
        if low == self.command_low {
            return;
        }
        self.command_low = low;
        if low {
            self.frame_len = 0;
            self.data_in.clear();
            self.out.clear();
            self.phase = BusPhase::Command { bad: false };
        } else if let BusPhase::Command { bad } = self.phase {
            self.phase = BusPhase::Idle;
            if !bad {
                self.end_command_frame(now);
            }
        }
    }

    /// Checks the received command frame and schedules the reply (REQ-615, REQ-617,
    /// REQ-618). `now` is the cycle of the rising edge of the command line.
    fn end_command_frame(&mut self, now: u64) {
        let [device, command, aux1, aux2, checksum] = self.frame;
        if usize::from(self.frame_len) != FRAME_LEN
            || sio_checksum(&[device, command, aux1, aux2]) != checksum
            || device != DEVICE_D1
        {
            return;
        }
        let frame = CommandFrame {
            device,
            command,
            aux1,
            aux2,
        };
        let ack_start = now + self.timing.ack_delay;
        match self.drive.accept(frame) {
            Accept::Nak => {
                self.schedule(NAK, ack_start);
                self.phase = BusPhase::Reply;
            }
            Accept::Read => {
                let ack_end = self.schedule_ack(ack_start);
                let completion = self.drive.execute(frame, None);
                self.schedule_result(ack_end, completion);
            }
            Accept::Write { len } => {
                self.schedule(ACK, ack_start);
                self.data_in.clear();
                self.phase = BusPhase::DataIn { frame, len };
            }
        }
    }

    /// Schedules one device byte that starts at `start`.
    fn schedule(&mut self, value: u8, start: u64) {
        let byte = DeviceByte {
            value,
            start,
            bit_cycles: self.timing.bit_period,
        };
        let index = self.out.partition_point(|other| other.start <= start);
        self.out.insert(index, byte);
    }

    /// Schedules the ACK byte at `start`, and gives the first cycle after its stop bit.
    fn schedule_ack(&mut self, start: u64) -> u64 {
        self.schedule(ACK, start);
        start + BITS_PER_BYTE * u64::from(self.timing.bit_period)
    }

    /// Schedules the Complete or Error byte after the ACK that ends at `ack_end`, and the
    /// data frame with its checksum (REQ-619, REQ-621, REQ-622).
    fn schedule_result(&mut self, ack_end: u64, completion: Completion) {
        let busy = self.timing.cycles(completion.busy_us);
        let result_start = ack_end + busy.max(self.timing.complete_min);
        self.schedule(if completion.ok { COMPLETE } else { ERROR }, result_start);
        if let Some(data) = completion.data {
            let period = self.timing.byte_period;
            let mut start = result_start;
            for &byte in &data {
                start += period;
                self.schedule(byte, start);
            }
            self.schedule(sio_checksum(&data), start + period);
        }
        self.phase = BusPhase::Reply;
    }

    /// POKEY finished sending `byte` (from `Pokey::take_serial_byte`).
    pub fn host_byte(&mut self, byte: SerialByte) {
        match &mut self.phase {
            BusPhase::Command { bad } => {
                let length = byte.end_cycle.saturating_sub(byte.start_cycle);
                let accepted = BITS_PER_BYTE * HOST_BIT_MIN..=BITS_PER_BYTE * HOST_BIT_MAX;
                if !accepted.contains(&length) {
                    *bad = true;
                } else if let Some(slot) = self.frame.get_mut(usize::from(self.frame_len)) {
                    *slot = byte.value;
                    self.frame_len += 1;
                } else {
                    *bad = true;
                }
            }
            BusPhase::DataIn { frame, len } => {
                let (frame, len) = (*frame, *len);
                self.data_in.push(byte.value);
                if self.data_in.len() > len {
                    self.end_data_frame(frame, len, byte.end_cycle);
                }
            }
            BusPhase::Idle | BusPhase::Reply => {}
        }
    }

    /// The data frame of a write-type command is complete at cycle `end`: ACK and the result,
    /// or NAK for a checksum error (REQ-619, REQ-620).
    fn end_data_frame(&mut self, frame: CommandFrame, len: usize, end: u64) {
        let ack_start = end + self.timing.data_ack_delay;
        let data = mem::take(&mut self.data_in);
        let (payload, checksum) = data.split_at(len);
        if checksum.first().copied() != Some(sio_checksum(payload)) {
            self.drive.data_frame_error();
            self.schedule(NAK, ack_start);
            self.phase = BusPhase::Reply;
            return;
        }
        let ack_end = self.schedule_ack(ack_start);
        let completion = self.drive.execute(frame, Some(payload));
        self.schedule_result(ack_end, completion);
    }

    /// The level of the SIO DATA IN line in machine cycle `now` (`true` = mark). The bus
    /// makes the start bit, the 8 data bits (LSB first) and the stop bit of each scheduled
    /// device byte. The line is high when no byte is in progress.
    #[must_use]
    pub fn data_in_level(&self, now: u64) -> bool {
        let index = self.out.partition_point(|byte| byte.start <= now);
        match index.checked_sub(1).and_then(|last| self.out.get(last)) {
            Some(&byte) if now < byte.end() => byte.level(now),
            _ => true,
        }
    }

    /// The start cycle of the first scheduled device byte of the current transaction, for
    /// tests.
    #[must_use]
    pub fn next_device_byte_start(&self) -> Option<u64> {
        self.out.front().map(|byte| byte.start)
    }

    /// The scheduled device bytes of the current transaction as `(value, start,
    /// bit_cycles)`, for tests.
    #[must_use]
    pub fn scheduled_device_bytes(&self) -> Vec<(u8, u64, u32)> {
        self.out
            .iter()
            .map(|byte| (byte.value, byte.start, byte.bit_cycles))
            .collect()
    }

    /// A complete transaction at once, for the SIOV shortcut (REQ-623). It takes no emulated
    /// time and does not change the bus phase. The drive results map to the OS status bytes
    /// of the "SIOV shortcut" table of the design.
    pub fn transact(&mut self, request: &SioRequest<'_>) -> SioOutcome {
        let frame = request.frame;
        if frame.device != DEVICE_D1 {
            return SioOutcome::NoDevice;
        }
        let done = |status, data| SioOutcome::Done { status, data };
        match self.drive.accept(frame) {
            Accept::Nak => done(STATUS_NAK, Vec::new()),
            Accept::Read => {
                let completion = self.drive.execute(frame, None);
                let status = if completion.ok {
                    STATUS_COMPLETE
                } else {
                    STATUS_ERROR
                };
                let mut data = completion.data.unwrap_or_default();
                match request.data {
                    SioData::Receive(len) if data.len() > len => {
                        data.truncate(len);
                        done(STATUS_CHECKSUM, data)
                    }
                    SioData::Receive(len) if data.len() < len => done(STATUS_TIMEOUT, data),
                    SioData::Receive(_) => done(status, data),
                    SioData::None => done(status, Vec::new()),
                    // The computer waits for the ACK of a data frame that the device does not
                    // take: a project decision, the OS times out.
                    SioData::Send(_) => done(STATUS_TIMEOUT, Vec::new()),
                }
            }
            Accept::Write { len } => match request.data {
                SioData::Send(bytes) if bytes.len() < len => done(STATUS_TIMEOUT, Vec::new()),
                SioData::Send(bytes) if bytes.len() > len => {
                    // The drive reads the byte after the data as the checksum.
                    self.drive.data_frame_error();
                    done(STATUS_NAK, Vec::new())
                }
                SioData::Send(bytes) => {
                    let completion = self.drive.execute(frame, Some(bytes));
                    let status = if completion.ok {
                        STATUS_COMPLETE
                    } else {
                        STATUS_ERROR
                    };
                    done(status, Vec::new())
                }
                // The drive waits for a data frame that the computer does not send.
                SioData::None | SioData::Receive(_) => done(STATUS_TIMEOUT, Vec::new()),
            },
        }
    }

    /// Bus state to idle (cold reset). The inserted image does not change.
    pub fn reset(&mut self) {
        self.command_low = false;
        self.phase = BusPhase::Idle;
        self.frame_len = 0;
        self.data_in.clear();
        self.out.clear();
    }
}
