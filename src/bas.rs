//! Run a BASIC program from a text file (`--bas`, `--no-run`, REQ-080).
//!
//! Design: `docs/design/architecture.md`, "BASIC program from a text file". [`load`] reads
//! and checks the whole file before the machine starts, and gives the key codes to type.
//! [`Typist`] types them one frame at a time: the frontend calls [`Typist::before_frame`]
//! before each frame, so the window keeps showing frames while the program is typed.

use std::fs;
use std::path::Path;

use atari_core::Machine;
use atari_core::keyboard::{KEY_DOWN_FRAMES, KEY_UP_FRAMES, RETURN, ascii_to_kbcode};
use atari_core::screen::has_row;

use crate::error::FrontendError;

/// The prompt of BASIC (Altirra BASIC prints `Ready`).
pub const READY: &str = "Ready";

/// The frames that the typist waits for [`READY`] before it stops (REQ-080 criterion 6).
pub const READY_TIMEOUT_FRAMES: u32 = 600;

/// The text typed after the program, unless `--no-run`.
const RUN: &str = "RUN";

/// Converts one line of text to key codes, without RETURN.
fn line_codes(path: &Path, number: usize, line: &str) -> Result<Vec<u8>, FrontendError> {
    line.chars()
        .map(|character| {
            ascii_to_kbcode(character).ok_or_else(|| FrontendError::BasCharacter {
                path: path.to_owned(),
                line: number,
                character,
            })
        })
        .collect()
}

/// The key codes for the program `text` of the file `path`: each program line, then RETURN.
/// With `run`, `RUN` and RETURN follow.
///
/// A line ends with LF or CR LF. A line that is empty or has only spaces is not typed.
///
/// # Errors
///
/// [`FrontendError::BasCharacter`] for the first character without an Atari key, with the
/// line number (from 1).
pub fn program_codes(path: &Path, text: &str, run: bool) -> Result<Vec<u8>, FrontendError> {
    let mut codes = Vec::with_capacity(text.len() + RUN.len() + 1);
    for (index, line) in text.lines().enumerate() {
        if line.trim_matches(' ').is_empty() {
            continue;
        }
        codes.extend(line_codes(path, index + 1, line)?);
        codes.push(RETURN);
    }
    if run {
        codes.extend(line_codes(path, 0, RUN)?);
        codes.push(RETURN);
    }
    Ok(codes)
}

/// Reads the program file `path` and gives its key codes (see [`program_codes`]).
///
/// # Errors
///
/// [`FrontendError::Read`] when the file cannot be read or is not UTF-8 text, and
/// [`FrontendError::BasCharacter`] for a character without a key.
pub fn load(path: &Path, run: bool) -> Result<Vec<u8>, FrontendError> {
    let text = fs::read_to_string(path).map_err(|source| FrontendError::Read {
        path: path.to_owned(),
        source,
    })?;
    program_codes(path, &text, run)
}

/// What the typist does now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// It waits for the `Ready` prompt; the value is the number of frames that ran.
    WaitReady(u32),
    /// It types key `index`; `tick` is the frame within the key (down, then up).
    Typing {
        /// The index of the key code.
        index: usize,
        /// The frame within the key: `0..KEY_DOWN_FRAMES` down, then up.
        tick: u32,
    },
    /// All keys are typed.
    Done,
}

/// The `Ready` prompt did not show in [`READY_TIMEOUT_FRAMES`] frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotReady;

/// Types key codes on the machine, one frame at a time, with the key timing of the harness
/// `type_text` (REQ-025): each key down for [`KEY_DOWN_FRAMES`] frames, then up for
/// [`KEY_UP_FRAMES`] frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typist {
    /// The key codes to type.
    codes: Vec<u8>,
    /// The state.
    phase: Phase,
}

impl Typist {
    /// A typist that waits for `Ready`, then types `codes`.
    #[must_use]
    pub const fn new(codes: Vec<u8>) -> Self {
        Self {
            codes,
            phase: Phase::WaitReady(0),
        }
    }

    /// The state.
    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    /// `true` when all keys are typed.
    #[must_use]
    pub const fn done(&self) -> bool {
        matches!(self.phase, Phase::Done)
    }

    /// Called before each frame: presses or releases a key when it is time to.
    ///
    /// # Errors
    ///
    /// [`NotReady`] when [`READY`] is not on the screen after [`READY_TIMEOUT_FRAMES`]
    /// frames.
    pub fn before_frame(&mut self, machine: &mut Machine) -> Result<(), NotReady> {
        if let Phase::WaitReady(frames) = self.phase {
            if has_row(machine, READY) {
                self.phase = Phase::Typing { index: 0, tick: 0 };
            } else if frames >= READY_TIMEOUT_FRAMES {
                return Err(NotReady);
            } else {
                self.phase = Phase::WaitReady(frames + 1);
                return Ok(());
            }
        }
        if let Phase::Typing { index, tick } = self.phase {
            let Some(&code) = self.codes.get(index) else {
                self.phase = Phase::Done;
                return Ok(());
            };
            if tick == 0 {
                machine.key_down(code);
            } else if tick == KEY_DOWN_FRAMES {
                machine.key_up();
            }
            let tick = tick + 1;
            self.phase = if tick < KEY_DOWN_FRAMES + KEY_UP_FRAMES {
                Phase::Typing { index, tick }
            } else if index + 1 < self.codes.len() {
                Phase::Typing {
                    index: index + 1,
                    tick: 0,
                }
            } else {
                Phase::Done
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use atari_core::keyboard::{DIGIT_1, KEY_DOWN_FRAMES, KEY_UP_FRAMES, RETURN, ascii_to_kbcode};
    use atari_core::{Machine, MachineConfig, VideoStandard};

    use super::{NotReady, Phase, READY_TIMEOUT_FRAMES, Typist, program_codes};
    use crate::error::FrontendError;

    fn machine() -> Machine {
        Machine::new(MachineConfig {
            os_rom: vec![0xEA; 0x4000],
            basic_rom: None,
            cartridge: None,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: 48_000,
        })
        .unwrap()
    }

    fn codes(text: &str) -> Vec<u8> {
        text.chars().map(|c| ascii_to_kbcode(c).unwrap()).collect()
    }

    #[test]
    fn program_lines_then_run() {
        let path = Path::new("p.bas");
        assert_eq!(
            program_codes(path, "10 ?1\r\n\n   \n20 END", true).unwrap(),
            codes("10 ?1\n20 END\nRUN\n"),
            "with RUN"
        );
        assert_eq!(
            program_codes(path, "10 ?1\n", false).unwrap(),
            codes("10 ?1\n"),
            "without RUN"
        );
        let error = program_codes(path, "10 ?1\n20 ?\"{\"", true).unwrap_err();
        assert!(
            matches!(
                &error,
                FrontendError::BasCharacter {
                    line: 2,
                    character: '{',
                    ..
                }
            ),
            "{error:?}"
        );
    }

    /// The typist never sees `Ready` on a machine without an OS, so it stops after the
    /// timeout.
    #[test]
    fn typist_stops_without_ready() {
        let mut machine = machine();
        let mut typist = Typist::new(vec![DIGIT_1]);
        for frame in 0..READY_TIMEOUT_FRAMES {
            assert_eq!(typist.before_frame(&mut machine), Ok(()), "frame {frame}");
        }
        assert_eq!(typist.phase(), Phase::WaitReady(READY_TIMEOUT_FRAMES));
        assert_eq!(typist.before_frame(&mut machine), Err(NotReady), "timeout");
    }

    /// After `Ready`, each key is down for 3 frames and up for 3 frames.
    #[test]
    fn typist_key_timing() {
        let mut machine = machine();
        // A screen at $1000 with `Ready` (internal codes) in row 0.
        machine.poke_ram(0x58, 0x00);
        machine.poke_ram(0x59, 0x10);
        for (i, code) in [0x32, 0x65, 0x61, 0x64, 0x79].into_iter().enumerate() {
            machine.poke_ram(0x1000 + u16::try_from(i).unwrap(), code);
        }
        let mut typist = Typist::new(vec![DIGIT_1, RETURN]);
        let mut phases = Vec::new();
        while !typist.done() {
            typist.before_frame(&mut machine).unwrap();
            phases.push(typist.phase());
        }
        let per_key = usize::try_from(KEY_DOWN_FRAMES + KEY_UP_FRAMES).unwrap();
        assert_eq!(phases.len(), 2 * per_key, "frames for two keys");
        assert_eq!(phases[0], Phase::Typing { index: 0, tick: 1 }, "first key");
        assert_eq!(
            phases[per_key],
            Phase::Typing { index: 1, tick: 1 },
            "second key"
        );
        assert_eq!(typist.before_frame(&mut machine), Ok(()), "done stays done");
        assert!(
            Typist::new(Vec::new()).phase() == Phase::WaitReady(0),
            "new"
        );
    }
}
