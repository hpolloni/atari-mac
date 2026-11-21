//! The disassembler of the core (REQ-087). The test names follow the test plan in
//! `docs/design/debugger.md`.

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

/// Verifies REQ-087: each of the 256 opcodes with the operand bytes `$34 $12` at
/// `$2000` gives the length (1 to 3) and the text of a table in this test. The undocumented
/// opcodes have the names of `docs/design/debugger.md`, "Undocumented names" (HRM 3.5), and
/// each KIL opcode is `KIL` (criteria 1 to 4).
#[test]
#[ignore = "planned: SLICE-10"]
fn disassembles_all_opcodes() {
    planned();
}

/// Verifies REQ-087: a branch shows its target address with a wrap at 16 bits:
/// `BNE` at `$FFF0` with the offset `$7F` is `BNE $0071`, and at `$0010` with the offset `$80`
/// is `BNE $FF92` (criterion 2).
#[test]
#[ignore = "planned: SLICE-10"]
fn branch_targets_wrap() {
    planned();
}

/// Verifies REQ-087: `Machine::disassemble_at` and `Machine::debug_peek`
/// for each address `$D000` to `$D7FF` leave the machine equal to a copy from before the reads
/// (criterion 5).
#[test]
#[ignore = "planned: SLICE-10"]
fn disassembly_reads_without_side_effects() {
    planned();
}
