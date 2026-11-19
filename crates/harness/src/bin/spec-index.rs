//! Writes the generated specification files (`docs/design/spec-gates.md`):
//! `docs/spec/requirements/INDEX.md`, `docs/spec/open-items/README.md` and
//! `target/traceability.md`.
//!
//! Usage: `cargo run -p atari-harness --bin spec-index`.

use std::io::{self, Write as _};
use std::process::ExitCode;

use atari_harness::data::repo_root;
use atari_harness::spec;

fn main() -> ExitCode {
    match spec::generate(&repo_root()) {
        Ok(counts) => {
            let written = writeln!(io::stdout(), "{counts}");
            if written.is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            let _written = writeln!(io::stderr(), "spec-index: {error}");
            ExitCode::FAILURE
        }
    }
}
