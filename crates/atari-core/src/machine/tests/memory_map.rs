//! Tests of the memory map, the bank switch and the DMA view of the map
//! (`docs/design/system.md`, "Test plan").

use super::{
    Machine, cart_8k, cart_16k, config, cpu_read, cpu_write, machine, machine_with, maxflash,
    os_rom, set_portb,
};
use crate::antic::DmaMemory as _;
use crate::planned::planned;

/// Verifies REQ-209: with `PORTB` pin 7 = 1, a CPU write and then a CPU read of each
/// address in `$5000-$57FF` give the written value (RAM) (criterion 3).
#[test]
fn self_test_disabled_by_bit_7() {
    let mut m = machine();
    set_portb(&mut m, 0xFF);
    for addr in 0x5000..=0x57FF_u16 {
        let [lo, hi] = addr.to_le_bytes();
        let value = lo ^ hi ^ 0xA5;
        cpu_write(&mut m, addr, value);
        assert_eq!(cpu_read(&mut m, addr), value, "RAM at ${addr:04X}");
    }
}

/// Verifies REQ-210: with RD5 inactive, a CPU read of `$A000` gives BASIC byte `$0000` with
/// `PORTB` pins = `$FD`, and RAM with `PORTB` pins = `$FF` (criterion 2).
#[test]
fn basic_after_cartridge_disabled() {
    let mut basic = vec![0xBB; super::super::BASIC_ROM_SIZE];
    basic[0] = 0xB0;
    let mut m = Machine::new(super::MachineConfig {
        basic_rom: Some(basic),
        ..config(os_rom(), Some(maxflash()))
    })
    .unwrap();
    m.poke_ram(0xA000, 0x5A);
    set_portb(&mut m, 0xFD);
    assert_eq!(
        cpu_read(&mut m, 0xA000),
        0x00,
        "RD5 active: cartridge bank 0"
    );
    let _read = cpu_read(&mut m, 0xD510);
    assert_eq!(
        cpu_read(&mut m, 0xA000),
        0xB0,
        "RD5 inactive, BASIC on: BASIC byte $0000"
    );
    set_portb(&mut m, 0xFF);
    assert_eq!(
        cpu_read(&mut m, 0xA000),
        0x5A,
        "RD5 inactive, BASIC off: RAM"
    );
}

/// Verifies REQ-210: with RD4 active, a CPU read of `$8000` gives the cartridge byte; with
/// RD4 inactive, it gives RAM (criterion 3).
#[test]
fn cartridge_rd4_before_ram() {
    let mut m = machine_with(Some(cart_16k()));
    m.poke_ram(0x8000, 0x77);
    assert_eq!(
        cpu_read(&mut m, 0x8000),
        0xCC,
        "RD4 active: the cartridge byte"
    );
    let mut small = machine_with(Some(cart_8k()));
    small.poke_ram(0x8000, 0x77);
    assert_eq!(cpu_read(&mut small, 0x8000), 0x77, "RD4 inactive: RAM");
}

/// Verifies REQ-212: code in RAM: `STA $D301` with `$FE` disables the OS ROM, and the next
/// instruction `LDA $E000` reads the RAM value `$AA` (criterion 1).
#[test]
#[ignore = "planned: SLICE-06"]
fn bank_switch_next_cpu_access() {
    planned();
}

/// Verifies REQ-212: code in RAM: a write of `$01` to `DDRB` (output register `$00`)
/// disables the OS ROM, and the next `LDA $E000` reads the RAM value `$AA` (criterion 2).
#[test]
#[ignore = "planned: SLICE-06"]
fn ddrb_switch_next_cpu_access() {
    planned();
}

/// Verifies REQ-213: with `PORTB` pins = `$FF`, an ANTIC DMA read of `$E000` gives OS image
/// byte `$2000` (criterion 1).
#[test]
fn dma_reads_os_rom() {
    let mut os = os_rom();
    os[0x2000] = 0x5A;
    let mut m = Machine::new(config(os, None)).unwrap();
    set_portb(&mut m, 0xFF);
    m.poke_ram(0xE000, 0xAA);
    assert_eq!(m.mem.dma_read(0xE000), 0x5A, "OS image byte $2000");
}

/// Verifies REQ-213: with `PORTB` pins = `$FE` and RAM at `$E000` = `$AA`, an ANTIC DMA read
/// of `$E000` gives `$AA` (criterion 2).
#[test]
fn dma_reads_ram_under_os() {
    let mut m = machine();
    set_portb(&mut m, 0xFE);
    m.poke_ram(0xE000, 0xAA);
    assert_eq!(m.mem.dma_read(0xE000), 0xAA, "RAM under the OS ROM");
}

/// Verifies REQ-213: an ANTIC DMA read of `$D300` leaves `PACTL` bit 7 = 1, and a counting
/// test cartridge counts exactly one access for one DMA read of `$D500` (criterion 4).
#[test]
fn dma_read_no_side_effects() {
    let mut m = machine_with(Some(cart_16k()));
    cpu_write(&mut m, 0xD302, 0x3C); // PACTL
    m.pia.set_ca1(false); // an active (negative) edge on CA1 sets PACTL bit 7
    assert_eq!(m.pia.peek(2), 0xBC, "PACTL bit 7 = 1 before");
    assert_eq!(m.mem.dma_read(0xD300), 0xFF, "DMA read of $D300");
    assert_eq!(m.pia.peek(2), 0xBC, "PACTL bit 7 = 1 after");

    let count = |m: &Machine| m.mem.cart.as_ref().unwrap().control_accesses;
    assert_eq!(count(&m), 0, "no access before");
    let _value: u8 = m.mem.dma_read(0xD500);
    assert_eq!(count(&m), 1, "one access for one DMA read of $D500");
}

/// Verifies REQ-214: with no cartridge, a CPU read of each address in `$D500-$D5FF` gives
/// `$FF` (criterion 2).
#[test]
#[ignore = "planned: SLICE-06"]
fn cartridge_control_without_cartridge_reads_ff() {
    planned();
}

/// Verifies REQ-214: a test cartridge that returns `None` for `$D500` gives `$FF` for a CPU
/// read of `$D500` (criterion 3).
#[test]
#[ignore = "planned: SLICE-06"]
fn undriven_cartridge_control_reads_ff() {
    planned();
}

/// Verifies REQ-223: while `PORTB` bits 2-6 read 1 as floating bits, CPU reads of `$C000`,
/// `$A000` and `$5000` give the same values as before the write to `DDRB` (criterion 6).
#[test]
#[ignore = "planned: SLICE-06"]
fn floating_bits_do_not_change_map() {
    planned();
}
