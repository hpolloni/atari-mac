//! MOS 6502 CPU (NMOS), as used in the Atari 65XE (the 6502C "SALLY").
//!
//! Design: `docs/design/cpu.md`. Requirements: REQ-100 to REQ-128.
//!
//! The CPU runs one cycle per [`Cpu::tick`] call. Each tick makes exactly one bus access
//! (REQ-101). The machine calls `tick` only in the cycles where ANTIC does not halt the CPU
//! (REQ-125), and gives the RDY level of the cycle (REQ-126).

#![expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    reason = "WP-0 stubs: delete this when the work package replaces the stubs"
)]

/// The CPU's view of the system. Each call is the bus access of one CPU cycle.
pub trait Bus {
    /// Reads one byte, with the side effects of a real read.
    fn read(&mut self, addr: u16) -> u8;
    /// Writes one byte.
    fn write(&mut self, addr: u16, value: u8);
}

/// Bits of the processor status register `P`.
pub mod flags {
    /// Carry.
    pub const C: u8 = 0x01;
    /// Zero.
    pub const Z: u8 = 0x02;
    /// IRQ disable.
    pub const I: u8 = 0x04;
    /// Decimal mode.
    pub const D: u8 = 0x08;
    /// Break. Exists only in the copy of `P` that is pushed to the stack.
    pub const B: u8 = 0x10;
    /// Unused. Always 1.
    pub const U: u8 = 0x20;
    /// Overflow.
    pub const V: u8 = 0x40;
    /// Negative.
    pub const N: u8 = 0x80;
}

/// The programmer-visible registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Registers {
    /// Accumulator.
    pub a: u8,
    /// Index register X.
    pub x: u8,
    /// Index register Y.
    pub y: u8,
    /// Stack pointer (the stack is at `$0100 + s`).
    pub s: u8,
    /// Processor status. Bits 4 and 5 always read as 1 (REQ-107).
    pub p: u8,
    /// Program counter.
    pub pc: u16,
}

/// The result of [`Cpu::step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// One instruction executed.
    Instruction,
    /// One interrupt sequence (NMI or IRQ, 7 cycles) executed.
    Interrupt,
    /// The reset sequence (7 cycles) executed.
    Reset,
    /// The CPU is locked by a KIL opcode. One cycle executed.
    Jammed,
}

/// What the CPU did in one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    /// A normal cycle.
    Ran,
    /// RDY was low and the cycle was a read: the read was repeated and the CPU did not
    /// advance.
    Stalled,
}

/// The 6502 CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cpu {
    /// Programmer-visible registers.
    regs: Registers,
    /// The level of the NMI input.
    nmi_level: bool,
    /// The level of the IRQ input.
    irq_level: bool,
    /// A KIL opcode locked the CPU.
    jammed: bool,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    /// Creates the CPU in the power-on state (REQ-128). A reset is pending, so the first
    /// 7 ticks run the reset sequence (REQ-124).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            regs: Registers {
                a: 0,
                x: 0,
                y: 0,
                s: 0,
                p: flags::B | flags::U,
                pc: 0,
            },
            nmi_level: false,
            irq_level: false,
            jammed: false,
        }
    }

    /// Asserts the reset input. The next tick starts the 7-cycle reset sequence. It also ends
    /// the KIL state and clears a pending NMI edge (REQ-113, REQ-124).
    pub const fn reset(&mut self) {
        // WP-0 STUB: WP-1 implements the reset sequence.
        self.jammed = false;
    }

    /// Runs one CPU cycle. It makes exactly one call to `bus.read` or `bus.write`. `rdy` is
    /// the level of the RDY input in this cycle (`false` = low, CPU stopped).
    pub fn tick(&mut self, bus: &mut impl Bus, rdy: bool) -> Tick {
        // WP-0 STUB: WP-1 implements the instruction set. The stub reads PC and does not
        // advance, so the machine can run without a CPU.
        let _opcode: u8 = bus.read(self.regs.pc);
        if rdy { Tick::Ran } else { Tick::Stalled }
    }

    /// Sets the level of the NMI input (`true` = asserted). The CPU detects the edge.
    pub const fn set_nmi(&mut self, asserted: bool) {
        self.nmi_level = asserted;
    }

    /// Sets the level of the IRQ input (`true` = asserted).
    pub const fn set_irq(&mut self, asserted: bool) {
        self.irq_level = asserted;
    }

    /// `true` if the next tick is the first cycle of an instruction, an interrupt sequence or
    /// the reset sequence.
    #[must_use]
    pub const fn at_boundary(&self) -> bool {
        // WP-0 STUB.
        true
    }

    /// Runs ticks with `rdy = true` until the next boundary. For tests and debuggers.
    pub fn step(&mut self, bus: &mut impl Bus) -> Step {
        // WP-0 STUB.
        let _tick: Tick = self.tick(bus, true);
        Step::Jammed
    }

    /// The registers.
    #[must_use]
    pub const fn registers(&self) -> &Registers {
        &self.regs
    }

    /// The registers, for change at a boundary (test harnesses, debuggers).
    pub const fn registers_mut(&mut self) -> &mut Registers {
        &mut self.regs
    }

    /// `true` while a KIL opcode locks the CPU.
    #[must_use]
    pub const fn is_jammed(&self) -> bool {
        self.jammed
    }
}
