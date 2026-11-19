//! Planned tests of the pot scan (REQ-534 to REQ-537).
//!
//! Each test is a stub until its package writes it (`docs/design/pokey.md`, "Test plan").

use crate::planned::planned;

/// Verifies REQ-534: Criterion 1: On cycle G+1, for each input below threshold, `ALLPOT` bit n
/// reads 1 and `POTn` reads 0.
#[test]
#[ignore = "planned: SLICE-04"]
fn potgo_starts_scan() {
    planned();
}

/// Verifies REQ-534: Criterion 2: With `set_pot(0, 100)`, `POT0` reads 100 after the scan. `ALLPOT`
/// bit 0 reads 1 while `POT0` reads less than 100. It changes to 0 while the counter is 100.
#[test]
#[ignore = "planned: SLICE-04"]
fn slow_pot_latches_count() {
    planned();
}

/// Verifies REQ-534: Criterion 3: For each input that does not reach threshold, `POTn` reads 228
/// from cycle P + 227 x 114 + 1. `ALLPOT` reads `$00` first on cycle P + 227 x 114 + 2.
#[test]
#[ignore = "planned: SLICE-04"]
fn slow_pot_scan_end() {
    planned();
}

/// Verifies REQ-534: Criterion 4: P is a cycle W + 81 + 114 k (k = 0, 1, 2, ...). For an input
/// without a paddle, `POTn` reads the old count on each cycle W + 80 + 114 k and the new count on
/// each cycle W + 82 + 114 k, during the scan.
#[test]
#[ignore = "planned: SLICE-04"]
fn pot_increment_phase() {
    planned();
}

/// Verifies REQ-534: Criterion 5: With `set_pot(0, 100)`, `ALLPOT` bit 0 changes to 0 on cycle G +
/// 100 x 114 + 1, for each of the 114 values of (G - W) modulo 114. This cycle has no fixed
/// relation to the pot counter increments.
#[test]
#[ignore = "planned: SLICE-04"]
fn allpot_updates_each_cycle() {
    planned();
}

/// Verifies REQ-535: Criterion 1: For each input 0-3, `set_pot(n, c)` makes `POTn` read c after the
/// scan, and `ALLPOT` bit n change to 0 on cycle G + 114 x c + 1, for c = 1, 100 and 227. The test
/// checks input 0 for each of the 114 values of (G - W) modulo 114.
#[test]
#[ignore = "planned: SLICE-04"]
fn paddle_host_value_range() {
    planned();
}

/// Verifies REQ-535: Criterion 2: `set_pot(0, 228)`, `set_pot(0, 255)`, or no call of `set_pot` for
/// input 0, makes `POT0` read 228 after the scan.
#[test]
#[ignore = "planned: SLICE-04"]
fn no_paddle_reads_228() {
    planned();
}

/// Verifies REQ-535: Criterion 3: `POT4-POT7` read 228 after a slow scan. They read 229 after a
/// fast scan (REQ-536 criterion 1).
#[test]
#[ignore = "planned: SLICE-04"]
fn grounded_pots_read_228_and_229() {
    planned();
}

/// Verifies REQ-535: Criterion 4: With `set_pot(0, 200)`, a call of `set_pot(0, 50)` when `POT0`
/// reads 100 makes `ALLPOT` bit 0 read 0 from the next cycle. `POT0` reads 100 or 101 after the
/// scan.
#[test]
#[ignore = "planned: SLICE-04"]
fn pot_change_during_scan() {
    planned();
}

/// Verifies REQ-536: Criterion 1: With `SKCTL` = `$07` and a `POTGO` write on cycle G, `ALLPOT` bit
/// 4 reads 1 on cycle G+230, and `ALLPOT` reads `$00` from cycle G+231. `POT4` reads 229 from cycle
/// G+230.
#[test]
#[ignore = "planned: SLICE-04"]
fn fast_pot_scan_end_231() {
    planned();
}

/// Verifies REQ-536: Criterion 2: With `SKCTL` = `$03` for at least 100 cycles (no scan), a write
/// of `SKCTL` = `$07` on cycle F and a `POTGO` write on cycle F+1: `set_pot(0, 1)` gives a `POT0`
/// value from 112 to 114, `set_pot(0, 2)` gives a value from 226 to 228, and `set_pot(0, 3)` gives
/// 229.
#[test]
#[ignore = "planned: SLICE-04"]
fn fast_pot_paddle_count() {
    planned();
}

/// Verifies REQ-536: Criterion 3: With `SKCTL` = `$07` and `set_pot(0, 1)`, a first fast scan
/// latches `POT0` = v. A second `POTGO` write 1000 cycles later keeps `POT0` at v and `ALLPOT` bit
/// 0 at 0 on each cycle of the second scan.
#[test]
#[ignore = "planned: SLICE-04"]
fn fast_pot_no_dump() {
    planned();
}

/// Verifies REQ-536: Criterion 4: With `SKCTL` = `$04` (initialization mode and fast pot scan) and
/// a `POTGO` write on cycle G, `ALLPOT` reads `$00` from cycle G+231 and `POT4` reads 229.
#[test]
#[ignore = "planned: SLICE-04"]
fn fast_pot_ignores_init_mode() {
    planned();
}

/// Verifies REQ-537: Criterion 1: With `set_pot(0, 150)`, a second `POTGO` write on cycle G + 64 x
/// 114 makes `POT0` read 86 +/- 1 after the second scan.
#[test]
#[ignore = "planned: SLICE-04"]
fn truncated_pot_scan() {
    planned();
}

/// Verifies REQ-537: Criterion 2: With `set_pot(0, 150)`, a write of `SKCTL` = `$00` on cycle G +
/// 20 x 114 and a write of `SKCTL` = `$03` on cycle G + 70 x 114 make `POT0` read 100 +/- 1 after
/// the scan.
#[test]
#[ignore = "planned: SLICE-04"]
fn suspended_pot_scan() {
    planned();
}

/// Verifies REQ-537: Criterion 3: After the second `POTGO` write of criterion 1 on cycle G2,
/// `ALLPOT` reads `$00` first on cycle P2 + 227 x 114 + 2. P2 is the cycle of the first pot counter
/// increment after G2.
#[test]
#[ignore = "planned: SLICE-04"]
fn restarted_scan_runs_full() {
    planned();
}

/// Verifies REQ-537: Criterion 4: With `set_pot(0, 30)`, a second `POTGO` write on cycle G + 64 x
/// 114 keeps `POT0` at 30 and `ALLPOT` bit 0 at 0 on each cycle of the second scan. The test
/// restarts a scan after the input latched and checks `POT0` and `ALLPOT` bit 0.
#[test]
#[ignore = "planned: SLICE-04"]
fn restarted_scan_keeps_latched_input() {
    planned();
}
