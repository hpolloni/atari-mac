//! POKEY: audio, timers, keyboard scan, IRQs, random numbers and serial I/O.
//!
//! Design: `docs/design/pokey.md`. Requirements: REQ-500 to REQ-541.
//!
//! SLICE-01 implements the register map (REQ-500), the power-on state (REQ-501), `IRQEN`
//! and `IRQST` (REQ-515 to REQ-517), the keyboard scan with SHIFT, CONTROL and BREAK
//! (REQ-518 to REQ-522), the unlinked and linked timers (REQ-504, REQ-505) and the serial
//! output (REQ-525) that the OS SIO code needs to send its command frames. Audio, the noise
//! generators, the serial input and the pot scan come in later slices. Their registers are
//! stored, and their inputs and outputs keep the idle levels.

mod kbscan;
mod serial;
mod timers;

use std::collections::VecDeque;

use crate::keyboard::{CONTROL, SHIFT};
use crate::timing::{CYCLES_PER_LINE, VideoStandard};
use kbscan::{KeyboardScan, ScanInput};
use serial::{OutputClock, SerialOut};
use timers::Timers;

/// `IRQST`/`IRQEN` bit 0: timer 1.
const IRQ_TIMER1: u8 = 0x01;
/// `IRQST`/`IRQEN` bit 1: timer 2.
const IRQ_TIMER2: u8 = 0x02;
/// `IRQST`/`IRQEN` bit 2: timer 4.
const IRQ_TIMER4: u8 = 0x04;
/// `IRQST`/`IRQEN` bit 3: serial output complete (not latched).
const IRQ_SERIAL_DONE: u8 = 0x08;
/// `IRQST`/`IRQEN` bit 4: serial output needed.
const IRQ_SERIAL_NEEDED: u8 = 0x10;
/// `IRQST`/`IRQEN` bit 6: keyboard.
const IRQ_KEY: u8 = 0x40;
/// `IRQST`/`IRQEN` bit 7: BREAK key.
const IRQ_BREAK: u8 = 0x80;

/// `SKSTAT` bit 6: 0 = keyboard overrun.
const SK_KEY_OVERRUN: u8 = 0x40;
/// `SKSTAT` bits 5-7 with no error.
const SK_NO_ERRORS: u8 = 0xE0;

/// `SKCTL` bit 0: keyboard debounce.
const SKCTL_DEBOUNCE: u8 = 0x01;
/// `SKCTL` bit 1: keyboard scan.
const SKCTL_KEY_SCAN: u8 = 0x02;
/// `SKCTL` bits 0-1: both 0 = initialization mode (HRM 5.2).
const SKCTL_INIT_MASK: u8 = 0x03;
/// `SKCTL` bits 4-6: the serial clock mode (HRM Table 10).
const SKCTL_CLOCK_MODE: u8 = 0x70;
/// `SKCTL` bit 7: force break.
const SKCTL_FORCE_BREAK: u8 = 0x80;

/// `AUDCTL` bit 0: the slow timers count on the 15 kHz clock.
const AUDCTL_15KHZ: u8 = 0x01;
/// `AUDCTL` bit 3: timers 3 and 4 are linked.
const AUDCTL_LINK34: u8 = 0x08;
/// `AUDCTL` bit 4: timers 1 and 2 are linked.
const AUDCTL_LINK12: u8 = 0x10;
/// `AUDCTL` bit 5: timer 3 counts on the 1.79 MHz clock.
const AUDCTL_FAST3: u8 = 0x20;
/// `AUDCTL` bit 6: timer 1 counts on the 1.79 MHz clock.
const AUDCTL_FAST1: u8 = 0x40;

/// The period of the 64 kHz clock in machine cycles.
const PERIOD_64KHZ: u64 = 28;
/// The period of the 15 kHz clock in machine cycles (one scanline).
const PERIOD_15KHZ: u64 = CYCLES_PER_LINE as u64;
/// Cycles from the exit of initialization mode to the first 64 kHz timer pulse (HRM 5.2).
const OFFSET_64KHZ: u64 = 20;
/// Cycles from the exit of initialization mode to the first 15 kHz timer pulse (HRM 5.2).
const OFFSET_15KHZ: u64 = 79;
/// Cycles from the exit of initialization mode to the first keyboard scan pulse (design
/// open issue 5: the pot counter pulse of HRM 5.9 Scan timing).
const OFFSET_KEY_SCAN: u64 = 81;

/// The number of bytes that the `take_serial_byte` FIFO holds.
const SERIAL_FIFO: usize = 64;

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

/// The write registers as stored, for the tests (`test_write_regs`).
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WriteRegs {
    /// `AUDF1-4`.
    audf: [u8; 4],
    /// `AUDC1-4`.
    audc: [u8; 4],
    /// `AUDCTL`.
    audctl: u8,
    /// `SKCTL`.
    skctl: u8,
    /// `IRQEN`.
    irqen: u8,
}

/// The POKEY chip of the 65XE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pokey {
    /// The TV standard (it sets the audio clock rate).
    standard: VideoStandard,
    /// The host audio rate in Hz.
    sample_rate: u32,
    /// `AUDF1-4`.
    audf: [u8; 4],
    /// `AUDC1-4`.
    audc: [u8; 4],
    /// `AUDCTL`.
    audctl: u8,
    /// `SKCTL` (bits 0-1 both 0 = initialization mode).
    skctl: u8,
    /// `IRQEN`.
    irqen: u8,
    /// `IRQEN` at the end of cycles N-1 to N-4 (REQ-517).
    irqen_hist: [u8; 4],
    /// Latched `IRQST` bits 0-2 and 4-7 (0 = event). Bit 3 is computed.
    irqst: u8,
    /// `KBCODE`.
    kbcode: u8,
    /// `SERIN`.
    serin: u8,
    /// `SKSTAT` bits 5-7 (1 = no error).
    sk_err: u8,
    /// The number of `tick` calls since power-on.
    cycle: u64,
    /// The cycle of the last write that ended initialization mode.
    exit_cycle: u64,
    /// The timer counters.
    timers: Timers,
    /// The timers that fired on the last tick (bit n = timer n + 1).
    fired: u8,
    /// The 17-bit polynomial counter (bits 16-0). It stays 0 until the noise generators
    /// are implemented.
    poly17: u32,
    /// `POT0-7`.
    pot: [u8; 8],
    /// `ALLPOT`.
    allpot: u8,
    /// The keys that are down, and the host levels of SHIFT, CONTROL and BREAK.
    keys: ScanInput,
    /// The modifiers that `key_down` pressed (`SHIFT` and `CONTROL` bits of `KBCODE`).
    typed_mods: u8,
    /// The keyboard scan.
    scan: KeyboardScan,
    /// The serial output shift register and its clock.
    serial: SerialOut,
    /// SIO DATA IN.
    in_level: bool,
    /// Sent bytes for the SIO model.
    sent: VecDeque<SerialByte>,
    /// The console speaker level.
    speaker: bool,
}

impl Pokey {
    /// Creates POKEY in the power-on state (REQ-501). `sample_rate` is the host audio rate
    /// in Hz, 8000..=192000 (REQ-538).
    #[must_use]
    pub const fn new(standard: VideoStandard, sample_rate: u32) -> Self {
        Self {
            standard,
            sample_rate,
            audf: [0; 4],
            audc: [0; 4],
            audctl: 0,
            skctl: 0,
            irqen: 0,
            irqen_hist: [0; 4],
            irqst: 0xFF,
            kbcode: 0xFF,
            serin: 0,
            sk_err: SK_NO_ERRORS,
            cycle: 0,
            exit_cycle: 0,
            timers: Timers::new(),
            fired: 0,
            poly17: 0,
            pot: [0; 8],
            allpot: 0,
            keys: ScanInput {
                matrix: 0,
                shift: false,
                control: false,
                brk: false,
            },
            typed_mods: 0,
            scan: KeyboardScan::new(),
            serial: SerialOut::new(),
            in_level: true,
            sent: VecDeque::new(),
            speaker: false,
        }
    }

    /// Sets the power-on state again (REQ-501). The machine calls it only for a cold start.
    pub fn reset(&mut self) {
        *self = Self::new(self.standard, self.sample_rate);
    }

    /// Advances one machine cycle.
    pub fn tick(&mut self) {
        self.cycle += 1;
        let now = self.cycle;
        self.irqen_hist = [
            self.irqen,
            self.irqen_hist[0],
            self.irqen_hist[1],
            self.irqen_hist[2],
        ];
        let clocked = self.timer_clocks();
        let linked = [
            self.audctl & AUDCTL_LINK12 != 0,
            self.audctl & AUDCTL_LINK34 != 0,
        ];
        let fired = self.timers.tick(now, clocked, linked, self.audf);
        self.fired = fired;
        // HRM 5.7: timers 1, 2 and 4 have an IRQ; timer 3 has none.
        if fired & 0b0001 != 0 {
            self.latch(IRQ_TIMER1);
        }
        if fired & 0b0010 != 0 {
            self.latch(IRQ_TIMER2);
        }
        if fired & 0b1000 != 0 {
            self.latch(IRQ_TIMER4);
        }
        self.serial_tick();
        self.keyboard_tick();
    }

    /// Reads a register. `reg` is the low byte of the address; POKEY uses `reg & 0x0F`.
    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "POKEY reads have no side effects, but the bus interface of pokey.md is \
                  `read(&mut self)`"
    )]
    pub const fn read(&mut self, reg: u8) -> u8 {
        self.peek(reg)
    }

    /// Returns the value that `read` would return. It has no side effects (debugger).
    #[must_use]
    pub const fn peek(&self, reg: u8) -> u8 {
        match reg & 0x0F {
            index @ 0x00..=0x07 => self.pot[index as usize],
            0x08 => self.allpot,
            0x09 => self.kbcode,
            0x0A => {
                let [low, ..] = (self.poly17 >> 8).to_le_bytes();
                !low
            }
            0x0D => self.serin,
            0x0E => {
                let busy = if self.serial.busy() {
                    IRQ_SERIAL_DONE
                } else {
                    0
                };
                (self.irqst & !IRQ_SERIAL_DONE) | busy
            }
            0x0F => self.skstat(),
            // `$B` and `$C` have no read register (HRM 14.4).
            _ => 0xFF,
        }
    }

    /// Writes a register. `reg` is the low byte of the address.
    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            index @ 0x00..=0x07 => {
                let channel = usize::from(index >> 1);
                if index & 1 == 0 {
                    self.audf[channel] = value;
                } else {
                    self.audc[channel] = value;
                }
            }
            0x08 => self.audctl = value,
            0x09 => self.timers.stimer(self.cycle),
            0x0A => self.sk_err = SK_NO_ERRORS,
            0x0E => {
                self.irqen = value;
                self.irqst |= !value & !IRQ_SERIAL_DONE;
            }
            0x0D => self.serial.write(value),
            0x0F => self.write_skctl(value),
            // `$B` (`POTGO`) starts the pot scan, which is not in SLICE-01. `$C` has no write
            // register.
            _ => {}
        }
    }

    /// The level of the IRQ output. `true` = asserted.
    #[must_use]
    pub const fn irq(&self) -> bool {
        (!self.irqst & !IRQ_SERIAL_DONE) != 0
            || (self.irqen & IRQ_SERIAL_DONE != 0 && !self.serial.busy())
    }

    /// The number of `tick` calls since power-on.
    #[must_use]
    pub const fn cycle(&self) -> u64 {
        self.cycle
    }

    /// Presses (`down`) or releases a key. `code` is a 6-bit key code; bits 6 and 7 are
    /// ignored.
    pub const fn set_key(&mut self, code: u8, down: bool) {
        let bit = 1_u64 << (code & 0x3F);
        if down {
            self.keys.matrix |= bit;
        } else {
            self.keys.matrix &= !bit;
        }
    }

    /// Releases all keys, and the modifiers that `key_down` pressed.
    pub const fn release_all_keys(&mut self) {
        self.keys.matrix = 0;
        self.typed_mods = 0;
    }

    /// Sets the SHIFT key.
    pub const fn set_shift(&mut self, down: bool) {
        self.keys.shift = down;
    }

    /// Sets the CONTROL key.
    pub const fn set_control(&mut self, down: bool) {
        self.keys.control = down;
    }

    /// Sets the BREAK key.
    pub const fn set_break(&mut self, down: bool) {
        self.keys.brk = down;
    }

    /// Test helper: releases all keys, then presses `kbcode & 0x3F`, with SHIFT if bit 6 is
    /// set and CONTROL if bit 7 is set.
    pub const fn key_down(&mut self, kbcode: u8) {
        self.keys.matrix = 1_u64 << (kbcode & 0x3F);
        self.typed_mods = kbcode & 0xC0;
    }

    /// Test helper: releases the key and the modifiers that `key_down` pressed.
    pub const fn key_up(&mut self) {
        self.release_all_keys();
    }

    /// Paddle input `index` 0..=3 reaches threshold at slow-scan count `count` (REQ-535).
    #[expect(
        clippy::unused_self,
        clippy::needless_pass_by_ref_mut,
        reason = "the pot scan is not in SLICE-01; the interface of pokey.md is kept"
    )]
    pub const fn set_pot(&mut self, _index: usize, _count: u8) {}

    /// The SIO DATA IN level. `true` = high (mark).
    pub const fn set_serial_in(&mut self, high: bool) {
        self.in_level = high;
    }

    /// The SIO DATA OUT level. `true` = high (mark).
    #[must_use]
    pub const fn serial_out(&self) -> bool {
        self.skctl & SKCTL_FORCE_BREAK == 0 && self.serial.level()
    }

    /// Removes and returns the oldest byte that was sent completely. The FIFO holds
    /// 64 bytes; when it is full, the oldest byte is lost.
    pub fn take_serial_byte(&mut self) -> Option<SerialByte> {
        debug_assert!(self.sent.len() <= SERIAL_FIFO, "serial FIFO overflow");
        self.sent.pop_front()
    }

    /// The level of the GTIA console speaker line (`true` = high). Audio is not in SLICE-01,
    /// so POKEY only stores the level.
    pub const fn set_speaker(&mut self, high: bool) {
        self.speaker = high;
    }

    /// Test accessor: the console speaker level from `set_speaker`.
    #[cfg(test)]
    pub(crate) const fn test_speaker(&self) -> bool {
        self.speaker
    }

    /// Moves all samples made since the last call to the end of `out`.
    #[expect(
        clippy::unused_self,
        clippy::needless_pass_by_ref_mut,
        reason = "audio is not in SLICE-01, so there are no samples; `Machine::drain_audio` \
                  is a const fn"
    )]
    pub const fn drain_samples(&mut self, _out: &mut Vec<f32>) {}

    /// `SKSTAT` (HRM 14.4).
    const fn skstat(&self) -> u8 {
        self.sk_err
            | ((self.in_level as u8) << 4)
            | ((!self.scan.shift_latch() as u8) << 3)
            | ((!self.scan.key_down() as u8) << 2)
            // Bit 1: the serial input is not busy. Bit 0 is always 1.
            | 0x03
    }

    /// POKEY is in initialization mode (HRM 5.2).
    const fn init_mode(&self) -> bool {
        self.skctl & SKCTL_INIT_MASK == 0
    }

    /// A write to `SKCTL`.
    const fn write_skctl(&mut self, value: u8) {
        if self.init_mode() && value & SKCTL_INIT_MASK != 0 {
            self.exit_cycle = self.cycle;
        }
        self.skctl = value;
        if value & SKCTL_CLOCK_MODE == 0 {
            self.serial.reset_clock();
        }
        if self.init_mode() {
            self.serial.stop();
        }
        if value & SKCTL_KEY_SCAN == 0 {
            self.scan.disable();
        }
    }

    /// A slow clock pulse on this cycle: `offset + period x k` cycles after the exit of
    /// initialization mode, and never in initialization mode (HRM 5.2).
    const fn slow_pulse(&self, offset: u64, period: u64) -> bool {
        if self.init_mode() {
            return false;
        }
        match self.cycle.checked_sub(self.exit_cycle + offset) {
            Some(delta) => delta % period == 0,
            None => false,
        }
    }

    /// Which timers get a count pulse on this cycle (HRM 5.4). The linked modes are not in
    /// SLICE-01.
    const fn timer_clocks(&self) -> [bool; 4] {
        let slow = if self.audctl & AUDCTL_15KHZ != 0 {
            self.slow_pulse(OFFSET_15KHZ, PERIOD_15KHZ)
        } else {
            self.slow_pulse(OFFSET_64KHZ, PERIOD_64KHZ)
        };
        [
            self.audctl & AUDCTL_FAST1 != 0 || slow,
            slow,
            self.audctl & AUDCTL_FAST3 != 0 || slow,
            slow,
        ]
    }

    /// The serial output for this cycle (HRM 5.6), after the timer fires of the cycle.
    fn serial_tick(&mut self) {
        let fired = self.fired;
        if self.init_mode() {
            self.serial.stop();
            return;
        }
        let force_break = self.skctl & SKCTL_FORCE_BREAK != 0;
        self.serial.note_break(force_break);
        let edge = match OutputClock::from_skctl(self.skctl) {
            OutputClock::Timer4 => fired & 0b1000 != 0,
            OutputClock::Timer2 => fired & 0b0010 != 0,
            OutputClock::External => false,
        };
        if !edge {
            return;
        }
        let event = self.serial.toggle(self.cycle, force_break);
        if let Some(frame) = event.sent {
            if self.sent.len() == SERIAL_FIFO {
                let _lost = self.sent.pop_front();
            }
            self.sent.push_back(SerialByte {
                value: frame.value,
                start_cycle: frame.start_cycle,
                end_cycle: self.cycle,
            });
        }
        if event.loaded {
            self.latch(IRQ_SERIAL_NEEDED);
        }
    }

    /// The keyboard scan for this cycle (HRM 5.8).
    const fn keyboard_tick(&mut self) {
        if self.skctl & SKCTL_KEY_SCAN == 0 {
            self.scan.disable();
            return;
        }
        if !self.slow_pulse(OFFSET_KEY_SCAN, PERIOD_15KHZ) {
            return;
        }
        let input = ScanInput {
            shift: self.keys.shift || self.typed_mods & SHIFT != 0,
            control: self.keys.control || self.typed_mods & CONTROL != 0,
            ..self.keys
        };
        let event = self.scan.pulse(input, self.skctl & SKCTL_DEBOUNCE != 0);
        if let Some(kbcode) = event.key {
            self.kbcode = kbcode;
            // REQ-521: a new key while the keyboard IRQ is still active is an overrun.
            if self.irqst & IRQ_KEY == 0 {
                self.sk_err &= !SK_KEY_OVERRUN;
            }
            self.latch(IRQ_KEY);
        }
        if event.brk {
            self.latch(IRQ_BREAK);
        }
    }

    /// An interrupt event on this cycle E. It latches if its bit was enabled at the end of
    /// cycles E-4 and E-2 (HRM 5.7 Enable/disable timing, REQ-517).
    const fn latch(&mut self, bit: u8) {
        let mask = self.irqen_hist[3] & self.irqen_hist[1];
        if mask & bit != 0 {
            self.irqst &= !bit;
        }
    }

    /// Test accessor: the write registers as stored.
    #[cfg(test)]
    const fn test_write_regs(&self) -> WriteRegs {
        WriteRegs {
            audf: self.audf,
            audc: self.audc,
            audctl: self.audctl,
            skctl: self.skctl,
            irqen: self.irqen,
        }
    }

    /// Test accessor: the four timer counters.
    #[cfg(test)]
    const fn test_counters(&self) -> [u8; 4] {
        self.timers.counters()
    }

    /// Test accessor: the timers that fired on the last tick (bit n = timer n + 1). It also
    /// shows the fires of disabled IRQs.
    #[cfg(test)]
    const fn test_fired(&self) -> u8 {
        self.fired
    }
}

#[cfg(test)]
mod tests {
    mod audio;
    mod kbscan;
    mod pots;
    mod serial;
    mod timers;

    use core::iter;

    use super::{Pokey, WriteRegs};
    use crate::keyboard::{self, KEYS};
    use crate::timing::VideoStandard;

    /// Machine cycles in one scanline (one 15 kHz clock period).
    const LINE: u64 = 114;
    /// `AUDF1`.
    const AUDF1: u8 = 0x00;
    /// `AUDC1`.
    const AUDC1: u8 = 0x01;
    /// `AUDF2`.
    const AUDF2: u8 = 0x02;
    /// `AUDCTL`.
    const AUDCTL: u8 = 0x08;
    /// `KBCODE` (read) and `STIMER` (write).
    const KBCODE: u8 = 0x09;
    /// `STIMER` (write).
    const STIMER: u8 = 0x09;
    /// `SKRES` (write).
    const SKRES: u8 = 0x0A;
    /// `POTGO` (write).
    const POTGO: u8 = 0x0B;
    /// `IRQST` (read) and `IRQEN` (write).
    const IRQST: u8 = 0x0E;
    /// `IRQEN` (write).
    const IRQEN: u8 = 0x0E;
    /// `SKSTAT` (read) and `SKCTL` (write).
    const SKSTAT: u8 = 0x0F;
    /// `SKCTL` (write).
    const SKCTL: u8 = 0x0F;
    /// `SKSTAT` bit 2: 0 = a key is down.
    const SK_KEY: u8 = 0x04;
    /// `SKSTAT` bit 3: 0 = SHIFT is down.
    const SK_SHIFT: u8 = 0x08;
    /// `SKSTAT` bit 6: 0 = keyboard overrun.
    const SK_OVERRUN: u8 = 0x40;
    /// `IRQST` bit 6: keyboard.
    const IRQ_KEY: u8 = 0x40;
    /// `IRQST` bit 7: BREAK.
    const IRQ_BREAK: u8 = 0x80;

    /// A new NTSC POKEY with a 48 kHz audio rate.
    fn new_pokey() -> Pokey {
        Pokey::new(VideoStandard::Ntsc, 48_000)
    }

    /// Runs `cycles` cycles.
    fn run(p: &mut Pokey, cycles: u64) {
        for _ in 0..cycles {
            p.tick();
        }
    }

    /// Runs up to `max` cycles until `done` is true after a tick. Returns the cycle.
    fn run_until(p: &mut Pokey, max: u64, done: impl Fn(&Pokey) -> bool) -> Option<u64> {
        for _ in 0..max {
            p.tick();
            if done(p) {
                return Some(p.cycle());
            }
        }
        None
    }

    /// Runs `cycles` cycles and checks `ok` after each tick.
    fn run_checking(p: &mut Pokey, cycles: u64, what: &str, ok: impl Fn(&Pokey) -> bool) {
        for _ in 0..cycles {
            p.tick();
            assert!(ok(p), "{what} on cycle {}", p.cycle());
        }
    }

    /// `KBCODE`.
    fn kbcode(p: &Pokey) -> u8 {
        p.peek(KBCODE)
    }

    /// `IRQST`.
    fn irqst(p: &Pokey) -> u8 {
        p.peek(IRQST)
    }

    /// `SKSTAT`.
    fn skstat(p: &Pokey) -> u8 {
        p.peek(SKSTAT)
    }

    /// `SKSTAT` bit 2 reads 0.
    fn key_is_down(p: &Pokey) -> bool {
        skstat(p) & SK_KEY == 0
    }

    /// The setup of REQ-518: `SKCTL` = `skctl` and `IRQEN` = `irqen`, then 300 scanlines.
    fn keyboard_ready(skctl: u8, irqen: u8) -> Pokey {
        let mut p = new_pokey();
        p.tick();
        p.write(SKCTL, skctl);
        p.write(IRQEN, irqen);
        run(&mut p, 300 * LINE);
        p
    }

    /// Presses `code` before the next tick, and runs until `SKSTAT` bit 2 reads 0. Returns
    /// the latency R - K in cycles.
    fn press_until_registered(p: &mut Pokey, code: u8) -> u64 {
        p.set_key(code, true);
        let k = p.cycle() + 1;
        let r = run_until(p, 130 * LINE, key_is_down)
            .unwrap_or_else(|| panic!("key {code:#04X} did not register"));
        r - k
    }

    /// Clears an `IRQST` bit with `IRQEN` = `irqen & !bit`, then `IRQEN` = `irqen`.
    fn clear_irq(p: &mut Pokey, irqen: u8, bit: u8) {
        p.write(IRQEN, irqen & !bit);
        p.tick();
        p.write(IRQEN, irqen);
    }

    /// The reads of all 16 registers (with `peek`).
    fn all_reads(p: &Pokey) -> [u8; 16] {
        let mut reads = [0; 16];
        for (reg, read) in (0_u8..).zip(reads.iter_mut()) {
            *read = p.peek(reg);
        }
        reads
    }

    /// The write registers after power-on.
    const ZERO_REGS: WriteRegs = WriteRegs {
        audf: [0; 4],
        audc: [0; 4],
        audctl: 0,
        skctl: 0,
        irqen: 0,
    };

    /// The script of REQ-501 criterion 4, run on `p`. It drains the samples into `samples`
    /// and calls `each` after each tick.
    fn power_on_script(p: &mut Pokey, samples: &mut Vec<f32>, mut each: impl FnMut(&mut Pokey)) {
        p.tick();
        each(p);
        p.write(SKCTL, 0x03);
        p.write(AUDF1, 0x48);
        p.write(AUDC1, 0xAF);
        p.write(POTGO, 0x00);
        p.set_key(keyboard::A, true);
        for cycle in 1..298_680_u64 {
            if cycle == 20_001 {
                p.set_key(keyboard::A, false);
            }
            p.tick();
            each(p);
            if cycle % 29_868 == 0 {
                p.drain_samples(samples);
            }
        }
        p.drain_samples(samples);
    }

    /// Verifies REQ-500: all 16 mirrors of each register select the same register.
    #[test]
    fn mirrors_select_same_register() {
        // Criterion 1: AUDC1 through a mirror.
        let mut mirror = new_pokey();
        let mut canonical = new_pokey();
        mirror.write(0xF1, 0x1F);
        canonical.write(AUDC1, 0x1F);
        assert_eq!(
            mirror.test_write_regs().audc[0],
            0x1F,
            "AUDC1 through $D2F1"
        );
        assert_eq!(mirror, canonical, "$D2F1 and $D201");
        // Criterion 2: reads.
        let mut p = new_pokey();
        for state in 0..2 {
            if state == 1 {
                p.tick();
                p.write(SKCTL, 0x03);
                p.write(IRQEN, 0xFF);
                run(&mut p, 1000);
            }
            for addr in 0_u8..=0xFF {
                assert_eq!(p.read(addr), p.read(addr & 0x0F), "read of $D2{addr:02X}");
            }
        }
        // Criterion 3: writes.
        for addr in 0_u8..=0xFF {
            let mut a = new_pokey();
            let mut b = new_pokey();
            a.tick();
            b.tick();
            a.write(addr, 0x5B);
            b.write(addr & 0x0F, 0x5B);
            assert_eq!(
                a.test_write_regs(),
                b.test_write_regs(),
                "write of $D2{addr:02X}"
            );
            run(&mut a, 200);
            run(&mut b, 200);
            assert_eq!(a, b, "state after a write of $D2{addr:02X}");
        }
    }

    /// Verifies REQ-500: offsets `$B` and `$C` read `$FF`.
    #[test]
    fn unassigned_reads_return_ff() {
        let mut p = new_pokey();
        for state in 0..2 {
            if state == 1 {
                p.tick();
                p.write(SKCTL, 0x03);
                p.write(IRQEN, 0xFF);
                run(&mut p, 1000);
            }
            for addr in [0x0B, 0x0C, 0xEB, 0xFC] {
                assert_eq!(p.read(addr), 0xFF, "read of $D2{addr:02X} in state {state}");
            }
        }
    }

    /// Verifies REQ-500: a write to offset `$C` changes nothing.
    #[test]
    fn offset_c_write_has_no_effect() {
        let mut p = new_pokey();
        p.tick();
        p.write(SKCTL, 0x03);
        p.write(IRQEN, 0xC0);
        run(&mut p, 1000);
        let before = p.clone();
        p.write(0x0C, 0xFF);
        p.write(0xFC, 0xFF);
        assert_eq!(all_reads(&p), all_reads(&before), "reads");
        assert_eq!(
            p.test_write_regs(),
            before.test_write_regs(),
            "write registers"
        );
        assert_eq!(p.test_counters(), before.test_counters(), "counters");
        assert_eq!(p, before, "state");
    }

    /// Verifies REQ-500: each offset reads and writes the register of the table.
    #[test]
    fn register_map_matches_table() {
        let mut p = new_pokey();
        p.tick();
        for (reg, value) in (0_u8..8).zip([0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18]) {
            p.write(reg, value);
        }
        p.write(AUDCTL, 0x41);
        p.write(IRQEN, 0x40);
        p.write(SKCTL, 0x03);
        let regs = p.test_write_regs();
        assert_eq!(
            regs.audf,
            [0x11, 0x13, 0x15, 0x17],
            "AUDF1-4 at offsets 0, 2, 4, 6"
        );
        assert_eq!(
            regs.audc,
            [0x12, 0x14, 0x16, 0x18],
            "AUDC1-4 at offsets 1, 3, 5, 7"
        );
        assert_eq!(regs.audctl, 0x41, "AUDCTL at offset 8");
        assert_eq!(regs.irqen, 0x40, "IRQEN at offset $E");
        assert_eq!(regs.skctl, 0x03, "SKCTL at offset $F");
        // STIMER at offset 9 reloads the counters 3 cycles later.
        p.write(STIMER, 0x00);
        run(&mut p, 3);
        assert_eq!(p.test_counters(), [0x11, 0x13, 0x15, 0x17], "STIMER reload");
        // The reads: POT0-7, ALLPOT, KBCODE, RANDOM, none, none, SERIN, IRQST, SKSTAT.
        let reads = all_reads(&p);
        assert_eq!(reads[..8], [0; 8], "POT0-7");
        assert_eq!(reads[8], 0x00, "ALLPOT");
        assert_eq!(reads[9], 0xFF, "KBCODE");
        assert_eq!(reads[0xA], 0xFF, "RANDOM");
        assert_eq!(reads[0xB..=0xC], [0xFF, 0xFF], "no registers");
        assert_eq!(reads[0xD], 0x00, "SERIN");
        assert_eq!(reads[0xE], 0xF7, "IRQST");
        assert_eq!(reads[0xF], 0xFF, "SKSTAT");
        // KBCODE at offset 9 and SKSTAT at offset $F follow the keyboard.
        let _latency = press_until_registered(&mut p, keyboard::A);
        assert_eq!(p.read(KBCODE), keyboard::A, "KBCODE at offset 9");
        assert_eq!(p.read(IRQST), 0xB7, "IRQST at offset $E");
        assert_eq!(p.read(SKSTAT), 0xFB, "SKSTAT at offset $F");
        // SKRES at offset $A clears the error bits; POTGO and SEROUT change no read.
        p.write(SKRES, 0x00);
        p.write(POTGO, 0x00);
        p.write(0x0D, 0x55);
        assert_eq!(p.read(SKSTAT), 0xFB, "SKSTAT after SKRES");
    }

    /// Verifies REQ-501: the write registers are `$00` after power-on.
    #[test]
    fn power_on_write_registers_zero() {
        for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
            let p = Pokey::new(standard, 48_000);
            assert_eq!(p.test_write_regs(), ZERO_REGS, "{standard:?}");
        }
    }

    /// Verifies REQ-501: the register reads and the IRQ after power-on.
    #[test]
    fn power_on_state() {
        let mut p = new_pokey();
        assert_eq!(p.read(IRQST), 0xF7, "IRQST");
        assert!(!p.irq(), "IRQ");
        assert_eq!(p.read(KBCODE), 0xFF, "KBCODE");
        assert_eq!(p.read(0x0D), 0x00, "SERIN");
        assert_eq!(p.read(SKSTAT), 0xFF, "SKSTAT");
        assert_eq!(p.read(0x0A), 0xFF, "RANDOM");
        assert_eq!(p.read(0x08), 0x00, "ALLPOT");
        for reg in 0..8 {
            assert_eq!(p.read(reg), 0x00, "POT{reg}");
        }
        assert_eq!(p.cycle(), 0, "cycle");
    }

    /// Verifies REQ-501: two instances give the same reads on each cycle and the same
    /// samples.
    #[test]
    fn power_on_is_deterministic() {
        let mut reads_a = Vec::new();
        let mut reads_b = Vec::new();
        let mut samples_a = Vec::new();
        let mut samples_b = Vec::new();
        let mut a = new_pokey();
        let mut b = new_pokey();
        power_on_script(&mut a, &mut samples_a, |p| reads_a.push(all_reads(p)));
        power_on_script(&mut b, &mut samples_b, |p| reads_b.push(all_reads(p)));
        assert_eq!(reads_a.len(), 298_680, "cycles");
        assert!(reads_a == reads_b, "the reads differ");
        assert!(
            samples_a
                .iter()
                .map(|s| s.to_bits())
                .eq(samples_b.iter().map(|s| s.to_bits())),
            "the samples differ"
        );
        assert!(
            reads_a.iter().any(|reads| reads[9] == keyboard::A),
            "the A key registered"
        );
    }

    /// Verifies REQ-501: `reset` after the script gives the power-on state.
    #[test]
    fn reset_equals_new() {
        let mut p = new_pokey();
        let mut samples = Vec::new();
        power_on_script(&mut p, &mut samples, |_| {});
        assert_ne!(p, new_pokey(), "the script changes the state");
        p.reset();
        let fresh = new_pokey();
        assert_eq!(all_reads(&p), all_reads(&fresh), "reads");
        assert_eq!(
            p.test_write_regs(),
            fresh.test_write_regs(),
            "write registers"
        );
        assert_eq!(p.test_counters(), fresh.test_counters(), "counters");
        assert_eq!(p, fresh, "state");
        let pal = Pokey::new(VideoStandard::Pal, 44_100);
        let mut reset_pal = pal.clone();
        reset_pal.reset();
        assert_eq!(
            reset_pal, pal,
            "reset keeps the standard and the sample rate"
        );
    }

    /// The setup of REQ-515 criterion 1: timer 1 at 1.79 MHz with `AUDF1` = 0 and
    /// `IRQEN` = `irqen` before the `STIMER` write. Returns T.
    fn fast_timer1(p: &mut Pokey, irqen: u8) -> u64 {
        p.write(AUDCTL, 0x40);
        p.write(AUDF1, 0x00);
        p.write(IRQEN, irqen);
        p.tick();
        p.write(STIMER, 0x00);
        p.cycle()
    }

    /// Verifies REQ-515: a timer 1 event latches `IRQST` bit 0 and asserts the IRQ.
    #[test]
    fn irq_latch_and_line() {
        let mut p = new_pokey();
        let t = fast_timer1(&mut p, 0x01);
        run(&mut p, 7);
        assert_eq!(irqst(&p), 0xF7, "IRQST on T+7");
        assert!(!p.irq(), "IRQ on T+7");
        p.tick();
        assert_eq!(p.cycle(), t + 8, "cycle");
        assert_eq!(irqst(&p), 0xF6, "IRQST on T+8");
        assert!(p.irq(), "IRQ on T+8");
    }

    /// Verifies REQ-515: an `IRQEN` write of 0 sets the status bit to 1 and ends the IRQ.
    #[test]
    fn irqen_clear_resets_status() {
        let mut p = new_pokey();
        let _t = fast_timer1(&mut p, 0x01);
        run(&mut p, 8);
        assert_eq!(irqst(&p), 0xF6, "IRQST on T+8");
        p.write(IRQEN, 0x00);
        assert_eq!(irqst(&p), 0xF7, "IRQST after the write");
        run_checking(&mut p, 100, "IRQST $F7 and no IRQ", |p| {
            irqst(p) == 0xF7 && !p.irq()
        });
    }

    /// Verifies REQ-515: the IRQ stays asserted until all status bits are 1.
    #[test]
    fn irq_line_until_all_cleared() {
        let mut p = new_pokey();
        p.tick();
        p.write(AUDCTL, 0x00);
        p.write(AUDF1, 0x00);
        p.write(AUDF2, 0x00);
        p.write(IRQEN, 0x03);
        p.write(SKCTL, 0x03);
        let latched = run_until(&mut p, 1000, |p| irqst(p) == 0xF4);
        assert!(latched.is_some(), "both timer IRQs latch");
        assert!(p.irq(), "IRQ with both bits");
        p.write(IRQEN, 0x02);
        assert_eq!(irqst(&p), 0xF5, "IRQST after IRQEN = $02");
        run_checking(&mut p, 200, "IRQ with bit 1", Pokey::irq);
        p.write(IRQEN, 0x00);
        assert_eq!(irqst(&p), 0xF7, "IRQST after IRQEN = $00");
        run_checking(&mut p, 200, "no IRQ", |p| !p.irq() && irqst(p) == 0xF7);
    }

    /// Verifies REQ-515: reading `IRQST` changes nothing.
    #[test]
    fn irqst_read_no_side_effect() {
        let mut a = new_pokey();
        let mut b = new_pokey();
        let _ta = fast_timer1(&mut a, 0x01);
        let _tb = fast_timer1(&mut b, 0x01);
        let mut samples_a = Vec::new();
        let mut samples_b = Vec::new();
        for _ in 0..10_000 {
            a.tick();
            b.tick();
            let _irqst = b.read(IRQST);
            assert_eq!(all_reads(&a), all_reads(&b), "reads on cycle {}", a.cycle());
            assert_eq!(a.test_counters(), b.test_counters(), "counters");
            assert_eq!(a, b, "state on cycle {}", a.cycle());
        }
        a.drain_samples(&mut samples_a);
        b.drain_samples(&mut samples_b);
        assert_eq!(samples_a, samples_b, "samples");
    }

    /// Verifies REQ-515: an event while its source is disabled is lost.
    #[test]
    fn disabled_event_is_lost() {
        let setup = |irqen| {
            let mut p = new_pokey();
            p.tick();
            p.write(AUDCTL, 0x00);
            p.write(AUDF1, 0x00);
            p.write(IRQEN, irqen);
            p.write(SKCTL, 0x03);
            p
        };
        // A reference instance with the IRQ enabled finds the fire cycles E and E+28.
        let mut reference = setup(0x01);
        let e = run_until(&mut reference, 1000, |p| irqst(p) & 0x01 == 0)
            .unwrap_or_else(|| panic!("timer 1 does not fire"));
        clear_irq(&mut reference, 0x01, 0x01);
        let next = run_until(&mut reference, 1000, |p| irqst(p) & 0x01 == 0);
        assert_eq!(next, Some(e + 28), "the next fire");
        // The test instance enables the IRQ one cycle after the fire on E.
        let mut p = setup(0x00);
        let start = p.cycle();
        run(&mut p, e + 1 - start);
        p.write(IRQEN, 0x01);
        assert_eq!(irqst(&p) & 0x01, 0x01, "bit 0 on cycle E+1");
        run_checking(&mut p, 26, "bit 0 is 1 before E+28", |p| {
            irqst(p) & 0x01 != 0
        });
        p.tick();
        assert_eq!(p.cycle(), e + 28, "cycle");
        assert_eq!(irqst(&p) & 0x01, 0x00, "bit 0 on cycle E+28");
    }

    /// Verifies REQ-517: an enable is effective after 4 cycles.
    #[test]
    fn irqen_enable_timing() {
        for (write_at, first_latch) in [(4, 8), (5, 12)] {
            let mut p = new_pokey();
            let t = fast_timer1(&mut p, 0x00);
            run(&mut p, write_at);
            p.write(IRQEN, 0x01);
            let latched = run_until(&mut p, 100, |p| irqst(p) & 0x01 == 0);
            assert_eq!(
                latched,
                Some(t + first_latch),
                "IRQEN written on T+{write_at}"
            );
        }
    }

    /// Verifies REQ-517: a disable is effective after 2 cycles.
    #[test]
    fn irqen_disable_timing() {
        let mut p = new_pokey();
        let _t = fast_timer1(&mut p, 0x01);
        run(&mut p, 6);
        p.write(IRQEN, 0x00);
        run_checking(&mut p, 14, "bit 0 is 1 (written on E-2)", |p| {
            irqst(p) & 0x01 != 0
        });
        let mut p = new_pokey();
        let _t = fast_timer1(&mut p, 0x01);
        run(&mut p, 7);
        p.write(IRQEN, 0x00);
        p.tick();
        assert_eq!(irqst(&p) & 0x01, 0, "bit 0 on E (written on E-1)");
        assert!(p.irq(), "IRQ on E (written on E-1)");
    }

    /// Verifies REQ-502: the keyboard scan does not run in initialization mode.
    #[test]
    fn init_mode_holds_keyboard_scan() {
        let mut p = keyboard_ready(0x00, 0xC0);
        p.set_key(keyboard::A, true);
        p.set_shift(true);
        p.set_break(true);
        run_checking(&mut p, 300 * LINE, "no keyboard event", |p| {
            irqst(p) == 0xF7 && skstat(p) == 0xFF && kbcode(p) == 0xFF
        });
    }

    /// Verifies REQ-518: a key press sets `KBCODE`, `IRQST` bit 6 and `SKSTAT` bit 2 on the
    /// same cycle.
    #[test]
    fn key_press_sets_kbcode_irq_skstat() {
        let mut p = keyboard_ready(0x03, 0x40);
        p.set_key(keyboard::A, true);
        let (mut code_at, mut irq_at, mut skstat_at) = (None, None, None);
        for _ in 0..130 * LINE {
            p.tick();
            let now = Some(p.cycle());
            if code_at.is_none() && kbcode(&p) == 0x3F {
                code_at = now;
            }
            if irq_at.is_none() && irqst(&p) & IRQ_KEY == 0 {
                irq_at = now;
            }
            if skstat_at.is_none() && key_is_down(&p) {
                skstat_at = now;
            }
        }
        assert!(code_at.is_some(), "KBCODE reads $3F");
        assert_eq!(code_at, irq_at, "KBCODE and IRQST bit 6");
        assert_eq!(code_at, skstat_at, "KBCODE and SKSTAT bit 2");
        assert!(p.irq(), "the keyboard IRQ");
    }

    /// Verifies REQ-518: a press registers 63 to 129 scanlines after the key goes down.
    #[test]
    fn key_press_latency() {
        let (mut min, mut max) = (u64::MAX, 0);
        for phase in 0..140 {
            let mut p = keyboard_ready(0x03, 0x40);
            run(&mut p, phase * 53);
            let latency = press_until_registered(&mut p, keyboard::A);
            assert!(
                (63 * LINE..=129 * LINE).contains(&latency),
                "latency {latency} at phase {phase}"
            );
            min = min.min(latency);
            max = max.max(latency);
        }
        assert!(
            max - min > 60 * LINE,
            "the phases cover a full scan: {min}..{max}"
        );
    }

    /// Verifies REQ-518: a release shows 63 to 129 scanlines after the key goes up, and
    /// changes neither `KBCODE` nor `IRQST`.
    #[test]
    fn key_release_latency() {
        for phase in [0, 17, 3000, 5000] {
            let mut p = keyboard_ready(0x03, 0x40);
            run(&mut p, phase);
            let latency = press_until_registered(&mut p, keyboard::A);
            run(&mut p, 200 * LINE - latency);
            p.set_key(keyboard::A, false);
            let u = p.cycle() + 1;
            let (code, status) = (kbcode(&p), irqst(&p));
            let v = run_until(&mut p, 130 * LINE, |p| {
                assert_eq!((kbcode(p), irqst(p)), (code, status), "KBCODE and IRQST");
                !key_is_down(p)
            })
            .unwrap_or_else(|| panic!("the release does not show"));
            assert!(
                (63 * LINE..=129 * LINE).contains(&(v - u)),
                "release latency {} at phase {phase}",
                v - u
            );
            run_checking(&mut p, 1000, "KBCODE and IRQST", |p| {
                (kbcode(p), irqst(p)) == (code, status)
            });
        }
    }

    /// Verifies REQ-518: two keys that go down together do not register.
    #[test]
    fn two_keys_rejected() {
        let mut p = keyboard_ready(0x03, 0x40);
        p.set_key(keyboard::A, true);
        p.set_key(keyboard::S, true);
        run_checking(&mut p, 300 * LINE, "no key", |p| {
            irqst(p) & IRQ_KEY != 0 && !key_is_down(p) && kbcode(p) == 0xFF
        });
    }

    /// Verifies REQ-518: a second key while the first is down is ignored.
    #[test]
    fn second_key_ignored() {
        let mut p = keyboard_ready(0x03, 0x40);
        let _latency = press_until_registered(&mut p, keyboard::A);
        clear_irq(&mut p, 0x40, IRQ_KEY);
        run(&mut p, 10 * LINE);
        p.set_key(keyboard::S, true);
        run_checking(&mut p, 300 * LINE, "A stays the key", |p| {
            kbcode(p) == 0x3F && key_is_down(p) && irqst(p) & IRQ_KEY != 0
        });
    }

    /// Verifies REQ-518: each press of the same key gives an IRQ.
    #[test]
    fn same_key_repeats_irq() {
        let mut p = keyboard_ready(0x03, 0x40);
        for press in 0..2 {
            p.set_key(keyboard::A, true);
            let k = p.cycle() + 1;
            let r = run_until(&mut p, 130 * LINE, |p| irqst(p) & IRQ_KEY == 0)
                .unwrap_or_else(|| panic!("press {press} gives no IRQ"));
            assert!(
                r - k <= 129 * LINE,
                "press {press}: IRQ after {} cycles",
                r - k
            );
            assert_eq!(kbcode(&p), 0x3F, "KBCODE after press {press}");
            clear_irq(&mut p, 0x40, IRQ_KEY);
            let held = p.cycle() + 1 - k;
            run(&mut p, 200 * LINE - held);
            p.set_key(keyboard::A, false);
            run(&mut p, 200 * LINE);
        }
    }

    /// Verifies REQ-519: SHIFT and CONTROL go into `KBCODE` bits 6 and 7.
    #[test]
    fn shift_control_bits_in_kbcode() {
        for (shift, control, expected) in
            [(true, false, 0x7F), (false, true, 0xBF), (true, true, 0xFF)]
        {
            let mut p = keyboard_ready(0x03, 0x40);
            p.set_shift(shift);
            p.set_control(control);
            let _latency = press_until_registered(&mut p, keyboard::A);
            assert_eq!(kbcode(&p), expected, "SHIFT {shift}, CONTROL {control}");
        }
        // The key_down helper presses the modifiers of its KBCODE value.
        for value in [0x3F, 0x7F, 0xBF, 0xFF] {
            let mut p = keyboard_ready(0x03, 0x40);
            p.key_down(value);
            assert!(
                run_until(&mut p, 130 * LINE, key_is_down).is_some(),
                "{value:#04X}"
            );
            assert_eq!(kbcode(&p), value, "key_down({value:#04X})");
            p.key_up();
            assert!(
                run_until(&mut p, 130 * LINE, |p| !key_is_down(p)).is_some(),
                "key_up"
            );
            assert_eq!(skstat(&p) & SK_SHIFT, SK_SHIFT, "key_up releases SHIFT");
        }
    }

    /// Verifies REQ-519: a modifier change after the press does not change `KBCODE`.
    #[test]
    fn late_shift_keeps_kbcode() {
        let mut p = keyboard_ready(0x03, 0x40);
        let _latency = press_until_registered(&mut p, keyboard::A);
        run(&mut p, 10 * LINE);
        p.set_shift(true);
        run_checking(&mut p, 300 * LINE, "KBCODE $3F", |p| kbcode(p) == 0x3F);

        let mut p = keyboard_ready(0x03, 0x40);
        p.set_shift(true);
        let _latency = press_until_registered(&mut p, keyboard::A);
        assert_eq!(kbcode(&p), 0x7F, "SHIFT + A");
        run(&mut p, 10 * LINE);
        p.set_shift(false);
        run_checking(&mut p, 300 * LINE, "KBCODE $7F", |p| kbcode(p) == 0x7F);
    }

    /// SHIFT alone changes `SKSTAT` bit 3 within 65 scanlines (REQ-519 criterion 3).
    fn check_shift_follows(skctl: u8) {
        let mut p = keyboard_ready(skctl, 0x40);
        p.set_shift(true);
        let down = run_until(&mut p, 65 * LINE, |p| skstat(p) & SK_SHIFT == 0);
        assert!(down.is_some(), "SHIFT down shows, SKCTL {skctl:#04X}");
        run(&mut p, 100 * LINE);
        p.set_shift(false);
        let up = run_until(&mut p, 65 * LINE, |p| skstat(p) & SK_SHIFT != 0);
        assert!(up.is_some(), "SHIFT up shows, SKCTL {skctl:#04X}");
    }

    /// SHIFT or CONTROL alone gives no key (REQ-519 criterion 4).
    fn check_modifiers_alone(skctl: u8) {
        for modifier in 0..2 {
            let mut p = keyboard_ready(skctl, 0x40);
            if modifier == 0 {
                p.set_shift(true);
            } else {
                p.set_control(true);
            }
            run_checking(&mut p, 300 * LINE, "no key", |p| {
                irqst(p) & IRQ_KEY != 0 && !key_is_down(p) && kbcode(p) == 0xFF
            });
        }
    }

    /// Verifies REQ-519: `SKSTAT` bit 3 follows SHIFT with no other key.
    #[test]
    fn skstat_shift_follows_key() {
        check_shift_follows(0x03);
    }

    /// Verifies REQ-519: SHIFT or CONTROL alone gives no key.
    #[test]
    fn modifiers_alone_no_irq() {
        check_modifiers_alone(0x03);
    }

    /// Verifies REQ-519: the modifiers do not depend on debounce.
    #[test]
    fn modifiers_ignore_debounce() {
        check_shift_follows(0x02);
        check_modifiers_alone(0x02);
    }

    /// Verifies REQ-520: with debounce off, a single key never registers.
    #[test]
    fn debounce_off_single_key() {
        let mut p = keyboard_ready(0x02, 0x40);
        p.set_key(keyboard::L, true);
        run_checking(&mut p, 300 * LINE, "no key", |p| {
            irqst(p) & IRQ_KEY != 0 && !key_is_down(p) && kbcode(p) == 0xFF
        });
    }

    /// Verifies REQ-520: with debounce off, two adjacent keys register the second key once
    /// per scan, for 2 scanlines.
    #[test]
    fn debounce_off_adjacent_keys() {
        let mut p = keyboard_ready(0x02, 0x40);
        p.set_key(keyboard::L, true);
        p.set_key(keyboard::J, true);
        let first = run_until(&mut p, 65 * LINE, |p| irqst(p) & IRQ_KEY == 0)
            .unwrap_or_else(|| panic!("no IRQ"));
        assert_eq!(kbcode(&p), keyboard::J, "KBCODE");
        assert!(key_is_down(&p), "SKSTAT bit 2 on the IRQ cycle");
        let mut starts = vec![first];
        let mut down_cycles = 1;
        let mut was_down = true;
        for _ in 0..300 * LINE {
            p.tick();
            let down = key_is_down(&p);
            if down {
                down_cycles += 1;
                if !was_down {
                    starts.push(p.cycle());
                }
            }
            was_down = down;
        }
        for pair in starts.windows(2) {
            assert_eq!(
                pair[1] - pair[0],
                64 * LINE,
                "spacing of the key-down periods"
            );
        }
        assert!(starts.len() >= 4, "key-down periods: {}", starts.len());
        let complete = u64::try_from(starts.len()).unwrap_or(u64::MAX);
        assert!(
            (complete - 1) * 2 * LINE <= down_cycles && down_cycles <= complete * 2 * LINE,
            "each key-down period is 2 scanlines: {down_cycles} cycles in {complete} periods"
        );
    }

    /// Verifies REQ-520: a disabled scan clears `SKSTAT` bit 2 and checks no key.
    #[test]
    fn scan_disabled_state() {
        let mut p = keyboard_ready(0x03, 0x40);
        let _latency = press_until_registered(&mut p, keyboard::A);
        p.write(SKCTL, 0x01);
        let up = run_until(&mut p, LINE, |p| !key_is_down(p));
        assert!(up.is_some(), "SKSTAT bit 2 reads 1");
        clear_irq(&mut p, 0xC0, IRQ_KEY);
        p.set_shift(true);
        p.set_break(true);
        run_checking(&mut p, 300 * LINE, "the scan is stopped", |p| {
            !key_is_down(p)
                && kbcode(p) == 0x3F
                && irqst(p) & (IRQ_KEY | IRQ_BREAK) == IRQ_KEY | IRQ_BREAK
                && skstat(p) & SK_SHIFT != 0
        });
    }

    /// Verifies REQ-520: turning debounce off releases a held key.
    #[test]
    fn debounce_off_releases_key() {
        let mut p = keyboard_ready(0x03, 0x40);
        let _latency = press_until_registered(&mut p, keyboard::A);
        p.write(SKCTL, 0x02);
        let up = run_until(&mut p, 3 * LINE, |p| !key_is_down(p));
        assert!(up.is_some(), "SKSTAT bit 2 reads 1 within 3 scanlines");
        run_checking(&mut p, 300 * LINE, "no key", |p| !key_is_down(p));
    }

    /// Verifies REQ-521: a new key while the keyboard IRQ is active is an overrun.
    #[test]
    fn keyboard_overrun() {
        let mut p = keyboard_ready(0x03, 0x40);
        let _latency = press_until_registered(&mut p, keyboard::A);
        assert_eq!(
            skstat(&p) & SK_OVERRUN,
            SK_OVERRUN,
            "no overrun after the first key"
        );
        run(&mut p, 200 * LINE);
        p.set_key(keyboard::A, false);
        run(&mut p, 200 * LINE);
        let _latency = press_until_registered(&mut p, keyboard::S);
        assert_eq!(skstat(&p) & SK_OVERRUN, 0, "overrun after the second key");
        assert_eq!(kbcode(&p), keyboard::S, "KBCODE");
    }

    /// Verifies REQ-521: `SKRES` clears the keyboard overrun.
    #[test]
    fn skres_clears_keyboard_overrun() {
        let mut p = keyboard_ready(0x03, 0x40);
        let _latency = press_until_registered(&mut p, keyboard::A);
        p.set_key(keyboard::A, false);
        run(&mut p, 200 * LINE);
        let _latency = press_until_registered(&mut p, keyboard::S);
        assert_eq!(skstat(&p) & SK_OVERRUN, 0, "overrun");
        p.write(SKRES, 0x00);
        p.tick();
        assert_eq!(
            skstat(&p) & SK_OVERRUN,
            SK_OVERRUN,
            "SKSTAT bit 6 after SKRES"
        );
    }

    /// Verifies REQ-521: SHIFT, CONTROL and BREAK do not make an overrun.
    #[test]
    fn modifiers_no_overrun() {
        let mut p = keyboard_ready(0x03, 0xC0);
        let _latency = press_until_registered(&mut p, keyboard::A);
        p.set_key(keyboard::A, false);
        p.set_shift(true);
        p.set_control(true);
        p.set_break(true);
        run_checking(&mut p, 300 * LINE, "no overrun", |p| {
            skstat(p) & SK_OVERRUN != 0
        });
        assert_eq!(irqst(&p) & IRQ_BREAK, 0, "the BREAK IRQ");
    }

    /// Presses BREAK before the next tick and runs until `IRQST` bit 7 reads 0, for at most
    /// 65 scanlines. Returns true if it latched.
    fn break_latches(p: &mut Pokey) -> bool {
        p.set_break(true);
        run_until(p, 65 * LINE, |p| irqst(p) & IRQ_BREAK == 0).is_some()
    }

    /// Verifies REQ-522: a BREAK press latches `IRQST` bit 7 within 65 scanlines.
    #[test]
    fn break_irq_on_key_down() {
        for phase in [0, 33, 1000, 4000, 7000] {
            let mut p = keyboard_ready(0x03, 0x80);
            run(&mut p, phase);
            assert!(break_latches(&mut p), "BREAK IRQ at phase {phase}");
            assert!(p.irq(), "IRQ");
        }
    }

    /// Verifies REQ-522: BREAK held down gives one IRQ.
    #[test]
    fn break_irq_once_while_held() {
        let mut p = keyboard_ready(0x03, 0x80);
        assert!(break_latches(&mut p), "BREAK IRQ");
        clear_irq(&mut p, 0x80, IRQ_BREAK);
        run_checking(&mut p, 300 * LINE, "IRQST bit 7 is 1", |p| {
            irqst(p) & IRQ_BREAK != 0
        });
    }

    /// Verifies REQ-522: a BREAK press while the IRQ is disabled is lost.
    #[test]
    fn break_lost_when_disabled() {
        let mut p = keyboard_ready(0x03, 0x00);
        p.set_break(true);
        run(&mut p, 100 * LINE);
        p.write(IRQEN, 0x80);
        run_checking(&mut p, 300 * LINE, "IRQST bit 7 is 1", |p| {
            irqst(p) & IRQ_BREAK != 0
        });
    }

    /// Verifies REQ-522: a second BREAK press gives a second IRQ.
    #[test]
    fn break_irq_on_second_press() {
        let mut p = keyboard_ready(0x03, 0x80);
        assert!(break_latches(&mut p), "first BREAK IRQ");
        clear_irq(&mut p, 0x80, IRQ_BREAK);
        p.set_break(false);
        run(&mut p, 100 * LINE);
        assert_eq!(irqst(&p) & IRQ_BREAK, IRQ_BREAK, "no IRQ on the release");
        assert!(break_latches(&mut p), "second BREAK IRQ");
    }

    /// Verifies REQ-522: BREAK changes no key register.
    #[test]
    fn break_no_kbcode_change() {
        let mut p = keyboard_ready(0x03, 0x80);
        let unchanged = |p: &Pokey| {
            kbcode(p) == 0xFF && skstat(p) & (SK_KEY | SK_OVERRUN) == SK_KEY | SK_OVERRUN
        };
        p.set_break(true);
        run_checking(&mut p, 200 * LINE, "KBCODE and SKSTAT", unchanged);
        p.set_break(false);
        run_checking(&mut p, 200 * LINE, "KBCODE and SKSTAT", unchanged);
    }

    /// Verifies REQ-522: BREAK needs the keyboard scan.
    #[test]
    fn break_needs_scan() {
        let mut p = keyboard_ready(0x01, 0x80);
        p.set_break(true);
        run_checking(&mut p, 300 * LINE, "IRQST bit 7 is 1", |p| {
            irqst(p) & IRQ_BREAK != 0
        });
    }

    /// Verifies REQ-522: BREAK does not depend on debounce.
    #[test]
    fn break_ignores_debounce() {
        let mut p = keyboard_ready(0x02, 0x80);
        assert!(break_latches(&mut p), "BREAK IRQ with debounce off");
    }

    /// Verifies REQ-523: each key alone latches its code.
    #[test]
    fn each_key_latches_its_code() {
        let mut p = keyboard_ready(0x03, 0x40);
        for (code, name) in KEYS {
            let _latency = press_until_registered(&mut p, code);
            assert_eq!(kbcode(&p), code, "key {name}");
            p.set_key(code, false);
            assert!(
                run_until(&mut p, 130 * LINE, |p| !key_is_down(p)).is_some(),
                "key {name} releases"
            );
            run(&mut p, 10 * LINE);
        }
    }

    // ------------------------------------------------------------------------------------
    // Linked timers (REQ-505) and serial output (REQ-502, REQ-516, REQ-525)
    // ------------------------------------------------------------------------------------

    /// `AUDF3`.
    const AUDF3: u8 = 0x04;
    /// `AUDC3`.
    const AUDC3: u8 = 0x05;
    /// `AUDF4`.
    const AUDF4: u8 = 0x06;
    /// `SEROUT` (write).
    const SEROUT: u8 = 0x0D;
    /// `IRQST` bit 3: serial output complete.
    const IRQ_DONE: u8 = 0x08;
    /// `IRQST` bit 4: serial output needed.
    const IRQ_NEEDED: u8 = 0x10;
    /// One bit on SIO DATA OUT with timers 3 and 4 linked at `$0028` (2 x 47 cycles).
    const BIT: u64 = 94;
    /// One frame of 10 bits.
    const FRAME: u64 = 10 * BIT;

    /// A POKEY with `SKCTL` = `skctl`, `AUDCTL` = `audctl` and `AUDF1-4` = `audf`, after
    /// 2000 cycles.
    fn timer_pokey(skctl: u8, audctl: u8, audf: [u8; 4]) -> Pokey {
        let mut p = new_pokey();
        p.tick();
        p.write(SKCTL, skctl);
        p.write(AUDCTL, audctl);
        for (reg, value) in [AUDF1, AUDF2, AUDF3, AUDF4].into_iter().zip(audf) {
            p.write(reg, value);
        }
        run(&mut p, 2000);
        p
    }

    /// The fires of the next `cycles` cycles as `(timer 1-4, cycle)`.
    fn fire_log(p: &mut Pokey, cycles: u64) -> Vec<(u8, u64)> {
        let mut log = Vec::new();
        for _ in 0..cycles {
            p.tick();
            for timer in 0..4 {
                if p.test_fired() & (1 << timer) != 0 {
                    log.push((timer + 1, p.cycle()));
                }
            }
        }
        log
    }

    /// The cycles of the fires of `timer` in `log`.
    fn fires_of(log: &[(u8, u64)], timer: u8) -> Vec<u64> {
        log.iter()
            .filter(|(t, _)| *t == timer)
            .map(|(_, cycle)| *cycle)
            .collect()
    }

    /// The distances between consecutive cycles.
    fn spacing(cycles: &[u64]) -> Vec<u64> {
        cycles.windows(2).map(|w| w[1] - w[0]).collect()
    }

    /// Checks that `timer` fires more than `count` times, `period` cycles apart.
    fn check_period(p: &mut Pokey, timer: u8, period: u64, count: u64) {
        let log = fire_log(p, period * (count + 2));
        let fires = fires_of(&log, timer);
        assert!(
            fires.len() as u64 > count,
            "timer {timer}: {} fires",
            fires.len()
        );
        assert!(
            spacing(&fires).iter().all(|d| *d == period),
            "timer {timer} period {period}: {:?}",
            spacing(&fires)
        );
    }

    /// The setup of REQ-525: `SKCTL` = `$23`, timers 3 and 4 linked at `$0028`, and
    /// `IRQEN` = `irqen`, at least 200 cycles before the first `SEROUT` write.
    fn serial_pokey(irqen: u8) -> Pokey {
        let mut p = timer_pokey(0x23, 0x28, [0, 0, 0x28, 0]);
        p.write(IRQEN, irqen);
        run(&mut p, 300);
        p
    }

    /// Writes `values` to `SEROUT` on the next cycle X and runs to the load L (the first
    /// cycle on which `IRQST` bit 3 reads 1). Returns `(X, L)`.
    fn send(p: &mut Pokey, values: &[u8]) -> (u64, u64) {
        p.tick();
        let x = p.cycle();
        for value in values {
            p.write(SEROUT, *value);
        }
        let l = run_until(p, 2 * BIT, |p| irqst(p) & IRQ_DONE != 0).expect("no load");
        (x, l)
    }

    /// Runs up to and including cycle `last`.
    fn run_to(p: &mut Pokey, last: u64) {
        let cycles = last - p.cycle();
        run(p, cycles);
    }

    /// Runs up to and including cycle `last` and checks `ok` after each tick.
    fn run_checking_to(p: &mut Pokey, last: u64, what: &str, ok: impl Fn(&Pokey) -> bool) {
        let cycles = last - p.cycle();
        run_checking(p, cycles, what, ok);
    }

    /// SIO DATA OUT on the next `cycles` cycles.
    fn levels(p: &mut Pokey, cycles: u64) -> Vec<bool> {
        (0..cycles)
            .map(|_| {
                p.tick();
                p.serial_out()
            })
            .collect()
    }

    /// The levels of the frame of `value` from cycle L+1 to cycle L+940, one entry per cycle.
    fn frame_levels(value: u8) -> Vec<bool> {
        let bits = (0..10).map(|bit| match bit {
            0 => false,
            9 => true,
            n => (value >> (n - 1)) & 1 != 0,
        });
        let mut levels: Vec<bool> = bits
            .flat_map(|level| iter::repeat_n(level, 94))
            .skip(1)
            .collect();
        levels.push(true);
        levels
    }

    /// Verifies REQ-505: timers 3 and 4 linked at 1.79 MHz with `$0028` fire every
    /// 47 cycles.
    #[test]
    fn linked_fast_period() {
        let mut p = timer_pokey(0x03, 0x28, [0, 0, 0x28, 0x00]);
        check_period(&mut p, 4, 47, 20);
    }

    /// Verifies REQ-505: `AUDF3` = `$CC`, `AUDF4` = `$05` gives 1491 cycles.
    #[test]
    fn linked_period_05cc() {
        let mut p = timer_pokey(0x03, 0x28, [0, 0, 0xCC, 0x05]);
        check_period(&mut p, 4, 1491, 4);
    }

    /// Verifies REQ-505: linked timers on the 64 kHz and 15 kHz clocks.
    #[test]
    fn linked_slow_periods() {
        let mut p = timer_pokey(0x03, 0x10, [0x00, 0x01, 0, 0]);
        check_period(&mut p, 2, 257 * 28, 3);
        let mut p = timer_pokey(0x03, 0x09, [0, 0, 0x00, 0x01]);
        check_period(&mut p, 4, 257 * LINE, 2);
    }

    /// Verifies REQ-505: the low timer fires `AUDF2` + 1 times in each period of the high
    /// timer.
    #[test]
    fn linked_low_timer_fires_hi_plus_one() {
        let mut p = timer_pokey(0x03, 0x50, [0x40, 0x01, 0, 0]);
        let log = fire_log(&mut p, 5000);
        let high = fires_of(&log, 2);
        assert!(high.len() >= 3, "timer 2 fires: {high:?}");
        for pair in high.windows(2) {
            let low = fires_of(&log, 1)
                .into_iter()
                .filter(|c| (pair[0]..pair[1]).contains(c))
                .count();
            assert_eq!(low, 2, "timer 1 fires between {pair:?}");
        }
    }

    /// Verifies REQ-505: with `AUDF4` = 1, timer 3 fires 2 times in each period of timer 4,
    /// and each fire of timer 4 is 3 cycles after the second fire of timer 3.
    #[test]
    fn linked_high_fires_3_after_low() {
        let mut p = timer_pokey(0x03, 0x28, [0, 0, 0x28, 0x01]);
        let log = fire_log(&mut p, 2000);
        let low = fires_of(&log, 3);
        let high = fires_of(&log, 4);
        assert!(high.len() >= 3, "timer 4 fires: {high:?}");
        for pair in high.windows(2) {
            let in_period: Vec<u64> = low
                .iter()
                .copied()
                .filter(|c| pair[0] < *c && *c < pair[1])
                .collect();
            assert_eq!(in_period.len(), 2, "timer 3 fires between {pair:?}");
            assert_eq!(in_period[1] + 3, pair[1], "timer 4 after timer 3");
        }
    }

    /// Verifies REQ-505: `AUDC3` does not change the linked period.
    #[test]
    fn linked_period_ignores_audc() {
        for audc in [0x00, 0x0F, 0x1F, 0x8F, 0xAF] {
            let mut p = timer_pokey(0x03, 0x28, [0, 0, 0x28, 0x00]);
            p.write(AUDC3, audc);
            check_period(&mut p, 4, 47, 10);
        }
    }

    /// Verifies REQ-525: the load is within one bit time of the `SEROUT` write, and it
    /// latches `IRQST` bit 4 and sets bit 3.
    #[test]
    fn serial_output_load_and_ready_irq() {
        let mut p = serial_pokey(0x10);
        assert_eq!(irqst(&p) & (IRQ_DONE | IRQ_NEEDED), IRQ_NEEDED, "idle");
        let (x, l) = send(&mut p, &[0x4F]);
        assert!(x < l && l <= x + BIT, "load on {l}, write on {x}");
        assert_eq!(irqst(&p) & IRQ_NEEDED, 0, "IRQST bit 4 on L");
        assert_eq!(irqst(&p) & IRQ_DONE, IRQ_DONE, "IRQST bit 3 on L");
        assert!(p.irq(), "IRQ line");
    }

    /// Verifies REQ-525: `$4F` goes out as 0, 1, 1, 1, 1, 0, 0, 1, 0, 1, each for 94 cycles.
    #[test]
    fn serial_output_frame_4f() {
        let mut p = serial_pokey(0x10);
        p.tick();
        p.write(SEROUT, 0x4F);
        // Before the load, the output is idle (1).
        for _ in 0..2 * BIT {
            p.tick();
            if irqst(&p) & IRQ_DONE != 0 {
                break;
            }
            assert!(p.serial_out(), "1 before the load, cycle {}", p.cycle());
        }
        let l = p.cycle();
        assert_eq!(irqst(&p) & IRQ_DONE, IRQ_DONE, "load");
        assert!(!p.serial_out(), "start bit on L = {l}");
        assert_eq!(
            levels(&mut p, FRAME),
            frame_levels(0x4F),
            "frame from L = {l}"
        );
        assert!(
            levels(&mut p, 1000).iter().all(|level| *level),
            "idle after the frame"
        );
    }

    /// Verifies REQ-525: a byte written during a frame follows it without a gap.
    #[test]
    fn serial_output_back_to_back() {
        let mut p = serial_pokey(0x10);
        let (_, l) = send(&mut p, &[0x4F]);
        clear_irq(&mut p, 0x10, IRQ_NEEDED);
        run(&mut p, 100);
        p.write(SEROUT, 0x55);
        run_to(&mut p, l + FRAME - 1);
        assert!(p.serial_out(), "stop bit of $4F on L+939");
        assert_ne!(irqst(&p) & IRQ_NEEDED, 0, "IRQST bit 4 cleared");
        p.tick();
        assert_eq!(p.cycle(), l + FRAME, "cycle L+940");
        assert!(!p.serial_out(), "start bit of $55 on L+940");
        assert_eq!(irqst(&p) & IRQ_NEEDED, 0, "IRQST bit 4 on L+940");
    }

    /// Verifies REQ-525: a second `SEROUT` write before the load replaces the first.
    #[test]
    fn serout_overwrite_before_load() {
        let mut p = serial_pokey(0x10);
        let _load = send(&mut p, &[0x4F, 0x55]);
        assert_eq!(levels(&mut p, FRAME), frame_levels(0x55), "frame of $55");
        assert_eq!(
            p.take_serial_byte().map(|byte| byte.value),
            Some(0x55),
            "sent byte"
        );
        assert_eq!(p.take_serial_byte(), None, "only one byte");
    }

    /// Verifies REQ-525: without a new `SEROUT` write, the end of a frame latches no IRQ.
    #[test]
    fn no_ready_irq_at_end() {
        let mut p = serial_pokey(0x10);
        let (_, l) = send(&mut p, &[0x4F]);
        clear_irq(&mut p, 0x10, IRQ_NEEDED);
        run_checking_to(&mut p, l + 2000, "IRQST bit 4 reads 1", |p| {
            irqst(p) & IRQ_NEEDED != 0
        });
    }

    /// Verifies REQ-525: `take_serial_byte` gives the byte after its stop bit, with the
    /// cycles of the frame.
    #[test]
    fn serial_byte_reported_with_cycle() {
        let mut p = serial_pokey(0x10);
        let (_, l) = send(&mut p, &[0x4F]);
        while p.cycle() < l + FRAME - 1 {
            assert_eq!(p.take_serial_byte(), None, "cycle {}", p.cycle());
            p.tick();
        }
        assert_eq!(p.take_serial_byte(), None, "cycle L+939");
        p.tick();
        assert_eq!(
            p.take_serial_byte(),
            Some(super::SerialByte {
                value: 0x4F,
                start_cycle: l,
                end_cycle: l + FRAME,
            }),
            "on L+940"
        );
        assert_eq!(p.take_serial_byte(), None, "then none");
    }

    /// Verifies REQ-516: after power-on, `IRQST` bit 3 reads 0 and the IRQ line is off.
    #[test]
    fn serial_complete_idle_no_irq() {
        let p = new_pokey();
        assert_eq!(irqst(&p) & IRQ_DONE, 0, "IRQST bit 3");
        assert!(!p.irq(), "IRQ line");
    }

    /// Verifies REQ-516: with the output idle, `IRQEN` bit 3 asserts the IRQ at once.
    #[test]
    fn serial_complete_irq_when_enabled() {
        let mut p = serial_pokey(0x00);
        p.tick();
        p.write(IRQEN, IRQ_DONE);
        run_checking(&mut p, 100, "IRQ with the output idle", |p| {
            p.irq() && irqst(p) & IRQ_DONE == 0
        });
    }

    /// Verifies REQ-516: `IRQST` bit 3 reads 1 while the shift register sends, also over two
    /// frames.
    #[test]
    fn serial_complete_reads_1_while_shifting() {
        for frames in [1, 2] {
            let mut p = serial_pokey(0x10);
            let (_, l) = send(&mut p, &[0x4F]);
            if frames == 2 {
                run(&mut p, 10);
                p.write(SEROUT, 0x55);
            }
            run_checking_to(&mut p, l + frames * FRAME - 1, "bit 3 = 1", |p| {
                irqst(p) & IRQ_DONE != 0
            });
            p.tick();
            assert_eq!(irqst(&p) & IRQ_DONE, 0, "bit 3 = 0 after {frames} frames");
        }
    }

    /// Verifies REQ-516: `IRQEN` writes do not change `IRQST` bit 3.
    #[test]
    fn irqen_does_not_clear_bit_3() {
        let mut p = serial_pokey(0x10);
        for value in [0x00, 0xF7] {
            p.write(IRQEN, value);
            assert_eq!(irqst(&p) & IRQ_DONE, 0, "idle, IRQEN = {value:02X}");
        }
        p.write(IRQEN, 0x10);
        run(&mut p, 10);
        let _load = send(&mut p, &[0x4F]);
        for value in [0x00, 0xF7] {
            p.write(IRQEN, value);
            assert_eq!(
                irqst(&p) & IRQ_DONE,
                IRQ_DONE,
                "shifting, IRQEN = {value:02X}"
            );
        }
    }

    /// Verifies REQ-516: with `IRQEN` = `$08`, the IRQ line is off while the frame shifts
    /// and on from its end.
    #[test]
    fn serial_complete_after_last_stop_bit() {
        let mut p = serial_pokey(IRQ_DONE);
        let (_, l) = send(&mut p, &[0x4F]);
        assert!(!p.irq(), "IRQ off on L");
        run_checking_to(&mut p, l + FRAME - 1, "IRQ off while shifting", |p| {
            !p.irq()
        });
        run_checking(&mut p, 100, "IRQ on from L+940", Pokey::irq);
    }

    /// Verifies REQ-502: initialization mode drops the frame and the pending `SEROUT`, and
    /// keeps SIO DATA OUT.
    #[test]
    fn init_mode_flushes_serout() {
        let mut p = serial_pokey(0x10);
        let _load = send(&mut p, &[0x4F]);
        p.write(SEROUT, 0x55);
        run(&mut p, 200);
        let level = p.serial_out();
        p.write(SKCTL, 0x00);
        run(&mut p, 1000);
        p.write(SKCTL, 0x23);
        run_checking(&mut p, 5000, "no output after initialization mode", |p| {
            p.serial_out() == level && irqst(p) & IRQ_DONE == 0
        });
        assert_eq!(p.take_serial_byte(), None, "no byte");
    }
}
