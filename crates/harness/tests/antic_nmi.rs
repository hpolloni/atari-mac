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

/// The planned tests (`docs/design/spec-gates.md`, "Planned test stubs"). The module has
/// `cfg(test)`, so that the helper can panic as the tests do.
#[cfg(test)]
mod stubs {
    /// Fails the calling test: a planned stub never counts as a pass.
    fn planned() {
        panic!("planned test: not written yet");
    }

    /// Verifies REQ-334: with `DMACTL` = `$20` and `NMIEN` = `$C0`, after an instruction `$80`,
    /// a `NOP` that starts on cycle 8 gives the NMI sequence start on cycle 10, and one on
    /// cycle 9 on cycle 11; with `STA NMIEN` writing `$C0` on cycle 7 and `NOP` instructions on
    /// cycles 8, 10 and 12, the NMI sequence starts on cycle 12.
    #[test]
    #[ignore = "planned: SLICE-02"]
    fn nmi_sequence_start_cycles() {
        planned();
    }

    /// Verifies REQ-335: with `DMACTL` = `$00` and `NMIEN` = `$40`, a `BRK` with its opcode
    /// fetch on cycle 4 of scan line 248 runs the `BRK` handler, the NMI handler does not run
    /// in that frame, and `NMIST` bit 6 reads 1 from cycle 7 of scan line 248.
    #[test]
    #[ignore = "planned: SLICE-02"]
    fn brk_on_cycle_4_skips_vbi_handler() {
        planned();
    }

    /// Verifies REQ-335: in the same setup, with the `BRK` opcode fetch on cycle 3 or on
    /// cycle 5, the NMI handler runs once in that frame.
    #[test]
    #[ignore = "planned: SLICE-02"]
    fn brk_on_cycle_3_or_5_keeps_vbi_handler() {
        planned();
    }
}
