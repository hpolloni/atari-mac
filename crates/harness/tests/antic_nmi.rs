//! ANTIC NMI system tests with `AltirraOS` (REQ-333).

use atari_harness::screen::has_row;
use atari_harness::system::{BOOT_FRAMES, BootOptions, boot, run_until};

/// The byte of the OS real-time clock `RTCLOK` (`$12-$14`) that the OS vertical blank
/// handler increments in each frame (REQ-333).
const RTCLOK_LOW: u16 = 0x14;

/// Verifies REQ-333: with `AltirraOS`, after the boot, the byte at `$14` increases by 1 in each
/// frame, for 10 frames (criterion 3).
#[test]
fn os_vbi_increments_rtclok() {
    let mut machine = boot(BootOptions::default()).unwrap();
    let frames = run_until(&mut machine, BOOT_FRAMES, |m| has_row(m, "Ready"));
    assert!(frames.is_some(), "no Ready row after {BOOT_FRAMES} frames");
    let mut last = machine.peek(RTCLOK_LOW);
    for frame in 0..10 {
        machine.run_frame();
        let now = machine.peek(RTCLOK_LOW);
        assert_eq!(
            now,
            last.wrapping_add(1),
            "frame {frame}: $14 increases by 1"
        );
        last = now;
    }
}
