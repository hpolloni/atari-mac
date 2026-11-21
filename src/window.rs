//! The `minifb` window loop (REQ-008).
//!
//! The loop only moves data between the window and [`Frontend`]. All logic is in
//! [`crate::frontend`] and [`crate::keymap`], where unit tests reach it.
//!
//! The frames run on an emulation thread, and the window runs on the main thread (macOS
//! needs the window on the main thread). A window update waits for the display, and macOS
//! can stop it for up to 1 s. On its own thread, the
//! emulation keeps the frame rate (REQ-007) and gives the audio samples in time (REQ-011)
//! while the window waits.

use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use minifb::{Key, Window, WindowOptions};

use crate::audio::{AudioOutput, AudioSink};
use crate::cli::Options;
use crate::error::FrontendError;
use crate::frontend::{Flow, Frontend, Pacing, render, window_size};

/// The time that the window loop waits when it has no new frame to show.
const IDLE: Duration = Duration::from_millis(2);

/// The data between the window thread and the emulation thread.
#[derive(Debug, Default)]
struct Shared {
    /// The host keys that are down, from the window.
    keys: Mutex<Vec<Key>>,
    /// The last frame to show (frame buffer color bytes), when it is new.
    frame: Mutex<Option<Vec<u8>>>,
    /// The window is closed or failed: the emulation thread stops.
    stop: AtomicBool,
}

/// Locks `mutex`. The data stays valid when the other thread panics.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Shows the machine in a window until the user quits or `options.frames` frames have run.
/// Returns the time from the first frame to the end. The time to open the window is not
/// part of it (REQ-007).
///
/// `options.scale` is the window scale, and `options.filters` are the display filters of the
/// window (REQ-019). While a BASIC program is typed, the frames run in turbo ([`Pacing`],
/// REQ-080).
///
/// After each frame, the audio samples of the frame go to `audio`, and the level of its
/// ring corrects the wait (REQ-011). While the emulation is paused or in turbo, the sound
/// stops.
///
/// # Errors
///
/// [`FrontendError::Window`] when the window cannot be opened or updated, and the errors of
/// [`Frontend::step`].
pub fn run(
    frontend: &mut Frontend,
    title: &str,
    options: &Options,
    audio: &AudioOutput,
) -> Result<Duration, FrontendError> {
    let Options {
        scale,
        filters,
        frames,
        ..
    } = *options;
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
    let shared = Shared::default();
    *lock(&shared.keys) = window.get_keys();
    let sink = audio.sink();
    thread::scope(|scope| {
        let emulation = scope.spawn(|| emulate(frontend, &shared, &sink, frames));
        let mut shown = Ok(());
        while window.is_open() && !emulation.is_finished() {
            let frame = lock(&shared.frame).take();
            shown = if let Some(frame) = frame {
                render(&frame, standard, filters, scale, &mut buffer);
                window.update_with_buffer(&buffer, width, height)
            } else {
                window.update();
                thread::sleep(IDLE);
                Ok(())
            };
            if shown.is_err() {
                break;
            }
            *lock(&shared.keys) = window.get_keys();
        }
        shared.stop.store(true, Ordering::Relaxed);
        let elapsed = emulation
            .join()
            .unwrap_or_else(|panic| panic::resume_unwind(panic))?;
        shown.map_err(FrontendError::Window)?;
        Ok(elapsed)
    })
}

/// The emulation thread: runs the frames at real-time speed (or in turbo while a BASIC
/// program is typed) until the user quits, `frames` frames have run or the window stops.
/// Returns the time from the first frame to the end.
#[expect(
    clippy::disallowed_methods,
    reason = "the frontend owns the host clock; the core reads no clock (REQ-014)"
)]
fn emulate(
    frontend: &mut Frontend,
    shared: &Shared,
    audio: &AudioSink,
    frames: Option<u64>,
) -> Result<Duration, FrontendError> {
    let start = Instant::now();
    let mut pacing = Pacing::new(frontend.machine().standard(), start);
    let mut keys = Vec::new();
    while !shared.stop.load(Ordering::Relaxed) && frames.is_none_or(|n| frontend.frames_run() < n) {
        keys.clone_from(&lock(&shared.keys));
        if frontend.step(&keys)? == Flow::Quit {
            break;
        }
        let turbo = frontend.typist().is_some();
        if turbo || frontend.paused() {
            audio.stop();
        } else {
            audio.push(frontend.audio());
            pacing.correct(audio.correction());
        }
        let after = pacing.after_frame(turbo, Instant::now());
        if after.present {
            let mut frame = Vec::with_capacity(frontend.machine().frame().len());
            frame.extend_from_slice(frontend.machine().frame());
            *lock(&shared.frame) = Some(frame);
        }
        thread::sleep(after.wait);
    }
    Ok(start.elapsed())
}
