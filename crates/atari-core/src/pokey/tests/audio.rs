//! Planned tests of the audio channels, the noise generators, `RANDOM` and the samples
//! (REQ-509 to REQ-514, REQ-538 to REQ-541).
//!
//! Each test is a stub until its package writes it (`docs/design/pokey.md`, "Test plan").

use crate::planned::planned;

/// Verifies REQ-509: Criterion 1: With `AUDCTL` = `$20` and `AUDC3` = `$AF`, the output changes on
/// each fire of timer 3 and on no other cycle.
#[test]
#[ignore = "planned: SLICE-03"]
fn distortion_pure_tone() {
    planned();
}

/// Verifies REQ-509: Criterion 2: With `AUDCTL` = `$20` and `AUDC3` = `$CF`, the outputs after 15
/// consecutive fires contain 7 ones and 8 zeros. The sequence of outputs after the fires repeats
/// with a period of 15 fires.
#[test]
#[ignore = "planned: SLICE-03"]
fn distortion_poly4() {
    planned();
}

/// Verifies REQ-509: Criterion 3: With `AUDCTL` = `$A0` and `AUDC3` = `$8F`, the sequence of
/// outputs after the fires repeats with a period of 511 fires and has 255 ones in each period.
#[test]
#[ignore = "planned: SLICE-03"]
fn distortion_poly9() {
    planned();
}

/// Verifies REQ-509: Criterion 4: With `AUDCTL` = `$20` and `AUDC3` = `$8F`, the sequence of
/// outputs after the fires repeats with a period of 131071 fires and has 65535 ones in each period.
/// The test runs channel 3 for 131071 fires in 17-bit mode and counts the ones.
#[test]
#[ignore = "planned: SLICE-03"]
fn distortion_poly17() {
    planned();
}

/// Verifies REQ-509: Criterion 5: With `AUDCTL` = `$20` and `AUDC3` = `$2F`, the output changes on
/// exactly 15 of each 31 consecutive fires. The pattern of changes repeats with a period of 31
/// fires.
#[test]
#[ignore = "planned: SLICE-03"]
fn distortion_poly5_mask() {
    planned();
}

/// Verifies REQ-509: Criterion 6: With `AUDCTL` = `$20` and `AUDC3` = `$0F` or `$4F`, the output
/// changes only on fires on which the output changes in criterion 5 (the fires that the 5-bit
/// generator lets through). The test runs criterion 5 and criterion 6 from the same `SKCTL` write
/// cycle and the same `STIMER` write cycle. The test runs `AUDC3` = `$2F`, `$0F` and `$4F` from the
/// same start cycles and compares the fires that change the output.
#[test]
#[ignore = "planned: SLICE-03"]
fn distortion_poly5_masks_noise() {
    planned();
}

/// Verifies REQ-509: Criterion 7: With `AUDCTL` = `$28`, `AUDF3` = `$28`, `AUDF4` = `$00`, `SKCTL`
/// = `$23` and `IRQEN` = `$04`, the cycles on which `IRQST` bit 2 goes to 0 and the cycles on which
/// SIO CLOCK OUT changes are the same for each of the 8 values of `AUDC4` bits 7-5.
#[test]
#[ignore = "planned: SLICE-03"]
fn distortion_ignored_by_irq_and_serial() {
    planned();
}

/// Verifies REQ-510: Criterion 1: With `AUDC1` = `$1F` and `AUDC2-4` = `$00`, the mix level is 15
/// on each cycle for 10000 cycles. This is true with `AUDCTL` = `$00` and with `AUDCTL` = `$40`,
/// with `AUDF1` = `$00`.
#[test]
#[ignore = "planned: SLICE-03"]
fn volume_only_level() {
    planned();
}

/// Verifies REQ-510: Criterion 2: With `AUDC1` = `$10` and `AUDC2-4` = `$00`, the mix level is 0 on
/// each cycle for 10000 cycles.
#[test]
#[ignore = "planned: SLICE-03"]
fn volume_only_zero() {
    planned();
}

/// Verifies REQ-510: Criterion 3: With `AUDC1` = `$18`, `AUDC3` = `$18` and `AUDC2` = `AUDC4` =
/// `$00`, the mix level is 16 on each cycle for 1000 cycles after an `STIMER` write, for each of
/// the 256 `AUDCTL` values.
#[test]
#[ignore = "planned: SLICE-03"]
fn volume_only_channels_add() {
    planned();
}

/// Verifies REQ-510: Criterion 4: With `AUDCTL` = `$01`, `AUDF1` = `AUDF3` = `$FF`, `AUDC1` =
/// `$AF`, `AUDC3` = `$A7`, `AUDC2` = `AUDC4` = `$00` and an `STIMER` write on cycle T, the mix
/// level is 7 on each cycle from T+1 to T+1000. Channel 1 has output 0 and adds nothing. Channel 3
/// has output 1 and adds 7. The test checks the mix level 7 of REQ-510 criterion 4 for 1000 cycles.
#[test]
#[ignore = "planned: SLICE-03"]
fn output_zero_adds_nothing() {
    planned();
}

/// Verifies REQ-510: Criterion 5: With `AUDCTL` = `$20`, `AUDF3` = `$00`, `AUDC3` = `$BF` and an
/// `STIMER` write on cycle T, timer 3 fires on cycles T+8 and T+12. The channel 3 flip-flop (test
/// accessor) changes on each fire, and the digital output stays 1. A write of `AUDC3` = `$AF` on
/// cycle T+10 makes the digital output 0 on cycle T+11 and 1 on cycle T+12.
#[test]
#[ignore = "planned: SLICE-03"]
fn volume_only_keeps_flip_flop_running() {
    planned();
}

/// Verifies REQ-510: Criterion 6: With `AUDC1-4` = `$1F`, the mix level is 60. The test sets
/// `AUDC1-4` = `$1F` and checks the mix level.
#[test]
#[ignore = "planned: SLICE-03"]
fn mix_level_maximum_60() {
    planned();
}

/// Verifies REQ-511: Criterion 1: With `AUDCTL` = `$00`, `AUDF1` = `AUDF3` = `$10`, `AUDC1` =
/// `AUDC3` = `$A8`, `AUDC2` = `AUDC4` = `$00` and an `STIMER` write, the mix level is 8 on each
/// cycle for 10000 cycles.
#[test]
#[ignore = "planned: SLICE-03"]
fn ch1_ch3_cancel_without_high_pass() {
    planned();
}

/// Verifies REQ-511: Criterion 2: With `AUDCTL` = `$00`, `AUDF1` = `AUDF2` = `$10`, `AUDC1` =
/// `AUDC2` = `$A8`, `AUDC3` = `AUDC4` = `$00` and an `STIMER` write, the mix level is 0 or 16 on
/// each cycle, and it changes between 0 and 16 every 476 cycles (17 x 28).
#[test]
#[ignore = "planned: SLICE-03"]
fn ch1_ch2_add_without_high_pass() {
    planned();
}

/// Verifies REQ-511: Criterion 3: On NTSC, `AUDCTL` = `$64`, `AUDF1` = `$3B`, `AUDF3` = `$3C`,
/// `AUDC1` = `$AF` and `AUDC3` = `$A0` make the channel 1 digital output repeat with a period of
/// 4032 cycles (1789773 / 4032 = 443.9 Hz). At 48000 Hz, the measured frequency (REQ-503 Notes) of
/// the audio output is 443.9 Hz +/- 1 Hz. The test checks that the channel 1 digital output repeats
/// every 4032 cycles.
#[test]
#[ignore = "planned: SLICE-03"]
fn high_pass_period_4032() {
    planned();
}

/// Verifies REQ-511: Criterion 4: With `AUDCTL` = `$2A`, `AUDF2` = `$00`, `AUDF3` = `AUDF4` = `$00`
/// and `AUDC2` = `$AF`, the channel 2 digital output changes to 1 on each fire of timer 2 and
/// changes to 0 on the next fire of timer 4. The test selects an `STIMER` write cycle on which no
/// fire of timer 2 is on the same cycle as a fire of timer 4. The test checks the changes of the
/// channel 2 digital output against the fire log of timers 2 and 4.
#[test]
#[ignore = "planned: SLICE-03"]
fn high_pass_channel_2() {
    planned();
}

/// Verifies REQ-511: Criterion 5: With the setup of criterion 3, the channel 1 digital output is
/// the same on each cycle for `AUDC3` = `$A0`, `$00`, `$1F`, `$8F` and `$C5`.
#[test]
#[ignore = "planned: SLICE-03"]
fn high_pass_ignores_audc3() {
    planned();
}

/// Verifies REQ-512: Criterion 1: With `AUDCTL` = `$00`, `AUDF1` = `AUDF2` = `$00`, `AUDC1` =
/// `AUDC2` = `$CF` and an `STIMER` write, timers 1 and 2 fire on the same cycles. When channel 1
/// gets bit i of the 4-bit pattern of REQ-513, channel 2 gets bit i - 1 (modulo 15) on the same
/// fire.
#[test]
#[ignore = "planned: SLICE-03"]
fn noise_delay_ch1_ch2() {
    planned();
}

/// Verifies REQ-512: Criterion 2: With `AUDCTL` = `$60`, `AUDF1` = `AUDF3` = `$00`, `AUDC1` =
/// `AUDC3` = `$CF` and an `STIMER` write, when channel 1 gets bit i of the 4-bit pattern, channel 3
/// gets bit i - 2 (modulo 15) on the same fire. The test compares the bits that channels 1 and 3
/// take on the same fires.
#[test]
#[ignore = "planned: SLICE-03"]
fn noise_delay_ch1_ch3() {
    planned();
}

/// Verifies REQ-512: Criterion 3: With `AUDCTL` = `$80`, `AUDF1` = `AUDF4` = `$00`, `AUDC1` =
/// `AUDC4` = `$8F` and an `STIMER` write, when channel 1 gets bit i of the 9-bit pattern, channel 4
/// gets bit i - 3 (modulo 511) on the same fire. The 9-bit pattern is the inverse of `RANDOM` bit 0
/// on consecutive cycles.
#[test]
#[ignore = "planned: SLICE-03"]
fn noise_delay_ch1_ch4() {
    planned();
}

/// Verifies REQ-512: Criterion 4: With the setup of criterion 3, the bit that channel 1 gets on a
/// fire on cycle C is the inverse of `RANDOM` bit 0 on cycle C - 1. The test reads `RANDOM` on each
/// cycle and compares bit 0 with the bit that channel 1 takes one cycle later.
#[test]
#[ignore = "planned: SLICE-03"]
fn noise_follows_random_bit_0() {
    planned();
}

/// Verifies REQ-513: Criterion 1: The 4-bit noise bits of channel 1 on cycles W+d, W+d+1, W+d+2,
/// ... are `000011101100101`, and then the sequence repeats with a period of 15 cycles. d is 0 or
/// 1.
#[test]
#[ignore = "planned: SLICE-03"]
fn poly4_pattern() {
    planned();
}

/// Verifies REQ-513: Criterion 2: The 5-bit noise bits of channel 1 on cycles W+d, W+d+1, ... are
/// `0000011100100010101111011010011`, and then the sequence repeats with a period of 31 cycles. d
/// is 0 or 1.
#[test]
#[ignore = "planned: SLICE-03"]
fn poly5_pattern() {
    planned();
}

/// Verifies REQ-513: Criterion 3: From 9 cycles after a write of `SKCTL` = `$00` until the next
/// write that ends initialization mode, the 4-bit noise bit of each channel is 0 and the 5-bit
/// noise bit of each channel is 1. With `AUDCTL` = `$40`, `AUDF1` = `$00` and `AUDC1` = `$CF`, the
/// channel 1 flip-flop is 0 after each fire. With `AUDC1` = `$2F`, the channel 1 flip-flop changes
/// on each fire.
#[test]
#[ignore = "planned: SLICE-03"]
fn poly_outputs_in_init_mode() {
    planned();
}

/// Verifies REQ-514: Criterion 1: With `AUDCTL` = `$80`, the reads of `RANDOM` on 511 consecutive
/// cycles are a rotation of the 511 entries of the HRM 14.4 table (entry 0 = `$00`, entry 1 =
/// `$80`, entry 2 = `$40`, entry 3 = `$20`, entry 4 = `$10`, entry 5 = `$88`, ...). The read on
/// cycle C + 511 is equal to the read on cycle C.
#[test]
#[ignore = "planned: SLICE-03"]
fn random_9bit_table() {
    planned();
}

/// Verifies REQ-514: Criterion 2: With `AUDCTL` = `$80`, there is a cycle C in each 511 consecutive
/// cycles on which the reads of `RANDOM` on cycles C, C+114, C+228, C+342 and C+456 are `$00`,
/// `$DF`, `$EE`, `$16` and `$B9`.
#[test]
#[ignore = "planned: SLICE-03"]
fn random_9bit_every_scanline() {
    planned();
}

/// Verifies REQ-514: Criterion 3: With `AUDCTL` = `$80`, after at least 18 cycles of initialization
/// mode, a read of `RANDOM` 4 cycles after the `SKCTL` = `$03` write returns `$1F`.
#[test]
#[ignore = "planned: SLICE-03"]
fn random_after_init_exit_1f() {
    planned();
}

/// Verifies REQ-514: Criterion 4: With `AUDCTL` = `$00`, the read of `RANDOM` on cycle C + 131071
/// is equal to the read on cycle C, for 131071 consecutive values of C. The reads on these cycles
/// are not all equal.
#[test]
#[ignore = "planned: SLICE-03"]
fn random_17bit_period() {
    planned();
}

/// Verifies REQ-514: Criterion 5: After a write of `SKCTL` = `$00` on cycle I, each read of
/// `RANDOM` from cycle I+9 to cycle I+1000 returns `$FF`.
#[test]
#[ignore = "planned: SLICE-03"]
fn random_ff_in_init_mode() {
    planned();
}

/// Verifies REQ-538: Criterion 1: On NTSC at 48000 Hz, 60 frames (1792080 cycles) after
/// `Pokey::new` give exactly 48061 samples (the sum of the lengths that `drain_samples` adds).
#[test]
#[ignore = "planned: SLICE-03"]
fn sample_count_ntsc() {
    planned();
}

/// Verifies REQ-538: Criterion 2: On PAL at 48000 Hz, 50 frames (1778400 cycles) give exactly 48134
/// samples.
#[test]
#[ignore = "planned: SLICE-03"]
fn sample_count_pal() {
    planned();
}

/// Verifies REQ-538: Criterion 3: With the input of criterion 4, and with `AUDC1-4` changing
/// between `$1F` and `$10` every 1000 cycles, each sample is in the range -1.0 to 1.0.
#[test]
#[ignore = "planned: SLICE-03"]
fn samples_in_range() {
    planned();
}

/// Verifies REQ-538: Criterion 4: With `AUDCTL` = `$00`, `AUDF1` = `$48`, `AUDC1` = `$AF` and the
/// speaker level changing every 895 cycles, the samples of one `drain_samples` call after 10 frames
/// are bit-identical to the samples of 10 calls, one after each frame.
#[test]
#[ignore = "planned: SLICE-03"]
fn drain_split_invariant() {
    planned();
}

/// Verifies REQ-538: Criterion 5: On NTSC, after 1792080 cycles, the number of samples is 8010 at
/// 8000 Hz, 44156 at 44100 Hz, 96123 at 96000 Hz and 192247 at 192000 Hz.
#[test]
#[ignore = "planned: SLICE-03"]
fn sample_count_other_rates() {
    planned();
}

/// Verifies REQ-539: Criterion 4: `AUDCTL` = `$20`, `AUDF3` = `$00` and `AUDC3` = `$AF` (a 224 kHz
/// tone) give a peak-to-peak of less than 0.03.
#[test]
#[ignore = "planned: SLICE-03"]
fn ultrasonic_tone_averages() {
    planned();
}

/// Verifies REQ-539: Criterion 5: With `AUDC3` = `$10` for 1 s, a write of `AUDC3` = `$1F` makes
/// the first sample that starts after the write larger than the last sample before the write, by
/// 0.20 to 0.26.
#[test]
#[ignore = "planned: SLICE-03"]
fn level_polarity() {
    planned();
}

/// Verifies REQ-540: Criterion 1: With `AUDC1` = `$10` for 1 s, a write of `AUDC1` = `$1F` makes
/// sample 125 (2.6 ms after the step) 25 % to 40 % of the peak.
#[test]
#[ignore = "planned: SLICE-03"]
fn pokey_step_response() {
    planned();
}

/// Verifies REQ-540: Criterion 2: With the speaker level 0 for 1 s, a change of the speaker level
/// to 1 makes sample 1186 (24.7 ms after the step) 33 % to 40 % of the peak.
#[test]
#[ignore = "planned: SLICE-03"]
fn speaker_step_response() {
    planned();
}

/// Verifies REQ-540: Criterion 3: After the step of criterion 1, each sample from 0.5 s to 1.0 s
/// after the step has a magnitude below 0.001.
#[test]
#[ignore = "planned: SLICE-03"]
fn dc_decays() {
    planned();
}

/// Verifies REQ-541: Criterion 1: With the option on, `AUDC1` = `$1F` and `AUDC2-4` = `$00`, the
/// normalized DAC sum (test accessor) is 0.25 +/- 0.0001. With `AUDC1` = `$15`, it is (0.12 + 0.56)
/// / 8.24 = 0.0825 +/- 0.0001.
#[test]
#[ignore = "planned: SLICE-03"]
fn nonlinear_dac_sum() {
    planned();
}

/// Verifies REQ-541: Criterion 2: The curve (test accessor, input x) gives these outputs, each +/-
/// 0.001: x = 0 gives 0, x = 0.07 gives 0.152, x = 0.14 gives 0.304, x = 0.25 gives 0.509, x = 0.5
/// gives 0.793, x = 1.0 gives 1.000. These values come from `y = 2.171 * x` for `x <= 0.14`, and `y
/// = 2.171 * (0.14 + (1 - exp(-2.85 * (x - 0.14))) / 2.85)` for `x >= 0.14`.
#[test]
#[ignore = "planned: SLICE-03"]
fn nonlinear_curve() {
    planned();
}

/// Verifies REQ-541.
///
/// Criterion 3: With the option on, the sample step of REQ-539 criterion 5 is 2.036 times the step
/// with the option off, +/- 2 % (0.509 / 0.25). The test compares the sample step with the option
/// on and off.
#[test]
#[ignore = "planned: SLICE-03"]
fn nonlinear_step_gain() {
    planned();
}

/// Verifies REQ-541.
///
/// Criterion 4: After `Pokey::new`, the input of REQ-539 criterion 5 gives the same samples as
/// after `set_nonlinear_audio(false)`.
#[test]
#[ignore = "planned: SLICE-03"]
fn nonlinear_off_at_power_on() {
    planned();
}
