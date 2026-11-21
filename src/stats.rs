//! The speed statistics of `--stats` (REQ-083).
//!
//! Design: `docs/design/architecture.md`, "Speed statistics". The accumulator [`Stats`]
//! takes the host time as an argument, so the tests use a fake clock.

use std::time::{Duration, Instant};

use atari_core::{Machine, VideoStandard};

/// One second of statistics.
pub const INTERVAL: Duration = Duration::from_secs(1);

/// The cassette motor at a frame (REQ-083 criterion 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motor {
    /// A tape is inserted and the motor line is low.
    On,
    /// A tape is inserted and the motor line is high.
    Off,
    /// No tape is inserted.
    NoTape,
}

impl Motor {
    /// `NoTape` when `Machine::tape_position()` is `None`, else `On` or `Off` from the deck.
    #[must_use]
    pub fn of(machine: &Machine) -> Self {
        if machine.tape_position().is_none() {
            Self::NoTape
        } else if machine.tape_motor_on() {
            Self::On
        } else {
            Self::Off
        }
    }

    /// `on`, `off`, `-`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::NoTape => "-",
        }
    }
}

/// The totals that other parts of the frontend count since the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    /// The frames that the window showed. 0 with `--headless`.
    pub shown: u64,
    /// The audio underruns (REQ-011). 0 with `--headless`.
    pub underruns: u64,
}

/// One frame that ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    /// The host time of the emulation of the frame: `Frontend::step` or one frame of
    /// `Frontend::run_frames`, without the wait and without the window update.
    pub emulation: Duration,
    /// `Frontend::turbo()` after the frame.
    pub turbo: bool,
    /// `Motor::of` the machine after the frame.
    pub motor: Motor,
}

/// The accumulator of the statistics lines: one line for each second of host time.
#[derive(Debug, Clone, Copy)]
pub struct Stats {
    /// The frame rate of the video standard (REQ-201).
    frames_per_second: f64,
    /// The start of the current second.
    start: Instant,
    /// The totals at the start of the current second.
    counters: Counters,
    /// The frames since [`Stats::new`].
    frames: u64,
    /// The frames of the current second.
    second_frames: u32,
    /// The sum of the emulation times of the frames of the current second.
    emulation: Duration,
    /// The turbo of the last frame that ran.
    turbo: bool,
    /// The motor of the last frame that ran.
    motor: Motor,
}

impl Stats {
    /// Starts the first second at `now`, the start of the first frame. `counters` are the
    /// totals at `now`, and `motor` is the motor before the first frame.
    #[must_use]
    pub fn new(standard: VideoStandard, now: Instant, counters: Counters, motor: Motor) -> Self {
        Self {
            frames_per_second: standard.frames_per_second(),
            start: now,
            counters,
            frames: 0,
            second_frames: 0,
            emulation: Duration::ZERO,
            turbo: false,
            motor,
        }
    }

    /// Counts one frame that ran.
    pub fn frame(&mut self, frame: FrameStats) {
        self.frames += 1;
        self.second_frames = self.second_frames.saturating_add(1);
        self.emulation += frame.emulation;
        self.turbo = frame.turbo;
        self.motor = frame.motor;
    }

    /// Called at `now` after each step. When [`INTERVAL`] or more passed since the start of
    /// the current second, gives the line and starts the next second at `now`.
    pub fn tick(&mut self, now: Instant, counters: Counters) -> Option<String> {
        let length = now.saturating_duration_since(self.start);
        if length < INTERVAL {
            return None;
        }
        let frames = f64::from(self.second_frames);
        let fps = frames / length.as_secs_f64();
        let speed = (fps / self.frames_per_second * 100.0).round();
        let emulation = if self.second_frames == 0 {
            0.0
        } else {
            self.emulation.as_secs_f64() * 1000.0 / frames
        };
        let line = format!(
            "stats: frame {} fps {fps:.1} speed {speed:.0}% shown {} emu {emulation:.2} ms turbo {} motor {} underruns {}",
            self.frames,
            counters.shown.saturating_sub(self.counters.shown),
            if self.turbo { "on" } else { "off" },
            self.motor.name(),
            counters.underruns.saturating_sub(self.counters.underruns),
        );
        self.start = now;
        self.counters = counters;
        self.second_frames = 0;
        self.emulation = Duration::ZERO;
        Some(line)
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "the fake clock starts at the host time; the core reads no clock (REQ-014)"
)]
mod tests {
    use std::time::{Duration, Instant};

    use atari_core::media::Tape;
    use atari_core::{Machine, MachineConfig, VideoStandard};

    use super::{Counters, FrameStats, Motor, Stats};

    /// A frame of `ms` milliseconds of emulation, turbo off, no tape.
    fn frame_ms(ms: u64) -> FrameStats {
        FrameStats {
            emulation: Duration::from_millis(ms),
            turbo: false,
            motor: Motor::NoTape,
        }
    }

    /// Runs `frames` frames of `emulation` each, evenly in `length` of fake host time from
    /// `start`, with a tick after each frame. Returns the lines and the fake time at the end.
    fn run_second(
        stats: &mut Stats,
        start: Instant,
        frames: u32,
        emulation: Duration,
        length: Duration,
    ) -> (Vec<String>, Instant) {
        let mut lines = Vec::new();
        for i in 1..=frames {
            stats.frame(FrameStats {
                emulation,
                ..frame_ms(0)
            });
            let now = start + length * i / frames;
            lines.extend(stats.tick(now, Counters::default()));
        }
        (lines, start + length)
    }

    /// Verifies REQ-083: with a fake clock, `Stats::tick` gives no line before 1.0 s after
    /// `Stats::new`, one line at 1.0 s, and the next line 1.0 s after the time of that
    /// line. The line is exactly
    /// `stats: frame 60 fps 60.0 speed 100% shown 60 emu 2.00 ms turbo off motor - underruns 0`
    /// for 60 frames of 2 ms each in 1.0 s (criterion 1).
    #[test]
    fn stats_line_each_second() {
        let start = Instant::now();
        let mut stats = Stats::new(
            VideoStandard::Ntsc,
            start,
            Counters::default(),
            Motor::NoTape,
        );
        let mut lines = Vec::new();
        for i in 1..=60_u32 {
            stats.frame(frame_ms(2));
            let now = start + Duration::from_secs(1) * i / 60;
            let shown = Counters {
                shown: u64::from(i),
                underruns: 0,
            };
            let line = stats.tick(now, shown);
            if i < 60 {
                assert_eq!(line, None, "no line at frame {i}, before 1.0 s");
            }
            lines.extend(line);
        }
        assert_eq!(
            lines,
            [
                "stats: frame 60 fps 60.0 speed 100% shown 60 emu 2.00 ms turbo off motor - underruns 0"
            ],
            "one line at 1.0 s"
        );
        let first = start + Duration::from_secs(1);
        let totals = Counters {
            shown: 60,
            underruns: 0,
        };
        assert_eq!(
            stats.tick(first + Duration::from_millis(999), totals),
            None,
            "0.999 s"
        );
        stats.frame(frame_ms(2));
        // A late line: the next second starts at the time of the line, not at 2.0 s.
        let second = first + Duration::from_millis(1001);
        assert!(
            stats.tick(second, totals).is_some(),
            "the second line after 1.0 s"
        );
        assert_eq!(
            stats.tick(second + Duration::from_millis(999), totals),
            None,
            "the third second starts at the second line"
        );
        let line = stats.tick(second + Duration::from_secs(1), totals);
        assert!(
            line.as_deref()
                .is_some_and(|line| line.starts_with("stats: frame 61 fps 0.0 ")),
            "the third line, without frames: {line:?}"
        );
        // After a stop of the host, one line and not a series.
        let after_stop = first + Duration::from_secs(10);
        assert!(
            stats.tick(after_stop, totals).is_some(),
            "a line after the stop"
        );
        assert_eq!(
            stats.tick(after_stop, totals),
            None,
            "only one line after the stop"
        );
    }

    /// Verifies REQ-083: `fps` is the frames of the second divided by its length with one
    /// decimal (30 frames in 1.25 s give `24.0`), and `speed` is `fps` divided by 59.92
    /// (NTSC) or 49.86 (PAL) times 100, rounded to an integer: 120 frames in 1.0 s give
    /// `200%` (NTSC) and `241%` (PAL) (criteria 2 and 3).
    #[test]
    fn stats_frame_rate_and_speed() {
        let start = Instant::now();
        let mut stats = Stats::new(
            VideoStandard::Ntsc,
            start,
            Counters::default(),
            Motor::NoTape,
        );
        let (lines, _) = run_second(
            &mut stats,
            start,
            30,
            Duration::ZERO,
            Duration::from_millis(1250),
        );
        // The first tick at 1.0 s or later is at frame 24 (1.0 s): 24 frames in 1.0 s.
        assert_eq!(lines.len(), 1, "one line: {lines:?}");
        assert!(
            lines[0].contains(" fps 24.0 speed 40% "),
            "24 frames in 1.0 s: {}",
            lines[0]
        );
        let mut stats = Stats::new(
            VideoStandard::Ntsc,
            start,
            Counters::default(),
            Motor::NoTape,
        );
        for _ in 0..30 {
            stats.frame(frame_ms(0));
        }
        let line = stats.tick(start + Duration::from_millis(1250), Counters::default());
        assert!(
            line.as_deref()
                .is_some_and(|line| line.contains(" fps 24.0 speed 40% ")),
            "30 frames in 1.25 s: {line:?}"
        );
        for (standard, speed) in [(VideoStandard::Ntsc, "200%"), (VideoStandard::Pal, "241%")] {
            let mut stats = Stats::new(standard, start, Counters::default(), Motor::NoTape);
            let (lines, _) = run_second(
                &mut stats,
                start,
                120,
                Duration::ZERO,
                Duration::from_secs(1),
            );
            assert_eq!(lines.len(), 1, "{standard:?}: one line");
            assert!(
                lines[0].starts_with(&format!("stats: frame 120 fps 120.0 speed {speed} ")),
                "{standard:?}: {}",
                lines[0]
            );
        }
        let mut stats = Stats::new(
            VideoStandard::Pal,
            start,
            Counters::default(),
            Motor::NoTape,
        );
        let (lines, _) = run_second(
            &mut stats,
            start,
            50,
            Duration::ZERO,
            Duration::from_secs(1),
        );
        assert!(
            lines[0].contains(" fps 50.0 speed 100% "),
            "PAL real time: {}",
            lines[0]
        );
    }

    /// Verifies REQ-083: `shown` and `underruns` are the increase of the `Counters` totals
    /// in the second, and both are 0 with `Counters::default()`, as the headless loop gives
    /// (criteria 4 and 7).
    #[test]
    fn stats_shown_and_underruns() {
        let start = Instant::now();
        let totals = |shown, underruns| Counters { shown, underruns };
        let mut stats = Stats::new(VideoStandard::Ntsc, start, totals(5, 2), Motor::NoTape);
        stats.frame(frame_ms(1));
        let line = stats.tick(start + Duration::from_secs(1), totals(45, 5));
        assert!(
            line.as_deref()
                .is_some_and(|line| line.contains(" shown 40 ") && line.ends_with(" underruns 3")),
            "the increase in the first second: {line:?}"
        );
        stats.frame(frame_ms(1));
        let line = stats.tick(start + Duration::from_secs(2), totals(100, 5));
        assert!(
            line.as_deref()
                .is_some_and(|line| line.contains(" shown 55 ") && line.ends_with(" underruns 0")),
            "the increase in the second second: {line:?}"
        );
        let mut stats = Stats::new(
            VideoStandard::Ntsc,
            start,
            Counters::default(),
            Motor::NoTape,
        );
        let (lines, _) = run_second(
            &mut stats,
            start,
            60,
            Duration::ZERO,
            Duration::from_secs(1),
        );
        assert!(
            lines[0].contains(" shown 0 ") && lines[0].ends_with(" underruns 0"),
            "headless: {}",
            lines[0]
        );
    }

    /// Verifies REQ-083: `emu` is the mean `FrameStats::emulation` of the frames of the
    /// second in ms with two decimals (1 ms and 2 ms give `1.50`), it does not include the
    /// host time between the frames, and it is `0.00` in a second without frames (criterion
    /// 5).
    #[test]
    fn stats_emulation_time() {
        let start = Instant::now();
        let mut stats = Stats::new(
            VideoStandard::Ntsc,
            start,
            Counters::default(),
            Motor::NoTape,
        );
        stats.frame(frame_ms(1));
        assert_eq!(
            stats.tick(start + Duration::from_millis(500), Counters::default()),
            None,
            "0.5 s"
        );
        stats.frame(frame_ms(2));
        // The frames took 3 ms of the 1.0 s: the time between them does not count.
        let line = stats.tick(start + Duration::from_secs(1), Counters::default());
        assert!(
            line.as_deref()
                .is_some_and(|line| line.contains(" emu 1.50 ms ")),
            "1 ms and 2 ms: {line:?}"
        );
        let mut stats = Stats::new(
            VideoStandard::Ntsc,
            start,
            Counters::default(),
            Motor::NoTape,
        );
        let (lines, _) = run_second(
            &mut stats,
            start,
            3,
            Duration::from_micros(16_667),
            Duration::from_secs(1),
        );
        assert!(
            lines[0].contains(" emu 16.67 ms "),
            "rounded to two decimals: {}",
            lines[0]
        );
        let line = stats.tick(start + Duration::from_secs(2), Counters::default());
        assert!(
            line.as_deref().is_some_and(
                |line| line.contains(" fps 0.0 speed 0% ") && line.contains(" emu 0.00 ms ")
            ),
            "a second without frames: {line:?}"
        );
    }

    /// A machine whose OS ROM writes `pactl` to `PACTL` in a loop, with `tape` in the deck,
    /// after one frame.
    fn pactl_machine(pactl: u8, tape: Option<Tape>) -> Machine {
        let mut rom = vec![0xEA; 0x4000];
        let program = [
            0xA9, pactl, // LDA #pactl
            0x8D, 0x02, 0xD3, // loop: STA PACTL
            0x4C, 0x02, 0xC0, // JMP loop
        ];
        rom[..program.len()].copy_from_slice(&program);
        rom[0x3FF0] = 0x40; // RTI
        rom[0x3FFA..].copy_from_slice(&[0xF0, 0xFF, 0x00, 0xC0, 0xF0, 0xFF]);
        let mut machine = Machine::new(MachineConfig {
            os_rom: rom,
            basic_rom: None,
            cartridge: None,
            disk: None,
            fast_sio: false,
            tape,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: 48_000,
        })
        .unwrap();
        machine.run_frame();
        machine
    }

    /// A tape with one `data` chunk of 16 bytes.
    fn tape() -> Tape {
        let mut file = b"FUJI".to_vec();
        file.extend([0; 4]);
        file.extend(b"data");
        file.extend(16_u16.to_le_bytes());
        file.extend(100_u16.to_le_bytes());
        file.extend([0x55; 16]);
        Tape::from_cas(&file).unwrap()
    }

    /// Verifies REQ-083: `turbo` and `motor` are the values of the last frame of the second
    /// (`on`, `off`, and `on`, `off` or `-`), and `Motor::of` gives `-` for a machine
    /// without a tape and `on` while the motor line of an inserted tape is low (criterion 6).
    #[test]
    fn stats_turbo_and_motor() {
        // PACTL $34: the motor line (CA2) is low; $3C: high.
        assert_eq!(
            Motor::of(&pactl_machine(0x34, None)),
            Motor::NoTape,
            "no tape, line low"
        );
        assert_eq!(
            Motor::of(&pactl_machine(0x34, Some(tape()))),
            Motor::On,
            "tape, line low"
        );
        assert_eq!(
            Motor::of(&pactl_machine(0x3C, Some(tape()))),
            Motor::Off,
            "tape, line high"
        );
        assert_eq!(
            [Motor::On, Motor::Off, Motor::NoTape].map(Motor::name),
            ["on", "off", "-"],
            "the names"
        );
        let start = Instant::now();
        let mut stats = Stats::new(VideoStandard::Ntsc, start, Counters::default(), Motor::Off);
        let line = stats.tick(start + Duration::from_secs(1), Counters::default());
        assert!(
            line.as_deref()
                .is_some_and(|line| line.contains(" turbo off motor off ")),
            "before the first frame: the motor of new: {line:?}"
        );
        for (turbo, motor, text) in [
            (true, Motor::On, " turbo on motor on "),
            (false, Motor::Off, " turbo off motor off "),
            (true, Motor::NoTape, " turbo on motor - "),
        ] {
            let mut stats = Stats::new(
                VideoStandard::Ntsc,
                start,
                Counters::default(),
                Motor::NoTape,
            );
            stats.frame(FrameStats {
                emulation: Duration::ZERO,
                turbo: !turbo,
                motor: Motor::Off,
            });
            stats.frame(FrameStats {
                emulation: Duration::ZERO,
                turbo,
                motor,
            });
            let line = stats.tick(start + Duration::from_secs(1), Counters::default());
            assert!(
                line.as_deref().is_some_and(|line| line.contains(text)),
                "the last frame gives{text}: {line:?}"
            );
        }
    }
}
