//! ANTIC: the display list processor.
//!
//! Design: `docs/design/antic.md`. Requirements: REQ-300 to REQ-347. The interface with
//! GTIA is decided in OI-001: the machine forwards [`CycleOutput`] to `Gtia::clock`.
//!
//! The hardware source is chapter 4 of the Altirra Hardware Reference Manual (HRM).

mod decode;
#[cfg(test)]
mod tests;

use crate::timing::{CYCLES_PER_LINE, VBLANK_START_LINE, VideoStandard};
use decode::{
    blink_invert, char_codes, font_address, font_row, height, is_char, is_hires, mapped_codes,
    mode, period,
};

/// Memory reads for ANTIC DMA. The memory map is the same as the CPU's (REQ-213).
pub trait DmaMemory {
    /// Reads one byte for DMA. The only side effect: a read of `$D500-$D5FF` reaches the
    /// cartridge control (REQ-609, OI-002).
    fn dma_read(&mut self, addr: u16) -> u8;
}

/// One 3-bit code on the ANTIC-to-GTIA bus (REQ-307, HRM 6.9 Table 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnCode(u8);

impl AnCode {
    /// Background.
    pub const BACKGROUND: Self = Self(0b000);
    /// Vertical sync.
    pub const VSYNC: Self = Self(0b001);
    /// Horizontal blank, low resolution.
    pub const HBLANK_LORES: Self = Self(0b010);
    /// Horizontal blank, high resolution.
    pub const HBLANK_HIRES: Self = Self(0b011);

    /// Low resolution playfield color `PF0`-`PF3` (`n` = 0..=3).
    #[must_use]
    pub const fn playfield(n: u8) -> Self {
        Self(0b100 | (n & 3))
    }

    /// High resolution pixel pair; bit 1 = left pixel (`pair` = 0..=3).
    #[must_use]
    pub const fn hires(pair: u8) -> Self {
        Self(0b100 | (pair & 3))
    }

    /// The 3 code bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// The code with AN2 set (the hi-res bug, REQ-346).
    const fn with_an2(self) -> Self {
        Self(self.0 | 0b100)
    }
}

/// The kind of bus cycle that ANTIC did in this machine cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmaKind {
    /// No DMA.
    #[default]
    None,
    /// Missile graphics byte.
    Missile,
    /// Player graphics byte for player 0-3.
    Player(u8),
    /// Display list instruction byte.
    DisplayList,
    /// LMS or jump address byte.
    DisplayListAddress,
    /// Character name byte.
    CharacterName,
    /// Character data byte.
    CharacterData,
    /// Bitmap byte.
    Bitmap,
    /// Memory refresh. No device drives the data bus.
    Refresh,
}

/// What happened in one machine cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per hardware signal"
)]
pub struct CycleOutput {
    /// ANTIC uses the bus. The CPU must not do a bus cycle, read or write (REQ-310).
    pub halt: bool,
    /// The CPU RDY line. `false` = a CPU read cycle must wait and repeat. A CPU write cycle
    /// continues (REQ-336).
    pub rdy: bool,
    /// The kind of DMA cycle. `DmaKind::None` for virtual DMA cycles (REQ-311).
    pub dma: DmaKind,
    /// The byte that ANTIC read. Valid only if `halt` and `dma != Refresh`.
    pub dma_data: u8,
    /// `true` on the one cycle where the NMI becomes visible to the CPU (REQ-334).
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

/// Internal ANTIC state for tests and the debugger (see [`Antic::debug_state`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnticDebugState {
    /// The instruction register.
    pub ir: u8,
    /// The display list address (`DLISTL`/`DLISTH`).
    pub dlist: u16,
    /// The memory scan counter.
    pub memscan: u16,
    /// The row counter, 0-15.
    pub row: u8,
    /// The playfield DMA clock ring. Bit 0 is the tap; bit `n` reaches the tap after `n`
    /// cycles. The mapping to the bit order of the HRM 4.12 tables is antic.md open issue 15.
    pub dma_clock: u8,
    /// The 48-byte line buffer.
    pub line_buffer: [u8; LINE_BUFFER_SIZE],
    /// The line buffer address.
    pub lb_index: u8,
    /// The refresh row counter.
    pub refresh_row: u8,
    /// `DMACTL` (bits 5-0).
    pub dmactl: u8,
    /// `CHACTL` (bits 2-0).
    pub chactl: u8,
    /// `HSCROL` (bits 3-0).
    pub hscrol: u8,
    /// `VSCROL` (bits 3-0).
    pub vscrol: u8,
    /// `PMBASE` (bits 7-2).
    pub pmbase: u8,
    /// `CHBASE` (bits 7-1).
    pub chbase: u8,
    /// `NMIEN` (bits 7-6).
    pub nmien: u8,
}

/// The size of the line buffer (HRM 4.11).
pub const LINE_BUFFER_SIZE: usize = 48;

/// The first scan line of the display list region (REQ-305).
const FIRST_DISPLAY_LINE: u32 = 8;
/// The last cycle with real playfield DMA. Later fetches are virtual (REQ-311, OI-018).
const LAST_REAL_DMA_CYCLE: u32 = 105;
/// The first visible horizontal position (HRM 6.2).
const FIRST_VISIBLE: usize = 0x22;
/// The last visible horizontal position (HRM 6.2).
const LAST_VISIBLE: usize = 0xDD;
/// The size of the per-line code array. The last fetch can place codes up to position 250.
const PIXELS: usize = 256;
/// `win_right` before the stop event of the scan line: the window is open on the right.
const OPEN: usize = usize::MAX;

/// `NMIEN`/`NMIST` bit 7: display list interrupt.
const NMI_DLI: u8 = 0x80;
/// `NMIEN`/`NMIST` bit 6: vertical blank interrupt.
const NMI_VBI: u8 = 0x40;

/// A playfield fetch that waits for the data bus value of the cycle (REQ-311, REQ-347).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Deferred {
    /// The cycle of the fetch.
    cycle: u32,
    /// A name or bitmap fetch that loads the line buffer at this index.
    name_or_bitmap: Option<u8>,
    /// A character data fetch for this name, with `false` if ANTIC substitutes `$00`.
    char_data: Option<(u8, bool)>,
}

/// The ANTIC chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the flip-flops of the hardware"
)]
pub struct Antic {
    /// The TV standard (it sets the number of scan lines).
    standard: VideoStandard,
    /// The scan line of the next `tick`.
    line: u32,
    /// The cycle of the next `tick`.
    cycle: u32,
    /// The scan line of the last `tick` (reads and writes happen after it).
    cur_line: u32,
    /// The cycle of the last `tick`.
    cur_cycle: u32,
    /// The absolute machine cycle of the next `tick`.
    total_cycles: u64,
    /// `DMACTL` as written (bits 5-0).
    dmactl: u8,
    /// `DMACTL` one cycle later: the value for the bits with two cycles of delay.
    dmactl_late: u8,
    /// `CHACTL` (bits 2-0).
    chactl: u8,
    /// The display list address.
    dlist: u16,
    /// `HSCROL` (bits 3-0).
    hscrol: u8,
    /// `VSCROL` (bits 3-0).
    vscrol: u8,
    /// `PMBASE` (bits 7-2).
    pmbase: u8,
    /// `CHBASE` as written (bits 7-1).
    chbase: u8,
    /// `CHBASE` one cycle later (REQ-317).
    chbase_late: u8,
    /// `NMIEN` (bits 7-6).
    nmien: u8,
    /// `NMIEN` as seen by tick 7 (writes up to cycle 6).
    nmien_at_7: u8,
    /// `NMIST` bits 7-5.
    nmist: u8,
    /// Light pen horizontal latch.
    penh: u8,
    /// Light pen vertical latch.
    penv: u8,
    /// The last light pen input level.
    light_pen_in: bool,
    /// The instruction register.
    ir: u8,
    /// Bit 5 of the last mode line (kept across vertical blank).
    prev_vs: bool,
    /// The row counter.
    row: u8,
    /// The current scan line is the first of its mode line.
    first_line: bool,
    /// The next scan line starts a mode line.
    new_mode_line: bool,
    /// A JVB executed: wait for vertical blank.
    jvb_wait: bool,
    /// An instruction was fetched on cycle 1 of this scan line.
    dl_fetched: bool,
    /// The low address byte of an LMS or jump, from cycle 6.
    addr_lo: u8,
    /// The memory scan counter.
    memscan: u16,
    /// The line buffer.
    line_buffer: [u8; LINE_BUFFER_SIZE],
    /// The line buffer address.
    lb_index: u8,
    /// The playfield DMA clock ring. Bit 0 is the tap.
    dma_clock: u8,
    /// The tap outputs: bit 0 = this cycle, bit `n` = `n` cycles before.
    tap_history: u8,
    /// The names of the last 4 cycles, indexed by `cycle & 3`.
    names: [u8; 4],
    /// The playfield started on this scan line.
    pf_started: bool,
    /// The fetch width of the start event (for a stop event while the width is 0).
    start_width: u8,
    /// The left edge of the display window.
    win_left: usize,
    /// The right edge of the display window, or `OPEN`.
    win_right: usize,
    /// The codes of this scan line, by horizontal position.
    pixels: [AnCode; PIXELS],
    /// A blocked refresh request waits.
    refresh_pending: bool,
    /// The refresh row counter.
    refresh_row: u8,
    /// The first absolute cycle with RDY low.
    wsync_start: u64,
    /// The first absolute cycle with RDY high again.
    wsync_end: u64,
    /// The result of the DLI check on cycle 6.
    dli_this_line: bool,
    /// The NMI source of this scan line (`NMI_DLI` or `NMI_VBI`), from tick 7.
    nmi_source: u8,
    /// The NMI request of tick 8.
    nmi_request: u8,
    /// The request comes one cycle late (`NMIEN` was set on cycle 7).
    nmi_late: bool,
    /// An IRQ sequence started on cycle 4 of this scan line (REQ-335).
    nmi_lost: bool,
    /// The IR mode was hi-res at scan line 248 (REQ-346).
    hires_bug: bool,
    /// A virtual or disabled playfield fetch waits for `end_cycle`.
    deferred: Option<Deferred>,
}

/// Increments the low 10 bits of a display list address (REQ-324).
const fn inc10(addr: u16) -> u16 {
    (addr & 0xFC00) | (addr.wrapping_add(1) & 0x03FF)
}

/// Increments the low 12 bits of the memory scan counter (REQ-325).
const fn inc12(addr: u16) -> u16 {
    (addr & 0xF000) | (addr.wrapping_add(1) & 0x0FFF)
}

/// The fetch width, one step wider for a horizontally scrolled line (REQ-327).
const fn fetch_width(width: u8, ir: u8) -> u8 {
    if ir & 0x10 != 0 && width < 3 {
        width + 1
    } else {
        width
    }
}

/// The cycle of the start event without `HSCROL` (1 narrow, 2 normal, 3 wide).
const fn start_cycle(width: u8) -> u32 {
    match width {
        1 => 25,
        2 => 17,
        _ => 9,
    }
}

/// The cycle of the stop event without `HSCROL`.
const fn stop_cycle(width: u8) -> u32 {
    match width {
        1 => 89,
        2 => 97,
        _ => 105,
    }
}

/// The display window for a `DMACTL` width (REQ-308). Wide is clipped at `$2C`.
const fn window(width: u8) -> (usize, usize) {
    match width {
        1 => (0x40, 0xBF),
        2 => (0x30, 0xCF),
        _ => (0x2C, 0xDD),
    }
}

/// One DMA read.
fn dma(mem: &mut impl DmaMemory, out: &mut CycleOutput, kind: DmaKind, addr: u16) -> u8 {
    let value = mem.dma_read(addr);
    out.dma = kind;
    out.dma_data = value;
    value
}

/// The low byte of a value.
const fn low_byte(value: u32) -> u8 {
    value.to_le_bytes()[0]
}

impl Antic {
    /// Creates ANTIC in the power-on state (REQ-303).
    #[must_use]
    pub const fn new(standard: VideoStandard) -> Self {
        Self {
            standard,
            line: 0,
            cycle: 0,
            cur_line: 0,
            cur_cycle: 0,
            total_cycles: 0,
            dmactl: 0,
            dmactl_late: 0,
            chactl: 0,
            dlist: 0,
            hscrol: 0,
            vscrol: 0,
            pmbase: 0,
            chbase: 0,
            chbase_late: 0,
            nmien: 0,
            nmien_at_7: 0,
            nmist: 0x60,
            penh: 0x00,
            penv: 0xFF,
            light_pen_in: false,
            ir: 0,
            prev_vs: false,
            row: 0,
            first_line: false,
            new_mode_line: false,
            jvb_wait: false,
            dl_fetched: false,
            addr_lo: 0,
            memscan: 0,
            line_buffer: [0; LINE_BUFFER_SIZE],
            lb_index: 0,
            dma_clock: 0,
            tap_history: 0,
            names: [0; 4],
            pf_started: false,
            start_width: 0,
            win_left: 0,
            win_right: OPEN,
            pixels: [AnCode::BACKGROUND; PIXELS],
            refresh_pending: false,
            refresh_row: 0,
            wsync_start: 0,
            wsync_end: 0,
            dli_this_line: false,
            nmi_source: 0,
            nmi_request: 0,
            nmi_late: false,
            nmi_lost: false,
            hires_bug: false,
            deferred: None,
        }
    }

    /// The RESET line: power-on and the RESET key (REQ-302). Clears `NMIEN`, `DMACTL` and
    /// the playfield DMA clock only.
    pub const fn reset(&mut self) {
        self.dmactl = 0;
        self.dmactl_late = 0;
        self.nmien = 0;
        self.nmi_request = 0;
        self.dma_clock = 0;
        self.tap_history = 0;
    }

    /// Reads a register. `reg` is `addr & 0x0F` (REQ-300, REQ-301). No side effects.
    #[must_use]
    pub const fn read(&self, reg: u8) -> u8 {
        match reg & 0x0F {
            0x0B => self.vcount(),
            0x0C => self.penh,
            0x0D => self.penv,
            0x0F => self.nmist | 0x1F,
            _ => 0xFF,
        }
    }

    /// Writes a register. `reg` is `addr & 0x0F`.
    pub const fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            0x00 => self.dmactl = value & 0x3F,
            0x01 => self.chactl = value & 0x07,
            0x02 => self.dlist = (self.dlist & 0xFF00) | value as u16,
            0x03 => self.dlist = (self.dlist & 0x00FF) | ((value as u16) << 8),
            0x04 => self.hscrol = value & 0x0F,
            0x05 => self.vscrol = value & 0x0F,
            0x07 => self.pmbase = value & 0xFC,
            0x09 => self.chbase = value & 0xFE,
            0x0A => self.wsync(),
            0x0E => self.nmien = value & 0xC0,
            0x0F => self.nmist = 0,
            _ => {}
        }
    }

    /// Runs one machine cycle (see `docs/design/antic.md`, "Call order").
    pub fn tick(&mut self, mem: &mut impl DmaMemory) -> CycleOutput {
        self.cur_line = self.line;
        self.cur_cycle = self.cycle;
        self.deferred = None;
        let mut out = CycleOutput {
            rdy: !(self.wsync_start <= self.total_cycles && self.total_cycles < self.wsync_end),
            line: self.line,
            cycle: self.cycle,
            ..CycleOutput::default()
        };
        if self.cycle == 0 {
            self.start_line(&mut out);
        }
        if self.in_display() {
            self.fixed_dma(mem, &mut out);
        }
        self.playfield(mem, &mut out);
        if self.in_display() {
            self.row_events();
        }
        self.refresh(&mut out);
        self.nmi(&mut out);
        out.halt = out.dma != DmaKind::None;
        let first = (2 * self.cycle as usize + 226) % 228;
        out.video = [self.code_at(first), self.code_at(first + 1)];
        self.advance();
        out
    }

    /// Ends the machine cycle. `data_bus` is the value on the data bus in this cycle.
    pub fn end_cycle(&mut self, data_bus: u8) {
        if let Some(deferred) = self.deferred.take() {
            if let Some(index) = deferred.name_or_bitmap {
                self.apply_name_or_bitmap(deferred.cycle, index, true, data_bus);
            }
            if let Some((name, shown)) = deferred.char_data {
                self.apply_char_data(deferred.cycle, name, shown, data_bus);
            }
        }
    }

    /// Sets the light pen input. `true` = asserted (REQ-342).
    pub const fn set_light_pen(&mut self, asserted: bool) {
        if asserted && !self.light_pen_in {
            self.penh = low_byte(2 * self.cur_cycle);
            self.penv = self.vcount();
        }
        self.light_pen_in = asserted;
    }

    /// The CPU started an IRQ or BRK interrupt sequence in this cycle (REQ-335).
    pub const fn irq_sequence_started(&mut self) {
        if self.cur_cycle == 4 {
            self.nmi_lost = true;
        }
    }

    /// The scan line of the next `tick`, `0..lines_per_frame`.
    #[must_use]
    pub const fn scanline(&self) -> u32 {
        self.line
    }

    /// The cycle of the next `tick`, `0..114`.
    #[must_use]
    pub const fn cycle(&self) -> u32 {
        self.cycle
    }

    /// Internal state for tests and the debugger.
    #[must_use]
    pub const fn debug_state(&self) -> AnticDebugState {
        AnticDebugState {
            ir: self.ir,
            dlist: self.dlist,
            memscan: self.memscan,
            row: self.row,
            dma_clock: self.dma_clock,
            line_buffer: self.line_buffer,
            lb_index: self.lb_index,
            refresh_row: self.refresh_row,
            dmactl: self.dmactl,
            chactl: self.chactl,
            hscrol: self.hscrol,
            vscrol: self.vscrol,
            pmbase: self.pmbase,
            chbase: self.chbase,
            nmien: self.nmien,
        }
    }

    /// `VCOUNT` for a read after the last tick (REQ-338).
    const fn vcount(&self) -> u8 {
        let last = self.standard.lines_per_frame() - 1;
        let count = if self.cur_cycle < 111 {
            self.cur_line >> 1
        } else if self.cur_line == last && self.cur_cycle >= 112 {
            0
        } else {
            (self.cur_line + 1) >> 1
        };
        low_byte(count)
    }

    /// A write to `WSYNC` after the last tick (REQ-336, REQ-337).
    const fn wsync(&mut self) {
        let write = self.total_cycles.saturating_sub(1);
        let cycle = self.cur_cycle as u64;
        let start = write + 2;
        let next_line = if cycle <= 103 { 0 } else { 114 };
        let end = write - cycle + 105 + next_line;
        if start >= end {
            return;
        }
        if self.total_cycles < self.wsync_end && start <= self.wsync_end {
            if start < self.wsync_start {
                self.wsync_start = start;
            }
            if end > self.wsync_end {
                self.wsync_end = end;
            }
        } else {
            self.wsync_start = start;
            self.wsync_end = end;
        }
    }

    /// The playfield width of `DMACTL` bits 1-0 (0 = playfield DMA off, REQ-308).
    const fn pf_width(&self) -> u8 {
        self.dmactl & 3
    }

    /// The scan line of the last tick is in the display list region (REQ-305).
    const fn in_display(&self) -> bool {
        self.cur_line >= FIRST_DISPLAY_LINE && self.cur_line < VBLANK_START_LINE
    }

    /// The scan line of the last tick is a vertical sync scan line (REQ-306).
    const fn in_vsync(&self) -> bool {
        match self.standard {
            VideoStandard::Ntsc => matches!(self.cur_line, 251..=253),
            VideoStandard::Pal => matches!(self.cur_line, 275..=277),
        }
    }

    /// The vertical scroll bit of the current mode line.
    const fn vs(&self) -> bool {
        mode(self.ir) >= 2 && self.ir & 0x20 != 0
    }

    /// The last row of the current mode line (REQ-328).
    const fn end_row(&self) -> u8 {
        if !self.vs() && self.prev_vs {
            self.vscrol
        } else {
            height(self.ir) - 1
        }
    }

    /// Tick 0: a new scan line.
    const fn start_line(&mut self, out: &mut CycleOutput) {
        self.pixels = [AnCode::BACKGROUND; PIXELS];
        self.pf_started = false;
        self.win_right = OPEN;
        self.tap_history = 0;
        self.dl_fetched = false;
        self.dli_this_line = false;
        self.nmi_lost = false;
        if self.cur_line == FIRST_DISPLAY_LINE {
            self.hires_bug = false;
        }
        if self.cur_line == VBLANK_START_LINE {
            // HRM 4.6: vertical blank truncates the mode line and stops a JVB wait.
            self.new_mode_line = true;
            self.dma_clock = 0;
            self.jvb_wait = false;
            self.hires_bug = is_hires(mode(self.ir));
            self.ir &= !0x40;
            out.frame_done = true;
        }
        self.first_line = self.in_display() && self.new_mode_line;
    }

    /// The DMA of cycles 0-7: missiles, display list and players (REQ-339, REQ-319).
    fn fixed_dma(&mut self, mem: &mut impl DmaMemory, out: &mut CycleOutput) {
        match self.cur_cycle {
            0 if self.dmactl_late & 0x0C != 0 => {
                let addr = self.pm_address(None);
                let _missiles = dma(mem, out, DmaKind::Missile, addr);
            }
            1 => self.fetch_instruction(mem, out),
            c @ 2..=5 if self.dmactl_late & 0x08 != 0 => {
                let player = low_byte(c - 2);
                let addr = self.pm_address(Some(player));
                let _player = dma(mem, out, DmaKind::Player(player), addr);
            }
            6 | 7 => self.fetch_address(mem, out),
            _ => {}
        }
    }

    /// The P/M DMA address for a missile (`None`) or a player (REQ-340).
    fn pm_address(&self, player: Option<u8>) -> u16 {
        let y = u16::try_from(self.cur_line).unwrap_or(0);
        let one_line = self.dmactl & 0x10 != 0;
        let base = if one_line {
            u16::from(self.pmbase & 0xF8) << 8
        } else {
            u16::from(self.pmbase & 0xFC) << 8
        };
        match (player, one_line) {
            (None, false) => base + 0x180 + (y >> 1),
            (None, true) => base + 0x300 + y,
            (Some(n), false) => base + 0x200 + u16::from(n) * 0x80 + (y >> 1),
            (Some(n), true) => base + 0x400 + u16::from(n) * 0x100 + y,
        }
    }

    /// Tick 1: the instruction fetch of a new mode line (REQ-319, REQ-322, REQ-326).
    fn fetch_instruction(&mut self, mem: &mut impl DmaMemory, out: &mut CycleOutput) {
        if self.new_mode_line {
            self.new_mode_line = false;
            if !self.jvb_wait && self.dmactl_late & 0x20 != 0 {
                self.ir = dma(mem, out, DmaKind::DisplayList, self.dlist);
                self.dlist = inc10(self.dlist);
                self.dl_fetched = true;
            }
            self.row = if self.vs() && !self.prev_vs {
                self.vscrol
            } else {
                0
            };
        }
        if mode(self.ir) < 2 {
            // HRM 4.12: blank and jump lines reset the playfield DMA clock.
            self.dma_clock = 0;
        }
    }

    /// Ticks 6 and 7: the LMS or jump address (REQ-321, REQ-322, REQ-323).
    fn fetch_address(&mut self, mem: &mut impl DmaMemory, out: &mut CycleOutput) {
        let mode = mode(self.ir);
        if !self.dl_fetched || !(mode == 1 || (mode >= 2 && self.ir & 0x40 != 0)) {
            return;
        }
        let value = dma(mem, out, DmaKind::DisplayListAddress, self.dlist);
        self.dlist = inc10(self.dlist);
        if self.cur_cycle == 6 {
            self.addr_lo = value;
            return;
        }
        let addr = u16::from_le_bytes([self.addr_lo, value]);
        if mode == 1 {
            self.dlist = addr;
            self.jvb_wait = self.ir & 0x40 != 0;
        } else {
            self.memscan = addr;
        }
    }

    /// The DLI check (tick 6) and the end check (tick 109) (REQ-328, REQ-329, REQ-332).
    const fn row_events(&mut self) {
        match self.cur_cycle {
            6 => self.dli_this_line = self.ir & 0x80 != 0 && self.row == self.end_row(),
            109 => {
                if self.row == self.end_row() {
                    self.new_mode_line = true;
                    self.prev_vs = self.vs();
                } else {
                    self.row = (self.row + 1) & 0x0F;
                }
            }
            _ => {}
        }
    }

    /// The playfield DMA clock and its requests (REQ-309, REQ-311, REQ-312).
    fn playfield(&mut self, mem: &mut impl DmaMemory, out: &mut CycleOutput) {
        let mode = mode(self.ir);
        let tap = self.dma_clock & 1;
        self.tap_history = (self.tap_history << 1) | tap;
        self.dma_clock = (self.dma_clock >> 1) | (tap << (period(mode) - 1));
        if !self.in_display() || mode < 2 {
            return;
        }
        self.clock_events();
        let (name_or_bitmap, char_data) = if is_char(mode) {
            (self.tap_history & 0x01 != 0, self.tap_history & 0x08 != 0)
        } else {
            (self.tap_history & 0x04 != 0, false)
        };
        if name_or_bitmap || char_data {
            self.requests(mem, out, name_or_bitmap, char_data);
        }
    }

    /// The `HSCROL` delay of the start and stop events in cycles.
    const fn hscrol_cycles(&self) -> u32 {
        if self.ir & 0x10 == 0 {
            0
        } else {
            (self.hscrol >> 1) as u32
        }
    }

    /// The start and stop events of the playfield DMA clock (HRM 4.12).
    const fn clock_events(&mut self) {
        let width = self.dmactl & 3;
        let delay = self.hscrol_cycles();
        if width != 0 && self.cur_cycle == start_cycle(fetch_width(width, self.ir)) + delay {
            self.dma_clock |= 1;
            if !self.pf_started {
                self.pf_started = true;
                self.lb_index = 0;
                self.win_left = window(width).0;
                self.start_width = width;
            }
        }
        let stop_width = if width == 0 { self.start_width } else { width };
        if stop_width != 0 && self.cur_cycle == stop_cycle(fetch_width(stop_width, self.ir)) + delay
        {
            self.dma_clock &= !1;
            self.win_right = window(stop_width).1;
        }
    }

    /// The name, bitmap and character data requests of one cycle.
    fn requests(
        &mut self,
        mem: &mut impl DmaMemory,
        out: &mut CycleOutput,
        name_or_bitmap: bool,
        char_data: bool,
    ) {
        let cycle = self.cur_cycle;
        let mode = mode(self.ir);
        let from_memory = name_or_bitmap && self.first_line;
        let name = self.names[((cycle + 1) & 3) as usize];
        let frow = font_row(mode, name, self.row, self.chactl);
        let font = font_address(mode, name, frow.unwrap_or(0), self.chbase_late);
        let real = self.pf_width() != 0 && cycle <= LAST_REAL_DMA_CYCLE && out.dma == DmaKind::None;
        let value = (real && (from_memory || char_data)).then(|| {
            let (kind, addr) = match (from_memory, char_data) {
                // REQ-345: two requests in one cycle read from the AND of the addresses.
                (true, true) => (DmaKind::CharacterName, self.memscan & font),
                (true, false) if is_char(mode) => (DmaKind::CharacterName, self.memscan),
                (true, false) => (DmaKind::Bitmap, self.memscan),
                (false, _) => (DmaKind::CharacterData, font),
            };
            dma(mem, out, kind, addr)
        });
        let mut deferred = Deferred {
            cycle,
            name_or_bitmap: None,
            char_data: None,
        };
        if name_or_bitmap {
            let index = self.lb_index;
            self.lb_index = (index + 1) & 0x3F;
            if from_memory {
                self.memscan = inc12(self.memscan);
                match value {
                    Some(byte) => self.apply_name_or_bitmap(cycle, index, true, byte),
                    None => deferred.name_or_bitmap = Some(index),
                }
            } else {
                let byte = self
                    .line_buffer
                    .get(usize::from(index))
                    .copied()
                    .unwrap_or(0xFF);
                self.apply_name_or_bitmap(cycle, index, false, byte);
            }
        }
        if char_data {
            match value {
                Some(byte) => self.apply_char_data(cycle, name, frow.is_some(), byte),
                None => deferred.char_data = Some((name, frow.is_some())),
            }
        }
        if deferred.name_or_bitmap.is_some() || deferred.char_data.is_some() {
            self.deferred = Some(deferred);
        }
    }

    /// Stores a name or bitmap byte and decodes a bitmap byte.
    fn apply_name_or_bitmap(&mut self, cycle: u32, index: u8, store: bool, byte: u8) {
        if store && let Some(slot) = self.line_buffer.get_mut(usize::from(index)) {
            *slot = byte;
        }
        let mode = mode(self.ir);
        if is_char(mode) {
            self.names[(cycle & 3) as usize] = byte;
        } else {
            let codes = mapped_codes(mode, byte);
            self.place(2 * cycle as usize + 8, codes.as_slice());
        }
    }

    /// Decodes a character data byte. `shown` is `false` for the `$00` rows of mode 3.
    fn apply_char_data(&mut self, cycle: u32, name: u8, shown: bool, byte: u8) {
        let mode = mode(self.ir);
        let data = blink_invert(mode, name, if shown { byte } else { 0 }, self.chactl);
        let codes = char_codes(mode, name, data);
        self.place(2 * cycle as usize + 6, codes.as_slice());
    }

    /// Puts the codes of one byte at `position` (plus the odd `HSCROL` delay, HRM 4.7).
    fn place(&mut self, position: usize, codes: &[AnCode]) {
        let odd = if self.ir & 0x10 == 0 {
            0
        } else {
            usize::from(self.hscrol & 1)
        };
        let start = position + odd;
        for (slot, code) in self.pixels.iter_mut().skip(start).zip(codes) {
            *slot = *code;
        }
    }

    /// Memory refresh (REQ-341).
    const fn refresh(&mut self, out: &mut CycleOutput) {
        let cycle = self.cur_cycle;
        let request = cycle >= 25 && cycle <= 57 && (cycle - 25).is_multiple_of(4);
        if matches!(out.dma, DmaKind::None) {
            if request || self.refresh_pending {
                out.dma = DmaKind::Refresh;
                self.refresh_row = self.refresh_row.wrapping_add(1);
                self.refresh_pending = request && self.refresh_pending;
            }
        } else if request {
            // A new request while one waits is dropped (project decision, REQ-341).
            self.refresh_pending = true;
        }
    }

    /// `NMIST` and the NMI edge (REQ-331 to REQ-335).
    const fn nmi(&mut self, out: &mut CycleOutput) {
        match self.cur_cycle {
            7 => {
                self.nmi_source = if self.cur_line == VBLANK_START_LINE {
                    NMI_VBI
                } else if self.dli_this_line {
                    NMI_DLI
                } else {
                    0
                };
                if self.nmi_source != 0 {
                    self.nmist = (self.nmist & 0x20) | self.nmi_source;
                }
                self.nmien_at_7 = self.nmien;
            }
            8 => {
                self.nmi_request = self.nmi_source & self.nmien;
                self.nmi_late = self.nmi_request & !self.nmien_at_7 != 0;
            }
            9 | 10 => {
                let late_cycle = self.cur_cycle == 10;
                if self.nmi_request != 0 && self.nmi_late == late_cycle {
                    out.nmi = self.nmi_request & self.nmien != 0 && !self.nmi_lost;
                    self.nmi_request = 0;
                }
            }
            _ => {}
        }
    }

    /// The code for horizontal position `x` of the last tick (REQ-306 to REQ-308).
    fn code_at(&self, x: usize) -> AnCode {
        let visible = (FIRST_VISIBLE..=LAST_VISIBLE).contains(&x);
        let bug = self.hires_bug && visible && self.pf_width() != 0;
        let hblank = if is_hires(mode(self.ir)) {
            AnCode::HBLANK_HIRES
        } else {
            AnCode::HBLANK_LORES
        };
        if self.in_vsync() {
            return if bug {
                AnCode::VSYNC.with_an2()
            } else {
                AnCode::VSYNC
            };
        }
        if !self.in_display() {
            return if bug { hblank.with_an2() } else { hblank };
        }
        if !visible {
            return hblank;
        }
        let in_window = self.pf_started && x >= self.win_left && x <= self.win_right;
        if self.pf_width() == 0 || !in_window {
            return AnCode::BACKGROUND;
        }
        self.pixels.get(x).copied().unwrap_or_default()
    }

    /// Advances the counters to the next cycle.
    const fn advance(&mut self) {
        self.dmactl_late = self.dmactl;
        self.chbase_late = self.chbase;
        self.total_cycles += 1;
        self.cycle += 1;
        if self.cycle == CYCLES_PER_LINE {
            self.cycle = 0;
            self.line += 1;
            if self.line == self.standard.lines_per_frame() {
                self.line = 0;
            }
        }
    }
}
