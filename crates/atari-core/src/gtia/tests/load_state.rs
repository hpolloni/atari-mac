//! Tests of the check of a loaded GTIA state (REQ-061).
//!
//! Each test sets one value of a new chip exactly at its limit and one step past or inside
//! it, and calls `loaded_is_valid` directly.

use super::{FRAME_HEIGHT, FRAME_WIDTH, Gtia};
use crate::gtia::PENDING;
use crate::state::TIME_LIMIT;
use crate::timing::VideoStandard;

/// `loaded_is_valid` of a new NTSC chip after `change`, against a new NTSC chip as the
/// state before the load.
fn valid_after(change: impl FnOnce(&mut Gtia)) -> bool {
    let config = Gtia::new(VideoStandard::Ntsc);
    let mut gtia = config.clone();
    change(&mut gtia);
    gtia.loaded_is_valid(&config)
}

/// Verifies REQ-061: a new chip is a valid loaded state, and a state of the other TV
/// standard is not.
#[test]
fn gtia_load_check_standard() {
    assert!(valid_after(|_| {}), "new chip");
    assert!(
        !valid_after(|g| g.standard = VideoStandard::Pal),
        "other standard"
    );
}

/// Verifies REQ-061: the pending write count may fill the queue, but not more.
#[test]
fn gtia_load_check_pending() {
    assert!(valid_after(|g| g.pending_len = PENDING), "full queue");
    assert!(
        !valid_after(|g| g.pending_len = PENDING + 1),
        "past the queue"
    );
}

/// Verifies REQ-061: the color clock count must be below `TIME_LIMIT`.
#[test]
fn gtia_load_check_time_limit() {
    assert!(valid_after(|g| g.clock = TIME_LIMIT - 1), "below the limit");
    assert!(!valid_after(|g| g.clock = TIME_LIMIT), "at the limit");
}

/// Verifies REQ-061: each frame buffer must have the frame size.
#[test]
fn gtia_load_check_frame_size() {
    let size = FRAME_WIDTH * FRAME_HEIGHT;
    assert!(
        !valid_after(|g| g.back.truncate(size - 1)),
        "short back buffer"
    );
    assert!(!valid_after(|g| g.front.push(0)), "long front buffer");
}
