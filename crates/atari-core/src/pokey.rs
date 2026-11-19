//! POKEY: audio, timers, keyboard scan, IRQs, random numbers and serial I/O.
//!
//! Design: `docs/design/pokey.md`. Requirements: REQ-500 to REQ-541.

#![expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    reason = "WP-0 stubs: delete this when the work package replaces the stubs"
)]

use crate::timing::VideoStandard;

/// A byte that the serial output shift register sent completely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerialByte {
    /// The 8 data bits.
    pub value: u8,
    /// The POKEY cycle (see [`Pokey::cycle`]) on which the start bit began.
    pub start_cycle: u64,
    /// The POKEY cycle (see [`Pokey::cycle`]) on which the stop bit ended.
    pub end_cycle: u64,
}

/// The POKEY chip of the 65XE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pokey {
    /// The TV standard (it sets the audio clock rate).
    standard: VideoStandard,
    /// The host audio rate in Hz.
    sample_rate: u32,
    /// The number of `tick` calls since power-on.
    cycles: u64,
    /// The 6-bit code of the key that is down, if any.
    key: Option<u8>,
    /// SHIFT is down.
    shift: bool,
    /// CONTROL is down.
    control: bool,
}

impl Pokey {
    /// Creates POKEY in the power-on state (REQ-501). `sample_rate` is the host audio rate
    /// in Hz, 8000..=192000 (REQ-538).
    #[must_use]
    pub const fn new(standard: VideoStandard, sample_rate: u32) -> Self {
        Self {
            standard,
            sample_rate,
            cycles: 0,
            key: None,
            shift: false,
            control: false,
        }
    }

    /// Sets the power-on state again (REQ-501). The machine calls it only for a cold start.
    pub const fn reset(&mut self) {
        *self = Self::new(self.standard, self.sample_rate);
    }

    /// Advances one machine cycle.
    pub const fn tick(&mut self) {
        // WP-0 STUB: WP-3 implements the keyboard scan and the IRQs.
        self.cycles += 1;
    }

    /// Reads a register. `reg` is the low byte of the address; POKEY uses `reg & 0x0F`.
    pub const fn read(&mut self, reg: u8) -> u8 {
        self.peek(reg)
    }

    /// Returns the value that `read` would return. It has no side effects (debugger).
    #[must_use]
    pub const fn peek(&self, reg: u8) -> u8 {
        // WP-0 STUB: KBCODE and SKSTAT only.
        match reg & 0x0F {
            0x09 => match self.key {
                Some(code) => {
                    let mut kbcode = code;
                    if self.shift {
                        kbcode |= 0x40;
                    }
                    if self.control {
                        kbcode |= 0x80;
                    }
                    kbcode
                }
                None => 0xFF,
            },
            0x0F => {
                let mut status = 0xFF;
                if self.shift {
                    status &= !0x08;
                }
                if self.key.is_some() {
                    status &= !0x04;
                }
                status
            }
            _ => 0xFF,
        }
    }

    /// Writes a register. `reg` is the low byte of the address.
    pub const fn write(&mut self, _reg: u8, _value: u8) {
        // WP-0 STUB.
    }

    /// The level of the IRQ output. `true` = asserted.
    #[must_use]
    pub const fn irq(&self) -> bool {
        false
    }

    /// The number of `tick` calls since power-on.
    #[must_use]
    pub const fn cycle(&self) -> u64 {
        self.cycles
    }

    /// Presses (`down`) or releases a key. `code` is a 6-bit key code; bits 6 and 7 are
    /// ignored.
    pub const fn set_key(&mut self, code: u8, down: bool) {
        if down {
            self.key = Some(code & 0x3F);
        } else if let Some(current) = self.key
            && current == code & 0x3F
        {
            self.key = None;
        }
    }

    /// Releases all keys.
    pub const fn release_all_keys(&mut self) {
        self.key = None;
    }

    /// Sets the SHIFT key.
    pub const fn set_shift(&mut self, down: bool) {
        self.shift = down;
    }

    /// Sets the CONTROL key.
    pub const fn set_control(&mut self, down: bool) {
        self.control = down;
    }

    /// Sets the BREAK key.
    pub const fn set_break(&mut self, _down: bool) {
        // WP-0 STUB.
    }

    /// Test helper: releases all keys, then presses `kbcode & 0x3F`, with SHIFT if bit 6 is
    /// set and CONTROL if bit 7 is set.
    pub const fn key_down(&mut self, kbcode: u8) {
        self.key = Some(kbcode & 0x3F);
        self.shift = kbcode & 0x40 != 0;
        self.control = kbcode & 0x80 != 0;
    }

    /// Test helper: releases the key and the modifiers that `key_down` pressed.
    pub const fn key_up(&mut self) {
        self.key = None;
        self.shift = false;
        self.control = false;
    }

    /// Paddle input `index` 0..=3 reaches threshold at slow-scan count `count` (REQ-535).
    pub const fn set_pot(&mut self, _index: usize, _count: u8) {}

    /// The SIO DATA IN level. `true` = high (mark).
    pub const fn set_serial_in(&mut self, _high: bool) {}

    /// The SIO DATA OUT level. `true` = high (mark).
    #[must_use]
    pub const fn serial_out(&self) -> bool {
        true
    }

    /// Removes and returns the oldest byte that was sent completely.
    pub const fn take_serial_byte(&mut self) -> Option<SerialByte> {
        None
    }

    /// The level of the GTIA console speaker line (`true` = high).
    pub const fn set_speaker(&mut self, _high: bool) {}

    /// Moves all samples made since the last call to the end of `out`.
    pub const fn drain_samples(&mut self, _out: &mut Vec<f32>) {
        // WP-0 STUB: audio is not in SLICE-01.
    }
}
