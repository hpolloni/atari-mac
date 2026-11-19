//! Planned tests of the cartridge sense with `TRIG3` (`docs/design/system.md`, "Test plan").

use crate::planned::planned;

/// Verifies REQ-232: an 8 KB cartridge (RD5) and a 16 KB cartridge (RD4 and RD5) each make
/// `TRIG3` read `$01` (criteria 2, 7).
#[test]
#[ignore = "planned: SLICE-04"]
fn trig3_one_with_cartridge() {
    planned();
}

/// Verifies REQ-232: a `MaxFlash` 1Mbit cartridge: after `LDA $D510`, the next read of
/// `TRIG3` gives `$00`; after `LDA $D501`, it gives `$01` (criteria 3, 8).
#[test]
#[ignore = "planned: SLICE-04"]
fn trig3_follows_cartridge_bank_switch() {
    planned();
}

/// Verifies REQ-232: with no cartridge and BASIC on (`PORTB` pins = `$FD`), `TRIG3` reads
/// `$00` (criterion 4).
#[test]
#[ignore = "planned: SLICE-04"]
fn trig3_ignores_basic() {
    planned();
}

/// Verifies REQ-232: a read of `TRIG2` (`$D012`) gives `$01` (criterion 5).
#[test]
#[ignore = "planned: SLICE-04"]
fn trig2_reads_one() {
    planned();
}
