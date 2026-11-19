//! Unit tests of the CPU (test plan in `docs/design/cpu.md`).
//!
//! The tests use a [`Driver`] that runs numbered machine cycles. In each cycle `c`, the
//! driver sets the NMI and IRQ levels from its script. Then it calls `tick` with the RDY level
//! of the script, or it does not call `tick` if the script halts cycle `c` (ANTIC DMA). A
//! [`TestBus`] records each access with its cycle number.
//!
//! Unless a test says otherwise, the memory is full of `NOP` (`$EA`), the program starts at
//! `$0200` in cycle 1, the NMI handler is at `$4000` and the IRQ handler is at `$3000`.

use core::iter;
use core::ops::RangeInclusive;

use super::decode::{Op, TABLE, decode};
use super::flags::{B, C, D, I, N, U, V, Z};
use super::{Bus, Cpu, Registers, Step, Tick};

/// `NOP`.
const NOP: u8 = 0xEA;
/// The start address of the test programs.
const START: u16 = 0x0200;
/// The address of the NMI handler.
const NMI_HANDLER: u16 = 0x4000;
/// The address of the IRQ handler.
const IRQ_HANDLER: u16 = 0x3000;

/// One bus access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Access {
    /// The machine cycle.
    cycle: u32,
    /// The address.
    addr: u16,
    /// The value read or written.
    value: u8,
    /// `true` for a write.
    write: bool,
}

/// 64 KB of RAM that records each access.
struct TestBus {
    /// The memory.
    ram: Vec<u8>,
    /// The accesses.
    log: Vec<Access>,
    /// The number of the current machine cycle.
    now: u32,
}

impl Bus for TestBus {
    fn read(&mut self, addr: u16) -> u8 {
        let value = self.ram[usize::from(addr)];
        self.log.push(Access {
            cycle: self.now,
            addr,
            value,
            write: false,
        });
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.ram[usize::from(addr)] = value;
        self.log.push(Access {
            cycle: self.now,
            addr,
            value,
            write: true,
        });
    }
}

/// Runs the CPU in numbered machine cycles with a script for the NMI, IRQ and RDY lines and
/// for the halted cycles.
struct Driver {
    /// The CPU.
    cpu: Cpu,
    /// The bus.
    bus: TestBus,
    /// The number of the last cycle that ran (0 before the first cycle).
    cycle: u32,
    /// The cycles in which the NMI line is high.
    nmi: Vec<RangeInclusive<u32>>,
    /// The cycles in which the IRQ line is asserted.
    irq: Vec<RangeInclusive<u32>>,
    /// The cycles in which RDY is low.
    rdy_low: Vec<RangeInclusive<u32>>,
    /// The cycles that ANTIC uses (the CPU does not tick).
    halt: Vec<RangeInclusive<u32>>,
}

/// A script with one range of cycles, `first` to `last`.
fn span(first: u32, last: u32) -> Vec<RangeInclusive<u32>> {
    iter::once(first..=last).collect()
}

/// `true` if `cycle` is in one of `ranges`.
fn within(ranges: &[RangeInclusive<u32>], cycle: u32) -> bool {
    ranges.iter().any(|range| range.contains(&cycle))
}

impl Driver {
    /// A CPU after its reset sequence, at `START`, with the memory full of `NOP` and the
    /// vectors set. The registers are those after reset: `S` = `$FD`, `P` = `$34`.
    fn new() -> Self {
        let mut ram = vec![NOP; 0x1_0000];
        ram[0xFFFA..].copy_from_slice(&[0x00, 0x40, 0x00, 0x02, 0x00, 0x30]);
        let mut bus = TestBus {
            ram,
            log: Vec::new(),
            now: 0,
        };
        let mut cpu = Cpu::new();
        assert_eq!(cpu.step(&mut bus), Step::Reset, "power-on reset");
        bus.log.clear();
        Self {
            cpu,
            bus,
            cycle: 0,
            nmi: Vec::new(),
            irq: Vec::new(),
            rdy_low: Vec::new(),
            halt: Vec::new(),
        }
    }

    /// A driver with `code` at `START`.
    fn with_code(code: &[u8]) -> Self {
        let mut driver = Self::new();
        driver.load(START, code);
        driver
    }

    /// Copies `bytes` to memory at `addr`.
    fn load(&mut self, addr: u16, bytes: &[u8]) {
        let start = usize::from(addr);
        self.bus.ram[start..start + bytes.len()].copy_from_slice(bytes);
    }

    /// Sets one memory byte.
    fn set(&mut self, addr: u16, value: u8) {
        self.bus.ram[usize::from(addr)] = value;
    }

    /// One memory byte.
    fn byte(&self, addr: u16) -> u8 {
        self.bus.ram[usize::from(addr)]
    }

    /// The registers, for change before the first cycle.
    fn regs(&mut self) -> &mut Registers {
        self.cpu.registers_mut()
    }

    /// The registers.
    fn r(&self) -> Registers {
        *self.cpu.registers()
    }

    /// Runs the next machine cycle. Returns `None` for a halted cycle.
    fn cycle(&mut self) -> Option<Tick> {
        self.cycle += 1;
        let c = self.cycle;
        self.bus.now = c;
        self.cpu.set_nmi(within(&self.nmi, c));
        self.cpu.set_irq(within(&self.irq, c));
        if within(&self.halt, c) {
            return None;
        }
        Some(self.cpu.tick(&mut self.bus, !within(&self.rdy_low, c)))
    }

    /// Runs machine cycles up to and including cycle `last`.
    fn run_to(&mut self, last: u32) {
        while self.cycle < last {
            let _tick: Option<Tick> = self.cycle();
        }
    }

    /// Runs machine cycles until the CPU is at a boundary again. Returns the accesses.
    fn step(&mut self) -> Vec<Access> {
        let first = self.bus.log.len();
        loop {
            let tick = self.cycle();
            if tick.is_some() && self.cpu.at_boundary() {
                break;
            }
        }
        self.bus.log[first..].to_vec()
    }

    /// The cycles in which `addr` is read.
    fn reads(&self, addr: u16) -> Vec<u32> {
        self.bus
            .log
            .iter()
            .filter(|a| a.addr == addr && !a.write)
            .map(|a| a.cycle)
            .collect()
    }

    /// The writes as `(address, value)`.
    fn writes(&self) -> Vec<(u16, u8)> {
        self.bus
            .log
            .iter()
            .filter(|a| a.write)
            .map(|a| (a.addr, a.value))
            .collect()
    }

    /// The access in cycle `c`.
    fn at(&self, c: u32) -> Access {
        *self
            .bus
            .log
            .iter()
            .find(|a| a.cycle == c)
            .unwrap_or_else(|| panic!("no access in cycle {c}"))
    }

    /// The start cycles of the NMI sequences (the vector read is in cycle 6 of a sequence).
    fn nmi_starts(&self) -> Vec<u32> {
        self.reads(0xFFFA).iter().map(|c| c - 5).collect()
    }

    /// The start cycles of the IRQ and `BRK` sequences.
    fn irq_starts(&self) -> Vec<u32> {
        self.reads(0xFFFE).iter().map(|c| c - 5).collect()
    }
}

/// The addresses and directions of `log`.
fn pattern(log: &[Access]) -> Vec<(u16, bool)> {
    log.iter().map(|a| (a.addr, a.write)).collect()
}

/// Reads at `addrs`.
fn reads_at(addrs: &[u16]) -> Vec<(u16, bool)> {
    addrs.iter().map(|&a| (a, false)).collect()
}

/// A small deterministic random generator (xorshift).
struct Random(u32);

impl Random {
    /// The next random byte.
    const fn byte(&mut self) -> u8 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0.to_le_bytes()[1]
    }
}

/// Random memory without KIL opcodes, so random code keeps running.
fn random_memory(seed: u32) -> Vec<u8> {
    let mut random = Random(seed);
    (0..0x1_0000)
        .map(|_| {
            let value = random.byte();
            if value & 0x0F == 0x02 && value != 0x82 && value != 0xA2 && value != 0xC2 {
                NOP
            } else {
                value
            }
        })
        .collect()
}

// ----------------------------------------------------------------------------------------
// Instructions and bus cycles (REQ-100 to REQ-106)
// ----------------------------------------------------------------------------------------

/// Verifies REQ-100 (criterion 2): an indexed load keeps `C` and `D`.
#[test]
fn address_calculation_keeps_flags() {
    let mut d = Driver::with_code(&[0xBD, 0xF0, 0x20]); // LDA $20F0,X
    d.set(0x2170, 0x5A);
    d.regs().x = 0x80;
    d.regs().p = U | B | C | D;
    let log = d.step();
    assert_eq!(d.r().a, 0x5A, "A");
    assert_eq!(d.r().p & (C | D), C | D, "C and D");
    assert_eq!(log.last().map(|a| a.addr), Some(0x2170), "operand read");
}

/// Verifies REQ-100 (criteria 3 and 4): `BIT` sets `N`, `V` and `Z`.
#[test]
fn bit_sets_n_v_z() {
    for (a, m, flags) in [(0x0F, 0xC0, N | V | Z), (0x40, 0x40, V)] {
        let mut d = Driver::with_code(&[0x24, 0x10]); // BIT $10
        d.set(0x0010, m);
        d.regs().a = a;
        let _log = d.step();
        assert_eq!(d.r().p & (N | V | Z), flags, "flags for A = {a:02X}");
        assert_eq!(d.r().a, a, "A");
    }
}

/// Verifies REQ-101 (criterion 2): `TXA` reads `PC` and `PC + 1`.
#[test]
fn implied_dummy_read() {
    let mut d = Driver::with_code(&[0x8A]); // TXA
    let log = d.step();
    assert_eq!(pattern(&log), reads_at(&[0x0200, 0x0201]), "bus cycles");
}

/// A bus that counts the accesses.
struct CountingBus {
    /// The memory.
    ram: Vec<u8>,
    /// The number of accesses.
    count: u32,
}

impl Bus for CountingBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.count += 1;
        self.ram[usize::from(addr)]
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.count += 1;
        self.ram[usize::from(addr)] = value;
    }
}

/// Verifies REQ-101 (criterion 3): each tick makes exactly one access, also with interrupts
/// and RDY stops.
#[test]
fn one_access_per_tick() {
    let mut bus = CountingBus {
        ram: random_memory(1),
        count: 0,
    };
    let mut cpu = Cpu::new();
    let mut random = Random(7);
    for tick in 0..10_000 {
        let lines = random.byte();
        cpu.set_nmi(lines & 0x01 != 0);
        cpu.set_irq(lines & 0x02 != 0);
        let before = bus.count;
        let _tick: Tick = cpu.tick(&mut bus, lines & 0x0C != 0);
        assert_eq!(bus.count, before + 1, "tick {tick}");
    }
}

/// Verifies REQ-102 (criterion 1): `LDA abs,X` with a page cross reads the wrong page first.
#[test]
fn abs_x_read_page_cross_dummy_read() {
    let mut d = Driver::with_code(&[0xBD, 0xF0, 0x20]); // LDA $20F0,X
    d.regs().x = 0x80;
    let log = d.step();
    assert_eq!(
        pattern(&log),
        reads_at(&[0x0200, 0x0201, 0x0202, 0x2070, 0x2170]),
        "bus cycles"
    );
}

/// Verifies REQ-102 (criteria 2 and 3): `LDA abs,Y` and `LDA (zp),Y` with a page cross.
#[test]
fn abs_y_and_ind_y_read_page_cross_dummy_read() {
    let mut d = Driver::with_code(&[0xB9, 0xF0, 0x20]); // LDA $20F0,Y
    d.regs().y = 0x80;
    let log = d.step();
    assert_eq!(
        pattern(&log),
        reads_at(&[0x0200, 0x0201, 0x0202, 0x2070, 0x2170]),
        "abs,Y"
    );
    let mut d = Driver::with_code(&[0xB1, 0x40]); // LDA ($40),Y
    d.load(0x0040, &[0xF0, 0x20]);
    d.regs().y = 0x80;
    let log = d.step();
    assert_eq!(
        pattern(&log),
        reads_at(&[0x0200, 0x0201, 0x0040, 0x0041, 0x2070, 0x2170]),
        "(zp),Y"
    );
}

/// Verifies REQ-102 (criterion 4): no dummy read without a page cross.
#[test]
fn abs_x_read_no_page_cross() {
    let mut d = Driver::with_code(&[0xBD, 0x00, 0x20]); // LDA $2000,X
    d.regs().x = 0x10;
    let log = d.step();
    assert_eq!(
        pattern(&log),
        reads_at(&[0x0200, 0x0201, 0x0202, 0x2010]),
        "bus cycles"
    );
}

/// Verifies REQ-102 (criterion 5): a pointer at `$FF` adds no cycle.
#[test]
fn ind_y_pointer_ff_no_extra_cycle() {
    let mut d = Driver::with_code(&[0xB1, 0xFF]); // LDA ($FF),Y
    d.set(0x00FF, 0x00);
    d.set(0x0000, 0x20);
    d.regs().y = 0x10;
    let log = d.step();
    assert_eq!(log.len(), 5, "cycles");
    assert_eq!(log[4].addr, 0x2010, "operand read in cycle 5");
}

/// Verifies REQ-102 (criterion 6): zero page indexed modes have fixed cycle counts.
#[test]
fn zero_page_index_no_extra_cycle() {
    for (code, cycles) in [([0xB5, 0xF0], 4), ([0xB6, 0xF0], 4), ([0xA1, 0xF0], 6)] {
        let mut d = Driver::with_code(&code);
        d.regs().x = 0x20;
        d.regs().y = 0x20;
        assert_eq!(d.step().len(), cycles, "opcode {:02X}", code[0]);
    }
}

/// Verifies REQ-103 (criteria 1 and 3): indexed stores read the uncorrected address first.
#[test]
fn indexed_store_dummy_read_page_cross() {
    let mut d = Driver::with_code(&[0x9D, 0xF0, 0xD2]); // STA $D2F0,X
    d.regs().x = 0x20;
    d.regs().a = 0x77;
    let log = d.step();
    assert_eq!(
        pattern(&log),
        [
            (0x0200, false),
            (0x0201, false),
            (0x0202, false),
            (0xD210, false),
            (0xD310, true)
        ],
        "abs,X"
    );
    assert_eq!(log[4].value, 0x77, "value");
    let mut d = Driver::with_code(&[0x99, 0xF0, 0x20]); // STA $20F0,Y
    d.regs().y = 0x20;
    let log = d.step();
    assert_eq!(log[3].addr, 0x2010, "abs,Y dummy read");
    assert_eq!((log[4].addr, log[4].write), (0x2110, true), "abs,Y write");
}

/// Verifies REQ-103 (criterion 2): the dummy read also occurs without a page cross.
#[test]
fn indexed_store_dummy_read_same_page() {
    let mut d = Driver::with_code(&[0x9D, 0x00, 0x20]); // STA $2000,X
    d.regs().x = 0x01;
    let log = d.step();
    assert_eq!(log.len(), 5, "cycles");
    assert_eq!((log[3].addr, log[3].write), (0x2001, false), "dummy read");
    assert_eq!((log[4].addr, log[4].write), (0x2001, true), "write");
}

/// Verifies REQ-103 (criteria 4 and 5): `STA (zp),Y` takes 6 cycles with and without a page
/// cross.
#[test]
fn ind_y_store_six_cycles() {
    for (lo, dummy, target) in [(0x00, 0x2001, 0x2001), (0xF0, 0x2010, 0x2110)] {
        let mut d = Driver::with_code(&[0x91, 0x40]); // STA ($40),Y
        d.load(0x0040, &[lo, 0x20]);
        d.regs().y = if lo == 0 { 0x01 } else { 0x20 };
        let log = d.step();
        assert_eq!(log.len(), 6, "cycles");
        assert_eq!((log[4].addr, log[4].write), (dummy, false), "dummy read");
        assert_eq!((log[5].addr, log[5].write), (target, true), "write");
    }
}

/// Verifies REQ-104 (criterion 1): `INC abs` writes the old value, then the new value.
#[test]
fn rmw_writes_old_value_then_new_value() {
    let mut d = Driver::with_code(&[0xEE, 0x00, 0x06]); // INC $0600
    d.set(0x0600, 0x41);
    let log = d.step();
    let tail: Vec<(u16, u8, bool)> = log[3..]
        .iter()
        .map(|a| (a.addr, a.value, a.write))
        .collect();
    assert_eq!(log.len(), 6, "cycles");
    assert_eq!(
        tail,
        [
            (0x0600, 0x41, false),
            (0x0600, 0x41, true),
            (0x0600, 0x42, true)
        ],
        "cycles 4 to 6"
    );
}

/// Verifies REQ-105 (criterion 1): `zp,X` wraps in page zero.
#[test]
fn zero_page_x_wraps() {
    let mut d = Driver::with_code(&[0xB5, 0xF0]); // LDA $F0,X
    d.regs().x = 0x20;
    let log = d.step();
    assert_eq!(log[3].addr, 0x0010, "operand read");
}

/// Verifies REQ-105 (criterion 2): `zp,Y` wraps in page zero.
#[test]
fn zero_page_y_wraps() {
    let mut d = Driver::with_code(&[0xB6, 0xF0]); // LDX $F0,Y
    d.regs().y = 0x20;
    let log = d.step();
    assert_eq!(log[3].addr, 0x0010, "operand read");
}

/// Verifies REQ-105 (criterion 3): the `(zp,X)` pointer wraps from `$FF` to `$00`.
#[test]
fn ind_x_pointer_wraps() {
    let mut d = Driver::with_code(&[0xA1, 0xF0]); // LDA ($F0,X)
    d.regs().x = 0x0F;
    let log = d.step();
    assert_eq!(
        (log[3].addr, log[4].addr),
        (0x00FF, 0x0000),
        "pointer reads"
    );
}

/// Verifies REQ-105 (criterion 4): the `(zp),Y` pointer wraps from `$FF` to `$00`.
#[test]
fn ind_y_pointer_wraps_at_ff() {
    let mut d = Driver::with_code(&[0xB1, 0xFF]); // LDA ($FF),Y
    d.set(0x00FF, 0x34);
    d.set(0x0000, 0x12);
    d.set(0x0100, 0x56);
    d.regs().y = 0x00;
    let log = d.step();
    assert_eq!(
        pattern(&log),
        reads_at(&[0x0200, 0x0201, 0x00FF, 0x0000, 0x1234]),
        "bus cycles"
    );
}

/// Verifies REQ-105 (criterion 5): `JMP (abs)` reads the high byte from the same page.
#[test]
fn jmp_indirect_page_bug() {
    let mut d = Driver::with_code(&[0x6C, 0xFF, 0x30]); // JMP ($30FF)
    d.set(0x30FF, 0x00);
    d.set(0x3000, 0x40);
    d.set(0x3100, 0x50);
    let log = d.step();
    assert_eq!(
        (log[3].addr, log[4].addr),
        (0x30FF, 0x3000),
        "pointer reads"
    );
    assert_eq!(d.r().pc, 0x4000, "PC");
}

/// Verifies REQ-106 (criterion 1): the page of the target is compared with the page of the
/// next instruction.
#[test]
fn branch_page_cross_from_next_instruction() {
    let mut d = Driver::new();
    d.load(0x80FE, &[0x90, 0xC0]); // BCC $80C0
    d.regs().pc = 0x80FE;
    d.regs().p = U | B;
    let log = d.step();
    assert_eq!(
        pattern(&log),
        reads_at(&[0x80FE, 0x80FF, 0x8100, 0x81C0]),
        "bus cycles"
    );
    assert_eq!(d.r().pc, 0x80C0, "next opcode fetch");
}

/// Verifies REQ-106 (criterion 2): a taken branch in the same page takes 3 cycles.
#[test]
fn branch_same_page_three_cycles() {
    let mut d = Driver::new();
    d.load(0x80FE, &[0xF0, 0x10]); // BEQ $8110
    d.regs().pc = 0x80FE;
    d.regs().p = U | B | Z;
    let log = d.step();
    assert_eq!(
        pattern(&log),
        reads_at(&[0x80FE, 0x80FF, 0x8100]),
        "bus cycles"
    );
    assert_eq!(d.r().pc, 0x8110, "next opcode fetch");
}

/// Verifies REQ-106 (criterion 3): a branch not taken takes 2 cycles.
#[test]
fn branch_not_taken_two_cycles() {
    let mut d = Driver::new();
    d.load(0x80FE, &[0x90, 0xC0]); // BCC $80C0
    d.regs().pc = 0x80FE;
    d.regs().p = U | B | C;
    assert_eq!(d.step().len(), 2, "cycles");
    assert_eq!(d.r().pc, 0x8100, "next opcode fetch");
}

// ----------------------------------------------------------------------------------------
// Flags and decimal mode (REQ-107 to REQ-109)
// ----------------------------------------------------------------------------------------

/// Verifies REQ-107 (criterion 1): `PHP` pushes bits 4 and 5 as 1.
#[test]
fn php_pushes_bits_4_and_5() {
    // LDA #$00, PHA, PLP, PHP, PLA
    let mut d = Driver::with_code(&[0xA9, 0x00, 0x48, 0x28, 0x08, 0x68]);
    for _ in 0..5 {
        let _log = d.step();
    }
    assert_eq!(d.r().a, 0x30, "A");
}

/// Verifies REQ-107 (criterion 2): the IRQ sequence pushes `B` = 0.
#[test]
fn irq_pushes_b_clear() {
    let mut d = Driver::new();
    d.regs().p = 0xC1;
    d.irq = span(1, 100);
    d.run_to(9);
    assert_eq!(d.writes()[2], (0x01FB, 0xE1), "pushed P");
}

/// Verifies REQ-107 (criterion 3): the NMI sequence pushes `B` = 0.
#[test]
fn nmi_pushes_b_clear() {
    let mut d = Driver::new();
    d.regs().p = 0xC5;
    d.nmi = span(1, 100);
    d.run_to(9);
    assert_eq!(d.writes()[2], (0x01FB, 0xE5), "pushed P");
}

/// Verifies REQ-107 (criterion 4): `BRK` pushes `B` = 1.
#[test]
fn brk_pushes_b_set() {
    let mut d = Driver::with_code(&[0x00]);
    d.regs().p = 0xC1;
    let _log = d.step();
    assert_eq!(d.writes()[2], (0x01FB, 0xF1), "pushed P");
}

/// Runs `code` with `D` = 1 and the start values `a` and `carry`. Returns the registers.
fn decimal(code: &[u8], a: u8, carry: bool) -> Registers {
    let mut d = Driver::with_code(code);
    d.regs().a = a;
    d.regs().p = U | B | D | if carry { C } else { 0 };
    let _log = d.step();
    d.r()
}

/// Verifies REQ-108 (criterion 1).
#[test]
fn decimal_adc_0f_0f() {
    assert_eq!(decimal(&[0x69, 0x0F], 0x0F, false).a, 0x14, "A");
}

/// Verifies REQ-108 (criterion 2).
#[test]
fn decimal_adc_99_01_flags() {
    let r = decimal(&[0x69, 0x01], 0x99, false);
    assert_eq!(r.a, 0x00, "A");
    assert_eq!(r.p & (C | N), C | N, "C and N");
}

/// Verifies REQ-108 (criterion 3).
#[test]
fn decimal_adc_ff_01_zero() {
    let r = decimal(&[0x69, 0x01], 0xFF, false);
    assert_eq!(r.a, 0x66, "A");
    assert_eq!(r.p & Z, Z, "Z");
}

/// Verifies REQ-108 (criterion 4).
#[test]
fn decimal_sbc_borrow() {
    let r = decimal(&[0xE9, 0x01], 0x00, true);
    assert_eq!(r.a, 0x99, "A");
    assert_eq!(r.p & C, 0, "C");
}

/// Verifies REQ-108 (criterion 5).
#[test]
fn decimal_sbc_no_borrow() {
    let r = decimal(&[0xE9, 0x01], 0x10, true);
    assert_eq!(r.a, 0x09, "A");
    assert_eq!(r.p & C, C, "C");
}

/// Verifies REQ-108 (criterion 7): decimal mode adds no cycle.
#[test]
fn decimal_no_extra_cycle() {
    for (code, cycles) in [([0x69, 0x01], 2), ([0x65, 0x10], 3)] {
        let mut d = Driver::with_code(&code);
        d.regs().p = U | B | D;
        assert_eq!(d.step().len(), cycles, "opcode {:02X}", code[0]);
    }
}

/// Verifies REQ-109 (criterion 1): compare, increment and decrement ignore `D`.
#[test]
fn decimal_flag_ignored_by_compare_and_increment() {
    let values = [0x00, 0x09, 0x0F, 0x10, 0x99, 0xFF];
    // CMP #imm, CPX #imm, CPY #imm, INC $10, DEC $10, INX, DEX, INY, DEY.
    let codes = [0xC9, 0xE0, 0xC0, 0xE6, 0xC6, 0xE8, 0xCA, 0xC8, 0x88];
    for opcode in codes {
        for reg in values {
            for operand in values {
                assert_eq!(
                    run_with_p(opcode, reg, operand, U | B),
                    run_with_p(opcode, reg, operand, U | B | D),
                    "{opcode:02X} {reg:02X} {operand:02X}"
                );
            }
        }
    }
}

/// Runs `opcode` with `A` = `X` = `Y` = `reg` and the operand `operand` (immediate, or at
/// `$10`). Returns the registers without `D` and the byte at `$10`.
fn run_with_p(opcode: u8, reg: u8, operand: u8, p: u8) -> (Registers, u8) {
    let immediate = matches!(opcode, 0xC9 | 0xE0 | 0xC0);
    let second = if immediate { operand } else { 0x10 };
    let mut d = Driver::with_code(&[opcode, second]);
    d.set(0x0010, operand);
    let r = d.regs();
    r.a = reg;
    r.x = reg;
    r.y = reg;
    r.p = p;
    let _log = d.step();
    let mut r = d.r();
    r.p &= !D;
    (r, d.byte(0x0010))
}

/// Verifies REQ-109 (criterion 3): the IRQ, NMI and `BRK` sequences keep `D`.
#[test]
fn interrupt_keeps_decimal_flag() {
    for kind in 0..3 {
        let mut d = Driver::with_code(&[if kind == 2 { 0x00 } else { NOP }]);
        d.regs().p = U | B | D;
        if kind == 0 {
            d.irq = span(1, 100);
        } else if kind == 1 {
            d.nmi = span(1, 100);
        }
        let start = if kind == 2 { 1 } else { 3 };
        d.run_to(start + 6);
        assert_eq!(d.r().p & (D | I), D | I, "D after sequence {kind}");
        assert_eq!(d.writes()[2].1 & D, D, "pushed D in sequence {kind}");
    }
}

/// Verifies REQ-109 (criterion 4) and REQ-124 (criterion 3): reset keeps `D`.
#[test]
fn reset_keeps_decimal_flag() {
    for p in [U | B | D, U | B] {
        let mut d = Driver::new();
        d.regs().p = p;
        d.cpu.reset();
        d.run_to(7);
        assert_eq!(d.r().p & D, p & D, "D");
    }
}

// ----------------------------------------------------------------------------------------
// Undocumented and KIL opcodes (REQ-111 to REQ-113)
// ----------------------------------------------------------------------------------------

/// Verifies REQ-111 (criterion 1): `ISB` subtracts the incremented value.
#[test]
fn isb_uses_inc_result() {
    let mut d = Driver::with_code(&[0xE7, 0x10]); // ISB $10
    d.set(0x0010, 0x41);
    d.regs().a = 0x50;
    d.regs().p = U | B | C;
    assert_eq!(d.step().len(), 5, "cycles");
    assert_eq!(d.byte(0x0010), 0x42, "memory");
    assert_eq!(d.r().a, 0x0E, "A");
    assert_eq!(d.r().p & C, C, "C");
}

/// Verifies REQ-111 (criterion 2): `LAX` loads A and X.
#[test]
fn lax_loads_a_and_x() {
    let mut d = Driver::with_code(&[0xA7, 0x10]); // LAX $10
    d.set(0x0010, 0x80);
    let _log = d.step();
    assert_eq!((d.r().a, d.r().x), (0x80, 0x80), "A and X");
    assert_eq!(d.r().p & (N | Z), N, "N and Z");
}

/// Verifies REQ-111 (criterion 3): `SAX` stores `A & X` and keeps `P`.
#[test]
fn sax_keeps_flags() {
    let mut d = Driver::with_code(&[0x87, 0x10]); // SAX $10
    d.regs().a = 0xF0;
    d.regs().x = 0x3C;
    let p = d.r().p;
    let _log = d.step();
    assert_eq!(d.byte(0x0010), 0x30, "memory");
    assert_eq!(d.r().p, p, "P");
}

/// Verifies REQ-111 (criteria 4 and 5): `NOP abs,X` reads like `LDA abs,X`.
#[test]
fn nop_abs_x_page_cross() {
    let mut d = Driver::with_code(&[0x1C, 0xF0, 0x20]); // NOP $20F0,X
    d.regs().x = 0x80;
    let before = d.r();
    let log = d.step();
    assert_eq!(
        pattern(&log),
        reads_at(&[0x0200, 0x0201, 0x0202, 0x2070, 0x2170]),
        "bus cycles"
    );
    assert_eq!(
        d.r(),
        Registers {
            pc: 0x0203,
            ..before
        },
        "registers"
    );
    let mut d = Driver::with_code(&[0x1C, 0x00, 0x20]); // NOP $2000,X
    d.regs().x = 0x10;
    assert_eq!(d.step().len(), 4, "cycles without page cross");
}

/// Verifies REQ-112 (criterion 2): `SHX` stores `X & (H + 1)`.
#[test]
fn shx_and_high_byte_plus_one() {
    let mut d = Driver::with_code(&[0x9E, 0x00, 0x20]); // SHX $2000,Y
    d.regs().x = 0x0F;
    d.regs().y = 0x01;
    let _log = d.step();
    assert_eq!(d.writes(), [(0x2001, 0x01)], "write");
}

/// Verifies REQ-112 (criterion 3): a page cross puts the value on the high address lines.
#[test]
fn shx_page_cross_replaces_high_byte() {
    let mut d = Driver::with_code(&[0x9E, 0xF0, 0x20]); // SHX $20F0,Y
    d.regs().x = 0x0F;
    d.regs().y = 0x20;
    let _log = d.step();
    assert_eq!(d.writes(), [(0x0110, 0x01)], "write");
}

/// Verifies REQ-112 (criterion 4): `SHS` sets `S = A & X`.
#[test]
fn shs_sets_s() {
    let mut d = Driver::with_code(&[0x9B, 0x00, 0x20]); // SHS $2000,Y
    d.regs().a = 0xF0;
    d.regs().x = 0x3C;
    d.regs().y = 0x01;
    let _log = d.step();
    assert_eq!(d.r().s, 0x30, "S");
    assert_eq!(d.writes(), [(0x2001, 0x20)], "write");
}

/// A driver with a KIL opcode at `START` that has run the 11 cycles of the
/// `SingleStepTests` pattern.
fn jammed() -> Driver {
    let mut d = Driver::with_code(&[0x02]);
    d.run_to(11);
    d
}

/// Verifies REQ-113 (criterion 2): after a KIL opcode, each tick reads `$FFFF`.
#[test]
fn kil_reads_ffff_forever() {
    let mut d = jammed();
    let first: Vec<u16> = d.bus.log.iter().map(|a| a.addr).collect();
    assert_eq!(
        first[..6],
        [0x0200, 0x0201, 0xFFFF, 0xFFFE, 0xFFFE, 0xFFFF],
        "KIL pattern"
    );
    assert!(d.cpu.is_jammed(), "jammed");
    let regs = d.r();
    d.bus.log.clear();
    d.run_to(1011);
    assert_eq!(d.bus.log.len(), 1000, "accesses");
    assert!(
        d.bus.log.iter().all(|a| a.addr == 0xFFFF && !a.write),
        "reads of $FFFF"
    );
    assert_eq!(d.r(), regs, "registers");
    assert_eq!(regs.pc, 0x0201, "PC");
}

/// Verifies REQ-113 (criterion 3): the KIL state ignores NMI and IRQ.
#[test]
fn kil_ignores_interrupts() {
    let mut d = jammed();
    d.regs().p = U | B;
    d.nmi = span(100, 1011);
    d.irq = span(12, 1011);
    d.bus.log.clear();
    d.run_to(1011);
    assert!(
        d.bus.log.iter().all(|a| a.addr == 0xFFFF && !a.write),
        "no interrupt sequence"
    );
    assert_eq!(d.cpu.step(&mut d.bus), Step::Jammed, "step");
}

/// Verifies REQ-113 (criterion 5) and REQ-124 (criterion 4): `reset` ends the KIL state.
#[test]
fn reset_ends_kil() {
    let mut d = jammed();
    d.load(0xFFFC, &[0x00, 0xC0]);
    d.load(0xC000, &[0xA9, 0x42]); // LDA #$42
    d.cpu.reset();
    d.run_to(18);
    assert!(!d.cpu.is_jammed(), "not jammed");
    let log = d.step();
    assert_eq!((log[0].cycle, log[0].addr), (19, 0xC000), "opcode fetch");
    assert_eq!(d.r().a, 0x42, "A");
}

// ----------------------------------------------------------------------------------------
// Interrupts (REQ-114 to REQ-123)
// ----------------------------------------------------------------------------------------

/// Verifies REQ-114 (criteria 1 and 2).
#[test]
fn brk_pushes_and_sets_i() {
    let mut d = Driver::new();
    d.set(0x8B82, 0x00);
    d.load(0xFFFE, &[0x34, 0x12]);
    let r = d.regs();
    r.pc = 0x8B82;
    r.s = 0x51;
    r.p = 0x6A;
    assert_eq!(d.step().len(), 7, "cycles");
    assert_eq!(
        d.writes(),
        [(0x0151, 0x8B), (0x0150, 0x84), (0x014F, 0x7A)],
        "pushes"
    );
    assert_eq!((d.r().pc, d.r().s), (0x1234, 0x4E), "PC and S");
    assert_eq!(d.r().p & 0xCF, 0x6E & 0xCF, "I set, other flags kept");
}

/// Verifies REQ-115 (criteria 1 and 2).
#[test]
fn irq_sequence_seven_cycles() {
    let mut d = Driver::new();
    let r = d.regs();
    r.p = 0x20;
    r.s = 0xFF;
    d.irq = span(1, 100);
    d.run_to(10);
    assert_eq!(
        pattern(&d.bus.log[2..]),
        [
            (0x0201, false),
            (0x0201, false),
            (0x01FF, true),
            (0x01FE, true),
            (0x01FD, true),
            (0xFFFE, false),
            (0xFFFF, false),
            (0x3000, false)
        ],
        "cycles 3 to 10"
    );
    assert_eq!(
        d.writes(),
        [(0x01FF, 0x02), (0x01FE, 0x01), (0x01FD, 0x20)],
        "pushes"
    );
    assert_eq!((d.r().pc, d.r().s), (0x3001, 0xFC), "PC and S");
    assert_eq!(d.r().p & I, I, "I");
}

/// Verifies REQ-115 (criterion 3): `I` = 1 masks the IRQ.
#[test]
fn irq_masked_by_i() {
    let mut d = Driver::new();
    d.irq = span(1, 100);
    d.run_to(100);
    assert!(d.writes().is_empty(), "no write");
}

/// Verifies REQ-115 (criterion 4): an IRQ that ends before `CLI` is lost.
#[test]
fn irq_removed_before_cli_is_lost() {
    let mut d = Driver::new();
    d.set(0x020A, 0x58); // CLI after 10 NOPs
    d.irq = span(1, 20);
    d.run_to(42);
    assert!(d.writes().is_empty(), "no write");
    assert_eq!(d.r().p & I, 0, "CLI ran");
}

/// Verifies REQ-115 (criterion 5): the IRQ repeats while the line is asserted.
#[test]
fn irq_repeats_while_asserted() {
    let mut d = Driver::new();
    d.set(IRQ_HANDLER, 0x40); // RTI
    d.regs().p = 0x20;
    d.regs().s = 0xFF;
    d.irq = span(1, 1000);
    d.run_to(50);
    assert_eq!(d.irq_starts(), [3, 16, 29, 42], "IRQ sequences");
    assert!(d.reads(0x0202).is_empty(), "NOP at $0201 not executed");
}

/// Verifies REQ-116 (criterion 1): NMI ignores `I` and uses `$FFFA`.
#[test]
fn nmi_ignores_i_and_uses_fffa() {
    let mut d = Driver::new();
    d.regs().p = 0x24;
    d.regs().s = 0xFF;
    d.nmi = span(1, 1000);
    d.run_to(9);
    assert_eq!(
        d.writes(),
        [(0x01FF, 0x02), (0x01FE, 0x01), (0x01FD, 0x24)],
        "pushes"
    );
    assert_eq!(
        (d.at(8).addr, d.at(9).addr),
        (0xFFFA, 0xFFFB),
        "vector reads"
    );
    assert_eq!(d.r().pc, NMI_HANDLER, "PC");
    assert_eq!(d.r().p & I, I, "I");
}

/// Verifies REQ-116 (criterion 2): a held NMI level gives one interrupt.
#[test]
fn nmi_level_held_one_interrupt() {
    let mut d = Driver::new();
    d.nmi = span(1, 1000);
    d.run_to(1000);
    assert_eq!(d.nmi_starts(), [3], "NMI sequences");
}

/// Verifies REQ-116 (criterion 3): a second edge before the vector selection is merged.
#[test]
fn nmi_two_edges_one_interrupt() {
    let mut d = Driver::new();
    d.nmi = vec![1..=1, 3..=1000];
    d.run_to(1000);
    assert_eq!(d.nmi_starts(), [3], "NMI sequences");
}

/// Verifies REQ-117 (criterion 1): `INC abs,X` (7 cycles) runs to its end.
#[test]
fn nmi_waits_for_seven_cycle_instruction() {
    let mut d = Driver::with_code(&[0xFE, 0x00, 0x10]); // INC $1000,X
    d.nmi = span(1, 1000);
    d.run_to(20);
    assert_eq!(d.nmi_starts(), [8], "NMI sequence");
}

/// Verifies REQ-117 (criterion 2): `SLO (zp),Y` (8 cycles) runs to its end.
#[test]
fn nmi_waits_for_eight_cycle_rmw() {
    let mut d = Driver::with_code(&[0x13, 0x40]); // SLO ($40),Y
    d.nmi = span(1, 1000);
    d.run_to(20);
    assert_eq!(d.nmi_starts(), [9], "NMI sequence");
}

/// Verifies REQ-117 (criterion 3): the same for the IRQ line.
#[test]
fn irq_waits_for_long_instructions() {
    for (code, start) in [(&[0xFE_u8, 0x00, 0x10][..], 8), (&[0x13, 0x40][..], 9)] {
        let mut d = Driver::with_code(code);
        d.regs().p = U | B;
        d.irq = span(1, 1000);
        d.run_to(20);
        assert_eq!(
            d.irq_starts(),
            [start],
            "IRQ sequence after {:02X}",
            code[0]
        );
    }
}

/// Verifies REQ-118 (criterion 1): a line asserted in the first cycle of a `NOP` starts the
/// sequence after the `NOP`.
#[test]
fn nmi_two_cycle_recognition_delay() {
    let mut d = Driver::new();
    d.nmi = span(5, 1000);
    d.run_to(30);
    assert_eq!(d.nmi_starts(), [7], "NMI sequence");
}

/// Verifies REQ-118 (criterion 2): a line asserted in the last cycle runs one more
/// instruction.
#[test]
fn nmi_in_last_cycle_runs_next_instruction() {
    let mut d = Driver::new();
    d.nmi = span(6, 1000);
    d.run_to(30);
    assert_eq!(d.nmi_starts(), [9], "NMI sequence");
}

/// Verifies REQ-118 (criterion 3): the same for the IRQ line.
#[test]
fn irq_two_cycle_recognition_delay() {
    for (first, start) in [(5, 7), (6, 9)] {
        let mut d = Driver::new();
        d.regs().p = U | B;
        d.irq = span(first, 1000);
        d.run_to(30);
        assert_eq!(d.irq_starts(), [start], "IRQ asserted in cycle {first}");
    }
}

/// A driver with `I` = 1, the IRQ line asserted from cycle 1, `code` at `START`.
fn pending_irq(code: &[u8]) -> Driver {
    let mut d = Driver::with_code(code);
    d.irq = span(1, 1000);
    d
}

/// The return address of the first interrupt sequence.
fn pushed_return(d: &Driver) -> u16 {
    let w = d.writes();
    u16::from_le_bytes([w[1].1, w[0].1])
}

/// Verifies REQ-119 (criterion 1): `CLI` delays the IRQ by one instruction.
#[test]
fn cli_delays_irq_one_instruction() {
    let mut d = pending_irq(&[0x58]); // CLI
    d.run_to(30);
    assert_eq!(d.irq_starts(), [5], "IRQ sequence");
    assert_eq!(pushed_return(&d), 0x0202, "return address");
}

/// Verifies REQ-119 (criterion 2): `PLP` delays the IRQ by one instruction.
#[test]
fn plp_delays_irq_one_instruction() {
    let mut d = pending_irq(&[0x28]); // PLP
    d.regs().s = 0xFE;
    d.set(0x01FF, 0x20);
    d.run_to(30);
    assert_eq!(d.irq_starts(), [7], "IRQ sequence");
    assert_eq!(pushed_return(&d), 0x0202, "return address");
}

/// Verifies REQ-119 (criterion 3): `RTI` that clears `I` allows the IRQ at once.
#[test]
fn rti_irq_immediate() {
    let mut d = pending_irq(&[0x40]); // RTI
    d.regs().s = 0xFC;
    d.load(0x01FD, &[0x20, 0x00, 0x04]);
    d.run_to(30);
    assert_eq!(d.irq_starts(), [7], "IRQ sequence");
    assert_eq!(pushed_return(&d), 0x0400, "return address");
}

/// Verifies REQ-120 (criterion 1): `CLI`+`SEI` dispatches the IRQ with `I` = 1 pushed.
#[test]
fn cli_sei_irq_with_i_set() {
    let mut d = pending_irq(&[0x58, 0x78]); // CLI, SEI
    d.run_to(30);
    assert_eq!(d.irq_starts(), [5], "IRQ sequence");
    assert_eq!(pushed_return(&d), 0x0202, "return address");
    assert_eq!(d.writes()[2].1 & I, I, "pushed I");
}

/// Verifies REQ-120 (criterion 2): `CLI`+`PLP` dispatches the IRQ with `I` = 1 pushed.
#[test]
fn cli_plp_irq_with_i_set() {
    let mut d = pending_irq(&[0x58, 0x28]); // CLI, PLP
    d.regs().s = 0xFE;
    d.set(0x01FF, 0x24);
    d.run_to(30);
    assert_eq!(d.irq_starts(), [7], "IRQ sequence");
    assert_eq!(pushed_return(&d), 0x0202, "return address");
    assert_eq!(d.writes()[2].1 & I, I, "pushed I");
}

/// Verifies REQ-120 (criterion 3, the pairs `CLI`+`RTI` and `PLP`+`RTI`): no IRQ is
/// dispatched. The pairs `PLP`+`SEI` and `PLP`+`PLP` are not in this test: the poll rule of
/// `cpu.md` dispatches the IRQ for them (`cpu.md`, open issue 2).
#[test]
fn pairs_without_irq_dispatch() {
    // CLI, RTI (pulls P = $24, returns to $0300).
    let mut d = pending_irq(&[0x58, 0x40]);
    d.regs().s = 0xFC;
    d.load(0x01FD, &[0x24, 0x00, 0x03]);
    d.run_to(40);
    assert!(d.irq_starts().is_empty(), "CLI+RTI");
    // PLP (pulls $20), RTI (pulls P = $24, returns to $0300).
    let mut d = pending_irq(&[0x28, 0x40]);
    d.regs().s = 0xFB;
    d.load(0x01FC, &[0x20, 0x24, 0x00, 0x03]);
    d.run_to(40);
    assert!(d.irq_starts().is_empty(), "PLP+RTI");
}

/// Verifies REQ-120 (criterion 4): after an `RTI` that clears `I`, the IRQ comes before the
/// next instruction.
#[test]
fn rti_pairs_irq_after_rti() {
    for second in [0x78, 0x40, 0x28] {
        let mut d = pending_irq(&[0x40]); // RTI to $0400
        d.regs().s = 0xFC;
        d.load(0x01FD, &[0x20, 0x00, 0x04]);
        d.set(0x0400, second);
        d.run_to(30);
        assert_eq!(d.irq_starts(), [7], "IRQ sequence before {second:02X}");
        assert_eq!(pushed_return(&d), 0x0400, "return address");
        assert_eq!(d.writes()[2].1 & I, 0, "pushed I");
    }
}

/// A driver for the branch tests. `nmi` selects the NMI line, else the IRQ line with `I` = 0.
fn branch_driver(code: &[u8], at: u16, p: u8, line: u32, nmi: bool) -> Driver {
    let mut d = Driver::new();
    d.load(at, code);
    d.regs().pc = at;
    d.regs().p = U | B | p;
    if nmi {
        d.nmi = span(line, 1000);
    } else {
        d.regs().p &= !I;
        d.irq = span(line, 1000);
    }
    d.run_to(40);
    d
}

/// The first sequence start of the line that `branch_driver` used.
fn sequence_start(d: &Driver, nmi: bool) -> u32 {
    let starts = if nmi { d.nmi_starts() } else { d.irq_starts() };
    starts[0]
}

/// Verifies REQ-121 (criterion 1): a taken branch in the same page delays the NMI.
#[test]
fn taken_branch_delays_nmi() {
    let d = branch_driver(&[0xD0, 0x02], START, 0, 2, true);
    assert_eq!(sequence_start(&d, true), 6, "NMI sequence");
}

/// Verifies REQ-121 (criterion 2): a line asserted in the first cycle of the branch is not
/// delayed.
#[test]
fn taken_branch_early_nmi_no_delay() {
    let d = branch_driver(&[0xD0, 0x02], START, 0, 1, true);
    assert_eq!(sequence_start(&d, true), 4, "NMI sequence");
}

/// Verifies REQ-121 (criterion 3): a branch with a page cross is not delayed.
#[test]
fn page_cross_branch_no_delay() {
    let d = branch_driver(&[0xD0, 0x10], 0x02FD, 0, 3, true);
    assert_eq!(sequence_start(&d, true), 5, "NMI sequence");
}

/// Verifies REQ-121 (criterion 4): a branch not taken is not delayed.
#[test]
fn not_taken_branch_no_delay() {
    let d = branch_driver(&[0xD0, 0x02], START, Z, 1, true);
    assert_eq!(sequence_start(&d, true), 3, "NMI sequence");
}

/// Runs the jump instructions of REQ-121 criterion 5 with the NMI or IRQ line. The pulled
/// `P` of `RTI` has `I` = 0.
fn check_jumps(nmi: bool) {
    let cases: [(&[u8], u32, u32); 4] = [
        (&[0x4C, 0x00, 0x03], 2, 4), // JMP $0300
        (&[0x20, 0x00, 0x03], 5, 7), // JSR $0300
        (&[0x60], 5, 7),             // RTS
        (&[0x40], 5, 7),             // RTI
    ];
    for (code, line, start) in cases {
        let mut d = Driver::new();
        d.load(START, code);
        d.regs().s = 0xFC;
        d.load(0x01FD, &[0x20, 0xFF, 0x02]);
        if nmi {
            d.nmi = span(line, 1000);
        } else {
            d.regs().p = U | B;
            d.irq = span(line, 1000);
        }
        d.run_to(40);
        assert_eq!(
            sequence_start(&d, nmi),
            start,
            "sequence after {:02X}",
            code[0]
        );
    }
}

/// Verifies REQ-121 (criterion 5): `JMP`, `JSR`, `RTS` and `RTI` are not delayed.
#[test]
fn jump_instructions_no_delay() {
    check_jumps(true);
}

/// Verifies REQ-121 (criterion 6): the same results for the IRQ line.
#[test]
fn taken_branch_delays_irq() {
    let cases = [
        (&[0xD0_u8, 0x02][..], START, 0, 2, 6),
        (&[0xD0, 0x02][..], START, 0, 1, 4),
        (&[0xD0, 0x10][..], 0x02FD, 0, 3, 5),
        (&[0xD0, 0x02][..], START, Z, 1, 3),
    ];
    for (code, at, p, line, start) in cases {
        let d = branch_driver(code, at, p, line, false);
        assert_eq!(sequence_start(&d, false), start, "IRQ asserted in {line}");
    }
    check_jumps(false);
}

/// A driver with an IRQ sequence in cycles 3 to 9 (IRQ handler `LSR $5000`) and the NMI
/// line high from cycle `n`.
fn irq_then_nmi(n: u32) -> Driver {
    let mut d = Driver::new();
    d.load(IRQ_HANDLER, &[0x4E, 0x00, 0x50]); // LSR $5000
    d.regs().p = U | B;
    d.irq = span(1, 2000);
    d.nmi = span(n, 2000);
    d.run_to(1010);
    d
}

/// Verifies REQ-122 (criterion 1): S = N-5, the NMI comes after the first handler
/// instruction.
#[test]
fn nmi_after_irq_handler_first_instruction() {
    let d = irq_then_nmi(8);
    assert_eq!(d.irq_starts(), [3], "IRQ sequence");
    assert_eq!(d.at(10).addr, IRQ_HANDLER, "LSR starts in N+2");
    assert_eq!(d.nmi_starts(), [16], "NMI sequence in N+8");
}

/// Verifies REQ-122 (criterion 2): S = N-4, the NMI is lost.
#[test]
fn nmi_lost_when_irq_starts_four_cycles_before() {
    let d = irq_then_nmi(7);
    assert_eq!(d.irq_starts(), [3], "IRQ sequence");
    assert!(d.nmi_starts().is_empty(), "no NMI sequence");
}

/// Verifies REQ-122 (criterion 3): S = N-3 to N+1, the NMI hijacks the IRQ sequence.
#[test]
fn nmi_hijacks_irq_sequence() {
    for n in 2..=6 {
        let d = irq_then_nmi(n);
        assert_eq!(d.nmi_starts(), [3], "NMI vector for N = {n}");
        assert_eq!(d.at(9).addr, 0xFFFB, "vector high byte for N = {n}");
        assert!(d.irq_starts().is_empty(), "no IRQ vector for N = {n}");
        assert_eq!(d.writes()[2].1 & B, 0, "pushed B for N = {n}");
    }
}

/// Verifies REQ-122 (criterion 4): the NMI hijacks a `BRK` that starts in cycle 7, and the
/// pushed `B` stays 1.
#[test]
fn nmi_hijacks_brk_with_b_set() {
    let run = |n: u32| {
        let mut d = Driver::with_code(&[NOP, NOP, NOP, 0x00]);
        d.load(IRQ_HANDLER, &[0x4E, 0x00, 0x50]); // LSR $5000
        d.nmi = span(n, 2000);
        d.run_to(1010);
        d
    };
    let d = run(12);
    assert_eq!(d.irq_starts(), [7], "BRK, S = N-5");
    assert_eq!(d.nmi_starts(), [20], "NMI, S = N-5");
    let d = run(11);
    assert_eq!(d.irq_starts(), [7], "BRK, S = N-4");
    assert!(d.nmi_starts().is_empty(), "no NMI, S = N-4");
    for n in 6..=10 {
        let d = run(n);
        assert_eq!(d.nmi_starts(), [7], "NMI vector for N = {n}");
        assert_eq!(d.at(13).addr, 0xFFFB, "vector high byte for N = {n}");
        assert!(d.irq_starts().is_empty(), "no IRQ vector for N = {n}");
        assert_eq!(d.writes()[2].1 & B, B, "pushed B for N = {n}");
    }
}

/// Verifies REQ-123 (criterion 1): the first handler instruction always runs.
#[test]
fn handler_first_instruction_always_runs() {
    let mut d = Driver::new();
    d.regs().p = U | B;
    d.irq = span(1, 1000);
    d.nmi = span(9, 1000);
    d.run_to(30);
    assert_eq!(d.irq_starts(), [3], "IRQ sequence");
    assert_eq!(
        pattern(&d.bus.log[9..11]),
        reads_at(&[0x3000, 0x3001]),
        "NOP"
    );
    assert_eq!(d.nmi_starts(), [12], "NMI sequence");
}

/// Verifies REQ-123 (criterion 2): a second NMI edge in cycle 7 of an NMI sequence comes after
/// the first handler instruction.
#[test]
fn nmi_handler_first_instruction_before_second_nmi() {
    let mut d = Driver::new();
    d.nmi = vec![1..=7, 9..=1000];
    d.run_to(30);
    assert_eq!(d.nmi_starts(), [3, 12], "NMI sequences");
    assert_eq!(
        pattern(&d.bus.log[9..11]),
        reads_at(&[0x4000, 0x4001]),
        "NOP"
    );
}

/// Verifies REQ-123 (criterion 3): a `BRK` as the first IRQ handler instruction is hijacked.
#[test]
fn nmi_hijacks_brk_as_first_handler_instruction() {
    let mut d = Driver::new();
    d.set(IRQ_HANDLER, 0x00); // BRK
    d.regs().p = U | B;
    d.irq = span(1, 1000);
    d.nmi = span(9, 1000);
    d.run_to(16);
    let w = d.writes();
    assert_eq!((w[3].1, w[4].1), (0x30, 0x02), "return address $3002");
    assert_eq!(w[5].1 & B, B, "pushed B");
    assert_eq!(
        (d.at(15).addr, d.at(16).addr),
        (0xFFFA, 0xFFFB),
        "NMI vector"
    );
}

/// Runs cycles up to `last` and returns the cycles whose tick set `irq_sequence_started`.
fn irq_start_signals(d: &mut Driver, last: u32) -> Vec<u32> {
    let mut signals = Vec::new();
    while d.cycle < last {
        if d.cycle().is_some() && d.cpu.irq_sequence_started() {
            signals.push(d.cycle);
        }
    }
    signals
}

/// Verifies REQ-335: the CPU signals the first cycle of each IRQ and `BRK` sequence, but not
/// of an NMI sequence, and not in a stalled tick.
#[test]
fn irq_sequence_start_signal() {
    // IRQ: the signal is in the first cycle of the sequence.
    let mut irq = Driver::new();
    irq.regs().p = 0x20;
    irq.irq = span(1, 3);
    let signals = irq_start_signals(&mut irq, 20);
    assert_eq!(signals, irq.irq_starts(), "IRQ sequence");
    assert_eq!(signals.len(), 1, "one IRQ sequence");
    // BRK: the signal is in the opcode fetch.
    let mut brk = Driver::with_code(&[0x00, 0x00]);
    let signals = irq_start_signals(&mut brk, 10);
    assert_eq!(signals, [1], "BRK opcode fetch");
    assert_eq!(brk.irq_starts(), [1], "BRK sequence");
    // NMI: no signal.
    let mut nmi = Driver::new();
    nmi.nmi = span(1, 100);
    assert!(irq_start_signals(&mut nmi, 20).is_empty(), "NMI sequence");
    assert_eq!(nmi.nmi_starts().len(), 1, "one NMI sequence");
    // RDY low in the opcode fetch of BRK: the stalled ticks give no signal.
    let mut stalled = Driver::with_code(&[0x00, 0x00]);
    stalled.rdy_low = span(1, 4);
    assert_eq!(
        irq_start_signals(&mut stalled, 12),
        [5],
        "BRK opcode fetch after RDY low"
    );
}

// ----------------------------------------------------------------------------------------
// Reset and power-on (REQ-124, REQ-128)
// ----------------------------------------------------------------------------------------

/// A driver that calls `reset` before cycle 1, with `S` = `s` and the reset vector `$C000`.
fn after_reset(s: u8) -> Driver {
    let mut d = Driver::new();
    d.load(0xFFFC, &[0x00, 0xC0]);
    d.regs().s = s;
    d.cpu.reset();
    d.run_to(8);
    d
}

/// Verifies REQ-124 (criterion 1).
#[test]
fn reset_loads_fffc_in_seven_cycles() {
    let d = after_reset(0xFF);
    assert!(d.writes().is_empty(), "no write");
    assert_eq!(
        (d.at(6).addr, d.at(7).addr),
        (0xFFFC, 0xFFFD),
        "vector reads"
    );
    assert_eq!(d.at(8).addr, 0xC000, "opcode fetch");
}

/// Verifies REQ-124 (criterion 2).
#[test]
fn reset_decrements_s_by_three_no_writes() {
    let d = after_reset(0xFF);
    assert_eq!(
        pattern(&d.bus.log[2..5]),
        reads_at(&[0x01FF, 0x01FE, 0x01FD]),
        "stack reads"
    );
    assert_eq!(d.r().s, 0xFC, "S");
    assert_eq!(after_reset(0x01).r().s, 0xFE, "S wraps");
}

/// Verifies REQ-124 (criterion 3).
#[test]
fn reset_sets_i_keeps_d() {
    for p in [U | B | D, U | B] {
        let mut d = Driver::new();
        d.regs().p = p;
        d.cpu.reset();
        d.run_to(7);
        assert_eq!(d.r().p, p | I, "P");
    }
}

/// Verifies REQ-124 (criterion 5): `reset` clears a pending NMI edge.
#[test]
fn reset_clears_pending_nmi() {
    let mut d = Driver::new();
    d.nmi = span(1, 2000);
    d.run_to(1);
    d.cpu.reset();
    d.run_to(1008);
    assert!(d.nmi_starts().is_empty(), "no NMI sequence");
}

/// Verifies REQ-128 (criteria 1 and 2).
#[test]
fn power_on_registers() {
    let mut cpu = Cpu::new();
    let zero = Registers {
        a: 0,
        x: 0,
        y: 0,
        s: 0,
        p: 0x30,
        pc: 0,
    };
    assert_eq!(*cpu.registers(), zero, "before the first tick");
    assert!(cpu.at_boundary(), "at boundary");
    let mut bus = TestBus {
        ram: vec![0; 0x1_0000],
        log: Vec::new(),
        now: 0,
    };
    for _ in 0..7 {
        assert_eq!(cpu.tick(&mut bus, true), Tick::Ran, "tick");
    }
    let r = *cpu.registers();
    assert_eq!(
        (r.a, r.x, r.y, r.s, r.p),
        (0, 0, 0, 0xFD, 0x34),
        "after reset"
    );
    assert!(cpu.at_boundary(), "at boundary after reset");
}

/// Verifies REQ-128 (criterion 3): two CPUs on equal memory make the same accesses.
#[test]
fn power_on_is_deterministic() {
    let run = || {
        let mut bus = TestBus {
            ram: random_memory(3),
            log: Vec::new(),
            now: 0,
        };
        let mut cpu = Cpu::new();
        for _ in 0..100_000 {
            let _tick: Tick = cpu.tick(&mut bus, true);
        }
        bus.log
    };
    let first = run();
    assert_eq!(first.len(), 100_000, "accesses");
    assert!(first == run(), "the same accesses");
}

// ----------------------------------------------------------------------------------------
// Halt by ANTIC and stop by RDY (REQ-125 to REQ-127)
// ----------------------------------------------------------------------------------------

/// Verifies REQ-125 (criterion 1): halted cycles delay the accesses.
#[test]
fn halted_cycles_delay_accesses() {
    let mut d = Driver::new();
    d.load(0x0209, &[0xAD, 0x00, 0x10]); // LDA $1000 after 9 NOPs
    d.halt = span(20, 29);
    d.run_to(32);
    let cycles: Vec<u32> = d.bus.log[18..].iter().map(|a| a.cycle).collect();
    assert_eq!(cycles, [19, 30, 31, 32], "cycles");
    assert_eq!(d.at(19).addr, 0x0209, "opcode fetch");
    assert_eq!(d.at(32).addr, 0x1000, "operand read");
}

/// Verifies REQ-125 (criterion 2): halted cycles also delay writes.
#[test]
fn halted_rmw_writes() {
    let mut d = Driver::new();
    d.load(0x0206, &[0xA5, 0x10, 0xEE, 0x00, 0x06]); // 6 NOPs, LDA $10, INC $0600
    d.halt = span(20, 29);
    d.run_to(31);
    assert_eq!(d.at(16).addr, 0x0208, "opcode fetch");
    assert_eq!((d.at(19).addr, d.at(19).write), (0x0600, false), "read");
    assert_eq!((d.at(30).addr, d.at(30).write), (0x0600, true), "old value");
    assert_eq!((d.at(31).addr, d.at(31).write), (0x0600, true), "new value");
}

/// Verifies REQ-125 (criterion 3): halted cycles do not change the results.
#[test]
fn halted_cycles_do_not_change_results() {
    let run = |halt: bool| {
        let mut bus = TestBus {
            ram: random_memory(5),
            log: Vec::new(),
            now: 0,
        };
        let mut cpu = Cpu::new();
        let mut regs = Vec::new();
        let mut cycle = 0_u32;
        while regs.len() < 10_000 {
            cycle += 1;
            if halt && cycle.is_multiple_of(3) {
                continue;
            }
            let _tick: Tick = cpu.tick(&mut bus, true);
            if cpu.at_boundary() {
                regs.push(*cpu.registers());
            }
        }
        let log: Vec<(u16, u8, bool)> =
            bus.log.iter().map(|a| (a.addr, a.value, a.write)).collect();
        (log, regs)
    };
    assert!(run(false) == run(true), "the same accesses and registers");
}

/// Verifies REQ-126 (criterion 1): RDY low repeats the opcode fetch.
#[test]
fn rdy_low_repeats_read() {
    let mut d = Driver::new();
    d.regs().pc = 0x2000;
    d.rdy_low = span(1, 10);
    d.run_to(11);
    assert_eq!(d.reads(0x2000), (1..=11).collect::<Vec<u32>>(), "reads");
    d.run_to(12);
    assert_eq!(d.at(12).addr, 0x2001, "second cycle");
}

/// Verifies REQ-126 (criterion 2): the last write of `INC WSYNC` runs, then the CPU stops.
#[test]
fn rdy_low_after_inc_wsync() {
    let mut d = Driver::with_code(&[0xEE, 0x0A, 0xD4]); // INC $D40A
    d.rdy_low = span(6, 21);
    d.run_to(23);
    assert!(d.at(5).write && d.at(6).write, "writes in C+4 and C+5");
    assert_eq!(
        d.reads(0x0203),
        (7..=22).collect::<Vec<u32>>(),
        "opcode fetch"
    );
    assert_eq!(d.at(23).addr, 0x0204, "second cycle");
}

/// Verifies REQ-126 (criterion 3): RDY low does not stop a write.
#[test]
fn rdy_low_does_not_stop_write() {
    let mut d = Driver::with_code(&[0x8D, 0x00, 0x06]); // STA $0600
    d.rdy_low = span(4, 13);
    d.run_to(14);
    assert_eq!((d.at(4).addr, d.at(4).write), (0x0600, true), "write");
    assert_eq!(
        d.reads(0x0203),
        (5..=14).collect::<Vec<u32>>(),
        "opcode fetch"
    );
}

/// Verifies REQ-126 (criterion 4): the registers do not change during a stop.
#[test]
fn rdy_low_keeps_registers() {
    let mut d = Driver::with_code(&[0xEE, 0x0A, 0xD4]); // INC $D40A
    d.rdy_low = span(6, 21);
    d.run_to(6);
    let regs = d.r();
    for c in 7..=21 {
        assert_eq!(d.cycle(), Some(Tick::Stalled), "cycle {c}");
        assert_eq!(d.r(), regs, "registers in cycle {c}");
    }
    assert_eq!(d.cycle(), Some(Tick::Ran), "cycle 22");
}

/// Verifies REQ-127 (criterion 1): halted cycles extend the recognition delay.
#[test]
fn halted_cycles_extend_recognition_delay() {
    let mut d = Driver::new();
    d.nmi = span(1, 1000);
    d.halt = span(2, 9);
    d.run_to(30);
    assert_eq!(d.at(10).addr, 0x0201, "second cycle of the NOP");
    assert_eq!(d.nmi_starts(), [11], "NMI sequence");
}

/// Verifies REQ-127 (criterion 2): an NMI edge in a halted cycle is kept.
#[test]
fn nmi_edge_in_halted_cycle_kept() {
    let mut d = Driver::new();
    d.nmi = span(2, 1000);
    d.halt = span(2, 9);
    d.run_to(30);
    assert_eq!(
        pattern(&d.bus.log[2..4]),
        reads_at(&[0x0201, 0x0202]),
        "next NOP in N+10"
    );
    assert_eq!(d.nmi_starts(), [13], "NMI sequence");
}

/// Verifies REQ-127 (criterion 3): an NMI during a RDY stop is seen after the stopped
/// instruction.
#[test]
fn nmi_during_rdy_after_stopped_instruction() {
    let mut d = Driver::with_code(&[0x8D, 0x0A, 0xD4]); // STA $D40A
    d.rdy_low = span(5, 21);
    d.nmi = span(11, 1000);
    d.run_to(40);
    assert_eq!(d.at(23).addr, 0x0204, "NOP in C+21 and C+22");
    assert_eq!(d.nmi_starts(), [24], "NMI sequence");
}

/// Verifies REQ-127 (criterion 4): the NMI sequence stops on RDY.
#[test]
fn nmi_sequence_stops_on_rdy() {
    let mut d = Driver::with_code(&[0x8D, 0x0A, 0xD4]); // STA $D40A
    d.rdy_low = span(5, 21);
    d.nmi = span(2, 1000);
    d.run_to(29);
    let first: Vec<u32> = d.reads(0x0203).into_iter().take(18).collect();
    assert_eq!(first, (5..=22).collect::<Vec<u32>>(), "first read repeated");
    assert_eq!(d.at(28).addr, 0xFFFB, "sequence ends in C+27");
    assert_eq!(d.at(29).addr, NMI_HANDLER, "handler opcode fetch");
}

// ----------------------------------------------------------------------------------------
// Opcode table
// ----------------------------------------------------------------------------------------

/// Verifies REQ-111 and REQ-113: the opcode table has the 12 KIL opcodes and the 28 `NOP`
/// opcodes (`$EA` and the 27 undocumented ones), and the table built at compile time agrees
/// with the decoder.
#[test]
fn opcode_table_counts() {
    let mut kil = 0;
    let mut nop = 0;
    for opcode in 0..=u8::MAX {
        let entry = decode(opcode);
        assert_eq!(entry, TABLE[usize::from(opcode)], "{opcode:02X}");
        match entry.0 {
            Op::Kil => kil += 1,
            Op::Nop => nop += 1,
            _ => {}
        }
    }
    assert_eq!((kil, nop), (12, 28), "KIL and NOP opcodes");
}
