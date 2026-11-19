//! System boot tests with the free ROMs (REQ-002, REQ-004 to REQ-006, REQ-014, REQ-024).

use atari_core::gtia::ConsoleKeys;
use atari_core::{ConfigError, Machine, MachineConfig, VideoStandard};
use atari_harness::screen::has_row;
use atari_harness::system::{
    BOOT_FRAMES, BootOptions, TEST_SAMPLE_RATE, boot, hold_console_keys, run_frames, run_until,
};
use atari_harness::{HarnessError, data};

/// A configuration with the free ROMs.
fn config() -> Result<MachineConfig, HarnessError> {
    Ok(MachineConfig {
        os_rom: data::os_rom()?,
        basic_rom: Some(data::basic_rom()?),
        cartridge: None,
        standard: VideoStandard::Ntsc,
        audio_sample_rate: TEST_SAMPLE_RATE,
    })
}

/// Verifies REQ-002: one frame is 29,868 cycles (NTSC) or 35,568 cycles (PAL).
#[test]
fn frame_length_matches_standard() {
    for (standard, cycles) in [(VideoStandard::Ntsc, 29_868), (VideoStandard::Pal, 35_568)] {
        let mut machine = boot(BootOptions {
            basic: true,
            standard,
        })
        .unwrap();
        machine.run_frame();
        let start = machine.cycle_count();
        machine.run_frame();
        assert_eq!(machine.cycle_count() - start, cycles, "{standard:?}");
    }
}

/// Verifies REQ-004: ROMs of the right size are accepted and mapped.
#[test]
fn accepts_user_roms() {
    let os = data::os_rom().unwrap();
    let mut machine = Machine::new(config().unwrap()).unwrap();
    machine.run_frame();
    assert_eq!(
        machine.peek(0xFFFC),
        os[0x3FFC],
        "reset vector low byte from the OS ROM"
    );
}

/// Verifies REQ-004: ROMs of the wrong size are rejected.
#[test]
fn rejects_rom_with_wrong_size() {
    let mut short_os = config().unwrap();
    short_os.os_rom.truncate(16_383);
    assert_eq!(
        Machine::new(short_os).unwrap_err(),
        ConfigError::OsRomSize(16_383),
        "OS ROM"
    );
    let mut long_basic = config().unwrap();
    long_basic.basic_rom = Some(vec![0; 8_193]);
    assert_eq!(
        Machine::new(long_basic).unwrap_err(),
        ConfigError::BasicRomSize(8_193),
        "BASIC ROM"
    );
}

/// Verifies REQ-014: two machines with the same configuration give the same frames.
#[test]
fn emulation_is_deterministic() {
    let mut a = Machine::new(config().unwrap()).unwrap();
    let mut b = Machine::new(config().unwrap()).unwrap();
    for frame in 0..300 {
        a.run_frame();
        b.run_frame();
        assert_eq!(a.frame(), b.frame(), "frame {frame}");
    }
    assert_eq!(a, b, "machine state after 300 frames");
}

/// Verifies REQ-005 and REQ-024: the machine boots to the BASIC `READY` prompt.
///
/// Altirra BASIC prints the prompt as `Ready`. Before BASIC starts, the OS sends three SIO
/// commands (the D1: status of the disk boot and two type 3 polls) to an empty bus. Each
/// command makes 28 attempts (HRM 9.2), and each attempt waits 2 frames for the ACK, so
/// the prompt appears after about 195 frames.
#[test]
fn boots_to_basic_ready() {
    let mut machine = boot(BootOptions::default()).unwrap();
    let frames = run_until(&mut machine, BOOT_FRAMES, |m| has_row(m, "Ready"));
    assert!(frames.is_some(), "no Ready row after {BOOT_FRAMES} frames");
}

/// Verifies REQ-006: OPTION held at power-on disables BASIC.
#[test]
fn option_key_disables_basic() {
    let mut machine = boot(BootOptions::default()).unwrap();
    hold_console_keys(
        &mut machine,
        ConsoleKeys {
            option: true,
            ..ConsoleKeys::default()
        },
        BOOT_FRAMES,
    );
    machine.poke_ram(0xA000, 0x5A);
    assert_eq!(machine.peek(0xA000), 0x5A, "RAM shows at $A000");
}

/// Verifies REQ-006: without OPTION, BASIC is mapped at `$A000`.
#[test]
fn basic_is_mapped_without_option() {
    let basic = data::basic_rom().unwrap();
    let mut machine = boot(BootOptions::default()).unwrap();
    run_frames(&mut machine, BOOT_FRAMES);
    assert_eq!(machine.peek(0xA000), basic[0], "BASIC byte $0000 at $A000");
}
