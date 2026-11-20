# ANTIC design

## Scope

This document tells how the emulator satisfies the ANTIC requirements REQ-300 to REQ-347. The source of the hardware facts is chapter 4 of the Altirra Hardware Reference Manual (HRM). Some facts come from HRM 2.3, 6.2, 6.9, 6.10 and 7.4.

The module is `crates/atari-core/src/antic.rs`. It contains:

- The ANTIC registers (REQ-300 to REQ-303, REQ-331, REQ-338). The light pen (REQ-342) is withdrawn (OI-016).
- The scan line and frame counters (REQ-304, REQ-305).
- The display list processor (REQ-319 to REQ-330).
- The DMA scheduler: display list, playfield, player/missile and refresh (REQ-309 to REQ-312, REQ-339 to REQ-341).
- The playfield decoder, which makes the 3-bit codes for GTIA (REQ-306 to REQ-308, REQ-313 to REQ-318).
- The NMI logic (REQ-331 to REQ-335) and the `WSYNC` logic (REQ-336, REQ-337).
- The Could behaviors: line buffer replay, mid-line changes, abnormal DMA, the hi-res bug and virtual DMA data (REQ-343 to REQ-347).

Out of scope: 130XE ANTIC banking, the reset NMI of the 400/800 (the 65XE does not connect it), and mixed PAL/NTSC chip sets.

## Interfaces

### Types

```rust
/// Memory reads for ANTIC DMA. The memory map is the same as the CPU's (REQ-213).
pub trait DmaMemory {
    /// Reads one byte for DMA. The only side effect: a read of `$D500-$D5FF` reaches the
    /// cartridge control (REQ-609, OI-002).
    fn dma_read(&mut self, addr: u16) -> u8;
}

/// One 3-bit code on the ANTIC-to-GTIA bus (REQ-307, HRM 6.9 Table 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnCode(u8); // Always 0..=7.

impl AnCode {
    pub const BACKGROUND: Self = Self(0b000);
    pub const VSYNC: Self = Self(0b001);
    pub const HBLANK_LORES: Self = Self(0b010);
    pub const HBLANK_HIRES: Self = Self(0b011);
    /// Lo-res playfield color `PF0`-`PF3` (`n` = 0..=3).
    pub const fn playfield(n: u8) -> Self { Self(0b100 | (n & 3)) }
    /// Hi-res pixel pair; bit 1 = left pixel (`pair` = 0..=3).
    pub const fn hires(pair: u8) -> Self { Self(0b100 | (pair & 3)) }
    pub const fn bits(self) -> u8 { self.0 }
}

/// The kind of bus cycle that ANTIC did in this machine cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmaKind {
    #[default]
    None,
    Missile,
    Player(u8),          // 0..=3
    DisplayList,         // Instruction byte.
    DisplayListAddress,  // LMS or jump address byte.
    CharacterName,
    CharacterData,
    Bitmap,
    Refresh,             // No device drives the data bus.
}

/// What happened in one machine cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CycleOutput {
    /// ANTIC uses the bus. The CPU must not do a bus cycle, read or write (REQ-310).
    pub halt: bool,
    /// The CPU RDY line. `false` = a CPU read cycle must wait and repeat.
    /// A CPU write cycle continues (REQ-336).
    pub rdy: bool,
    /// The kind of DMA cycle. `DmaKind::None` for virtual DMA cycles (REQ-311).
    pub dma: DmaKind,
    /// The byte that ANTIC read. Valid only if `halt` and `dma != Refresh`.
    pub dma_data: u8,
    /// `true` in the tick after the cycle in which the NMI level rises (REQ-334).
    /// For tests and tracing: the CPU uses `nmi_level` (OI-017).
    pub nmi: bool,
    /// The codes for the two color clocks of this cycle, left first (REQ-306, REQ-307).
    pub video: [AnCode; 2],
    /// `true` on cycle 0 of scan line 248: the frame is complete.
    pub frame_done: bool,
    /// The scan line of this cycle. GTIA uses bit 0 for `VDELAY` (HRM 6.5, OI-001).
    pub line: u32,
    /// The cycle of this tick, 0..114.
    pub cycle: u32,
}
```

### Functions

```rust
impl Antic {
    /// Creates ANTIC in the power-on state (REQ-303).
    pub const fn new(standard: VideoStandard) -> Self;

    /// The RESET line: power-on and the RESET key (REQ-302).
    /// Clears NMIEN, DMACTL and the playfield DMA clock only.
    pub fn reset(&mut self);

    /// Reads a register. `reg` is `addr & 0x0F` (REQ-300, REQ-301). No side effects.
    pub fn read(&self, reg: u8) -> u8;

    /// Writes a register. `reg` is `addr & 0x0F`.
    pub fn write(&mut self, reg: u8, value: u8);

    /// Runs one machine cycle (see "Call order").
    pub fn tick(&mut self, mem: &mut impl DmaMemory) -> CycleOutput;

    /// Ends the machine cycle. `data_bus` is the value on the data bus in this cycle.
    pub fn end_cycle(&mut self, data_bus: u8);

    /// The NMI level of the current cycle, after the CPU access of the cycle (OI-017).
    pub fn nmi_level(&self) -> bool;

    /// The scan line of the next `tick`, 0..lines_per_frame.
    pub const fn scanline(&self) -> u32;

    /// The cycle of the next `tick`, 0..114.
    pub const fn cycle(&self) -> u32;

    /// Internal state for tests and the debugger: IR, display list address,
    /// memory scan counter, row counter, DMA clock (in the bit order of the
    /// HRM 4.12 tables), line buffer (48 bytes) and line buffer address,
    /// refresh row counter, and the write-only register values `DMACTL`,
    /// `CHACTL`, `HSCROL`, `VSCROL`, `PMBASE`, `CHBASE` and `NMIEN`.
    pub fn debug_state(&self) -> AnticDebugState;
}
```

### Call order

The machine calls ANTIC exactly once per machine cycle, in this order. `L` is the scan line and `C` is the cycle.

1. `let out = antic.tick(&mut mem);` ANTIC does the work of cycle `C` of scan line `L`. It does the DMA read through `mem` and computes `halt`, `rdy`, `nmi` and `video`. Then it advances its counters to the next cycle.
2. The CPU part of the cycle:
   - If `out.halt` is `true`, the CPU does no bus cycle.
   - If `out.rdy` is `false` and the next CPU bus cycle is a read, the CPU does the read again on the bus, but does not advance.
   - Otherwise, the CPU does its bus cycle. A CPU access to `$D4xx` calls `antic.read` or `antic.write`.
3. The machine finds the data bus value:
   - `out.dma_data`, if `out.halt` and `out.dma != DmaKind::Refresh`.
   - Else the byte of the CPU bus cycle, if there was one.
   - Else the floating bus value (a system decision, REQ-200 to REQ-299).
4. `antic.end_cycle(data_bus);`
5. `gtia.clock(&out, data_bus);` (see "Interface with GTIA").
6. After the CPU access, the machine passes `antic.nmi_level()` to `cpu.end_cycle` (`system.md`, step 4a).

### Register write timing

A register write in cycle `C` comes after `tick` for cycle `C`. Thus the first `tick` that sees the write is the `tick` for cycle `C + 1`. This gives the HRM meaning of "written by cycle N": a write in cycle N changes an event at cycle N + 1.

Some registers have one more cycle of delay. ANTIC keeps a two-stage pipeline for these values:

| Value | First `tick` that uses a write in cycle `C` | Source |
| --- | --- | --- |
| `CHBASE` | `C + 2` | HRM 4.4 (REQ-317) |
| `DMACTL` bits 2, 3 (P/M DMA) | `C + 2` | HRM 4.13 (REQ-339) |
| `DMACTL` bit 5 (display list DMA) | `C + 2` | HRM 4.6 (REQ-326) |
| All other bits and registers | `C + 1` | HRM 4.14 Figure 6 |

### Register reads

`read` uses the state after `tick` for cycle `C`. So a read in cycle `C` sees events of cycle `C`. Example: ANTIC sets the `NMIST` bit in `tick` for cycle 7, and a read on cycle 7 sees it (REQ-334).

### Interface with the machine

The machine must make these changes to the draft code in `machine.rs`:

1. `System::read` waits while `out.halt || !out.rdy`. `System::write` waits only while `out.halt`. The 6502 ignores RDY on write cycles (HRM 4.9).
2. During a RDY wait, the machine repeats the CPU read on the bus in each cycle. This matters only for read-sensitive addresses (HRM 4.9, Bus activity during WSYNC). This is Could behavior.
3. `Machine::reset_key` calls `antic.reset()`. On the XL/XE, the RESET key also resets ANTIC (HRM 4.1).
4. `Machine::cold_reset` calls `antic.reset()`. It does not set the scan line and cycle to 0. `Antic::new` sets them to 0 (REQ-303).
5. After the CPU access of cycle c, the machine passes `antic.nmi_level()` to the CPU as the NMI level of cycle c (OI-017). `out.nmi` is true in the `tick` after the cycle in which the level rises (tick 9, or tick 10 after an enable on cycle 7), for tests and tracing only.

### Interface with GTIA

ANTIC does not hold a reference to GTIA. The machine forwards `CycleOutput` and the data bus value to GTIA once per cycle:

```rust
impl Gtia {
    /// One machine cycle. `antic.video` gives the codes of two color clocks.
    /// `data_bus` is the value on the data bus in this cycle.
    pub fn clock(&mut self, antic: &CycleOutput, data_bus: u8);
}
```

The contract:

1. **Position.** `video[0]` and `video[1]` of cycle `C` are for GTIA horizontal positions `(2C - 2) mod 228` and `(2C - 1) mod 228`. This is a project decision. It comes from HRM 6.10: a color register write on cycle 65 shows at position `$81`, one color clock after position `$80`.
2. **Playfield.** ANTIC sends `%000` (background) or `%1xx` in the playfield. ANTIC already applies the playfield window, the HSCROL delay, the wide-playfield clip at `$22-$2B`, and the hi-res/lo-res encoding.
3. **Horizontal blank.** ANTIC sends `%010` or `%011` outside the visible positions `$22-$DD` (project decision, see REQ-306). The code tells GTIA if the current IR mode is hi-res.
4. **Vertical blank.** ANTIC sends horizontal blank codes for all color clocks of vertical blank scan lines, and `%001` on the vertical sync scan lines. ANTIC has no other vertical blank signal (HRM 6.2).
5. **Player/missile data.** On a P/M DMA cycle, `halt` is `true`, `dma` is `Missile` or `Player(n)`, and `data_bus` is the fetched byte. GTIA can use `dma` directly. For the Could cases of HRM 6.5, GTIA must find the P/M bytes from the position of the halted cycles in horizontal blank. For that purpose, `halt` and `data_bus` are also given for the other cycles.
6. **Scan line.** `out.line` and `out.cycle` give the position of the cycle. GTIA uses bit 0 of `line` only, because `VDELAY` loads DMA data only on odd scan lines (HRM 6.5). This contract is decided in OI-001.

The light pen input goes the other way, from the trigger inputs to ANTIC. The machine forwards it (see above).

## State

All state is in `struct Antic`. Sizes are the sizes in hardware. The Rust type can be larger.

| Field | Size | Power-on value | Reset | Meaning |
| --- | --- | --- | --- | --- |
| `standard` | - | Configuration | Kept | NTSC or PAL |
| `line` | 9 bits | 0 | Kept | Scan line of the next tick |
| `cycle` | 7 bits | 0 | Kept | Cycle of the next tick, 0-113 |
| `total_cycles` | u64 | 0 | Kept | Machine cycles since power-on (for `WSYNC` windows) |
| `dmactl` | 6 bits + 2-stage pipeline | 0 | Cleared | `DMACTL` |
| `chactl` | 3 bits | 0 | Kept | `CHACTL` |
| `dlist` | 16 bits | 0 | Kept | `DLISTL`/`DLISTH` |
| `hscrol` | 4 bits | 0 | Kept | `HSCROL` |
| `vscrol` | 4 bits | 0 | Kept | `VSCROL` |
| `pmbase` | 6 bits | 0 | Kept | `PMBASE` bits 7-2 |
| `chbase` | 7 bits + 2-stage pipeline | 0 | Kept | `CHBASE` bits 7-1 |
| `nmien` | 2 bits | 0 | Cleared | `NMIEN` bits 7-6 |
| `nmist` | 3 bits | `%011` (reads `$7F`) | Kept | `NMIST` bits 7-5 |
| `ir` | 8 bits | 0 | Kept | Instruction register |
| `prev_vs` | 1 bit | 0 | Kept | Bit 5 of the last mode line (kept across vertical blank) |
| `row` | 4 bits | 0 | Kept | Row counter |
| `first_line` | 1 bit | 0 | Kept | Current scan line is the first of the mode line |
| `new_mode_line` | 1 bit | 0 | Kept | The next scan line starts a mode line |
| `jvb_wait` | 1 bit | 0 | Kept | JVB executed, wait for vertical blank |
| `memscan` | 16 bits | 0 | Kept | Memory scan counter |
| `line_buffer` | 48 bytes | 0 | Kept | Line buffer (never cleared) |
| `lb_index` | 6 bits | 0 | Kept | Line buffer address (63-state counter for REQ-345) |
| `dma_clock` | 8 bits | 0 | Cleared | Playfield DMA clock ring |
| `tap_history` | 3 bits | 0 | Cleared | Last 3 outputs of the DMA clock tap |
| `pf_window` | start/stop positions | none | Kept | Display window of the current scan line |
| `shifter` | pipeline of codes | empty | Kept | Pixel codes waiting for their display position |
| `last_invert` | 1 bit | 0 | Kept | Invert/blink state of the last character |
| `refresh_pending` | 1 bit | 0 | Kept | A blocked refresh request waits |
| `refresh_row` | 8 bits | 0 | Kept | Refresh row counter |
| `wsync_start`, `wsync_end` | u64 each | 0 | Kept | RDY-low window, in `total_cycles` |
| `dli_this_line` | 1 bit | 0 | Kept | Result of the DLI check on cycle 5 |
| `nmien_prev` | 2 bits | 0 | Cleared | `NMIEN` bits 6-7 as written by the previous cycles, for `en_by(6)` and `en_by(7)` |
| `nmi_source` | 2 bits | 0 | Kept | R: the NMI source of this scan line (DLI or VBI), set on cycle 7 |
| `nmi_armed` | 2 bits | 0 | Kept | `R && en_by(6)` on cycle 8, `R && en_by(7)` on cycle 9, else 0. `nmi_level()` is `nmi_armed & nmien != 0` |
| `nmi_was_high` | 1 bit | 0 | Kept | NMI level of the cycle before the last tick, for `out.nmi` |
| `virtual_pending` | 1 bit + kind | none | Kept | A virtual fetch waits for `end_cycle` |

## Behavior

### Registers (REQ-300 to REQ-303)

`read` and `write` use `reg & 0x0F`. The machine passes `addr & 0xFF`, so the mask gives the mirrors (REQ-300). `read` returns `$FF` for all offsets except `$B`, `$C`, `$D` and `$F` (REQ-301). `NMIST` reads as `nmist << 5 | 0x1F`. `PENH` always reads `$00` and `PENV` always reads `$FF`, the power-on values of REQ-303, because the emulator has no light pen (OI-016).

`reset` clears `dmactl` (with its pipeline), `nmien`, `dma_clock` and `tap_history`. It keeps all other fields (REQ-302, HRM 4.1).

### Frame and vertical blank (REQ-304, REQ-305, REQ-306)

The frame has 262 (NTSC) or 312 (PAL) scan lines of 114 cycles. Scan lines 8 to 247 are the display region. At cycle 0 of scan line 248, ANTIC:

- Truncates the current mode line. A DLI that was not yet requested does not occur.
- Clears the DMA clock (HRM 4.12, Resetting the playfield clocks).
- Clears `jvb_wait` and IR bit 6 (HRM 4.6). It keeps `prev_vs`.
- Sets `new_mode_line`, so that scan line 8 fetches a new instruction.
- Sets `NMIST` for the VBI on cycle 7 (see NMIs).
- Sets `frame_done` in the output.

During vertical blank, `video` is `HBLANK_LORES` or `HBLANK_HIRES` for every color clock. On scan lines 251-253 (NTSC) or 275-277 (PAL), `video` is `VSYNC` (REQ-306). REQ-346 changes this for the hi-res bug.

### Display list (REQ-319 to REQ-326)

On cycle 1 of a scan line with `new_mode_line`:

- If `jvb_wait` is set, ANTIC replays the JVB as a one-line blank line. No fetch occurs (REQ-322).
- Else, if display list DMA is enabled (pipeline value), ANTIC reads `ir` from `dlist` and increments the low 10 bits of `dlist` (REQ-324).
- Else ANTIC keeps `ir`. No fetch occurs and `dlist` does not change (REQ-326).

On cycles 6 and 7 of that scan line, if a fetch occurred on cycle 1 and the instruction needs an address (LMS on modes 2-F, or mode 1), ANTIC reads two bytes from `dlist`, low byte first. It increments the low 10 bits of `dlist` after each byte.

- Mode 2-F with LMS: the bytes go into `memscan` (REQ-323).
- Mode 1: the bytes go into `dlist` (REQ-321). If IR bit 6 is set, ANTIC sets `jvb_wait` (REQ-322).

Mode 0 (blank) and mode 1 (jump) clear the DMA clock at the start of the line (HRM 4.12). For mode 0, the height is `((ir >> 4) & 7) + 1` (REQ-320). ANTIC treats bits 4-6 of modes 0 and 1 as clear for scrolling (REQ-320, REQ-321).

For Could behavior (REQ-330): a mode 1 line that vertical scrolling extends fetches two address bytes on cycles 6 and 7 of each of its later scan lines too, if display list DMA is enabled. Each pair goes into `dlist`.

### Row counter and vertical scrolling (REQ-312, REQ-328, REQ-329, REQ-330)

`vs` is IR bit 5 for modes 2-F, and 0 for modes 0 and 1. `h` is the normal height of the mode (REQ-312 table, or the blank count).

- **Start row.** On cycle 1 of the first scan line, `row` = `vscrol` if `vs && !prev_vs`, else 0. Tick 1 sees writes up to cycle 0, so it uses the correct `vscrol` value (REQ-329).
- **End row.** `end` = `vscrol` if `!vs && prev_vs`, else `h - 1`.
- **DLI check.** In `tick` for cycle 6 (writes up to cycle 5), `dli_this_line` = IR bit 7 and `row == end` (REQ-329, HRM 4.8).
- **End check.** In `tick` for cycle 109 (writes up to cycle 108), if `row == end`, ANTIC sets `new_mode_line` and `prev_vs = vs`. Else `row = (row + 1) & 15`.

The row counter is 4 bits, so rows past 15 wrap to 0 (REQ-328, REQ-330).

### Playfield DMA clock (REQ-309, REQ-311, REQ-327, REQ-344, REQ-345)

ANTIC models the playfield DMA clock of HRM 4.12. This model gives the normal DMA timing and also the abnormal cases.

- `dma_clock` is an 8-bit ring. Bit 0 is the tap. The ring period `P` is 2, 4 or 8 cycles: fast for modes 2-5 and D-F, medium for 6-7 and A-C, slow for 8-9 (HRM 4.12 Table 7).
- In each `tick`: ANTIC reads the tap (bit 0) into `tap_history`. Then it shifts the ring right by one. A bit that leaves bit 0 enters again at bit `P - 1`. Bits above `P - 1` only shift down.
- **Start event.** ANTIC sets bit 0 for the next cycle. The start event is in `tick` for cycle `S + (hscrol >> 1)`, where `S` is 25, 17 or 9 for a narrow, normal or wide fetch width.
- **Stop event.** ANTIC clears bit 0 for the next cycle. The stop event is in `tick` for cycle `E + (hscrol >> 1)`, where `E` is 89, 97 or 105 for a narrow, normal or wide fetch width.
- The fetch width is the `DMACTL` width, or one step wider if IR bit 4 is set (wide stays wide). The `hscrol` term is 0 if IR bit 4 is clear (REQ-327).
- `DMACTL` and `HSCROL` writes change the start and stop events from the next `tick`. This gives the deadlines of REQ-344.

The taps make the requests (HRM 4.12: offsets 0, +2 and +3):

| Request | Cycle | Modes |
| --- | --- | --- |
| Character name | Tap output on this cycle | 2-7 |
| Bitmap data | Tap output 2 cycles before | 8-F |
| Character data | Tap output 3 cycles before | 2-7 |

Check against REQ-309: a normal width gives the start event on cycle 17, so names start on cycle 18, bitmap data on cycle 20 and character data on cycle 21. The stop event on cycle 97 removes the bit after the last name on cycle 96 (fast), 94 (medium) or 90 (slow). This gives 40, 20 or 10 fetches. The narrow and wide widths give 26/28 and 10/12, and 32 and 48 fast fetches.

Each request does this:

- **Name or bitmap request, first scan line.** If playfield DMA is enabled, ANTIC reads `memscan` and writes the byte into `line_buffer[lb_index]`. If the cycle is 106 or later, the fetch is virtual: no halt, and the byte comes from `end_cycle` (REQ-311, REQ-347). In all cases, ANTIC increments the low 12 bits of `memscan` (REQ-325) and increments `lb_index`.
- **Name or bitmap request, other scan lines.** ANTIC reads `line_buffer[lb_index]` and increments `lb_index`. No DMA occurs.
- **Character data request.** ANTIC reads the font byte (see Character modes). This is DMA on every scan line.

The start event sets `lb_index` to 0 (HRM 4.11, Line buffer addressing). A start event on a scan line where the playfield already started does not set `lb_index` to 0 and does not move the left edge of the display window (HRM 4.11: a later start after its deadline is ignored, REQ-344). Mode 0 and mode 1 lines and vertical blank clear the ring (REQ-345).

If playfield DMA is disabled when the playfield has started, the DMA cycles stop, but `memscan` still increments and the line buffer still loads the data bus value (REQ-344). If the playfield did not start, nothing changes.

For REQ-345, two requests on the same cycle read from the bitwise AND of their addresses. The data goes to both requests. `lb_index` is a 6-bit maximal-length counter with 63 states. States 48-62 do not access the buffer and read as `$FF`. The exact gate behavior is an open issue.

### Character modes (REQ-313 to REQ-317)

The font address for a character data request is:

| Modes | Address |
| --- | --- |
| 2-5 | `(chbase & $FC) << 8 \| (name & $7F) << 3 \| frow` |
| 6-7 | `(chbase & $FE) << 8 \| (name & $3F) << 3 \| frow` |

`chbase` is the pipeline value for the current cycle (REQ-317). `frow` is the font row:

- Modes 2, 4, 6: `frow = row & 7`.
- Mode 3: `frow = row & 7`. ANTIC substitutes `$00` data for rows 8-9 if name bits 6-5 are not `%11`, and for rows 0-1 if they are `%11` (REQ-313).
- Mode 2 with rows 8-9 (extended mode line): `$00` if name bits 6-5 are not `%11` (REQ-330).
- Modes 5, 7: `frow = (row >> 1) & 7`.
- If `CHACTL` bit 2 is set, `frow = 7 - frow`. This does not change which mode 3 rows are `$00` (REQ-314).

Then for modes 2 and 3, if name bit 7 is set: `CHACTL` bit 0 sets the data to `$00`, then `CHACTL` bit 1 inverts the data (REQ-314). ANTIC keeps the attribute state in `last_invert`. It uses that state while playfield DMA is off (REQ-344).

The decoder converts the data byte to codes (REQ-307):

| Modes | Codes |
| --- | --- |
| 2, 3 | Four hi-res pairs, `AnCode::hires(pair)` |
| 4, 5 | Four 2-bit pixels: `%00` → `BAK`, `%01` → `PF0`, `%10` → `PF1`, `%11` → `PF2` or `PF3` (name bit 7) |
| 6, 7 | Eight 1-bit pixels: 0 → `BAK`, 1 → `PF(name >> 6)` |

### Mapped modes (REQ-318)

The decoder converts each bitmap byte to codes:

| Modes | Pixels per byte | Color clocks per pixel | Codes |
| --- | --- | --- | --- |
| 8 | 4 | 4 | 2-bit: `BAK`, `PF0`, `PF1`, `PF2` |
| 9 | 8 | 2 | 1-bit: `BAK`, `PF0` |
| A | 4 | 2 | 2-bit |
| B, C | 8 | 1 | 1-bit |
| D, E | 4 | 1 | 2-bit |
| F | 8 (hi-res) | 1/2 | Four hi-res pairs |

Each byte fills exactly `2 × P` color clocks, so the output is continuous.

### Pixel output (REQ-307, REQ-308, REQ-327)

The decoder puts the codes of each byte into `shifter`, with the position of the first color clock:

- Bitmap byte fetched on cycle `t`: position `2t + 8 + (hscrol & 1)`.
- Character data byte fetched on cycle `t`: position `2t + 6 + (hscrol & 1)`.

The `hscrol & 1` term applies only if IR bit 4 is set. It is the one color clock delay in ANTIC (HRM 4.7).

These offsets are a project decision. They make the playfield start at `$30` for a normal width (HRM 6.2): `2 × 20 + 8 = 48 = $30` and `2 × 21 + 6 = 48`. They also give the one-cycle difference between character data and bitmap data (HRM 4.14, Virtual DMA cycles).

For each color clock at position `x`, `tick` sends:

1. `HBLANK_*` if `x` is outside `$22-$DD`, or if the scan line is in vertical blank (REQ-306).
2. Else `BACKGROUND` if `DMACTL` bits 1-0 are `%00` (REQ-308).
3. Else `BACKGROUND` if `x` is outside the display window (`$40-$BF`, `$30-$CF` or `$2C-$DD`). ANTIC sets the left edge of the window at the start event. It sets the right edge in `tick` for cycle `E` of the `DMACTL` width (89, 97 or 105), without the wider fetch and without `HSCROL`. On a line without scrolling, this is the cycle of the stop event. On a scrolled line, the stop event comes later than the right edge, and the window keeps the displayed width (REQ-327 criterion 6). Both edges come from the displayed width. This gives the 144 color clock case of REQ-344.
4. Else the code from `shifter` for `x`, or `BACKGROUND` if `shifter` has no code for `x`.

### Player/missile DMA (REQ-339, REQ-340)

On scan lines 8 to 247, with the pipeline value of `DMACTL`:

- Cycle 0: missile fetch, if bit 2 or bit 3 is set.
- Cycles 2-5: player 0-3 fetches, if bit 3 is set.

The addresses come from the table in REQ-340. `Y` is the scan line number.

### Memory refresh (REQ-341)

On cycles 25, 29, ..., 57, ANTIC makes a refresh request. If no other DMA uses the cycle, the refresh occurs. Else, if `refresh_pending` is clear, ANTIC sets it. Else ANTIC drops the new request (project decision, REQ-341). On each cycle with no other DMA, a pending refresh occurs and ANTIC clears `refresh_pending`. A refresh increments `refresh_row`. `dma_data` is not valid on a refresh cycle.

### DMA priority (REQ-310)

In each cycle, at most one DMA cycle occurs, with this priority:

1. Missile, display list, player and address fetches (cycles 0-7).
2. Playfield fetches (cycles up to 105).
3. Refresh.

`halt` is `true` if a DMA cycle occurs. A virtual fetch (cycle 106 or later) does not set `halt`.

### NMIs (REQ-331 to REQ-335)

- In `tick` for cycle 7: if `dli_this_line`, ANTIC sets `NMIST` bit 7 and clears bit 6. On scan line 248, ANTIC sets bit 6 and clears bit 7 (REQ-331).
- The NMI level of cycle c is computed after the CPU access of cycle c (OI-017). R is true if the scan line has a DLI (`dli_this_line`) or is scan line 248 (VBI). `en_by(k)` is the matching `NMIEN` bit as written by cycle k, and `en_now` includes a write in cycle c:
  - `nmi(8) = R && en_by(6) && en_now`
  - `nmi(9) = R && en_by(7) && en_now`
  - false on all other cycles.
- A write to `NMIRES` clears `nmist`. It does not change the level (REQ-334).
- The lost NMI of REQ-335 is the CPU rule of REQ-122. ANTIC has no separate mechanism.

The CPU contract: the CPU samples the NMI level at the end of each cycle (`cpu.md`, call order). A level that is first true in cycle 8 gives the earliest sequence on cycle 10, and an enable on cycle 7 gives the earliest sequence on cycle 11 (HRM 4.8, HRM 3.4 Figure 2).

### WSYNC (REQ-336, REQ-337)

A write to `WSYNC` in cycle `C` (absolute cycle `n`) makes a RDY-low window:

- Start: `n + 2`. The CPU runs cycle `C + 1`.
- End (first cycle with RDY high): cycle 105 of the same scan line if `C <= 103`, else cycle 105 of the next scan line.
- If the window is empty (start >= end), it has no effect.
- If a window is active or waits, and the new window starts at or before its end, ANTIC joins them: `start = min`, `end = max`. Else the new window replaces it.

`out.rdy` is `false` if `wsync_start <= total_cycles < wsync_end`. The join gives the RMW cases of REQ-337: `INC WSYNC` writes on cycles 102 and 103 give one window from 104 to 105.

DMA on cycle 105 or 106 delays the CPU restart without special code, because `halt` has priority over the CPU cycle (REQ-336).

### VCOUNT (REQ-338)

`read` of `VCOUNT` returns:

- `((line + 1) >> 1)` if `cycle >= 111`, where the read is in cycle `cycle`. For the last scan line, this gives `$83` (NTSC) or `$9C` (PAL) on cycle 111.
- `$00` on cycles 112 and 113 of the last scan line.
- Else `line >> 1`.

### Light pen (REQ-342, withdrawn)

The light pen is a non-goal (OI-016). `PENH` and `PENV` keep their power-on values (REQ-301). The `set_light_pen` input is removed.

### Hi-res bug (REQ-346)

At cycle 0 of scan line 248, ANTIC saves "IR mode is 2, 3 or F". While that flag is set and `DMACTL` bits 1-0 are not `%00`, ANTIC sets AN2 = 1 in the codes of the vertical blank scan lines for positions `$22-$DD`. Thus it sends `%111` instead of `%011`, and `%101` instead of `%001` on the vertical sync scan lines (HRM 6.2, Figure 14). Positions outside `$22-$DD` keep the horizontal blank code. When `DMACTL` bits 1-0 become `%00`, the normal codes return. The range `$22-$DD` is a project decision (see Open issues).

## Timing

Events in one scan line. "Tick N" is the `tick` call for cycle N. A write "by cycle N" is seen by tick N + 1.

| Tick | Event | Requirement |
| --- | --- | --- |
| 0 | Missile DMA (lines 8-247). New scan line: `first_line` from `new_mode_line`. Line 248: vertical blank start. | REQ-339, REQ-305 |
| 1 | Display list instruction fetch (first scan line of a mode line). Set the start row (VSCROL writes by cycle 0). | REQ-319, REQ-328, REQ-329 |
| 2-5 | Player 0-3 DMA | REQ-339 |
| 6 | DLI check (VSCROL writes by cycle 5) | REQ-329, REQ-332 |
| 6, 7 | LMS or jump address fetch | REQ-321, REQ-323 |
| 7 | `NMIST` bit set (DLI or VBI) | REQ-331, REQ-334 |
| 8 | NMI level (enable by cycle 6, not disabled by cycle 8) | REQ-334 |
| 9 | Wide start event (`+ hscrol >> 1`). NMI level (enable by cycle 7). | REQ-309, REQ-334 |
| 10 / 12 | Wide: first name / bitmap fetch | REQ-309 |
| 17 | Normal start event | REQ-309 |
| 18 / 20 | Normal: first name / bitmap fetch | REQ-309 |
| 25 | Narrow start event. First refresh request. | REQ-309, REQ-341 |
| 26 / 28 | Narrow: first name / bitmap fetch | REQ-309 |
| 25 + 4k (k = 0-8) | Refresh requests, up to 57 | REQ-341 |
| 89 | Narrow stop event | REQ-309 |
| 97 | Normal stop event | REQ-309 |
| 103 | Last cycle of a `WSYNC` write that waits on this scan line | REQ-337 |
| 105 | Wide stop event. RDY high (end of `WSYNC` wait). Last cycle of real playfield DMA. | REQ-309, REQ-311, REQ-336 |
| 106-113 | Playfield fetches are virtual | REQ-311, REQ-347 |
| 109 | End check (VSCROL writes by cycle 108): end of mode line or `row + 1` | REQ-328, REQ-329 |
| 111 | `VCOUNT` shows the next scan line (`$83`/`$9C` on the last scan line) | REQ-338 |
| 112 | `VCOUNT` = 0 on the last scan line | REQ-338 |
| 113 | Last cycle. After it, `line` increments. | REQ-304 |

All start and stop events move one cycle later for each 2 of `HSCROL` on scrolled lines (HRM 4.14 Figure 6, note 1).

## Test plan

### Test methods

Unit tests are in `crates/atari-core/src/antic.rs` (`#[cfg(test)] mod tests`). They call `tick`, `read`, `write`, `end_cycle`, `reset` and `nmi_level` directly, between two ticks. They use these helpers:

- **Fake memory.** A fake `DmaMemory` holds 64 KB. It logs each `dma_read` as `(cycle, address)`. It uses interior mutability (`RefCell`), because `dma_read` takes `&self`. A test finds the address of each fetch from this log.
- **Output record.** The test records the `CycleOutput` of each cycle. "`out.dma` is X on cycle t" means the record of cycle t.
- **Positions.** The test converts `video[0]` and `video[1]` of cycle `C` to the positions `(2C - 2) mod 228` and `(2C - 1) mod 228` (see "Interface with GTIA", item 1). "The code at position x" means the code for that position.
- **Data bus.** The test passes the data bus value to `end_cycle`: `out.dma_data` on DMA cycles, and a value that the test selects on the other cycles (for example `$FF`, or `$A5` on one cycle).
- **Two instances.** For "no effect" criteria, the test runs two `Antic` instances from the same state with the same fake memory, and compares the records of each cycle.
- **Debug state.** The test reads IR, the display list address, the memory scan counter, the row counter, the DMA clock, the line buffer and the register values with `debug_state`.

System tests are in `crates/harness/tests/` and run 6502 test programs on the full machine (`Machine`, see `docs/design/system.md`). A system test finds the cycle of a CPU write as follows: it calls `run_cycle` once at a time and calls `peek` on the target RAM address after each cycle. The write cycle is the first cycle after which the address has the new value. The scan line position is `cycle_count() % 114` (cycle) and `(cycle_count() / 114) % 262` (NTSC scan line), because ANTIC starts at scan line 0, cycle 0 at power-on. The test program uses `STA WSYNC` on a free scan line to put its first instruction on a known cycle, and then counts cycles.

Machine unit tests are in `crates/atari-core/src/machine.rs`. They are necessary where a criterion needs both the CPU bus and the ANTIC internal state (REQ-347).

### Requirement to test map

| Requirement | Test |
| --- | --- |
| REQ-300 | `antic.rs` `registers_mirror_every_16_bytes` (2), `dmactl_mirror_write_gives_same_output` (3), `nmist_mirror_d4ff_reads_7f` (4); `crates/harness/tests/antic_registers.rs` `vcount_mirror_d47b_reads_vcount` (1) |
| REQ-301 | `antic.rs` `unassigned_reads_return_ff` (1), `vcount_penh_penv_nmist_offsets` (2, 3), `writes_to_unused_offsets_are_ignored` (4), `reads_have_no_side_effects` (5). Criterion 6: the tests of the requirements that it names. |
| REQ-302 | `antic.rs` `reset_clears_nmien_dmactl_and_dma_clock` (1, 2), `reset_keeps_other_state` (3), `reset_keeps_position_and_vcount` (4), `reset_keeps_wsync_wait` (5); `crates/harness/tests/antic_registers.rs` `reset_key_keeps_vcount` (4, with `press_reset` and `peek($D40B)`) |
| REQ-303 | `antic.rs` `power_on_values_penh_penv_nmist` (1-3) |
| REQ-304 | `antic.rs` `ntsc_frame_is_262_lines_of_114_cycles`, `pal_frame_is_312_lines_of_114_cycles` (1, 2), `vbi_nmi_period_ntsc_pal` (3) |
| REQ-305 | `antic.rs` `no_dma_in_vertical_blank` (1), `mode_line_truncated_at_248_skips_dli` (2), `long_display_list_alternates_frames` (3), `display_list_resumes_on_line_8` (4), `vblank_has_no_playfield_codes` (5) |
| REQ-306 | `antic.rs` `hblank_code_shows_hires_mode` (1), `vsync_lines_ntsc_251_to_253` (2), `vsync_lines_pal_275_to_277` (3), `vblank_lines_send_hblank_code` (4), `vblank_hires_ir_without_playfield_sends_011` (5) |
| REQ-307 | `antic.rs` `hires_mode_sends_pair_codes` (1), `hires_zero_pair_is_100` (2), `lores_background_is_000` (3), `border_is_background` (4), `mode_4_bit7_selects_pf3` (5) |
| REQ-308 | `antic.rs` `narrow_playfield_at_40_to_bf` (1), `normal_playfield_at_30_to_cf` (2), `wide_playfield_clipped_at_22_to_2b` (3), `playfield_off_shows_background` (4) |
| REQ-309 | `antic.rs` `dma_cycles_mode_e_normal` (1), `dma_cycles_mode_2_normal_first_line` (2), `dma_cycles_mode_2_later_lines` (3), `dma_cycles_mode_8_narrow` (4), `dma_cycles_all_modes_and_widths` (5), `memscan_counts_fetches` (6) |
| REQ-310 | `antic.rs` `dma_cycles_set_halt` (1), `dmactl_zero_halts_only_refresh` (3), `blank_line_halt_map` (4); `crates/harness/tests/antic_dma.rs` `cpu_write_waits_for_dma` (2); `antic.rs` `dma_cycles_all_modes_and_widths` |
| REQ-311 | `antic.rs` `fetch_on_cycle_106_is_virtual` (1), `wide_mode_e_advances_48` (2), `virtual_fetch_loads_data_bus` (3), `fetch_on_cycle_105_halts` (4) |
| REQ-312 | `antic.rs` `display_list_of_24_mode_2_lines` (1), `mode_heights_match_table` (2), `mapped_mode_fetches_first_line_only` (3), `mode_5_fetches_font_every_line` (4), `mode_a_uses_line_buffer` (5), `row_counter_0_to_h_minus_1` (6) |
| REQ-313 | `antic.rs` `mode_2_3_font_bit_order` (1), `mode_2_forty_characters` (2), `mode_2_font_address` (3), `mode_3_lower_rows_blank` (4), `mode_3_descender_rows` (5), `mode_3_blank_rows_inverted` (6) |
| REQ-314 | `antic.rs` `chactl_blink_blanks_bit7_chars` (1), `chactl_invert` (2), `chactl_blink_and_invert_gives_solid` (3), `chactl_bit7_clear_unchanged` (4), `chactl_no_effect_modes_4_to_7` (5), `chactl_reflect_all_char_modes` (6), `chactl_reflect_keeps_mode_3_blank_rows` (7) |
| REQ-315 | `antic.rs` `mode_4_color_pairs` (1), `mode_4_bit7_selects_pf3` (2), `mode_4_font_address` (3), `mode_4_forty_characters` (4), `mode_5_double_rows` (5) |
| REQ-316 | `antic.rs` `mode_6_zero_bit_is_background` (1), `mode_6_color_from_name_bits` (2), `mode_6_twenty_characters` (3), `mode_6_font_address_512_aligned` (4), `mode_7_double_rows` (5) |
| REQ-317 | `antic.rs` `chbase_write_one_cycle_before_is_too_late` (1), `chbase_takes_effect_after_two_cycles` (2), `chbase_bit1_latent_for_mode_6` (3) |
| REQ-318 | `antic.rs` `mapped_two_bit_encoding` (1), `mapped_one_bit_encoding` (2), `mode_f_hires_pairs` (3), `mapped_bytes_per_line` (4) |
| REQ-319 | `antic.rs` `instruction_fetch_on_cycle_1` (1, 2), `address_fetch_on_cycles_6_7` (3), `dlist_write_changes_next_fetch` (5). Criterion 4: the tests of the requirements that it names. |
| REQ-320 | `antic.rs` `blank_line_counts_1_to_8` (1), `blank_line_has_no_playfield_dma` (2), `blank_line_ignores_lms_and_scroll_bits` (3), `blank_line_dli` (4), `blank_line_ends_scroll_region` (5) |
| REQ-321 | `antic.rs` `jump_loads_dlist` (1), `jump_shows_one_blank_line` (2), `jump_ignores_scroll_bits` (3), `jump_crosses_1k` (4), `jump_dli` (5) |
| REQ-322 | `antic.rs` `jvb_stops_fetch_until_vblank` (1), `jvb_loads_address_on_first_line` (2), `jvb_restarts_on_line_8` (3), `jvb_dlist_write_replaces_address` (4), `jvb_with_dli_fires_every_line` (5), `jvb_after_240_lines_alternates_frames` (6) |
| REQ-323 | `antic.rs` `lms_loads_memscan` (1), `no_lms_continues_memscan` (2), `lms_adds_no_blank_line` (3), `lms_address_on_cycles_6_7` (4); `antic.rs` `address_fetch_on_cycles_6_7` |
| REQ-324 | `antic.rs` `dlist_wraps_in_1k_block` (1), `dlist_wraps_inside_lms` (2), `jump_crosses_1k` (3) |
| REQ-325 | `antic.rs` `memscan_wraps_in_4k_block` (1, 2), `lms_loads_16_bits` (3) |
| REQ-326 | `antic.rs` `dl_dma_off_keeps_address` (1), `dl_dma_off_repeats_ir` (2), `dl_dma_off_repeated_dli` (3), `dl_dma_off_after_jvb_is_blank` (4), `dl_dma_enable_on_113_works` (5), `dl_dma_enable_on_0_too_late` (6) |
| REQ-327 | `antic.rs` `hscrol_widens_fetch` (1), `hscrol_0_windows_wider_playfield` (2), `hscrol_4_shifts_one_char` (3), `hscrol_delays_fetch` (4), `hscrol_odd_adds_one_color_clock` (5), `hscrol_window_width_unchanged` (6), `hscrol_change_between_rows` (7) |
| REQ-328 | `antic.rs` `vscroll_region_17_lines` (1, 2), `vscrol_high_bits_ignored` (3), `vscroll_4bit_row_wrap_mode_f` (4), `vscroll_region_ends_on_blank_line` (5), `vscroll_bit_kept_across_vblank` (6) |
| REQ-329 | `antic.rs` `vscrol_start_deadline_cycle_0` (1), `vscrol_end_deadline_cycle_108` (2), `vscrol_dli_deadline_cycle_5` (3, 4) |
| REQ-330 | `antic.rs` `extended_jump_refetches_each_line` (1), `extended_mode_4_repeats_rows` (2), `extended_mode_2_blank_rows_8_9` (3), `extended_mode_f_repeats_data` (4), `extended_blank_line_16` (5), `vscrol_change_mid_line_repeats_rows` (6) |
| REQ-331 | `antic.rs` `nmist_low_bits_read_one` (1), `dli_clears_vbi_bit` (2), `vbi_clears_dli_bit` (3), `nmist_set_when_disabled` (4), `nmires_clears_status` (5), `dli_bit_stays_set` (6), `nmist_bit5_stays_clear` (7) |
| REQ-332 | `antic.rs` `dli_on_last_line_of_mode_line` (1), `dli_masked_by_nmien` (2), `dli_on_blank_line` (3), `dli_on_shortened_line` (4), `dli_on_jump` (5) |
| REQ-333 | `antic.rs` `vbi_on_line_248` (1), `vbi_masked_by_nmien` (2); `crates/harness/tests/antic_nmi.rs` `os_vbi_increments_rtclok` (3) |
| REQ-334 | `antic.rs` `nmist_visible_from_cycle_7` (1), `nmien_enable_on_7_delays_nmi` (3), `nmien_enable_on_8_too_late` (4), `nmien_disable_on_8_prevents` (5), `nmires_does_not_cancel` (6); `crates/harness/tests/antic_nmi.rs` `nmi_sequence_start_cycles` (2, 3); `crates/harness/tests/antic_cpu.rs` `nmi_start_cycles_with_real_cpu` (2, 5; joint ANTIC and CPU test, OI-017) |
| REQ-335 | `crates/harness/tests/antic_nmi.rs` `brk_on_cycle_4_skips_vbi_handler` (1), `brk_on_cycle_3_or_5_keeps_vbi_handler` (2); `machine.rs` `brk_on_cycle_4_loses_vbi_nmi` |
| REQ-336 | `antic.rs` `wsync_rdy_window` (5), `halt_on_cycles_105_106_setups` (supports 3, 4); `crates/harness/tests/antic_wsync.rs` `wsync_next_instruction_on_104` (1), `wsync_blocked_next_cycle_starts_on_105` (2), `wsync_delayed_by_playfield_105` (3), `wsync_delayed_by_refresh_105` (4), `wsync_delayed_by_refresh_106` (4) |
| REQ-337 | `antic.rs` `wsync_write_on_104_waits_next_line` (supports 1), `wsync_windows_join` (supports 2, 3); `crates/harness/tests/antic_wsync.rs` `wsync_write_on_104_next_line` (1), `inc_wsync_ignores_dma` (2), `inc_wsync_on_103_104` (3), `wsync_write_on_113` (4) |
| REQ-338 | `antic.rs` `vcount_changes_on_cycle_111` (1), `vcount_line_7_reads_04` (2), `vcount_7c_for_13_cycles` (3), `vcount_end_of_frame_anomaly_ntsc`, `vcount_end_of_frame_anomaly_pal` (4), `vcount_range` (5) |
| REQ-339 | `antic.rs` `missile_dma_on_cycle_0` (1), `player_dma_forces_missile_dma` (2), `player_dma_cycles_2_to_5` (3), `no_pm_dma_in_vblank` (4), `pm_dma_disable_two_cycles_ahead` (5), `pm_dma_enable_two_cycles_ahead` (6), `pm_dma_data_on_bus` (7) |
| REQ-340 | `antic.rs` `pm_address_two_line` (1), `pm_address_one_line` (2), `pm_two_line_repeats` (3), `pmbase_bit2_latent` (4) |
| REQ-341 | `antic.rs` `refresh_cycles_on_blank_line` (1), `refresh_in_mode_2_first_line` (2), `refresh_narrow_mode_2_two_cycles` (3), `refresh_no_memory_read` (4), `refresh_chart_cases` (5) |
| REQ-343 | `antic.rs` `line_buffer_never_cleared` (1), `line_buffer_partial_load_keeps_tail` (2), `line_buffer_replay_without_dma` (3), `line_buffer_reinterpreted_by_mode` (4), `line_buffer_follows_left_border` (5) |
| REQ-344 | `antic.rs` `width_change_narrow_to_normal_gives_144` (1), `late_start_ignored` (2), `early_start_after_deadline_no_playfield` (3), `early_stop_extends_to_wide` (4), `playfield_off_mid_line_counts_on` (5), `playfield_off_before_start_no_change` (6), `hscrol_bit0_mid_line` (7) |
| REQ-345 | `antic.rs` `abnormal_dma_hscrol_0_to_4_mode_8` (1), `overlap_uses_address_and` (2), `blank_line_clears_dma_clock` (3), `mode_8_after_scrolled_mode_2_bug` (4), `narrow_width_no_mode_8_bug` (5), `line_buffer_counter_63_states` (6) |
| REQ-346 | `antic.rs` `hires_ir_at_248_opens_border` (1), `hires_bug_corrupts_vsync_code` (1), `hires_bug_pal_vsync` (2), `jvb_at_248_no_bug` (3), `hires_bug_ends_when_playfield_off` (4), `hires_ir_with_playfield_off_no_bug` (5), `hires_bug_region_follows_width` (6) |
| REQ-347 | `antic.rs` `virtual_fetch_loads_data_bus` (1), `virtual_fetch_with_refresh_uses_data_bus` (3); `machine.rs` `virtual_fetch_during_wsync_reads_cpu_byte` (2) |

The numbers in parentheses are the acceptance criteria that the test checks.

### New tests

These tests are new in this revision of the test plan. Each line tells what the test does.

- `dmactl_mirror_write_gives_same_output`: two instances, one writes `$22` to `$D4F0` and one to `$D400`; compares the records of the next frame.
- `nmist_mirror_d4ff_reads_7f`: reads `$D4FF` and `$D40F` after power-on; both give `$7F`.
- `vcount_penh_penv_nmist_offsets`: reads offsets `$B`, `$C`, `$D` and `$F` at power-on, on scan line 100, cycle 50, and after an `NMIRES` write.
- `reset_keeps_position_and_vcount`: calls `reset` on scan line 128, cycle 50; checks `scanline()`, `cycle()` and `VCOUNT` = `$40`.
- `reset_keeps_wsync_wait`: writes `WSYNC` on cycle 50 and calls `reset` before tick 52; checks `out.rdy` on cycles 52-105.
- `vbi_nmi_period_ntsc_pal`: with `NMIEN` = `$40`, measures the cycles between the `out.nmi` cycles for 10 frames.
- `vblank_has_no_playfield_codes`: checks that every code on the vertical blank scan lines is `%001`, `%010` or `%011`.
- `vblank_hires_ir_without_playfield_sends_011`: IR = `$02` at scan line 248 and `DMACTL` = `$20`; checks `%011` and `%001` on the vertical blank scan lines.
- `dma_cycles_all_modes_and_widths`: for modes 2-F and the three widths, compares the fetch cycles and `out.halt` with the REQ-309 table.
- `display_list_of_24_mode_2_lines`: runs the Graphics 0 display list of REQ-312 criterion 1; checks the scan lines of the instruction fetches.
- `row_counter_0_to_h_minus_1`: for modes 2-F, reads the row counter on the first and last scan line of a mode line.
- `mode_2_3_font_bit_order`: font byte `$9C` in modes 2 and 3; checks the 4 codes.
- `mode_3_blank_rows_inverted`: `CHACTL` = `$02`, mode 3 name `$C1`; checks `%111` on rows 8-9.
- `chactl_bit7_clear_unchanged`: name `$41` with `CHACTL` = `$01`, `$02` and `$03`; checks the normal codes.
- `mode_4_font_address`: checks the font address `$E208` for name `$C1` and `CHBASE` = `$E0`.
- `mode_4_forty_characters`: 40 names with different font bytes; checks positions `$30 + 4k`.
- `mode_6_zero_bit_is_background`: font byte `$80`; checks `%100` on the first color clock and `%000` on the other 7.
- `mode_6_twenty_characters`: 20 names with different font bytes; checks positions `$30 + 8k`.
- `chbase_write_one_cycle_before_is_too_late`: writes `CHBASE` on cycle 50; checks the fetch address on cycle 51.
- `blank_line_ignores_lms_and_scroll_bits`: `$70` makes no address fetch and does not start a scroll region with `VSCROL` = 4.
- `blank_line_ends_scroll_region`: `$70` after a `$22` line with `VSCROL` = 2 has 3 scan lines.
- `jump_dli`: `$81 $00 $30` with `NMIEN` = `$80`; checks `NMIST` and `out.nmi` on the jump scan line.
- `jvb_loads_address_on_first_line`: reads the display list address in the debug state from cycle 8 of the JVB scan line.
- `lms_address_on_cycles_6_7`: checks the `DisplayListAddress` fetches and their addresses on cycles 6 and 7.
- `dl_dma_off_keeps_address`: with `DMACTL` = `$02`, the display list address does not change for a frame.
- `dl_dma_off_after_jvb_is_blank`: display list DMA off after a JVB; checks `%000` codes and no display list DMA for two frames.
- `hscrol_window_width_unchanged`: `$1F` line with `HSCROL` = 0-15; checks the `%111` positions for normal and narrow width.
- `vscrol_high_bits_ignored`: compares a `VSCROL` write of `$F3` with a write of `$03`.
- `extended_mode_f_repeats_data`: `$0F` after `$2F` with `VSCROL` = 3; checks 4 scan lines with the same codes and DMA only on the first.
- `extended_blank_line_16`: `$00` after `$22` with `VSCROL` = 15; checks 16 blank scan lines.
- `vscrol_change_mid_line_repeats_rows`: writes `VSCROL` = 1 on row 2 of a region-end line; checks 18 scan lines.
- `nmist_bit5_stays_clear`: after an `NMIRES` write, checks bit 5 for 2 frames and after a reset.
- `os_vbi_increments_rtclok`: boots AltirraOS and checks that `$14` increases by 1 in each frame. It replaces `os_boots_with_vbi`.
- `nmi_sequence_start_cycles`: a DLI on a `$80` line with `NOP` loops at two alignments, and an `NMIEN` write on cycle 7; finds the start of the NMI sequence from the cycle of its first stack write (sequence cycle 3).
- `nmi_start_cycles_with_real_cpu`: the four cases of the OI-017 WP-5 measurement with the real CPU: `NOP` on cycle 8 and 9 of a DLI line, `NMIEN` = `$00` written on cycle 8 and on cycle 9, and a `BRK` on cycle 5 of scan line 248 (NMI handler only, pushed `P` with bit 4 = 1).
- `brk_on_cycle_4_skips_vbi_handler`: a `BRK` with its opcode fetch on cycle 4 of scan line 248; the NMI handler writes a marker to RAM, and the marker does not change in that frame. It replaces `irq_on_cycle_4_skips_dli_handler`.
- `brk_on_cycle_3_or_5_keeps_vbi_handler`: the same with the `BRK` on cycle 3 and on cycle 5; the marker changes once.
- `halt_on_cycles_105_106_setups`: checks `out.halt` on cycles 104-107 in the three setups of REQ-336 criteria 3 and 4.
- `wsync_delayed_by_refresh_105`: the `$12` line with `DMACTL` = `$21` and `HSCROL` = 14 of REQ-336 criterion 4; checks the `STX` write on cycle 108.
- `wsync_write_on_104_next_line`: `STA WSYNC` write on cycle 104; checks the `STX` write on cycle 107 of the next scan line.
- `pm_dma_data_on_bus`: checks `out.halt` and `out.dma_data` on the P/M DMA cycles against the fake memory.
- `refresh_narrow_mode_2_two_cycles`: first scan line of a narrow mode 2 line; checks refresh on cycles 25 and 90 only.
- `refresh_no_memory_read`: checks that the fake memory log has no read on refresh cycles.
- `line_buffer_follows_left_border`: replays a normal mode A buffer with narrow width; checks that B0 starts at `$40`.
- `playfield_off_before_start_no_change`: writes `DMACTL` = `$20` on cycle 10; checks the memory scan counter and the line buffer.
- `hires_bug_pal_vsync`: the hi-res bug on PAL; checks `%101` at `$80` on scan lines 275-277.
- `hires_ir_with_playfield_off_no_bug`: IR = `$0F` at scan line 248 with `DMACTL` = `$20`; checks the normal codes.
- `virtual_fetch_with_refresh_uses_data_bus`: the `$12` wide line with `HSCROL` = 2; passes `$5A` on cycle 106 and checks positions `$DA-$DD`.
- `virtual_fetch_during_wsync_reads_cpu_byte` (`machine.rs`): `STA WSYNC` on cycle 104 of a scrolled wide mode E line; checks line buffer byte 47 = `$EA`. It moves from `antic.rs`.

### Removed tests

These tests are removed, because no acceptance criterion needs them: `crates/harness/tests/antic_timing.rs` `vbi_period_matches_frame_length` (the CPU instruction length makes the handler start vary, see `vbi_nmi_period_ntsc_pal`), `crates/harness/tests/antic_dma.rs` `cpu_cycles_per_line_match_dma_map`, `crates/harness/tests/antic_display.rs` `os_screen_inverse_cursor` and `pacem_in_terris_late_dmactl`, `crates/harness/tests/antic_nmi.rs` `race_in_space_dli_every_line` and `dli_changes_color_mid_screen`, and `antic.rs` `refresh_deferred_to_free_cycle`, `refresh_second_blocked_request_dropped` and `wsync_rdy_ignored_for_writes` (REQ-341 criteria 2 and 3 and REQ-337 criteria 2 and 3 cover them). The tests that need commercial software cannot run in CI.

## Open issues

1. **Horizontal blank positions.** The HRM does not give the cycles where ANTIC starts and ends horizontal blank on visible scan lines. This design uses the visible range `$22-$DD` (HRM 6.2). The GTIA design must agree.
2. **Cycle-to-position mapping.** The mapping "cycle C = positions 2C - 2 and 2C - 1" and the fetch-to-display offsets (+8 and +6 color clocks) come from HRM 6.2 and the 6.10 example. They are not stated directly. They must be checked against Altirra test results.
3. **PAL vertical sync lines.** HRM 4.6 says 275-278, HRM 6.2 says 275-277. This design uses 275-277.
4. **VSCROL end deadline.** HRM 4.7 says cycle 108, HRM 14.6 says cycle 109. This design uses 108.
5. **Playfield DMA on cycle 105.** HRM 4.9 and 4.14 allow cycle 105. HRM 4.12 says cycles 105-111 and 0 are suppressed for abnormal DMA. This design allows real DMA up to cycle 105. Abnormal DMA is suppressed on cycles 105-111 and 0 (OI-018, decided).
6. **NMI contract with the CPU.** Resolved by OI-017: the CPU samples the NMI level after its bus access, and the level includes `NMIEN` writes of the same cycle.
7. **DMA reads of I/O addresses.** The HRM does not say what ANTIC DMA reads from `$D000-$D7FF`. The draft `DmaMemory` returns `$FF`. The system design must decide.
8. **Floating bus value.** The data bus value on refresh cycles and undriven cycles is a system decision (HRM 2.3). XL and some XE machines have pull-ups; others float.
9. **Abnormal DMA details.** The ring model is from HRM 4.12. The behavior of bits outside the ring after a mode change, and the shift clock in mode 8, need tests against Altirra.
10. **Character data fetch after the end of the line.** With a wide fetch and large `HSCROL`, a name on cycle 111 has its data request on cycle 114. This design drops requests that fall past cycle 113. The HRM does not describe this case.
11. **Refresh drop rule.** The HRM does not say which request ANTIC drops when two refresh requests are blocked. This design keeps the waiting request.
12. **PENH phase.** `PENH = 2 × cycle` is a project decision.
13. **Hi-res bug region.** Resolved by OI-015: the playfield positions of the current width (REQ-346 criterion 6).
14. **Name and invert state while playfield DMA is off.** Resolved by OI-014 for the line buffer. The codes of the removed REQ-344 criterion 8 need a comparison with Altirra.
15. **DMA clock bit order.** REQ-345 criterion 1 uses the bit order of the HRM 4.12 table. `debug_state` must give the DMA clock in that bit order. The mapping from the ring model to that bit order is not yet defined.
16. **Derived DMA cycles.** Resolved by OI-006: the HRM 4.14 charts confirm the derived cycles.
