//! The serial shift registers and their clocks (HRM 5.6 Serial port).
//!
//! Design: `docs/design/pokey.md`, "Serial port". Requirements: REQ-525 (output), REQ-526
//! (input), REQ-527 (clock modes), REQ-528 (asynchronous receive), REQ-529 (input errors),
//! REQ-531 (reset), REQ-532 (force break).

use crate::state::impl_state;

/// The number of bits in one output frame: start bit, 8 data bits, stop bit.
const FRAME_BITS: u8 = 10;

/// The source of the serial output clock (HRM Table 10, REQ-527).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputClock {
    /// SIO CLOCK IN (modes `000` and `001`).
    External,
    /// The fires of timer 4 (modes `010` to `101`).
    Timer4,
    /// The fires of timer 2 (modes `110` and `111`).
    Timer2,
}

impl OutputClock {
    /// The output clock of `SKCTL` bits 6-4.
    pub(super) const fn from_skctl(skctl: u8) -> Self {
        match (skctl >> 4) & 0x07 {
            0b000 | 0b001 => Self::External,
            0b110 | 0b111 => Self::Timer2,
            _ => Self::Timer4,
        }
    }
}

/// The source of the serial input clock (HRM Table 10, REQ-527).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputClock {
    /// SIO CLOCK IN (modes `000` and `100`).
    External,
    /// The fires of timer 4 (modes `010` and `110`).
    Timer4,
    /// The fires of timer 4, with timers 3 and 4 held until a start edge (modes `xx1`).
    Async,
}

impl InputClock {
    /// The input clock of `SKCTL` bits 6-4.
    pub(super) const fn from_skctl(skctl: u8) -> Self {
        match (skctl >> 4) & 0x07 {
            0b000 | 0b100 => Self::External,
            0b010 | 0b110 => Self::Timer4,
            _ => Self::Async,
        }
    }
}

/// A frame that the shift register sent completely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SentFrame {
    /// The 8 data bits.
    pub(super) value: u8,
    /// The cycle on which the start bit began.
    pub(super) start_cycle: u64,
}

/// What one output clock edge did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct EdgeEvent {
    /// A frame ended on this edge and a device can receive it.
    pub(super) sent: Option<SentFrame>,
    /// `SEROUT` was loaded into the shift register (`IRQST` bit 4 latches).
    pub(super) loaded: bool,
}

/// The serial output: `SEROUT`, the shift register and the output clock flip-flop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the flags are independent latches of the HRM 5.6 serial output hardware"
)]
pub(super) struct SerialOut {
    /// `SEROUT`.
    serout: u8,
    /// A `SEROUT` value waits for the load.
    pending: bool,
    /// The frame: bit 0 is the start bit, bits 1-8 the data, bit 9 the stop bit.
    shift: u16,
    /// Bits of the frame that are not complete, including the bit on SIO DATA OUT (0 = idle).
    bits: u8,
    /// SIO DATA OUT from the shift register.
    level: bool,
    /// The output clock flip-flop.
    clock: bool,
    /// The cycle on which the start bit of the current frame began.
    start_cycle: u64,
    /// `SKCTL` bit 7 (force break) was 1 in a cycle of the current frame (OI-029).
    break_seen: bool,
}

impl_state!(SerialOut {
    serout,
    pending,
    shift,
    bits,
    level,
    clock,
    start_cycle,
    break_seen,
});

impl SerialOut {
    /// `true` if a loaded state has a bit count of a frame (REQ-061).
    pub(super) const fn loaded_is_valid(&self) -> bool {
        self.bits <= FRAME_BITS
    }

    /// The power-on state.
    pub(super) const fn new() -> Self {
        Self {
            serout: 0,
            pending: false,
            shift: 0,
            bits: 0,
            level: true,
            clock: false,
            start_cycle: 0,
            break_seen: false,
        }
    }

    /// A write of `SEROUT`.
    pub(super) const fn write(&mut self, value: u8) {
        self.serout = value;
        self.pending = true;
    }

    /// `true` while the shift register sends a frame (`IRQST` bit 3 reads 1).
    pub(super) const fn busy(&self) -> bool {
        self.bits != 0
    }

    /// The data bits of the frame in the shift register while it sends, else `None`.
    pub(super) const fn sending(&self) -> Option<u8> {
        if self.bits == 0 {
            None
        } else {
            Some(self.data())
        }
    }

    /// SIO DATA OUT from the shift register.
    pub(super) const fn level(&self) -> bool {
        self.level
    }

    /// The output clock flip-flop (SIO CLOCK OUT).
    pub(super) const fn clock(&self) -> bool {
        self.clock
    }

    /// Initialization mode (REQ-531): the output stops, a pending `SEROUT` is discarded, and
    /// SIO DATA OUT keeps its level.
    pub(super) const fn stop(&mut self) {
        self.bits = 0;
        self.pending = false;
    }

    /// A write of `SKCTL` bits 6-4 = `000`: the clock flip-flop goes to 0.
    pub(super) const fn reset_clock(&mut self) {
        self.clock = false;
    }

    /// Records force break (`SKCTL` bit 7) for the cycle.
    pub(super) const fn note_break(&mut self, force_break: bool) {
        if force_break && self.bits != 0 {
            self.break_seen = true;
        }
    }

    /// A toggle of the output clock flip-flop on cycle `now`. The shift register moves on
    /// the rising edge only. `force_break` is `SKCTL` bit 7.
    pub(super) fn toggle(&mut self, now: u64, force_break: bool) -> EdgeEvent {
        self.clock = !self.clock;
        if self.clock {
            self.rise(now, force_break)
        } else {
            EdgeEvent::default()
        }
    }

    /// A rising edge of the output clock on cycle `now`: the flip-flop of an internal clock,
    /// or SIO CLOCK IN for the external clock (REQ-527 criterion 6).
    pub(super) fn rise(&mut self, now: u64, force_break: bool) -> EdgeEvent {
        let mut event = EdgeEvent::default();
        if self.bits > 0 {
            self.bits -= 1;
            if self.bits > 0 {
                self.level = (self.shift >> (FRAME_BITS - self.bits)) & 1 != 0;
            } else if !self.break_seen {
                event.sent = Some(SentFrame {
                    value: self.data(),
                    start_cycle: self.start_cycle,
                });
            }
        }
        if self.bits == 0 && self.pending {
            self.shift = (u16::from(self.serout) << 1) | (1 << (FRAME_BITS - 1));
            self.level = false;
            self.bits = FRAME_BITS;
            self.pending = false;
            self.start_cycle = now;
            self.break_seen = force_break;
            event.loaded = true;
        }
        event
    }

    /// The data bits of the frame in the shift register.
    const fn data(&self) -> u8 {
        let [low, _stop] = (self.shift >> 1).to_le_bytes();
        low
    }
}

/// The number of the stop bit in an input frame: start bit 0, data bits 1-8, stop bit 9.
const STOP_BIT: u8 = 9;

/// A byte that the input shift register received completely: the stop bit was sampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Received {
    /// The 8 data bits.
    pub(super) value: u8,
    /// The stop bit was 1 (no framing error).
    pub(super) stop_ok: bool,
}

/// What one falling edge of the input clock did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sample {
    /// A start bit or a data bit.
    Shift,
    /// The input waited and sampled 1: no start bit.
    NoStart,
    /// The stop bit: the byte is complete.
    Done(Received),
}

/// The serial input: the shift register, its clock flip-flop and the asynchronous receive
/// state (HRM 5.6 Input shift register, Asynchronous receive mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SerialIn {
    /// The data bits received so far, bit 0 first.
    data: u8,
    /// The number of bits sampled in the current frame (0 = waiting for a start bit).
    bits: u8,
    /// The input clock flip-flop.
    clock: bool,
    /// Asynchronous mode: timers 3 and 4 run (from the start edge to the stop bit sample).
    running: bool,
    /// SIO DATA IN in the last cycle, for the start edge.
    last_level: bool,
}

impl_state!(SerialIn {
    data,
    bits,
    clock,
    running,
    last_level,
});

impl SerialIn {
    /// `true` if a loaded state has a bit count of a frame (REQ-061).
    pub(super) const fn loaded_is_valid(self) -> bool {
        self.bits <= STOP_BIT
    }

    /// The power-on state: the input waits, and the line was idle (1).
    pub(super) const fn new() -> Self {
        Self {
            data: 0,
            bits: 0,
            clock: false,
            running: false,
            last_level: true,
        }
    }

    /// `true` from the start bit sample to the stop bit sample (`SKSTAT` bit 1 reads 0).
    pub(super) const fn busy(self) -> bool {
        self.bits != 0
    }

    /// In asynchronous mode, timers 3 and 4 run.
    pub(super) const fn running(self) -> bool {
        self.running
    }

    /// The input clock flip-flop.
    pub(super) const fn clock(self) -> bool {
        self.clock
    }

    /// Records the SIO DATA IN level of this cycle. Returns `true` on a start edge: the line
    /// fell from 1 to 0 while the input waits and timers 3 and 4 are held.
    pub(super) const fn note_level(&mut self, level: bool) -> bool {
        let fell = self.last_level && !level;
        self.last_level = level;
        fell && self.bits == 0 && !self.running
    }

    /// The start edge in asynchronous mode: timers 3 and 4 run, and the clock is preset so
    /// that the first fire of timer 4 is a falling edge (HRM 5.6 Shift timing).
    pub(super) const fn start_async(&mut self) {
        self.running = true;
        self.clock = true;
    }

    /// Initialization mode (REQ-502): the input waits for a start bit. The clock flip-flop
    /// keeps its level.
    pub(super) const fn stop(&mut self) {
        self.bits = 0;
        self.running = false;
    }

    /// A write of `SKCTL` bits 6-4 = `000`: the clock flip-flop goes to 0, so that the input
    /// shifts after the next two clock cycles (HRM 5.6 Serial port reset).
    pub(super) const fn reset_clock(&mut self) {
        self.clock = false;
    }

    /// A fire of the input timer: the flip-flop toggles. Returns `true` on a falling edge.
    pub(super) const fn toggle(&mut self) -> bool {
        self.clock = !self.clock;
        !self.clock
    }

    /// A falling edge of the input clock, with the SIO DATA IN level `level` of this cycle.
    pub(super) const fn sample(&mut self, level: bool) -> Sample {
        match self.bits {
            0 => {
                if level {
                    self.running = false;
                    Sample::NoStart
                } else {
                    self.bits = 1;
                    self.data = 0;
                    Sample::Shift
                }
            }
            STOP_BIT => {
                self.bits = 0;
                self.running = false;
                Sample::Done(Received {
                    value: self.data,
                    stop_ok: level,
                })
            }
            n => {
                self.data |= (level as u8) << (n - 1);
                self.bits = n + 1;
                Sample::Shift
            }
        }
    }
}
