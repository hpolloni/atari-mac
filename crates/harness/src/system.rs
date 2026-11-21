//! Machine setup and run helpers.

use atari_core::gtia::ConsoleKeys;
use atari_core::machine::OS_ROM_SIZE;
use atari_core::{Machine, MachineConfig, VideoStandard};

use crate::{HarnessError, data};

/// The audio sample rate of test machines.
pub const TEST_SAMPLE_RATE: u32 = 48_000;

/// The frames from power-on within which `AltirraOS` and BASIC show the prompt (REQ-005).
///
/// The boot takes about 195 frames: the OS makes 3 x 28 SIO attempts of 2 frames each to an
/// empty bus before BASIC starts (HRM 9.2).
pub const BOOT_FRAMES: u32 = 240;

/// How to boot a test machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootOptions {
    /// Insert Altirra BASIC.
    pub basic: bool,
    /// The TV standard.
    pub standard: VideoStandard,
}

impl Default for BootOptions {
    fn default() -> Self {
        Self {
            basic: true,
            standard: VideoStandard::Ntsc,
        }
    }
}

/// A machine with `AltirraOS` (and Altirra BASIC if `options.basic`), after power-on.
///
/// # Errors
///
/// [`HarnessError::Missing`] if the ROMs are not downloaded.
pub fn boot(options: BootOptions) -> Result<Machine, HarnessError> {
    let basic_rom = if options.basic {
        Some(data::basic_rom()?)
    } else {
        None
    };
    Ok(Machine::new(MachineConfig {
        os_rom: data::os_rom()?,
        basic_rom,
        cartridge: None,
        disk: None,
        fast_sio: false,
        tape: None,
        standard: options.standard,
        audio_sample_rate: TEST_SAMPLE_RATE,
    })?)
}

/// Runs `frames` frames.
pub fn run_frames(machine: &mut Machine, frames: u32) {
    for _ in 0..frames {
        machine.run_frame();
    }
}

/// Runs frames until `condition` is true. Returns the number of frames that ran before the
/// condition became true, or `None` after `max_frames`.
pub fn run_until(
    machine: &mut Machine,
    max_frames: u32,
    mut condition: impl FnMut(&Machine) -> bool,
) -> Option<u32> {
    for frame in 0..=max_frames {
        if condition(machine) {
            return Some(frame);
        }
        if frame < max_frames {
            machine.run_frame();
        }
    }
    None
}

/// Holds the console keys for `frames` frames, then releases them.
pub fn hold_console_keys(machine: &mut Machine, keys: ConsoleKeys, frames: u32) {
    machine.set_console_keys(keys);
    run_frames(machine, frames);
    machine.set_console_keys(ConsoleKeys::default());
}

/// A 16 KB OS ROM image for chip system tests.
///
/// The image has `program` at `$C000`, the RESET vector at `$C000`, and the NMI and IRQ
/// vectors at an `RTI` (`$40`) at `$FFF0`. The other bytes are `$EA` (`NOP`). A program
/// longer than `$3FF0` bytes is cut.
#[must_use]
pub fn test_os_rom(program: &[u8]) -> Vec<u8> {
    let mut rom = vec![0xEA; OS_ROM_SIZE];
    for (slot, byte) in rom.iter_mut().take(0x3FF0).zip(program) {
        *slot = *byte;
    }
    let tail: [u8; 16] = [
        0x40, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, // RTI at $FFF0
        0xF0, 0xFF, // NMI vector ($FFFA) -> $FFF0
        0x00, 0xC0, // RESET vector ($FFFC) -> $C000
        0xF0, 0xFF, // IRQ vector ($FFFE) -> $FFF0
    ];
    for (slot, byte) in rom.iter_mut().skip(0x3FF0).zip(tail) {
        *slot = byte;
    }
    rom
}

/// A machine with [`test_os_rom`] of `program` (and Altirra BASIC if `options.basic`),
/// after power-on.
///
/// # Errors
///
/// [`HarnessError::Missing`] if `options.basic` and the BASIC ROM is not downloaded.
pub fn boot_program(program: &[u8], options: BootOptions) -> Result<Machine, HarnessError> {
    let basic_rom = if options.basic {
        Some(data::basic_rom()?)
    } else {
        None
    };
    Ok(Machine::new(MachineConfig {
        os_rom: test_os_rom(program),
        basic_rom,
        cartridge: None,
        disk: None,
        fast_sio: false,
        tape: None,
        standard: options.standard,
        audio_sample_rate: TEST_SAMPLE_RATE,
    })?)
}
