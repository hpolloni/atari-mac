//! The audio output (REQ-011): the ring buffer between the frame loop and the audio
//! callback, the frame wait correction from the buffer level, and the `cpal` stream.
//!
//! Design: `docs/design/architecture.md`, "Audio output". [`SampleRing`] and
//! [`SampleRing::correction`] are plain logic, so that unit tests need no audio device.
//! [`AudioOutput`] only connects the ring to the default host output device.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};

/// The sample rate of the machine when there is no audio device (headless, or a host
/// without an output device).
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// The buffer level at which the ring starts to play, in milliseconds of audio (3 NTSC
/// frames). The real target is at least two callback requests ([`SampleRing::target`]).
pub const TARGET_MS: u32 = 50;

/// The size of the ring in milliseconds of audio. When the frame loop gives more samples,
/// the oldest samples are dropped.
pub const CAPACITY_MS: u32 = 250;

/// The part of the level error (in seconds of audio) that one frame corrects.
pub const CORRECTION_GAIN: f64 = 0.05;

/// The largest correction of one frame wait, in seconds (6 percent of an NTSC frame).
pub const MAX_CORRECTION: f64 = 0.001;

/// The number of samples in `ms` milliseconds at `sample_rate`.
fn samples_in_ms(sample_rate: u32, ms: u32) -> usize {
    usize::try_from(u64::from(sample_rate) * u64::from(ms) / 1000).unwrap_or(usize::MAX)
}

/// A sample count as `f64`.
fn count_f64(count: usize) -> f64 {
    u32::try_from(count).map_or_else(|_| f64::from(u32::MAX), f64::from)
}

/// The ring buffer of mono samples between the frame loop and the audio callback.
///
/// The frame loop calls [`SampleRing::push`] after each frame, and the audio callback
/// calls [`SampleRing::fill_with`]. The ring is silent until it holds
/// [`SampleRing::target`] samples, then it plays. When the callback asks for more samples
/// than the ring holds, that is one underrun: the ring holds the last sample value for the
/// rest of the request and waits for the target level again.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleRing {
    /// The samples that are not played yet, oldest first.
    samples: VecDeque<f32>,
    /// The largest number of samples that the ring holds.
    capacity: usize,
    /// The start level from [`TARGET_MS`].
    base_target: usize,
    /// The largest callback request so far.
    largest_request: usize,
    /// The ring plays (it reached the target level and had no underrun since).
    playing: bool,
    /// The last sample that was played, for the silence.
    last: f32,
    /// The samples given to [`SampleRing::push`].
    received: u64,
    /// The number of underruns.
    underruns: u64,
    /// The samples dropped because the ring was full.
    dropped: u64,
}

impl SampleRing {
    /// An empty ring for `sample_rate`.
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let capacity = samples_in_ms(sample_rate, CAPACITY_MS).max(1);
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
            base_target: samples_in_ms(sample_rate, TARGET_MS),
            largest_request: 0,
            playing: false,
            last: 0.0,
            received: 0,
            underruns: 0,
            dropped: 0,
        }
    }

    /// The level at which the ring starts to play, and which the frame wait correction
    /// keeps: [`TARGET_MS`] of audio, or two callback requests when they are larger. At
    /// most half the capacity.
    #[must_use]
    pub fn target(&self) -> usize {
        self.base_target
            .max(self.largest_request.saturating_mul(2))
            .min(self.capacity / 2)
    }

    /// The number of samples that are not played yet.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.samples.len()
    }

    /// The ring plays: it reached the target level and had no underrun since.
    #[must_use]
    pub const fn playing(&self) -> bool {
        self.playing
    }

    /// The number of underruns.
    #[must_use]
    pub const fn underruns(&self) -> u64 {
        self.underruns
    }

    /// The number of samples given to [`SampleRing::push`].
    #[must_use]
    pub const fn received(&self) -> u64 {
        self.received
    }

    /// The number of samples dropped because the ring was full.
    #[must_use]
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Adds the samples of one frame. An empty slice has no effect. When the ring is full,
    /// the oldest samples are dropped.
    pub fn push(&mut self, samples: &[f32]) {
        self.received += u64::try_from(samples.len()).unwrap_or(u64::MAX);
        self.samples.extend(samples);
        let excess = self.samples.len().saturating_sub(self.capacity);
        if excess > 0 {
            drop(self.samples.drain(..excess));
            self.dropped += u64::try_from(excess).unwrap_or(u64::MAX);
        }
        if !self.playing && !self.samples.is_empty() && self.samples.len() >= self.target() {
            self.playing = true;
        }
    }

    /// Stops the sound (pause, turbo): removes the samples that are not played yet. The
    /// ring plays again when it reaches the target level. This is not an underrun.
    pub fn stop(&mut self) {
        self.samples.clear();
        self.playing = false;
    }

    /// The audio callback: gives `count` samples to `put`, oldest first. While the ring
    /// does not play, each sample is the last played value.
    pub fn fill_with(&mut self, count: usize, mut put: impl FnMut(f32)) {
        self.largest_request = self.largest_request.max(count);
        let mut given = 0;
        if self.playing {
            while given < count {
                if let Some(sample) = self.samples.pop_front() {
                    self.last = sample;
                    put(sample);
                    given += 1;
                } else {
                    self.underruns += 1;
                    self.playing = false;
                    break;
                }
            }
        }
        for _ in given..count {
            put(self.last);
        }
    }

    /// The correction of the next frame wait in seconds (REQ-011, architecture.md step 5):
    /// positive when the ring holds more than [`SampleRing::target`] (the emulation is
    /// ahead of the audio device, so the frame loop waits longer), negative when it holds
    /// less. It is [`CORRECTION_GAIN`] of the level error, at most [`MAX_CORRECTION`]. It
    /// is 0 while the ring does not play, so that a machine without samples runs on the
    /// host clock only.
    #[must_use]
    pub fn correction(&self, sample_rate: u32) -> f64 {
        if !self.playing || sample_rate == 0 {
            return 0.0;
        }
        let error = (count_f64(self.queued()) - count_f64(self.target())) / f64::from(sample_rate);
        (CORRECTION_GAIN * error).clamp(-MAX_CORRECTION, MAX_CORRECTION)
    }
}

/// Locks the ring. A panic in the other thread cannot leave the ring in a state that is
/// not valid, so a poisoned lock is used as it is.
fn lock(ring: &Mutex<SampleRing>) -> MutexGuard<'_, SampleRing> {
    ring.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The numbers that `--audio-stats` prints (REQ-011 criterion 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioStats {
    /// The output: the device stream, or the reason why there is no sound.
    pub output: Result<DeviceInfo, String>,
    /// The samples that the frame loop gave to the ring.
    pub received: u64,
    /// The underruns of the ring.
    pub underruns: u64,
    /// The samples dropped because the ring was full.
    pub dropped: u64,
}

/// The stream of the output device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceInfo {
    /// The sample rate of the stream and the machine.
    pub sample_rate: u32,
    /// The number of channels of the stream. Each channel gets the same mono sample.
    pub channels: u16,
}

impl fmt::Display for AudioStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.output {
            Ok(device) => write!(
                f,
                "audio: {} Hz, {} channels, {} samples, {} underruns, {} dropped",
                device.sample_rate, device.channels, self.received, self.underruns, self.dropped
            ),
            Err(reason) => write!(f, "audio: off ({reason})"),
        }
    }
}

/// The audio output of the window: the ring and the `cpal` stream.
///
/// The stream plays on the default output device. Headless, or without a device, there is
/// no stream: the samples are dropped and the frame wait has no correction.
pub struct AudioOutput {
    /// The ring that the stream reads.
    ring: Arc<Mutex<SampleRing>>,
    /// The stream (it plays while it exists), or the reason why there is none.
    stream: Result<(Stream, DeviceInfo), String>,
}

impl fmt::Debug for AudioOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioOutput")
            .field("ring", &self.ring)
            .field("device", &self.stream.as_ref().map(|(_, info)| info))
            .finish()
    }
}

impl AudioOutput {
    /// No audio output, for the reason `reason` (for example `--headless`).
    #[must_use]
    pub fn off(reason: &str) -> Self {
        Self {
            ring: Arc::new(Mutex::new(SampleRing::new(DEFAULT_SAMPLE_RATE))),
            stream: Err(reason.to_owned()),
        }
    }

    /// Opens a stream on the default output device of the host, with the default sample
    /// rate and channel count of the device. When there is no device or the stream cannot
    /// start, the result has no stream (see [`AudioOutput::error`]); the emulator runs
    /// without sound.
    #[must_use]
    pub fn open_default() -> Self {
        let Some(device) = cpal::default_host().default_output_device() else {
            return Self::off("no audio output device");
        };
        let config = match device.default_output_config() {
            Ok(config) => config,
            Err(error) => return Self::off(&format!("audio device: {error}")),
        };
        let format = config.sample_format();
        let config: StreamConfig = config.into();
        let ring = Arc::new(Mutex::new(SampleRing::new(config.sample_rate)));
        let info = DeviceInfo {
            sample_rate: config.sample_rate,
            channels: config.channels,
        };
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, config, &ring),
            SampleFormat::F64 => build::<f64>(&device, config, &ring),
            SampleFormat::I16 => build::<i16>(&device, config, &ring),
            SampleFormat::I32 => build::<i32>(&device, config, &ring),
            SampleFormat::U16 => build::<u16>(&device, config, &ring),
            other => Err(format!("audio sample format {other} is not supported")),
        };
        Self {
            ring,
            stream: stream.map(|stream| (stream, info)),
        }
    }

    /// The reason why there is no stream, or `None` when the sound plays.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.stream.as_ref().err().map(String::as_str)
    }

    /// The sample rate for the machine: the rate of the stream, or
    /// [`DEFAULT_SAMPLE_RATE`] without a stream.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.stream
            .as_ref()
            .map_or(DEFAULT_SAMPLE_RATE, |(_, info)| info.sample_rate)
    }

    /// The handle for the emulation thread: it sends the samples to the ring and reads the
    /// wait correction. Without a stream, it drops the samples.
    #[must_use]
    pub fn sink(&self) -> AudioSink {
        AudioSink {
            ring: Arc::clone(&self.ring),
            active: self.stream.is_ok(),
            sample_rate: self.sample_rate(),
        }
    }

    /// The numbers for `--audio-stats`.
    #[must_use]
    pub fn stats(&self) -> AudioStats {
        let ring = lock(&self.ring);
        AudioStats {
            output: self
                .stream
                .as_ref()
                .map(|(_, info)| *info)
                .map_err(Clone::clone),
            received: ring.received(),
            underruns: ring.underruns(),
            dropped: ring.dropped(),
        }
    }
}

/// The frame loop side of [`AudioOutput`] ([`AudioOutput::sink`]). It can move to the
/// emulation thread; the stream stays on the thread that opened it.
#[derive(Debug, Clone)]
pub struct AudioSink {
    /// The ring that the stream reads.
    ring: Arc<Mutex<SampleRing>>,
    /// A stream plays the ring.
    active: bool,
    /// The sample rate of the stream.
    sample_rate: u32,
}

impl AudioSink {
    /// Sends the samples of one frame to the ring (architecture.md step 4). Without a
    /// stream, nothing happens.
    pub fn push(&self, samples: &[f32]) {
        if self.active {
            lock(&self.ring).push(samples);
        }
    }

    /// Stops the sound while no frames run at real-time speed (pause, turbo).
    pub fn stop(&self) {
        lock(&self.ring).stop();
    }

    /// The correction of the next frame wait in seconds ([`SampleRing::correction`]), or
    /// 0 without a stream.
    #[must_use]
    pub fn correction(&self) -> f64 {
        if !self.active {
            return 0.0;
        }
        lock(&self.ring).correction(self.sample_rate)
    }

    /// The number of underruns of the ring since the start (REQ-083), or 0 without a
    /// stream. The frame loop reads it with `FrameAudio::underruns`.
    #[must_use]
    pub fn underrun_count(&self) -> u64 {
        if !self.active {
            return 0;
        }
        lock(&self.ring).underruns()
    }
}

/// Builds and starts the output stream with samples of type `T`. Each channel of a frame
/// gets the same mono sample.
fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    ring: &Arc<Mutex<SampleRing>>,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels).max(1);
    let callback_ring = Arc::clone(ring);
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let count = data.len() / channels;
                let mut frames = data.chunks_mut(channels);
                lock(&callback_ring).fill_with(count, |sample| {
                    if let Some(frame) = frames.next() {
                        frame.fill(T::from_sample(sample));
                    }
                });
                for frame in frames {
                    frame.fill(T::from_sample(0.0_f32));
                }
            },
            // A stream error (for example a device change) shows as underruns.
            |_: cpal::Error| {},
            None,
        )
        .map_err(|error| format!("audio stream: {error}"))?;
    stream
        .play()
        .map_err(|error| format!("audio stream: {error}"))?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::{AudioOutput, AudioStats, DeviceInfo, MAX_CORRECTION, SampleRing};

    /// Takes `count` samples from the ring, as the audio callback does.
    fn take(ring: &mut SampleRing, count: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(count);
        ring.fill_with(count, |sample| out.push(sample));
        out
    }

    /// The samples `start..start + count` as values.
    fn ramp(start: u16, count: u16) -> Vec<f32> {
        (start..start + count).map(f32::from).collect()
    }

    /// Verifies REQ-011: the ring is silent until it holds the target level, then it plays
    /// the samples in order, with no gap and no underrun.
    #[test]
    fn ring_plays_samples_in_order() {
        let mut ring = SampleRing::new(48_000);
        assert_eq!(ring.target(), 2400, "50 ms at 48 kHz");
        ring.push(&ramp(1, 2000));
        assert!(!ring.playing(), "below the target");
        assert_eq!(
            take(&mut ring, 512),
            vec![0.0; 512],
            "silent before the start"
        );
        assert_eq!(ring.queued(), 2000, "nothing taken while silent");
        ring.push(&ramp(2001, 800));
        assert!(ring.playing(), "at the target");
        let mut played = Vec::new();
        for _ in 0..5 {
            played.extend(take(&mut ring, 512));
        }
        assert_eq!(played, ramp(1, 2560), "the samples in order");
        assert_eq!(ring.underruns(), 0, "no underrun");
        assert_eq!(ring.received(), 2800, "received");
    }

    /// Verifies REQ-011: a ring that never gets samples (the core before POKEY audio) is
    /// silent, counts no underrun and asks for no correction.
    #[test]
    fn ring_without_samples_is_silent() {
        let mut ring = SampleRing::new(44_100);
        for _ in 0..100 {
            ring.push(&[]);
            assert_eq!(take(&mut ring, 512), vec![0.0; 512], "silence");
        }
        assert!(!ring.playing(), "never plays");
        assert_eq!(ring.underruns(), 0, "no underrun without samples");
        assert!(
            ring.correction(44_100).abs() < f64::EPSILON,
            "no correction"
        );
        let off = AudioOutput::off("test");
        let sink = off.sink();
        sink.push(&[0.5; 800]);
        assert!(
            sink.correction().abs() < f64::EPSILON,
            "no stream, no correction"
        );
        assert_eq!(off.sample_rate(), 48_000, "default rate");
        assert_eq!(off.stats().received, 0, "no stream drops the samples");
    }

    /// Verifies REQ-011: when the callback asks for more samples than the ring holds, that
    /// is one underrun; the ring holds the last value and plays again at the target level.
    /// Stopping (pause, turbo) is not an underrun.
    #[test]
    fn ring_counts_underruns() {
        let mut ring = SampleRing::new(48_000);
        ring.push(&ramp(1, 2400));
        let out = take(&mut ring, 3000);
        assert_eq!(out[..2400], ramp(1, 2400)[..], "all samples played");
        assert_eq!(out[2400..], vec![2400.0; 600][..], "then the last value");
        assert_eq!(ring.underruns(), 1, "one underrun");
        assert!(!ring.playing(), "waits for the target again");
        ring.push(&ramp(1, 1000));
        assert_eq!(
            take(&mut ring, 100),
            vec![2400.0; 100],
            "silent below the target"
        );
        assert_eq!(ring.underruns(), 1, "no underrun while it waits");
        ring.push(&ramp(1001, 2000));
        ring.stop();
        assert_eq!(ring.queued(), 0, "stop removes the samples");
        drop(take(&mut ring, 512));
        assert_eq!(ring.underruns(), 1, "stop is not an underrun");
    }

    /// Verifies REQ-011: a full ring drops the oldest samples, and the target grows with
    /// a large callback request.
    #[test]
    fn ring_drops_oldest_when_full() {
        let mut ring = SampleRing::new(8_000);
        assert_eq!(ring.target(), 400, "50 ms at 8 kHz");
        ring.push(&ramp(1, 2100));
        assert_eq!(ring.queued(), 2000, "250 ms at 8 kHz");
        assert_eq!(ring.dropped(), 100, "the oldest 100 are dropped");
        assert_eq!(
            take(&mut ring, 3),
            ramp(101, 3),
            "the oldest samples are gone"
        );
        drop(take(&mut ring, 300));
        assert_eq!(ring.target(), 600, "two requests of 300");
        drop(take(&mut ring, 900));
        assert_eq!(ring.target(), 1000, "at most half the capacity");
    }

    /// Verifies REQ-011: the wait correction has the sign of the level error, and is at
    /// most `MAX_CORRECTION`.
    #[test]
    fn correction_follows_the_level() {
        let mut ring = SampleRing::new(48_000);
        ring.push(&[0.0; 2400]);
        assert!(ring.correction(48_000).abs() < 1e-12, "at the target");
        ring.push(&[0.0; 480]);
        let ahead = ring.correction(48_000);
        assert!((ahead - 0.0005).abs() < 1e-9, "10 ms ahead: {ahead}");
        drop(take(&mut ring, 960));
        let behind = ring.correction(48_000);
        assert!((behind + 0.0005).abs() < 1e-9, "10 ms behind: {behind}");
        ring.push(&vec![0.0; 9000]);
        assert!(
            (ring.correction(48_000) - MAX_CORRECTION).abs() < 1e-12,
            "clamped"
        );
    }

    #[test]
    fn stats_text() {
        let mut stats = AudioStats {
            output: Ok(DeviceInfo {
                sample_rate: 48_000,
                channels: 2,
            }),
            received: 480_000,
            underruns: 0,
            dropped: 0,
        };
        assert_eq!(
            stats.to_string(),
            "audio: 48000 Hz, 2 channels, 480000 samples, 0 underruns, 0 dropped",
            "with a device"
        );
        stats.output = Err("no audio output device".to_owned());
        assert_eq!(
            stats.to_string(),
            "audio: off (no audio output device)",
            "without a device"
        );
    }
}
