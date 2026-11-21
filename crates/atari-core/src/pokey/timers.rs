//! The four timer counters, their reloads and their fires (HRM 5.3, HRM 5.4).
//!
//! SLICE-01 has the unlinked timers, which the `IRQEN`/`IRQST` logic needs (REQ-515), and
//! the linked pairs (REQ-505), which the serial clock of the OS SIO code needs. The two-tone
//! resync and the asynchronous receive hold come with the timer package.
//!
//! Design: `docs/design/pokey.md`, "Clocks and timers".

/// Cycles from an underflow, or from an `STIMER` write, to the reload.
const RELOAD_DELAY: u64 = 3;

/// Cycles from an underflow to the visible cycle of its fire.
const FIRE_DELAY: u64 = 4;

/// Cycles from an underflow of the low timer of a linked pair to the count of the high timer.
const LINK_DELAY: u64 = 3;

/// The cycle value of an event that is not scheduled. `tick` never runs this cycle, so a
/// slot with this value never matches.
const NEVER: u64 = u64::MAX;

/// The four timers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Timers {
    /// Timer counters.
    counter: [u8; 4],
    /// Cycle of the next scheduled reload, or `NEVER`. The counter does not decrement until
    /// then.
    reload_at: [u64; 4],
    /// Visible cycles of scheduled fires, or `NEVER`.
    fire_at: [[u64; 2]; 4],
    /// For the high timers 2 and 4 (index 1 and 3) of a linked pair: the cycle of the next
    /// count, 3 cycles after an underflow of the low timer, or `NEVER`.
    link_at: [u64; 4],
}

impl Timers {
    /// The power-on state.
    pub(super) const fn new() -> Self {
        Self {
            counter: [0; 4],
            reload_at: [NEVER; 4],
            fire_at: [[NEVER; 2]; 4],
            link_at: [NEVER; 4],
        }
    }

    /// The four counters.
    #[cfg(test)]
    pub(super) const fn counters(&self) -> [u8; 4] {
        self.counter
    }

    /// Runs cycle `now`. `clocked[n]` is true if timer n gets a count pulse in this cycle.
    /// `linked[0]` links timers 1 and 2, `linked[1]` links timers 3 and 4 (REQ-505). A reload
    /// reads `audf`. Returns the timers that fire on `now` (bit n = timer n).
    pub(super) fn tick(
        &mut self,
        now: u64,
        clocked: [bool; 4],
        linked: [bool; 2],
        audf: [u8; 4],
    ) -> u8 {
        let mut fired = 0;
        for n in 0..4 {
            let [first, second] = &mut self.fire_at[n];
            if *first == now {
                *first = NEVER;
                fired |= 1 << n;
            }
            if *second == now {
                *second = NEVER;
                fired |= 1 << n;
            }
            let pair_linked = linked[n / 2];
            let high = !n.is_multiple_of(2);
            let pulse = if pair_linked && high {
                let link = self.link_at[n] == now;
                if link {
                    self.link_at[n] = NEVER;
                }
                link
            } else {
                clocked[n]
            };
            if self.reload_at[n] == now {
                self.counter[n] = audf[n];
                self.reload_at[n] = NEVER;
            } else if pulse && self.reload_at[n] == NEVER {
                self.count(n, now, pair_linked);
            }
        }
        fired
    }

    /// One count pulse of timer `n` on cycle `now`.
    fn count(&mut self, n: usize, now: u64, pair_linked: bool) {
        if self.counter[n] != 0 {
            self.counter[n] -= 1;
            return;
        }
        self.counter[n] = 0xFF;
        self.schedule_fire(n, now + FIRE_DELAY);
        if !pair_linked {
            self.reload_at[n] = now + RELOAD_DELAY;
        } else if n.is_multiple_of(2) {
            // The low timer of a linked pair does not reload. It counts the high timer.
            self.link_at[n + 1] = now + LINK_DELAY;
        } else {
            // The high timer underflows: both timers of the pair reload.
            self.reload_at[n] = now + RELOAD_DELAY;
            self.reload_at[n - 1] = now + RELOAD_DELAY;
        }
    }

    /// An `STIMER` write on cycle `now` (REQ-507): cancels the fires that would be visible on
    /// `now + 4` or later, and reloads all four timers on `now + 3`.
    pub(super) fn stimer(&mut self, now: u64) {
        for n in 0..4 {
            for slot in &mut self.fire_at[n] {
                // `NEVER` is above each cycle, so a free slot stays free.
                if *slot >= now + FIRE_DELAY {
                    *slot = NEVER;
                }
            }
            self.reload_at[n] = now + RELOAD_DELAY;
            self.link_at[n] = NEVER;
        }
    }

    /// Schedules a fire of timer `n` with the visible cycle `at`.
    fn schedule_fire(&mut self, n: usize, at: u64) {
        if let Some(slot) = self.fire_at[n].iter_mut().find(|slot| **slot == NEVER) {
            *slot = at;
        }
    }
}
