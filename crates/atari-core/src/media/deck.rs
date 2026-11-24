//! The cassette deck: the tape motor and the tape signal on SIO DATA IN
//! (`docs/design/media.md`, "Cassette deck", "Tape signal" and "Motor").
//!
//! REQ-641 gives the motor and the tape position, REQ-642 the signal. The deck never reads a
//! clock: all times come from the `now` arguments (REQ-014). The tape position is the tape
//! time in machine cycles from the start of the tape.

use super::tape::{Tape, TapeBlock};
use crate::state::{Reader, State, StateError, TIME_LIMIT, Writer};
use crate::timing::VideoStandard;

/// The bits of one byte on the tape: start bit, 8 data bits and stop bit (HRM C.2).
const BITS_PER_BYTE: u64 = 10;

/// The tape times of one block, made at `insert` from the block and the machine clock.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockTimes {
    /// The tape time where the block starts: the start of its gap.
    start: u64,
    /// The tape time where the gap ends: the start bit of the first byte, or the first
    /// signal.
    data: u64,
    /// The tape time where the block ends: the start of the next block.
    end: u64,
    /// For a signal block: the tape time where each signal starts. Empty for a data block.
    edges: Vec<u64>,
}

/// A point of the tape that the machine event log reports (REQ-084).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapeMark {
    /// A data block starts its first start bit (after its gap).
    Record {
        /// The index of the block in [`Tape::blocks`], from 0.
        index: usize,
        /// The length of the block in bytes.
        len: usize,
        /// The baud rate of the block.
        baud: u16,
    },
    /// The end of the last block.
    End,
}

/// The cassette deck with its tape (REQ-641, REQ-642).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapeDeck {
    /// The inserted tape.
    tape: Option<Tape>,
    /// The video standard, for the machine clock `VideoStandard::cpu_clock_hz`.
    standard: VideoStandard,
    /// The tape times of each block of `tape`, in tape order. Made at `insert`.
    times: Vec<BlockTimes>,
    /// The tape time that the motor played before the current motor start.
    played: u64,
    /// The first cycle in which the tape moves, while the motor is on.
    running_since: Option<u64>,
}

/// `round(x)` for a positive `x`: a half rounds up (REQ-642).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is positive and far below 2^64 for each time of a CAS file"
)]
const fn round(x: f64) -> u64 {
    x.round().max(0.0) as u64
}

/// `n` as `f64`. Each count of a CAS file is far below 2^52, so the value is exact.
#[expect(
    clippy::cast_precision_loss,
    reason = "each count of a CAS file is far below 2^52"
)]
const fn exact(n: u64) -> f64 {
    n as f64
}

impl TapeDeck {
    /// `true` if a loaded state has a played time and a motor start below [`TIME_LIMIT`]
    /// (REQ-061).
    pub(crate) fn loaded_is_valid(&self) -> bool {
        self.played < TIME_LIMIT && self.running_since.is_none_or(|since| since < TIME_LIMIT)
    }

    /// A deck with no tape and the motor off, with the clock of `standard`.
    #[must_use]
    pub const fn new(standard: VideoStandard) -> Self {
        Self {
            tape: None,
            standard,
            times: Vec::new(),
            played: 0,
            running_since: None,
        }
    }

    /// Inserts `tape` at position 0 from cycle `now`. The motor state does not change.
    pub fn insert(&mut self, tape: Tape, now: u64) {
        self.times = Self::block_times(&tape, self.standard.cpu_clock_hz());
        self.tape = Some(tape);
        self.rewind(now);
    }

    /// Removes the tape and gives it back. The deck output is then mark.
    pub fn eject(&mut self) -> Option<Tape> {
        self.times.clear();
        self.tape.take()
    }

    /// The inserted tape.
    #[must_use]
    pub const fn tape(&self) -> Option<&Tape> {
        self.tape.as_ref()
    }

    /// The motor line is low (the motor is on), with or without a tape.
    #[must_use]
    pub const fn motor_on(&self) -> bool {
        self.running_since.is_some()
    }

    /// The motor line changed at the end of cycle `now`. `low` = motor on. The tape moves
    /// from cycle `now + 1`, or stops after cycle `now`.
    pub const fn set_motor_line(&mut self, low: bool, now: u64) {
        match (low, self.running_since) {
            (true, None) => self.running_since = Some(now + 1),
            (false, Some(since)) => {
                self.played += (now + 1).saturating_sub(since);
                self.running_since = None;
            }
            _ => {}
        }
    }

    /// The deck output in cycle `now` (`true` = mark). `true` with no tape, with the motor
    /// off and after the end of the tape.
    #[must_use]
    pub fn level(&self, now: u64) -> bool {
        match self.running_since {
            None => true,
            Some(_) if self.tape.is_none() => true,
            Some(_) => self.signal_at(self.position(now)),
        }
    }

    /// The tape position in cycle `now`: the tape time in machine cycles (REQ-641).
    #[must_use]
    pub const fn position(&self, now: u64) -> u64 {
        match self.running_since {
            Some(since) => self.played + now.saturating_sub(since),
            None => self.played,
        }
    }

    /// Back to position 0 from cycle `now` (cold reset, OI-052). The tape stays inserted.
    pub const fn rewind(&mut self, now: u64) {
        self.played = 0;
        if self.running_since.is_some() {
            self.running_since = Some(now);
        }
    }

    /// The next data record at tape time `p`, for the tape shortcut (REQ-644): the first
    /// data block whose gap ends at `p` or later, with its index in [`Tape::blocks`] and
    /// the tape time where it ends. `None` with no tape and when no such block remains.
    #[must_use]
    pub fn next_record(&self, p: u64) -> Option<(usize, u64)> {
        let tape = self.tape.as_ref()?;
        self.times
            .iter()
            .zip(tape.blocks())
            .enumerate()
            .find_map(|(index, (times, block))| match block {
                TapeBlock::Data { .. } if times.data >= p => Some((index, times.end)),
                TapeBlock::Data { .. } | TapeBlock::Signal { .. } => None,
            })
    }

    /// Moves the tape to position `p` in cycle `from` (the tape shortcut, REQ-644). With the
    /// motor on, the tape moves on from `p` after cycle `from`; with the motor off, it stays
    /// at `p`.
    pub const fn seek(&mut self, p: u64, from: u64) {
        self.played = p;
        if self.running_since.is_some() {
            self.running_since = Some(from);
        }
    }

    /// The tape signal at tape time `p` (REQ-642), independent of the motor: mark in a
    /// gap and after the last block, the bit cells of a data block, the signals of a signal
    /// block. `true` with no tape.
    #[must_use]
    pub fn signal_at(&self, p: u64) -> bool {
        let Some(tape) = &self.tape else {
            return true;
        };
        let index = self.times.partition_point(|times| times.start <= p);
        let Some(last) = index.checked_sub(1) else {
            return true;
        };
        let (Some(times), Some(block)) = (self.times.get(last), tape.blocks().get(last)) else {
            return true;
        };
        if p < times.data || p >= times.end {
            return true;
        }
        match block {
            TapeBlock::Data { baud, bytes, .. } => {
                let bit = self.bit_index(p - times.data, *baud);
                let byte = usize::try_from(bit / BITS_PER_BYTE)
                    .ok()
                    .and_then(|i| bytes.get(i))
                    .copied()
                    .unwrap_or(0xFF);
                match bit % BITS_PER_BYTE {
                    0 => false,
                    9 => true,
                    data_bit => (byte >> (data_bit - 1)) & 1 != 0,
                }
            }
            TapeBlock::Signal { .. } => {
                // The last signal that starts at or before `p`; signal 0 is a 0.
                let signal = times.edges.partition_point(|&edge| edge <= p);
                signal % 2 == 0
            }
        }
    }

    /// The block and the data byte at tape time `p`, for the event log (REQ-084): the index
    /// of the block in [`Tape::blocks`] and the index of the data byte (bit index / 10). The
    /// byte is 0 in the gap of a block and in a signal block. `None` with no tape. After the
    /// end of the tape: the number of blocks and 0.
    #[must_use]
    pub fn locate(&self, p: u64) -> Option<(usize, usize)> {
        let tape = self.tape.as_ref()?;
        let index = self.times.partition_point(|times| times.end <= p);
        let (Some(times), Some(block)) = (self.times.get(index), tape.blocks().get(index)) else {
            return Some((tape.blocks().len(), 0));
        };
        let byte = match block {
            TapeBlock::Data { baud, .. } if p >= times.data => {
                usize::try_from(self.bit_index(p - times.data, *baud) / BITS_PER_BYTE)
                    .unwrap_or(usize::MAX)
            }
            TapeBlock::Data { .. } | TapeBlock::Signal { .. } => 0,
        };
        Some((index, byte))
    }

    /// The first mark at tape time `p` or later, with its tape time, for the event log
    /// (REQ-084). `None` with no tape and after the end.
    #[must_use]
    pub fn next_mark(&self, p: u64) -> Option<(u64, TapeMark)> {
        let tape = self.tape.as_ref()?;
        let first = self.times.partition_point(|times| times.data < p);
        let record = self
            .times
            .iter()
            .zip(tape.blocks())
            .enumerate()
            .skip(first)
            .find_map(|(index, (times, block))| match block {
                TapeBlock::Data { baud, bytes, .. } if times.data >= p => Some((
                    times.data,
                    TapeMark::Record {
                        index,
                        len: bytes.len(),
                        baud: *baud,
                    },
                )),
                TapeBlock::Data { .. } | TapeBlock::Signal { .. } => None,
            });
        record.or_else(|| {
            let end = self.times.last()?.end;
            (end >= p).then_some((end, TapeMark::End))
        })
    }

    /// The bit `k` of a data block at the baud rate `baud` that holds the tape time `offset`
    /// after the gap: `start(k) <= offset < start(k + 1)` with
    /// `start(k) = round(k * f / baud)`.
    fn bit_index(&self, offset: u64, baud: u16) -> u64 {
        let clock_hz = self.standard.cpu_clock_hz();
        let baud = f64::from(baud.max(1));
        let start = |k: u64| round(exact(k) * clock_hz / baud);
        let mut k = round((exact(offset) * baud / clock_hz).floor());
        while k > 0 && start(k) > offset {
            k -= 1;
        }
        while start(k + 1) <= offset {
            k += 1;
        }
        k
    }

    /// The tape times of each block of `tape` with the machine clock `clock_hz` (REQ-642).
    /// Each edge is rounded from the start of its block, so the errors do not add up.
    fn block_times(tape: &Tape, clock_hz: f64) -> Vec<BlockTimes> {
        let mut start = 0_u64;
        let mut times = Vec::with_capacity(tape.blocks().len());
        for block in tape.blocks() {
            let (TapeBlock::Data { gap_ms, .. } | TapeBlock::Signal { gap_ms, .. }) = block;
            let data = start + round(f64::from(*gap_ms) * clock_hz / 1000.0);
            let (end, edges) = match block {
                TapeBlock::Data { baud, bytes, .. } => {
                    let bits = BITS_PER_BYTE * u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                    let end = data + round(exact(bits) * clock_hz / f64::from((*baud).max(1)));
                    (end, Vec::new())
                }
                TapeBlock::Signal { lengths, .. } => {
                    let mut sum = 0_u64;
                    let mut edges = Vec::with_capacity(lengths.len() + 1);
                    edges.push(data);
                    for &length in lengths {
                        sum += u64::from(length);
                        edges.push(data + round(exact(sum) * clock_hz / 10_000.0));
                    }
                    let end = edges.last().copied().unwrap_or(data);
                    // The last entry is the end of the block, not the start of a signal.
                    let _end = edges.pop();
                    (end, edges)
                }
            };
            times.push(BlockTimes {
                start,
                data,
                end,
                edges,
            });
            start = end;
        }
        times
    }

    /// The tape times where each block starts, and the end of the last block, for tests.
    #[must_use]
    pub fn block_starts(&self) -> Vec<u64> {
        let mut starts: Vec<u64> = self.times.iter().map(|times| times.start).collect();
        if let Some(last) = self.times.last() {
            starts.push(last.end);
        }
        starts
    }
}

// The tape, the played time and the motor. The block times come from the tape and the
// video standard, so a load makes them again.
impl State for TapeDeck {
    #[expect(
        clippy::unneeded_field_pattern,
        reason = "the pattern names each field, so a new field must be listed"
    )]
    fn save(&self, w: &mut Writer) {
        let Self {
            tape,
            standard: _,
            times: _,
            played,
            running_since,
        } = self;
        tape.is_some().save(w);
        if let Some(tape) = tape {
            tape.save(w);
        }
        played.save(w);
        running_since.save(w);
    }

    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        let Self {
            tape,
            standard,
            times,
            played,
            running_since,
        } = self;
        let mut some = false;
        some.load(r)?;
        *tape = if some {
            let mut loaded = tape.take().unwrap_or_else(Tape::empty);
            loaded.load(r)?;
            Some(loaded)
        } else {
            None
        };
        *times = tape.as_ref().map_or_else(Vec::new, |tape| {
            Self::block_times(tape, standard.cpu_clock_hz())
        });
        played.load(r)?;
        running_since.load(r)
    }
}

#[cfg(test)]
impl TapeDeck {
    /// Test constructor: an NTSC deck with no tape, the played time `played` and the motor
    /// start `running_since`.
    pub(crate) const fn with_times(played: u64, running_since: Option<u64>) -> Self {
        let mut deck = Self::new(VideoStandard::Ntsc);
        deck.played = played;
        deck.running_since = running_since;
        deck
    }
}

#[cfg(test)]
mod tests {
    use super::TapeDeck;
    use crate::state::TIME_LIMIT;

    /// Verifies REQ-061: the played time of a loaded deck must be below `TIME_LIMIT`.
    #[test]
    fn deck_load_check_played() {
        assert!(TapeDeck::with_times(0, None).loaded_is_valid(), "new deck");
        assert!(
            TapeDeck::with_times(TIME_LIMIT - 1, None).loaded_is_valid(),
            "below the limit"
        );
        assert!(
            !TapeDeck::with_times(TIME_LIMIT, None).loaded_is_valid(),
            "at the limit"
        );
    }

    /// Verifies REQ-061: the motor start of a loaded deck must be below `TIME_LIMIT`.
    #[test]
    fn deck_load_check_motor_start() {
        assert!(
            TapeDeck::with_times(0, Some(TIME_LIMIT - 1)).loaded_is_valid(),
            "below the limit"
        );
        assert!(
            !TapeDeck::with_times(0, Some(TIME_LIMIT)).loaded_is_valid(),
            "at the limit"
        );
    }
}
