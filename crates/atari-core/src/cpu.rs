//! MOS 6502 CPU (NMOS), as used in the Atari 65XE (the 6502C "SALLY").
//!
//! Design: `docs/design/cpu.md`. Requirements: REQ-100 to REQ-128.
//!
//! The CPU runs one cycle per [`Cpu::tick`] call. Each tick makes exactly one bus access
//! (REQ-101). The machine calls `tick` only in the cycles where ANTIC does not halt the CPU
//! (REQ-125), and gives the RDY level of the cycle (REQ-126).
//!
//! The cycle sequences follow HRM 3.3 (cycle timing), the interrupt behavior HRM 3.4 and the
//! undocumented opcodes HRM 3.5. The `SingleStepTests` 6502 data is the reference for the
//! exact address and value of each dummy cycle (project decision in `cpu.md`).

mod alu;
mod decode;
#[cfg(test)]
mod tests;

use core::mem;

use decode::{Am, Kind, Op, TABLE};

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

/// What the CPU does in its next tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// At an instruction boundary: fetch an opcode or start an interrupt sequence.
    Fetch,
    /// In an instruction, after the opcode fetch.
    Execute,
    /// In an IRQ, NMI or `BRK` sequence.
    Interrupt,
    /// In the reset sequence.
    Reset,
    /// Locked by a KIL opcode (REQ-113).
    Jammed,
}

/// How a cycle ends, for the interrupt poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    /// The instruction continues. The poll result of this cycle replaces the old one.
    Continue,
    /// The instruction continues, but the poll result of this cycle is not used: the second
    /// cycle of a taken branch (REQ-121).
    Hold,
    /// This was the last cycle. The poll result of the cycle before is used.
    Done,
}

/// The first value of `Cpu::cycle` in the cycles at the effective address.
const OPERAND: u8 = 0x40;
/// The base address of the stack.
const STACK: u16 = 0x0100;
/// The NMI vector.
const NMI_VECTOR: u16 = 0xFFFA;
/// The reset vector.
const RESET_VECTOR: u16 = 0xFFFC;
/// The IRQ and `BRK` vector.
const IRQ_VECTOR: u16 = 0xFFFE;

/// The 6502 CPU.
#[expect(
    clippy::struct_excessive_bools,
    reason = "the bools are independent latches of the CPU (cpu.md, State), not a mode"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cpu {
    /// Programmer-visible registers.
    regs: Registers,
    /// Opcode of the current instruction.
    opcode: u8,
    /// Number of the next cycle in the current instruction or sequence (the opcode fetch is
    /// cycle 1). From [`OPERAND`] on: the cycles at the effective address.
    cycle: u8,
    /// What the next tick does.
    mode: Mode,
    /// Effective address (or zero page pointer) under construction.
    addr: u16,
    /// High byte of the base address of an indexed mode, for `SHA`, `SHX`, `SHY`, `SHS`.
    base_hi: u8,
    /// The indexed address crossed a page.
    crossed: bool,
    /// Operand, pointer low byte, branch offset or read-modify-write value.
    data: u8,
    /// Last level from `end_cycle`.
    nmi_level: bool,
    /// Edge latched by `end_cycle`, not yet sampled.
    nmi_edge: bool,
    /// Edge sampled in a tick, not yet serviced.
    nmi_pending: bool,
    /// Level from `set_irq`.
    irq_level: bool,
    /// Result of the interrupt poll for the current instruction.
    poll: bool,
    /// Vector of the current interrupt sequence.
    vector: u16,
    /// The current sequence is a `BRK`.
    brk: bool,
    /// Skip the interrupt check at the next boundary (REQ-123).
    no_poll_next: bool,
    /// Cycle count in the KIL state.
    jam_cycle: u8,
    /// How the tick of this machine cycle ended, for `end_cycle`. `None` if no tick ran in
    /// this cycle (halted, stalled or already ended).
    tick_end: Option<End>,
    /// The tick of this machine cycle was cycle 5 of an interrupt sequence: an NMI edge
    /// sampled at its end is lost (REQ-122).
    discard_nmi: bool,
}

/// A bus that forwards each access and records whether it was a write. `tick` uses it when
/// RDY is low (REQ-126).
struct Probe<'a, B: Bus> {
    /// The real bus.
    bus: &'a mut B,
    /// The access was a write.
    wrote: bool,
}

impl<B: Bus> Bus for Probe<'_, B> {
    fn read(&mut self, addr: u16) -> u8 {
        self.bus.read(addr)
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.wrote = true;
        self.bus.write(addr, value);
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

/// The high and low byte of `value`.
const fn split(value: u16) -> (u8, u8) {
    let [lo, hi] = value.to_le_bytes();
    (hi, lo)
}

/// The address `hi:lo`.
const fn join(hi: u8, lo: u8) -> u16 {
    u16::from_le_bytes([lo, hi])
}

/// The address in page zero that follows `addr` (wraps from `$FF` to `$00`, REQ-105).
const fn zp_next(addr: u16) -> u16 {
    join(0, split(addr).1.wrapping_add(1))
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
            opcode: 0,
            cycle: 1,
            mode: Mode::Reset,
            addr: 0,
            base_hi: 0,
            crossed: false,
            data: 0,
            nmi_level: false,
            nmi_edge: false,
            nmi_pending: false,
            irq_level: false,
            poll: false,
            vector: RESET_VECTOR,
            brk: false,
            no_poll_next: false,
            jam_cycle: 0,
            tick_end: None,
            discard_nmi: false,
        }
    }

    /// Asserts the reset input. The next tick starts the 7-cycle reset sequence. It also ends
    /// the KIL state and clears a pending NMI edge (REQ-113, REQ-124).
    pub const fn reset(&mut self) {
        self.mode = Mode::Reset;
        self.cycle = 1;
        self.jam_cycle = 0;
        self.nmi_edge = false;
        self.nmi_pending = false;
        self.poll = false;
        self.no_poll_next = false;
        self.tick_end = None;
        self.discard_nmi = false;
    }

    /// Runs one CPU cycle. It makes exactly one call to `bus.read` or `bus.write`. `rdy` is
    /// the level of the RDY input in this cycle (`false` = low, CPU stopped).
    ///
    /// With RDY low, a read cycle is made on the bus and then undone: the CPU keeps the state
    /// from before the tick and makes no interrupt poll. A write cycle runs normally
    /// (REQ-126, REQ-127, HRM 4.9). The interrupt poll of the cycle is in `end_cycle`.
    pub fn tick(&mut self, bus: &mut impl Bus, rdy: bool) -> Tick {
        self.tick_end = None;
        self.discard_nmi = false;
        if rdy {
            self.tick_end = Some(self.run_cycle(bus));
            return Tick::Ran;
        }
        let saved = *self;
        let mut probe = Probe { bus, wrote: false };
        let end = self.run_cycle(&mut probe);
        if probe.wrote {
            self.tick_end = Some(end);
            Tick::Ran
        } else {
            *self = saved;
            Tick::Stalled
        }
    }

    /// Ends machine cycle `c`, after the bus access (OI-017). `nmi_level` is the level of the
    /// NMI input in cycle `c` (`true` = asserted). The machine calls it in every cycle, also
    /// in a halted cycle.
    ///
    /// The CPU latches the NMI edge. If a tick ran in this cycle (`Tick::Ran`), the CPU then
    /// samples the edge into the pending NMI and makes the interrupt poll of the cycle
    /// (HRM 3.4, REQ-118, REQ-127).
    pub const fn end_cycle(&mut self, nmi_level: bool) {
        if nmi_level && !self.nmi_level {
            self.nmi_edge = true;
        }
        self.nmi_level = nmi_level;
        let Some(end) = self.tick_end.take() else {
            return;
        };
        if self.discard_nmi {
            // An NMI edge sampled in cycle 5 of a sequence is lost (REQ-122).
            self.discard_nmi = false;
            self.nmi_edge = false;
        }
        if self.nmi_edge {
            self.nmi_edge = false;
            self.nmi_pending = true;
        }
        if matches!(end, End::Continue) {
            self.poll = self.nmi_pending || (self.irq_level && self.regs.p & flags::I == 0);
        }
    }

    /// Sets the level of the IRQ input (`true` = asserted).
    pub const fn set_irq(&mut self, asserted: bool) {
        self.irq_level = asserted;
    }

    /// `true` if the next tick is the first cycle of an instruction, an interrupt sequence or
    /// the reset sequence. In the KIL state each tick is a unit of its own, so the CPU is
    /// always at a boundary.
    #[must_use]
    pub const fn at_boundary(&self) -> bool {
        match self.mode {
            Mode::Fetch | Mode::Jammed => true,
            Mode::Reset => self.cycle == 1,
            Mode::Execute | Mode::Interrupt => false,
        }
    }

    /// Runs ticks with `rdy = true` until the next boundary. For tests and debuggers. Each
    /// tick is followed by `end_cycle` with the last NMI level, so no new NMI edge occurs.
    pub fn step(&mut self, bus: &mut impl Bus) -> Step {
        let reset = self.mode == Mode::Reset;
        let _first: Tick = self.tick(bus, true);
        self.end_cycle(self.nmi_level);
        let kind = match self.mode {
            Mode::Jammed => return Step::Jammed,
            _ if reset => Step::Reset,
            Mode::Interrupt if !self.brk => Step::Interrupt,
            _ => Step::Instruction,
        };
        while !self.at_boundary() {
            let _next: Tick = self.tick(bus, true);
            self.end_cycle(self.nmi_level);
        }
        kind
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
        matches!(self.mode, Mode::Jammed)
    }

    /// Runs one cycle. The NMI edge sample and the interrupt poll of the cycle follow in
    /// `end_cycle` (HRM 3.4, OI-017).
    fn run_cycle<B: Bus>(&mut self, bus: &mut B) -> End {
        let end = match self.mode {
            Mode::Fetch => self.fetch(bus),
            Mode::Execute => self.execute(bus),
            Mode::Interrupt => self.interrupt(bus),
            Mode::Reset => self.reset_cycle(bus),
            Mode::Jammed => self.jam(bus),
        };
        if end == End::Done {
            self.mode = Mode::Fetch;
        }
        end
    }

    /// Reads the byte at `PC` and increments `PC`.
    fn fetch_pc<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let value = bus.read(self.regs.pc);
        self.regs.pc = self.regs.pc.wrapping_add(1);
        value
    }

    /// A dummy read of `PC`.
    fn dummy_pc<B: Bus>(&self, bus: &mut B) {
        let _discarded: u8 = bus.read(self.regs.pc);
    }

    /// Writes `value` to the stack and decrements `S`.
    fn push<B: Bus>(&mut self, bus: &mut B, value: u8) {
        bus.write(STACK | u16::from(self.regs.s), value);
        self.regs.s = self.regs.s.wrapping_sub(1);
    }

    /// Increments `S` and reads the stack.
    fn pull<B: Bus>(&mut self, bus: &mut B) -> u8 {
        self.regs.s = self.regs.s.wrapping_add(1);
        bus.read(STACK | u16::from(self.regs.s))
    }

    /// A dummy read of the stack at `S`.
    fn dummy_stack<B: Bus>(&self, bus: &mut B) {
        let _discarded: u8 = bus.read(STACK | u16::from(self.regs.s));
    }

    /// Sets `P` from a pulled value. Bits 4 and 5 keep their value (REQ-107).
    const fn set_p(&mut self, value: u8) {
        let fixed = flags::B | flags::U;
        self.regs.p = (value & !fixed) | (self.regs.p & fixed);
    }

    /// Ends the address cycles: the next cycle is the first one at the effective address.
    const fn enter_operand(&mut self) -> End {
        self.cycle = OPERAND;
        End::Continue
    }

    /// Cycle 1 at a boundary: starts an interrupt sequence if the poll found a request, else
    /// fetches the opcode (HRM 3.4).
    fn fetch<B: Bus>(&mut self, bus: &mut B) -> End {
        let skip = mem::take(&mut self.no_poll_next);
        self.cycle = 2;
        if self.poll && !skip {
            self.dummy_pc(bus);
            self.mode = Mode::Interrupt;
            self.brk = false;
            return End::Continue;
        }
        self.opcode = self.fetch_pc(bus);
        let (op, _) = TABLE[usize::from(self.opcode)];
        self.mode = match op {
            Op::Brk => {
                self.brk = true;
                Mode::Interrupt
            }
            Op::Kil => {
                self.jam_cycle = 0;
                Mode::Jammed
            }
            _ => Mode::Execute,
        };
        End::Continue
    }

    /// One tick in the KIL state: the read pattern of the `SingleStepTests` data (REQ-113).
    fn jam<B: Bus>(&mut self, bus: &mut B) -> End {
        let addr = match self.jam_cycle {
            0 => self.regs.pc,
            2 | 3 => 0xFFFE,
            _ => 0xFFFF,
        };
        let _discarded: u8 = bus.read(addr);
        self.jam_cycle = self.jam_cycle.saturating_add(1);
        End::Hold
    }

    /// Cycles 2 to 7 of the IRQ, NMI and `BRK` sequence (REQ-114 to REQ-116, REQ-122).
    fn interrupt<B: Bus>(&mut self, bus: &mut B) -> End {
        let c = self.cycle;
        self.cycle = c.wrapping_add(1);
        let (pch, pcl) = split(self.regs.pc);
        match c {
            2 => {
                if self.brk {
                    let _signature: u8 = self.fetch_pc(bus);
                } else {
                    self.dummy_pc(bus);
                }
            }
            3 => self.push(bus, pch),
            4 => self.push(bus, pcl),
            5 => {
                let value = if self.brk {
                    self.regs.p | flags::B | flags::U
                } else {
                    (self.regs.p | flags::U) & !flags::B
                };
                self.push(bus, value);
                // The vector is selected here: an NMI can hijack the sequence (REQ-122).
                self.vector = if mem::take(&mut self.nmi_pending) {
                    NMI_VECTOR
                } else {
                    IRQ_VECTOR
                };
                // An NMI edge sampled at the end of this cycle is lost (REQ-122).
                self.discard_nmi = true;
            }
            6 => {
                self.data = bus.read(self.vector);
                self.regs.p |= flags::I;
            }
            _ => {
                let hi = bus.read(self.vector.wrapping_add(1));
                self.regs.pc = join(hi, self.data);
                self.no_poll_next = true;
                return End::Done;
            }
        }
        End::Continue
    }

    /// One cycle of the reset sequence: 7 reads, no write (REQ-124).
    fn reset_cycle<B: Bus>(&mut self, bus: &mut B) -> End {
        let c = self.cycle;
        self.cycle = c.wrapping_add(1);
        match c {
            1 | 2 => self.dummy_pc(bus),
            3..=5 => {
                self.dummy_stack(bus);
                self.regs.s = self.regs.s.wrapping_sub(1);
            }
            6 => {
                self.data = bus.read(RESET_VECTOR);
                self.regs.p |= flags::I;
            }
            _ => {
                let hi = bus.read(RESET_VECTOR + 1);
                self.regs.pc = join(hi, self.data);
                return End::Done;
            }
        }
        End::Continue
    }

    /// One cycle of an instruction after the opcode fetch.
    fn execute<B: Bus>(&mut self, bus: &mut B) -> End {
        let c = self.cycle;
        self.cycle = c.wrapping_add(1);
        let (op, am) = TABLE[usize::from(self.opcode)];
        if c >= OPERAND {
            return self.operand(bus, op, c - OPERAND);
        }
        match am {
            Am::Imp | Am::Acc => {
                self.dummy_pc(bus);
                self.implied(op, am);
                End::Done
            }
            Am::Imm => {
                let value = self.fetch_pc(bus);
                self.read_op(op, value);
                End::Done
            }
            Am::Zp => {
                self.addr = u16::from(self.fetch_pc(bus));
                self.enter_operand()
            }
            Am::Zpx | Am::Zpy => self.zero_page_indexed(bus, am, c),
            Am::Abs => self.absolute(bus, c),
            Am::Abx | Am::Aby => self.absolute_indexed(bus, op, am, c),
            Am::Izx => self.indexed_indirect(bus, c),
            Am::Izy => self.indirect_indexed(bus, op, c),
            Am::Rel => self.branch(bus, c),
            Am::Special => self.special(bus, op, c),
        }
    }

    /// `zp,X` and `zp,Y`: the sum wraps in page zero (REQ-105).
    fn zero_page_indexed<B: Bus>(&mut self, bus: &mut B, am: Am, c: u8) -> End {
        if c == 2 {
            self.addr = u16::from(self.fetch_pc(bus));
            return End::Continue;
        }
        let _discarded: u8 = bus.read(self.addr);
        let index = if am == Am::Zpx {
            self.regs.x
        } else {
            self.regs.y
        };
        self.addr = u16::from(split(self.addr).1.wrapping_add(index));
        self.enter_operand()
    }

    /// `abs`.
    fn absolute<B: Bus>(&mut self, bus: &mut B, c: u8) -> End {
        if c == 2 {
            self.data = self.fetch_pc(bus);
            return End::Continue;
        }
        let hi = self.fetch_pc(bus);
        self.addr = join(hi, self.data);
        self.enter_operand()
    }

    /// `abs,X` and `abs,Y`.
    fn absolute_indexed<B: Bus>(&mut self, bus: &mut B, op: Op, am: Am, c: u8) -> End {
        match c {
            2 => {
                self.data = self.fetch_pc(bus);
                End::Continue
            }
            3 => {
                let hi = self.fetch_pc(bus);
                let index = if am == Am::Abx {
                    self.regs.x
                } else {
                    self.regs.y
                };
                self.index(hi, index);
                End::Continue
            }
            _ => self.fix_address(bus, op),
        }
    }

    /// `(zp,X)`: the pointer wraps in page zero (REQ-105).
    fn indexed_indirect<B: Bus>(&mut self, bus: &mut B, c: u8) -> End {
        match c {
            2 => self.addr = u16::from(self.fetch_pc(bus)),
            3 => {
                let _discarded: u8 = bus.read(self.addr);
                self.addr = u16::from(split(self.addr).1.wrapping_add(self.regs.x));
            }
            4 => self.data = bus.read(self.addr),
            _ => {
                let hi = bus.read(zp_next(self.addr));
                self.addr = join(hi, self.data);
                return self.enter_operand();
            }
        }
        End::Continue
    }

    /// `(zp),Y`: the pointer wraps in page zero (REQ-105).
    fn indirect_indexed<B: Bus>(&mut self, bus: &mut B, op: Op, c: u8) -> End {
        match c {
            2 => self.addr = u16::from(self.fetch_pc(bus)),
            3 => self.data = bus.read(self.addr),
            4 => {
                let hi = bus.read(zp_next(self.addr));
                self.index(hi, self.regs.y);
            }
            _ => return self.fix_address(bus, op),
        }
        End::Continue
    }

    /// Adds `index` to the low byte of the base address `hi:data` (HRM 3.3).
    const fn index(&mut self, hi: u8, index: u8) {
        let (lo, crossed) = self.data.overflowing_add(index);
        self.base_hi = hi;
        self.crossed = crossed;
        self.addr = join(hi, lo);
    }

    /// The cycle after an indexed address: the read at `base_hi:low`. For a read without a
    /// page cross it is the operand read (REQ-102). Else it is a dummy read, and the address
    /// is corrected (REQ-103).
    fn fix_address<B: Bus>(&mut self, bus: &mut B, op: Op) -> End {
        if op.kind() == Kind::Read && !self.crossed {
            return self.operand(bus, op, 0);
        }
        let _discarded: u8 = bus.read(self.addr);
        let unstable = match op {
            Op::Sha => Some(self.regs.a & self.regs.x),
            Op::Shx => Some(self.regs.x),
            Op::Shy => Some(self.regs.y),
            Op::Shs => {
                self.regs.s = self.regs.a & self.regs.x;
                Some(self.regs.s)
            }
            _ => None,
        };
        if let Some(reg) = unstable {
            // HRM 3.5: the value is the AND of the register and the high byte of the base address
            // + 1. A page cross puts the value on the high address lines (REQ-112).
            self.data = reg & self.base_hi.wrapping_add(1);
            if self.crossed {
                self.addr = join(self.data, split(self.addr).1);
            }
        } else if self.crossed {
            self.addr = self.addr.wrapping_add(0x0100);
        }
        self.enter_operand()
    }

    /// The cycles at the effective address. `s` is the number of the cycle from 0.
    fn operand<B: Bus>(&mut self, bus: &mut B, op: Op, s: u8) -> End {
        match op.kind() {
            Kind::Read => {
                let value = bus.read(self.addr);
                self.read_op(op, value);
                End::Done
            }
            Kind::Write => {
                let value = self.store_value(op);
                bus.write(self.addr, value);
                End::Done
            }
            Kind::Rmw => match s {
                0 => {
                    self.data = bus.read(self.addr);
                    End::Continue
                }
                1 => {
                    bus.write(self.addr, self.data);
                    End::Continue
                }
                _ => {
                    self.data = self.rmw_op(op, self.data);
                    bus.write(self.addr, self.data);
                    End::Done
                }
            },
        }
    }

    /// The value that a write operation stores.
    const fn store_value(&self, op: Op) -> u8 {
        match op {
            Op::Sta => self.regs.a,
            Op::Stx => self.regs.x,
            Op::Sty => self.regs.y,
            Op::Sax => self.regs.a & self.regs.x,
            // SHA, SHX, SHY, SHS: computed by `fix_address`.
            _ => self.data,
        }
    }

    /// A read operation with its operand `value`.
    fn read_op(&mut self, op: Op, value: u8) {
        let r = &mut self.regs;
        let result = match op {
            Op::Lda => {
                r.a = value;
                value
            }
            Op::Ldx => {
                r.x = value;
                value
            }
            Op::Ldy => {
                r.y = value;
                value
            }
            Op::Lax => {
                r.a = value;
                r.x = value;
                value
            }
            Op::And => {
                r.a &= value;
                r.a
            }
            Op::Ora => {
                r.a |= value;
                r.a
            }
            Op::Eor => {
                r.a ^= value;
                r.a
            }
            Op::Ane => {
                r.a = (r.a | 0xEE) & r.x & value;
                r.a
            }
            Op::Lxa => {
                r.a = (r.a | 0xEE) & value;
                r.x = r.a;
                r.a
            }
            Op::Las => {
                r.s &= value;
                r.a = r.s;
                r.x = r.s;
                r.s
            }
            Op::Asr => {
                r.a = alu::lsr(r, r.a & value);
                return;
            }
            _ => return Self::alu_op(r, op, value),
        };
        alu::set_nz(r, result);
    }

    /// The read operations that set their own flags. `NOP` does nothing.
    fn alu_op(r: &mut Registers, op: Op, value: u8) {
        match op {
            Op::Adc => alu::adc(r, value),
            Op::Sbc => alu::sbc(r, value),
            Op::Cmp => alu::compare(r, r.a, value),
            Op::Cpx => alu::compare(r, r.x, value),
            Op::Cpy => alu::compare(r, r.y, value),
            Op::Bit => alu::bit(r, value),
            Op::Anc => alu::anc(r, value),
            Op::Arr => alu::arr(r, value),
            Op::Sbx => alu::sbx(r, value),
            _ => {}
        }
    }

    /// A read-modify-write operation. Returns the new value.
    fn rmw_op(&mut self, op: Op, value: u8) -> u8 {
        let r = &mut self.regs;
        let result = match op {
            Op::Asl | Op::Slo => alu::asl(r, value),
            Op::Lsr | Op::Sre => alu::lsr(r, value),
            Op::Rol | Op::Rla => alu::rol(r, value),
            Op::Ror | Op::Rra => alu::ror(r, value),
            Op::Inc | Op::Isb => alu::inc(r, value),
            _ => alu::dec(r, value),
        };
        // The combined opcodes do the ALU operation on the result (HRM 3.5).
        match op {
            Op::Slo => self.read_op(Op::Ora, result),
            Op::Rla => self.read_op(Op::And, result),
            Op::Sre => self.read_op(Op::Eor, result),
            Op::Rra => self.read_op(Op::Adc, result),
            Op::Dcp => self.read_op(Op::Cmp, result),
            Op::Isb => self.read_op(Op::Sbc, result),
            _ => {}
        }
        result
    }

    /// An implied or accumulator operation.
    fn implied(&mut self, op: Op, am: Am) {
        let r = &mut self.regs;
        if am == Am::Acc {
            r.a = match op {
                Op::Asl => alu::asl(r, r.a),
                Op::Lsr => alu::lsr(r, r.a),
                Op::Rol => alu::rol(r, r.a),
                _ => alu::ror(r, r.a),
            };
            return;
        }
        let result = match op {
            Op::Tax => {
                r.x = r.a;
                r.x
            }
            Op::Tay => {
                r.y = r.a;
                r.y
            }
            Op::Txa => {
                r.a = r.x;
                r.a
            }
            Op::Tya => {
                r.a = r.y;
                r.a
            }
            Op::Tsx => {
                r.x = r.s;
                r.x
            }
            Op::Inx => {
                r.x = r.x.wrapping_add(1);
                r.x
            }
            Op::Iny => {
                r.y = r.y.wrapping_add(1);
                r.y
            }
            Op::Dex => {
                r.x = r.x.wrapping_sub(1);
                r.x
            }
            Op::Dey => {
                r.y = r.y.wrapping_sub(1);
                r.y
            }
            _ => return Self::flag_op(r, op),
        };
        alu::set_nz(r, result);
    }

    /// The implied operations without a result: the flag operations, `TXS` and `NOP`.
    const fn flag_op(r: &mut Registers, op: Op) {
        match op {
            Op::Clc => r.p &= !flags::C,
            Op::Sec => r.p |= flags::C,
            Op::Cli => r.p &= !flags::I,
            Op::Sei => r.p |= flags::I,
            Op::Clv => r.p &= !flags::V,
            Op::Cld => r.p &= !flags::D,
            Op::Sed => r.p |= flags::D,
            Op::Txs => r.s = r.x,
            _ => {}
        }
    }

    /// `true` if the condition of the current branch opcode is met.
    const fn branch_taken(&self) -> bool {
        let flag = match self.opcode >> 6 {
            0 => flags::N,
            1 => flags::V,
            2 => flags::C,
            _ => flags::Z,
        };
        (self.regs.p & flag != 0) == (self.opcode & 0x20 != 0)
    }

    /// A branch: 2 cycles if not taken, 3 if taken, 4 with a page cross (REQ-106). The
    /// target is relative to the next instruction. A taken branch without a page cross uses
    /// the poll of cycle 1 (REQ-121).
    fn branch<B: Bus>(&mut self, bus: &mut B, c: u8) -> End {
        match c {
            2 => {
                self.data = self.fetch_pc(bus);
                if self.branch_taken() {
                    End::Hold
                } else {
                    End::Done
                }
            }
            3 => {
                self.dummy_pc(bus);
                let offset = i16::from(i8::from_le_bytes([self.data]));
                let target = self.regs.pc.wrapping_add_signed(offset);
                let (pch, _) = split(self.regs.pc);
                let (target_hi, target_lo) = split(target);
                if target_hi == pch {
                    self.regs.pc = target;
                    End::Done
                } else {
                    self.regs.pc = join(pch, target_lo);
                    self.addr = target;
                    End::Continue
                }
            }
            _ => {
                self.dummy_pc(bus);
                self.regs.pc = self.addr;
                End::Done
            }
        }
    }

    /// The fixed sequences: stack operations and jumps (HRM 3.3).
    fn special<B: Bus>(&mut self, bus: &mut B, op: Op, c: u8) -> End {
        match op {
            Op::Jsr => self.jsr(bus, c),
            Op::Rts => self.rts(bus, c),
            Op::Rti => self.rti(bus, c),
            Op::JmpAbs => {
                if c == 2 {
                    self.data = self.fetch_pc(bus);
                    return End::Continue;
                }
                let hi = self.fetch_pc(bus);
                self.regs.pc = join(hi, self.data);
                End::Done
            }
            Op::JmpInd => self.jmp_indirect(bus, c),
            Op::Pha | Op::Php => {
                if c == 2 {
                    self.dummy_pc(bus);
                    return End::Continue;
                }
                let value = if op == Op::Pha {
                    self.regs.a
                } else {
                    self.regs.p | flags::B | flags::U
                };
                self.push(bus, value);
                End::Done
            }
            _ => self.pull_register(bus, op, c),
        }
    }

    /// `PLA` and `PLP`: 4 cycles. `PLP` changes `I` in its last cycle (REQ-119).
    fn pull_register<B: Bus>(&mut self, bus: &mut B, op: Op, c: u8) -> End {
        match c {
            2 => self.dummy_pc(bus),
            3 => self.dummy_stack(bus),
            _ => {
                let value = self.pull(bus);
                if op == Op::Pla {
                    self.regs.a = value;
                    alu::set_nz(&mut self.regs, value);
                } else {
                    self.set_p(value);
                }
                return End::Done;
            }
        }
        End::Continue
    }

    /// `JMP (abs)`: the high byte comes from the same page as the low byte (REQ-105).
    fn jmp_indirect<B: Bus>(&mut self, bus: &mut B, c: u8) -> End {
        match c {
            2 => self.data = self.fetch_pc(bus),
            3 => {
                let hi = self.fetch_pc(bus);
                self.addr = join(hi, self.data);
            }
            4 => self.data = bus.read(self.addr),
            _ => {
                let (hi, lo) = split(self.addr);
                let pch = bus.read(join(hi, lo.wrapping_add(1)));
                self.regs.pc = join(pch, self.data);
                return End::Done;
            }
        }
        End::Continue
    }

    /// `JSR abs`: 6 cycles.
    fn jsr<B: Bus>(&mut self, bus: &mut B, c: u8) -> End {
        let (pch, pcl) = split(self.regs.pc);
        match c {
            2 => self.data = self.fetch_pc(bus),
            3 => self.dummy_stack(bus),
            4 => self.push(bus, pch),
            5 => self.push(bus, pcl),
            _ => {
                let hi = bus.read(self.regs.pc);
                self.regs.pc = join(hi, self.data);
                return End::Done;
            }
        }
        End::Continue
    }

    /// `RTS`: 6 cycles.
    fn rts<B: Bus>(&mut self, bus: &mut B, c: u8) -> End {
        match c {
            2 => self.dummy_pc(bus),
            3 => self.dummy_stack(bus),
            4 => self.data = self.pull(bus),
            5 => {
                let hi = self.pull(bus);
                self.regs.pc = join(hi, self.data);
            }
            _ => {
                let _discarded: u8 = self.fetch_pc(bus);
                return End::Done;
            }
        }
        End::Continue
    }

    /// `RTI`: 6 cycles. `P` is loaded in cycle 4, before the poll in cycle 5 (REQ-119).
    fn rti<B: Bus>(&mut self, bus: &mut B, c: u8) -> End {
        match c {
            2 => self.dummy_pc(bus),
            3 => self.dummy_stack(bus),
            4 => {
                let value = self.pull(bus);
                self.set_p(value);
            }
            5 => self.data = self.pull(bus),
            _ => {
                let hi = self.pull(bus);
                self.regs.pc = join(hi, self.data);
                return End::Done;
            }
        }
        End::Continue
    }
}
