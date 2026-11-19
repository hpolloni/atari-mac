//! `atari-mac` binary entry point.

use std::io::{self, Write as _};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut stdout = io::stdout().lock();
    match writeln!(stdout, "{}", atari_mac::greeting()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
