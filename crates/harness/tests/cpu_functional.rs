//! The Klaus Dormann 6502 functional test (REQ-110).

use atari_core::cpu::Cpu;
use atari_harness::data;
use atari_harness::single_step::{FlatBus, ready_cpu};

/// The address of the success trap (`jmp *`) in the test listing.
const SUCCESS: u16 = 0x3469;

/// The start address of the test.
const START: u16 = 0x0400;

/// The cycle limit of the test.
const MAX_CYCLES: u64 = 100_000_000;

/// Verifies REQ-110: the CPU reaches the success trap at `$3469`.
#[test]
fn klaus_dormann_functional_test() {
    let image =
        data::read_test_file("test-data/klaus/6502_functional_test.bin", data::FETCH_ROMS).unwrap();
    let mut bus = FlatBus::new();
    bus.ram.copy_from_slice(&image);
    let mut cpu = ready_cpu();
    cpu.registers_mut().pc = START;
    let mut cycles = 0_u64;
    let trap = loop {
        let pc = cpu.registers().pc;
        bus.log.clear();
        let _step = Cpu::step(&mut cpu, &mut bus);
        cycles += bus.log.len() as u64;
        if cpu.registers().pc == pc || cycles > MAX_CYCLES {
            break cpu.registers().pc;
        }
    };
    assert_eq!(trap, SUCCESS, "trap at ${trap:04X} after {cycles} cycles");
}
