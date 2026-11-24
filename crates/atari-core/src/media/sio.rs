//! The SIO bus: command frame reception, reply scheduling and the SIO DATA IN line level
//! (`docs/design/media.md`, "SIO bus and disk drive", "Command frame reception", "Reply
//! scheduling" and "Timing").
//!
//! `SioBus` has the byte timing. [`DiskDrive`] has the command semantics. `SioBus` never
//! reads a clock: all times come from the `now` arguments and the cycles of the host bytes,
//! so the core stays deterministic (REQ-014).
//!
//! The machine calls [`SioBus::data_in_level`] before each POKEY tick, gives each byte that
//! POKEY sent to [`SioBus::host_byte`] and each change of PIA CB2 to
//! [`SioBus::set_command_line`] (OI-038, `docs/design/system.md`, "Timing"). Other devices
//! (the tests) put their bytes on SIO DATA IN with [`SioBus::send_device_byte`]. The bus
//! owns the cassette deck ([`TapeDeck`]), whose motor follows PIA CA2
//! ([`SioBus::set_motor_line`]). The line is the wired AND of all device outputs.

use core::mem;
use std::collections::VecDeque;

use super::deck::TapeDeck;
use super::drive::{Accept, Completion, DiskDrive};
use crate::events::{self, record};
use crate::pokey::SerialByte;
use crate::state::{Reader, State, StateError, TIME_LIMIT, Writer, impl_state};
use crate::timing::VideoStandard;

/// The number of bytes from the computer that the bus keeps for `host_bytes`.
const HOST_BYTES: usize = 300;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

impl_state!(CommandFrame {
    device,
    command,
    aux1,
    aux2,
});

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

/// The part of D1: in the transfers on the SIO bus (REQ-095, `docs/design/media.md`, "Disk
/// transfer").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskTransfer {
    /// D1: takes part in a transfer in the cycle of the query.
    pub active: bool,
    /// The first cycle after the end of the last transfer that ended at or before the cycle
    /// of the query, or `None` (no transfer since the cold reset).
    pub last_end: Option<u64>,
}

/// The cycle counts of the SIO timing for one video standard (Timing section).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SioTiming {
    /// The video standard, for the CPU clock.
    standard: VideoStandard,
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
            standard,
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
        Self::to_cycles(self.standard.cpu_clock_hz(), us, false)
    }
}

/// A byte that a device sends on SIO DATA IN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct DeviceByte {
    /// The 8 data bits.
    value: u8,
    /// The machine cycle where the start bit begins.
    start: u64,
    /// The cycles of each bit.
    bit_cycles: u32,
}

impl_state!(DeviceByte {
    value,
    start,
    bit_cycles,
});

/// The level of a queue of device bytes in start order in machine cycle `now` (`true` =
/// mark): the level of the last byte that started, while it lasts, else mark.
fn queue_level(queue: &VecDeque<DeviceByte>, now: u64) -> bool {
    if queue.is_empty() {
        return true;
    }
    let index = queue.partition_point(|byte| byte.start <= now);
    match index.checked_sub(1).and_then(|last| queue.get(last)) {
        Some(&byte) if now < byte.end() => byte.level(now),
        _ => true,
    }
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

// An enum with data: the index of the variant as `u64`, then the data of the variant.
impl State for BusPhase {
    fn save(&self, w: &mut Writer) {
        match *self {
            Self::Idle => 0_usize.save(w),
            Self::Command { bad } => {
                1_usize.save(w);
                bad.save(w);
            }
            Self::DataIn { frame, len } => {
                2_usize.save(w);
                frame.save(w);
                len.save(w);
            }
            Self::Reply => 3_usize.save(w),
        }
    }

    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        let mut index = 0_usize;
        index.load(r)?;
        *self = match index {
            0 => Self::Idle,
            1 => {
                let mut bad = false;
                bad.load(r)?;
                Self::Command { bad }
            }
            2 => {
                let mut frame = CommandFrame::default();
                let mut len = 0_usize;
                frame.load(r)?;
                len.load(r)?;
                Self::DataIn { frame, len }
            }
            3 => Self::Reply,
            _ => return Err(StateError::Invalid),
        };
        Ok(())
    }
}

/// The SIO bus with the disk drive D1: (REQ-615 to REQ-624).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SioBus {
    /// Drive D1:.
    drive: DiskDrive,
    /// Drive D1: is on the bus. With `false`, no device answers a frame for D1:.
    drive_connected: bool,
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
    /// The values of the bytes from the computer since the command line last went low (at
    /// most `HOST_BYTES`), for `host_bytes`.
    host: Vec<u8>,
    /// The device bytes of `send_device_byte`, in start order.
    external: VecDeque<DeviceByte>,
    /// The cassette deck (REQ-641, REQ-642).
    deck: TapeDeck,
    /// The end of the last D1: transfer, or of the scheduled reply: the first cycle after
    /// its last bit (REQ-095, "Disk transfer").
    transfer_end: Option<u64>,
    /// The frame count for the `frame` key of the event log records (REQ-084). The machine
    /// sets it; it is not in the save state.
    log_frame: u64,
}

// The timing comes from the video standard of the configuration.
impl_state!(SioBus {
    drive,
    drive_connected,
    command_low,
    phase,
    frame,
    frame_len,
    data_in,
    out,
    host,
    external,
    deck,
    transfer_end,
} skip {
    timing,
    // The frame count of the event log: the machine sets it after a load.
    log_frame
});

impl SioBus {
    /// `true` if a loaded state has its cycle times below [`TIME_LIMIT`] (REQ-061).
    pub(crate) fn loaded_is_valid(&self) -> bool {
        self.out
            .iter()
            .chain(&self.external)
            .all(|byte| byte.start < TIME_LIMIT)
            && self.transfer_end.is_none_or(|end| end < TIME_LIMIT)
            && self.deck.loaded_is_valid()
    }

    /// An idle bus with an empty drive D1:, with the timing of `standard`.
    #[must_use]
    pub fn new(standard: VideoStandard) -> Self {
        Self {
            drive: DiskDrive::default(),
            drive_connected: true,
            timing: SioTiming::new(standard),
            command_low: false,
            phase: BusPhase::Idle,
            frame: [0; FRAME_LEN],
            frame_len: 0,
            data_in: Vec::new(),
            out: VecDeque::new(),
            host: Vec::new(),
            external: VecDeque::new(),
            deck: TapeDeck::new(standard),
            transfer_end: None,
            log_frame: 0,
        }
    }

    /// Sets the frame count for the `frame` key of the event log records (REQ-084).
    pub(crate) const fn set_log_frame(&mut self, frame: u64) {
        self.log_frame = frame;
    }

    /// The cassette deck.
    #[must_use]
    pub const fn tape_deck(&self) -> &TapeDeck {
        &self.deck
    }

    /// The cassette deck, for insert and eject.
    pub const fn tape_deck_mut(&mut self) -> &mut TapeDeck {
        &mut self.deck
    }

    /// The PIA CA2 output changed at the end of cycle `now`. `low` = motor on (REQ-641).
    pub const fn set_motor_line(&mut self, low: bool, now: u64) {
        self.deck.set_motor_line(low, now);
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

    /// Connects drive D1: to the bus or removes it (`docs/design/media.md`, "Machine
    /// configuration"). A new bus has the drive connected. A removed drive answers no
    /// frame, and `transact` gives `NoDevice` for D1:.
    pub const fn set_drive_connected(&mut self, on: bool) {
        self.drive_connected = on;
    }

    /// Drive D1: is on the bus.
    #[must_use]
    pub const fn drive_connected(&self) -> bool {
        self.drive_connected
    }

    /// The PIA CB2 output changed. `low` = command line asserted. A falling edge starts a
    /// new command frame and stops the current transaction (HRM 9.1). A rising edge ends the
    /// command frame: a valid frame for D1: gets its reply. Each call removes the bytes of
    /// `send_device_byte` that ended before `now`.
    pub fn set_command_line(&mut self, low: bool, now: u64) {
        while self.external.front().is_some_and(|byte| byte.end() <= now) {
            let _done = self.external.pop_front();
        }
        if low == self.command_low {
            return;
        }
        self.command_low = low;
        if low {
            // A new frame stops the transfer of D1: (REQ-095).
            if self.transfer_end.is_some_and(|end| end > now)
                || matches!(self.phase, BusPhase::DataIn { .. })
                || self.d1_command_frame()
            {
                self.transfer_end = Some(now);
            }
            self.host.clear();
            self.frame_len = 0;
            self.data_in.clear();
            self.out.clear();
            self.phase = BusPhase::Command { bad: false };
        } else if let BusPhase::Command { bad } = self.phase {
            let d1_frame = self.d1_command_frame();
            self.phase = BusPhase::Idle;
            if bad || usize::from(self.frame_len) != FRAME_LEN {
                record!(
                    info,
                    events::SIO,
                    self.log_frame,
                    now,
                    "command frame not valid: {} bytes",
                    self.frame_len
                );
            } else {
                let [device, command, aux1, aux2, checksum] = self.frame;
                let ok = sio_checksum(&[device, command, aux1, aux2]) == checksum;
                record!(
                    info,
                    events::SIO,
                    self.log_frame,
                    now,
                    "command device ${device:02X} command ${command:02X} aux1 ${aux1:02X} aux2 ${aux2:02X} checksum {}",
                    if ok { "ok" } else { "bad" }
                );
            }
            if !bad {
                self.end_command_frame(now);
            }
            // A frame for D1: with no reply ends its transfer here (REQ-095).
            if d1_frame && self.phase == BusPhase::Idle {
                self.transfer_end = Some(now);
            }
        }
    }

    /// D1: is on the bus, the phase is `Command`, and the first byte of the frame is the ID
    /// of D1:.
    const fn d1_command_frame(&self) -> bool {
        self.drive_connected
            && matches!(self.phase, BusPhase::Command { .. })
            && self.frame_len > 0
            && self.frame[0] == DEVICE_D1
    }

    /// The part of D1: in the transfers on the bus in machine cycle `now` (REQ-095,
    /// `docs/design/media.md`, "Disk transfer"). `sending` is the data byte of the frame
    /// that the POKEY shift register sends in `now`. Read-only.
    #[must_use]
    pub fn disk_transfer(&self, now: u64, sending: Option<u8>) -> DiskTransfer {
        let active = self.drive_connected
            && match self.phase {
                BusPhase::Command { .. } => {
                    self.d1_command_frame() || (self.frame_len == 0 && sending == Some(DEVICE_D1))
                }
                BusPhase::DataIn { .. } => true,
                BusPhase::Reply => self.transfer_end.is_some_and(|end| now < end),
                BusPhase::Idle => false,
            };
        DiskTransfer {
            active,
            last_end: self.transfer_end.filter(|&end| end <= now),
        }
    }

    /// Checks the received command frame and schedules the reply (REQ-615, REQ-617,
    /// REQ-618). `now` is the cycle of the rising edge of the command line.
    fn end_command_frame(&mut self, now: u64) {
        let [device, command, aux1, aux2, checksum] = self.frame;
        if usize::from(self.frame_len) != FRAME_LEN
            || sio_checksum(&[device, command, aux1, aux2]) != checksum
            || device != DEVICE_D1
            || !self.drive_connected
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
                self.reply_record("NAK", now, ack_start);
                self.schedule(NAK, ack_start);
                self.phase = BusPhase::Reply;
            }
            Accept::Read => {
                self.reply_record("ACK", now, ack_start);
                let ack_end = self.schedule_ack(ack_start);
                let completion = self.drive.execute(frame, None);
                self.schedule_result(ack_end, completion, now);
            }
            Accept::Write { len } => {
                self.reply_record("ACK", now, ack_start);
                self.schedule(ACK, ack_start);
                self.data_in.clear();
                self.phase = BusPhase::DataIn { frame, len };
            }
        }
    }

    /// Sends the event log record of an ACK or a NAK that starts at `start`, decided in
    /// cycle `now` (REQ-084).
    fn reply_record(&self, reply: &str, now: u64, start: u64) {
        record!(
            info,
            events::SIO,
            self.log_frame,
            now,
            "reply {reply} at cycle {start}"
        );
    }

    /// Schedules one device byte that starts at `start`.
    fn schedule(&mut self, value: u8, start: u64) {
        let byte = DeviceByte {
            value,
            start,
            bit_cycles: self.timing.bit_period,
        };
        let byte_end = byte.end();
        let index = self.out.partition_point(|other| other.start <= start);
        self.out.insert(index, byte);
        self.transfer_end = Some(self.transfer_end.map_or(byte_end, |end| end.max(byte_end)));
    }

    /// Schedules the ACK byte at `start`, and gives the first cycle after its stop bit.
    fn schedule_ack(&mut self, start: u64) -> u64 {
        self.schedule(ACK, start);
        start + BITS_PER_BYTE * u64::from(self.timing.bit_period)
    }

    /// Schedules the Complete or Error byte after the ACK that ends at `ack_end`, and the
    /// data frame with its checksum (REQ-619, REQ-621, REQ-622). `now` is the cycle of the
    /// decision, for the event log (REQ-084).
    fn schedule_result(&mut self, ack_end: u64, completion: Completion, now: u64) {
        let busy = self.timing.cycles(completion.busy_us);
        let result_start = ack_end + busy.max(self.timing.complete_min);
        record!(
            info,
            events::SIO,
            self.log_frame,
            now,
            "reply {} at cycle {result_start}, data frame {} bytes",
            if completion.ok { "COMPLETE" } else { "ERROR" },
            completion.data.as_ref().map_or(0, Vec::len)
        );
        self.schedule(if completion.ok { COMPLETE } else { ERROR }, result_start);
        if let Some(data) = completion.data {
            let period = self.timing.byte_period;
            let mut start = result_start;
            for &byte in &data {
                start += period;
                self.received_record(byte, now, start);
                self.schedule(byte, start);
            }
            let checksum = sio_checksum(&data);
            self.received_record(checksum, now, start + period);
            self.schedule(checksum, start + period);
        }
        self.phase = BusPhase::Reply;
    }

    /// Sends the event log record of a data frame byte of a device that starts at `start`,
    /// scheduled in cycle `now` (REQ-084).
    fn received_record(&self, byte: u8, now: u64, start: u64) {
        record!(
            debug,
            events::SIO,
            self.log_frame,
            now,
            "received ${byte:02X} at cycle {start}"
        );
    }

    /// The command line is asserted (low).
    #[must_use]
    pub const fn command_line_low(&self) -> bool {
        self.command_low
    }

    /// POKEY finished sending `byte` (from `Pokey::take_serial_byte`).
    pub fn host_byte(&mut self, byte: SerialByte) {
        record!(
            debug,
            events::SIO,
            self.log_frame,
            byte.end_cycle,
            "sent ${:02X}",
            byte.value
        );
        if self.host.len() < HOST_BYTES {
            self.host.push(byte.value);
        }
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
            self.reply_record("NAK", end, ack_start);
            self.schedule(NAK, ack_start);
            self.phase = BusPhase::Reply;
            return;
        }
        self.reply_record("ACK", end, ack_start);
        let ack_end = self.schedule_ack(ack_start);
        let completion = self.drive.execute(frame, Some(payload));
        self.schedule_result(ack_end, completion, end);
    }

    /// The level of the SIO DATA IN line in machine cycle `now` (`true` = mark). The bus
    /// makes the start bit, the 8 data bits (LSB first) and the stop bit of each scheduled
    /// device byte. The line is high when no byte is in progress.
    /// The bytes of `send_device_byte` and the cassette deck are on the same line: a space
    /// of any device gives a space (wired AND, REQ-642 criterion 6).
    #[must_use]
    pub fn data_in_level(&self, now: u64) -> bool {
        queue_level(&self.out, now) && queue_level(&self.external, now) && self.deck.level(now)
    }

    /// Schedules a byte on SIO DATA IN for a device that is not D1: (the tests): its start
    /// bit begins in cycle `start`, and each bit has `bit_cycles` cycles. The start must not be before the end of the last byte of
    /// `send_device_byte`. Each command line change removes the bytes that ended. The drive
    /// protocol does not change these bytes.
    ///
    /// # Panics
    ///
    /// If `bit_cycles` is 0 or the byte overlaps the last byte of `send_device_byte`.
    pub fn send_device_byte(&mut self, value: u8, start: u64, bit_cycles: u32) {
        assert!(bit_cycles > 0, "a bit cell needs at least one cycle");
        if let Some(last) = self.external.back() {
            assert!(start >= last.end(), "device bytes must not overlap");
        }
        self.external.push_back(DeviceByte {
            value,
            start,
            bit_cycles,
        });
    }

    /// The values of the bytes from the computer since the command line last went low (the
    /// first 300), for tests.
    #[must_use]
    pub fn host_bytes(&self) -> &[u8] {
        &self.host
    }

    /// The start cycle of the first scheduled device byte (of the current transaction or of
    /// `send_device_byte`), for tests.
    #[must_use]
    pub fn next_device_byte_start(&self) -> Option<u64> {
        let first = |queue: &VecDeque<DeviceByte>| queue.front().map(|byte| byte.start);
        match (first(&self.out), first(&self.external)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// The scheduled device bytes (of the current transaction and of `send_device_byte`)
    /// as `(value, start, bit_cycles)` in start order, for tests.
    #[must_use]
    pub fn scheduled_device_bytes(&self) -> Vec<(u8, u64, u32)> {
        let mut bytes: Vec<(u8, u64, u32)> = self
            .out
            .iter()
            .chain(&self.external)
            .map(|byte| (byte.value, byte.start, byte.bit_cycles))
            .collect();
        bytes.sort_by_key(|&(_, start, _)| start);
        bytes
    }

    /// A complete transaction at once, for the SIOV shortcut (REQ-623). It takes no emulated
    /// time and does not change the bus phase. The drive results map to the OS status bytes
    /// of the "SIOV shortcut" table of the design.
    pub fn transact(&mut self, request: &SioRequest<'_>) -> SioOutcome {
        let frame = request.frame;
        if frame.device != DEVICE_D1 || !self.drive_connected {
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

    /// Bus state to idle (cold reset). The inserted image and the cassette deck do not
    /// change.
    pub fn reset(&mut self) {
        self.command_low = false;
        self.phase = BusPhase::Idle;
        self.frame_len = 0;
        self.data_in.clear();
        self.out.clear();
        self.host.clear();
        self.external.clear();
        self.transfer_end = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceByte, SioBus};
    use crate::media::deck::TapeDeck;
    use crate::state::TIME_LIMIT;
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

    /// `loaded_is_valid` of a new NTSC bus after `change`.
    fn load_valid_after(change: impl FnOnce(&mut SioBus)) -> bool {
        let mut bus = SioBus::new(VideoStandard::Ntsc);
        change(&mut bus);
        bus.loaded_is_valid()
    }

    /// A device byte that starts on cycle `start`.
    const fn byte_at(start: u64) -> DeviceByte {
        DeviceByte {
            value: 0,
            start,
            bit_cycles: 94,
        }
    }

    /// Verifies REQ-061: the start of each scheduled device byte of a loaded bus must be
    /// below `TIME_LIMIT`.
    #[test]
    fn sio_load_check_device_bytes() {
        assert!(load_valid_after(|_| {}), "new bus");
        assert!(
            load_valid_after(|b| b.out.push_back(byte_at(TIME_LIMIT - 1))),
            "transaction byte below the limit"
        );
        assert!(
            !load_valid_after(|b| b.out.push_back(byte_at(TIME_LIMIT))),
            "transaction byte at the limit"
        );
        assert!(
            load_valid_after(|b| b.external.push_back(byte_at(TIME_LIMIT - 1))),
            "external byte below the limit"
        );
        assert!(
            !load_valid_after(|b| b.external.push_back(byte_at(TIME_LIMIT))),
            "external byte at the limit"
        );
    }

    /// Verifies REQ-061: the transfer end of a loaded bus must be below `TIME_LIMIT`, and
    /// its tape deck must be valid.
    #[test]
    fn sio_load_check_transfer_end_and_deck() {
        assert!(
            load_valid_after(|b| b.transfer_end = Some(TIME_LIMIT - 1)),
            "transfer end below the limit"
        );
        assert!(
            !load_valid_after(|b| b.transfer_end = Some(TIME_LIMIT)),
            "transfer end at the limit"
        );
        assert!(
            !load_valid_after(|b| b.deck = TapeDeck::with_times(TIME_LIMIT, None)),
            "invalid deck"
        );
    }
}
