//! Tests of the cartridge sense with `TRIG3` (`docs/design/system.md`, "Test plan").

use super::{cart_8k, cart_16k, cpu_read, machine, machine_with, maxflash_machine, set_portb};

/// Verifies REQ-232: an 8 KB cartridge (RD5) and a 16 KB cartridge (RD4 and RD5) each make
/// `TRIG3` read `$01` (criteria 2, 7).
#[test]
fn trig3_one_with_cartridge() {
    let mut m = machine_with(Some(cart_8k()));
    assert_eq!(cpu_read(&mut m, 0xD013), 0x01, "2: 8 KB cartridge");
    let mut m = machine_with(Some(cart_16k()));
    assert_eq!(cpu_read(&mut m, 0xD013), 0x01, "7: 16 KB cartridge");
}

/// Verifies REQ-232: a `MaxFlash` 1Mbit cartridge: after `LDA $D510`, the next read of
/// `TRIG3` gives `$00`; after `LDA $D501`, it gives `$01` (criteria 3, 8).
#[test]
fn trig3_follows_cartridge_bank_switch() {
    let mut m = maxflash_machine(&[
        0xAD, 0x10, 0xD5, // LDA $D510
        0xAD, 0x13, 0xD0, // LDA TRIG3
        0x8D, 0x00, 0x06, // STA $0600
        0xAD, 0x01, 0xD5, // LDA $D501
        0xAD, 0x13, 0xD0, // LDA TRIG3
        0x8D, 0x01, 0x06, // STA $0601
        0xA9, 0xA5, // LDA #$A5
        0x8D, 0x02, 0x06, // STA $0602
        0x4C, 0x17, 0xC2, // JMP $C217
    ]);
    for _ in 0..200 {
        m.run_cycle();
    }
    assert_eq!(m.peek_ram(0x0602), 0xA5, "the code ran");
    assert_eq!(m.peek_ram(0x0600), 0x00, "3: TRIG3 after LDA $D510");
    assert_eq!(m.peek_ram(0x0601), 0x01, "8: TRIG3 after LDA $D501");
}

/// Verifies REQ-232: with no cartridge and BASIC on (`PORTB` pins = `$FD`), `TRIG3` reads
/// `$00` (criterion 4).
#[test]
fn trig3_ignores_basic() {
    let mut m = machine();
    set_portb(&mut m, 0xFD);
    assert_eq!(m.peek(0xA000), 0xBB, "BASIC is on");
    m.run_cycle();
    assert_eq!(cpu_read(&mut m, 0xD013), 0x00, "4: TRIG3");
}

/// Verifies REQ-232: a read of `TRIG2` (`$D012`) gives `$01` (criterion 5).
#[test]
fn trig2_reads_one() {
    let mut m = machine();
    assert_eq!(cpu_read(&mut m, 0xD012), 0x01, "5: no cartridge");
    let mut m = machine_with(Some(cart_8k()));
    assert_eq!(cpu_read(&mut m, 0xD012), 0x01, "5: 8 KB cartridge");
}
