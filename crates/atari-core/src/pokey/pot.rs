//! The pot scan of HRM 5.9 Paddle scan.
//!
//! Design: `docs/design/pokey.md`, "Pot scan". Requirements: REQ-534 to REQ-537.
//!
//! All inputs share one charge time: the dump transistors drain all capacitors together,
//! and all capacitors charge from the same moment. Each input has its own threshold.

use crate::state::{impl_state, impl_state_enum};

/// The count at which the scan counter stops (HRM 5.9 Polling mechanism).
const SCAN_END: u8 = 228;
/// Machine cycles of charge per slow-scan count (one scanline).
const CYCLES_PER_COUNT: u32 = 114;
/// The number of pot inputs that the host can connect (`POT0-POT3`, REQ-535).
const CONNECTED: usize = 4;

/// The phase of a running scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// No scan.
    Idle,
    /// The counter was reset on the `POTGO` write; it does not count on the next cycle.
    Reset,
    /// The counter counts.
    Counting,
    /// The scan stop was detected on the last cycle; `ALLPOT` goes to `$00` on this cycle.
    Stopping,
}

impl_state_enum!(Phase {
    Idle,
    Reset,
    Counting,
    Stopping
});

/// The pot scan counter, the pot registers, `ALLPOT` and the capacitor charge model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PotScan {
    /// The scan phase.
    phase: Phase,
    /// The scan counter.
    counter: u8,
    /// `POT0-7`.
    pot: [u8; 8],
    /// `ALLPOT`.
    allpot: u8,
    /// Machine cycles of charge since the last dump (the same for all inputs).
    charge: u32,
    /// The charge in cycles that input n must pass to reach threshold; `None` = never.
    threshold: [Option<u32>; CONNECTED],
}

impl_state!(PotScan {
    phase,
    counter,
    pot,
    allpot,
    charge,
    threshold
});

impl PotScan {
    /// The power-on state: no scan, all registers 0, no paddle (REQ-501).
    pub(super) const fn new() -> Self {
        Self {
            phase: Phase::Idle,
            counter: SCAN_END,
            pot: [0; 8],
            allpot: 0,
            charge: 0,
            threshold: [None; CONNECTED],
        }
    }

    /// `POTn` (`index` 0-7).
    pub(super) const fn pot(&self, index: usize) -> u8 {
        self.pot[index]
    }

    /// `ALLPOT`.
    pub(super) const fn allpot(&self) -> u8 {
        self.allpot
    }

    /// The counter counts on a pulse of this cycle (so the caller needs the pulse).
    pub(super) const fn counting(&self) -> bool {
        matches!(self.phase, Phase::Counting)
    }

    /// Input `index` 0..=3 reaches threshold at slow-scan count `count`; 228 or more means
    /// never (REQ-535). Other inputs are grounded, so the call ignores them.
    pub(super) const fn set_threshold(&mut self, index: usize, count: u8) {
        if index < CONNECTED {
            self.threshold[index] = if count < SCAN_END {
                Some(count as u32 * CYCLES_PER_COUNT)
            } else {
                None
            };
        }
    }

    /// Input `n` is above threshold with the present charge.
    const fn above(&self, n: usize) -> bool {
        if n >= CONNECTED {
            return false;
        }
        match self.threshold[n] {
            Some(t) => self.charge > t,
            None => false,
        }
    }

    /// A `POTGO` write: the counter resets and a scan starts. Each input below threshold
    /// gets `ALLPOT` bit = 1 and `POTn` = 0; an input above threshold keeps its value and
    /// gets bit = 0 (HRM 5.9 Truncated scans).
    pub(super) const fn potgo(&mut self) {
        self.phase = Phase::Reset;
        self.counter = 0;
        let mut allpot = 0;
        let mut n = 0;
        while n < 8 {
            if !self.above(n) {
                allpot |= 1 << n;
                self.pot[n] = 0;
            }
            n += 1;
        }
        self.allpot = allpot;
    }

    /// One machine cycle. `fast` is `SKCTL` bit 2. `pulse` is a 15 kHz pot counter pulse on
    /// this cycle (never in initialization mode).
    pub(super) const fn tick(&mut self, fast: bool, pulse: bool) {
        // The dump transistors are on in slow mode while no scan runs (HRM 5.9).
        if !fast && matches!(self.phase, Phase::Idle) {
            self.charge = 0;
            return;
        }
        self.charge = self.charge.saturating_add(1);
        match self.phase {
            Phase::Idle => return,
            Phase::Stopping => {
                // HRM 5.9 Scan timing: `ALLPOT` reads `$00` one cycle after the stop.
                self.allpot = 0;
                self.phase = Phase::Idle;
                return;
            }
            Phase::Reset => self.phase = Phase::Counting,
            Phase::Counting => {
                if self.counter >= SCAN_END {
                    // The stop is detected one cycle after the count of 228. In fast mode the
                    // counter runs over to 229 first (HRM 5.9 Scan timing).
                    if fast {
                        self.counter = SCAN_END + 1;
                    }
                    self.phase = Phase::Stopping;
                } else if fast || pulse {
                    self.counter += 1;
                }
            }
        }
        self.latch();
    }

    /// Each input with `ALLPOT` bit n = 1 takes the counter; then an input above threshold
    /// clears its bit, and the value of this cycle stays (HRM 5.9 Polling status).
    const fn latch(&mut self) {
        let mut n = 0;
        while n < 8 {
            let bit = 1 << n;
            if self.allpot & bit != 0 {
                self.pot[n] = self.counter;
                if self.above(n) {
                    self.allpot &= !bit;
                }
            }
            n += 1;
        }
    }
}
