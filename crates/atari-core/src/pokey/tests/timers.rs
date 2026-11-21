//! Tests of initialization mode and the timers (REQ-502 to REQ-508).
//!
//! A read "on cycle N" is a `peek` after the tick of cycle N, as a CPU read in cycle N sees
//! it.

use super::serial::receive;
use super::{
    AUDC1, AUDC3, AUDCTL, AUDF1, AUDF2, AUDF3, AUDF4, BIT, IRQEN, LINE, POTGO, SKCTL, SKSTAT,
    STIMER, check_period, clear_irq, fire_log, fires_of, irqst, kbcode, key_is_down, new_pokey,
    run, run_checking, run_until, spacing, timer_pokey,
};
use crate::keyboard;
use crate::pokey::Pokey;
use crate::timing::VideoStandard;

/// `RANDOM` (read).
const RANDOM: u8 = 0x0A;
/// `SERIN` (read).
const SERIN: u8 = 0x0D;
/// `POT0` (read).
const POT0: u8 = 0x00;
/// `ALLPOT` (read).
const ALLPOT: u8 = 0x08;

/// Ticks once and writes `STIMER` on the new cycle. Returns T.
fn stimer_now(p: &mut Pokey) -> u64 {
    p.tick();
    p.write(STIMER, 0x00);
    p.cycle()
}

/// Verifies REQ-502: in initialization mode, the timers on the 64 kHz and 15 kHz clocks do
/// not count.
#[test]
fn init_mode_stops_slow_clocks() {
    for audctl in [0x00, 0x01] {
        let mut p = new_pokey();
        p.tick();
        p.write(IRQEN, 0x01);
        p.write(AUDF1, 0x00);
        p.write(AUDCTL, audctl);
        run(&mut p, 10);
        let t = stimer_now(&mut p);
        run(&mut p, 4);
        let counter = p.test_counters()[0];
        run_checking(&mut p, 10_000 - 4, "no count and no IRQ", |p| {
            p.test_counters()[0] == counter && irqst(p) & 0x01 != 0
        });
        assert_eq!(p.cycle(), t + 10_000, "AUDCTL = {audctl:02X}: last cycle");
    }
}

/// Verifies REQ-502: in initialization mode, the timers on the 1.79 MHz clock run.
#[test]
fn init_mode_keeps_fast_timers() {
    let mut p = new_pokey();
    p.tick();
    p.write(IRQEN, 0x01);
    p.write(AUDF1, 0x00);
    run(&mut p, 10);
    p.tick();
    p.write(AUDCTL, 0x40);
    p.write(STIMER, 0x00);
    let t = p.cycle();
    run_checking(&mut p, 7, "IRQST bit 0 before T+8", |p| {
        irqst(p) & 0x01 != 0
    });
    run_checking(&mut p, 1000, "IRQST bit 0 from T+8", |p| {
        irqst(p) & 0x01 == 0
    });
    assert_eq!(p.cycle(), t + 1007, "cycle");
}

/// The values that initialization mode keeps (REQ-502 criterion 3).
#[derive(Debug, PartialEq, Eq)]
struct Kept {
    /// The write registers as stored.
    regs: super::WriteRegs,
    /// `IRQST`, `KBCODE` and `SERIN`.
    reads: [u8; 3],
    /// The timer counters.
    counters: [u8; 4],
    /// The channel output flip-flops.
    flip_flops: [bool; 4],
}

/// The values of `p` that initialization mode keeps.
fn kept(p: &Pokey) -> Kept {
    Kept {
        regs: p.test_write_regs(),
        reads: [irqst(p), kbcode(p), p.peek(SERIN)],
        counters: p.test_counters(),
        flip_flops: p.test_flip_flops(),
    }
}

/// Verifies REQ-502: initialization mode keeps the registers, the counters and the channel
/// outputs.
#[test]
fn init_mode_keeps_registers() {
    // One received frame for `SERIN` = `$43`.
    let mut p = timer_pokey(0x13, 0x28, [0, 0, 0x28, 0]);
    run(&mut p, 1000);
    let _s = receive(&mut p, 0x43, true, BIT, |_| {});
    assert_eq!(p.peek(SERIN), 0x43, "SERIN");
    // The start state.
    p.write(SKCTL, 0x03);
    p.write(AUDCTL, 0x01);
    for (reg, value) in [(AUDF1, 0x11), (AUDF2, 0x22), (AUDF3, 0x33), (AUDF4, 0x44)] {
        p.write(reg, value);
    }
    for (reg, value) in [(AUDC1, 0xA1), (0x03, 0xA2), (AUDC3, 0xA3), (0x07, 0xA4)] {
        p.write(reg, value);
    }
    p.write(IRQEN, 0x01);
    p.set_key(keyboard::A, true);
    let registered = run_until(&mut p, 200 * LINE, key_is_down);
    assert!(registered.is_some(), "the A key registers");
    p.set_key(keyboard::A, false);
    let latched = run_until(&mut p, 20 * LINE, |p| irqst(p) & 0x01 == 0);
    assert!(latched.is_some(), "IRQST bit 0 latches");
    assert_eq!(kbcode(&p), keyboard::A, "KBCODE");
    // I is 10 cycles after a fire of a timer: no 15 kHz pulse, fire or reload on I.
    let fire = run_until(&mut p, 40 * LINE, |p| p.test_fired() != 0).expect("a timer fires");
    run(&mut p, 9);
    let mut before = kept(&p);
    // Only `SKCTL` changes.
    before.regs.skctl = 0x00;
    p.tick();
    let i = p.cycle();
    assert_eq!(i, fire + 10, "cycle I");
    p.write(SKCTL, 0x00);
    p.tick();
    assert_eq!(kept(&p), before, "on I+1");
    run(&mut p, 999);
    assert_eq!(p.cycle(), i + 1000, "cycle I+1000");
    assert_eq!(kept(&p), before, "on I+1000");
}

/// Verifies REQ-502: the slow pot counter stops in initialization mode.
#[test]
fn init_mode_stops_slow_pot_counter() {
    let mut p = new_pokey();
    p.tick();
    p.write(SKCTL, 0x03);
    p.set_pot(0, 228);
    run(&mut p, 1000);
    p.write(POTGO, 0x00);
    let ten = run_until(&mut p, 20 * LINE, |p| p.peek(POT0) == 10);
    assert!(ten.is_some(), "POT0 reaches 10");
    p.write(SKCTL, 0x00);
    run_checking(&mut p, 10 * LINE, "POT0 = 10 and ALLPOT bit 0 = 1", |p| {
        p.peek(POT0) == 10 && p.peek(ALLPOT) & 0x01 != 0
    });
}

/// Verifies REQ-502: `RANDOM` reads `$FF` from 9 cycles after the entry to initialization
/// mode.
#[test]
fn init_mode_random_ff_after_9_cycles() {
    for audctl in [0x00, 0x80] {
        let mut p = new_pokey();
        p.tick();
        p.write(SKCTL, 0x03);
        p.write(AUDCTL, audctl);
        run(&mut p, 1000);
        p.tick();
        let i = p.cycle();
        p.write(SKCTL, 0x00);
        run(&mut p, 8);
        run_checking(&mut p, 1000 - 8, "RANDOM = $FF", |p| p.peek(RANDOM) == 0xFF);
        assert_eq!(p.cycle(), i + 1000, "AUDCTL = {audctl:02X}: last cycle");
    }
}

/// Verifies REQ-502: in initialization mode, the serial input does not receive.
#[test]
fn init_mode_holds_serial_input() {
    let mut p = timer_pokey(0x20, 0x28, [0, 0, 0x28, 0]);
    p.write(IRQEN, 0x20);
    run(&mut p, 1000);
    let mut fires = 0;
    let ok =
        |p: &Pokey| p.peek(SERIN) == 0x00 && irqst(p) & 0x20 != 0 && p.peek(SKSTAT) & 0x02 != 0;
    let _s = receive(&mut p, 0x43, true, BIT, |p| {
        assert!(ok(p), "the input changed on cycle {}", p.cycle());
        if p.test_fired() & 0b1000 != 0 {
            fires += 1;
        }
    });
    run_checking(&mut p, 1000, "the input after the frame", ok);
    assert!(fires > 15, "timer 4 runs: {fires} fires");
}

// ----------------------------------------------------------------------------------------
// Timers (REQ-503 to REQ-508)
// ----------------------------------------------------------------------------------------

/// `AUDC2`.
const AUDC2: u8 = 0x03;
/// `AUDC4`.
const AUDC4: u8 = 0x07;

/// Writes `AUDF1-4` = `audf`.
fn write_audf(p: &mut Pokey, audf: [u8; 4]) {
    for (reg, value) in [AUDF1, AUDF2, AUDF3, AUDF4].into_iter().zip(audf) {
        p.write(reg, value);
    }
}

/// Writes `AUDC1-4` = `audc`.
fn write_audc(p: &mut Pokey, audc: [u8; 4]) {
    for (reg, value) in [AUDC1, AUDC2, AUDC3, AUDC4].into_iter().zip(audc) {
        p.write(reg, value);
    }
}

/// Ticks once and writes `STIMER` on the new cycle. Returns T.
fn stimer(p: &mut Pokey) -> u64 {
    p.tick();
    p.write(STIMER, 0x00);
    p.cycle()
}

/// The setup of REQ-506 criteria 1-4: `IRQEN` = `$01` and `AUDF1` = `audf1` 20 cycles
/// before the `STIMER` write, and `AUDCTL` = `$40` on the cycle T of the `STIMER` write.
/// POKEY stays in initialization mode, where the 1.79 MHz clock runs (HRM 5.2). Returns T.
fn fast_irq_setup(p: &mut Pokey, audctl: u8, audf1: u8, irqen: u8) -> u64 {
    p.tick();
    p.write(IRQEN, irqen);
    p.write(AUDF1, audf1);
    run(p, 20);
    p.tick();
    p.write(AUDCTL, audctl);
    p.write(STIMER, 0x00);
    p.cycle()
}

/// Runs up to and including cycle `last`.
fn run_to(p: &mut Pokey, last: u64) {
    let cycles = last - p.cycle();
    run(p, cycles);
}

/// `IRQST` bit `bit` on the current cycle (`true` = 1, no event).
fn irq_bit(p: &Pokey, bit: u8) -> bool {
    irqst(p) & bit != 0
}

/// Runs to cycle `last` and returns the first cycle on which `IRQST` bit `bit` reads 0.
fn first_irq(p: &mut Pokey, bit: u8, last: u64) -> Option<u64> {
    while p.cycle() < last {
        p.tick();
        if !irq_bit(p, bit) {
            return Some(p.cycle());
        }
    }
    None
}

/// The setup of REQ-506 criteria 5 and 6: `AUDCTL`, `AUDF1` = `$00` and `IRQEN` = `$01`,
/// initialization mode for 200 cycles with an `STIMER` write in it, then `SKCTL` = `$03`.
/// Returns W.
fn init_exit_setup(p: &mut Pokey, audctl: u8, audf: [u8; 4], irqen: u8) -> u64 {
    p.tick();
    p.write(AUDCTL, audctl);
    write_audf(p, audf);
    p.write(IRQEN, irqen);
    run(p, 200);
    let _t = stimer(p);
    run(p, 20);
    p.tick();
    p.write(SKCTL, 0x03);
    p.cycle()
}

/// Verifies REQ-503: Criterion 1: With `AUDCTL` = `$00` and `AUDF1` = `$00`, the fires of timer 1
/// are 28 cycles apart. With `AUDCTL` = `$01` and `AUDF1` = `$00`, they are 114 cycles apart.
#[test]
fn slow_clock_pulse_spacing() {
    let mut p = timer_pokey(0x03, 0x00, [0; 4]);
    check_period(&mut p, 1, 28, 20);
    let mut p = timer_pokey(0x03, 0x01, [0; 4]);
    check_period(&mut p, 1, LINE, 10);
}

/// Verifies REQ-503: Criterion 2: With `AUDCTL` = `$60` and `AUDF1-4` = `$00`, the fires of timers
/// 1 and 3 are 4 cycles apart, and the fires of timers 2 and 4 are 28 cycles apart. With `AUDCTL` =
/// `$61`, the fires of timers 2 and 4 are 114 cycles apart.
#[test]
fn timers_2_and_4_never_fast() {
    for (timer, period) in [(1, 4), (2, 28), (3, 4), (4, 28)] {
        let mut p = timer_pokey(0x03, 0x60, [0; 4]);
        check_period(&mut p, timer, period, 10);
    }
    for (timer, period) in [(1, 4), (2, LINE), (3, 4), (4, LINE)] {
        let mut p = timer_pokey(0x03, 0x61, [0; 4]);
        check_period(&mut p, timer, period, 5);
    }
}

/// Verifies REQ-503: Criterion 3: With `AUDCTL` = `$40` and `AUDF3` = `$00`, the fires of timer 3
/// are 28 cycles apart. With `AUDCTL` = `$20` and `AUDF1` = `$00`, the fires of timer 1 are 28
/// cycles apart. The test checks the fire spacing of timers 1 and 3 with `AUDCTL` = `$40` and
/// `$20`.
#[test]
fn fast_clock_bits_select_timer() {
    for (audctl, timer, period) in [(0x40, 1, 4), (0x40, 3, 28), (0x20, 1, 28), (0x20, 3, 4)] {
        let mut p = timer_pokey(0x03, audctl, [0; 4]);
        check_period(&mut p, timer, period, 10);
    }
}

/// Verifies REQ-503: Criteria 4, 5: On NTSC, `AUDCTL` = `$00`, `AUDF1` = `$48` and `AUDC1` = `$AF`
/// make the channel 1 digital output a square wave with a period of 4088 cycles (2 x 73 x 28), with
/// 2044 cycles at each level. At 48000 Hz, the measured frequency (see Notes) of the audio output
/// is 437.8 Hz +/- 0.5 Hz. On PAL, the same register values give the same 4088-cycle period. The
/// measured frequency is 433.8 Hz +/- 0.5 Hz. The test checks the period of the channel 1 digital
/// output for NTSC and PAL.
#[test]
fn square_wave_period_4088() {
    for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
        let mut p = Pokey::new(standard, 48_000);
        p.tick();
        p.write(SKCTL, 0x03);
        p.write(AUDCTL, 0x00);
        p.write(AUDF1, 0x48);
        p.write(AUDC1, 0xAF);
        run(&mut p, 5000);
        let mut changes = Vec::new();
        let mut last = p.test_channel_output(0);
        for _ in 0..10 * 4088 {
            p.tick();
            let now = p.test_channel_output(0);
            if now != last {
                changes.push(p.cycle());
                last = now;
            }
        }
        assert!(
            changes.len() >= 19,
            "{standard:?}: {} changes",
            changes.len()
        );
        assert!(
            spacing(&changes).iter().all(|d| *d == 2044),
            "{standard:?}: {:?}",
            spacing(&changes)
        );
    }
}

/// Verifies REQ-504: Criterion 1: With `AUDCTL` = `$40` and `AUDF1` = N, the fires of timer 1 are N
/// + 4 cycles apart, for N = 0, 1, 5 and 255.
#[test]
fn unlinked_fast_period() {
    for n in [0_u8, 1, 5, 255] {
        let mut p = timer_pokey(0x03, 0x40, [n, 0, 0, 0]);
        check_period(&mut p, 1, u64::from(n) + 4, 5);
    }
}

/// Verifies REQ-504: Criterion 2: With `AUDCTL` = `$00` and `AUDF1` = `$09`, the fires of timer 1
/// are 280 cycles apart.
#[test]
fn unlinked_64khz_period() {
    let mut p = timer_pokey(0x03, 0x00, [0x09, 0, 0, 0]);
    check_period(&mut p, 1, 280, 5);
}

/// Verifies REQ-504: Criterion 3: With `AUDCTL` = `$01` and `AUDF4` = `$00`, the fires of timer 4
/// are 114 cycles apart. With `AUDF4` = `$09`, they are 1140 cycles apart.
#[test]
fn unlinked_15khz_period() {
    let mut p = timer_pokey(0x03, 0x01, [0, 0, 0, 0x00]);
    check_period(&mut p, 4, LINE, 5);
    let mut p = timer_pokey(0x03, 0x01, [0, 0, 0, 0x09]);
    check_period(&mut p, 4, 10 * LINE, 3);
}

/// Verifies REQ-504: Criterion 4: With `AUDCTL` = `$00`, `AUDF1` = `$03`, `AUDF2` = `$01` and an
/// `STIMER` write, each fire of timer 1 is on the same cycle as a fire of timer 2.
#[test]
fn slow_timers_share_pulse() {
    let mut p = timer_pokey(0x03, 0x00, [0x03, 0x01, 0, 0]);
    let _t = stimer(&mut p);
    let log = fire_log(&mut p, 2000);
    let timer1 = fires_of(&log, 1);
    let timer2 = fires_of(&log, 2);
    assert!(timer1.len() >= 15, "timer 1 fires: {timer1:?}");
    assert!(
        timer1.iter().all(|c| timer2.contains(c)),
        "timer 1 {timer1:?}, timer 2 {timer2:?}"
    );
}

/// Verifies REQ-506: Criterion 1: With `AUDCTL` = `$40`, `AUDF1` = `$00`, `IRQEN` = `$01` and an
/// `STIMER` write on cycle T, `IRQST` bit 0 reads 1 on cycle T+7 and 0 on cycle T+8.
#[test]
fn stimer_first_irq_t8() {
    let mut p = new_pokey();
    let t = fast_irq_setup(&mut p, 0x40, 0x00, 0x01);
    assert_eq!(
        first_irq(&mut p, 0x01, t + 7),
        None,
        "IRQST bit 0 up to T+7"
    );
    p.tick();
    assert!(!irq_bit(&p, 0x01), "IRQST bit 0 on T+8");
}

/// Verifies REQ-506: Criterion 2: With the setup of criterion 1 and `AUDF1` = `$05`, `IRQST` bit 0
/// reads 1 on cycle T+12 and 0 on cycle T+13.
#[test]
fn stimer_first_irq_t13() {
    let mut p = new_pokey();
    let t = fast_irq_setup(&mut p, 0x40, 0x05, 0x01);
    assert_eq!(
        first_irq(&mut p, 0x01, t + 12),
        None,
        "IRQST bit 0 up to T+12"
    );
    p.tick();
    assert!(!irq_bit(&p, 0x01), "IRQST bit 0 on T+13");
}

/// Verifies REQ-506: Criterion 3: With the setup of criterion 1, a second `STIMER` write on cycle
/// T+4 makes `IRQST` bit 0 read 1 on cycles T+8 to T+11 and 0 on cycle T+12. A second `STIMER`
/// write on cycle T+5 makes `IRQST` bit 0 read 0 on cycle T+8.
#[test]
fn stimer_preemption_window() {
    let mut p = new_pokey();
    let t = fast_irq_setup(&mut p, 0x40, 0x00, 0x01);
    run_to(&mut p, t + 4);
    p.write(STIMER, 0x00);
    assert_eq!(
        first_irq(&mut p, 0x01, t + 11),
        None,
        "IRQST bit 0 up to T+11"
    );
    p.tick();
    assert!(!irq_bit(&p, 0x01), "IRQST bit 0 on T+12");

    let mut p = new_pokey();
    let t = fast_irq_setup(&mut p, 0x40, 0x00, 0x01);
    run_to(&mut p, t + 5);
    p.write(STIMER, 0x00);
    assert_eq!(first_irq(&mut p, 0x01, t + 8), Some(t + 8), "STIMER on T+5");
}

/// Verifies REQ-506: Criterion 4: With the setup of criterion 1, an `AUDF1` = `$05` write on cycle
/// T+2 makes `IRQST` bit 0 read 0 first on cycle T+13. An `AUDF1` = `$05` write on cycle T+3 makes
/// `IRQST` bit 0 read 0 first on cycle T+8, and the second fire of timer 1 is on cycle T+17.
#[test]
fn audf_reload_deadline() {
    let mut p = new_pokey();
    let t = fast_irq_setup(&mut p, 0x40, 0x00, 0x01);
    run_to(&mut p, t + 2);
    p.write(AUDF1, 0x05);
    assert_eq!(
        first_irq(&mut p, 0x01, t + 20),
        Some(t + 13),
        "AUDF1 on T+2"
    );

    let mut p = new_pokey();
    let t = fast_irq_setup(&mut p, 0x40, 0x00, 0x01);
    run_to(&mut p, t + 3);
    p.write(AUDF1, 0x05);
    let log = fire_log(&mut p, 20);
    assert_eq!(
        fires_of(&log, 1)[..2],
        [t + 8, t + 17],
        "fires of timer 1 after AUDF1 on T+3"
    );
    let mut p = new_pokey();
    let t = fast_irq_setup(&mut p, 0x40, 0x00, 0x01);
    run_to(&mut p, t + 3);
    p.write(AUDF1, 0x05);
    assert_eq!(first_irq(&mut p, 0x01, t + 20), Some(t + 8), "AUDF1 on T+3");
}

/// Verifies REQ-506: Criterion 5: With `AUDCTL` = `$01`, `AUDF1` = `$00`, `IRQEN` = `$01`, `SKCTL`
/// = `$00` for at least 114 cycles and an `STIMER` write while `SKCTL` = `$00`, a write of `SKCTL`
/// = `$03` on cycle W makes `IRQST` bit 0 read 1 on cycle W+82 and 0 on cycle W+83.
#[test]
fn init_exit_irq_w83() {
    let mut p = new_pokey();
    let w = init_exit_setup(&mut p, 0x01, [0; 4], 0x01);
    assert_eq!(
        first_irq(&mut p, 0x01, w + 82),
        None,
        "IRQST bit 0 up to W+82"
    );
    p.tick();
    assert!(!irq_bit(&p, 0x01), "IRQST bit 0 on W+83");
}

/// Verifies REQ-506: Criterion 6: With the setup of criterion 5 and `AUDCTL` = `$00`, `IRQST` bit 0
/// reads 1 on cycle W+23 and 0 on cycle W+24.
#[test]
fn init_exit_irq_w24() {
    let mut p = new_pokey();
    let w = init_exit_setup(&mut p, 0x00, [0; 4], 0x01);
    assert_eq!(
        first_irq(&mut p, 0x01, w + 23),
        None,
        "IRQST bit 0 up to W+23"
    );
    p.tick();
    assert!(!irq_bit(&p, 0x01), "IRQST bit 0 on W+24");
}

/// Verifies REQ-506: Criterion 7: With `AUDCTL` = `$50`, `AUDF1` = `$00`, `AUDF2` = `$00`, `IRQEN`
/// = `$02` and an `STIMER` write on cycle T, timer 1 fires on cycles T+8, T+15 and T+22, and timer
/// 2 fires on cycles T+11, T+18 and T+25. `IRQST` bit 1 reads 1 on cycle T+10 and 0 on cycle T+11.
#[test]
fn linked_timer_irq_timing() {
    let mut p = new_pokey();
    let t = fast_irq_setup(&mut p, 0x50, 0x00, 0x02);
    let mut q = p.clone();
    let log = fire_log(&mut p, 25);
    assert_eq!(fires_of(&log, 1), [t + 8, t + 15, t + 22], "timer 1");
    assert_eq!(fires_of(&log, 2), [t + 11, t + 18, t + 25], "timer 2");
    assert_eq!(
        first_irq(&mut q, 0x02, t + 10),
        None,
        "IRQST bit 1 up to T+10"
    );
    q.tick();
    assert!(!irq_bit(&q, 0x02), "IRQST bit 1 on T+11");
}

/// Verifies REQ-507: Criterion 1: With `AUDCTL` = `$00` and `AUDC1-4` = `$AF`, the digital outputs
/// on the cycle after an `STIMER` write are 0 for channels 1 and 2 and 1 for channels 3 and 4. The
/// test checks this for the flip-flop values 0, 0, 0, 0 and 1, 1, 1, 1 before the write. The test
/// sets these values with timer fires before the write.
#[test]
fn stimer_sets_output_flip_flops() {
    for fires in [1, 2] {
        let mut p = timer_pokey(0x03, 0x00, [0; 4]);
        write_audc(&mut p, [0xAF; 4]);
        let _s = stimer(&mut p);
        // All four timers fire on the same cycles and toggle their flip-flops.
        for _ in 0..fires {
            let fired = run_until(&mut p, 100, |p| p.test_fired() != 0);
            assert!(fired.is_some(), "no fire");
            assert_eq!(p.test_fired(), 0x0F, "all four timers fire together");
        }
        let before = fires == 2;
        assert_eq!(p.test_flip_flops(), [before; 4], "flip-flops before STIMER");
        let _t = stimer(&mut p);
        p.tick();
        let outputs: Vec<bool> = (0..4).map(|n| p.test_channel_output(n)).collect();
        assert_eq!(
            outputs,
            [false, false, true, true],
            "outputs after STIMER, flip-flops {before} before"
        );
    }
}

/// A POKEY with `AUDCTL` = `audctl` and `AUDF1-4` = `$F0`, `STIMER` written, after `k`
/// cycles. The counters are then between `$01` and `$EF`, so no fire is scheduled.
fn counters_at(audctl: u8, k: u64) -> Pokey {
    let mut p = new_pokey();
    p.tick();
    p.write(SKCTL, 0x03);
    p.write(AUDCTL, audctl);
    write_audf(&mut p, [0xF0; 4]);
    let _s = stimer(&mut p);
    run(&mut p, k);
    let counters = p.test_counters();
    assert!(
        counters.iter().all(|c| (0x01..=0xEF).contains(c)),
        "counters {counters:02X?} after {k} cycles"
    );
    p
}

/// Verifies REQ-507: Criteria 2, 3: With `AUDCTL` = `$60`, `AUDF1` = `$00` and `AUDF3` = `$05`,
/// after an `STIMER` write on cycle T, the next fire of timer 1 is on cycle T+8 and the next fire
/// of timer 3 is on cycle T+13. This is true for 10 different start values of the counters. With
/// `AUDCTL` = `$00` and `AUDF2` = `AUDF4` = `$03`, after an `STIMER` write on cycle T, the first
/// fires of timers 2 and 4 are on the same cycle, in the range T+92 to T+119. The test sets random
/// counter values, writes `STIMER`, and checks the next fire cycles of all four timers.
#[test]
fn stimer_reloads_all_timers() {
    // Criterion 2.
    let mut starts = Vec::new();
    for k in (20..=200).step_by(20) {
        let mut p = counters_at(0x60, k);
        starts.push(p.test_counters());
        p.write(AUDF1, 0x00);
        p.write(AUDF3, 0x05);
        let t = stimer(&mut p);
        let log = fire_log(&mut p, 20);
        assert_eq!(
            fires_of(&log, 1).first(),
            Some(&(t + 8)),
            "timer 1, k = {k}"
        );
        assert_eq!(
            fires_of(&log, 3).first(),
            Some(&(t + 13)),
            "timer 3, k = {k}"
        );
    }
    starts.dedup();
    assert_eq!(starts.len(), 10, "start values {starts:02X?}");
    // Criterion 3.
    let mut starts = Vec::new();
    for k in (100..=1000).step_by(100) {
        let mut p = counters_at(0x00, k);
        starts.push(p.test_counters());
        p.write(AUDF2, 0x03);
        p.write(AUDF4, 0x03);
        let t = stimer(&mut p);
        let log = fire_log(&mut p, 200);
        let first2 = fires_of(&log, 2).first().copied();
        let first4 = fires_of(&log, 4).first().copied();
        assert_eq!(first2, first4, "timers 2 and 4, k = {k}");
        let first = first2.unwrap_or(0);
        assert!(
            (t + 92..=t + 119).contains(&first),
            "first fire T+{}, k = {k}",
            first.saturating_sub(t)
        );
    }
    starts.dedup();
    assert_eq!(starts.len(), 10, "start values {starts:02X?}");
}

/// Verifies REQ-507: Criterion 4: With `IRQEN` = `$07`, an `STIMER` write does not change `IRQST`.
/// With the setup of criterion 2 and `AUDC1` = `$AF`, the channel 1 flip-flop changes only on the
/// fire cycles of timer 1. The `STIMER` write itself is not a fire.
#[test]
fn stimer_does_not_fire() {
    let mut p = counters_at(0x60, 50);
    p.write(IRQEN, 0x07);
    p.write(AUDC1, 0xAF);
    run(&mut p, 10);
    // The flip-flop is 1 since the first STIMER write, and no timer fired since then.
    assert!(p.test_flip_flops()[0], "channel 1 flip-flop before STIMER");
    p.write(AUDF1, 0x00);
    p.write(AUDF3, 0x05);
    p.tick();
    let before = irqst(&p);
    p.write(STIMER, 0x00);
    let t = p.cycle();
    assert_eq!(irqst(&p), before, "IRQST after the STIMER write");
    assert!(
        p.test_flip_flops()[0],
        "channel 1 flip-flop after the STIMER write"
    );
    run_checking(&mut p, 7, "IRQST until T+7", |p| irqst(p) == before);
    let mut last = p.test_flip_flops()[0];
    let mut changes = 0;
    for _ in 0..1000 {
        p.tick();
        let now = p.test_flip_flops()[0];
        let fired = p.test_fired() & 0x01 != 0;
        assert_eq!(
            now != last,
            fired,
            "flip-flop change on T+{}",
            p.cycle() - t
        );
        changes += usize::from(fired);
        last = now;
    }
    assert!(changes > 100, "{changes} fires");
}

/// Verifies REQ-507: Criterion 5: With `SKCTL` = `$03` written on cycle W, `AUDCTL` = `$00` and
/// `AUDF1` = `$00`, all fires of timer 1 are on cycles W + 24 + 28k (k = 0, 1, 2, ...). The test
/// checks this with one `STIMER` write on each of the cycles W+100 to W+127, one cycle in each run.
#[test]
fn stimer_keeps_slow_clock_phase() {
    for d in 100..=127 {
        let mut p = new_pokey();
        p.tick();
        p.write(AUDCTL, 0x00);
        p.write(AUDF1, 0x00);
        p.tick();
        p.write(SKCTL, 0x03);
        let w = p.cycle();
        let mut log = fire_log(&mut p, d - 1);
        p.tick();
        p.write(STIMER, 0x00);
        log.extend(fire_log(&mut p, 500));
        let fires = fires_of(&log, 1);
        assert!(fires.len() > 15, "W+{d}: {} fires", fires.len());
        assert!(
            fires
                .iter()
                .all(|c| *c >= w + 24 && (c - w - 24).is_multiple_of(28)),
            "STIMER on W+{d}: fires {:?}",
            fires.iter().map(|c| c - w).collect::<Vec<_>>()
        );
    }
}

/// Verifies REQ-508: Criterion 1: With `AUDCTL` = `$00`, `AUDF1-4` = `$00`, `IRQEN` = `$07`, an
/// `STIMER` write while `SKCTL` = `$00`, and a write of `SKCTL` = `$03` on cycle W, `IRQST` reads
/// `$F7` on cycle W+23 and `$F0` on cycle W+24.
#[test]
fn timer_irq_bits() {
    let mut p = new_pokey();
    let w = init_exit_setup(&mut p, 0x00, [0; 4], 0x07);
    run_to(&mut p, w + 23);
    assert_eq!(irqst(&p), 0xF7, "IRQST on W+23");
    p.tick();
    assert_eq!(irqst(&p), 0xF0, "IRQST on W+24");
}

/// Verifies REQ-508: Criterion 2: With `AUDCTL` = `$20`, `AUDF3` = `$00`, `AUDF1` = `AUDF2` =
/// `AUDF4` = `$FF`, `IRQEN` = `$FF`, no key down, SIO DATA IN at 1 and an `STIMER` write on cycle
/// T, `IRQST` reads `$F7` on each cycle from T+1 to T+1000. Timer 3 fires every 4 cycles in this
/// time.
#[test]
fn timer_3_has_no_irq() {
    let mut p = new_pokey();
    p.tick();
    p.write(AUDCTL, 0x20);
    write_audf(&mut p, [0xFF, 0xFF, 0x00, 0xFF]);
    p.write(SKCTL, 0x03);
    p.set_serial_in(true);
    let _s = stimer(&mut p);
    p.write(IRQEN, 0xFF);
    run(&mut p, 20);
    assert_eq!(irqst(&p), 0xF7, "IRQST before T");
    let t = stimer(&mut p);
    let mut fires = Vec::new();
    for _ in 0..1000 {
        p.tick();
        assert_eq!(irqst(&p), 0xF7, "IRQST on cycle {}", p.cycle());
        // A fire that was visible before T+4 is not cancelled by the STIMER write (REQ-506);
        // the restarted timer fires from T+8.
        if p.test_fired() & 0x04 != 0 && p.cycle() >= t + 4 {
            fires.push(p.cycle());
        }
    }
    assert_eq!(fires.first(), Some(&(t + 8)), "first fire of timer 3");
    assert!(fires.len() >= 240, "{} fires of timer 3", fires.len());
    assert!(spacing(&fires).iter().all(|d| *d == 4), "timer 3 period");
}

/// Verifies REQ-508: Criterion 3: With `AUDCTL` = `$50`, `AUDF1` = `$40`, `AUDF2` = `$01` and
/// `IRQEN` = `$02`, timer 2 fires every 327 cycles. After each event, the test writes `IRQEN` =
/// `$00` and then `IRQEN` = `$02`. `IRQST` bit 1 goes to 0 exactly once in each 327-cycle period,
/// on the fire cycle of timer 2.
#[test]
fn linked_timer_irq_once_per_period() {
    let mut p = timer_pokey(0x03, 0x50, [0x40, 0x01, 0, 0]);
    p.write(IRQEN, 0x02);
    run(&mut p, 1000);
    clear_irq(&mut p, 0x02, 0x02);
    let mut fires = Vec::new();
    let mut events = Vec::new();
    for _ in 0..10 * 327 {
        p.tick();
        if p.test_fired() & 0x02 != 0 {
            fires.push(p.cycle());
        }
        if !irq_bit(&p, 0x02) {
            events.push(p.cycle());
            p.write(IRQEN, 0x00);
            p.write(IRQEN, 0x02);
        }
    }
    assert!(fires.len() >= 9, "{} fires of timer 2", fires.len());
    assert!(spacing(&fires).iter().all(|d| *d == 327), "timer 2 period");
    assert_eq!(events, fires, "IRQST bit 1 events");
}

/// Verifies REQ-508: Criterion 4: With `AUDCTL` = `$00`, `AUDF1` = `$00` and `IRQEN` = `$00`, timer
/// 1 fires on cycles E and E+28. A write of `IRQEN` = `$01` on cycle E+1 makes `IRQST` bit 0 read 1
/// on cycles E+1 to E+27 and 0 on cycle E+28.
#[test]
fn timer_irq_lost_when_disabled() {
    let mut p = timer_pokey(0x03, 0x00, [0; 4]);
    let e = run_until(&mut p, 100, |p| p.test_fired() & 0x01 != 0).unwrap_or(0);
    assert!(e > 0, "timer 1 fires");
    p.write(IRQEN, 0x00);
    p.tick();
    p.write(IRQEN, 0x01);
    assert!(irq_bit(&p, 0x01), "IRQST bit 0 on E+1");
    assert_eq!(
        first_irq(&mut p, 0x01, e + 27),
        None,
        "IRQST bit 0 up to E+27"
    );
    p.tick();
    assert_eq!(p.test_fired() & 0x01, 0x01, "timer 1 fires on E+28");
    assert!(!irq_bit(&p, 0x01), "IRQST bit 0 on E+28");
}

/// Verifies REQ-508.
///
/// Criterion 5: With the setup of REQ-506 criterion 1, `IRQST` bit 0 reads 0 first on cycle T+8 for
/// each of `AUDC1` = `$00`, `$0F`, `$1F`, `$2F`, `$8F`, `$AF` and `$CF`.
#[test]
fn timer_irq_ignores_audc() {
    for audc in [0x00, 0x0F, 0x1F, 0x2F, 0x8F, 0xAF, 0xCF] {
        let mut p = new_pokey();
        p.write(AUDC1, audc);
        let t = fast_irq_setup(&mut p, 0x40, 0x00, 0x01);
        assert_eq!(
            first_irq(&mut p, 0x01, t + 20),
            Some(t + 8),
            "AUDC1 = {audc:#04X}"
        );
    }
}
