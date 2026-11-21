//! The speed statistics of `--stats` (REQ-083).
//!
//! Design: `docs/design/architecture.md`, "Speed statistics". SLICE-09 WP-2 adds the
//! accumulator `Stats`. It takes the host time as an argument, so the tests use a fake clock.

#[cfg(test)]
mod tests {
    use crate::planned::planned;

    /// Verifies REQ-083: with a fake clock, `Stats::tick` gives no line before 1.0 s after
    /// `Stats::new`, one line at 1.0 s, and the next line 1.0 s after the time of that
    /// line. The line is exactly
    /// `stats: frame 60 fps 60.0 speed 100% shown 60 emu 2.00 ms turbo off motor - underruns 0`
    /// for 60 frames of 2 ms each in 1.0 s (criterion 1).
    #[test]
    #[ignore = "planned: SLICE-09"]
    fn stats_line_each_second() {
        planned();
    }

    /// Verifies REQ-083: `fps` is the frames of the second divided by its length with one
    /// decimal (30 frames in 1.25 s give `24.0`), and `speed` is `fps` divided by 59.92
    /// (NTSC) or 49.86 (PAL) times 100, rounded to an integer: 120 frames in 1.0 s give
    /// `200%` (NTSC) and `241%` (PAL) (criteria 2 and 3).
    #[test]
    #[ignore = "planned: SLICE-09"]
    fn stats_frame_rate_and_speed() {
        planned();
    }

    /// Verifies REQ-083: `shown` and `underruns` are the increase of the `Counters` totals
    /// in the second, and both are 0 with `Counters::default()`, as the headless loop gives
    /// (criteria 4 and 7).
    #[test]
    #[ignore = "planned: SLICE-09"]
    fn stats_shown_and_underruns() {
        planned();
    }

    /// Verifies REQ-083: `emu` is the mean `FrameStats::emulation` of the frames of the
    /// second in ms with two decimals (1 ms and 2 ms give `1.50`), it does not include the
    /// host time between the frames, and it is `0.00` in a second without frames (criterion
    /// 5).
    #[test]
    #[ignore = "planned: SLICE-09"]
    fn stats_emulation_time() {
        planned();
    }

    /// Verifies REQ-083: `turbo` and `motor` are the values of the last frame of the second
    /// (`on`, `off`, and `on`, `off` or `-`), and `Motor::of` gives `-` for a machine
    /// without a tape and `on` while the motor line of an inserted tape is low (criterion 6).
    #[test]
    #[ignore = "planned: SLICE-09"]
    fn stats_turbo_and_motor() {
        planned();
    }
}
