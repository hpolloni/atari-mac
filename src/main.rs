//! `atari-mac` binary entry point. See [`atari_mac::run`].

#![expect(
    clippy::multiple_crate_versions,
    reason = "bitflags 1 comes only from sdl2, which orbclient uses on non-Redox hosts; minifb uses orbclient only on Redox, so it is never built (cargo-deny checks only the real targets)"
)]

use std::env;
use std::io::{self, Write as _};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut stdout = io::stdout().lock();
    match atari_mac::run(env::args_os().skip(1), &mut stdout) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => writeln!(io::stderr(), "atari-mac: {error}")
            .map_or(ExitCode::FAILURE, |()| ExitCode::from(error.exit_code())),
    }
}
