//! `atari-mac`: the macOS frontend of the Atari 65XE emulator.
//!
//! Design: `docs/design/architecture.md`, "Frontend". The modules:
//!
//! | Module | Contents |
//! | --- | --- |
//! | [`cli`] | The command line |
//! | [`keymap`] | Host key to Atari key (REQ-009, REQ-010, REQ-017) |
//! | [`frontend`] | Machine setup, input, pause, frame conversion, pacing, PNG screenshot |
//! | [`window`] | The `minifb` window loop (REQ-008) |
//! | [`error`] | The errors and the exit status |

#![expect(
    clippy::multiple_crate_versions,
    reason = "bitflags 1 comes only from sdl2, which orbclient uses on non-Redox hosts; minifb uses orbclient only on Redox, so it is never built (cargo-deny checks only the real targets)"
)]

pub mod cli;
pub mod error;
pub mod frontend;
pub mod keymap;
#[cfg(test)]
mod planned;
pub mod window;

use std::ffi::OsString;
use std::io::Write;
use std::time::Instant;

pub use atari_core;

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

/// Runs the machine with the window or headless, then writes the elapsed time and the
/// screenshot.
fn run_machine(options: &Options, out: &mut impl Write) -> Result<(), FrontendError> {
    let mut frontend = Frontend::new(load_machine(options)?);
    let elapsed = match options.frames {
        Some(frames) if options.headless => {
            let start = Instant::now();
            frontend.run_frames(frames);
            start.elapsed()
        }
        frames => window::run(
            &mut frontend,
            &window_title(options.cart.as_deref()),
            options.scale,
            options.filters,
            frames,
        )?,
    };
    if options.frames.is_some() {
        let seconds = elapsed.as_secs_f64();
        writeln!(out, "{} frames in {seconds:.3} s", frontend.frames_run())
            .map_err(FrontendError::Output)?;
    }
    if let Some(path) = &options.screenshot {
        let machine = frontend.machine();
        write_png(path, machine.frame(), machine.standard(), options.filters)?;
    }
    Ok(())
}
