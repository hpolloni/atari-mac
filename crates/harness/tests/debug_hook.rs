//! The CPU hook of the machine for the trace and the monitor (REQ-086). The test names
//! follow the test plan in `docs/design/debugger.md`.

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

/// Verifies REQ-086: a program machine with ANTIC DMA and a `WSYNC` write
/// runs one frame with `Machine::run_frame_with`. Each opcode fetch cycle that a cycle-by-cycle
/// run finds (the cycle after which `Cpu::at_opcode_fetch` is false) has exactly one
/// `instruction` call, with that position and the registers before the instruction (criteria 1
/// and 4).
#[test]
#[ignore = "planned: SLICE-10"]
fn hook_reports_each_instruction_start() {
    planned();
}

/// Verifies REQ-086: the reset sequence after power-on, a VBI NMI and a POKEY
/// timer IRQ each give one `sequence` call with the kind, the position of the first cycle of
/// the sequence and the handler address, before the `instruction` call of the first handler
/// instruction (criterion 2).
#[test]
#[ignore = "planned: SLICE-10"]
fn hook_reports_interrupt_sequences() {
    planned();
}

/// Verifies REQ-086: 3 frames of the `AltirraOS` boot with `run_frame` and with
/// `run_frame_with` and a hook that records each call give equal machines (criterion 5).
#[test]
#[ignore = "planned: SLICE-10"]
fn hooked_run_matches_plain_run() {
    planned();
}
