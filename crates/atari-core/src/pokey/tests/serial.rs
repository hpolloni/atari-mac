//! Planned tests of the serial port and the two-tone mode (REQ-526 to REQ-533).
//!
//! Each test is a stub until its package writes it (`docs/design/pokey.md`, "Test plan").

use crate::planned::planned;

/// Verifies REQ-526: Criterion 1: A frame for `$43` makes `SERIN` read `$43` from the cycle on
/// which `IRQST` bit 5 goes to 0.
#[test]
#[ignore = "planned: SLICE-05"]
fn serial_input_frame_43() {
    planned();
}

/// Verifies REQ-526: Criterion 2: `IRQST` bit 5 reads 0 first on a cycle from S+885 to S+901 (19
/// timer periods = 893 cycles, +/- 8 cycles).
#[test]
#[ignore = "planned: SLICE-05"]
fn serial_input_irq_timing() {
    planned();
}

/// Verifies REQ-526: Criterion 3: `SKSTAT` bit 1 reads 1 on each cycle before S+39, 0 on each cycle
/// from S+55 to S+885, and 1 from cycle S+901.
#[test]
#[ignore = "planned: SLICE-05"]
fn serial_input_busy_bit() {
    planned();
}

/// Verifies REQ-526: Criterion 4: Two instances receive the frame of criterion 1. One instance also
/// reads `SERIN` on each cycle. The reads of all registers (with `peek`) of the two instances are
/// the same on each cycle.
#[test]
#[ignore = "planned: SLICE-05"]
fn serin_read_no_side_effect() {
    planned();
}

/// Verifies REQ-526: Criterion 5: The test sends the 256 bytes `$00` to `$FF` back-to-back: each
/// start bit begins directly after the stop bit of the byte before. On each cycle on which `IRQST`
/// bit 5 goes to 0, the test reads `SERIN` and writes `IRQEN` = `$00` and then `IRQEN` = `$20`. The
/// 256 values read are `$00` to `$FF` in order. At the end, `SKSTAT` bits 5 and 7 read 1.
#[test]
#[ignore = "planned: SLICE-05"]
fn serial_input_256_bytes() {
    planned();
}

/// Verifies REQ-526: Criterion 6: With `IRQEN` = `$00`, the frame of criterion 1 makes `SERIN` read
/// `$43`, and `IRQST` bit 5 reads 1 on each cycle. The test receives a frame with `IRQEN` = `$00`
/// and checks `SERIN` and `IRQST`.
#[test]
#[ignore = "planned: SLICE-05"]
fn serial_input_without_irq() {
    planned();
}

/// Verifies REQ-527: Criteria 1, 3: A byte written to `SEROUT` goes out on SIO DATA OUT with a bit
/// time of 94 cycles in modes `010` and `100`, 56 cycles in modes `110` and `111`, and 200 cycles
/// in modes `000` and `001`. A frame for `$43` makes `SERIN` read `$43` and `SKSTAT` bit 7 read 1
/// in these cases: in modes `000` and `100`, with a bit time of 200 cycles and bit changes on the
/// changes of SIO CLOCK IN from 0 to 1; in modes `010` and `110`, with a bit time of 94 cycles and
/// bit changes on the changes from 0 to 1 of the level that POKEY drives on SIO CLOCK IN; in modes
/// `001`, `011`, `101` and `111`, with a bit time of 94 cycles and any start cycle.
#[test]
#[ignore = "planned: SLICE-05"]
fn serial_clock_mode_table() {
    planned();
}

/// Verifies REQ-527: Criterion 2: In modes `011` and `101`, SIO CLOCK OUT does not change while SIO
/// DATA IN stays at 1 for 10000 cycles. While a frame is received, SIO CLOCK OUT changes on each
/// fire of timer 4. The test checks `serial_clock_out` in modes `011` and `101` with and without a
/// received frame.
#[test]
#[ignore = "planned: SLICE-05"]
fn async_output_clock_stops() {
    planned();
}

/// Verifies REQ-527: Criterion 4: In modes `010` and `110`, `serial_clock_in_drive()` returns a
/// level that changes on each fire of timer 4. In the other 6 modes, it returns `None`. The test
/// checks `serial_clock_in_drive` in all 8 modes.
#[test]
#[ignore = "planned: SLICE-05"]
fn bidirectional_clock_drive() {
    planned();
}

/// Verifies REQ-527: Criterion 5: In mode `000`, with SIO CLOCK IN held at one level for 10000
/// cycles after a `SEROUT` write, SIO DATA OUT and `IRQST` bit 4 do not change.
#[test]
#[ignore = "planned: SLICE-05"]
fn external_clock_shifts_on_clock_in() {
    planned();
}

/// Verifies REQ-527: Criterion 6: In mode `000`, SIO DATA OUT changes only on cycles on which SIO
/// CLOCK IN changes from 0 to 1. The input shift register takes the SIO DATA IN level of the cycle
/// on which SIO CLOCK IN changes from 1 to 0.
#[test]
#[ignore = "planned: SLICE-05"]
fn serial_clock_edges() {
    planned();
}

/// Verifies REQ-527: Criterion 7: In mode `010`, with `AUDF3` = `$CC` and `AUDF4` = `$05` (timer 4
/// fires every 1491 cycles), the bit time on SIO DATA OUT is 2982 cycles.
#[test]
#[ignore = "planned: SLICE-05"]
fn serial_bit_rate_half_fire_rate() {
    planned();
}

/// Verifies REQ-527: Criterion 8: For `AUDC3` = `AUDC4` = `$00`, `$1F`, `$8F` and `$AF`, criteria 1
/// and 7 give the same bit times.
#[test]
#[ignore = "planned: SLICE-05"]
fn serial_clock_ignores_audc() {
    planned();
}

/// Verifies REQ-528: Criterion 1: With `IRQEN` = `$04` and SIO DATA IN at 1 for 10000 cycles,
/// timers 3 and 4 do not fire, and `IRQST` bit 2 reads 1.
#[test]
#[ignore = "planned: SLICE-05"]
fn async_holds_timers_3_4() {
    planned();
}

/// Verifies REQ-528: Criterion 2: After a start edge on cycle S, the first fire of timer 4 is on a
/// cycle from S+39 to S+55. The next fires are 47 cycles apart. The test checks the first timer 4
/// fire after a start edge and the spacing of the next fires.
#[test]
#[ignore = "planned: SLICE-05"]
fn async_first_timer4_fire() {
    planned();
}

/// Verifies REQ-528: Criterion 3: For each received byte, timer 4 fires 19 times from cycle S to
/// the cycle on which `IRQST` bit 5 goes to 0 (with `IRQEN` = `$20`). Timer 4 does not fire after
/// that cycle until the next start edge.
#[test]
#[ignore = "planned: SLICE-05"]
fn async_19_timer4_fires_per_byte() {
    planned();
}

/// Verifies REQ-528: Criterion 4: Frames for `$43` and `$A5` with a bit time of 91 cycles, and
/// again with a bit time of 97 cycles, make `SERIN` read the sent value and `SKSTAT` bit 7 read 1.
#[test]
#[ignore = "planned: SLICE-05"]
fn async_tolerates_3_percent() {
    planned();
}

/// Verifies REQ-528: Criterion 5: With SIO DATA IN at 1 and `AUDC3` = `AUDC4` = `$AF`, the channel
/// 3 and channel 4 digital outputs do not change for 10000 cycles. With `AUDC3` = `$1F` and `AUDC1`
/// = `AUDC2` = `AUDC4` = `$00`, the mix level is 15.
#[test]
#[ignore = "planned: SLICE-05"]
fn async_silences_ch3_ch4() {
    planned();
}

/// Verifies REQ-529: Criterion 1: A frame for `$43` with a stop bit of 0 makes `SKSTAT` bit 7 read
/// 0, `SERIN` read `$43` and `IRQST` bit 5 read 0, all from the cycle on which `IRQST` bit 5 goes
/// to 0.
#[test]
#[ignore = "planned: SLICE-05"]
fn framing_error() {
    planned();
}

/// Verifies REQ-529: Criterion 2: Two frames, `$43` and then `$55`, with no write to `IRQEN`
/// between them, make `SKSTAT` bit 5 read 0 after the second frame. `SERIN` reads `$55`.
#[test]
#[ignore = "planned: SLICE-05"]
fn serial_overrun() {
    planned();
}

/// Verifies REQ-529: Criterion 3: Criterion 2 gives the same result when the test reads `SERIN`
/// between the two frames.
#[test]
#[ignore = "planned: SLICE-05"]
fn serin_read_does_not_prevent_overrun() {
    planned();
}

/// Verifies REQ-529: Criterion 4: With `IRQEN` = `$00`, the two frames of criterion 2 keep `SKSTAT`
/// bit 5 at 1.
#[test]
#[ignore = "planned: SLICE-05"]
fn no_overrun_when_disabled() {
    planned();
}

/// Verifies REQ-529: Criterion 5: After criteria 1 and 2, the test clears `IRQST` bit 5 and sends a
/// good frame. `SKSTAT` bits 5 and 7 still read 0. A write to `SKRES` on cycle X makes `SKSTAT`
/// bits 5 and 7 read 1 from cycle X+1.
#[test]
#[ignore = "planned: SLICE-05"]
fn skres_clears_serial_errors() {
    planned();
}

/// Verifies REQ-530: Criterion 1: For each of the 256 `SKCTL` values, a call of
/// `set_serial_in(false)` before the tick of cycle X makes `SKSTAT` bit 4 read 0 on cycle X. A call
/// of `set_serial_in(true)` before the tick of cycle Y makes `SKSTAT` bit 4 read 1 on cycle Y.
#[test]
#[ignore = "planned: SLICE-05"]
fn skstat_direct_serial_in() {
    planned();
}

/// Verifies REQ-530: Criterion 2: `SKSTAT` bit 0 reads 1 for each of the 256 `SKCTL` values, with
/// SIO DATA IN at 0 and at 1, and with the A key and Shift down. It also reads 1 on each cycle of
/// the frame receive of REQ-526 criterion 1.
#[test]
#[ignore = "planned: SLICE-05"]
fn skstat_bit0_always_1() {
    planned();
}

/// Verifies REQ-531: Criterion 1: A write of `SKCTL` = `$00` on cycle L+10 (during the start bit)
/// makes SIO DATA OUT read 0 on each cycle for the next 10000 cycles, and `take_serial_byte` gives
/// no byte. A write of `SKCTL` = `$00` on cycle L+110 (during data bit 0, which is 1) makes SIO
/// DATA OUT read 1 for the next 10000 cycles.
#[test]
#[ignore = "planned: SLICE-05"]
fn init_keeps_data_out_level() {
    planned();
}

/// Verifies REQ-531: Criterion 2: A byte pending in `SEROUT` when `SKCTL` = `$00` is written is not
/// sent after initialization mode ends (REQ-502 criterion 4).
#[test]
#[ignore = "planned: SLICE-05"]
fn init_discards_pending_serout() {
    planned();
}

/// Verifies REQ-531: Criterion 3: A write of `SKCTL` = `$03` (bits 4-6 = `000`) on cycle L+300
/// (during data bit 2, which is 1), with SIO CLOCK IN constant, keeps SIO DATA OUT at 1 for 1000
/// cycles. Then a write of `SKCTL` = `$23` makes the frame continue: data bit 3 starts on the first
/// fire of timer 4 after the write, and data bits 3-7 and the stop bit follow, 94 cycles each.
/// `take_serial_byte` then gives the value `$4F`.
#[test]
#[ignore = "planned: SLICE-05"]
fn clock_reset_keeps_shift_registers() {
    planned();
}

/// Verifies REQ-531: Criterion 4: With `SKCTL` = `$0B`, `AUDCTL` = `$00`, `AUDF1` = `$05`, `AUDF2`
/// = `$07` and the output idle, on a cycle on which SIO DATA OUT reads 1, a write of `SKCTL` =
/// `$03` and then a write of `SKCTL` = `$0B` on the next cycle make SIO DATA OUT read 0 until the
/// next fire of timer 1 or timer 2.
#[test]
#[ignore = "planned: SLICE-05"]
fn two_tone_off_resets_tone_ff() {
    planned();
}

/// Verifies REQ-531: Criterion 5: In the setup of criterion 4, an `STIMER` write does not change
/// SIO DATA OUT. The test writes `STIMER` in two-tone mode and checks that `serial_out` does not
/// change.
#[test]
#[ignore = "planned: SLICE-05"]
fn stimer_keeps_tone_ff() {
    planned();
}

/// Verifies REQ-532: Criterion 1: With the output idle (SIO DATA OUT = 1) and `SKCTL` = `$03`, a
/// write of `SKCTL` = `$83` on cycle X makes SIO DATA OUT read 0 from cycle X+1. A later write of
/// `SKCTL` = `$03` on cycle Y makes SIO DATA OUT read 1 from cycle Y+1.
#[test]
#[ignore = "planned: SLICE-05"]
fn force_break_drives_low() {
    planned();
}

/// Verifies REQ-532.
///
/// Criteria 2, 3: In the setup of REQ-525 criterion 1 with `SKCTL` = `$A3`, SIO DATA OUT reads 0 on
/// each cycle of the frame and after it. `IRQST` bits 3 and 4 change on the same cycles as with
/// `SKCTL` = `$23`. In criterion 2, a write of `SKCTL` = `$23` on cycle L+300 (during data bit 2,
/// which is 1) makes SIO DATA OUT read 1 from cycle L+301. The rest of the frame has the levels and
/// the cycles of REQ-525 criterion 2.
#[test]
#[ignore = "planned: SLICE-05"]
fn force_break_keeps_timing() {
    planned();
}

/// Verifies REQ-533: Criterion 1: On NTSC, with `AUDCTL` = `$00`, `AUDF1` = `$05`, `AUDF2` = `$07`,
/// `SKCTL` = `$0B` and the output idle (data bit 1), SIO DATA OUT changes every 168 cycles (a 5327
/// Hz tone). With `SKCTL` = `$8B` (data bit forced to 0), SIO DATA OUT changes every 224 cycles (a
/// 3995 Hz tone).
#[test]
#[ignore = "planned: SLICE-05"]
fn two_tone_frequencies() {
    planned();
}

/// Verifies REQ-533: Criterion 2: With `AUDCTL` = `$28`, `AUDF1` = `$05`, `AUDF2` = `$07`, `AUDF3`
/// = `$CC`, `AUDF4` = `$05` and `SKCTL` = `$2B`, the test sends the byte `$55`. While a 1 bit is
/// sent, SIO DATA OUT changes every 168 cycles. While a 0 bit is sent, it changes every 224 cycles.
/// The test does not check the first change after each bit boundary. The test sends `$55` in
/// two-tone mode and checks the change spacing in each bit.
#[test]
#[ignore = "planned: SLICE-05"]
fn two_tone_byte_55() {
    planned();
}

/// Verifies REQ-533: Criterion 3: With `AUDCTL` = `$40`, `AUDF1` = `$00`, `AUDF2` = `$FF`, `SKCTL`
/// = `$0B`, the output idle and an `STIMER` write on cycle T, timer 1 fires on cycles T+8, T+14 and
/// T+20, and on no other cycle from T to T+20. Timer 2 does not fire from T to T+1000.
#[test]
#[ignore = "planned: SLICE-05"]
fn two_tone_resync_8_14_20() {
    planned();
}

/// Verifies REQ-533: Criterion 4: In the setup of criterion 3 with `IRQEN` = `$03`, `IRQST` bit 1
/// reads 1 on each cycle from T to T+1000. `IRQST` bit 0 changes to 0 only on the fire cycles of
/// timer 1.
#[test]
#[ignore = "planned: SLICE-05"]
fn two_tone_resync_no_fire() {
    planned();
}

/// Verifies REQ-533: Criterion 5: In the setup of criterion 2 with `SKCTL` = `$AB`, SIO DATA OUT
/// changes every 224 cycles during the whole frame.
#[test]
#[ignore = "planned: SLICE-05"]
fn two_tone_force_break_uses_timer2() {
    planned();
}

/// Verifies REQ-533: Criterion 6: For `AUDC1` = `AUDC2` = `$00`, `$1F`, `$8F` and `$AF`, the SIO
/// DATA OUT levels of criterion 1 are the same on each cycle.
#[test]
#[ignore = "planned: SLICE-05"]
fn two_tone_ignores_audc() {
    planned();
}
