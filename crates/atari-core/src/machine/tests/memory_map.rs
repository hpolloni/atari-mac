//! Planned tests of the memory map, the bank switch and the DMA view of the map
//! (`docs/design/system.md`, "Test plan").

use crate::planned::planned;

/// Verifies REQ-209: with `PORTB` pin 7 = 1, a CPU write and then a CPU read of each
/// address in `$5000-$57FF` give the written value (RAM) (criterion 3).
#[test]
#[ignore = "planned: SLICE-04"]
fn self_test_disabled_by_bit_7() {
    planned();
}

/// Verifies REQ-210: with RD5 inactive, a CPU read of `$A000` gives BASIC byte `$0000` with
/// `PORTB` pins = `$FD`, and RAM with `PORTB` pins = `$FF` (criterion 2).
#[test]
#[ignore = "planned: SLICE-04"]
fn basic_after_cartridge_disabled() {
    planned();
}

/// Verifies REQ-210: with RD4 active, a CPU read of `$8000` gives the cartridge byte; with
/// RD4 inactive, it gives RAM (criterion 3).
#[test]
#[ignore = "planned: SLICE-04"]
fn cartridge_rd4_before_ram() {
    planned();
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
#[ignore = "planned: SLICE-02"]
fn dma_reads_os_rom() {
    planned();
}

/// Verifies REQ-213: with `PORTB` pins = `$FE` and RAM at `$E000` = `$AA`, an ANTIC DMA read
/// of `$E000` gives `$AA` (criterion 2).
#[test]
#[ignore = "planned: SLICE-02"]
fn dma_reads_ram_under_os() {
    planned();
}

/// Verifies REQ-213: an ANTIC DMA read of `$D300` leaves `PACTL` bit 7 = 1, and a counting
/// test cartridge counts exactly one access for one DMA read of `$D500` (criterion 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn dma_read_no_side_effects() {
    planned();
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
