//! Joystick directions and buttons on the full machine (REQ-439, REQ-440).

/// The tests. The module has `cfg(test)`, so that the helper can panic as the tests do.
#[cfg(test)]
mod system {
    /// Fails the test: the test is a planned stub that a package replaces with the real test
    /// (`docs/design/spec-gates.md`, "Planned test stubs").
    fn planned() {
        panic!("planned test: a package replaces this stub with the real test");
    }

    /// Verifies REQ-439: with both joysticks centered `PORTA` reads `$FF`; port 1 up, down,
    /// left and right give `$FE`, `$FD`, `$FB`, `$F7`; port 2 up, down, left and right give
    /// `$EF`, `$DF`, `$BF`, `$7F`; port 2 left and right together give `$3F` (criteria 1-4).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn joystick_directions_on_porta() {
        planned();
    }

    /// Verifies REQ-440: with the port 1 button pressed `TRIG0` reads `$00` and with it
    /// released `$01`; with the port 2 button pressed `TRIG1` reads `$00` and `TRIG0` does not
    /// change (criteria 1 and 2).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn joystick_buttons_on_trig0_trig1() {
        planned();
    }
}
