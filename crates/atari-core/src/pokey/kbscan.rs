//! The keyboard scan state machine of HRM 5.8 Keyboard scan (Scan algorithm).
//!
//! Design: `docs/design/pokey.md`, "Keyboard scan". Requirements: REQ-518 to REQ-522.

use crate::state::{impl_state, impl_state_enum};

/// The scan codes on which the scan checks CONTROL, SHIFT and BREAK (HRM 5.8
/// Control/Shift/Break scan).
const CONTROL_CODE: u8 = 0x00;
/// See [`CONTROL_CODE`].
const SHIFT_CODE: u8 = 0x10;
/// See [`CONTROL_CODE`].
const BREAK_CODE: u8 = 0x30;

/// The levels of the keyboard lines in one scan pulse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScanInput {
    /// The keys that are down (bit n = key code n).
    pub(super) matrix: u64,
    /// SHIFT is down.
    pub(super) shift: bool,
    /// CONTROL is down.
    pub(super) control: bool,
    /// BREAK is down.
    pub(super) brk: bool,
}

impl_state!(ScanInput {
    matrix,
    shift,
    control,
    brk
});

/// The results of one scan pulse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct ScanEvent {
    /// A key press registered with this `KBCODE` value.
    pub(super) key: Option<u8>,
    /// The scan found the BREAK key changed from up to down.
    pub(super) brk: bool,
}

/// The four states of the scan state machine (the HRM numbers are in brackets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// State 0: key up.
    Up,
    /// State 1: debounce key down.
    DebounceDown,
    /// State 3: confirmed key down.
    Down,
    /// State 2: debounce key up.
    DebounceUp,
}

impl_state_enum!(State {
    Up,
    DebounceDown,
    Down,
    DebounceUp
});

/// The keyboard scan: the 6-bit counter, the compare register, the state machine and the
/// modifier latches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the four flags are independent latches of the HRM 5.8 scan hardware"
)]
pub(super) struct KeyboardScan {
    /// Scan counter, 0-63.
    counter: u8,
    /// Compare register.
    compare: u8,
    /// Scan state.
    state: State,
    /// SHIFT level latched at code `$10`.
    shift_latch: bool,
    /// CONTROL level latched at code `$00`.
    ctrl_latch: bool,
    /// `SKSTAT` bit 2 is 0.
    key_down: bool,
    /// BREAK level at the last check.
    brk_prev: bool,
}

impl_state!(KeyboardScan {
    counter,
    compare,
    state,
    shift_latch,
    ctrl_latch,
    key_down,
    brk_prev,
});

impl KeyboardScan {
    /// `true` if a loaded state has a scan counter of 0-63 (REQ-061).
    pub(super) const fn loaded_is_valid(self) -> bool {
        self.counter <= 63
    }

    /// The power-on state.
    pub(super) const fn new() -> Self {
        Self {
            counter: 0,
            compare: 0,
            state: State::Up,
            shift_latch: false,
            ctrl_latch: false,
            key_down: false,
            brk_prev: false,
        }
    }

    /// The latched SHIFT level (`SKSTAT` bit 3 is 0 while it is true).
    pub(super) const fn shift_latch(self) -> bool {
        self.shift_latch
    }

    /// The key-down flag (`SKSTAT` bit 2 is 0 while it is true).
    pub(super) const fn key_down(self) -> bool {
        self.key_down
    }

    /// The scan is disabled (`SKCTL` bit 1 is 0, HRM 5.8 Keyboard scan enable): the state
    /// goes to 0, the counter is held at 0 and the key-down flag clears. The latches keep
    /// their values.
    pub(super) const fn disable(&mut self) {
        self.state = State::Up;
        self.counter = 0;
        self.key_down = false;
    }

    /// One keyboard scan pulse. With `debounce` false (`SKCTL` bit 0 is 0), each compare
    /// passes.
    pub(super) const fn pulse(&mut self, input: ScanInput, debounce: bool) -> ScanEvent {
        let code = self.counter;
        let down = (input.matrix >> code) & 1 != 0;
        let mut event = ScanEvent {
            key: None,
            brk: false,
        };
        match code {
            CONTROL_CODE => self.ctrl_latch = input.control,
            SHIFT_CODE => self.shift_latch = input.shift,
            BREAK_CODE => {
                event.brk = input.brk && !self.brk_prev;
                self.brk_prev = input.brk;
            }
            _ => {}
        }
        let matched = !debounce || code == self.compare;
        self.state = match (self.state, matched, down) {
            (State::Up, _, true) => {
                self.compare = code;
                State::DebounceDown
            }
            (State::DebounceDown, true, true) => {
                let mut kbcode = code;
                if self.shift_latch {
                    kbcode |= 0x40;
                }
                if self.ctrl_latch {
                    kbcode |= 0x80;
                }
                event.key = Some(kbcode);
                self.key_down = true;
                State::Down
            }
            (State::DebounceDown, true, false) | (State::DebounceDown, false, true) => State::Up,
            (State::Down, true, false) => State::DebounceUp,
            (State::DebounceUp, true, false) => {
                self.key_down = false;
                State::Up
            }
            // Design open issue 6: the key is down again on a compare match.
            (State::DebounceUp, true, true) => State::Down,
            (state, _, _) => state,
        };
        self.counter = (code + 1) & 63;
        event
    }
}

#[cfg(test)]
impl KeyboardScan {
    /// Test constructor: the power-on state with the scan counter `counter`.
    pub(super) const fn with_counter(counter: u8) -> Self {
        Self {
            counter,
            ..Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::KeyboardScan;

    /// Verifies REQ-061: a loaded scan counter of 63 is valid, and 64 is not.
    #[test]
    fn kbscan_load_check_counter() {
        assert!(KeyboardScan::new().loaded_is_valid(), "power-on state");
        assert!(KeyboardScan::with_counter(63).loaded_is_valid(), "63");
        assert!(!KeyboardScan::with_counter(64).loaded_is_valid(), "64");
    }
}
