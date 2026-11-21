//! The debugger functions of the machine (`docs/design/debugger.md`, "Machine additions"):
//! the hooked run of the trace (REQ-086) and the reads without side effects (REQ-087
//! criterion 5, REQ-090 criterion 5).

use super::Machine;
use crate::antic::CycleOutput;
use crate::cpu::Boundary;
use crate::debug::{CpuHook, InstructionStart, Position, RunEnd, Sequence, SequenceKind};
use crate::disasm::{Disassembly, disassemble};

/// The NMI vector (HRM 3.4): the vector of a sequence of the kind `Nmi`.
const NMI_VECTOR: u16 = 0xFFFA;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the design puts the debugger functions of the machine in machine/debug.rs"
)]
impl Machine {
    /// A byte as the CPU sees it, without side effects (REQ-087 criterion 5, REQ-090
    /// criterion 5). `None` where the chip has no value to show without a side effect: the
    /// cartridge control area `$D500` to `$D5FF`, where a read can switch the cartridge bank.
    #[must_use]
    pub fn debug_peek(&self, addr: u16) -> Option<u8> {
        match addr {
            0xD500..=0xD5FF => None,
            _ => Some(self.peek(addr)),
        }
    }

    /// The disassembly at `addr`, with the bytes of [`Machine::debug_peek`] (`$FF` for
    /// `None`). The bytes after `addr` wrap at `$FFFF`.
    #[must_use]
    pub fn disassemble_at(&self, addr: u16) -> Disassembly {
        let byte = |offset: u16| self.debug_peek(addr.wrapping_add(offset)).unwrap_or(0xFF);
        disassemble(addr, [byte(0), byte(1), byte(2)])
    }

    /// Runs machine cycles until ANTIC ends the frame, with the calls of `hook`
    /// (`docs/design/debugger.md`, "Hooked run and the cycle order"). The machine runs the
    /// same chip calls as [`Machine::run_frame`], so the hook does not change it (REQ-086
    /// criterion 5).
    ///
    /// The held cycle of `Control::Stop` is planned (SLICE-11); until then a stop does not
    /// hold the machine, and the result is always [`RunEnd::FrameDone`].
    pub fn run_frame_with<H: CpuHook>(&mut self, hook: &mut H) -> RunEnd {
        let target = self.frames + 1;
        while self.frames < target {
            self.cycle_with(hook);
        }
        RunEnd::FrameDone
    }

    /// The hook calls of one cycle before the CPU tick, after the SIOV shortcut
    /// (`docs/design/debugger.md`, "Hooked run and the cycle order", steps 1 and 2). In a
    /// halted cycle or with RDY low the instruction or the sequence does not start.
    pub(super) fn before_tick<H: CpuHook>(&mut self, out: &CycleOutput, hook: &mut H) {
        if !out.rdy {
            return;
        }
        let at = Position {
            frame: self.frames,
            scanline: out.line,
            hpos: out.cycle,
            cycle: self.cycles,
        };
        match self.cpu.boundary() {
            Some(Boundary::Opcode) => {
                let regs = *self.cpu.registers();
                if let Some((start, reset)) = self.sequence_start.take() {
                    let kind = if reset {
                        SequenceKind::Reset
                    } else if self.cpu.vector() == NMI_VECTOR {
                        SequenceKind::Nmi
                    } else {
                        SequenceKind::Irq
                    };
                    hook.sequence(&Sequence {
                        at: start,
                        kind,
                        handler: regs.pc,
                    });
                }
                let byte = |offset: u16| {
                    self.debug_peek(regs.pc.wrapping_add(offset))
                        .unwrap_or(0xFF)
                };
                let start = InstructionStart {
                    at,
                    regs,
                    bytes: [byte(0), byte(1), byte(2)],
                };
                // The held cycle of a stop is planned (SLICE-11).
                let _control = hook.instruction(&start);
            }
            Some(Boundary::Interrupt) => self.sequence_start = Some((at, false)),
            Some(Boundary::Reset) => self.sequence_start = Some((at, true)),
            Some(Boundary::Jammed) | None => {}
        }
    }
}
