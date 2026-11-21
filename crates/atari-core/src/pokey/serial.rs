//! The serial output shift register and its clock (HRM 5.6 Serial port).
//!
//! Design: `docs/design/pokey.md`, "Serial port". Requirements: REQ-525, REQ-527 (output
//! clock only), REQ-532 (force break).
//!
//! The serial input, the external clock and the asynchronous receive hold come with the
//! serial package. Without an external clock input, the external output clock of `SKCTL`
//! modes `000` and `001` has no edges.

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

impl SerialOut {
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
        let mut event = EdgeEvent {
            sent: None,
            loaded: false,
        };
        if !self.clock {
            return event;
        }
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
