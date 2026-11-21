//! The frontend logic that does not need a window: machine setup, input, pause, frame
//! conversion, frame pacing and the PNG screenshot.
//!
//! Design: `docs/design/architecture.md`, "Frontend". The window loop in
//! [`crate::window`] only moves data between `minifb` and the functions here.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;
use std::time::{Duration, Instant};

use atari_core::debug::RunEnd;
use atari_core::gtia::{FRAME_HEIGHT, FRAME_WIDTH};
use atari_core::keyboard::RETURN;
use atari_core::media::{Cartridge, DiskImage, MediaError, Tape, Xex};
use atari_core::{ConfigError, Machine, MachineConfig, VideoStandard, palette};
use minifb::Key;

use crate::audio::AudioSink;
use crate::bas::{NotReady, READY_TIMEOUT_FRAMES, Typist};
use crate::cli::Options;
use crate::error::FrontendError;
use crate::keymap::InputState;
use crate::logging::FRONTEND_TARGET;
use crate::trace::TraceWriter;

/// Sends one record of the category `frontend` (REQ-085 criterion 3) with the frame count
/// and the cycle count of `machine` at the call: the counts at the start of the frame that
/// comes next.
fn record(machine: &Machine, message: &str) {
    log::info!(
        target: FRONTEND_TARGET,
        frame = machine.frame_count(),
        cycle = machine.cycle_count();
        "{message}"
    );
}

/// Why the frames run in turbo ([`Frontend::turbo_cause`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurboCause {
    /// The typist types the BASIC program (REQ-080 criterion 7).
    Typing,
    /// A tape is inserted and the motor line is low (REQ-082).
    TapeMotor,
}

/// Reads a whole file.
fn read(path: &Path) -> Result<Vec<u8>, FrontendError> {
    fs::read(path).map_err(|source| FrontendError::Read {
        path: path.to_owned(),
        source,
    })
}

/// A cartridge from the bytes of a `--cart` file: a CAR file if it starts with `CART`
/// (REQ-605, REQ-606), else a raw image (REQ-604).
///
/// # Errors
///
/// The [`MediaError`] of the CAR or raw image.
pub fn cartridge_from_file(bytes: Vec<u8>) -> Result<Cartridge, MediaError> {
    if bytes.starts_with(b"CART") {
        Cartridge::from_car(&bytes)
    } else {
        Cartridge::from_raw(bytes)
    }
}

/// Reads and parses the executable file of `--xex` (REQ-012, REQ-610, REQ-611).
///
/// # Errors
///
/// [`FrontendError::Read`] when the file cannot be read, and [`FrontendError::Media`] when
/// it is not a valid XEX file.
pub fn load_xex(path: &Path) -> Result<Xex, FrontendError> {
    Xex::parse(&read(path)?).map_err(|error| FrontendError::Media {
        path: path.to_owned(),
        error,
    })
}

/// The warnings for an executable that loads, but not completely.
///
/// They name the first defect of the file (REQ-611) and a segment that overlaps the loader
/// memory (REQ-612 Notes), as `docs/design/media.md`, "Executable files", gives.
#[must_use]
pub fn xex_warnings(xex: &Xex) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(defect) = xex.defect() {
        warnings.push(format!(
            "{} at offset {}: the segments after it do not load",
            defect.kind.describe(),
            defect.offset
        ));
    }
    if let Some(start) = xex.loader_conflict() {
        warnings.push(format!(
            "the segment at ${start:04X} overlaps the loader memory $0700-$08FF"
        ));
    }
    warnings
}

/// Creates the machine from the ROM and media files of `options` (REQ-004, REQ-012), with
/// the audio sample rate `sample_rate` (the rate of the audio stream, REQ-011).
///
/// # Errors
///
/// [`FrontendError::Read`] when a file cannot be read, [`FrontendError::Rom`] for a ROM
/// with the wrong size and [`FrontendError::Media`] for a cartridge, an executable or a disk
/// image that is not valid. With `--xex`, the machine starts the executable
/// ([`Machine::start_xex`], REQ-612). With `--cas`, the machine boots the tape
/// ([`Machine::start_tape`], REQ-081): START is held from the cold reset until frame 60.
pub fn load_machine(options: &Options, sample_rate: u32) -> Result<Machine, FrontendError> {
    let os_rom = read(&options.os)?;
    let basic_rom = read(&options.basic)?;
    let cartridge = match &options.cart {
        Some(path) => {
            Some(
                cartridge_from_file(read(path)?).map_err(|error| FrontendError::Media {
                    path: path.clone(),
                    error,
                })?,
            )
        }
        None => None,
    };
    let disk = match &options.disk {
        Some(path) => {
            Some(
                DiskImage::from_atr(&read(path)?).map_err(|error| FrontendError::Media {
                    path: path.clone(),
                    error,
                })?,
            )
        }
        None => None,
    };
    let xex = options.xex.as_deref().map(load_xex).transpose()?;
    let tape = match &options.cas {
        Some(path) => Some(
            Tape::from_cas(&read(path)?).map_err(|error| FrontendError::Media {
                path: path.clone(),
                error,
            })?,
        ),
        None => None,
    };
    let mut machine = Machine::new(MachineConfig {
        os_rom,
        basic_rom: Some(basic_rom),
        cartridge,
        disk,
        fast_sio: false,
        fast_tape: options.fast_tape,
        tape: None,
        standard: options.standard,
        audio_sample_rate: sample_rate,
    })
    .map_err(|error| FrontendError::Rom {
        path: match error {
            ConfigError::OsRomSize(_) => options.os.clone(),
            ConfigError::BasicRomSize(_) => options.basic.clone(),
        },
        error,
    })?;
    if let Some(xex) = &xex {
        machine.start_xex(xex);
    }
    if let Some(tape) = tape {
        machine.start_tape(tape);
    }
    Ok(machine)
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
    /// The audio samples of the last frame that ran (REQ-011).
    audio: Vec<f32>,
    /// The last frame that ran started with a tape inserted and the motor line low
    /// (REQ-082). `false` when the last step ran no frame.
    tape_turbo: bool,
    /// The turbo cause of the last frame that ran (REQ-085 criterion 3).
    turbo_cause: Option<TurboCause>,
    /// The trace writer of `--trace` (REQ-086).
    trace: Option<TraceWriter<BufWriter<File>>>,
    /// `--auto-return` (REQ-092): the frontend presses RETURN for the tape boot, until it
    /// releases the key.
    auto_return: bool,
}

/// The first frame of the RETURN press of `--auto-return` (REQ-092 criterion 1, the frames
/// of REQ-081 criterion 4): 180 (NTSC) or 150 (PAL).
#[must_use]
pub const fn auto_return_frame(standard: VideoStandard) -> u64 {
    match standard {
        VideoStandard::Ntsc => 180,
        VideoStandard::Pal => 150,
    }
}

/// The frames that `--auto-return` holds RETURN (REQ-092 criterion 1).
pub const AUTO_RETURN_FRAMES: u64 = 6;

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
            audio: Vec::new(),
            tape_turbo: false,
            turbo_cause: None,
            trace: None,
            auto_return: false,
        }
    }

    /// Presses RETURN from frame [`auto_return_frame`] for [`AUTO_RETURN_FRAMES`] frames
    /// (`--auto-return`, REQ-092), counted from the cold reset of the tape boot.
    #[must_use]
    pub const fn with_auto_return(mut self) -> Self {
        self.auto_return = true;
        self
    }

    /// Presses or releases RETURN before the frame for `--auto-return` (REQ-092).
    const fn auto_return_key(&mut self) {
        let press = auto_return_frame(self.machine.standard());
        let frame = self.machine.frame_count();
        if frame == press {
            self.machine.key_down(RETURN);
        } else if frame >= press + AUTO_RETURN_FRAMES {
            self.machine.key_up();
            self.auto_return = false;
        }
    }

    /// Writes the trace of REQ-086 with `writer` in the frames that it wants. The other
    /// frames run with the plain `Machine::run_frame`.
    #[must_use]
    pub fn with_trace(mut self, writer: TraceWriter<BufWriter<File>>) -> Self {
        self.trace = Some(writer);
        self
    }

    /// Takes the trace writer, to finish it at the end of the run.
    pub const fn take_trace(&mut self) -> Option<TraceWriter<BufWriter<File>>> {
        self.trace.take()
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

    /// The window runs the frames in turbo (no real-time pacing): while the BASIC program
    /// is typed (REQ-080 criterion 7), and when the last frame started with a tape inserted
    /// and the motor line low (`Machine::tape_motor_on`, REQ-082 criteria 1 and 2).
    #[must_use]
    pub const fn turbo(&self) -> bool {
        self.typist.is_some() || self.tape_turbo
    }

    /// The turbo cause of the last frame that ran, or `None` for real-time speed. A step
    /// that runs no frame (pause) does not change it.
    #[must_use]
    pub const fn turbo_cause(&self) -> Option<TurboCause> {
        self.turbo_cause
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

    /// The audio samples of the frame that the last [`Frontend::step`] ran, from
    /// `Machine::drain_audio`. Empty when the step ran no frame (pause), and while the core
    /// makes no samples (REQ-011).
    #[must_use]
    pub fn audio(&self) -> &[f32] {
        &self.audio
    }

    /// Runs one frame and counts it. Before the frame, the typist presses or releases a key.
    /// After the frame, the audio samples of the frame replace the samples of the last frame,
    /// so that they do not collect in headless mode. When the turbo cause changes, a record
    /// of the category `frontend` goes out before the frame (REQ-085 criterion 3).
    fn run_frame(&mut self) -> Result<(), FrontendError> {
        if self.auto_return {
            self.auto_return_key();
        }
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
        self.tape_turbo = self.machine.tape_motor_on();
        let cause = if self.typist.is_some() {
            Some(TurboCause::Typing)
        } else if self.tape_turbo {
            Some(TurboCause::TapeMotor)
        } else {
            None
        };
        if cause != self.turbo_cause {
            self.turbo_cause = cause;
            record(
                &self.machine,
                match cause {
                    Some(TurboCause::Typing) => "turbo on (BASIC typing)",
                    Some(TurboCause::TapeMotor) => "turbo on (tape motor)",
                    None => "turbo off",
                },
            );
        }
        let start = self.machine.cycle_count();
        match &mut self.trace {
            Some(trace) if trace.wants(self.machine.frame_count()) => {
                let _end: RunEnd = self.machine.run_frame_with(trace);
            }
            _ => self.machine.run_frame(),
        }
        self.last_frame_cycles = self.machine.cycle_count() - start;
        self.frames_run += 1;
        self.audio.clear();
        self.machine.drain_audio(&mut self.audio);
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
        self.audio.clear();
        self.tape_turbo = false;
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
            record(machine, "cold reset (F10)");
            machine.power_on();
        }
        if input.pause && !previous.pause {
            self.paused = !self.paused;
            record(
                &self.machine,
                if self.paused { "paused" } else { "continued" },
            );
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
    /// The last frame ended after the start time of the next frame.
    late: bool,
}

/// The largest delay that the pacing catches up.
///
/// When the frontend is more late than this (for example after a stop of the host), the
/// pacing starts again from the current time. A smaller delay is caught up: the frames run
/// without a wait until they are on time again.
pub const MAX_LATE: Duration = Duration::from_millis(100);

impl Pacer {
    /// Starts the pacing at `now` for the frame rate of `standard`.
    #[must_use]
    pub fn new(standard: VideoStandard, now: Instant) -> Self {
        let period = Duration::from_secs_f64(1.0 / standard.frames_per_second());
        Self {
            period,
            next: now + period,
            late: false,
        }
    }

    /// The time of one frame.
    #[must_use]
    pub const fn period(&self) -> Duration {
        self.period
    }

    /// Moves the time of the next frame by `seconds` (positive: later). The audio output
    /// gives this correction from the level of its ring (REQ-011,
    /// `SampleRing::correction`), so that the frames follow the clock of the audio device.
    pub fn correct(&mut self, seconds: f64) {
        if !seconds.is_finite() {
            return;
        }
        let shift = Duration::from_secs_f64(seconds.abs().min(1.0));
        if seconds > 0.0 {
            self.next += shift;
        } else {
            self.next = self.next.checked_sub(shift).unwrap_or(self.next);
        }
    }

    /// Called at `now`, after a frame: returns the time to wait until the next frame. When
    /// the frontend is more than [`MAX_LATE`] late, the pacing starts again from `now`.
    pub fn wait(&mut self, now: Instant) -> Duration {
        let wait = self.next.saturating_duration_since(now);
        if now > self.next + MAX_LATE {
            self.next = now + self.period;
            self.late = false;
        } else {
            self.late = now > self.next;
            self.next += self.period;
        }
        wait
    }

    /// The last [`Pacer::wait`] was called after the start time of the next frame, by at
    /// most [`MAX_LATE`]: the next frame must run at once to catch up.
    #[must_use]
    pub const fn late(&self) -> bool {
        self.late
    }
}

/// The host time after which the window shows the next frame in turbo (REQ-080), or while
/// the real-time pacing catches up (REQ-007).
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
/// Real time is [`Pacer`] (REQ-007). When a frame ends late (for example because the
/// window update waited for the display), the window does not show the next frame, so
/// that the frames catch up; it still shows a frame each [`TURBO_PRESENT_INTERVAL`]. Turbo
/// is used while a BASIC program is typed (REQ-080 criterion 7). In turbo, the frames run
/// without a wait, and the window shows a frame each [`TURBO_PRESENT_INTERVAL`] of host
/// time.
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

    /// Moves the time of the next real-time frame by `seconds` ([`Pacer::correct`]). In
    /// turbo, it has no effect.
    pub fn correct(&mut self, seconds: f64) {
        if !self.turbo {
            self.pacer.correct(seconds);
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
        let wait = self.pacer.wait(now);
        let present = !self.pacer.late()
            || now.saturating_duration_since(self.last_present) >= TURBO_PRESENT_INTERVAL;
        if present {
            self.last_present = now;
        }
        AfterFrame { present, wait }
    }
}

/// The audio output as the frame loop sees it ([`AudioSink`], or a fake device in the
/// tests).
pub trait FrameAudio {
    /// Sends the samples of one frame ([`AudioSink::push`]).
    fn send_frame(&mut self, samples: &[f32]);
    /// Stops the sound ([`AudioSink::stop`]): pause and turbo. This is not an underrun.
    fn stop_sound(&mut self);
    /// The correction of the next frame wait in seconds ([`AudioSink::correction`]).
    fn wait_correction(&self) -> f64;
    /// The number of underruns since the start ([`AudioSink::underrun_count`], REQ-083).
    fn underruns(&self) -> u64;
}

impl FrameAudio for AudioSink {
    fn send_frame(&mut self, samples: &[f32]) {
        Self::push(self, samples);
    }

    fn stop_sound(&mut self) {
        Self::stop(self);
    }

    fn wait_correction(&self) -> f64 {
        Self::correction(self)
    }

    fn underruns(&self) -> u64 {
        Self::underrun_count(self)
    }
}

/// The frame loop after [`Frontend::step`] at `now`: the audio, then the pacing.
///
/// `docs/design/architecture.md`, "Turbo while the tape motor runs": in turbo
/// ([`Frontend::turbo`]) or while paused, the sound stops (REQ-082 criterion 5); else the
/// samples of the frame go to `audio`, and its level corrects the wait (REQ-011). Then
/// [`Pacing::after_frame`] gives the wait and whether to show the frame.
pub fn after_step(
    frontend: &Frontend,
    pacing: &mut Pacing,
    audio: &mut impl FrameAudio,
    now: Instant,
) -> AfterFrame {
    let turbo = frontend.turbo();
    if turbo || frontend.paused() {
        audio.stop_sound();
    } else {
        audio.send_frame(frontend.audio());
        pacing.correct(audio.wait_correction());
    }
    pacing.after_frame(turbo, now)
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "the pacing tests measure the host time; the core reads no clock (REQ-014)"
)]
mod tests {
    use std::env::temp_dir;
    use std::fs::{create_dir_all, write};
    use std::path::{Path, PathBuf};
    use std::process::id;
    use std::time::{Duration, Instant};

    use atari_core::gtia::{FRAME_HEIGHT, FRAME_WIDTH};
    use atari_core::media::{CartKind, MediaError, Tape, Xex};
    use atari_core::{Machine, MachineConfig, VideoStandard, keyboard, palette};
    use minifb::Key;

    use super::{
        AfterFrame, DisplayFilters, Flow, FrameAudio, Frontend, Pacer, Pacing,
        TURBO_PRESENT_INTERVAL, after_step, cartridge_from_file, convert_row, load_machine, render,
        window_size, window_title, xex_warnings,
    };
    use crate::audio::{MAX_CORRECTION, SampleRing};
    use crate::bas::{CH, READY_TIMEOUT_FRAMES, Typist, program_codes};
    use crate::cli::Options;
    use crate::error::FrontendError;

    fn machine() -> Machine {
        Machine::new(MachineConfig {
            os_rom: vec![0xEA; 0x4000],
            basic_rom: None,
            cartridge: None,
            disk: None,
            fast_sio: false,
            fast_tape: false,
            tape: None,
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

    /// Verifies REQ-605: a `--cart` file that starts with `CART` loads as a CAR file, and
    /// other files as raw images (REQ-604). A bad CAR file gives its CAR error (REQ-606).
    #[test]
    fn cart_file_car_or_raw() {
        let data = vec![0x01_u8; 0x2000];
        let mut car = b"CART".to_vec();
        car.extend(1_u32.to_be_bytes());
        car.extend(0x2000_u32.to_be_bytes());
        car.extend([0; 4]);
        car.extend(&data);
        let kind = |bytes: Vec<u8>| cartridge_from_file(bytes).map(|cart| cart.kind());
        assert_eq!(kind(car.clone()), Ok(CartKind::Standard8k), "CAR type 1");
        assert_eq!(kind(data), Ok(CartKind::Standard8k), "raw 8 KB");
        car.truncate(100);
        assert_eq!(
            kind(car),
            Err(MediaError::CarSize {
                car_type: 1,
                expected: 0x2000,
                actual: 84
            }),
            "short CAR file"
        );
        assert_eq!(
            kind(vec![0; 100]),
            Err(MediaError::BadSize(100)),
            "raw 100 bytes"
        );
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
    fn xex_warnings_name_defect_and_loader_conflict() {
        let good = Xex::parse(&[0xFF, 0xFF, 0x00, 0x20, 0x00, 0x20, 0x60]).unwrap();
        assert!(xex_warnings(&good).is_empty(), "a complete file");
        let bad = Xex::parse(&[
            0xFF, 0xFF, 0x00, 0x08, 0x00, 0x08, 0x60, 0x00, 0x30, 0x10, 0x30,
        ])
        .unwrap();
        let warnings = xex_warnings(&bad);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains("at offset 7"), "{warnings:?}");
        assert!(warnings[1].contains("$0800"), "{warnings:?}");
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
        assert!(load_machine(&options, 48_000).is_ok(), "valid files");
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
            match load_machine(&bad, 48_000) {
                Err(FrontendError::Rom { path, .. }) => assert_eq!(&path, expect_path, "ROM"),
                other => panic!("{bad:?} gave {other:?}"),
            }
        }
        let bad_cart = Options {
            cart: Some(small.clone()),
            ..options.clone()
        };
        assert!(
            matches!(load_machine(&bad_cart, 48_000), Err(FrontendError::Media { path, .. }) if path == small),
            "cartridge"
        );
        let missing = Options {
            os: os.with_file_name("missing.rom"),
            ..options
        };
        assert!(
            matches!(
                load_machine(&missing, 48_000),
                Err(FrontendError::Read { .. })
            ),
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

    /// A fake audio device: it takes `chunk` samples from the ring each `chunk / rate`
    /// seconds of the fake clock, as the `cpal` callback does.
    struct FakeDevice {
        /// The sample rate of the device clock (it can differ from the nominal rate).
        rate: f64,
        /// The samples of one callback.
        chunk: u32,
        /// The time of the next callback, in seconds from the start.
        next: f64,
    }

    impl FakeDevice {
        /// Runs the callbacks up to `until` seconds from the start.
        fn run_until(&mut self, ring: &mut SampleRing, until: f64) {
            loop {
                if self.next > until {
                    break;
                }
                ring.fill_with(usize::try_from(self.chunk).unwrap(), |_| {});
                self.next += f64::from(self.chunk) / self.rate;
            }
        }
    }

    /// The result of [`simulate`].
    struct Simulated {
        /// The fake host time of all frames.
        elapsed: Duration,
        /// The ring after the frames.
        ring: SampleRing,
    }

    /// Runs `frames` frames of the window loop (architecture.md steps 2, 4 and 5) with a
    /// fake clock: each frame takes 3 ms of host time and makes `sample_rate / frame rate`
    /// samples (none when `sample_rate` is 0, as the core before POKEY audio). The ring
    /// gets the samples, its level corrects the wait, and `device` takes the samples.
    fn simulate(standard: VideoStandard, sample_rate: u32, device: FakeDevice) -> Simulated {
        simulate_frames(standard, sample_rate, device, 600, Duration::ZERO)
    }

    /// [`simulate`] with `frames` frames, and a window update that takes `present` of host
    /// time for each shown frame (as the window waits for the display). The wait after a
    /// shown frame is the wait minus the update time, as in `window::run`.
    fn simulate_frames(
        standard: VideoStandard,
        sample_rate: u32,
        mut device: FakeDevice,
        frames: u32,
        present: Duration,
    ) -> Simulated {
        let rate = if sample_rate == 0 {
            48_000
        } else {
            sample_rate
        };
        let mut ring = SampleRing::new(rate);
        let start = Instant::now();
        let mut pacing = Pacing::new(standard, start);
        let mut clock = start;
        let per_frame = f64::from(sample_rate) / standard.frames_per_second();
        let mut made = 0.0_f64;
        let mut pushed = 0_u64;
        let mut samples = Vec::new();
        for _ in 0..frames {
            clock += Duration::from_millis(3);
            device.run_until(&mut ring, (clock - start).as_secs_f64());
            made += per_frame;
            samples.clear();
            loop {
                if f64::from(u32::try_from(pushed).unwrap()) + 1.0 > made {
                    break;
                }
                samples.push(0.25);
                pushed += 1;
            }
            ring.push(&samples);
            pacing.correct(ring.correction(rate));
            let after = pacing.after_frame(false, clock);
            let mut wait = after.wait;
            if after.present {
                clock += present;
                device.run_until(&mut ring, (clock - start).as_secs_f64());
                wait = wait.saturating_sub(present);
            }
            clock += wait;
            device.run_until(&mut ring, (clock - start).as_secs_f64());
        }
        Simulated {
            elapsed: clock - start,
            ring,
        }
    }

    /// The device of a host: `rate` Hz, callbacks of 512 samples.
    const fn device(rate: f64) -> FakeDevice {
        FakeDevice {
            rate,
            chunk: 512,
            next: 0.0,
        }
    }

    /// Verifies REQ-007: 600 frames take 600 frame periods of host time (10.013 s NTSC,
    /// 12.034 s PAL) within 1 percent, with the audio level correction on (a device at
    /// the nominal rate, at 0.2 percent off, and with callbacks of 2048 samples), and
    /// exactly without audio samples (criterion 1, with a fake clock). A window update
    /// that takes 15 ms (frame and update longer than one frame period) does not slow
    /// the emulation: the late frames are not shown.
    #[test]
    fn paced_frames_keep_the_frame_rate() {
        for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
            let expected = 600.0 / standard.frames_per_second();
            for rate in [0, 48_000] {
                let slow = simulate_frames(
                    standard,
                    rate,
                    device(48_000.0),
                    600,
                    Duration::from_millis(15),
                );
                let seconds = slow.elapsed.as_secs_f64();
                assert!(
                    (seconds - expected).abs() <= expected * 0.01,
                    "{standard:?} {rate} Hz, slow window: {seconds} s, not {expected} s"
                );
                assert_eq!(slow.ring.underruns(), 0, "slow window: no underrun");
            }
            let expected = 600.0 / standard.frames_per_second();
            let cases = [
                (48_000, device(48_000.0)),
                (44_100, device(44_100.0)),
                (48_000, device(48_096.0)),
                (48_000, device(47_904.0)),
                (
                    48_000,
                    FakeDevice {
                        rate: 48_000.0,
                        chunk: 2048,
                        next: 0.0,
                    },
                ),
            ];
            for (rate, dev) in cases {
                let dev_rate = dev.rate;
                let run = simulate(standard, rate, dev);
                let seconds = run.elapsed.as_secs_f64();
                assert!(
                    (seconds - expected).abs() <= expected * 0.01,
                    "{standard:?} {rate} Hz on a {dev_rate} Hz device: {seconds} s, not {expected} s"
                );
                assert!(
                    run.ring.playing(),
                    "{standard:?} {rate} Hz: the sound plays"
                );
            }
            let silent = simulate(standard, 0, device(48_000.0));
            assert!(
                (silent.elapsed.as_secs_f64() - expected).abs() < 1e-3,
                "{standard:?} without samples: {:?}",
                silent.elapsed
            );
            assert!(!silent.ring.playing(), "no samples, no sound");
        }
    }

    /// Verifies REQ-011: with the samples of each frame in the ring and the level
    /// correction of the frame wait, the device callbacks have no underrun at real-time
    /// speed: for 10 s and for 10 minutes, at the nominal device rate and with a device
    /// clock 0.2 percent fast or slow (criterion 1, with a fake clock and a fake device).
    /// Without samples (the core before POKEY audio), nothing blocks and no underrun is
    /// counted.
    #[test]
    fn audio_ring_has_no_underruns() {
        for (frames, dev_rate, chunk) in [
            (600, 48_000.0, 512),
            (600, 48_096.0, 512),
            (600, 47_904.0, 512),
            (600, 48_000.0, 2048),
            (36_000, 48_096.0, 512),
            (36_000, 47_904.0, 512),
        ] {
            let dev = FakeDevice {
                rate: dev_rate,
                chunk,
                next: 0.0,
            };
            let run = simulate_frames(VideoStandard::Ntsc, 48_000, dev, frames, Duration::ZERO);
            assert_eq!(
                run.ring.underruns(),
                0,
                "{frames} frames on a {dev_rate} Hz device, callbacks of {chunk}"
            );
            assert_eq!(run.ring.dropped(), 0, "{frames} frames: no sample dropped");
            assert!(
                run.ring.queued() <= run.ring.target() * 2,
                "{frames} frames: the level stays near the target: {}",
                run.ring.queued()
            );
        }
        let silent = simulate(VideoStandard::Ntsc, 0, device(48_000.0));
        assert_eq!(silent.ring.underruns(), 0, "no samples: no underrun");
        assert_eq!(silent.ring.received(), 0, "no samples");
    }

    /// Verifies REQ-011: each step gives the samples of its own frame from
    /// `Machine::drain_audio` (at most one frame of samples, none while paused), so that
    /// they do not collect. The core before POKEY audio gives none.
    #[test]
    fn step_hands_over_the_frame_audio() {
        let mut frontend = Frontend::new(machine());
        let most = 48_000.0 / VideoStandard::Ntsc.frames_per_second() + 2.0;
        for _ in 0..5 {
            step(&mut frontend, &[]);
            let count = f64::from(u32::try_from(frontend.audio().len()).unwrap());
            assert!(count <= most, "one frame of samples, not {count}");
        }
        frontend.run_frames(10).unwrap();
        let count = f64::from(u32::try_from(frontend.audio().len()).unwrap());
        assert!(count <= most, "headless: one frame of samples, not {count}");
        step(&mut frontend, &[Key::F9]);
        assert!(frontend.paused(), "paused");
        step(&mut frontend, &[]);
        assert!(frontend.audio().is_empty(), "no samples while paused");
    }

    /// Verifies REQ-007: the pacer waits one frame period, and an audio correction moves
    /// the time of the next frame.
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
        // The audio level moves the next frame later or earlier.
        let now = late + period;
        pacer.correct(0.001);
        let wait = pacer.wait(now);
        assert!(
            wait.abs_diff(period + Duration::from_millis(1)) < Duration::from_micros(1),
            "later: {wait:?}"
        );
        let now = now + wait;
        pacer.correct(-0.001);
        let wait = pacer.wait(now);
        assert!(
            wait.abs_diff(period.checked_sub(Duration::from_millis(1)).unwrap())
                < Duration::from_micros(1),
            "earlier: {wait:?}"
        );
        // In turbo, the correction has no effect.
        let mut pacing = Pacing::new(VideoStandard::Ntsc, start);
        assert_eq!(
            pacing.after_frame(true, start).wait,
            Duration::ZERO,
            "turbo"
        );
        pacing.correct(0.001);
        assert_eq!(
            pacing.after_frame(true, start).wait,
            Duration::ZERO,
            "turbo: no wait"
        );
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

    /// The frame loop audio of the tests: a [`SampleRing`] that a [`FakeDevice`] plays.
    struct FakeAudio {
        /// The ring.
        ring: SampleRing,
        /// The sample rate of the ring.
        rate: u32,
    }

    impl FrameAudio for FakeAudio {
        fn send_frame(&mut self, samples: &[f32]) {
            self.ring.push(samples);
        }

        fn stop_sound(&mut self) {
            self.ring.stop();
        }

        fn wait_correction(&self) -> f64 {
            self.ring.correction(self.rate)
        }

        fn underruns(&self) -> u64 {
            self.ring.underruns()
        }
    }

    /// The RAM byte that the motor test program copies to `PACTL`.
    const MOTOR: u16 = 0x0080;
    /// `PACTL` with the motor line low (motor on).
    const MOTOR_ON: u8 = 0x34;
    /// `PACTL` with the motor line high (motor off).
    const MOTOR_OFF: u8 = 0x3C;

    /// A machine whose OS ROM copies the RAM byte [`MOTOR`] to `PACTL` in a loop, so that
    /// a test sets the motor line with `poke_ram`, and `tape` in the deck. The motor is off
    /// after the first frame.
    fn motor_machine(tape: Option<Tape>) -> Machine {
        let mut rom = vec![0xEA; 0x4000];
        let program = [
            0xA9, MOTOR_OFF, 0x85, 0x80, // LDA #$3C, STA $80
            0xA5, 0x80, 0x8D, 0x02, 0xD3, // loop: LDA $80, STA PACTL
            0x4C, 0x04, 0xC0, // JMP loop
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
            fast_tape: false,
            tape,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: 48_000,
        })
        .unwrap();
        machine.run_frame();
        machine
    }

    /// A tape with one `data` chunk (gap 100 ms, 600 baud) of 64 bytes.
    fn test_tape() -> Tape {
        let mut file = b"FUJI".to_vec();
        file.extend([0; 4]);
        file.extend(b"data");
        file.extend(64_u16.to_le_bytes());
        file.extend(100_u16.to_le_bytes());
        file.extend((0..64).map(|i: u8| i.wrapping_mul(37)));
        Tape::from_cas(&file).unwrap()
    }

    /// The motor byte of frame `frame` of the turbo tests: off, then on in frames 20-139,
    /// then off.
    const fn motor_at(frame: u32) -> u8 {
        if frame >= 20 && frame < 140 {
            MOTOR_ON
        } else {
            MOTOR_OFF
        }
    }

    /// The window loop with a fake clock and a fake audio device.
    struct Loop {
        /// The fake host time.
        clock: Instant,
        /// The start of the fake host time.
        start: Instant,
        /// The pacing.
        pacing: Pacing,
        /// The audio.
        audio: FakeAudio,
        /// The device that plays the audio.
        device: FakeDevice,
    }

    impl Loop {
        /// A loop at the current host time.
        fn new() -> Self {
            let start = Instant::now();
            Self {
                clock: start,
                start,
                pacing: Pacing::new(VideoStandard::Ntsc, start),
                audio: FakeAudio {
                    ring: SampleRing::new(48_000),
                    rate: 48_000,
                },
                device: device(48_000.0),
            }
        }

        /// Plays the device up to the fake time.
        fn play(&mut self) {
            let seconds = (self.clock - self.start).as_secs_f64();
            self.device.run_until(&mut self.audio.ring, seconds);
        }

        /// One frame of the window loop: sets the motor byte to `motor`, steps the frontend
        /// with `keys` (the frame takes 3 ms of host time), runs [`after_step`] and the
        /// wait, and plays the device up to the new time. Returns the tape motor state at
        /// the frame start and the result of [`after_step`].
        fn frame(
            &mut self,
            frontend: &mut Frontend,
            motor: u8,
            keys: &[Key],
        ) -> (bool, AfterFrame) {
            frontend.machine_mut().poke_ram(MOTOR, motor);
            let motor_on = frontend.machine().tape_motor_on();
            step(frontend, keys);
            self.clock += Duration::from_millis(3);
            self.play();
            let after = after_step(frontend, &mut self.pacing, &mut self.audio, self.clock);
            self.clock += after.wait;
            self.play();
            (motor_on, after)
        }
    }

    /// The largest change of a paced frame by the audio level correction
    /// ([`MAX_CORRECTION`]), and 1 µs for the rounding.
    fn correction_limit() -> Duration {
        Duration::from_secs_f64(MAX_CORRECTION) + Duration::from_micros(1)
    }

    /// Verifies REQ-082: with a tape inserted, a frame that starts while the motor line is
    /// low runs in turbo (no wait), and a frame that starts while it is high has the
    /// real-time pacing of REQ-007 (criterion 1, fake clock).
    #[test]
    fn tape_turbo_follows_motor_line() {
        let mut frontend = Frontend::new(motor_machine(Some(test_tape())));
        let mut run = Loop::new();
        let period = Pacer::new(VideoStandard::Ntsc, run.start).period();
        let (mut turbo, mut paced) = (0, 0);
        let mut last_turbo = false;
        for frame in 0..200 {
            let before = run.clock;
            let (motor_on, after) = run.frame(&mut frontend, motor_at(frame), &[]);
            assert_eq!(frontend.turbo(), motor_on, "frame {frame}: turbo");
            if motor_on {
                turbo += 1;
                assert_eq!(
                    after.wait,
                    Duration::ZERO,
                    "frame {frame}: no wait in turbo"
                );
            } else if frame > 0 && !last_turbo {
                // The first paced frame after turbo starts a new pacer from its end.
                paced += 1;
                assert!(
                    (run.clock - before).abs_diff(period) <= correction_limit(),
                    "frame {frame} takes one period: {:?}",
                    run.clock - before
                );
            }
            last_turbo = motor_on;
        }
        assert_eq!(turbo, 120, "the frames that start with the motor on");
        assert!(paced > 70, "paced frames: {paced}");
    }

    /// Verifies REQ-082: without a tape, the motor line low has no effect on the pacing
    /// (criterion 2, fake clock).
    #[test]
    fn motor_line_without_tape_keeps_pacing() {
        let mut frontend = Frontend::new(motor_machine(None));
        let mut run = Loop::new();
        let period = Pacer::new(VideoStandard::Ntsc, run.start).period();
        for frame in 0..60 {
            let before = run.clock;
            let (motor_on, after) = run.frame(&mut frontend, MOTOR_ON, &[]);
            assert!(!motor_on, "frame {frame}: no tape, so no tape motor");
            assert!(!frontend.turbo(), "frame {frame}: no turbo");
            assert!(after.present, "frame {frame} is shown");
            if frame > 0 {
                assert!(
                    (run.clock - before).abs_diff(period) <= correction_limit(),
                    "frame {frame} takes one period: {:?}",
                    run.clock - before
                );
            }
        }
        assert_eq!(
            frontend.machine().peek(0xD302) & 0x3F,
            MOTOR_ON,
            "the motor line is low"
        );
    }

    /// Verifies REQ-082: in tape turbo, the window loop shows a frame at least every 100 ms
    /// of host time (criterion 3, fake clock).
    #[test]
    fn tape_turbo_presents_frames() {
        let mut frontend = Frontend::new(motor_machine(Some(test_tape())));
        let mut run = Loop::new();
        let mut last_present = run.clock;
        let mut longest_gap = Duration::ZERO;
        let mut shown = 0;
        let mut turbo = 0;
        for _ in 0..400 {
            let (motor_on, after) = run.frame(&mut frontend, MOTOR_ON, &[]);
            if motor_on {
                turbo += 1;
            }
            if after.present {
                longest_gap = longest_gap.max(run.clock - last_present);
                last_present = run.clock;
                if motor_on {
                    shown += 1;
                }
            }
        }
        assert_eq!(turbo, 399, "the frames after the first run in turbo");
        assert!(shown > 10, "frames shown in turbo: {shown}");
        assert!(
            longest_gap <= Duration::from_millis(100),
            "a frame at least every 100 ms: {longest_gap:?}"
        );
    }

    /// Verifies REQ-082: after the same frames and the same key input, the frame buffer and
    /// the RAM are the same with tape turbo and without it (criterion 4).
    #[test]
    fn tape_turbo_keeps_machine_state() {
        let mut with_turbo = Frontend::new(motor_machine(Some(test_tape())));
        let mut paced = Frontend::new(motor_machine(Some(test_tape())));
        let mut turbo_run = Loop::new();
        let mut paced_run = Loop::new();
        let mut turbo_frames = 0;
        for frame in 0..160 {
            let keys: &[Key] = if (30..40).contains(&frame) {
                &[Key::A]
            } else {
                &[]
            };
            let motor = motor_at(frame);
            let (motor_on, _) = turbo_run.frame(&mut with_turbo, motor, keys);
            if motor_on {
                turbo_frames += 1;
            }
            // The same frame at real-time speed: this pacing never uses turbo.
            paced.machine_mut().poke_ram(MOTOR, motor);
            step(&mut paced, keys);
            paced_run.clock += Duration::from_millis(3);
            let after = paced_run.pacing.after_frame(false, paced_run.clock);
            paced_run.clock += after.wait;
        }
        assert_eq!(turbo_frames, 120, "frames in turbo");
        assert!(
            turbo_run.clock < paced_run.clock,
            "turbo is faster: {:?} and {:?}",
            turbo_run.clock - turbo_run.start,
            paced_run.clock - paced_run.start
        );
        assert_eq!(
            with_turbo.machine().frame(),
            paced.machine().frame(),
            "the frame buffer"
        );
        for addr in 0..=u16::MAX {
            assert_eq!(
                with_turbo.machine().peek_ram(addr),
                paced.machine().peek_ram(addr),
                "RAM at ${addr:04X}"
            );
        }
        assert_eq!(
            with_turbo.machine().tape_position(),
            paced.machine().tape_position(),
            "the tape position"
        );
        assert!(
            with_turbo.machine().tape_position() > Some(0),
            "the tape moved"
        );
    }

    /// Verifies REQ-082: in tape turbo, the frame loop plays no audio and counts no
    /// underrun, and the audio plays again after turbo (criterion 5, fake device).
    #[test]
    fn tape_turbo_stops_audio() {
        let mut frontend = Frontend::new(motor_machine(Some(test_tape())));
        let mut run = Loop::new();
        for frame in 0..400 {
            let motor = if (100..250).contains(&frame) {
                MOTOR_ON
            } else {
                MOTOR_OFF
            };
            let (motor_on, _) = run.frame(&mut frontend, motor, &[]);
            let ring = &run.audio.ring;
            if motor_on {
                assert!(
                    !frontend.audio().is_empty(),
                    "frame {frame}: the core makes samples"
                );
                assert_eq!(ring.queued(), 0, "frame {frame}: no samples in turbo");
                assert!(!ring.playing(), "frame {frame}: no audio in turbo");
            }
            if frame == 99 || frame == 399 {
                assert!(
                    ring.playing(),
                    "frame {frame}: the audio plays at real-time speed"
                );
            }
            assert_eq!(ring.underruns(), 0, "frame {frame}: no underrun");
        }
    }
}
