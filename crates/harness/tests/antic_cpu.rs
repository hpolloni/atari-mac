//! Joint ANTIC and CPU system tests (REQ-334, OI-017).

/// The planned tests (`docs/design/spec-gates.md`, "Planned test stubs"). The module has
/// `cfg(test)`, so that the helper can panic as the tests do.
#[cfg(test)]
mod stubs {
    /// Fails the calling test: a planned stub never counts as a pass.
    fn planned() {
        panic!("planned test: not written yet");
    }

    /// Verifies REQ-334: with the real CPU, the NMI sequence of a DLI starts on cycle 10 after
    /// a `NOP` that starts on cycle 8, and on cycle 11 after one that starts on cycle 9 (joint
    /// ANTIC and CPU test, OI-017).
    #[test]
    #[ignore = "planned: SLICE-02"]
    fn nmi_start_cycles_with_real_cpu() {
        planned();
    }
}
