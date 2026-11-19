//! Planned tests of initialization mode and the timers (REQ-502 to REQ-508).
//!
//! Each test is a stub until its package writes it (`docs/design/pokey.md`, "Test plan").

use crate::planned::planned;

/// Verifies REQ-502: Criterion 1: With `SKCTL` = `$00`, `IRQEN` = `$01`, `AUDF1` = `$00` and
/// `AUDCTL` = `$00` (and again with `AUDCTL` = `$01`), and an `STIMER` write on cycle T, `IRQST`
/// bit 0 reads 1 and the timer 1 counter (test accessor) does not change from cycle T+4 to cycle
/// T+10000.
#[test]
#[ignore = "planned: SLICE-05"]
fn init_mode_stops_slow_clocks() {
    planned();
}

/// Verifies REQ-502: Criterion 2: With `SKCTL` = `$00`, `AUDCTL` = `$40`, `AUDF1` = `$00`, `IRQEN`
/// = `$01` and an `STIMER` write on cycle T, `IRQST` bit 0 reads 0 from cycle T+8.
#[test]
#[ignore = "planned: SLICE-05"]
fn init_mode_keeps_fast_timers() {
    planned();
}

/// Verifies REQ-502: Criterion 3: Start state: `SKCTL` = `$03`, `AUDCTL` = `$01`, `AUDF1-4` =
/// `$11`, `$22`, `$33`, `$44`, `AUDC1-4` = `$A1`, `$A2`, `$A3`, `$A4`, `IRQEN` = `$01` with `IRQST`
/// bit 0 latched, `KBCODE` = `$3F` (A key registered) and `SERIN` = `$43` (one received frame). A
/// write of `SKCTL` = `$00` on cycle I does not change `IRQEN`, `IRQST`, `KBCODE`, `SERIN`,
/// `AUDF1-4`, `AUDC1-4`, `AUDCTL`, the four timer counters or the four channel output flip-flops.
/// The test compares the values on cycle I-1 with the values on cycles I+1 and I+1000.
#[test]
#[ignore = "planned: SLICE-05"]
fn init_mode_keeps_registers() {
    planned();
}

/// Verifies REQ-502: Criterion 5: With `SKCTL` = `$03`, a `POTGO` write and no paddle on input 0,
/// when `POT0` reads 10, a write of `SKCTL` = `$00` keeps `POT0` at 10 and `ALLPOT` bit 0 at 1 for
/// 10 x 114 cycles.
#[test]
#[ignore = "planned: SLICE-05"]
fn init_mode_stops_slow_pot_counter() {
    planned();
}

/// Verifies REQ-502: Criterion 6: After a write of `SKCTL` = `$00` on cycle I (from `SKCTL` =
/// `$03`), each read of `RANDOM` from cycle I+9 to cycle I+1000 returns `$FF`. This is true with
/// `AUDCTL` = `$00` and with `AUDCTL` = `$80`.
#[test]
#[ignore = "planned: SLICE-05"]
fn init_mode_random_ff_after_9_cycles() {
    planned();
}

/// Verifies REQ-502: Criterion 8: With `SKCTL` = `$20` (initialization mode, input clock channel
/// 4), `AUDCTL` = `$28`, `AUDF3` = `$28`, `AUDF4` = `$00` and `IRQEN` = `$20`, a frame for `$43`
/// with 94-cycle bits on SIO DATA IN does not change `SERIN`. `IRQST` bit 5 and `SKSTAT` bit 1 read
/// 1 during the frame and for 1000 cycles after it. The test sends a frame with `SKCTL` = `$20` and
/// checks that `SERIN`, `IRQST` bit 5 and `SKSTAT` bit 1 do not change.
#[test]
#[ignore = "planned: SLICE-05"]
fn init_mode_holds_serial_input() {
    planned();
}

/// Verifies REQ-503: Criterion 1: With `AUDCTL` = `$00` and `AUDF1` = `$00`, the fires of timer 1
/// are 28 cycles apart. With `AUDCTL` = `$01` and `AUDF1` = `$00`, they are 114 cycles apart.
#[test]
#[ignore = "planned: SLICE-03"]
fn slow_clock_pulse_spacing() {
    planned();
}

/// Verifies REQ-503: Criterion 2: With `AUDCTL` = `$60` and `AUDF1-4` = `$00`, the fires of timers
/// 1 and 3 are 4 cycles apart, and the fires of timers 2 and 4 are 28 cycles apart. With `AUDCTL` =
/// `$61`, the fires of timers 2 and 4 are 114 cycles apart.
#[test]
#[ignore = "planned: SLICE-03"]
fn timers_2_and_4_never_fast() {
    planned();
}

/// Verifies REQ-503: Criterion 3: With `AUDCTL` = `$40` and `AUDF3` = `$00`, the fires of timer 3
/// are 28 cycles apart. With `AUDCTL` = `$20` and `AUDF1` = `$00`, the fires of timer 1 are 28
/// cycles apart. The test checks the fire spacing of timers 1 and 3 with `AUDCTL` = `$40` and
/// `$20`.
#[test]
#[ignore = "planned: SLICE-03"]
fn fast_clock_bits_select_timer() {
    planned();
}

/// Verifies REQ-503: Criteria 4, 5: On NTSC, `AUDCTL` = `$00`, `AUDF1` = `$48` and `AUDC1` = `$AF`
/// make the channel 1 digital output a square wave with a period of 4088 cycles (2 x 73 x 28), with
/// 2044 cycles at each level. At 48000 Hz, the measured frequency (see Notes) of the audio output
/// is 437.8 Hz +/- 0.5 Hz. On PAL, the same register values give the same 4088-cycle period. The
/// measured frequency is 433.8 Hz +/- 0.5 Hz. The test checks the period of the channel 1 digital
/// output for NTSC and PAL.
#[test]
#[ignore = "planned: SLICE-03"]
fn square_wave_period_4088() {
    planned();
}

/// Verifies REQ-504: Criterion 1: With `AUDCTL` = `$40` and `AUDF1` = N, the fires of timer 1 are N
/// + 4 cycles apart, for N = 0, 1, 5 and 255.
#[test]
#[ignore = "planned: SLICE-03"]
fn unlinked_fast_period() {
    planned();
}

/// Verifies REQ-504: Criterion 2: With `AUDCTL` = `$00` and `AUDF1` = `$09`, the fires of timer 1
/// are 280 cycles apart.
#[test]
#[ignore = "planned: SLICE-03"]
fn unlinked_64khz_period() {
    planned();
}

/// Verifies REQ-504: Criterion 3: With `AUDCTL` = `$01` and `AUDF4` = `$00`, the fires of timer 4
/// are 114 cycles apart. With `AUDF4` = `$09`, they are 1140 cycles apart.
#[test]
#[ignore = "planned: SLICE-03"]
fn unlinked_15khz_period() {
    planned();
}

/// Verifies REQ-504: Criterion 4: With `AUDCTL` = `$00`, `AUDF1` = `$03`, `AUDF2` = `$01` and an
/// `STIMER` write, each fire of timer 1 is on the same cycle as a fire of timer 2.
#[test]
#[ignore = "planned: SLICE-03"]
fn slow_timers_share_pulse() {
    planned();
}

/// Verifies REQ-506: Criterion 1: With `AUDCTL` = `$40`, `AUDF1` = `$00`, `IRQEN` = `$01` and an
/// `STIMER` write on cycle T, `IRQST` bit 0 reads 1 on cycle T+7 and 0 on cycle T+8.
#[test]
#[ignore = "planned: SLICE-03"]
fn stimer_first_irq_t8() {
    planned();
}

/// Verifies REQ-506: Criterion 2: With the setup of criterion 1 and `AUDF1` = `$05`, `IRQST` bit 0
/// reads 1 on cycle T+12 and 0 on cycle T+13.
#[test]
#[ignore = "planned: SLICE-03"]
fn stimer_first_irq_t13() {
    planned();
}

/// Verifies REQ-506: Criterion 3: With the setup of criterion 1, a second `STIMER` write on cycle
/// T+4 makes `IRQST` bit 0 read 1 on cycles T+8 to T+11 and 0 on cycle T+12. A second `STIMER`
/// write on cycle T+5 makes `IRQST` bit 0 read 0 on cycle T+8.
#[test]
#[ignore = "planned: SLICE-03"]
fn stimer_preemption_window() {
    planned();
}

/// Verifies REQ-506: Criterion 4: With the setup of criterion 1, an `AUDF1` = `$05` write on cycle
/// T+2 makes `IRQST` bit 0 read 0 first on cycle T+13. An `AUDF1` = `$05` write on cycle T+3 makes
/// `IRQST` bit 0 read 0 first on cycle T+8, and the second fire of timer 1 is on cycle T+17.
#[test]
#[ignore = "planned: SLICE-03"]
fn audf_reload_deadline() {
    planned();
}

/// Verifies REQ-506: Criterion 5: With `AUDCTL` = `$01`, `AUDF1` = `$00`, `IRQEN` = `$01`, `SKCTL`
/// = `$00` for at least 114 cycles and an `STIMER` write while `SKCTL` = `$00`, a write of `SKCTL`
/// = `$03` on cycle W makes `IRQST` bit 0 read 1 on cycle W+82 and 0 on cycle W+83.
#[test]
#[ignore = "planned: SLICE-03"]
fn init_exit_irq_w83() {
    planned();
}

/// Verifies REQ-506: Criterion 6: With the setup of criterion 5 and `AUDCTL` = `$00`, `IRQST` bit 0
/// reads 1 on cycle W+23 and 0 on cycle W+24.
#[test]
#[ignore = "planned: SLICE-03"]
fn init_exit_irq_w24() {
    planned();
}

/// Verifies REQ-506: Criterion 7: With `AUDCTL` = `$50`, `AUDF1` = `$00`, `AUDF2` = `$00`, `IRQEN`
/// = `$02` and an `STIMER` write on cycle T, timer 1 fires on cycles T+8, T+15 and T+22, and timer
/// 2 fires on cycles T+11, T+18 and T+25. `IRQST` bit 1 reads 1 on cycle T+10 and 0 on cycle T+11.
#[test]
#[ignore = "planned: SLICE-03"]
fn linked_timer_irq_timing() {
    planned();
}

/// Verifies REQ-507: Criterion 1: With `AUDCTL` = `$00` and `AUDC1-4` = `$AF`, the digital outputs
/// on the cycle after an `STIMER` write are 0 for channels 1 and 2 and 1 for channels 3 and 4. The
/// test checks this for the flip-flop values 0, 0, 0, 0 and 1, 1, 1, 1 before the write. The test
/// sets these values with timer fires before the write.
#[test]
#[ignore = "planned: SLICE-03"]
fn stimer_sets_output_flip_flops() {
    planned();
}

/// Verifies REQ-507: Criteria 2, 3: With `AUDCTL` = `$60`, `AUDF1` = `$00` and `AUDF3` = `$05`,
/// after an `STIMER` write on cycle T, the next fire of timer 1 is on cycle T+8 and the next fire
/// of timer 3 is on cycle T+13. This is true for 10 different start values of the counters. With
/// `AUDCTL` = `$00` and `AUDF2` = `AUDF4` = `$03`, after an `STIMER` write on cycle T, the first
/// fires of timers 2 and 4 are on the same cycle, in the range T+92 to T+119. The test sets random
/// counter values, writes `STIMER`, and checks the next fire cycles of all four timers.
#[test]
#[ignore = "planned: SLICE-03"]
fn stimer_reloads_all_timers() {
    planned();
}

/// Verifies REQ-507: Criterion 4: With `IRQEN` = `$07`, an `STIMER` write does not change `IRQST`.
/// With the setup of criterion 2 and `AUDC1` = `$AF`, the channel 1 flip-flop changes only on the
/// fire cycles of timer 1. The `STIMER` write itself is not a fire.
#[test]
#[ignore = "planned: SLICE-03"]
fn stimer_does_not_fire() {
    planned();
}

/// Verifies REQ-507: Criterion 5: With `SKCTL` = `$03` written on cycle W, `AUDCTL` = `$00` and
/// `AUDF1` = `$00`, all fires of timer 1 are on cycles W + 24 + 28k (k = 0, 1, 2, ...). The test
/// checks this with one `STIMER` write on each of the cycles W+100 to W+127, one cycle in each run.
#[test]
#[ignore = "planned: SLICE-03"]
fn stimer_keeps_slow_clock_phase() {
    planned();
}

/// Verifies REQ-508: Criterion 1: With `AUDCTL` = `$00`, `AUDF1-4` = `$00`, `IRQEN` = `$07`, an
/// `STIMER` write while `SKCTL` = `$00`, and a write of `SKCTL` = `$03` on cycle W, `IRQST` reads
/// `$F7` on cycle W+23 and `$F0` on cycle W+24.
#[test]
#[ignore = "planned: SLICE-03"]
fn timer_irq_bits() {
    planned();
}

/// Verifies REQ-508: Criterion 2: With `AUDCTL` = `$20`, `AUDF3` = `$00`, `AUDF1` = `AUDF2` =
/// `AUDF4` = `$FF`, `IRQEN` = `$FF`, no key down, SIO DATA IN at 1 and an `STIMER` write on cycle
/// T, `IRQST` reads `$F7` on each cycle from T+1 to T+1000. Timer 3 fires every 4 cycles in this
/// time.
#[test]
#[ignore = "planned: SLICE-03"]
fn timer_3_has_no_irq() {
    planned();
}

/// Verifies REQ-508: Criterion 3: With `AUDCTL` = `$50`, `AUDF1` = `$40`, `AUDF2` = `$01` and
/// `IRQEN` = `$02`, timer 2 fires every 327 cycles. After each event, the test writes `IRQEN` =
/// `$00` and then `IRQEN` = `$02`. `IRQST` bit 1 goes to 0 exactly once in each 327-cycle period,
/// on the fire cycle of timer 2.
#[test]
#[ignore = "planned: SLICE-03"]
fn linked_timer_irq_once_per_period() {
    planned();
}

/// Verifies REQ-508: Criterion 4: With `AUDCTL` = `$00`, `AUDF1` = `$00` and `IRQEN` = `$00`, timer
/// 1 fires on cycles E and E+28. A write of `IRQEN` = `$01` on cycle E+1 makes `IRQST` bit 0 read 1
/// on cycles E+1 to E+27 and 0 on cycle E+28.
#[test]
#[ignore = "planned: SLICE-03"]
fn timer_irq_lost_when_disabled() {
    planned();
}

/// Verifies REQ-508.
///
/// Criterion 5: With the setup of REQ-506 criterion 1, `IRQST` bit 0 reads 0 first on cycle T+8 for
/// each of `AUDC1` = `$00`, `$0F`, `$1F`, `$2F`, `$8F`, `$AF` and `$CF`.
#[test]
#[ignore = "planned: SLICE-03"]
fn timer_irq_ignores_audc() {
    planned();
}
