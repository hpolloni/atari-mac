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
//! | [`logging`] | The log output of `--log` (REQ-085) |
//! | [`trace`] | The CPU instruction trace of `--trace` (REQ-086) |
//! | [`stats`] | The speed statistics of `--stats` (REQ-083) |
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
pub mod logging;
pub mod stats;
pub mod trace;
pub mod window;

use std::ffi::OsString;
use std::io::{self, Write};
use std::time::{Duration, Instant};

pub use atari_core;

use crate::audio::{AudioOutput, DEFAULT_SAMPLE_RATE};
use crate::bas::Typist;
use crate::cli::{Command, Options, USAGE};
use crate::error::FrontendError;
use crate::frontend::{Frontend, load_machine, load_xex, window_title, write_png, xex_warnings};
use crate::stats::{Counters, FrameStats, Motor, Stats};
use crate::trace::TraceWriter;

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
    let mut frontend = Frontend::new(load_machine(options, sample_rate)?);
    if options.auto_return {
        frontend = frontend.with_auto_return();
    }
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
///
/// With `--log`, the logger is installed before the machine is created (REQ-085). Without
/// it, the frontend installs no logger, and the maximum level of `log` stays `Off`.
#[expect(
    clippy::disallowed_methods,
    reason = "the frontend owns the host clock; the core reads no clock (REQ-014)"
)]
fn run_machine(options: &Options, out: &mut impl Write) -> Result<(), FrontendError> {
    if let Some(categories) = options.log {
        logging::install(categories);
    }
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
    if let Some(path) = &options.xex {
        for warning in xex_warnings(&load_xex(path)?) {
            writeln!(
                io::stderr(),
                "atari-mac: warning: {}: {warning}",
                path.display()
            )
            .map_err(FrontendError::Output)?;
        }
    }
    let mut frontend = start_at(options, audio.sample_rate())?;
    if let Some(trace) = &options.trace {
        let output = trace.open().map_err(|source| FrontendError::Trace {
            path: trace.file.clone(),
            source,
        })?;
        frontend = frontend.with_trace(TraceWriter::new(output, trace.from, trace.frames));
    }
    let elapsed = match options.frames {
        Some(frames) if options.headless && options.stats => {
            run_headless_with_stats(&mut frontend, frames)?
        }
        Some(frames) if options.headless => {
            let start = Instant::now();
            frontend.run_frames(frames)?;
            start.elapsed()
        }
        _ => window::run(
            &mut frontend,
            &window_title(
                options
                    .cart
                    .as_deref()
                    .or(options.xex.as_deref())
                    .or(options.disk.as_deref())
                    .or(options.cas.as_deref())
                    .or(options.bas.as_deref()),
            ),
            options,
            &audio,
        )?,
    };
    // The trace is flushed first: with `--trace -` the lines come before the other output on
    // standard output (REQ-086 criterion 7).
    if let (Some(trace), Some(writer)) = (&options.trace, frontend.take_trace()) {
        writer.finish().map_err(|source| FrontendError::Trace {
            path: trace.file.clone(),
            source,
        })?;
    }
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

/// The headless loop with `--stats` (REQ-083): runs `frames` frames one at a time, measures
/// the emulation of each frame, and writes a statistics line to standard error after each
/// second of host time. `shown` and `underruns` are 0 (no window and no audio). Returns the
/// time from the first frame to the end.
///
/// # Errors
///
/// The errors of [`Frontend::run_frames`], and [`FrontendError::Output`] when standard
/// error cannot be written.
#[expect(
    clippy::disallowed_methods,
    reason = "the frontend owns the host clock; the core reads no clock (REQ-014)"
)]
fn run_headless_with_stats(
    frontend: &mut Frontend,
    frames: u64,
) -> Result<Duration, FrontendError> {
    let start = Instant::now();
    let machine = frontend.machine();
    let mut stats = Stats::new(
        machine.standard(),
        start,
        Counters::default(),
        Motor::of(machine),
    );
    for _ in 0..frames {
        let t0 = Instant::now();
        frontend.run_frames(1)?;
        let t1 = Instant::now();
        stats.frame(FrameStats {
            emulation: t1 - t0,
            turbo: frontend.turbo(),
            motor: Motor::of(frontend.machine()),
        });
        if let Some(line) = stats.tick(t1, Counters::default()) {
            writeln!(io::stderr(), "{line}").map_err(FrontendError::Output)?;
        }
    }
    Ok(start.elapsed())
}
