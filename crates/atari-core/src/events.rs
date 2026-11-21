//! The `log` targets and the record macro of the machine event log (REQ-084,
//! `docs/design/architecture.md`, "Logging and statistics").
//!
//! Each record of the core has the target `atari::<category>`, the key `frame` (the frame
//! count at the event) and the key `cycle` (the machine cycle of the event). The `log`
//! macros compare the level first, so with the level `Off` the core formats nothing and
//! calls no logger function (criterion 5).
#![expect(
    clippy::redundant_pub_crate,
    reason = "the design makes the targets crate-private: other crates use the strings"
)]

/// The target of the tape records.
pub(crate) const TAPE: &str = "atari::tape";
/// The target of the SIO records.
pub(crate) const SIO: &str = "atari::sio";
/// The target of the reset records.
pub(crate) const RESET: &str = "atari::reset";
/// The target of the media records.
pub(crate) const MEDIA: &str = "atari::media";

/// Sends one record with the frame count and the cycle count (REQ-084 criterion 2).
/// `$level` is `info` or `debug`. `$frame` and `$cycle` are `u64` values.
macro_rules! record {
    ($level:ident, $target:expr, $frame:expr, $cycle:expr, $($message:tt)+) => {
        ::log::$level!(target: $target, frame = $frame, cycle = $cycle; $($message)+)
    };
}
pub(crate) use record;
