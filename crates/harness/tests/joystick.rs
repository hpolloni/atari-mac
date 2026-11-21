//! Joystick directions and buttons on the full machine (REQ-439, REQ-440).
//!
//! The tests boot `AltirraOS`, which sets PIA port A for input (REQ-439 Notes), and read the
//! registers with `Machine::peek`, which has no side effects. They also check the OS shadow
//! registers `STICK0`, `STICK1`, `STRIG0` and `STRIG1`, which the OS vertical blank copies
//! from `PORTA`, `TRIG0` and `TRIG1`.

/// The tests. The module has `cfg(test)`, so that the helpers can panic as the tests do.
#[cfg(test)]
mod system {
    use atari_core::{JoystickState, Machine};
    use atari_harness::screen::has_row;
    use atari_harness::system::{BOOT_FRAMES, BootOptions, boot, run_frames, run_until};

    /// `PORTA`.
    const PORTA: u16 = 0xD300;
    /// `TRIG0`.
    const TRIG0: u16 = 0xD010;
    /// `TRIG1`.
    const TRIG1: u16 = 0xD011;
    /// The OS shadow of joystick 1 (`STICK0`).
    const STICK0: u16 = 0x0278;
    /// The OS shadow of joystick 2 (`STICK1`).
    const STICK1: u16 = 0x0279;
    /// The OS shadow of the port 1 button (`STRIG0`).
    const STRIG0: u16 = 0x0284;
    /// The OS shadow of the port 2 button (`STRIG1`).
    const STRIG1: u16 = 0x0285;
    /// A machine at the BASIC prompt. The OS has set up the PIA, and its vertical blank
    /// copies the shadow registers (it skips them during the SIO boot attempts, CRITIC).
    fn machine() -> Machine {
        let mut m = boot(BootOptions::default()).unwrap();
        let ready = run_until(&mut m, BOOT_FRAMES, |m| has_row(m, "Ready"));
        assert!(ready.is_some(), "no Ready row after {BOOT_FRAMES} frames");
        m
    }

    /// Verifies REQ-439: with both joysticks centered `PORTA` reads `$FF`; port 1 up, down,
    /// left and right give `$FE`, `$FD`, `$FB`, `$F7`; port 2 up, down, left and right give
    /// `$EF`, `$DF`, `$BF`, `$7F`; port 2 left and right together give `$3F` (criteria 1-4).
    #[test]
    fn joystick_directions_on_porta() {
        let mut m = machine();
        let center = JoystickState::default();
        // Criterion 1.
        assert_eq!(m.peek(PORTA), 0xFF, "both centered");
        let up = JoystickState { up: true, ..center };
        let down = JoystickState {
            down: true,
            ..center
        };
        let left = JoystickState {
            left: true,
            ..center
        };
        let right = JoystickState {
            right: true,
            ..center
        };
        // Criteria 2 and 3.
        for (port, state, expected) in [
            (0, up, 0xFE),
            (0, down, 0xFD),
            (0, left, 0xFB),
            (0, right, 0xF7),
            (1, up, 0xEF),
            (1, down, 0xDF),
            (1, left, 0xBF),
            (1, right, 0x7F),
        ] {
            m.set_joystick(port, state);
            assert_eq!(m.peek(PORTA), expected, "port {}: {state:?}", port + 1);
            m.set_joystick(port, center);
            assert_eq!(m.peek(PORTA), 0xFF, "port {} centered again", port + 1);
        }
        // Criterion 4: no filter for opposite directions.
        m.set_joystick(
            1,
            JoystickState {
                left: true,
                right: true,
                ..center
            },
        );
        assert_eq!(m.peek(PORTA), 0x3F, "port 2 left and right");
        // The OS reads the same values: port 1 up and port 2 left and right.
        m.set_joystick(0, up);
        run_frames(&mut m, 2);
        assert_eq!(m.peek(STICK0), 0x0E, "STICK0");
        assert_eq!(m.peek(STICK1), 0x03, "STICK1");
        m.set_joystick(0, center);
        m.set_joystick(1, center);
        run_frames(&mut m, 2);
        assert_eq!([m.peek(STICK0), m.peek(STICK1)], [0x0F, 0x0F], "centered");
    }

    /// Verifies REQ-440: with the port 1 button pressed `TRIG0` reads `$00` and with it
    /// released `$01`; with the port 2 button pressed `TRIG1` reads `$00` and `TRIG0` does not
    /// change (criteria 1 and 2).
    #[test]
    fn joystick_buttons_on_trig0_trig1() {
        let mut m = machine();
        let pressed = JoystickState {
            fire: true,
            ..JoystickState::default()
        };
        let released = JoystickState::default();
        assert_eq!([m.peek(TRIG0), m.peek(TRIG1)], [0x01, 0x01], "released");
        // Criterion 1.
        m.set_joystick(0, pressed);
        assert_eq!(m.peek(TRIG0), 0x00, "port 1 pressed");
        assert_eq!(m.peek(TRIG1), 0x01, "TRIG1 with port 1 pressed");
        assert_eq!(m.peek(PORTA), 0xFF, "the button is not a direction");
        run_frames(&mut m, 2);
        assert_eq!([m.peek(STRIG0), m.peek(STRIG1)], [0x00, 0x01], "OS shadows");
        m.set_joystick(0, released);
        assert_eq!(m.peek(TRIG0), 0x01, "port 1 released");
        // Criterion 2.
        m.set_joystick(1, pressed);
        assert_eq!(m.peek(TRIG1), 0x00, "port 2 pressed");
        assert_eq!(m.peek(TRIG0), 0x01, "TRIG0 does not change");
        run_frames(&mut m, 2);
        assert_eq!([m.peek(STRIG0), m.peek(STRIG1)], [0x01, 0x00], "OS shadows");
        m.set_joystick(1, released);
        assert_eq!(
            [m.peek(TRIG0), m.peek(TRIG1)],
            [0x01, 0x01],
            "both released"
        );
    }
}
