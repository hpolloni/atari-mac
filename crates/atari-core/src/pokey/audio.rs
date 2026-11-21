//! The host audio output: the mix, the resampler and the two high-pass stages (REQ-538 to
//! REQ-541).
//!
//! Design: `docs/design/pokey.md`, "Audio output". The HRM does not cover host audio. The
//! machine clock rates come from HRM 3.3, the output stages from HRM E.2 to E.5.

use crate::state::impl_state;

/// The DAC weights of the volume bits 0-3 (HRM E.2, REQ-541).
const DAC_WEIGHTS: [f64; 4] = [0.12, 0.26, 0.56, 1.12];
/// The normalizing divisor of the DAC sum: the sum of the weights of four channels.
const DAC_DIVISOR: f64 = 8.24;
/// The mix level that gives the output 1.0 (REQ-539).
const FULL_MIX: f64 = 60.0;
/// The output level of the console speaker when its line is high (REQ-539).
const SPEAKER_LEVEL: f64 = 0.25;
/// The time constant of the first high-pass stage in seconds (HRM E.3, REQ-540).
const TAU_STAGE1: f64 = 0.0026;
/// The time constant of the second high-pass stage in seconds (HRM E.5, REQ-540).
const TAU_STAGE2: f64 = 0.0247;

/// The normalized nonlinear DAC sum of the four channels (REQ-541). `volumes[n]` is the
/// `AUDCx` volume of channel n if its digital output is 1, else 0.
pub(super) fn dac_sum(volumes: [u8; 4]) -> f64 {
    let mut sum = 0.0;
    for volume in volumes {
        for (bit, weight) in DAC_WEIGHTS.iter().enumerate() {
            if volume & (1 << bit) != 0 {
                sum += weight;
            }
        }
    }
    sum / DAC_DIVISOR
}

/// The saturation curve of HRM E.2 for the normalized DAC sum `x` (REQ-541).
pub(super) fn curve(x: f64) -> f64 {
    /// The gain of the linear part.
    const GAIN: f64 = 2.171;
    /// The end of the linear part.
    const KNEE: f64 = 0.14;
    /// The compression rate above the knee.
    const RATE: f64 = 2.85;
    if x <= KNEE {
        GAIN * x
    } else {
        GAIN * (KNEE + (1.0 - (-RATE * (x - KNEE)).exp()) / RATE)
    }
}

/// The audio output state.
#[derive(Debug, Clone)]
pub(super) struct Audio {
    /// The machine clock rate in Hz.
    clock_hz: u64,
    /// The host sample rate in Hz.
    sample_rate: u64,
    /// The REQ-541 option.
    nonlinear: bool,
    /// The resampler phase, in units of 1 / (`clock_hz` x `sample_rate`) s.
    phase: u64,
    /// The sum of the POKEY levels of the current sample.
    acc_pokey: f64,
    /// The sum of the speaker levels of the current sample.
    acc_spk: f64,
    /// The capacitor state of the first high-pass stage.
    hp1: f64,
    /// The capacitor state of the second high-pass stage.
    hp2: f64,
    /// The filter coefficient of the first stage.
    a1: f64,
    /// The filter coefficient of the second stage.
    a2: f64,
    /// The samples that are not drained yet.
    samples: Vec<f32>,
    /// A cache: the `volumes` of the last cycle as little-endian bytes, or `NO_LEVEL`.
    level_key: u32,
    /// A cache: the mix level `x` of `level_key`.
    level: f64,
}

impl_state!(Audio {
    clock_hz,
    sample_rate,
    nonlinear,
    phase,
    acc_pokey,
    acc_spk,
    hp1,
    hp2,
    a1,
    a2,
    samples,
    level_key,
    level,
});

/// The `level_key` that no `volumes` can give (each volume is 0-15).
const NO_LEVEL: u32 = u32::MAX;

impl Audio {
    /// The power-on state for the machine clock `clock_hz` and the host rate `sample_rate`.
    pub(super) fn new(clock_hz: u32, sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        Self {
            clock_hz: u64::from(clock_hz),
            sample_rate: u64::from(sample_rate),
            nonlinear: false,
            phase: 0,
            acc_pokey: 0.0,
            acc_spk: 0.0,
            hp1: 0.0,
            hp2: 0.0,
            a1: 1.0 - (-1.0 / (TAU_STAGE1 * rate)).exp(),
            a2: 1.0 - (-1.0 / (TAU_STAGE2 * rate)).exp(),
            samples: Vec::new(),
            level_key: NO_LEVEL,
            level: 0.0,
        }
    }

    /// Selects the nonlinear DAC model (REQ-541).
    pub(super) const fn set_nonlinear(&mut self, on: bool) {
        self.nonlinear = on;
        self.level_key = NO_LEVEL;
    }

    /// The mix level of `volumes` (REQ-539, REQ-541).
    fn mix(&self, volumes: [u8; 4]) -> f64 {
        if self.nonlinear {
            curve(dac_sum(volumes))
        } else {
            let level: u8 = volumes.iter().sum();
            f64::from(level) / FULL_MIX
        }
    }

    /// Adds one machine cycle. `volumes[n]` is the `AUDCx` volume of channel n if its
    /// digital output is 1, else 0. `speaker` is the console speaker line level.
    pub(super) fn cycle(&mut self, volumes: [u8; 4], speaker: bool) {
        // The volumes change seldom, so the level of the last cycle is kept. The value is the
        // same as a new computation.
        let key = u32::from_le_bytes(volumes);
        if key != self.level_key {
            self.level_key = key;
            self.level = self.mix(volumes);
        }
        let x = self.level;
        let s = if speaker { SPEAKER_LEVEL } else { 0.0 };
        self.phase += self.sample_rate;
        if self.phase < self.clock_hz {
            self.acc_pokey += x;
            self.acc_spk += s;
            return;
        }
        // The sample ends in this cycle: the fraction `next` of the cycle goes into the
        // next sample.
        self.phase -= self.clock_hz;
        let next = to_f64(self.phase) / to_f64(self.sample_rate);
        let this = 1.0 - next;
        let scale = to_f64(self.sample_rate) / to_f64(self.clock_hz);
        let xp = x.mul_add(this, self.acc_pokey) * scale;
        let xs = s.mul_add(this, self.acc_spk) * scale;
        self.acc_pokey = x * next;
        self.acc_spk = s * next;
        // Stage 1 (HRM E.3), then the speaker (HRM E.4), then stage 2 (HRM E.5).
        let h1 = xp - self.hp1;
        self.hp1 = (xp - self.hp1).mul_add(self.a1, self.hp1);
        let u = h1 + xs;
        let out = u - self.hp2;
        self.hp2 = (u - self.hp2).mul_add(self.a2, self.hp2);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the host format is f32; the value is clamped to -1.0..=1.0"
        )]
        self.samples.push(out.clamp(-1.0, 1.0) as f32);
    }

    /// Moves all samples to the end of `out`.
    pub(super) fn drain(&mut self, out: &mut Vec<f32>) {
        out.append(&mut self.samples);
    }
}

/// A cycle count or rate as `f64`. The values are below 2^53, so the conversion is exact.
#[expect(
    clippy::cast_precision_loss,
    reason = "the phase and the rates are below 2^32, far below 2^53"
)]
const fn to_f64(value: u64) -> f64 {
    value as f64
}

impl PartialEq for Audio {
    /// The floating-point state compares bit for bit, so that two instances with the same
    /// history are equal (REQ-501). The level cache is not state, so it does not compare.
    fn eq(&self, other: &Self) -> bool {
        self.clock_hz == other.clock_hz
            && self.sample_rate == other.sample_rate
            && self.nonlinear == other.nonlinear
            && self.phase == other.phase
            && self.acc_pokey.to_bits() == other.acc_pokey.to_bits()
            && self.acc_spk.to_bits() == other.acc_spk.to_bits()
            && self.hp1.to_bits() == other.hp1.to_bits()
            && self.hp2.to_bits() == other.hp2.to_bits()
            && self.a1.to_bits() == other.a1.to_bits()
            && self.a2.to_bits() == other.a2.to_bits()
            && self.samples.len() == other.samples.len()
            && self
                .samples
                .iter()
                .zip(&other.samples)
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

impl Eq for Audio {}
