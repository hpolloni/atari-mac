//! POKEY audio in the samples of `Machine::drain_audio` (REQ-503, REQ-511, REQ-539).
//!
//! A test runs a small 6502 program from a synthetic OS ROM, or BASIC, and analyzes the
//! samples with the measurement methods of REQ-503 Notes (measured frequency) and REQ-539
//! (peak-to-peak) in the module [`measure`] (`docs/design/pokey.md`, "Test plan").

/// The tests and their helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod system {
    use atari_core::{Machine, VideoStandard};
    use atari_harness::input::type_text;
    use atari_harness::screen::has_row;
    use atari_harness::system::{
        BOOT_FRAMES, BootOptions, TEST_SAMPLE_RATE, boot, boot_program, run_frames, run_until,
    };

    /// The measurement methods of REQ-503 Notes and REQ-539.
    #[expect(
        clippy::suboptimal_flops,
        reason = "the formulas stay in their textbook form; the rounding is far below the \
                  tolerances of the criteria"
    )]
    mod measure {
        use core::f64::consts::PI;

        /// A complex number (real, imaginary).
        type Complex = (f64, f64);

        /// The discrete Fourier transform of `input`, with a mixed-radix (2, 3, 5)
        /// Cooley-Tukey recursion. Other prime factors use the direct sum.
        fn dft(input: &[Complex]) -> Vec<Complex> {
            let n = input.len();
            if n <= 1 {
                return input.to_vec();
            }
            let radix = [2, 3, 5]
                .into_iter()
                .find(|f| n.is_multiple_of(*f))
                .unwrap_or(n);
            let m = n / radix;
            let parts: Vec<Vec<Complex>> = if radix == n {
                input.iter().map(|x| vec![*x]).collect()
            } else {
                (0..radix)
                    .map(|r| {
                        let part: Vec<Complex> =
                            input.iter().skip(r).step_by(radix).copied().collect();
                        dft(&part)
                    })
                    .collect()
            };
            let n_f = index_f64(n);
            (0..n)
                .map(|k| {
                    parts.iter().enumerate().fold((0.0, 0.0), |acc, (r, part)| {
                        let (re, im) = part[k % m];
                        let angle = -2.0 * PI * index_f64((r * k) % n) / n_f;
                        let (sin, cos) = angle.sin_cos();
                        (acc.0 + re * cos - im * sin, acc.1 + re * sin + im * cos)
                    })
                })
                .collect()
        }

        /// An index or a count as `f64` (exact below 2^32).
        fn index_f64(value: usize) -> f64 {
            f64::from(u32::try_from(value).unwrap())
        }

        /// The measured frequency of REQ-503 Notes: a Hann window, the magnitude spectrum,
        /// the largest peak from 0.5 to 1.5 times `expected` and a parabolic interpolation
        /// over the peak bin and its two neighbors. With 2 s of samples at `rate` Hz, the bin
        /// width is 0.5 Hz.
        pub(super) fn frequency(samples: &[f32], rate: u32, expected: f64) -> f64 {
            let n = samples.len();
            let last = index_f64(n - 1);
            let windowed: Vec<Complex> = samples
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let w = 0.5 - 0.5 * (2.0 * PI * index_f64(i) / last).cos();
                    (f64::from(*s) * w, 0.0)
                })
                .collect();
            let spectrum: Vec<f64> = dft(&windowed)
                .iter()
                .map(|(re, im)| re.hypot(*im))
                .collect();
            let bin = f64::from(rate) / index_f64(n);
            // The band in whole bins: the first bin at or above 0.5 x expected, the last
            // bin at or below 1.5 x expected.
            let low = (0..n)
                .find(|k| index_f64(*k) * bin >= 0.5 * expected)
                .unwrap();
            let high = (0..n)
                .rfind(|k| index_f64(*k) * bin <= 1.5 * expected)
                .unwrap();
            let peak = (low..=high)
                .max_by(|a, b| spectrum[*a].total_cmp(&spectrum[*b]))
                .unwrap();
            let (a, b, c) = (spectrum[peak - 1], spectrum[peak], spectrum[peak + 1]);
            let offset = 0.5 * (a - c) / (a - 2.0 * b + c);
            (index_f64(peak) + offset) * bin
        }

        /// The maximum minus the minimum of `samples` (REQ-539).
        pub(super) fn peak_to_peak(samples: &[f32]) -> f32 {
            let max = samples.iter().copied().fold(f32::MIN, f32::max);
            let min = samples.iter().copied().fold(f32::MAX, f32::min);
            max - min
        }
    }

    /// Samples per second.
    const RATE: usize = TEST_SAMPLE_RATE as usize;

    /// `LDA #value`, `STA addr`.
    fn store(addr: u16, value: u8) -> Vec<u8> {
        let [lo, hi] = addr.to_le_bytes();
        vec![0xA9, value, 0x8D, lo, hi]
    }

    /// A machine that runs `code` and then a `JMP` to itself, after power-on.
    fn machine(code: &[u8], standard: VideoStandard) -> Machine {
        let mut program = code.to_vec();
        let [lo, hi] = (0xC000 + u16::try_from(program.len()).unwrap()).to_le_bytes();
        program.extend([0x4C, lo, hi]);
        boot_program(
            &program,
            BootOptions {
                basic: false,
                standard,
            },
        )
        .unwrap()
    }

    /// The code that writes `SKCTL` = `$03` and then each POKEY register of `writes`
    /// (offset, value).
    fn pokey_code(writes: &[(u8, u8)]) -> Vec<u8> {
        let mut code = store(0xD20F, 0x03);
        for (reg, value) in writes {
            code.extend(store(0xD200 + u16::from(*reg), *value));
        }
        code
    }

    /// Runs `m` until it made at least `count` samples since power-on, and returns them.
    fn samples(m: &mut Machine, count: usize) -> Vec<f32> {
        let mut out = Vec::new();
        while out.len() < count {
            m.run_frame();
            m.drain_audio(&mut out);
        }
        out
    }

    /// The samples from 0.5 s to 2.5 s after power-on (the register writes are in the first
    /// cycles of the program).
    fn measured_window(m: &mut Machine) -> Vec<f32> {
        let all = samples(m, RATE * 5 / 2);
        all[RATE / 2..RATE * 5 / 2].to_vec()
    }

    /// The samples from 1.0 s to 1.1 s after power-on (REQ-539).
    fn level_window(m: &mut Machine) -> Vec<f32> {
        let all = samples(m, RATE * 11 / 10);
        all[RATE..RATE * 11 / 10].to_vec()
    }

    /// Checks that the measured frequency is `expected` +/- `tolerance` Hz.
    fn check_frequency(samples: &[f32], expected: f64, tolerance: f64) {
        let f = measure::frequency(samples, TEST_SAMPLE_RATE, expected);
        assert!(
            (f - expected).abs() <= tolerance,
            "measured {f} Hz, expected {expected} +/- {tolerance} Hz"
        );
    }

    /// The registers of REQ-503 criteria 4 and 5: `AUDCTL` = `$00`, `AUDF1` = `$48`,
    /// `AUDC1` = `$AF`.
    const TONE_437: [(u8, u8); 3] = [(0x08, 0x00), (0x00, 0x48), (0x01, 0xAF)];

    /// Verifies REQ-503: Criterion 4: On NTSC, `AUDCTL` = `$00`, `AUDF1` = `$48` and `AUDC1` =
    /// `$AF` make the channel 1 digital output a square wave with a period of 4088 cycles (2 x 73
    /// x 28), with 2044 cycles at each level. At 48000 Hz, the measured frequency (see Notes) of
    /// the audio output is 437.8 Hz +/- 0.5 Hz.
    #[test]
    fn tone_437_hz_ntsc() {
        let mut m = machine(&pokey_code(&TONE_437), VideoStandard::Ntsc);
        check_frequency(&measured_window(&mut m), 437.8, 0.5);
    }

    /// Verifies REQ-503: Criterion 5: On PAL, the same register values give the same
    /// 4088-cycle period. The measured frequency is 433.8 Hz +/- 0.5 Hz.
    #[test]
    fn tone_433_hz_pal() {
        let mut m = machine(&pokey_code(&TONE_437), VideoStandard::Pal);
        check_frequency(&measured_window(&mut m), 433.8, 0.5);
    }

    /// Verifies REQ-503: BASIC `SOUND 0,121,10,8` sets `AUDF1` = 121 and `AUDC1` = `$A8`; the
    /// measured tone in the samples of `Machine::drain_audio` is 261.5-262.5 Hz on NTSC (system
    /// check).
    #[test]
    fn basic_sound_middle_c() {
        let mut m = boot(BootOptions::default()).unwrap();
        let ready = run_until(&mut m, BOOT_FRAMES, |m| has_row(m, "Ready"));
        assert!(ready.is_some(), "no BASIC prompt");
        type_text(&mut m, "SOUND 0,121,10,8\n").unwrap();
        run_frames(&mut m, 30);
        let mut discard = Vec::new();
        m.drain_audio(&mut discard);
        let tone = samples(&mut m, 2 * RATE);
        check_frequency(&tone[..2 * RATE], 262.0, 0.5);
    }

    /// Verifies REQ-511: Criterion 3: On NTSC, `AUDCTL` = `$64`, `AUDF1` = `$3B`, `AUDF3` =
    /// `$3C`, `AUDC1` = `$AF` and `AUDC3` = `$A0` make the channel 1 digital output repeat with a
    /// period of 4032 cycles (1789773 / 4032 = 443.9 Hz). At 48000 Hz, the measured frequency
    /// (REQ-503 Notes) of the audio output is 443.9 Hz +/- 1 Hz.
    #[test]
    fn high_pass_sawtooth_443_hz() {
        let code = pokey_code(&[
            (0x08, 0x64),
            (0x00, 0x3B),
            (0x04, 0x3C),
            (0x01, 0xAF),
            (0x05, 0xA0),
        ]);
        let mut m = machine(&code, VideoStandard::Ntsc);
        check_frequency(&measured_window(&mut m), 443.9, 1.0);
    }

    /// The machine of REQ-539 criterion 1 with `AUDC3` = `audc3`.
    fn channel_3_tone(audc3: u8) -> Machine {
        machine(
            &pokey_code(&[(0x08, 0x00), (0x04, 0x07), (0x05, audc3)]),
            VideoStandard::Ntsc,
        )
    }

    /// Verifies REQ-539: Criterion 1: `AUDCTL` = `$00`, `AUDF3` = `$07` and `AUDC3` = `$AF` give
    /// a measured frequency (REQ-503 Notes) of 3995 Hz +/- 2 Hz and a peak-to-peak of 0.23 to
    /// 0.28.
    #[test]
    fn square_wave_level() {
        let mut m = channel_3_tone(0xAF);
        let all = samples(&mut m, RATE * 5 / 2);
        check_frequency(&all[RATE / 2..RATE * 5 / 2], 3995.0, 2.0);
        let p2p = measure::peak_to_peak(&all[RATE..RATE * 11 / 10]);
        assert!((0.23..=0.28).contains(&p2p), "peak-to-peak {p2p}");
    }

    /// Verifies REQ-539: Criterion 2: The setup of criterion 1 with `AUDC3` = `$A8` gives a
    /// peak-to-peak of 8/15 of the value of criterion 1, +/- 5 %.
    #[test]
    fn volume_ratio() {
        let full = measure::peak_to_peak(&level_window(&mut channel_3_tone(0xAF)));
        let half = measure::peak_to_peak(&level_window(&mut channel_3_tone(0xA8)));
        let ratio = half / full;
        let expected = 8.0 / 15.0;
        assert!(
            (ratio - expected).abs() <= 0.05 * expected,
            "ratio {ratio} ({half} / {full})"
        );
    }

    /// Verifies REQ-539: Criterion 3: The speaker level changing every 895 cycles (a 1000 Hz
    /// square wave) gives a peak-to-peak of 0.23 to 0.28.
    #[test]
    fn speaker_level() {
        // `CONSOL` = `$08`, a delay, `CONSOL` = `$00`, a delay, and again. Each half of the
        // loop takes 6 + 5 x 163 + 1 CPU cycles plus the ANTIC refresh cycles, about 895
        // machine cycles.
        let half = |consol: u8| {
            let mut code = store(0xD01F, consol);
            code.extend([0xA2, 163, 0xCA, 0xD0, 0xFD]); // LDX #163; DEX; BNE *-1
            code
        };
        let mut code = half(0x08);
        code.extend(half(0x00));
        code.extend([0x4C, 0x00, 0xC0]); // JMP $C000
        let mut m = machine(&code, VideoStandard::Ntsc);
        let window = level_window(&mut m);
        check_frequency(&window, 1000.0, 100.0);
        let p2p = measure::peak_to_peak(&window);
        assert!((0.23..=0.28).contains(&p2p), "peak-to-peak {p2p}");
    }
}
