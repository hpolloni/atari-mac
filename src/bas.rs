//! Run a BASIC program from a text file (`--bas`, `--no-run`, REQ-080).
//!
//! Design: `docs/design/architecture.md`, "BASIC program from a text file". [`load`] reads
//! and checks the whole file before the machine starts, and gives the key codes to type.
//! [`Typist`] types them one frame at a time: the frontend calls [`Typist::before_frame`]
//! before each frame, so the window keeps showing frames while the program is typed.
//!
//! The key timing is the shortest timing that `AltirraOS` accepts without a lost or doubled
//! key (REQ-080 criteria 9 and 10): see [`KEY_DOWN_FRAMES`], [`KEY_UP_FRAMES`],
//! [`SAME_KEY_UP_FRAMES`] and [`CH`]. It is not the timing of the harness `type_text`.

use std::fs;
use std::path::Path;

use atari_core::Machine;
use atari_core::keyboard::{CAPS, RETURN, SHIFT, ascii_to_kbcode};
use atari_core::screen::has_row;

use crate::error::FrontendError;

/// The prompt of BASIC (Altirra BASIC prints `Ready`).
pub const READY: &str = "Ready";

/// The frames that the typist waits for [`READY`] before it stops (REQ-080 criterion 6).
pub const READY_TIMEOUT_FRAMES: u32 = 600;

/// Frames that each key is held down. POKEY registers a key after 64 to 128 scanlines
/// (REQ-518), so one frame (262 or 312 scanlines) is enough.
pub const KEY_DOWN_FRAMES: u32 = 1;

/// Frames that the key is up before the next key, when the next key is a different key.
/// POKEY registers the release after 64 to 128 scanlines (REQ-518).
pub const KEY_UP_FRAMES: u32 = 1;

/// Frames that the key is up before the same key code again.
///
/// The OS keyboard interrupt ignores the key code of the last key while its debounce
/// counter `KEYDEL` (`$02F1`) is not 0. The OS sets `KEYDEL` to 3 when it accepts a key, and
/// the vertical blank interrupt counts it down only when it sees no key down. A frame starts
/// at the vertical blank, so the first VBI after the release still sees the key down; with
/// 3 frames up, `KEYDEL` is 0 when the next press comes (measured: 2 frames lose the second
/// key).
pub const SAME_KEY_UP_FRAMES: u32 = 3;

/// The OS variable `CH` (`$02FC`): the last key code, or `$FF` after the editor read it.
///
/// The OS has no key buffer: a key that comes while `CH` holds a key replaces it. So
/// after each key, the typist waits until `CH` is `$FF` (for example while BASIC stores a
/// line after RETURN), then it presses the next key.
pub const CH: u16 = 0x02FC;

/// The value of [`CH`] when no key waits.
const CH_EMPTY: u8 = 0xFF;

/// The frames that the typist waits for [`CH`] to be `$FF` after the key timing
/// of a key. Then it types the next key, so a program that does not read the keyboard
/// cannot stop the typing.
pub const KEY_READ_TIMEOUT_FRAMES: u32 = 120;

/// The text typed after the program, unless `--no-run`.
const RUN: &str = "RUN";

/// The key that switches the OS screen editor to lower case letters: CAPS toggles between
/// upper and lower case (`SHFLOK` `$02BE`: `$40` upper, `$00` lower).
const TO_LOWER: u8 = CAPS;

/// The key that switches the OS screen editor back to upper case letters: SHIFT + CAPS
/// always sets upper case.
const TO_UPPER: u8 = CAPS | SHIFT;

/// Converts one line of text to key codes, without RETURN.
///
/// The OS starts with upper case letters, and a letter key gives the letter in the case of
/// the editor. So before a lower case letter in upper case mode the codes have
/// [`TO_LOWER`], and before an upper case letter in lower case mode [`TO_UPPER`]. `lower`
/// is the case mode before and after the line.
fn line_codes(
    path: &Path,
    number: usize,
    line: &str,
    lower: &mut bool,
) -> Result<Vec<u8>, FrontendError> {
    let mut codes = Vec::with_capacity(line.len());
    for character in line.chars() {
        let code = ascii_to_kbcode(character).ok_or_else(|| FrontendError::BasCharacter {
            path: path.to_owned(),
            line: number,
            character,
        })?;
        if character.is_ascii_lowercase() && !*lower {
            codes.push(TO_LOWER);
            *lower = true;
        } else if character.is_ascii_uppercase() && *lower {
            codes.push(TO_UPPER);
            *lower = false;
        }
        codes.push(code);
    }
    Ok(codes)
}

/// The key codes for the program `text` of the file `path`: each program line, then RETURN.
/// With `run`, `RUN` and RETURN follow.
///
/// A line ends with LF or CR LF. A line that is empty or has only spaces is not typed.
/// Lower case letters are typed in lower case (see [`line_codes`]); after the program, the
/// editor is in upper case again.
///
/// # Errors
///
/// [`FrontendError::BasCharacter`] for the first character without an Atari key, with the
/// line number (from 1).
pub fn program_codes(path: &Path, text: &str, run: bool) -> Result<Vec<u8>, FrontendError> {
    let mut codes = Vec::with_capacity(text.len() + RUN.len() + 2);
    let mut lower = false;
    for (index, line) in text.lines().enumerate() {
        if line.trim_matches(' ').is_empty() {
            continue;
        }
        codes.extend(line_codes(path, index + 1, line, &mut lower)?);
        codes.push(RETURN);
    }
    if lower {
        codes.push(TO_UPPER);
    }
    if run {
        codes.extend(line_codes(path, 0, RUN, &mut lower)?);
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
    /// It types key `index`; `tick` is the frame within the key (down, up, then the wait
    /// for the OS to read the key).
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

/// Types key codes on the machine, one frame at a time.
///
/// Each key is down for [`KEY_DOWN_FRAMES`] frames, then up for [`KEY_UP_FRAMES`] frames
/// ([`SAME_KEY_UP_FRAMES`] before the same key code again), then until the OS has read the
/// key ([`CH`]).
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
            let (index, tick) = if self.key_done(machine, index, tick) {
                (index + 1, 0)
            } else {
                (index, tick)
            };
            let Some(&code) = self.codes.get(index) else {
                self.phase = Phase::Done;
                return Ok(());
            };
            if tick == 0 {
                machine.key_down(code);
            } else if tick == KEY_DOWN_FRAMES {
                machine.key_up();
            }
            self.phase = Phase::Typing {
                index,
                tick: tick + 1,
            };
        }
        Ok(())
    }

    /// The frames that key `index` is up: [`SAME_KEY_UP_FRAMES`] when the next key has the
    /// same code, else [`KEY_UP_FRAMES`].
    fn up_frames(&self, index: usize) -> u32 {
        if self.codes.get(index + 1) == self.codes.get(index) {
            SAME_KEY_UP_FRAMES
        } else {
            KEY_UP_FRAMES
        }
    }

    /// `true` when key `index` at frame `tick` is typed: it was down and up, and the OS read
    /// it ([`CH`] is empty) or [`KEY_READ_TIMEOUT_FRAMES`] frames passed.
    fn key_done(&self, machine: &Machine, index: usize, tick: u32) -> bool {
        let typed = KEY_DOWN_FRAMES + self.up_frames(index);
        tick >= typed && (machine.peek(CH) == CH_EMPTY || tick >= typed + KEY_READ_TIMEOUT_FRAMES)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use atari_core::keyboard::{DIGIT_1, RETURN, ascii_to_kbcode};
    use atari_core::{Machine, MachineConfig, VideoStandard};

    use super::{
        CH, KEY_DOWN_FRAMES, KEY_READ_TIMEOUT_FRAMES, KEY_UP_FRAMES, NotReady, Phase,
        READY_TIMEOUT_FRAMES, SAME_KEY_UP_FRAMES, Typist, program_codes,
    };
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

    /// A machine without an OS, with `Ready` (internal codes) in row 0 of a screen at
    /// `$1000`, and with `CH` = `$FF` (no key waits).
    fn ready_machine() -> Machine {
        let mut machine = machine();
        machine.poke_ram(0x58, 0x00);
        machine.poke_ram(0x59, 0x10);
        for (i, code) in [0x32, 0x65, 0x61, 0x64, 0x79].into_iter().enumerate() {
            machine.poke_ram(0x1000 + u16::try_from(i).unwrap(), code);
        }
        machine.poke_ram(CH, 0xFF);
        machine
    }

    /// The phases of `typist` after each call of `before_frame`, until it is done. `on_frame`
    /// can change the machine before each call.
    fn run_typist(
        machine: &mut Machine,
        typist: &mut Typist,
        mut on_frame: impl FnMut(usize, &mut Machine),
    ) -> Vec<Phase> {
        let mut phases = Vec::new();
        while !typist.done() {
            on_frame(phases.len(), machine);
            typist.before_frame(machine).unwrap();
            phases.push(typist.phase());
        }
        phases
    }

    /// After `Ready`, each key is down for 1 frame and up for 1 frame, and up for 3 frames
    /// before the same key code again.
    #[test]
    fn typist_key_timing() {
        let mut machine = ready_machine();
        let mut typist = Typist::new(vec![DIGIT_1, DIGIT_1, RETURN]);
        let phases = run_typist(&mut machine, &mut typist, |_, _| {});
        let first = usize::try_from(KEY_DOWN_FRAMES + SAME_KEY_UP_FRAMES).unwrap();
        let other = usize::try_from(KEY_DOWN_FRAMES + KEY_UP_FRAMES).unwrap();
        assert_eq!((first, other), (4, 2), "the timing");
        assert_eq!(phases.len(), first + 2 * other + 1, "frames for three keys");
        assert_eq!(phases[0], Phase::Typing { index: 0, tick: 1 }, "first key");
        assert_eq!(
            phases[first],
            Phase::Typing { index: 1, tick: 1 },
            "the same key again after 3 frames up"
        );
        assert_eq!(
            phases[first + other],
            Phase::Typing { index: 2, tick: 1 },
            "a different key after 1 frame up"
        );
        assert_eq!(typist.phase(), Phase::Done, "done");
        assert_eq!(typist.before_frame(&mut machine), Ok(()), "done stays done");
        assert!(
            Typist::new(Vec::new()).phase() == Phase::WaitReady(0),
            "new"
        );
        let mut empty = Typist::new(Vec::new());
        assert_eq!(
            run_typist(&mut machine, &mut empty, |_, _| {}),
            [Phase::Done],
            "no key"
        );
    }

    /// The next key waits until the OS read the key (`CH` = `$FF`), or until the timeout.
    #[test]
    fn typist_waits_for_the_key_to_be_read() {
        let mut machine = ready_machine();
        let mut typist = Typist::new(vec![DIGIT_1, RETURN]);
        // The OS reads the first key 10 frames after it is pressed.
        let phases = run_typist(&mut machine, &mut typist, |frame, machine| match frame {
            1 => machine.poke_ram(CH, DIGIT_1),
            10 => machine.poke_ram(CH, 0xFF),
            _ => {}
        });
        assert_eq!(
            phases[10],
            Phase::Typing { index: 1, tick: 1 },
            "the second key after CH is $FF"
        );

        let mut machine = ready_machine();
        machine.poke_ram(CH, DIGIT_1);
        let mut typist = Typist::new(vec![DIGIT_1, RETURN]);
        let phases = run_typist(&mut machine, &mut typist, |_, _| {});
        let wait =
            usize::try_from(KEY_DOWN_FRAMES + KEY_UP_FRAMES + KEY_READ_TIMEOUT_FRAMES).unwrap();
        assert_eq!(
            phases[wait],
            Phase::Typing { index: 1, tick: 1 },
            "the second key after the timeout"
        );
    }
}
