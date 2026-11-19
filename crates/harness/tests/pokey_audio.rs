//! POKEY audio in the samples of `Machine::drain_audio` (REQ-503, REQ-511, REQ-539).
//!
//! Each test is a stub until its package writes it (`docs/design/pokey.md`, "Test plan").

/// Fails the planned test stub that calls it: the test is not written yet (REQ-027).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: not written yet");
}

/// Verifies REQ-503: Criterion 4: On NTSC, `AUDCTL` = `$00`, `AUDF1` = `$48` and `AUDC1` = `$AF`
/// make the channel 1 digital output a square wave with a period of 4088 cycles (2 x 73 x 28), with
/// 2044 cycles at each level. At 48000 Hz, the measured frequency (see Notes) of the audio output
/// is 437.8 Hz +/- 0.5 Hz.
#[test]
#[ignore = "planned: SLICE-03"]
fn tone_437_hz_ntsc() {
    planned();
}

/// Verifies REQ-503: Criterion 5: On PAL, the same register values give the same 4088-cycle period.
/// The measured frequency is 433.8 Hz +/- 0.5 Hz.
#[test]
#[ignore = "planned: SLICE-03"]
fn tone_433_hz_pal() {
    planned();
}

/// Verifies REQ-503: BASIC `SOUND 0,121,10,8` sets `AUDF1` = 121 and `AUDC1` = `$A8`; the measured
/// tone in the samples of `Machine::drain_audio` is 261.5-262.5 Hz on NTSC (system check).
#[test]
#[ignore = "planned: SLICE-03"]
fn basic_sound_middle_c() {
    planned();
}

/// Verifies REQ-511: Criterion 3: On NTSC, `AUDCTL` = `$64`, `AUDF1` = `$3B`, `AUDF3` = `$3C`,
/// `AUDC1` = `$AF` and `AUDC3` = `$A0` make the channel 1 digital output repeat with a period of
/// 4032 cycles (1789773 / 4032 = 443.9 Hz). At 48000 Hz, the measured frequency (REQ-503 Notes) of
/// the audio output is 443.9 Hz +/- 1 Hz.
#[test]
#[ignore = "planned: SLICE-03"]
fn high_pass_sawtooth_443_hz() {
    planned();
}

/// Verifies REQ-539: Criterion 1: `AUDCTL` = `$00`, `AUDF3` = `$07` and `AUDC3` = `$AF` give a
/// measured frequency (REQ-503 Notes) of 3995 Hz +/- 2 Hz and a peak-to-peak of 0.23 to 0.28.
#[test]
#[ignore = "planned: SLICE-03"]
fn square_wave_level() {
    planned();
}

/// Verifies REQ-539: Criterion 2: The setup of criterion 1 with `AUDC3` = `$A8` gives a
/// peak-to-peak of 8/15 of the value of criterion 1, +/- 5 %.
#[test]
#[ignore = "planned: SLICE-03"]
fn volume_ratio() {
    planned();
}

/// Verifies REQ-539: Criterion 3: The speaker level changing every 895 cycles (a 1000 Hz square
/// wave) gives a peak-to-peak of 0.23 to 0.28.
#[test]
#[ignore = "planned: SLICE-03"]
fn speaker_level() {
    planned();
}
