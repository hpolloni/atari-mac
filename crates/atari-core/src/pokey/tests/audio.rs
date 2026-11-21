//! Tests of the audio channels, the noise generators, `RANDOM` and the samples (REQ-509 to
//! REQ-514, REQ-538 to REQ-541).
//!
//! The tests drive `Pokey` directly (`docs/design/pokey.md`, "Test plan"). The measurement
//! of the audio samples follows REQ-539 (peak-to-peak) and REQ-540 (step response).

use super::{
    AUDC1, AUDC3, AUDCTL, AUDF1, AUDF2, AUDF3, AUDF4, IRQEN, SKCTL, STIMER, clear_irq, irqst,
    new_pokey, run, spacing,
};
use crate::pokey::Pokey;
use crate::timing::VideoStandard;

/// `AUDC2`.
const AUDC2: u8 = 0x03;
/// `AUDC4`.
const AUDC4: u8 = 0x07;
/// `RANDOM` (read).
const RANDOM: u8 = 0x0A;
/// The NTSC machine clock rate of REQ-538.
const CLOCK_NTSC: u64 = 1_789_773;
/// Cycles in one NTSC frame.
const FRAME_NTSC: u64 = 29_868;
/// Cycles in one PAL frame.
const FRAME_PAL: u64 = 35_568;
/// The 4-bit pattern of HRM 5.5.
const PATTERN4: &str = "000011101100101";
/// The 5-bit pattern of HRM 5.5.
const PATTERN5: &str = "0000011100100010101111011010011";

/// The `RANDOM` values of HRM 14.4 in 9-bit mode, from the zero state of the counter.
const RANDOM_TABLE: [u8; 511] = [
    0x00, 0x80, 0x40, 0x20, 0x10, 0x88, 0x44, 0x22, 0x11, 0x88, 0xC4, 0x62, 0x31, 0x98, 0x4C, 0x26,
    0x13, 0x89, 0xC4, 0xE2, 0x71, 0xB8, 0x5C, 0xAE, 0x57, 0xAB, 0xD5, 0x6A, 0xB5, 0xDA, 0x6D, 0x36,
    0x1B, 0x8D, 0xC6, 0xE3, 0x71, 0x38, 0x1C, 0x8E, 0x47, 0x23, 0x91, 0x48, 0xA4, 0x52, 0xA9, 0x54,
    0x2A, 0x15, 0x8A, 0xC5, 0x62, 0xB1, 0xD8, 0x6C, 0x36, 0x9B, 0xCD, 0xE6, 0xF3, 0xF9, 0x7C, 0x3E,
    0x9F, 0xCF, 0xE7, 0xF3, 0x79, 0x3C, 0x1E, 0x8F, 0x47, 0xA3, 0xD1, 0x68, 0xB4, 0xDA, 0xED, 0x76,
    0x3B, 0x9D, 0x4E, 0xA7, 0x53, 0x29, 0x94, 0x4A, 0x25, 0x12, 0x09, 0x04, 0x82, 0x41, 0x20, 0x90,
    0xC8, 0x64, 0x32, 0x99, 0xCC, 0xE6, 0x73, 0xB9, 0x5C, 0x2E, 0x17, 0x8B, 0xC5, 0xE2, 0xF1, 0xF8,
    0x7C, 0xBE, 0xDF, 0xEF, 0xF7, 0x7B, 0x3D, 0x1E, 0x0F, 0x07, 0x83, 0xC1, 0xE0, 0xF0, 0xF8, 0xFC,
    0xFE, 0xFF, 0xFF, 0x7F, 0x3F, 0x1F, 0x0F, 0x87, 0xC3, 0xE1, 0xF0, 0x78, 0xBC, 0xDE, 0xEF, 0x77,
    0x3B, 0x1D, 0x0E, 0x87, 0x43, 0xA1, 0xD0, 0x68, 0x34, 0x9A, 0xCD, 0x66, 0xB3, 0xD9, 0x6C, 0xB6,
    0xDB, 0xED, 0xF6, 0x7B, 0xBD, 0x5E, 0x2F, 0x17, 0x0B, 0x85, 0xC2, 0xE1, 0x70, 0x38, 0x9C, 0xCE,
    0x67, 0x33, 0x19, 0x0C, 0x86, 0x43, 0x21, 0x90, 0x48, 0x24, 0x12, 0x89, 0x44, 0xA2, 0x51, 0xA8,
    0xD4, 0xEA, 0x75, 0xBA, 0x5D, 0xAE, 0xD7, 0xEB, 0xF5, 0x7A, 0x3D, 0x9E, 0x4F, 0x27, 0x93, 0x49,
    0xA4, 0xD2, 0xE9, 0x74, 0x3A, 0x9D, 0xCE, 0xE7, 0x73, 0x39, 0x1C, 0x0E, 0x07, 0x03, 0x81, 0xC0,
    0xE0, 0x70, 0xB8, 0xDC, 0xEE, 0x77, 0xBB, 0x5D, 0x2E, 0x97, 0xCB, 0xE5, 0xF2, 0x79, 0xBC, 0x5E,
    0xAF, 0x57, 0x2B, 0x95, 0x4A, 0xA5, 0x52, 0x29, 0x14, 0x0A, 0x05, 0x02, 0x81, 0x40, 0xA0, 0x50,
    0xA8, 0x54, 0xAA, 0x55, 0xAA, 0xD5, 0xEA, 0xF5, 0xFA, 0x7D, 0xBE, 0x5F, 0xAF, 0xD7, 0x6B, 0xB5,
    0x5A, 0x2D, 0x16, 0x0B, 0x05, 0x82, 0xC1, 0x60, 0xB0, 0xD8, 0xEC, 0x76, 0xBB, 0xDD, 0x6E, 0xB7,
    0xDB, 0x6D, 0xB6, 0x5B, 0xAD, 0xD6, 0x6B, 0x35, 0x1A, 0x0D, 0x06, 0x83, 0x41, 0xA0, 0xD0, 0xE8,
    0x74, 0xBA, 0xDD, 0xEE, 0xF7, 0xFB, 0x7D, 0x3E, 0x1F, 0x8F, 0xC7, 0xE3, 0xF1, 0x78, 0x3C, 0x9E,
    0xCF, 0x67, 0xB3, 0x59, 0x2C, 0x96, 0xCB, 0x65, 0xB2, 0x59, 0xAC, 0xD6, 0xEB, 0x75, 0x3A, 0x1D,
    0x8E, 0xC7, 0x63, 0xB1, 0x58, 0x2C, 0x16, 0x8B, 0x45, 0xA2, 0xD1, 0xE8, 0xF4, 0xFA, 0xFD, 0xFE,
    0x7F, 0xBF, 0x5F, 0x2F, 0x97, 0x4B, 0xA5, 0xD2, 0x69, 0x34, 0x1A, 0x8D, 0x46, 0xA3, 0x51, 0x28,
    0x94, 0xCA, 0x65, 0x32, 0x19, 0x8C, 0xC6, 0x63, 0x31, 0x18, 0x0C, 0x06, 0x03, 0x01, 0x80, 0xC0,
    0x60, 0x30, 0x98, 0xCC, 0x66, 0x33, 0x99, 0x4C, 0xA6, 0x53, 0xA9, 0xD4, 0x6A, 0x35, 0x9A, 0x4D,
    0x26, 0x93, 0xC9, 0xE4, 0xF2, 0xF9, 0xFC, 0x7E, 0xBF, 0xDF, 0x6F, 0xB7, 0x5B, 0x2D, 0x96, 0x4B,
    0x25, 0x92, 0x49, 0x24, 0x92, 0xC9, 0x64, 0xB2, 0xD9, 0xEC, 0xF6, 0xFB, 0xFD, 0x7E, 0x3F, 0x9F,
    0x4F, 0xA7, 0xD3, 0x69, 0xB4, 0x5A, 0xAD, 0x56, 0x2B, 0x15, 0x0A, 0x85, 0x42, 0xA1, 0x50, 0x28,
    0x14, 0x8A, 0x45, 0x22, 0x91, 0xC8, 0xE4, 0x72, 0xB9, 0xDC, 0x6E, 0x37, 0x9B, 0x4D, 0xA6, 0xD3,
    0xE9, 0xF4, 0x7A, 0xBD, 0xDE, 0x6F, 0x37, 0x1B, 0x0D, 0x86, 0xC3, 0x61, 0xB0, 0x58, 0xAC, 0x56,
    0xAB, 0x55, 0x2A, 0x95, 0xCA, 0xE5, 0x72, 0x39, 0x9C, 0x4E, 0x27, 0x13, 0x09, 0x84, 0xC2, 0x61,
    0x30, 0x18, 0x8C, 0x46, 0x23, 0x11, 0x08, 0x84, 0x42, 0x21, 0x10, 0x08, 0x04, 0x02, 0x01,
];

/// The bits of a pattern string.
fn pattern(text: &str) -> Vec<bool> {
    text.chars().map(|c| c == '1').collect()
}

/// Writes `AUDF1-4` = `freqs` and `AUDC1-4` = `controls`.
fn write_channels(p: &mut Pokey, freqs: [u8; 4], controls: [u8; 4]) {
    for ((f, c), (freq, control)) in [
        (AUDF1, AUDC1),
        (AUDF2, AUDC2),
        (AUDF3, AUDC3),
        (AUDF4, AUDC4),
    ]
    .into_iter()
    .zip(freqs.into_iter().zip(controls))
    {
        p.write(f, freq);
        p.write(c, control);
    }
}

/// A POKEY with `SKCTL` = `$03` written on cycle W = 1 and an `STIMER` write on cycle
/// T = 52. Each call gives the same W and T.
///
/// Before T, the timers wait on the 64 kHz clock with `AUDF1-4` = `$FF`, so that no fire is
/// scheduled at T. `AUDC1-4` and `AUDCTL` bit 7 (the noise mode) are written on W. `AUDF1-4`
/// and `AUDCTL` are written on T, before the `STIMER` write. Returns the POKEY, W and T.
fn started(audctl: u8, freqs: [u8; 4], controls: [u8; 4]) -> (Pokey, u64, u64) {
    let mut p = new_pokey();
    p.tick();
    p.write(SKCTL, 0x03);
    let w = p.cycle();
    p.write(AUDCTL, audctl & 0x80);
    write_channels(&mut p, [0xFF; 4], controls);
    p.write(STIMER, 0x00);
    run(&mut p, 50);
    p.tick();
    for (reg, value) in [AUDF1, AUDF2, AUDF3, AUDF4].into_iter().zip(freqs) {
        p.write(reg, value);
    }
    p.write(AUDCTL, audctl);
    p.write(STIMER, 0x00);
    let t = p.cycle();
    (p, w, t)
}

/// The state of one channel after one fire of its timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fire {
    /// The fire cycle.
    cycle: u64,
    /// The digital output on the cycle before the fire.
    before: bool,
    /// The digital output on the fire cycle.
    after: bool,
    /// The output flip-flops of the four channels on the fire cycle.
    flip_flops: [bool; 4],
}

/// Runs until timer `n + 1` fired `count` times and returns the fires.
fn fires(p: &mut Pokey, n: usize, count: usize) -> Vec<Fire> {
    let mut list = Vec::with_capacity(count);
    let mut before = p.test_channel_output(n);
    while list.len() < count {
        p.tick();
        let after = p.test_channel_output(n);
        if p.test_fired() & (1 << n) != 0 {
            list.push(Fire {
                cycle: p.cycle(),
                before,
                after,
                flip_flops: p.test_flip_flops(),
            });
        }
        before = after;
    }
    list
}

/// The smallest p with `seq[i + p] == seq[i]` for all i, or `None` if no p up to half the
/// length works.
fn period<T: PartialEq>(seq: &[T]) -> Option<usize> {
    (1..=seq.len() / 2).find(|p| (0..seq.len() - p).all(|i| seq[i + p] == seq[i]))
}

/// The digital output of channel `n + 1` on the next `cycles` cycles.
fn outputs(p: &mut Pokey, n: usize, cycles: usize) -> Vec<bool> {
    (0..cycles)
        .map(|_| {
            p.tick();
            p.test_channel_output(n)
        })
        .collect()
}

/// Runs `cycles` cycles and checks the mix level after each tick.
fn check_mix(p: &mut Pokey, cycles: u64, what: &str, ok: impl Fn(u8) -> bool) {
    for _ in 0..cycles {
        p.tick();
        let level = p.test_mix_level();
        assert!(
            ok(level),
            "{what}: mix level {level} on cycle {}",
            p.cycle()
        );
    }
}

/// Runs `cycles` cycles and returns the samples that they made.
fn run_samples(p: &mut Pokey, cycles: u64) -> Vec<f32> {
    run(p, cycles);
    let mut samples = Vec::new();
    p.drain_samples(&mut samples);
    samples
}

/// The maximum minus the minimum of `samples`.
fn peak_to_peak(samples: &[f32]) -> f32 {
    let max = samples.iter().copied().fold(f32::MIN, f32::max);
    let min = samples.iter().copied().fold(f32::MAX, f32::min);
    max - min
}

/// Runs until a sample ends in a cycle, and moves all samples to `samples`. A write after
/// this cycle is on the first cycle of the next sample, `samples.len()`.
fn run_to_sample_end(p: &mut Pokey, samples: &mut Vec<f32>) {
    let start = samples.len();
    while samples.len() == start {
        p.tick();
        p.drain_samples(samples);
    }
}

// ----------------------------------------------------------------------------------------
// Distortion (REQ-509)
// ----------------------------------------------------------------------------------------

/// The setup of REQ-509 criteria 1-6: timer 3 at 1.79 MHz with `AUDF3` = `$00`.
fn fast3(audctl: u8, audc3: u8) -> Pokey {
    started(audctl, [0; 4], [0, 0, audc3, 0]).0
}

/// The channel 3 outputs after `count` fires of timer 3.
fn outputs_after_fires(p: &mut Pokey, count: usize) -> Vec<bool> {
    fires(p, 2, count).iter().map(|f| f.after).collect()
}

/// Verifies REQ-509: Criterion 1: With `AUDCTL` = `$20` and `AUDC3` = `$AF`, the output changes on
/// each fire of timer 3 and on no other cycle.
#[test]
fn distortion_pure_tone() {
    let mut p = fast3(0x20, 0xAF);
    let mut last = p.test_channel_output(2);
    let mut changes = 0;
    for _ in 0..1000 {
        p.tick();
        let now = p.test_channel_output(2);
        let fired = p.test_fired() & 0x04 != 0;
        assert_eq!(now != last, fired, "change on cycle {}", p.cycle());
        changes += usize::from(fired);
        last = now;
    }
    assert!(changes >= 240, "{changes} fires");
}

/// Verifies REQ-509: Criterion 2: With `AUDCTL` = `$20` and `AUDC3` = `$CF`, the outputs after 15
/// consecutive fires contain 7 ones and 8 zeros. The sequence of outputs after the fires repeats
/// with a period of 15 fires.
#[test]
fn distortion_poly4() {
    let mut p = fast3(0x20, 0xCF);
    let seq = outputs_after_fires(&mut p, 15 * 8);
    assert_eq!(period(&seq), Some(15), "period");
    for window in seq.windows(15) {
        assert_eq!(
            window.iter().filter(|b| **b).count(),
            7,
            "ones in {window:?}"
        );
    }
}

/// Verifies REQ-509: Criterion 3: With `AUDCTL` = `$A0` and `AUDC3` = `$8F`, the sequence of
/// outputs after the fires repeats with a period of 511 fires and has 255 ones in each period.
#[test]
fn distortion_poly9() {
    let mut p = fast3(0xA0, 0x8F);
    let seq = outputs_after_fires(&mut p, 511 * 3);
    assert_eq!(period(&seq), Some(511), "period");
    assert_eq!(seq[..511].iter().filter(|b| **b).count(), 255, "ones");
}

/// Verifies REQ-509: Criterion 4: With `AUDCTL` = `$20` and `AUDC3` = `$8F`, the sequence of
/// outputs after the fires repeats with a period of 131071 fires and has 65535 ones in each period.
/// The test runs channel 3 for 131071 fires in 17-bit mode and counts the ones.
#[test]
fn distortion_poly17() {
    const N: usize = 0x0001_FFFF; // 131071
    let mut p = fast3(0x20, 0x8F);
    let seq = outputs_after_fires(&mut p, 2 * N);
    assert!(
        (0..N).all(|i| seq[i + N] == seq[i]),
        "the outputs repeat after 131071 fires"
    );
    // 131071 is prime, so the period is 131071 if the sequence is not constant.
    assert!(
        seq[..N].contains(&true) && seq[..N].contains(&false),
        "constant"
    );
    assert_eq!(seq[..N].iter().filter(|b| **b).count(), 0xFFFF, "ones");
}

/// The fires (as indexes) on which the channel 3 output changes, for `count` fires.
fn changes(p: &mut Pokey, count: usize) -> Vec<bool> {
    fires(p, 2, count)
        .iter()
        .map(|f| f.before != f.after)
        .collect()
}

/// Verifies REQ-509: Criterion 5: With `AUDCTL` = `$20` and `AUDC3` = `$2F`, the output changes on
/// exactly 15 of each 31 consecutive fires. The pattern of changes repeats with a period of 31
/// fires.
#[test]
fn distortion_poly5_mask() {
    let mut p = fast3(0x20, 0x2F);
    let seq = changes(&mut p, 31 * 6);
    assert_eq!(period(&seq), Some(31), "period");
    for window in seq.windows(31) {
        assert_eq!(window.iter().filter(|b| **b).count(), 15, "changes");
    }
}

/// Verifies REQ-509: Criterion 6: With `AUDCTL` = `$20` and `AUDC3` = `$0F` or `$4F`, the output
/// changes only on fires on which the output changes in criterion 5 (the fires that the 5-bit
/// generator lets through). The test runs criterion 5 and criterion 6 from the same `SKCTL` write
/// cycle and the same `STIMER` write cycle. The test runs `AUDC3` = `$2F`, `$0F` and `$4F` from the
/// same start cycles and compares the fires that change the output.
#[test]
fn distortion_poly5_masks_noise() {
    let count = 31 * 40;
    let gate = changes(&mut fast3(0x20, 0x2F), count);
    for audc3 in [0x0F, 0x4F] {
        let noise = changes(&mut fast3(0x20, audc3), count);
        let mut n = 0;
        for (i, (changed, open)) in noise.iter().zip(&gate).enumerate() {
            assert!(
                !changed || *open,
                "AUDC3 = {audc3:#04X}: change on masked fire {i}"
            );
            n += usize::from(*changed);
        }
        assert!(n > 100, "AUDC3 = {audc3:#04X}: {n} changes");
    }
}

/// Verifies REQ-509: Criterion 7: With `AUDCTL` = `$28`, `AUDF3` = `$28`, `AUDF4` = `$00`, `SKCTL`
/// = `$23` and `IRQEN` = `$04`, the cycles on which `IRQST` bit 2 goes to 0 and the cycles on which
/// SIO CLOCK OUT changes are the same for each of the 8 values of `AUDC4` bits 7-5.
#[test]
fn distortion_ignored_by_irq_and_serial() {
    let run_mode = |audc4: u8| {
        let mut p = new_pokey();
        p.tick();
        p.write(SKCTL, 0x23);
        p.write(AUDCTL, 0x28);
        write_channels(&mut p, [0, 0, 0x28, 0x00], [0, 0, 0, audc4]);
        p.write(IRQEN, 0x04);
        let mut events = Vec::new();
        let mut clock = Vec::new();
        let mut last = p.serial_clock_out();
        for _ in 0..3000 {
            p.tick();
            if irqst(&p) & 0x04 == 0 {
                events.push(p.cycle());
                clear_irq(&mut p, 0x04, 0x04);
            }
            if p.serial_clock_out() != last {
                clock.push(p.cycle());
                last = p.serial_clock_out();
            }
        }
        (events, clock)
    };
    let reference = run_mode(0x0F);
    assert!(reference.0.len() > 30, "IRQ events {:?}", reference.0);
    assert!(reference.1.len() > 30, "clock changes {:?}", reference.1);
    for bits in 0..8_u8 {
        let audc4 = (bits << 5) | 0x0F;
        assert_eq!(run_mode(audc4), reference, "AUDC4 = {audc4:#04X}");
    }
}

// ----------------------------------------------------------------------------------------
// Volume (REQ-510) and high-pass filters (REQ-511)
// ----------------------------------------------------------------------------------------

/// Verifies REQ-510: Criterion 1: With `AUDC1` = `$1F` and `AUDC2-4` = `$00`, the mix level is 15
/// on each cycle for 10000 cycles. This is true with `AUDCTL` = `$00` and with `AUDCTL` = `$40`,
/// with `AUDF1` = `$00`.
#[test]
fn volume_only_level() {
    for audctl in [0x00, 0x40] {
        let (mut p, _, _) = started(audctl, [0; 4], [0x1F, 0, 0, 0]);
        check_mix(&mut p, 10_000, "AUDC1 = $1F", |level| level == 15);
    }
}

/// Verifies REQ-510: Criterion 2: With `AUDC1` = `$10` and `AUDC2-4` = `$00`, the mix level is 0 on
/// each cycle for 10000 cycles.
#[test]
fn volume_only_zero() {
    let (mut p, _, _) = started(0x00, [0; 4], [0x10, 0, 0, 0]);
    check_mix(&mut p, 10_000, "AUDC1 = $10", |level| level == 0);
}

/// Verifies REQ-510: Criterion 3: With `AUDC1` = `$18`, `AUDC3` = `$18` and `AUDC2` = `AUDC4` =
/// `$00`, the mix level is 16 on each cycle for 1000 cycles after an `STIMER` write, for each of
/// the 256 `AUDCTL` values.
#[test]
fn volume_only_channels_add() {
    for audctl in 0..=0xFF {
        let (mut p, _, _) = started(audctl, [0; 4], [0x18, 0, 0x18, 0]);
        check_mix(&mut p, 1000, &format!("AUDCTL = {audctl:#04X}"), |level| {
            level == 16
        });
    }
}

/// Verifies REQ-510: Criterion 4: With `AUDCTL` = `$01`, `AUDF1` = `AUDF3` = `$FF`, `AUDC1` =
/// `$AF`, `AUDC3` = `$A7`, `AUDC2` = `AUDC4` = `$00` and an `STIMER` write on cycle T, the mix
/// level is 7 on each cycle from T+1 to T+1000. Channel 1 has output 0 and adds nothing. Channel 3
/// has output 1 and adds 7. The test checks the mix level 7 of REQ-510 criterion 4 for 1000 cycles.
#[test]
fn output_zero_adds_nothing() {
    let (mut p, _, _) = started(0x01, [0xFF, 0, 0xFF, 0], [0xAF, 0, 0xA7, 0]);
    for _ in 0..1000 {
        p.tick();
        assert!(!p.test_channel_output(0), "channel 1 output");
        assert!(p.test_channel_output(2), "channel 3 output");
        assert_eq!(p.test_mix_level(), 7, "mix level on cycle {}", p.cycle());
    }
}

/// Verifies REQ-510: Criterion 5: With `AUDCTL` = `$20`, `AUDF3` = `$00`, `AUDC3` = `$BF` and an
/// `STIMER` write on cycle T, timer 3 fires on cycles T+8 and T+12. The channel 3 flip-flop (test
/// accessor) changes on each fire, and the digital output stays 1. A write of `AUDC3` = `$AF` on
/// cycle T+10 makes the digital output 0 on cycle T+11 and 1 on cycle T+12.
#[test]
fn volume_only_keeps_flip_flop_running() {
    let (mut p, _, t) = started(0x20, [0; 4], [0, 0, 0xBF, 0]);
    let mut flip_flop = p.test_flip_flops()[2];
    for _ in 0..9 {
        p.tick();
        let fired = p.test_fired() & 0x04 != 0;
        let offset = p.cycle() - t;
        if offset >= 4 {
            assert_eq!(fired, offset == 8, "fire of timer 3 on T+{offset}");
        }
        if offset >= 4 {
            assert_eq!(
                p.test_flip_flops()[2] != flip_flop,
                fired,
                "flip-flop change on T+{offset}"
            );
        }
        flip_flop = p.test_flip_flops()[2];
        assert!(p.test_channel_output(2), "digital output on T+{offset}");
    }
    p.tick();
    assert_eq!(p.cycle(), t + 10, "cycle");
    assert!(p.test_channel_output(2), "digital output on T+10");
    p.write(AUDC3, 0xAF);
    p.tick();
    assert!(!p.test_channel_output(2), "digital output on T+11");
    p.tick();
    assert_eq!(p.test_fired() & 0x04, 0x04, "fire of timer 3 on T+12");
    assert!(p.test_channel_output(2), "digital output on T+12");
}

/// Verifies REQ-510: Criterion 6: With `AUDC1-4` = `$1F`, the mix level is 60. The test sets
/// `AUDC1-4` = `$1F` and checks the mix level.
#[test]
fn mix_level_maximum_60() {
    let (mut p, _, _) = started(0x00, [0; 4], [0x1F; 4]);
    assert_eq!(p.test_mix_level(), 60, "after the writes");
    check_mix(&mut p, 1000, "AUDC1-4 = $1F", |level| level == 60);
}

/// Verifies REQ-511: Criterion 1: With `AUDCTL` = `$00`, `AUDF1` = `AUDF3` = `$10`, `AUDC1` =
/// `AUDC3` = `$A8`, `AUDC2` = `AUDC4` = `$00` and an `STIMER` write, the mix level is 8 on each
/// cycle for 10000 cycles.
#[test]
fn ch1_ch3_cancel_without_high_pass() {
    let (mut p, _, _) = started(0x00, [0x10, 0, 0x10, 0], [0xA8, 0, 0xA8, 0]);
    check_mix(&mut p, 10_000, "channels 1 and 3", |level| level == 8);
}

/// Verifies REQ-511: Criterion 2: With `AUDCTL` = `$00`, `AUDF1` = `AUDF2` = `$10`, `AUDC1` =
/// `AUDC2` = `$A8`, `AUDC3` = `AUDC4` = `$00` and an `STIMER` write, the mix level is 0 or 16 on
/// each cycle, and it changes between 0 and 16 every 476 cycles (17 x 28).
#[test]
fn ch1_ch2_add_without_high_pass() {
    let (mut p, _, _) = started(0x00, [0x10, 0x10, 0, 0], [0xA8, 0xA8, 0, 0]);
    let mut last = p.test_mix_level();
    let mut changes = Vec::new();
    for _ in 0..10_000 {
        p.tick();
        let level = p.test_mix_level();
        assert!(level == 0 || level == 16, "mix level {level}");
        if level != last {
            changes.push(p.cycle());
            last = level;
        }
    }
    assert!(changes.len() >= 19, "{} changes", changes.len());
    assert!(
        spacing(&changes).iter().all(|d| *d == 476),
        "{:?}",
        spacing(&changes)
    );
}

/// The setup of REQ-511 criterion 3 with `AUDC3` = `audc3`, after 10000 cycles.
fn sawtooth(audc3: u8) -> Pokey {
    let (mut p, _, _) = started(0x64, [0x3B, 0, 0x3C, 0], [0xAF, 0, audc3, 0]);
    run(&mut p, 10_000);
    p
}

/// Verifies REQ-511: Criterion 3: On NTSC, `AUDCTL` = `$64`, `AUDF1` = `$3B`, `AUDF3` = `$3C`,
/// `AUDC1` = `$AF` and `AUDC3` = `$A0` make the channel 1 digital output repeat with a period of
/// 4032 cycles (1789773 / 4032 = 443.9 Hz). At 48000 Hz, the measured frequency (REQ-503 Notes) of
/// the audio output is 443.9 Hz +/- 1 Hz. The test checks that the channel 1 digital output repeats
/// every 4032 cycles.
#[test]
fn high_pass_period_4032() {
    let seq = outputs(&mut sawtooth(0xA0), 0, 3 * 4032);
    assert_eq!(period(&seq), Some(4032), "period");
}

/// Verifies REQ-511: Criterion 4: With `AUDCTL` = `$2A`, `AUDF2` = `$00`, `AUDF3` = `AUDF4` = `$00`
/// and `AUDC2` = `$AF`, the channel 2 digital output changes to 1 on each fire of timer 2 and
/// changes to 0 on the next fire of timer 4. The test selects an `STIMER` write cycle on which no
/// fire of timer 2 is on the same cycle as a fire of timer 4. The test checks the changes of the
/// channel 2 digital output against the fire log of timers 2 and 4.
#[test]
fn high_pass_channel_2() {
    let setup = |delay: u64| {
        let mut p = new_pokey();
        p.tick();
        p.write(SKCTL, 0x03);
        p.write(AUDCTL, 0x2A);
        write_channels(&mut p, [0; 4], [0, 0xAF, 0, 0]);
        run(&mut p, 50 + delay);
        p.tick();
        p.write(STIMER, 0x00);
        p
    };
    // One timer 2 period has 4 timer 4 periods (28 = 4 x 7), so the fires either always or
    // never share a cycle.
    let together = |p: &mut Pokey| {
        (0..500).any(|_| {
            p.tick();
            p.test_fired() & 0x0A == 0x0A
        })
    };
    let delay = (0..28)
        .find(|d| !together(&mut setup(*d)))
        .unwrap_or_else(|| panic!("no STIMER cycle without shared fires"));
    let mut p = setup(delay);
    // The check starts after the first fires of both timers after the STIMER write.
    run(&mut p, 100);
    let mut last = p.test_channel_output(1);
    let mut rises = 0;
    let mut falls = 0;
    for _ in 0..5000 {
        p.tick();
        let now = p.test_channel_output(1);
        let fired2 = p.test_fired() & 0x02 != 0;
        let fired4 = p.test_fired() & 0x08 != 0;
        if fired2 {
            assert!(
                !last && now,
                "rise on the timer 2 fire on cycle {}",
                p.cycle()
            );
            rises += 1;
        } else if fired4 && last {
            assert!(!now, "fall on the timer 4 fire on cycle {}", p.cycle());
            falls += 1;
        } else {
            assert_eq!(now, last, "no change on cycle {}", p.cycle());
        }
        last = now;
    }
    assert!(
        rises > 100 && falls + 1 >= rises,
        "{rises} rises, {falls} falls"
    );
}

/// Verifies REQ-511: Criterion 5: With the setup of criterion 3, the channel 1 digital output is
/// the same on each cycle for `AUDC3` = `$A0`, `$00`, `$1F`, `$8F` and `$C5`.
#[test]
fn high_pass_ignores_audc3() {
    let reference = outputs(&mut sawtooth(0xA0), 0, 3 * 4032);
    for audc3 in [0x00, 0x1F, 0x8F, 0xC5] {
        let seq = outputs(&mut sawtooth(audc3), 0, 3 * 4032);
        assert!(seq == reference, "AUDC3 = {audc3:#04X}");
    }
}

// ----------------------------------------------------------------------------------------
// Noise generators (REQ-512 to REQ-514)
// ----------------------------------------------------------------------------------------

/// The offset d (0 or 1) with `flip_flop(C) == pattern[(C - W - d) mod len]` for all fires,
/// for the 4-bit pattern on channel 1. The pattern index of a fire is then `C - W - d`.
fn pattern_offset(list: &[Fire], w: u64, bits: &[bool]) -> Option<u64> {
    let len = bits.len() as u64;
    (0..2).find(|d| {
        list.iter()
            .all(|f| f.flip_flops[0] == bits[usize::try_from((f.cycle - w - d) % len).unwrap()])
    })
}

/// Checks that channel `channel + 1` gets bit i - `delay` of the 4-bit pattern when channel 1
/// gets bit i, for 30 fires.
fn check_poly4_delay(audctl: u8, controls: [u8; 4], channel: usize, delay: u64) {
    let bits = pattern(PATTERN4);
    let (mut p, w, _) = started(audctl, [0; 4], controls);
    let list = fires(&mut p, 0, 30);
    let offset = pattern_offset(&list, w, &bits).unwrap_or_else(|| panic!("channel 1 pattern"));
    for fire in &list {
        let index = (fire.cycle - w - offset) % 15;
        let expected = bits[usize::try_from((index + 15 - delay) % 15).unwrap()];
        assert_eq!(
            fire.flip_flops[channel],
            expected,
            "channel {} on cycle {}",
            channel + 1,
            fire.cycle
        );
    }
}

/// Verifies REQ-512: Criterion 1: With `AUDCTL` = `$00`, `AUDF1` = `AUDF2` = `$00`, `AUDC1` =
/// `AUDC2` = `$CF` and an `STIMER` write, timers 1 and 2 fire on the same cycles. When channel 1
/// gets bit i of the 4-bit pattern of REQ-513, channel 2 gets bit i - 1 (modulo 15) on the same
/// fire.
#[test]
fn noise_delay_ch1_ch2() {
    let (mut p, _, _) = started(0x00, [0; 4], [0xCF, 0xCF, 0, 0]);
    for _ in 0..30 {
        let _f = fires(&mut p, 0, 1);
        assert_eq!(p.test_fired() & 0x03, 0x03, "timers 1 and 2 together");
    }
    check_poly4_delay(0x00, [0xCF, 0xCF, 0, 0], 1, 1);
}

/// Verifies REQ-512: Criterion 2: With `AUDCTL` = `$60`, `AUDF1` = `AUDF3` = `$00`, `AUDC1` =
/// `AUDC3` = `$CF` and an `STIMER` write, when channel 1 gets bit i of the 4-bit pattern, channel 3
/// gets bit i - 2 (modulo 15) on the same fire. The test compares the bits that channels 1 and 3
/// take on the same fires.
#[test]
fn noise_delay_ch1_ch3() {
    let (mut p, _, _) = started(0x60, [0; 4], [0xCF, 0, 0xCF, 0]);
    for _ in 0..30 {
        let _f = fires(&mut p, 0, 1);
        assert_eq!(p.test_fired() & 0x05, 0x05, "timers 1 and 3 together");
    }
    check_poly4_delay(0x60, [0xCF, 0, 0xCF, 0], 2, 2);
}

/// The setup of REQ-512 criterion 3, after 1000 cycles. Returns the POKEY, the
/// 9-bit pattern bits (the inverse of `RANDOM` bit 0) of each cycle from cycle 0, and 30
/// fires of timer 1.
fn poly9_fires() -> (Vec<bool>, Vec<Fire>) {
    let (mut p, _, _) = started(0x80, [0; 4], [0x8F, 0, 0, 0x8F]);
    let mut bits = vec![false; usize::try_from(p.cycle()).unwrap_or(0) + 1];
    let mut list = Vec::new();
    while list.len() < 30 {
        p.tick();
        bits.push(p.peek(RANDOM) & 1 == 0);
        if p.cycle() > 1000 && p.test_fired() & 0x01 != 0 {
            assert_eq!(p.test_fired() & 0x09, 0x09, "timers 1 and 4 together");
            list.push(Fire {
                cycle: p.cycle(),
                before: false,
                after: false,
                flip_flops: p.test_flip_flops(),
            });
        }
    }
    (bits, list)
}

/// The pattern bit of cycle `c`.
fn bit_at(bits: &[bool], c: u64) -> bool {
    bits[usize::try_from(c).unwrap_or(usize::MAX)]
}

/// Verifies REQ-512: Criterion 3: With `AUDCTL` = `$80`, `AUDF1` = `AUDF4` = `$00`, `AUDC1` =
/// `AUDC4` = `$8F` and an `STIMER` write, when channel 1 gets bit i of the 9-bit pattern, channel 4
/// gets bit i - 3 (modulo 511) on the same fire. The 9-bit pattern is the inverse of `RANDOM` bit 0
/// on consecutive cycles.
#[test]
fn noise_delay_ch1_ch4() {
    let (bits, list) = poly9_fires();
    // The pattern index of channel 1 is the cycle of the bit, C - e.
    let offsets: Vec<u64> = (0..511)
        .filter(|e| {
            list.iter()
                .all(|f| f.flip_flops[0] == bit_at(&bits, f.cycle - e))
        })
        .collect();
    assert_eq!(offsets.len(), 1, "channel 1 pattern offsets {offsets:?}");
    let e = offsets[0];
    for f in &list {
        assert_eq!(
            f.flip_flops[3],
            bit_at(&bits, f.cycle - e - 3),
            "channel 4 on cycle {}",
            f.cycle
        );
    }
}

/// Verifies REQ-512: Criterion 4: With the setup of criterion 3, the bit that channel 1 gets on a
/// fire on cycle C is the inverse of `RANDOM` bit 0 on cycle C - 1. The test reads `RANDOM` on each
/// cycle and compares bit 0 with the bit that channel 1 takes one cycle later.
#[test]
fn noise_follows_random_bit_0() {
    let (bits, list) = poly9_fires();
    let mut ones = 0;
    for f in &list {
        assert_eq!(
            f.flip_flops[0],
            bit_at(&bits, f.cycle - 1),
            "channel 1 on cycle {}",
            f.cycle
        );
        ones += usize::from(f.flip_flops[0]);
    }
    assert!((1..30).contains(&ones), "{ones} ones in 30 fires");
}

/// A POKEY that ran for 1000 cycles with `SKCTL` = `$03`, then 20 cycles with `SKCTL` =
/// `$00`, and then `SKCTL` = `$03` on cycle W. Returns W.
fn init_exit(p: &mut Pokey) -> u64 {
    p.tick();
    p.write(SKCTL, 0x03);
    run(p, 1000);
    p.write(SKCTL, 0x00);
    run(p, 20);
    p.tick();
    p.write(SKCTL, 0x03);
    p.cycle()
}

/// The noise bits of channel 1 on cycles W, W+1, ... for `cycles` cycles, as selected by
/// `select`.
fn noise_sequence(cycles: usize, select: impl Fn(&Pokey) -> bool) -> Vec<bool> {
    let mut p = new_pokey();
    let _w = init_exit(&mut p);
    let mut seq = vec![select(&p)];
    for _ in 1..cycles {
        p.tick();
        seq.push(select(&p));
    }
    seq
}

/// Checks that `seq` starts with `bits` at index 0 or 1 and then repeats.
fn check_pattern(seq: &[bool], bits: &[bool]) {
    let len = bits.len();
    let d = (0..2).find(|d| seq[*d..*d + len] == *bits);
    assert!(d.is_some(), "pattern {bits:?} not at W or W+1: {seq:?}");
    let d = d.unwrap_or(0);
    assert_eq!(period(&seq[d..]), Some(len), "period");
}

/// Verifies REQ-513: Criterion 1: The 4-bit noise bits of channel 1 on cycles W+d, W+d+1, W+d+2,
/// ... are `000011101100101`, and then the sequence repeats with a period of 15 cycles. d is 0 or
/// 1.
#[test]
fn poly4_pattern() {
    let seq = noise_sequence(15 * 6, |p| p.test_noise_bits(0).poly4);
    check_pattern(&seq, &pattern(PATTERN4));
}

/// Verifies REQ-513: Criterion 2: The 5-bit noise bits of channel 1 on cycles W+d, W+d+1, ... are
/// `0000011100100010101111011010011`, and then the sequence repeats with a period of 31 cycles. d
/// is 0 or 1.
#[test]
fn poly5_pattern() {
    let seq = noise_sequence(31 * 6, |p| p.test_noise_bits(0).poly5);
    check_pattern(&seq, &pattern(PATTERN5));
}

/// Verifies REQ-513: Criterion 3: From 9 cycles after a write of `SKCTL` = `$00` until the next
/// write that ends initialization mode, the 4-bit noise bit of each channel is 0 and the 5-bit
/// noise bit of each channel is 1. With `AUDCTL` = `$40`, `AUDF1` = `$00` and `AUDC1` = `$CF`, the
/// channel 1 flip-flop is 0 after each fire. With `AUDC1` = `$2F`, the channel 1 flip-flop changes
/// on each fire.
#[test]
fn poly_outputs_in_init_mode() {
    let mut p = new_pokey();
    p.tick();
    p.write(SKCTL, 0x03);
    run(&mut p, 1000);
    p.write(SKCTL, 0x00);
    run(&mut p, 8);
    let check = |p: &Pokey| {
        for n in 0..4 {
            let bits = p.test_noise_bits(n);
            assert!(
                !bits.poly4,
                "4-bit bit of channel {} on cycle {}",
                n + 1,
                p.cycle()
            );
            assert!(
                bits.poly5,
                "5-bit bit of channel {} on cycle {}",
                n + 1,
                p.cycle()
            );
        }
    };
    for _ in 0..2000 {
        p.tick();
        check(&p);
    }
    p.write(AUDCTL, 0x40);
    p.write(AUDF1, 0x00);
    p.write(AUDC1, 0xCF);
    p.write(STIMER, 0x00);
    for f in fires(&mut p, 0, 50) {
        assert!(
            !f.flip_flops[0],
            "AUDC1 = $CF: flip-flop after the fire on {}",
            f.cycle
        );
    }
    check(&p);
    p.write(AUDC1, 0x2F);
    let mut last = p.test_flip_flops()[0];
    for f in fires(&mut p, 0, 50) {
        assert_ne!(f.flip_flops[0], last, "AUDC1 = $2F: fire on {}", f.cycle);
        last = f.flip_flops[0];
    }
    check(&p);
}

/// The reads of `RANDOM` on the next `cycles` cycles.
fn random_reads(p: &mut Pokey, cycles: usize) -> Vec<u8> {
    (0..cycles)
        .map(|_| {
            p.tick();
            p.peek(RANDOM)
        })
        .collect()
}

/// A POKEY with `AUDCTL` = `audctl` and `SKCTL` = `$03`, after 1000 cycles.
fn random_pokey(audctl: u8) -> Pokey {
    let mut p = new_pokey();
    p.tick();
    p.write(AUDCTL, audctl);
    p.write(SKCTL, 0x03);
    run(&mut p, 1000);
    p
}

/// Verifies REQ-514: Criterion 1: With `AUDCTL` = `$80`, the reads of `RANDOM` on 511 consecutive
/// cycles are a rotation of the 511 entries of the HRM 14.4 table (entry 0 = `$00`, entry 1 =
/// `$80`, entry 2 = `$40`, entry 3 = `$20`, entry 4 = `$10`, entry 5 = `$88`, ...). The read on
/// cycle C + 511 is equal to the read on cycle C.
#[test]
fn random_9bit_table() {
    let reads = random_reads(&mut random_pokey(0x80), 2 * 511);
    let rotation = (0..511).find(|r| (0..511).all(|k| reads[k] == RANDOM_TABLE[(k + r) % 511]));
    assert!(rotation.is_some(), "no rotation of the HRM table");
    assert!((0..511).all(|c| reads[c + 511] == reads[c]), "period 511");
}

/// Verifies REQ-514: Criterion 2: With `AUDCTL` = `$80`, there is a cycle C in each 511 consecutive
/// cycles on which the reads of `RANDOM` on cycles C, C+114, C+228, C+342 and C+456 are `$00`,
/// `$DF`, `$EE`, `$16` and `$B9`.
#[test]
fn random_9bit_every_scanline() {
    const SEQUENCE: [u8; 5] = [0x00, 0xDF, 0xEE, 0x16, 0xB9];
    let reads = random_reads(&mut random_pokey(0x80), 2000);
    let hit = |c: usize| (0..5).all(|k| reads[c + 114 * k] == SEQUENCE[k]);
    for start in 0..1000 {
        assert!(
            (start..start + 511).any(hit),
            "no sequence in the 511 cycles from {start}"
        );
    }
}

/// Verifies REQ-514: Criterion 3: With `AUDCTL` = `$80`, after at least 18 cycles of initialization
/// mode, a read of `RANDOM` 4 cycles after the `SKCTL` = `$03` write returns `$1F`.
#[test]
fn random_after_init_exit_1f() {
    let mut p = new_pokey();
    p.tick();
    p.write(AUDCTL, 0x80);
    let w = init_exit(&mut p);
    run(&mut p, 4);
    assert_eq!(p.cycle(), w + 4, "cycle");
    assert_eq!(p.read(RANDOM), 0x1F, "RANDOM on W+4");
}

/// Verifies REQ-514: Criterion 4: With `AUDCTL` = `$00`, the read of `RANDOM` on cycle C + 131071
/// is equal to the read on cycle C, for 131071 consecutive values of C. The reads on these cycles
/// are not all equal.
#[test]
fn random_17bit_period() {
    const N: usize = 0x0001_FFFF; // 131071
    let reads = random_reads(&mut random_pokey(0x00), 2 * N);
    assert!((0..N).all(|c| reads[c + N] == reads[c]), "period 131071");
    assert!(reads[..N].iter().any(|r| *r != reads[0]), "constant");
}

/// Verifies REQ-514: Criterion 5: After a write of `SKCTL` = `$00` on cycle I, each read of
/// `RANDOM` from cycle I+9 to cycle I+1000 returns `$FF`.
#[test]
fn random_ff_in_init_mode() {
    for audctl in [0x00, 0x80] {
        let mut p = random_pokey(audctl);
        assert_ne!(p.peek(RANDOM), 0xFF, "RANDOM before the write");
        p.write(SKCTL, 0x00);
        let reads = random_reads(&mut p, 1000);
        assert!(
            reads[8..].iter().all(|r| *r == 0xFF),
            "AUDCTL = {audctl:#04X}: {:02X?}",
            &reads[..16]
        );
    }
}

// ----------------------------------------------------------------------------------------
// Audio samples (REQ-538 to REQ-541)
// ----------------------------------------------------------------------------------------

/// The total number of samples after each of `frames` frames of `frame` cycles.
fn sample_counts(standard: VideoStandard, rate: u32, frame: u64, frames: u64) -> Vec<usize> {
    let mut p = Pokey::new(standard, rate);
    let mut samples = Vec::new();
    (0..frames)
        .map(|_| {
            run(&mut p, frame);
            p.drain_samples(&mut samples);
            samples.len()
        })
        .collect()
}

/// The integer part of C x rate / clock.
fn expected_count(cycles: u64, rate: u64, clock: u64) -> usize {
    usize::try_from(cycles * rate / clock).unwrap_or(usize::MAX)
}

/// Verifies REQ-538: Criterion 1: On NTSC at 48000 Hz, 60 frames (1792080 cycles) after
/// `Pokey::new` give exactly 48061 samples (the sum of the lengths that `drain_samples` adds).
#[test]
fn sample_count_ntsc() {
    let counts = sample_counts(VideoStandard::Ntsc, 48_000, FRAME_NTSC, 60);
    assert_eq!(counts.last(), Some(&48_061), "samples after 60 frames");
    for (frame, count) in (1..).zip(&counts) {
        let expected = expected_count(frame * FRAME_NTSC, 48_000, CLOCK_NTSC);
        assert_eq!(*count, expected, "samples after frame {frame}");
    }
}

/// Verifies REQ-538: Criterion 2: On PAL at 48000 Hz, 50 frames (1778400 cycles) give exactly 48134
/// samples.
#[test]
fn sample_count_pal() {
    let counts = sample_counts(VideoStandard::Pal, 48_000, FRAME_PAL, 50);
    assert_eq!(counts.last(), Some(&48_134), "samples after 50 frames");
}

/// The input of REQ-538 criterion 4: `AUDCTL` = `$00`, `AUDF1` = `$48`, `AUDC1` = `$AF`, and
/// the speaker level changing every 895 cycles. `volume_steps` also changes `AUDC1-4` between
/// `$1F` and `$10` every 1000 cycles (criterion 3). `each` is called after each frame.
fn tone_and_speaker(frames: u64, volume_steps: bool, mut each: impl FnMut(&mut Pokey)) {
    let mut p = new_pokey();
    p.tick();
    p.write(SKCTL, 0x03);
    p.write(AUDCTL, 0x00);
    p.write(AUDF1, 0x48);
    p.write(AUDC1, 0xAF);
    let mut speaker = false;
    for cycle in 1..=frames * FRAME_NTSC {
        if cycle % 895 == 0 {
            speaker = !speaker;
            p.set_speaker(speaker);
        }
        if volume_steps && cycle % 1000 == 0 {
            let audc = if (cycle / 1000) % 2 == 0 { 0x1F } else { 0x10 };
            write_channels(&mut p, [0x48, 0, 0, 0], [audc; 4]);
        }
        p.tick();
        if cycle % FRAME_NTSC == 0 {
            each(&mut p);
        }
    }
}

/// Verifies REQ-538: Criterion 3: With the input of criterion 4, and with `AUDC1-4` changing
/// between `$1F` and `$10` every 1000 cycles, each sample is in the range -1.0 to 1.0.
#[test]
fn samples_in_range() {
    let mut samples = Vec::new();
    tone_and_speaker(10, true, |p| p.drain_samples(&mut samples));
    assert!(samples.len() > 8000, "{} samples", samples.len());
    assert!(
        samples.iter().all(|s| (-1.0..=1.0).contains(s)),
        "a sample out of range"
    );
    assert!(peak_to_peak(&samples) > 1.0, "the input is loud");
}

/// Verifies REQ-538: Criterion 4: With `AUDCTL` = `$00`, `AUDF1` = `$48`, `AUDC1` = `$AF` and the
/// speaker level changing every 895 cycles, the samples of one `drain_samples` call after 10 frames
/// are bit-identical to the samples of 10 calls, one after each frame.
#[test]
fn drain_split_invariant() {
    let mut once = Vec::new();
    let mut frames = 0;
    tone_and_speaker(10, false, |p| {
        frames += 1;
        if frames == 10 {
            p.drain_samples(&mut once);
        }
    });
    let mut split = Vec::new();
    let mut calls = 0;
    tone_and_speaker(10, false, |p| {
        calls += 1;
        p.drain_samples(&mut split);
    });
    assert_eq!(calls, 10, "drain calls");
    assert!(once.len() > 8000, "{} samples", once.len());
    assert!(
        once.iter()
            .map(|s| s.to_bits())
            .eq(split.iter().map(|s| s.to_bits())),
        "the samples differ"
    );
}

/// Verifies REQ-538: Criterion 5: On NTSC, after 1792080 cycles, the number of samples is 8010 at
/// 8000 Hz, 44156 at 44100 Hz, 96123 at 96000 Hz and 192247 at 192000 Hz.
#[test]
fn sample_count_other_rates() {
    for (rate, expected) in [
        (8_000, 8_010),
        (44_100, 44_156),
        (96_000, 96_123),
        (192_000, 192_247),
    ] {
        let counts = sample_counts(VideoStandard::Ntsc, rate, FRAME_NTSC, 60);
        assert_eq!(counts.last(), Some(&expected), "{rate} Hz");
    }
}

/// Verifies REQ-539: Criterion 4: `AUDCTL` = `$20`, `AUDF3` = `$00` and `AUDC3` = `$AF` (a 224 kHz
/// tone) give a peak-to-peak of less than 0.03.
#[test]
fn ultrasonic_tone_averages() {
    let (mut p, _, _) = started(0x20, [0; 4], [0, 0, 0xAF, 0]);
    let samples = run_samples(&mut p, 1_100 * CLOCK_NTSC / 1000);
    let window = &samples[48_000..52_800];
    let p2p = peak_to_peak(window);
    assert!(p2p < 0.03, "peak-to-peak {p2p}");
}

/// The input of REQ-539 criterion 5 with the nonlinear option `nonlinear` (`None`: the
/// option is not set): `AUDC3` = `$10` for 1 s, then `AUDC3` = `$1F`. Returns the samples
/// and the step: the first sample that starts after the write minus the last sample
/// before the write.
fn polarity_step(nonlinear: Option<bool>) -> (Vec<f32>, f32) {
    let mut p = new_pokey();
    if let Some(on) = nonlinear {
        p.set_nonlinear_audio(on);
    }
    p.tick();
    p.write(SKCTL, 0x03);
    p.write(AUDC3, 0x10);
    let mut samples = run_samples(&mut p, CLOCK_NTSC);
    run_to_sample_end(&mut p, &mut samples);
    p.write(AUDC3, 0x1F);
    let before = samples.len() - 1;
    // The sample after the write starts in the cycle of the write unless that sample began
    // exactly at the end of the cycle.
    let exact = (p.cycle() * 48_000).is_multiple_of(CLOCK_NTSC);
    let first = if exact { before + 1 } else { before + 2 };
    samples.extend(run_samples(&mut p, 10_000));
    let step = samples[first] - samples[before];
    (samples, step)
}

/// Verifies REQ-539: Criterion 5: With `AUDC3` = `$10` for 1 s, a write of `AUDC3` = `$1F` makes
/// the first sample that starts after the write larger than the last sample before the write, by
/// 0.20 to 0.26.
#[test]
fn level_polarity() {
    let (_, step) = polarity_step(None);
    assert!((0.20..=0.26).contains(&step), "step {step}");
}

/// The setup of REQ-540: 1 s with `prepare`, then a step with `step` on the first cycle of
/// sample 0. Returns the samples from sample 0 for `milliseconds` ms.
fn step_response(
    prepare: impl Fn(&mut Pokey),
    step: impl Fn(&mut Pokey),
    milliseconds: u64,
) -> Vec<f32> {
    let mut p = new_pokey();
    p.tick();
    p.write(SKCTL, 0x03);
    prepare(&mut p);
    let mut samples = run_samples(&mut p, CLOCK_NTSC);
    run_to_sample_end(&mut p, &mut samples);
    step(&mut p);
    run_samples(&mut p, milliseconds * CLOCK_NTSC / 1000)
}

/// The largest sample of the first 480 samples (10 ms).
fn peak(samples: &[f32]) -> f32 {
    samples[..480].iter().copied().fold(f32::MIN, f32::max)
}

/// Verifies REQ-540: Criterion 1: With `AUDC1` = `$10` for 1 s, a write of `AUDC1` = `$1F` makes
/// sample 125 (2.6 ms after the step) 25 % to 40 % of the peak.
#[test]
fn pokey_step_response() {
    let samples = step_response(|p| p.write(AUDC1, 0x10), |p| p.write(AUDC1, 0x1F), 20);
    let ratio = samples[125] / peak(&samples);
    assert!((0.25..=0.40).contains(&ratio), "ratio {ratio}");
}

/// Verifies REQ-540: Criterion 2: With the speaker level 0 for 1 s, a change of the speaker level
/// to 1 makes sample 1186 (24.7 ms after the step) 33 % to 40 % of the peak.
#[test]
fn speaker_step_response() {
    let samples = step_response(|p| p.set_speaker(false), |p| p.set_speaker(true), 40);
    let ratio = samples[1186] / peak(&samples);
    assert!((0.33..=0.40).contains(&ratio), "ratio {ratio}");
}

/// Verifies REQ-540: Criterion 3: After the step of criterion 1, each sample from 0.5 s to 1.0 s
/// after the step has a magnitude below 0.001.
#[test]
fn dc_decays() {
    let samples = step_response(|p| p.write(AUDC1, 0x10), |p| p.write(AUDC1, 0x1F), 1000);
    let late = &samples[24_000..48_000];
    let max = late.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(max < 0.001, "largest magnitude {max}");
    assert!(peak(&samples) > 0.2, "the step");
}

/// Verifies REQ-541: Criterion 1: With the option on, `AUDC1` = `$1F` and `AUDC2-4` = `$00`, the
/// normalized DAC sum (test accessor) is 0.25 +/- 0.0001. With `AUDC1` = `$15`, it is (0.12 + 0.56)
/// / 8.24 = 0.0825 +/- 0.0001.
#[test]
fn nonlinear_dac_sum() {
    let mut p = new_pokey();
    p.set_nonlinear_audio(true);
    p.tick();
    p.write(AUDC1, 0x1F);
    p.tick();
    assert!(
        (p.test_dac_sum() - 0.25).abs() <= 0.0001,
        "{}",
        p.test_dac_sum()
    );
    p.write(AUDC1, 0x15);
    p.tick();
    assert!(
        (p.test_dac_sum() - 0.0825).abs() <= 0.0001,
        "{}",
        p.test_dac_sum()
    );
}

/// Verifies REQ-541: Criterion 2: The curve (test accessor, input x) gives these outputs, each +/-
/// 0.001: x = 0 gives 0, x = 0.07 gives 0.152, x = 0.14 gives 0.304, x = 0.25 gives 0.509, x = 0.5
/// gives 0.793, x = 1.0 gives 1.000. These values come from `y = 2.171 * x` for `x <= 0.14`, and `y
/// = 2.171 * (0.14 + (1 - exp(-2.85 * (x - 0.14))) / 2.85)` for `x >= 0.14`.
#[test]
fn nonlinear_curve() {
    for (x, y) in [
        (0.0, 0.0),
        (0.07, 0.152),
        (0.14, 0.304),
        (0.25, 0.509),
        (0.5, 0.793),
        (1.0, 1.000),
    ] {
        let out = Pokey::test_curve(x);
        assert!((out - y).abs() <= 0.001, "curve({x}) = {out}, expected {y}");
    }
}

/// Verifies REQ-541.
///
/// Criterion 3: With the option on, the sample step of REQ-539 criterion 5 is 2.036 times the step
/// with the option off, +/- 2 % (0.509 / 0.25). The test compares the sample step with the option
/// on and off.
#[test]
fn nonlinear_step_gain() {
    let (_, off) = polarity_step(Some(false));
    let (_, on) = polarity_step(Some(true));
    let gain = on / off;
    assert!(
        (gain - 2.036).abs() <= 0.02 * 2.036,
        "gain {gain} ({on} / {off})"
    );
}

/// Verifies REQ-541.
///
/// Criterion 4: After `Pokey::new`, the input of REQ-539 criterion 5 gives the same samples as
/// after `set_nonlinear_audio(false)`.
#[test]
fn nonlinear_off_at_power_on() {
    let (default, _) = polarity_step(None);
    let (off, _) = polarity_step(Some(false));
    let (on, _) = polarity_step(Some(true));
    let bits = |s: &[f32]| s.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    assert!(bits(&default) == bits(&off), "the samples differ");
    assert!(
        bits(&default) != bits(&on),
        "the option changes the samples"
    );
}
