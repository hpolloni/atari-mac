//! The `minifb` window loop (REQ-008).
//!
//! The loop only moves data between the window and [`Frontend`]. All logic is in
//! [`crate::frontend`] and [`crate::keymap`], where unit tests reach it.

use std::thread;
use std::time::{Duration, Instant};

use minifb::{Window, WindowOptions};

use crate::error::FrontendError;
use crate::frontend::{Flow, Frontend, Pacer, render, window_size};

/// Shows the machine in a window until the user quits or `frames` frames have run.
/// Returns the time from the first frame to the end. The time to open the window is not
/// part of it (REQ-007).
///
/// # Errors
///
/// [`FrontendError::Window`] when the window cannot be opened or updated.
pub fn run(
    frontend: &mut Frontend,
    title: &str,
    scale: usize,
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
    render(frontend.machine().frame(), standard, scale, &mut buffer);
    window
        .update_with_buffer(&buffer, width, height)
        .map_err(FrontendError::Window)?;
    let start = Instant::now();
    let mut pacer = Pacer::new(standard, start);
    while window.is_open()
        && frames.is_none_or(|n| frontend.frames_run() < n)
        && frontend.step(&window.get_keys()) == Flow::Continue
    {
        render(frontend.machine().frame(), standard, scale, &mut buffer);
        window
            .update_with_buffer(&buffer, width, height)
            .map_err(FrontendError::Window)?;
        thread::sleep(pacer.wait(Instant::now()));
    }
    Ok(start.elapsed())
}
