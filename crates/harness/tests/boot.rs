//! System boot tests with the free ROMs (REQ-002, REQ-004 to REQ-006, REQ-014, REQ-018,
//! REQ-024).

use atari_core::gtia::ConsoleKeys;
use atari_core::{ConfigError, Machine, MachineConfig, StateError, VideoStandard};
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
        disk: None,
        fast_sio: false,
        tape: None,
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

/// Verifies REQ-014: two machines created with the same `MachineConfig` and run for 300
/// frames give equal frame buffers after each frame and equal audio sample sequences
/// (criterion 1).
#[test]
fn emulation_is_deterministic() {
    let mut a = Machine::new(config().unwrap()).unwrap();
    let mut b = Machine::new(config().unwrap()).unwrap();
    let mut audio_a = Vec::new();
    let mut audio_b = Vec::new();
    for frame in 0..300 {
        a.run_frame();
        b.run_frame();
        assert_eq!(a.frame(), b.frame(), "frame {frame}");
        a.drain_audio(&mut audio_a);
        b.drain_audio(&mut audio_b);
        assert_eq!(
            audio_a.len(),
            audio_b.len(),
            "sample count after frame {frame}"
        );
    }
    assert!(
        audio_a.len() > 300 * 700,
        "about 801 samples per frame at 48 kHz: {}",
        audio_a.len()
    );
    let same = audio_a
        .iter()
        .zip(&audio_b)
        .all(|(x, y)| x.to_bits() == y.to_bits());
    assert!(same, "equal audio sample sequences");
    assert_eq!(a, b, "machine state after 300 frames");
}

/// Verifies REQ-005, REQ-024 and REQ-216: the machine boots to the BASIC `READY` prompt
/// (REQ-216 criterion 4).
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

/// The frames of REQ-006: OPTION is held from power-on for this many frames.
const OPTION_FRAMES: u32 = 180;

/// Verifies REQ-006: with OPTION held from power-on for 180 frames, `poke_ram($A000, $5A)`
/// followed by `peek($A000)` gives `$5A` (RAM is visible) (criterion 1).
#[test]
fn option_key_disables_basic() {
    let mut machine = boot(BootOptions::default()).unwrap();
    hold_console_keys(
        &mut machine,
        ConsoleKeys {
            option: true,
            ..ConsoleKeys::default()
        },
        OPTION_FRAMES,
    );
    machine.poke_ram(0xA000, 0x5A);
    assert_eq!(machine.peek(0xA000), 0x5A, "RAM shows at $A000");
}

/// Verifies REQ-006: without OPTION, after 180 frames, `peek($A000)` gives byte `$0000` of
/// the BASIC ROM (criterion 2).
#[test]
fn basic_is_mapped_without_option() {
    let basic = data::basic_rom().unwrap();
    let mut machine = boot(BootOptions::default()).unwrap();
    run_frames(&mut machine, OPTION_FRAMES);
    assert_ne!(
        basic[0], 0x00,
        "the check needs a BASIC byte that is not RAM"
    );
    assert_eq!(machine.peek(0xA000), basic[0], "BASIC byte $0000 at $A000");
}

/// Verifies REQ-002: a `MachineConfig` with `VideoStandard::default()` gives NTSC: one frame
/// is 29,868 cycles (criterion 3).
#[test]
fn default_standard_is_ntsc() {
    assert_eq!(VideoStandard::default(), VideoStandard::Ntsc, "default");
    let mut machine = Machine::new(MachineConfig {
        standard: VideoStandard::default(),
        ..config().unwrap()
    })
    .unwrap();
    assert_eq!(machine.standard(), VideoStandard::Ntsc, "machine standard");
    machine.run_frame();
    let start = machine.cycle_count();
    machine.run_frame();
    assert_eq!(machine.cycle_count() - start, 29_868, "one NTSC frame");
}

/// Verifies REQ-018: a state saved after frame 100 and restored in a new machine gives the
/// same frame buffers for frames 101 to 200 as a run without interruption (criterion 1).
#[test]
fn save_state_round_trip() {
    let mut original = Machine::new(config().unwrap()).unwrap();
    run_frames(&mut original, 100);
    let state = original.save_state();
    let mut restored = Machine::new(config().unwrap()).unwrap();
    restored.load_state(&state).unwrap();
    assert_eq!(
        restored, original,
        "the restored machine equals the saved machine"
    );
    let mut audio_original = Vec::new();
    let mut audio_restored = Vec::new();
    let mut changed = false;
    for frame in 101..=200 {
        let before = original.frame().to_vec();
        original.run_frame();
        restored.run_frame();
        assert_eq!(restored.frame(), original.frame(), "frame {frame}");
        changed |= original.frame() != before.as_slice();
        original.drain_audio(&mut audio_original);
        restored.drain_audio(&mut audio_restored);
    }
    assert!(
        changed,
        "the screen changes in frames 101 to 200 (the Ready prompt)"
    );
    assert_eq!(audio_restored.len(), audio_original.len(), "sample count");
    let same = audio_original
        .iter()
        .zip(&audio_restored)
        .all(|(x, y)| x.to_bits() == y.to_bits());
    assert!(same, "equal audio samples");
    assert_eq!(restored, original, "machine state after frame 200");

    // A state does not load into a machine with another configuration, and a changed byte
    // fails the checksum. The machine does not change when a load fails.
    let mut pal = Machine::new(MachineConfig {
        standard: VideoStandard::Pal,
        ..config().unwrap()
    })
    .unwrap();
    let fresh = pal.clone();
    assert_eq!(
        pal.load_state(&state),
        Err(StateError::Config),
        "PAL machine"
    );
    assert_eq!(pal, fresh, "unchanged after a failed load");
    let mut bad = state.clone();
    let middle = bad.len() / 2;
    bad[middle] ^= 1;
    assert_eq!(
        restored.load_state(&bad),
        Err(StateError::Checksum),
        "changed byte"
    );
    assert_eq!(
        restored.load_state(&state[..state.len() - 9]),
        Err(StateError::Checksum),
        "truncated state"
    );
    assert_eq!(
        restored.load_state(b"not a state"),
        Err(StateError::Signature),
        "other bytes"
    );
}
