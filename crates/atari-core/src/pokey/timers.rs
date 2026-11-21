//! The four timer counters, their reloads and their fires (HRM 5.3, HRM 5.4).
//!
//! The unlinked timers (REQ-504), the linked pairs (REQ-505), `STIMER` (REQ-507), the
//! asynchronous receive hold of timers 3 and 4 (REQ-528) and the two-tone resync of timers 1
//! and 2 (REQ-533).
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

    /// The fires of cycle `now` (bit n = timer n). The caller runs `advance` for the same
    /// cycle after it: a two-tone resync (`resync`) between the two calls stops the count
    /// of timers 1 and 2 in this cycle.
    pub(super) fn take_fires(&mut self, now: u64) -> u8 {
        let mut fired = 0;
        for (n, slots) in self.fire_at.iter_mut().enumerate() {
            for slot in slots {
                if *slot == now {
                    *slot = NEVER;
                    fired |= 1 << n;
                }
            }
        }
        fired
    }

    /// The reloads and counts of cycle `now`. `clocked[n]` is true if timer n gets a count
    /// pulse in this cycle. `linked[0]` links timers 1 and 2, `linked[1]` links timers 3 and
    /// 4 (REQ-505). A reload reads `audf`. A timer in `held` (bit n = timer n) is held in
    /// reset: its counter is `audf`, and it does not count, reload or fire (the asynchronous
    /// receive mode, REQ-528).
    #[expect(
        clippy::too_many_arguments,
        reason = "the inputs of one timer cycle: the clock pulses, the links, AUDF and the hold"
    )]
    pub(super) fn advance(
        &mut self,
        now: u64,
        clocked: [bool; 4],
        linked: [bool; 2],
        audf: [u8; 4],
        held: u8,
    ) {
        for n in 0..4 {
            if held & (1 << n) != 0 {
                self.counter[n] = audf[n];
                self.reload_at[n] = NEVER;
                self.fire_at[n] = [NEVER; 2];
                self.link_at[n] = NEVER;
                continue;
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
        self.restart(0b1111, now);
    }

    /// The `STIMER` action for the timers in `mask` only (bit n = timer n). The start edge of
    /// the asynchronous receive mode restarts timers 3 and 4 this way (REQ-528).
    pub(super) fn restart(&mut self, mask: u8, now: u64) {
        for n in 0..4 {
            if mask & (1 << n) == 0 {
                continue;
            }
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

    /// The two-tone resync for a toggle by a fire visible on cycle `now` (REQ-533, HRM 5.6
    /// Two-tone resync timing): timers 1 and 2 reload on `now + 1`, two cycles after the
    /// reload of the timer that fired. Like `STIMER`, the reload is not a fire. A fire of
    /// timer 1 or 2 that would be visible on `now + 2` or later is cancelled, so a timer
    /// that fires up to one cycle after the resyncing timer still fires.
    pub(super) fn resync(&mut self, now: u64) {
        for n in 0..2 {
            for slot in &mut self.fire_at[n] {
                if *slot >= now + 2 {
                    *slot = NEVER;
                }
            }
            self.reload_at[n] = now + 1;
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
