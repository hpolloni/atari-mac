# Debugger design

## Scope

This document tells how the emulator satisfies the requirements of the CPU trace and the
monitor:

| Requirement | Topic | Slice |
| --- | --- | --- |
| REQ-087 | Disassembler in the core | SLICE-10 WP-1 |
| REQ-086 | `--trace`: one line for each instruction and each interrupt sequence | SLICE-10 WP-2 |
| REQ-088 | Monitor: stop, step, frame, go | SLICE-11 WP-1 |
| REQ-089 | Monitor: breakpoints and watches | SLICE-11 WP-2 |
| REQ-090 | Monitor: registers, memory, disassembly | SLICE-11 WP-3 |

The core part is the disassembler (`disasm` module), the hook types (`debug` module) and a
second run function of the machine that calls a hook (`machine/debug.rs`). The frontend part
is the trace writer (`src/trace.rs`) and the monitor (`src/monitor.rs`).

Where the user types the monitor commands is open (OI-057). This design follows option 1 of
OI-057, the recommendation: the commands come from standard input in the terminal that
started the emulator. If the user selects another option, only the section "Monitor input"
changes. The command parser and the monitor state do not need a window.

## Interfaces

### Disassembler (`atari-core`, module `disasm`)

The module is `crates/atari-core/src/disasm.rs`. It does not read memory. The caller gives
the three bytes at the address.

```rust
/// The addressing mode of an opcode, for the operand text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Implied,     // no operand
    Accumulator, // "A"
    Immediate,   // "#$nn"
    ZeroPage,    // "$nn"
    ZeroPageX,   // "$nn,X"
    ZeroPageY,   // "$nn,Y"
    Absolute,    // "$nnnn"
    AbsoluteX,   // "$nnnn,X"
    AbsoluteY,   // "$nnnn,Y"
    Indirect,    // "($nnnn)"
    IndirectX,   // "($nn,X)"
    IndirectY,   // "($nn),Y"
    Relative,    // the target address "$nnnn"
}

/// One disassembled instruction. `Display` writes the text of REQ-087 criterion 2,
/// for example `LDA $1234,X`, with no allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disassembly { /* pc, opcode, operand bytes */ }

impl Disassembly {
    /// The length in bytes, 1 to 3 (REQ-087 criterion 1).
    pub const fn len(&self) -> u8;
    /// The mnemonic in upper case: `LDA`, `SLO`, `KIL`.
    pub const fn mnemonic(&self) -> &'static str;
    pub const fn mode(&self) -> Mode;
    /// The instruction bytes; only the first `len()` bytes are valid.
    pub const fn bytes(&self) -> [u8; 3];
}

impl fmt::Display for Disassembly { /* mnemonic, then one space and the operand */ }

/// Disassembles the instruction at `pc`. `bytes` are the bytes at `pc`, `pc + 1` and
/// `pc + 2` (wrap at `$FFFF`). The function uses only the bytes that the opcode needs.
pub const fn disassemble(pc: u16, bytes: [u8; 3]) -> Disassembly;

/// The length in bytes of `opcode`, 1 to 3.
pub const fn length(opcode: u8) -> u8;
```

Rules:

1. The module has its own table of 256 entries (mnemonic, mode). It does not use the CPU
   decode table (`cpu/decode.rs`), because that table puts the stack, jump and `BRK`
   opcodes in one mode `Special`. The test of REQ-087 criterion 4 compares the
   disassembler table with a third table in the test.
2. The hexadecimal digits are in upper case: `$0A`, not `$0a`.
3. A branch shows the target address: `pc + 2 + offset`, with the offset as a signed byte
   and a wrap at 16 bits. `BNE` at `$2000` with the offset `$34` is `BNE $2036`.
4. `ASL`, `LSR`, `ROL` and `ROR` with the accumulator are `ASL A`.
5. `JSR` and `JMP` absolute are `JSR $1234`. `JMP ($1234)` is mode `Indirect`.
6. `BRK` has the length 1. The CPU skips the signature byte (cpu.md, REQ-114), but the
   disassembler does not show it as an operand.
7. A KIL opcode has the text `KIL` and the length 1 (REQ-087 criterion 3).

### Undocumented names (HRM 3.5)

HRM 3.5 Table 2 gives one name for each opcode. The text of HRM 3.5 adds the parts of
some combined operations in brackets, for example "ASR (AND + LSR)" and "LXA (LDA + TAX)".
These brackets are descriptions, not second names. HRM 3.5 note 8 refers to other names in
other sources. The disassembler uses the name of Table 2 for each opcode.

| Name | Opcodes and modes |
| --- | --- |
| `SLO` | `$03` `($nn,X)`, `$07` `$nn`, `$0F` `$nnnn`, `$13` `($nn),Y`, `$17` `$nn,X`, `$1B` `$nnnn,Y`, `$1F` `$nnnn,X` |
| `RLA` | `$23`, `$27`, `$2F`, `$33`, `$37`, `$3B`, `$3F` (the modes of `SLO`) |
| `SRE` | `$43`, `$47`, `$4F`, `$53`, `$57`, `$5B`, `$5F` (the modes of `SLO`) |
| `RRA` | `$63`, `$67`, `$6F`, `$73`, `$77`, `$7B`, `$7F` (the modes of `SLO`) |
| `DCP` | `$C3`, `$C7`, `$CF`, `$D3`, `$D7`, `$DB`, `$DF` (the modes of `SLO`) |
| `ISB` | `$E3`, `$E7`, `$EF`, `$F3`, `$F7`, `$FB`, `$FF` (the modes of `SLO`) |
| `SAX` | `$83` `($nn,X)`, `$87` `$nn`, `$8F` `$nnnn`, `$97` `$nn,Y` |
| `LAX` | `$A3` `($nn,X)`, `$A7` `$nn`, `$AF` `$nnnn`, `$B3` `($nn),Y`, `$B7` `$nn,Y`, `$BF` `$nnnn,Y` |
| `ANC` | `$0B` `#$nn`, `$2B` `#$nn` |
| `ASR` | `$4B` `#$nn` |
| `ARR` | `$6B` `#$nn` |
| `ANE` | `$8B` `#$nn` |
| `LXA` | `$AB` `#$nn` |
| `SBX` | `$CB` `#$nn` |
| `SBC` | `$EB` `#$nn` (an undocumented copy of `$E9`; Table 2 names it `SBC`) |
| `SHA` | `$93` `($nn),Y`, `$9F` `$nnnn,Y` |
| `SHS` | `$9B` `$nnnn,Y` |
| `SHY` | `$9C` `$nnnn,X` |
| `SHX` | `$9E` `$nnnn,Y` |
| `LAS` | `$BB` `$nnnn,Y` |
| `NOP` | implied: `$1A`, `$3A`, `$5A`, `$7A`, `$DA`, `$FA`; `#$nn`: `$80`, `$82`, `$89`, `$C2`, `$E2`; `$nn`: `$04`, `$44`, `$64`; `$nn,X`: `$14`, `$34`, `$54`, `$74`, `$D4`, `$F4`; `$nnnn`: `$0C`; `$nnnn,X`: `$1C`, `$3C`, `$5C`, `$7C`, `$DC`, `$FC` |
| `KIL` | `$02`, `$12`, `$22`, `$32`, `$42`, `$52`, `$62`, `$72`, `$92`, `$B2`, `$D2`, `$F2` (no operand) |

That is 105 undocumented opcodes. With the 151 documented opcodes, the table has 256
entries.

Two errors in the HRM text. Table 2 and the CPU decode table agree, and this design follows
them:

1. Table 2 shows `$71` as `ADC (zp,Y)`. The mode is `($nn),Y`, as in each other opcode of
   the column `x1` with an odd high digit.
2. The list of the `NOP` opcodes in the text of HRM 3.5 has `4C` (that is `JMP $nnnn`) and
   does not have `$3A`, `$5C`, `$C2` and `$E2`. Table 2 shows these four as `NOP`.

### Hook types (`atari-core`, module `debug`)

The module is `crates/atari-core/src/debug.rs`. It holds the types of the hook. The machine
functions that use them are in `crates/atari-core/src/machine/debug.rs`.

```rust
/// A point in time: the frame, the beam position and the cycle count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub at: Position,
    /// The registers before the instruction. `regs.pc` is the address of the opcode.
    pub regs: Registers,
    /// The bytes at `regs.pc` to `regs.pc + 2`, read with `Machine::debug_peek`.
    pub bytes: [u8; 3],
}

impl InstructionStart {
    pub const fn disassembly(&self) -> Disassembly; // disassemble(regs.pc, bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceKind { Nmi, Irq, Reset }

/// An interrupt or reset sequence (7 CPU cycles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequence {
    /// The first cycle of the sequence.
    pub at: Position,
    pub kind: SequenceKind,
    /// The handler address that the CPU read from the vector: the `PC` after the sequence.
    pub handler: u16,
}

/// One CPU bus access (the machine type `BusAccess` becomes public).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusAccess { pub addr: u16, pub value: u8, pub write: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control { Continue, Stop }

/// How `Machine::run_frame_with` ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd { FrameDone, Stopped }

/// The calls of the hooked run. The default methods do nothing.
pub trait CpuHook {
    /// Before the opcode fetch of each instruction. `Control::Stop` holds the cycle: the
    /// machine stops before the CPU access of this cycle.
    fn instruction(&mut self, start: &InstructionStart) -> Control;
    /// At the end of an interrupt or reset sequence, before `instruction` of the first
    /// instruction of the handler.
    fn sequence(&mut self, sequence: &Sequence) {}
    /// After each CPU bus access, also a dummy access (REQ-203).
    fn access(&mut self, access: BusAccess) {}
}

/// The hook that does nothing (the plain run).
pub struct NoHook;
```

The `Debug` and `PartialEq` derives follow the rule of `architecture.md` for the chip types.
`Registers` is the type of `cpu.md`.

### CPU additions (`cpu.rs`)

The hook needs two read functions. They do not change the CPU.

```rust
/// What the next tick starts, at a boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary { Opcode, Interrupt, Reset, Jammed }

impl Cpu {
    /// `None` inside an instruction or a sequence. `Some(Opcode)` is the same condition as
    /// `at_opcode_fetch()`.
    pub const fn boundary(&self) -> Option<Boundary>;
    /// The vector of the last interrupt sequence (`$FFFA` or `$FFFE`), after the hijack
    /// of REQ-122. The field `vector` of `cpu.md` "State".
    pub const fn vector(&self) -> u16;
}
```

### Machine additions (`machine/debug.rs`)

```rust
impl Machine {
    /// Runs machine cycles until ANTIC ends the frame or the hook returns `Control::Stop`.
    /// A held cycle (see "Held cycle") finishes first, without a new call of the hook.
    pub fn run_frame_with<H: CpuHook>(&mut self, hook: &mut H) -> RunEnd;

    /// `true` while a cycle is held: the machine stopped before an instruction.
    pub const fn is_held(&self) -> bool;

    /// The position of the next cycle, or of the held cycle.
    pub fn position(&self) -> Position;

    /// The instruction of the held cycle, from the current registers and memory. `None`
    /// when no cycle is held.
    pub fn held_instruction(&self) -> Option<InstructionStart>;

    /// A byte as the CPU sees it, without side effects (REQ-087 criterion 5, REQ-090
    /// criterion 5). `None` where the chip has no value to show without a side effect.
    pub fn debug_peek(&self, addr: u16) -> Option<u8>;

    /// The disassembly at `addr`, with the bytes of `debug_peek` (`$FF` for `None`).
    pub fn disassemble_at(&self, addr: u16) -> Disassembly;
}
```

`debug_peek` is `Machine::peek` (system.md), with one difference: it gives `None` for the
cartridge control area `$D500` to `$D5FF`. A read there can switch the cartridge bank
(media.md), and the area has no stored value to show. ANTIC, GTIA, POKEY and the PIA give the
value of a read without its side effect (their `peek` or `read` functions with `&self`). The
unused pages `$D100`, `$D600` and `$D700` give `$FF`, as a CPU read does (REQ-203).

`--` in the monitor output is the `None` of `debug_peek`.

## State

| Owner | Field | Type | Power-on value | Purpose |
| --- | --- | --- | --- | --- |
| `Machine` | `held` | `Option<HeldCycle>` | `None` | The first part of a held cycle (see "Held cycle"). |
| `Machine` | `sequence_start` | `Option<(Position, bool)>` | `None` | The start of the current interrupt or reset sequence, and `true` for reset. Only the hooked run sets it. |
| `TraceWriter` | `out` | `BufWriter<File>` | | The trace file. |
| `TraceWriter` | `from`, `frames` | `u64` | 0, 1 | `--trace-from`, `--trace-frames`. |
| `TraceWriter` | `error` | `Option<io::Error>` | `None` | The first write error. |
| `Monitor` | `points` | `Vec<Point>` | empty | Breakpoints and watches, in the order of the commands. |
| `Monitor` | `hit` | `Option<(BusAccess, InstructionStart)>` | `None` | A watch hit, until the next instruction start. |
| `Monitor` | `open` | `bool` | `false` | The monitor is open: the machine does not run. |

`HeldCycle` holds the `CycleOutput` of the cycle and the SIO cycle number `now` of system.md
"Timing", step 2. Power-on (`power_on`) and `press_reset` set `held` and `sequence_start` to
`None`. The save state (architecture.md, "Save states") does not contain these two fields. The
frontend does not save a state while the monitor is open.

## Behavior

### Hooked run and the cycle order

`run_frame_with` runs the same steps as `run_cycle` (system.md, "Timing"), and adds the hook
calls. One generic function holds the steps. `run_cycle` and `run_frame` call it with
`NoHook`, whose methods are empty and inline. Thus the plain run has no added branch, call or
memory access in each cycle, and REQ-007 applies without change. The teammate measures the
speed of REQ-007 before and after the change (`docs/plan/SLICE-06.md`, WP-3 baseline).

In each cycle of the hooked run, after step 4 "the SIOV shortcut" and before `cpu.tick`:

1. If `out.halt` is false, `out.rdy` is true and `cpu.boundary()` is `Some(Opcode)`, this
   cycle is the opcode fetch of an instruction.
   1. If `sequence_start` is `Some`, the machine calls `hook.sequence` and sets
      `sequence_start` to `None`. The kind is `Reset` for a reset sequence, `Nmi` for the
      vector `$FFFA` and `Irq` for `$FFFE`. The handler address is `cpu.registers().pc`.
   2. The machine calls `hook.instruction`. For `Control::Stop`, the cycle becomes held.
2. If `out.halt` is false, `out.rdy` is true and `cpu.boundary()` is `Some(Interrupt)` or
   `Some(Reset)`, the machine sets `sequence_start` to the position of this cycle.

After `cpu.tick`, if the tick made a bus access, the machine calls `hook.access`.

The conditions `out.halt` and `out.rdy` make the position exact. In a halted cycle the CPU
does not run. With RDY low, the opcode fetch is a read, so the CPU repeats it and does not
advance (REQ-126). The instruction starts in the first cycle where the fetch runs. A stalled
or halted cycle before the fetch is in the gap between two lines of the trace (REQ-086
criterion 4).

The hook is after the SIOV shortcut. With the shortcut on, an instruction at SIOV does not
start, and the trace shows the instruction at the return address (media.md, "SIOV
shortcut").

A `BRK` is an instruction: it has an instruction line and no sequence line. A jammed CPU
(KIL) has the line of its KIL instruction, then no more lines until a reset.

Plain `run_frame` sets `sequence_start` to `None` once at its start (one store in each
frame). So a hooked frame, then a plain frame, leave the machine equal to a plain run of two
frames. A sequence that starts in the last hooked frame and ends in the next frame of the
hooked run keeps its start.

### Held cycle

A stop of the hook happens in the middle of a machine cycle: ANTIC and POKEY ran steps 1 to
3 of the cycle, and the CPU did not access the bus. The machine keeps `HeldCycle` and returns
`RunEnd::Stopped`.

1. The next `run_frame_with`, `run_frame` or `run_cycle` first finishes the held cycle:
   steps 4 to 7, without the SIOV shortcut and without a new hook call for this
   instruction.
2. While a cycle is held, the CPU is at the boundary of an instruction. `registers_mut`
   is permitted (cpu.md, "Call contract"), and the monitor uses it for the `r` command.
3. `position()` gives the position of the held cycle. `debug_peek` gives the values that the
   CPU access of the held cycle sees.
4. The order of the chip calls is the same with and without a stop. A run with stops thus
   gives the same frame buffer and RAM at the same cycle count (REQ-088 criterion 5).

`run_frame` checks `held` once at its start. `run_cycle` checks it in each call; the loop of
`run_frame` calls the inner function and does not check it.

### Trace writer (frontend, `src/trace.rs`)

The command line options (`src/cli.rs`):

| Option | Value | Default | `Options` field |
| --- | --- | --- | --- |
| `--trace <file>` | The trace file | No trace | `trace: Option<TraceOptions>` |
| `--trace-from <frame>` | Decimal frame number | 0 | `TraceOptions::from` |
| `--trace-frames <n>` | Decimal count, at least 1 | 1 | `TraceOptions::frames` |

`--trace-from` or `--trace-frames` without `--trace` is a usage error (exit code 2). `USAGE`
lists the three options (REQ-086 criterion 7).

```rust
pub struct TraceOptions { pub file: PathBuf, pub from: u64, pub frames: u64 }

/// Writes the lines of REQ-086 for the frames `from` to `from + frames - 1`.
pub struct TraceWriter<W: Write> { /* see State */ }

impl<W: Write> TraceWriter<W> {
    pub fn new(out: W, from: u64, frames: u64) -> Self;
    /// `true` while the frame `frame` needs the hooked run.
    pub const fn wants(&self, frame: u64) -> bool;
    /// Flushes the output and gives the first write error.
    pub fn finish(self) -> io::Result<()>;
}

impl<W: Write> CpuHook for TraceWriter<W> { /* writes; always Control::Continue */ }

/// `Display` of one instruction line (REQ-086 criterion 1). The monitor uses it too.
pub struct InstructionLine<'a>(pub &'a InstructionStart);
/// `Display` of one sequence line (REQ-086 criterion 2).
pub struct SequenceLine<'a>(pub &'a Sequence);
```

The line formats:

```text
<frame> <scanline> <hpos> <cycle> <PC>: <bytes> <instruction>  A=<a> X=<x> Y=<y> S=<s> P=<flags>
<frame> <scanline> <hpos> <cycle> <NMI|IRQ|RESET> vector <handler>
```

1. The four numbers are decimal, with one space between them.
2. `<bytes>` is the `len()` bytes, as two hexadecimal digits each with one space between
   them, padded with spaces to 8 characters. One space follows.
3. `<flags>` has one character for each bit from 7 to 0: `N`, `V`, `-`, `B`, `D`, `I`, `Z`,
   `C` for a set bit, `.` for a clear bit.
4. `<handler>` is the handler address `$nnnn`. REQ-086 criterion 2 says `vector <address>`
   and does not say which address. The vector location follows from the kind, so this
   design writes the address that the CPU read from the vector.
5. Each line ends with `\n`.

Examples (P = `$B6`, X = `$FF`):

```text
1 8 42 30822 E477: A9 00    LDA #$00  A=00 X=FF Y=01 S=FD P=N.-B.IZ.
1 248 7 58147 NMI vector $E7B2
```

The writer writes an event when its frame is in the range. The frontend opens the file
before the first frame. A file that cannot be created is the error `FrontendError::Trace`
with the path, exit code 2 (REQ-086 criterion 6). A write error during the run is kept, and
the frontend reports the same error at the end of the run.

`Frontend::run_frame` (`src/frontend.rs`) uses `run_frame_with` when `wants(frame)` is true
for the frame that starts, or when the monitor needs the hooked run. Else it uses
`run_frame`. The frames outside the range thus run at full speed. The machine does not read
the writer, so the frame buffer and the RAM do not depend on `--trace` (REQ-086 criterion 5).

When the trace and the monitor are both on, one frontend type `Hooks` implements `CpuHook`.
It calls the writer first and then the monitor, and it returns the `Control` of the monitor.

### Monitor (frontend, `src/monitor.rs`)

The monitor state and the commands do not need a window or a terminal:

```rust
pub struct Monitor { /* see State */ }

/// The result of one command.
pub struct Reply {
    /// The output text, with `\n` after each line.
    pub text: String,
    /// `true` after `g`: the monitor is closed and the emulation continues in real time.
    pub resume: bool,
}

impl Monitor {
    pub fn new() -> Self;
    /// Adds a breakpoint (the `--break` option and the `b` command).
    pub fn add_breakpoint(&mut self, addr: u16);
    /// `true` when a breakpoint or a watch exists: the frames need the hooked run.
    pub fn active(&self) -> bool;
    pub const fn is_open(&self) -> bool;
    /// Opens the monitor: runs to the next instruction start and gives its line.
    pub fn open(&mut self, machine: &mut Machine) -> String;
    /// Runs one frame with the breakpoints and watches. `Some(text)` when one of them
    /// stopped the machine; the monitor is then open.
    pub fn run_frame(&mut self, machine: &mut Machine) -> Option<String>;
    /// Parses and runs one command line.
    pub fn command(&mut self, machine: &mut Machine, line: &str) -> Reply;
}

/// One parsed command. `parse` is a pure function, for tests.
pub enum Command { Step, Frame, Go, Break(u16), WatchWrite(u16), WatchRead(u16),
    List, Delete(usize), Registers, Memory(u16, u16), Disassemble(u16, u16),
    Enter(u16, Vec<u8>), Register(Reg, u16) }
pub fn parse(line: &str) -> Result<Command, ParseError>;
```

The tests of REQ-088 to REQ-090 call `Monitor::command` with a `Machine` and compare the
text.

#### Commands

| Command | Action | Output |
| --- | --- | --- |
| `s` | Runs the held instruction and the halt and stall cycles before the next instruction. | The next instruction line. |
| `f` | Runs to the first instruction start whose `frame` is larger than the frame of the held cycle. | That instruction line. |
| `g` | Closes the monitor. `Reply::resume` is `true`. | Empty. |
| `b <address>` | Adds a breakpoint. | `break <n> $nnnn` |
| `ww <address>`, `wr <address>` | Adds a write watch or a read watch. | `watch <n> write $nnnn` or `watch <n> read $nnnn` |
| `bl` | Lists the points. | One line for each: `<n>: break $nnnn`, `<n>: watch write $nnnn`, `<n>: watch read $nnnn` |
| `bd <n>` | Deletes point `<n>`. | `deleted <n>` |
| `x` | None. | `PC=nnnn A=nn X=nn Y=nn S=nn P=<flags> scanline <l> hpos <h>` |
| `m <address> [<length>]` | None. | Lines of 16 bytes: `nnnn: <16 bytes> <16 characters>` |
| `d <address> [<count>]` | None. | `<count>` lines: `nnnn: <bytes> <instruction>` |
| `e <address> <byte> [<byte>...]` | Writes the bytes to RAM with `Machine::poke_ram`, at `address` and the next addresses. | `wrote <k> bytes at $nnnn` |
| `r <register> <value>` | Sets `A`, `X`, `Y`, `S`, `P` or `PC` with `registers_mut`. | The `x` output. |

1. "The next instruction line" is `InstructionLine` of `Machine::held_instruction`, the
   format of REQ-086 criterion 1. The same line shows when the monitor opens (REQ-088
   criterion 1). `s` and `f` show the sequence lines of a sequence before the instruction
   too.
2. `<address>`, `<byte>` and `<value>` are hexadecimal, 1 to 4 digits (1 to 2 for a byte),
   with or without `$`. `<length>`, `<count>` and `<n>` are decimal. The defaults are 128
   for `<length>` and 16 for `<count>` (REQ-090 criteria 2 and 3).
3. A command or a value that does not parse gives one line `error: <reason>` and changes
   nothing (REQ-090 criterion 6). `<reason>` names the text that did not parse.
4. The point numbers start at 1 and follow the order in the list. After `bd`, the numbers
   of the next points decrease by 1.
5. `m` shows `--` for each byte where `debug_peek` gives `None`. The characters are the
   ATASCII codes `$20` to `$7E` (with bit 7 clear) as ASCII; each other code, and `--`,
   shows `.`.
6. `d` reads with `Machine::disassemble_at` and starts the next line at `address + len()`.
7. The monitor reads memory only with `debug_peek` and `disassemble_at`. Thus `m` and `d` do
   not change the machine (REQ-090 criterion 5).

#### Breakpoints and watches

`Monitor` implements `CpuHook` for the hooked run:

1. `instruction`: if `hit` is `Some`, return `Stop`. The output is
   `watch <write|read> $nnnn = $nn`, the instruction line of the instruction that made the
   access, then the next instruction line. Else, if `start.regs.pc` is a breakpoint, return
   `Stop` with `break $nnnn` and the instruction line. Else `Continue`.
2. `access`: if the access matches a watch (address and direction), keep it in `hit` with
   the last instruction start. The first hit of an instruction counts. A dummy access is a
   real bus access (REQ-203), so it matches too. ANTIC DMA reads are not CPU accesses and
   do not match.
3. A `s` or `f` command does not stop at a breakpoint at the held instruction itself: the
   held cycle finishes without a hook call.

With no breakpoints and no watches, `active()` is false, and the frontend uses the plain
`run_frame` (REQ-089 criterion 6). The `--break <address>` option (`src/cli.rs`, `Options::
breaks: Vec<u16>`, the option can repeat) adds breakpoints before the first frame
(REQ-089 criterion 4). `USAGE` lists it.

#### Monitor input (OI-057 option 1, open)

1. F8 in the window, or Ctrl+C in the terminal, asks the monitor to open. The emulation
   thread checks the request before each frame and calls `Monitor::open`. The machine thus
   stops at the first instruction of the next frame.
2. Ctrl+C needs a handler of `SIGINT`. The standard library has no safe interface for it,
   and `unsafe` is not permitted. The design uses the `ctrlc` crate (MIT or Apache-2.0) in
   the frontend only. `cargo-deny` checks it. This is a new dependency of the frontend (the
   core keeps REQ-015).
3. A thread reads standard input one line at a time and sends each line on a channel
   (`std::sync::mpsc`). The emulation thread calls `Monitor::command` for each line and
   writes `Reply::text` to standard output, then the prompt `> `.
4. While the monitor is open, the frontend runs no frame, as for a pause (F9). The window
   shows the last frame. The sound stops with `AudioSink::stop_sound`, which is not an
   underrun (REQ-088 criterion 6, REQ-011).
5. When standard input ends (end of file), the frontend runs the command `g`.
6. With `--headless`, a stop reads the commands from standard input on the emulation thread
   and waits for each line. The frame count of `--frames` counts a frame once, when it ends.

## Timing

1. The trace and the monitor take no emulated time. The hook calls and `debug_peek` do not
   change a chip.
2. The position of an instruction is the cycle of its opcode fetch. The difference between
   the cycles of two lines is the cycle count of the first instruction, plus the halted
   cycles and the stalled cycles (RDY low) before the second opcode fetch (REQ-086
   criterion 4).
3. A sequence line has the position of the first cycle of the sequence, and the machine
   gives it at the end of the sequence, because an NMI can change the vector in cycle 5
   (REQ-122). The line comes before the line of the first instruction of the handler.
4. `<cycle>` is `cycle_count()` at the start of the cycle, so the first cycle after power-on
   has the value 0. `<frame>` is `frame_count()` at the start of the cycle.

## Test plan

| Requirement | Test |
| --- | --- |
| REQ-087 | `crates/harness/tests/disasm.rs` `disassembles_all_opcodes` (criteria 1 to 4: the 256 opcodes with the bytes `$34 $12` at `$2000`, length and text from a table in the test, with the names of "Undocumented names"), `branch_targets_wrap` (criterion 2: `BNE` at `$FFF0` with offset `$7F` gives `$0071`; at `$0010` with offset `$80` gives `$FF92`), `disassembly_reads_without_side_effects` (criterion 5: `Machine::disassemble_at` and `Machine::debug_peek` for each address `$D000` to `$D7FF` leave the machine equal to a copy from before) |
| REQ-086 | `crates/harness/tests/debug_hook.rs` `hook_reports_each_instruction_start` (criteria 1 and 4: a program machine with DMA and `WSYNC`; each opcode fetch cycle that a cycle-by-cycle run finds with `Cpu::at_opcode_fetch` has exactly one `instruction` call, with the registers before it), `hook_reports_interrupt_sequences` (criterion 2: the reset after power-on, a VBI NMI and a POKEY timer IRQ each give one `sequence` call with the kind, the start position and the handler address, before the first handler instruction), `hooked_run_matches_plain_run` (criterion 5: 3 frames of the `AltirraOS` boot, plain and hooked, give equal machines); `tests/cli.rs` `trace_line_format` (criteria 1 and 2: `InstructionLine` and `SequenceLine` of fixed events give the example lines of this document), `trace_writes_selected_frame` (criteria 3 and 4: `--headless --frames 3 --trace <file> --trace-from 1 --trace-frames 1`; each line has frame 1, the first line is the first instruction of frame 1, and the cycle of each line minus the cycle of the line before is at least the base cycle count of the instruction before it and equals the count of a cycle-by-cycle run), `trace_does_not_change_machine` (criterion 5: the frame buffer and the RAM after 3 frames with and without the trace), `trace_file_error_exit_code` (criterion 6: a file in a directory that does not exist gives exit code 2 and a message with the path), `trace_help_lists_options` (criterion 7) |
| REQ-088 | `tests/cli.rs` `monitor_open_shows_next_instruction` (criterion 1), `monitor_step_runs_one_instruction` (criterion 2: the cycle of the new line minus the old cycle is the instruction cycles plus the halt cycles), `monitor_frame_runs_to_next_frame` (criterion 3), `monitor_go_continues` (criterion 4: `resume` is true and the next frame runs plain), `monitor_steps_keep_machine_deterministic` (criterion 5: 20 `s`, 2 `f` and a `g` in frame 2, then a run to cycle 5 frames; frame buffer and RAM equal a plain run to the same cycle), `monitor_open_silences_audio` (criterion 6: while open, no frame runs, `AudioSink::stop_sound` is called and no underrun is counted) |
| REQ-089 | `tests/cli.rs` `monitor_breakpoint_stops_before_instruction` (criterion 1), `monitor_watches_stop_after_access` (criterion 2: `ww` and `wr` on a RAM address of a test program; the output names the address, the value and the instruction line), `monitor_lists_and_deletes_points` (criterion 3), `break_option_sets_breakpoint` (criterion 4: `--break 0600` and `--break $E477` parse to `Options::breaks`), `monitor_breakpoint_at_boot_tape_start` (criterion 5: the boot test tape of REQ-643; the machine stops with `PC` = `$0600` before the first instruction there), `no_points_use_plain_run` (criterion 6: without points, `Monitor::active` is false and `Frontend` calls `Machine::run_frame`) |
| REQ-090 | `tests/cli.rs` `monitor_shows_registers` (criterion 1), `monitor_shows_memory` (criterion 2: 16 bytes for each line, hexadecimal and ATASCII, default 128 bytes), `monitor_disassembles_memory` (criterion 3: default 16 instructions), `monitor_changes_memory_and_registers` (criterion 4: `e` and `r` for each register), `monitor_reads_without_side_effects` (criterion 5: `m D000 2048` and `d D000 100` leave the machine equal; `$D500` shows `--`), `monitor_rejects_bad_input` (criterion 6: `m G000`, `e 10000 1`, `r Q 1` give `error:` and no change) |

## Open issues

1. OI-057 is open. This design follows option 1. With option 2 or 3, "Monitor input"
   changes, and the `Monitor` interface stays.
2. Ctrl+C needs the `ctrlc` crate in the frontend (see "Monitor input"). The lead decides
   whether the frontend can have this dependency. Without it, only F8 opens the monitor.
3. REQ-086 criterion 1 does not give the unit of `<hpos>`. This design uses the machine cycle
   in the scan line (0 to 113), the unit of `CycleOutput::cycle`.
4. REQ-086 criterion 2 does not say which address follows `vector`. This design writes the
   handler address.
5. REQ-086 does not say if a `BRK` that an NMI hijacks (REQ-122) has a sequence line. This
   design writes only the `BRK` instruction line.
6. REQ-087 does not give the length of `BRK`. This design uses 1.
7. REQ-089 criterion 2 does not say if a dummy access matches a watch. This design counts
   each CPU bus access.
