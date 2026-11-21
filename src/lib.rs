//! `atari-mac`: the macOS frontend of the Atari 65XE emulator.
//!
//! Design: `docs/design/architecture.md`, "Frontend". The modules:
//!
//! | Module | Contents |
//! | --- | --- |
//! | [`audio`] | The audio output: the ring buffer, the frame wait correction, the `cpal` stream (REQ-011) |
//! | [`cli`] | The command line |
//! | [`bas`] | A BASIC program from a text file, typed at the `Ready` prompt (REQ-080) |
//! | [`keymap`] | Host key to Atari key (REQ-009, REQ-010, REQ-017) |
//! | [`frontend`] | Machine setup, input, pause, frame conversion, pacing, PNG screenshot |
//! | [`window`] | The `minifb` window loop (REQ-008) |
//! | [`error`] | The errors and the exit status |

#![expect(
    clippy::multiple_crate_versions,
    reason = "bitflags 1 comes only from sdl2, which orbclient uses on non-Redox hosts; minifb uses orbclient only on Redox, so it is never built (cargo-deny checks only the real targets)"
)]

pub mod audio;
pub mod bas;
pub mod cli;
pub mod error;
pub mod frontend;
pub mod keymap;
#[cfg(test)]
mod planned;
pub mod window;

use std::ffi::OsString;
use std::io::{self, Write};
use std::time::Instant;

pub use atari_core;

use crate::audio::{AudioOutput, DEFAULT_SAMPLE_RATE};
use crate::bas::Typist;
use crate::cli::{Command, Options, USAGE};
use crate::error::FrontendError;
use crate::frontend::{Frontend, load_machine, window_title, write_png};

/// Runs the frontend with the command line arguments `args` (without the program name).
/// Normal output goes to `out`.
///
/// # Errors
///
/// Any [`FrontendError`]. [`FrontendError::exit_code`] gives the exit status.
pub fn run(
    args: impl IntoIterator<Item = OsString>,
    out: &mut impl Write,
) -> Result<(), FrontendError> {
    match cli::parse(args)? {
        Command::Help => writeln!(out, "{USAGE}").map_err(FrontendError::Output),
        Command::Run(options) => run_machine(&options, out),
    }
}

/// Creates the frontend for `options` with the audio sample rate [`DEFAULT_SAMPLE_RATE`]
/// ([`start_at`]).
///
/// # Errors
///
/// The errors of [`start_at`].
pub fn start(options: &Options) -> Result<Frontend, FrontendError> {
    start_at(options, DEFAULT_SAMPLE_RATE)
}

/// Creates the frontend for `options` with the audio sample rate `sample_rate`: reads and
/// checks the BASIC program file of `--bas` first (REQ-080 criteria 4 and 5), then creates
/// the machine.
///
/// # Errors
///
/// The errors of [`bas::load`] and [`load_machine`].
pub fn start_at(options: &Options, sample_rate: u32) -> Result<Frontend, FrontendError> {
    let codes = options
        .bas
        .as_deref()
        .map(|path| bas::load(path, !options.no_run))
        .transpose()?;
    let frontend = Frontend::new(load_machine(options, sample_rate)?);
    Ok(match codes {
        Some(codes) => frontend.with_typist(Typist::new(codes)),
        None => frontend,
    })
}

/// Runs the machine with the window or headless, then writes the elapsed time, the audio
/// numbers (`--audio-stats`) and the screenshot.
///
/// The window opens the audio stream on the default output device before it creates the
/// machine, so that the machine makes samples at the rate of the stream. Without a device,
/// a warning goes to standard error and the emulator runs without sound (REQ-011).
#[expect(
    clippy::disallowed_methods,
    reason = "the frontend owns the host clock; the core reads no clock (REQ-014)"
)]
fn run_machine(options: &Options, out: &mut impl Write) -> Result<(), FrontendError> {
    let audio = if options.headless {
        AudioOutput::off("--headless")
    } else {
        let audio = AudioOutput::open_default();
        if let Some(reason) = audio.error() {
            writeln!(io::stderr(), "atari-mac: warning: no sound: {reason}")
                .map_err(FrontendError::Output)?;
        }
        audio
    };
    let mut frontend = start_at(options, audio.sample_rate())?;
    let elapsed = match options.frames {
        Some(frames) if options.headless => {
            let start = Instant::now();
            frontend.run_frames(frames)?;
            start.elapsed()
        }
        _ => window::run(
            &mut frontend,
            &window_title(options.cart.as_deref().or(options.bas.as_deref())),
            options,
            &audio,
        )?,
    };
    if options.frames.is_some() {
        let seconds = elapsed.as_secs_f64();
        writeln!(
            out,
            "{} frames in {seconds:.3} s ({}, last frame {} cycles)",
            frontend.frames_run(),
            cli::standard_name(frontend.machine().standard()),
            frontend.last_frame_cycles()
        )
        .map_err(FrontendError::Output)?;
    }
    if options.audio_stats {
        writeln!(out, "{}", audio.stats()).map_err(FrontendError::Output)?;
    }
    if let Some(path) = &options.screenshot {
        let machine = frontend.machine();
        write_png(path, machine.frame(), machine.standard(), options.filters)?;
    }
    Ok(())
}
