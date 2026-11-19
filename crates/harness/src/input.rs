//! Keyboard input (REQ-025).

use atari_core::Machine;
use atari_core::keyboard::ascii_to_kbcode;

use crate::HarnessError;
use crate::system::run_frames;

/// Frames that a key is held down.
pub const KEY_DOWN_FRAMES: u32 = 3;

/// Frames between two keys.
pub const KEY_UP_FRAMES: u32 = 3;

/// Types `text` on the emulated keyboard. For each character, the key is held for
/// [`KEY_DOWN_FRAMES`] frames, then released for [`KEY_UP_FRAMES`] frames.
///
/// # Errors
///
/// [`HarnessError::NoKey`] for the first character without a key. No key is pressed then.
pub fn type_text(machine: &mut Machine, text: &str) -> Result<(), HarnessError> {
    let codes = text
        .chars()
        .map(|c| ascii_to_kbcode(c).ok_or(HarnessError::NoKey(c)))
        .collect::<Result<Vec<u8>, HarnessError>>()?;
    for code in codes {
        machine.key_down(code);
        run_frames(machine, KEY_DOWN_FRAMES);
        machine.key_up();
        run_frames(machine, KEY_UP_FRAMES);
    }
    Ok(())
}
