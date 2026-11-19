//! The `SingleStepTests` 6502 suite (REQ-023, REQ-100 to REQ-113). The test names follow the
//! test plan in `docs/design/cpu.md`.

use atari_harness::HarnessError;
use atari_harness::single_step::{
    DOCUMENTED_OPCODES, KIL_OPCODES, UNSTABLE_OPCODES, failure_summary, run_opcodes,
};

/// Runs `opcodes`. Returns a summary of each failing opcode (empty if all pass).
fn check(opcodes: &[u8]) -> Result<String, HarnessError> {
    Ok(failure_summary(&run_opcodes(opcodes)?))
}

/// The opcodes that are neither documented, unstable nor KIL.
fn stable_undocumented() -> Vec<u8> {
    (0..=u8::MAX)
        .filter(|op| {
            !DOCUMENTED_OPCODES.contains(op)
                && !UNSTABLE_OPCODES.contains(op)
                && !KIL_OPCODES.contains(op)
        })
        .collect()
}

/// Verifies REQ-100 and REQ-107: all documented opcodes.
#[test]
fn single_step_documented_opcodes() {
    let summary = check(&DOCUMENTED_OPCODES).unwrap();
    assert!(summary.is_empty(), "single-step failures:\n{summary}");
}

/// Verifies REQ-101 to REQ-106 and REQ-023: every bus cycle of every non-KIL opcode.
#[test]
fn single_step_bus_cycles() {
    let all: Vec<u8> = (0..=u8::MAX)
        .filter(|op| !KIL_OPCODES.contains(op))
        .collect();
    let summary = check(&all).unwrap();
    assert!(summary.is_empty(), "single-step failures:\n{summary}");
}

/// Verifies REQ-108: decimal mode `ADC` and `SBC` (the cases include `D` = 1).
#[test]
fn single_step_decimal() {
    let summary = check(&[
        0x61, 0x65, 0x69, 0x6D, 0x71, 0x75, 0x79, 0x7D, 0xE1, 0xE5, 0xE9, 0xED, 0xF1, 0xF5, 0xF9,
        0xFD, 0xEB,
    ])
    .unwrap();
    assert!(summary.is_empty(), "single-step failures:\n{summary}");
}

/// Verifies REQ-114: `BRK`.
#[test]
fn single_step_brk() {
    let summary = check(&[0x00]).unwrap();
    assert!(summary.is_empty(), "single-step failures:\n{summary}");
}

/// Verifies REQ-113: the KIL opcodes.
#[test]
fn single_step_kil() {
    let summary = check(&KIL_OPCODES).unwrap();
    assert!(summary.is_empty(), "single-step failures:\n{summary}");
}

/// Verifies REQ-111: the stable undocumented opcodes.
#[test]
fn single_step_undocumented_stable() {
    let summary = check(&stable_undocumented()).unwrap();
    assert!(summary.is_empty(), "single-step failures:\n{summary}");
}

/// Verifies REQ-112: the unstable undocumented opcodes.
#[test]
fn single_step_undocumented_unstable() {
    let summary = check(&UNSTABLE_OPCODES).unwrap();
    assert!(summary.is_empty(), "single-step failures:\n{summary}");
}
