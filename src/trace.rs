//! The CPU instruction trace of `--trace` (REQ-086).
//!
//! Design: `docs/design/debugger.md`, "Trace writer". The writer is a [`CpuHook`] of
//! `Machine::run_frame_with`. It writes one line for each instruction and each interrupt
//! sequence in the frames `from` to `from + frames - 1`.

use core::fmt;
use std::io::{self, Write};
use std::path::PathBuf;

use atari_core::cpu::flags;
use atari_core::debug::{Control, CpuHook, InstructionStart, Position, Sequence, SequenceKind};

/// The trace options of the command line (`--trace`, `--trace-from`, `--trace-frames`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceOptions {
    /// The trace file.
    pub file: PathBuf,
    /// The first frame of the trace (`--trace-from`, default 0).
    pub from: u64,
    /// The number of frames, at least 1 (`--trace-frames`, default 1).
    pub frames: u64,
}

/// Writes the lines of REQ-086 for the frames `from` to `from + frames - 1`.
#[derive(Debug)]
pub struct TraceWriter<W: Write> {
    /// The trace output.
    out: W,
    /// The first frame of the trace.
    from: u64,
    /// The number of frames of the trace.
    frames: u64,
    /// The first write error. After it the writer writes nothing more.
    error: Option<io::Error>,
}

impl<W: Write> TraceWriter<W> {
    /// A writer of the frames `from` to `from + frames - 1` to `out`.
    pub const fn new(out: W, from: u64, frames: u64) -> Self {
        Self {
            out,
            from,
            frames,
            error: None,
        }
    }

    /// `true` while the frame `frame` needs the hooked run: the frame is in the trace.
    #[must_use]
    pub const fn wants(&self, frame: u64) -> bool {
        frame >= self.from && frame - self.from < self.frames
    }

    /// Flushes the output and gives the first write error.
    ///
    /// # Errors
    ///
    /// The first error of a write or of the flush.
    pub fn finish(mut self) -> io::Result<()> {
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        self.out.flush()
    }

    /// Writes one line, if the frame of `at` is in the trace and no write failed before.
    fn line(&mut self, at: Position, line: impl fmt::Display) {
        if self.error.is_none()
            && self.wants(at.frame)
            && let Err(error) = writeln!(self.out, "{line}")
        {
            self.error = Some(error);
        }
    }
}

impl<W: Write> CpuHook for TraceWriter<W> {
    fn instruction(&mut self, start: &InstructionStart) -> Control {
        self.line(start.at, InstructionLine(start));
        Control::Continue
    }

    fn sequence(&mut self, sequence: &Sequence) {
        self.line(sequence.at, SequenceLine(sequence));
    }
}

/// Writes the four numbers of a position: frame, scan line, horizontal position, cycle.
fn position(f: &mut fmt::Formatter<'_>, at: Position) -> fmt::Result {
    write!(f, "{} {} {} {}", at.frame, at.scanline, at.hpos, at.cycle)
}

/// `Display` of one instruction line (REQ-086 criterion 1), without the line end:
/// `<frame> <scanline> <hpos> <cycle> <PC>: <bytes> <instruction>  A=<a> X=<x> Y=<y> S=<s>
/// P=<flags>`.
#[derive(Debug, Clone, Copy)]
pub struct InstructionLine<'a>(pub &'a InstructionStart);

impl fmt::Display for InstructionLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = self.0;
        let regs = start.regs;
        let disassembly = start.disassembly();
        let [b0, b1, b2] = start.bytes;
        position(f, start.at)?;
        write!(f, " {:04X}: ", regs.pc)?;
        // The bytes, padded with spaces to 8 characters, then one space.
        match disassembly.len() {
            1 => write!(f, "{b0:02X}      ")?,
            2 => write!(f, "{b0:02X} {b1:02X}   ")?,
            _ => write!(f, "{b0:02X} {b1:02X} {b2:02X}")?,
        }
        write!(
            f,
            " {disassembly}  A={:02X} X={:02X} Y={:02X} S={:02X} P=",
            regs.a, regs.x, regs.y, regs.s
        )?;
        let names = [
            (flags::N, 'N'),
            (flags::V, 'V'),
            (flags::U, '-'),
            (flags::B, 'B'),
            (flags::D, 'D'),
            (flags::I, 'I'),
            (flags::Z, 'Z'),
            (flags::C, 'C'),
        ];
        for (bit, name) in names {
            let shown = if regs.p & bit == 0 { '.' } else { name };
            write!(f, "{shown}")?;
        }
        Ok(())
    }
}

/// `Display` of one sequence line (REQ-086 criterion 2), without the line end:
/// `<frame> <scanline> <hpos> <cycle> <NMI|IRQ|RESET> vector <handler>`.
#[derive(Debug, Clone, Copy)]
pub struct SequenceLine<'a>(pub &'a Sequence);

impl fmt::Display for SequenceLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sequence = self.0;
        let kind = match sequence.kind {
            SequenceKind::Nmi => "NMI",
            SequenceKind::Irq => "IRQ",
            SequenceKind::Reset => "RESET",
        };
        position(f, sequence.at)?;
        write!(f, " {kind} vector ${:04X}", sequence.handler)
    }
}
