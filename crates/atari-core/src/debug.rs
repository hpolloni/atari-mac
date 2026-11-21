//! The hook types of the CPU trace and the monitor (`docs/design/debugger.md`, "Hook
//! types"). The machine functions that call a hook are in `machine/debug.rs`.
//!
//! Requirements: REQ-086.

use crate::cpu::Registers;
use crate::disasm::{Disassembly, disassemble};

/// A point in time: the frame, the beam position and the cycle count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    /// `Machine::frame_count` at the start of the cycle.
    pub frame: u64,
    /// The scan line of the cycle (`CycleOutput::line`).
    pub scanline: u32,
    /// The machine cycle in the scan line, 0 to 113 (`CycleOutput::cycle`).
    pub hpos: u32,
    /// `Machine::cycle_count` at the start of the cycle.
    pub cycle: u64,
}

/// The first cycle of an instruction: the cycle of the opcode fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstructionStart {
    /// The position of the opcode fetch.
    pub at: Position,
    /// The registers before the instruction. `regs.pc` is the address of the opcode.
    pub regs: Registers,
    /// The bytes at `regs.pc` to `regs.pc + 2`, read with `Machine::debug_peek` (`$FF` for
    /// `None`).
    pub bytes: [u8; 3],
}

impl InstructionStart {
    /// The disassembly of the instruction (REQ-087).
    #[must_use]
    pub const fn disassembly(&self) -> Disassembly {
        disassemble(self.regs.pc, self.bytes)
    }
}

/// The kind of an interrupt or reset sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceKind {
    /// A non-maskable interrupt (vector `$FFFA`).
    Nmi,
    /// An interrupt request (vector `$FFFE`).
    Irq,
    /// The reset sequence (vector `$FFFC`).
    Reset,
}

/// An interrupt or reset sequence (7 CPU cycles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequence {
    /// The first cycle of the sequence.
    pub at: Position,
    /// The kind. An NMI that hijacks an IRQ sequence (REQ-122) is `Nmi`.
    pub kind: SequenceKind,
    /// The handler address that the CPU read from the vector: the `PC` after the sequence.
    pub handler: u16,
}

/// One CPU bus access, also a dummy access (REQ-203).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BusAccess {
    /// The address.
    pub addr: u16,
    /// The byte read or written.
    pub value: u8,
    /// `true` for a write.
    pub write: bool,
}

/// What the hooked run does after a call of [`CpuHook::instruction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// The machine runs on.
    Continue,
    /// The machine holds the cycle before the CPU access (the monitor, SLICE-11). Until the
    /// held cycle of SLICE-11 exists, the run treats it as `Continue`.
    Stop,
}

/// How `Machine::run_frame_with` ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    /// ANTIC ended the frame.
    FrameDone,
    /// The hook stopped the machine (SLICE-11).
    Stopped,
}

/// The calls of the hooked run (`Machine::run_frame_with`). The default methods do nothing.
pub trait CpuHook {
    /// Before the opcode fetch of each instruction. `Control::Stop` holds the cycle: the
    /// machine stops before the CPU access of this cycle (SLICE-11).
    fn instruction(&mut self, _start: &InstructionStart) -> Control {
        Control::Continue
    }

    /// At the end of an interrupt or reset sequence, before `instruction` of the first
    /// instruction of the handler.
    fn sequence(&mut self, _sequence: &Sequence) {}

    /// After each CPU bus access, also a dummy access (REQ-203).
    fn access(&mut self, _access: BusAccess) {}
}

/// The hook that does nothing (the plain run).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoHook;

impl CpuHook for NoHook {}
