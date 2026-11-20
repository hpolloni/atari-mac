//! The frontend logic that does not need a window: machine setup, input, pause, frame
//! conversion, frame pacing and the PNG screenshot.
//!
//! Design: `docs/design/architecture.md`, "Frontend". The window loop in
//! [`crate::window`] only moves data between `minifb` and the functions here.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;
use std::time::{Duration, Instant};

use atari_core::gtia::{FRAME_HEIGHT, FRAME_WIDTH};
use atari_core::media::Cartridge;
use atari_core::{ConfigError, Machine, MachineConfig, VideoStandard, palette};
use minifb::Key;

use crate::bas::{NotReady, READY_TIMEOUT_FRAMES, Typist};
use crate::cli::Options;
use crate::error::FrontendError;
use crate::keymap::InputState;

/// The audio sample rate that the machine is configured with. Audio output is not part of
/// this version; the frontend does not drain the samples.
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Reads a whole file.
fn read(path: &Path) -> Result<Vec<u8>, FrontendError> {
    fs::read(path).map_err(|source| FrontendError::Read {
        path: path.to_owned(),
        source,
    })
}

/// Creates the machine from the ROM and media files of `options` (REQ-004, REQ-012).
///
/// # Errors
///
/// [`FrontendError::Read`] when a file cannot be read, [`FrontendError::Rom`] for a ROM
/// with the wrong size and [`FrontendError::Media`] for a cartridge that is not valid.
pub fn load_machine(options: &Options) -> Result<Machine, FrontendError> {
    let os_rom = read(&options.os)?;
    let basic_rom = read(&options.basic)?;
    let cartridge = match &options.cart {
        Some(path) => {
            Some(
                Cartridge::from_raw(read(path)?).map_err(|error| FrontendError::Media {
                    path: path.clone(),
                    error,
                })?,
            )
        }
        None => None,
    };
    Machine::new(MachineConfig {
        os_rom,
        basic_rom: Some(basic_rom),
        cartridge,
        standard: options.standard,
        audio_sample_rate: AUDIO_SAMPLE_RATE,
    })
    .map_err(|error| FrontendError::Rom {
        path: match error {
            ConfigError::OsRomSize(_) => options.os.clone(),
            ConfigError::BasicRomSize(_) => options.basic.clone(),
        },
        error,
    })
}

/// The window title (REQ-008): the file name of the loaded media, or `atari-mac`.
#[must_use]
pub fn window_title(media: Option<&Path>) -> String {
    media.and_then(Path::file_name).map_or_else(
        || "atari-mac".to_owned(),
        |name| format!("atari-mac - {}", name.display()),
    )
}

/// The window content size in pixels for `scale` (REQ-008).
#[must_use]
pub const fn window_size(scale: usize) -> (usize, usize) {
    (FRAME_WIDTH * scale, FRAME_HEIGHT * scale)
}

/// The optional display filters (REQ-019, REQ-411, REQ-412). `Default` turns both off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DisplayFilters {
    /// The NTSC artifacting filter (`--artifacting`, REQ-411).
    pub artifacting: bool,
    /// The PAL chroma blending filter (`--pal-blending`, REQ-412).
    pub pal_blending: bool,
}

/// Converts frame buffer row `y` to `0x00RRGGBB` pixels in `out`.
///
/// Without a filter, each pixel is `palette::rgb` of its byte. Artifacting works on NTSC rows only, and PAL
/// blending on PAL rows 1-239 only (row 0 has no row above it, OI-021).
pub fn convert_row(
    frame: &[u8],
    y: usize,
    standard: VideoStandard,
    filters: DisplayFilters,
    out: &mut [u32; FRAME_WIDTH],
) {
    let row = &frame[y * FRAME_WIDTH..(y + 1) * FRAME_WIDTH];
    match standard {
        VideoStandard::Ntsc if filters.artifacting => palette::artifact_ntsc_row(row, out),
        VideoStandard::Pal if filters.pal_blending && y > 0 => {
            let above = &frame[(y - 1) * FRAME_WIDTH..y * FRAME_WIDTH];
            palette::pal_blend_row(above, row, out);
        }
        _ => {
            for (slot, &color) in out.iter_mut().zip(row) {
                *slot = palette::rgb(standard, color);
            }
        }
    }
}

/// Converts the frame buffer to `0x00RRGGBB` pixels with `filters` and scales it by
/// `scale`: each frame buffer pixel becomes a block of `scale` x `scale` pixels in `out`
/// (REQ-008).
pub fn render(
    frame: &[u8],
    standard: VideoStandard,
    filters: DisplayFilters,
    scale: usize,
    out: &mut Vec<u32>,
) {
    out.clear();
    let mut pixels = [0; FRAME_WIDTH];
    for y in 0..frame.len() / FRAME_WIDTH {
        convert_row(frame, y, standard, filters, &mut pixels);
        let start = out.len();
        for &rgb in &pixels {
            out.extend((0..scale).map(|_| rgb));
        }
        let end = out.len();
        for _ in 1..scale {
            out.extend_from_within(start..end);
        }
    }
}

/// Writes the frame buffer as a 384 x 240 RGB PNG file (REQ-013). Each pixel is the
/// output of [`convert_row`]: `palette::rgb` of the frame buffer byte without a filter.
///
/// # Errors
///
/// [`FrontendError::Screenshot`] when the file cannot be written.
pub fn write_png(
    path: &Path,
    frame: &[u8],
    standard: VideoStandard,
    filters: DisplayFilters,
) -> Result<(), FrontendError> {
    let error = |source| FrontendError::Screenshot {
        path: path.to_owned(),
        source,
    };
    let file = File::create(path).map_err(|e| error(e.into()))?;
    let width = u32::try_from(FRAME_WIDTH).unwrap_or(u32::MAX);
    let height = u32::try_from(FRAME_HEIGHT).unwrap_or(u32::MAX);
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut data = Vec::with_capacity(frame.len() * 3);
    let mut pixels = [0; FRAME_WIDTH];
    for y in 0..frame.len() / FRAME_WIDTH {
        convert_row(frame, y, standard, filters, &mut pixels);
        data.extend(pixels.iter().flat_map(|&rgb| {
            let [_, r, g, b] = rgb.to_be_bytes();
            [r, g, b]
        }));
    }
    let mut writer = encoder.write_header().map_err(error)?;
    writer.write_image_data(&data).map_err(error)?;
    writer.finish().map_err(error)
}

/// Whether the frontend continues after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Continue with the next host frame.
    Continue,
    /// The user asked to quit.
    Quit,
}

/// The machine with the frontend state: the input of the last frame, the pause flag and the
/// typing of a BASIC program (`--bas`, REQ-080).
#[derive(Debug)]
pub struct Frontend {
    /// The emulated machine.
    machine: Machine,
    /// The input of the last host frame.
    input: InputState,
    /// The emulation is paused (F9).
    paused: bool,
    /// The number of frames that [`Frontend::step`] ran.
    frames_run: u64,
    /// The number of machine cycles of the last frame that ran.
    last_frame_cycles: u64,
    /// The typing of the BASIC program, until all keys are typed.
    typist: Option<Typist>,
}

impl Frontend {
    /// Wraps a machine.
    #[must_use]
    pub fn new(machine: Machine) -> Self {
        Self {
            machine,
            input: InputState::default(),
            paused: false,
            frames_run: 0,
            last_frame_cycles: 0,
            typist: None,
        }
    }

    /// Types `typist`'s keys before the frames that come next (REQ-080). While it types,
    /// the host keys other than quit have no effect.
    #[must_use]
    pub fn with_typist(mut self, typist: Typist) -> Self {
        self.typist = Some(typist);
        self
    }

    /// The typing of the BASIC program, while it is not done.
    #[must_use]
    pub const fn typist(&self) -> Option<&Typist> {
        self.typist.as_ref()
    }

    /// The machine.
    #[must_use]
    pub const fn machine(&self) -> &Machine {
        &self.machine
    }

    /// The machine, for changes outside the key input (for example a test that writes RAM).
    pub const fn machine_mut(&mut self) -> &mut Machine {
        &mut self.machine
    }

    /// The emulation is paused.
    #[must_use]
    pub const fn paused(&self) -> bool {
        self.paused
    }

    /// The number of frames that [`Frontend::step`] and [`Frontend::run_frames`] ran.
    #[must_use]
    pub const fn frames_run(&self) -> u64 {
        self.frames_run
    }

    /// The number of machine cycles of the last frame that [`Frontend::step`] or
    /// [`Frontend::run_frames`] ran (REQ-002 criterion 4), or 0 before the first frame.
    #[must_use]
    pub const fn last_frame_cycles(&self) -> u64 {
        self.last_frame_cycles
    }

    /// Runs one frame and counts it. Before the frame, the typist presses or releases a key.
    fn run_frame(&mut self) -> Result<(), FrontendError> {
        if let Some(typist) = &mut self.typist {
            typist.before_frame(&mut self.machine).map_err(|NotReady| {
                FrontendError::BasicNotStarted {
                    frames: READY_TIMEOUT_FRAMES,
                }
            })?;
            if typist.done() {
                self.typist = None;
            }
        }
        let start = self.machine.cycle_count();
        self.machine.run_frame();
        self.last_frame_cycles = self.machine.cycle_count() - start;
        self.frames_run += 1;
        Ok(())
    }

    /// One host frame: sends the host keys that are down to the machine, then runs one
    /// frame unless the emulation is paused. While a BASIC program is typed, only quit
    /// has an effect, and the frame always runs.
    ///
    /// # Errors
    ///
    /// [`FrontendError::BasicNotStarted`] when the typist does not see the `Ready` prompt.
    pub fn step(&mut self, keys: &[Key]) -> Result<Flow, FrontendError> {
        let input = InputState::from_keys(keys, &self.input);
        if input.quit {
            return Ok(Flow::Quit);
        }
        if self.typist.is_some() {
            return self.run_frame().map(|()| Flow::Continue);
        }
        self.apply(&input);
        self.input = input;
        if !self.paused {
            self.run_frame()?;
        }
        Ok(Flow::Continue)
    }

    /// Runs `frames` frames without input (headless mode, REQ-013).
    ///
    /// # Errors
    ///
    /// [`FrontendError::BasicNotStarted`] when the typist does not see the `Ready` prompt.
    pub fn run_frames(&mut self, frames: u64) -> Result<(), FrontendError> {
        for _ in 0..frames {
            self.run_frame()?;
        }
        Ok(())
    }

    /// Sends the changes from the last input to `input` to the machine.
    fn apply(&mut self, input: &InputState) {
        let previous = &self.input;
        let machine = &mut self.machine;
        // While an Atari key is down, the key code gives SHIFT; else the host Shift keys.
        let shift = input.key.is_none() && input.shift;
        if shift != (previous.key.is_none() && previous.shift) {
            machine.set_shift(shift);
        }
        if input.key != previous.key {
            match input.key {
                Some(code) => machine.key_down(code),
                None => machine.key_up(),
            }
        }
        machine.set_break(input.break_key);
        machine.set_console_keys(input.console);
        machine.set_joystick(0, input.joystick);
        if input.reset && !previous.reset {
            machine.press_reset();
        }
        if input.cold_reset && !previous.cold_reset {
            machine.power_on();
        }
        if input.pause && !previous.pause {
            self.paused = !self.paused;
        }
    }
}

/// Frame pacing: the time to wait after each frame, so that the frames come at the frame
/// rate of the machine (REQ-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pacer {
    /// The time of one frame.
    period: Duration,
    /// The time at which the next frame starts.
    next: Instant,
}

impl Pacer {
    /// Starts the pacing at `now` for the frame rate of `standard`.
    #[must_use]
    pub fn new(standard: VideoStandard, now: Instant) -> Self {
        let period = Duration::from_secs_f64(1.0 / standard.frames_per_second());
        Self {
            period,
            next: now + period,
        }
    }

    /// The time of one frame.
    #[must_use]
    pub const fn period(&self) -> Duration {
        self.period
    }

    /// Called at `now`, after a frame: returns the time to wait until the next frame. When
    /// the frontend is more than one frame late, the pacing starts again from `now`.
    pub fn wait(&mut self, now: Instant) -> Duration {
        let wait = self.next.saturating_duration_since(now);
        if now > self.next + self.period {
            self.next = now + self.period;
        } else {
            self.next += self.period;
        }
        wait
    }
}

/// The host time after which the window shows the next frame in turbo (REQ-080).
///
/// While the window types a BASIC program (criterion 7), one frame and one window update
/// take much less than 50 ms, so the window shows a frame at least every 100 ms.
pub const TURBO_PRESENT_INTERVAL: Duration = Duration::from_millis(50);

/// What the window does after a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AfterFrame {
    /// Show the frame in the window.
    pub present: bool,
    /// The time to wait before the next frame.
    pub wait: Duration,
}

/// The frame pacing of the window: real time or turbo.
///
/// Real time is [`Pacer`] (REQ-007). Turbo is used while a BASIC program is typed (REQ-080
/// criterion 7). In turbo, the frames run without a wait, and the
/// window shows a frame each [`TURBO_PRESENT_INTERVAL`] of host time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pacing {
    /// The video standard, for a new [`Pacer`] after turbo.
    standard: VideoStandard,
    /// The real-time pacing.
    pacer: Pacer,
    /// The time at which the window showed a frame last.
    last_present: Instant,
    /// The last frame ran in turbo.
    turbo: bool,
}

impl Pacing {
    /// Starts the pacing at `now`, when the window shows its first frame.
    #[must_use]
    pub fn new(standard: VideoStandard, now: Instant) -> Self {
        Self {
            standard,
            pacer: Pacer::new(standard, now),
            last_present: now,
            turbo: false,
        }
    }

    /// Called at `now`, after a frame. `turbo` is `true` while the BASIC program is typed
    /// ([`Frontend::typist`]). After turbo, the real-time pacing starts again from `now`.
    pub fn after_frame(&mut self, turbo: bool, now: Instant) -> AfterFrame {
        if turbo {
            self.turbo = true;
            let present =
                now.saturating_duration_since(self.last_present) >= TURBO_PRESENT_INTERVAL;
            if present {
                self.last_present = now;
            }
            return AfterFrame {
                present,
                wait: Duration::ZERO,
            };
        }
        if self.turbo {
            self.turbo = false;
            self.pacer = Pacer::new(self.standard, now);
        }
        self.last_present = now;
        AfterFrame {
            present: true,
            wait: self.pacer.wait(now),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env::temp_dir;
    use std::fs::{create_dir_all, write};
    use std::path::{Path, PathBuf};
    use std::process::id;
    use std::time::{Duration, Instant};

    use atari_core::gtia::{FRAME_HEIGHT, FRAME_WIDTH};
    use atari_core::{Machine, MachineConfig, VideoStandard, keyboard, palette};
    use minifb::Key;

    use super::{
        DisplayFilters, Flow, Frontend, Pacer, Pacing, TURBO_PRESENT_INTERVAL, convert_row,
        load_machine, render, window_size, window_title,
    };
    use crate::bas::{CH, READY_TIMEOUT_FRAMES, Typist, program_codes};
    use crate::cli::Options;
    use crate::error::FrontendError;

    fn machine() -> Machine {
        Machine::new(MachineConfig {
            os_rom: vec![0xEA; 0x4000],
            basic_rom: None,
            cartridge: None,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: 48_000,
        })
        .unwrap()
    }

    fn temp_file(name: &str, size: usize) -> PathBuf {
        let dir = temp_dir().join(format!("atari-mac-unit-{}", id()));
        create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        write(&path, vec![0; size]).unwrap();
        path
    }

    fn step(frontend: &mut Frontend, keys: &[Key]) {
        assert_eq!(frontend.step(keys).unwrap(), Flow::Continue, "{keys:?}");
    }

    #[test]
    fn window_size_is_scaled() {
        assert_eq!(window_size(2), (768, 480), "default scale 2");
        assert_eq!(window_size(3), (1152, 720), "scale 3");
    }

    #[test]
    fn title_names_the_media() {
        assert_eq!(window_title(None), "atari-mac", "no media");
        assert_eq!(
            window_title(Some(Path::new("/games/star raiders.car"))),
            "atari-mac - star raiders.car",
            "cartridge"
        );
    }

    #[test]
    fn render_scales_each_pixel_to_a_block() {
        let mut frame = vec![0_u8; FRAME_WIDTH * FRAME_HEIGHT];
        frame[0] = 0x0E;
        frame[FRAME_WIDTH + 1] = 0x94;
        frame[FRAME_WIDTH * FRAME_HEIGHT - 1] = 0x46;
        for scale in [1, 2, 3] {
            let mut out = vec![1, 2, 3];
            render(
                &frame,
                VideoStandard::Ntsc,
                DisplayFilters::default(),
                scale,
                &mut out,
            );
            let (width, height) = window_size(scale);
            assert_eq!(out.len(), width * height, "size at scale {scale}");
            for y in 0..height {
                for x in 0..width {
                    let color = frame[(y / scale) * FRAME_WIDTH + x / scale];
                    assert_eq!(
                        out[y * width + x],
                        palette::rgb(VideoStandard::Ntsc, color),
                        "pixel ({x}, {y}) at scale {scale}"
                    );
                }
            }
        }
    }

    #[test]
    fn load_machine_reports_the_bad_file() {
        let os = temp_file("os.rom", 0x4000);
        let basic = temp_file("basic.rom", 0x2000);
        let small = temp_file("small.rom", 100);
        let cart = temp_file("cart.rom", 0x2000);
        let options = Options {
            os: os.clone(),
            basic,
            cart: Some(cart),
            ..Options::default()
        };
        assert!(load_machine(&options).is_ok(), "valid files");
        for (bad, expect_path) in [
            (
                Options {
                    os: small.clone(),
                    ..options.clone()
                },
                &small,
            ),
            (
                Options {
                    basic: small.clone(),
                    ..options.clone()
                },
                &small,
            ),
        ] {
            match load_machine(&bad) {
                Err(FrontendError::Rom { path, .. }) => assert_eq!(&path, expect_path, "ROM"),
                other => panic!("{bad:?} gave {other:?}"),
            }
        }
        let bad_cart = Options {
            cart: Some(small.clone()),
            ..options.clone()
        };
        assert!(
            matches!(load_machine(&bad_cart), Err(FrontendError::Media { path, .. }) if path == small),
            "cartridge"
        );
        let missing = Options {
            os: os.with_file_name("missing.rom"),
            ..options
        };
        assert!(
            matches!(load_machine(&missing), Err(FrontendError::Read { .. })),
            "missing file"
        );
    }

    #[test]
    fn step_runs_one_frame_and_quits() {
        let mut frontend = Frontend::new(machine());
        assert_eq!(frontend.step(&[]).unwrap(), Flow::Continue, "no keys");
        assert_eq!(frontend.machine().frame_count(), 1, "one frame");
        assert_eq!(frontend.frames_run(), 1, "one frame run");
        assert_eq!(
            frontend.step(&[Key::LeftSuper, Key::Escape]).unwrap(),
            Flow::Quit,
            "Command + Escape"
        );
        assert_eq!(frontend.machine().frame_count(), 1, "no frame after quit");
        frontend.run_frames(3).unwrap();
        assert_eq!(frontend.machine().frame_count(), 4, "headless frames");
        assert_eq!(frontend.frames_run(), 4, "all frames counted");
        assert_eq!(
            frontend.last_frame_cycles(),
            u64::from(VideoStandard::Ntsc.cycles_per_frame()),
            "cycles of the last frame"
        );
    }

    #[test]
    fn keys_reach_the_machine() {
        let mut frontend = Frontend::new(machine());
        for keys in [
            &[Key::LeftShift][..],
            &[Key::LeftShift, Key::A],
            &[Key::A, Key::B],
            &[Key::F2, Key::F7, Key::Up, Key::LeftAlt],
            &[Key::F5],
            &[],
        ] {
            step(&mut frontend, keys);
        }
        // TRIG0 (joystick 1 trigger) reads 1 when the trigger is up.
        assert_eq!(frontend.machine().peek(0xD010), 1, "trigger released");
        step(&mut frontend, &[Key::LeftAlt]);
        assert_eq!(frontend.machine().peek(0xD010), 0, "trigger pressed");
        frontend.machine_mut().poke_ram(0x1234, 0x99);
        step(&mut frontend, &[Key::F10]);
        assert_eq!(
            frontend.machine().peek_ram(0x1234),
            0,
            "F10 (cold reset) clears RAM"
        );
        frontend.machine_mut().poke_ram(0x1234, 0x99);
        step(&mut frontend, &[Key::F10]);
        assert_eq!(
            frontend.machine().peek_ram(0x1234),
            0x99,
            "a held F10 resets only once"
        );
    }

    /// While a program is typed, the host keys except quit have no effect, and a machine
    /// without the `Ready` prompt stops the frontend after the timeout (REQ-080).
    #[test]
    fn typing_ignores_host_keys_until_ready_timeout() {
        let mut frontend = Frontend::new(machine()).with_typist(Typist::new(vec![keyboard::A]));
        assert!(frontend.typist().is_some(), "typing");
        step(&mut frontend, &[Key::F9]);
        assert!(!frontend.paused(), "F9 does not pause while typing");
        assert_eq!(frontend.frames_run(), 1, "one frame");
        frontend
            .run_frames(u64::from(READY_TIMEOUT_FRAMES) - 1)
            .unwrap();
        assert!(
            matches!(
                frontend.step(&[]),
                Err(FrontendError::BasicNotStarted { frames: 600 })
            ),
            "timeout in step"
        );
        assert!(
            matches!(
                frontend.run_frames(1),
                Err(FrontendError::BasicNotStarted { .. })
            ),
            "timeout in run_frames"
        );
        assert_eq!(
            frontend.step(&[Key::LeftSuper, Key::Escape]).unwrap(),
            Flow::Quit,
            "quit while typing"
        );
    }

    /// Verifies REQ-080: in the window, the frames run without the real-time pacing from the
    /// start of the machine until the typing is done, a frame is shown at least every 100 ms
    /// of host time, and then the frames run at real-time speed again (criterion 7).
    ///
    /// The test drives `Frontend::step` and `Pacing::after_frame` as the window loop does,
    /// with a fake clock: each frame takes 3 ms of host time. The machine has no OS; the
    /// `Ready` row and an empty `CH` are written to its RAM.
    #[test]
    fn bas_turbo_until_typed() {
        let mut machine = machine();
        machine.poke_ram(0x58, 0x00);
        machine.poke_ram(0x59, 0x10);
        for (i, code) in [0x32, 0x65, 0x61, 0x64, 0x79].into_iter().enumerate() {
            machine.poke_ram(0x1000 + u16::try_from(i).unwrap(), code);
        }
        machine.poke_ram(CH, 0xFF);
        let codes = program_codes(Path::new("p.bas"), "10 PRINT \"AA11\"\n", true).unwrap();
        let mut frontend = Frontend::new(machine).with_typist(Typist::new(codes));
        let frame_time = Duration::from_millis(3);
        let start = Instant::now();
        let mut pacing = Pacing::new(VideoStandard::Ntsc, start);
        let period = Pacer::new(VideoStandard::Ntsc, start).period();
        let mut clock = start;
        let mut last_present = start;
        let mut longest_gap = Duration::ZERO;
        let mut turbo_frames = 0_u32;
        let mut shown = false;
        while frontend.typist().is_some() {
            step(&mut frontend, &[]);
            clock += frame_time;
            let after = pacing.after_frame(frontend.typist().is_some(), clock);
            if frontend.typist().is_some() {
                turbo_frames += 1;
                assert_eq!(after.wait, Duration::ZERO, "no wait in turbo");
            }
            if after.present {
                longest_gap = longest_gap.max(clock - last_present);
                last_present = clock;
            }
            shown = after.present;
            clock += after.wait;
        }
        assert!(turbo_frames > 40, "the typing took {turbo_frames} frames");
        assert!(
            clock - start < period * turbo_frames / 4,
            "turbo is faster than real time: {:?}",
            clock - start
        );
        assert!(
            longest_gap >= TURBO_PRESENT_INTERVAL && longest_gap <= Duration::from_millis(100),
            "a frame at least every 100 ms: {longest_gap:?}"
        );
        assert!(shown, "the frame after the typing is shown");
        // Real-time speed again: each frame is shown, and a frame takes one period.
        for frame in 0..10 {
            let before = clock;
            step(&mut frontend, &[]);
            clock += frame_time;
            let after = pacing.after_frame(frontend.typist().is_some(), clock);
            assert!(after.present, "frame {frame} is shown");
            clock += after.wait;
            assert!(
                (clock - before).abs_diff(period) < Duration::from_micros(1),
                "frame {frame} takes one period: {:?}",
                clock - before
            );
        }
    }

    /// While paused, `Machine::frame_count` does not change for 60 host frames.
    ///
    /// Verifies REQ-017
    #[test]
    fn pause_stops_frames() {
        let mut frontend = Frontend::new(machine());
        step(&mut frontend, &[]);
        step(&mut frontend, &[Key::F9]);
        assert!(frontend.paused(), "F9 pauses");
        let frames = frontend.machine().frame_count();
        let start = Instant::now();
        let mut pacer = Pacer::new(VideoStandard::Ntsc, start);
        let mut clock = start;
        for _ in 0..60 {
            step(&mut frontend, &[Key::F9]);
            clock += pacer.wait(clock);
        }
        assert_eq!(
            frontend.machine().frame_count(),
            frames,
            "no frame while paused"
        );
        assert!(clock > start, "the fake clock advanced");
        step(&mut frontend, &[]);
        step(&mut frontend, &[Key::F9]);
        assert!(!frontend.paused(), "F9 again continues");
        assert_eq!(
            frontend.machine().frame_count(),
            frames + 1,
            "frames run again"
        );
    }

    #[test]
    fn pacer_keeps_the_frame_rate() {
        let start = Instant::now();
        let mut pacer = Pacer::new(VideoStandard::Ntsc, start);
        let period = pacer.period();
        assert!(
            (period.as_secs_f64() - 1.0 / 59.9227).abs() < 1e-6,
            "NTSC period {period:?}"
        );
        // A frame that took no time waits one period.
        assert_eq!(pacer.wait(start), period, "first wait");
        // A frame that took half a period waits the other half.
        let now = start + period + period / 2;
        let wait = pacer.wait(now);
        assert!(
            wait.abs_diff(period / 2) < Duration::from_micros(1),
            "second wait {wait:?}"
        );
        // A frontend that is far behind does not wait and starts again.
        let late = now + period * 10;
        assert_eq!(pacer.wait(late), Duration::ZERO, "late frame");
        assert_eq!(pacer.wait(late), period, "pacing restarted");
    }

    /// Verifies REQ-411, REQ-412: converts a frame with the default frontend settings and
    /// compares each pixel with `palette::rgb` (REQ-411 and REQ-412 criterion 1: the display
    /// filters are off by default).
    #[test]
    fn display_filters_off_by_default() {
        let filters = Options::default().filters;
        assert_eq!(filters, DisplayFilters::default(), "no filter by default");
        let frame: Vec<u8> = (0..FRAME_WIDTH * FRAME_HEIGHT)
            .map(|i| u8::try_from(i % 251).unwrap())
            .collect();
        for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
            let mut out = Vec::new();
            render(&frame, standard, filters, 1, &mut out);
            for (i, (&pixel, &color)) in out.iter().zip(&frame).enumerate() {
                assert_eq!(
                    pixel,
                    palette::rgb(standard, color),
                    "{standard:?} pixel {i}"
                );
            }
        }
    }

    /// Verifies REQ-019: each filter works only on its own video standard, and PAL blending
    /// keeps row 0 (REQ-411, REQ-412).
    #[test]
    fn filters_apply_to_their_standard() {
        let frame: Vec<u8> = (0..FRAME_WIDTH * FRAME_HEIGHT)
            .map(|i| if i % 2 == 0 { 0x0E } else { 0x48 })
            .collect();
        let both = DisplayFilters {
            artifacting: true,
            pal_blending: true,
        };
        let row = |y: usize| &frame[y * FRAME_WIDTH..(y + 1) * FRAME_WIDTH];
        let mut out = [0; FRAME_WIDTH];
        let mut expected = [0; FRAME_WIDTH];
        convert_row(&frame, 5, VideoStandard::Ntsc, both, &mut out);
        palette::artifact_ntsc_row(row(5), &mut expected);
        assert_eq!(out, expected, "NTSC: artifacting");
        convert_row(&frame, 5, VideoStandard::Pal, both, &mut out);
        palette::pal_blend_row(row(4), row(5), &mut expected);
        assert_eq!(out, expected, "PAL: blending");
        convert_row(&frame, 0, VideoStandard::Pal, both, &mut out);
        let plain: Vec<u32> = row(0)
            .iter()
            .map(|&c| palette::rgb(VideoStandard::Pal, c))
            .collect();
        assert_eq!(out[..], plain[..], "PAL: row 0 is not blended");
    }
}
