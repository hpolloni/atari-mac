//! The `minifb` window loop (REQ-008).
//!
//! The loop only moves data between the window and [`Frontend`]. All logic is in
//! [`crate::frontend`] and [`crate::keymap`], where unit tests reach it.

use std::thread;
use std::time::{Duration, Instant};

use minifb::{Window, WindowOptions};

use crate::error::FrontendError;
use crate::frontend::{DisplayFilters, Flow, Frontend, Pacing, render, window_size};

/// Shows the machine in a window until the user quits or `frames` frames have run.
/// Returns the time from the first frame to the end. The time to open the window is not
/// part of it (REQ-007).
///
/// `filters` are the display filters of the window (REQ-019). While a BASIC program is
/// typed, the frames run in turbo ([`Pacing`], REQ-080).
///
/// # Errors
///
/// [`FrontendError::Window`] when the window cannot be opened or updated, and the errors of
/// [`Frontend::step`].
pub fn run(
    frontend: &mut Frontend,
    title: &str,
    scale: usize,
    filters: DisplayFilters,
    frames: Option<u64>,
) -> Result<Duration, FrontendError> {
    let (width, height) = window_size(scale);
    let mut window = Window::new(title, width, height, WindowOptions::default())
        .map_err(FrontendError::Window)?;
    // The pacer sets the frame rate, not minifb.
    window.set_target_fps(0);
    let standard = frontend.machine().standard();
    let mut buffer = Vec::with_capacity(width * height);
    // Show the window before the first frame: the first update takes much longer.
    render(
        frontend.machine().frame(),
        standard,
        filters,
        scale,
        &mut buffer,
    );
    window
        .update_with_buffer(&buffer, width, height)
        .map_err(FrontendError::Window)?;
    let start = Instant::now();
    let mut pacing = Pacing::new(standard, start);
    while window.is_open()
        && frames.is_none_or(|n| frontend.frames_run() < n)
        && frontend.step(&window.get_keys())? == Flow::Continue
    {
        let after = pacing.after_frame(frontend.typist().is_some(), Instant::now());
        if after.present {
            render(
                frontend.machine().frame(),
                standard,
                filters,
                scale,
                &mut buffer,
            );
            window
                .update_with_buffer(&buffer, width, height)
                .map_err(FrontendError::Window)?;
        }
        thread::sleep(after.wait);
    }
    Ok(start.elapsed())
}
