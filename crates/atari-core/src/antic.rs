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
    /// `true` in the tick after the cycle in which the NMI level rises (REQ-334). For tests
    /// and tracing: the machine passes `Antic::nmi_level` to the CPU (OI-017).
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
    /// The playfield DMA clock: bit `k` is stage `k` of the 8-stage shift register (see
    /// `docs/design/antic.md`, "Playfield DMA clock"). The mapping to the bit order of the
    /// HRM 4.12 tables is antic.md open issue 15.
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
/// Abnormal playfield DMA is virtual on this cycle too (HRM 4.12).
const LAST_REAL_DMA_CYCLE: u32 = 105;
/// The number of states of the line buffer address counter (HRM 4.12, REQ-345).
const LB_STATES: u8 = 63;
/// The DMA clock stage of the character name request (HRM 4.12: offset 0).
const STAGE_NAME: u8 = 1 << 0;
/// The DMA clock stage of the bitmap request (HRM 4.12: offset +2).
const STAGE_BITMAP: u8 = 1 << 2;
/// The DMA clock stage of the character data request (HRM 4.12: offset +3).
const STAGE_CHAR_DATA: u8 = 1 << 3;
/// The first visible horizontal position (HRM 6.2).
const FIRST_VISIBLE: usize = 0x22;
/// The last visible horizontal position (HRM 6.2).
const LAST_VISIBLE: usize = 0xDD;
/// The size of the per-line code array. The last fetch can place codes up to position 250.
const PIXELS: usize = 256;
/// `win_right` before the right edge event of the scan line: the window is open on the right.
const OPEN: usize = usize::MAX;

/// The value of `PENH` (REQ-303). The emulator has no light pen (OI-016).
const PENH: u8 = 0x00;
/// The value of `PENV` (REQ-303).
const PENV: u8 = 0xFF;

/// `NMIEN`/`NMIST` bit 7: display list interrupt.
const NMI_DLI: u8 = 0x80;
/// `NMIEN`/`NMIST` bit 6: vertical blank interrupt.
const NMI_VBI: u8 = 0x40;

/// A playfield fetch that waits for the data bus value of the cycle (REQ-311, REQ-347).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Deferred {
    /// The cycle of the fetch.
    cycle: u32,
    /// The IR mode of the fetch.
    mode: u8,
    /// The slot in `names` of a name fetch.
    slot: usize,
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
    /// `NMIEN` as written by the previous cycles: by cycle 6 after tick 7, by cycle 7 after
    /// tick 8 (`en_by(6)` and `en_by(7)`, OI-017).
    nmien_prev: u8,
    /// `NMIST` bits 7-5.
    nmist: u8,
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
    /// The playfield DMA clock: an 8-stage shift register, bit `k` = stage `k`.
    dma_clock: u8,
    /// The bits of `dma_clock` that the start event of this scan line injected. A request
    /// from another bit is abnormal DMA (HRM 4.12).
    dma_clock_normal: u8,
    /// IR as the playfield DMA clock sees it: the value of the last tick. An instruction
    /// fetch on cycle 1 changes the clock from cycle 2 (REQ-345).
    pf_ir: u8,
    /// The names of the last 4 cycles, indexed by the absolute cycle `& 3`.
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
    /// The NMI sources that the earlier `NMIEN` value enables for this cycle: `R` and
    /// `en_by(6)` on cycle 8, `R` and `en_by(7)` on cycle 9, else 0 (OI-017).
    nmi_armed: u8,
    /// The NMI level of the cycle before the last tick, for `out.nmi`.
    nmi_was_high: bool,
    /// The IR mode was hi-res at scan line 248 (REQ-346).
    hires_bug: bool,
    /// A virtual or disabled playfield fetch waits for `end_cycle`.
    deferred: Option<Deferred>,
    /// The addresses of the DMA requests that the last tick combined into one read.
    #[cfg(test)]
    requests: [Option<u16>; 3],
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

/// The address of two DMA requests: the bitwise AND of the addresses that are present
/// (REQ-345).
const fn and_requests(left: Option<u16>, right: Option<u16>) -> Option<u16> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left & right),
        (Some(addr), None) | (None, Some(addr)) => Some(addr),
        (None, None) => None,
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

/// The playfield DMA requests of one cycle (HRM 4.12).
#[derive(Debug, Clone, Copy, Default)]
#[expect(clippy::struct_excessive_bools, reason = "one flag per request line")]
struct PfRequest {
    /// The IR mode that the DMA clock uses.
    mode: u8,
    /// A character name or bitmap request.
    name_or_bitmap: bool,
    /// A character data request.
    char_data: bool,
    /// The name or bitmap request reads memory (first scan line of the mode line).
    from_memory: bool,
    /// The requests make a real DMA cycle.
    real: bool,
    /// The name of the character data request (from 3 cycles before).
    name: u8,
    /// `false` if ANTIC substitutes `$00` for the character data (REQ-313).
    shown: bool,
    /// The font address of the character data request.
    font: u16,
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
            nmien_prev: 0,
            nmist: 0x60,
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
            dma_clock_normal: 0,
            pf_ir: 0,
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
            nmi_armed: 0,
            nmi_was_high: false,
            hires_bug: false,
            deferred: None,
            #[cfg(test)]
            requests: [None; 3],
        }
    }

    /// The RESET line: power-on and the RESET key (REQ-302). Clears `NMIEN`, `DMACTL` and
    /// the playfield DMA clock only.
    pub const fn reset(&mut self) {
        self.dmactl = 0;
        self.dmactl_late = 0;
        self.nmien = 0;
        self.dma_clock = 0;
        self.dma_clock_normal = 0;
    }

    /// Reads a register. `reg` is `addr & 0x0F` (REQ-300, REQ-301). No side effects.
    #[must_use]
    pub const fn read(&self, reg: u8) -> u8 {
        match reg & 0x0F {
            0x0B => self.vcount(),
            // The light pen is a non-goal (OI-016): `PENH` and `PENV` keep their
            // power-on values (REQ-301, REQ-303).
            0x0C => PENH,
            0x0D => PENV,
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
        let special = if self.in_display() {
            self.special_request()
        } else {
            None
        };
        let pf = self.playfield_request();
        self.dma_cycle(mem, &mut out, special, &pf);
        self.clock_step();
        if self.in_display() {
            self.row_events();
        }
        self.refresh(&mut out);
        self.nmi(&mut out);
        out.halt = out.dma != DmaKind::None;
        let first = (2 * self.cycle as usize + 226) % 228;
        out.video = self.codes_at(first);
        self.advance();
        out
    }

    /// Ends the machine cycle. `data_bus` is the value on the data bus in this cycle.
    pub fn end_cycle(&mut self, data_bus: u8) {
        if let Some(deferred) = self.deferred.take() {
            if let Some(index) = deferred.name_or_bitmap {
                let fetch = (deferred.cycle, deferred.mode, deferred.slot);
                self.apply_name_or_bitmap(fetch, Some(index), data_bus);
            }
            if let Some((name, shown)) = deferred.char_data {
                self.apply_char_data(deferred.cycle, deferred.mode, (name, shown), data_bus);
            }
        }
    }

    /// The NMI level of the cycle of the last tick, after the CPU access of the cycle
    /// (OI-017): `nmi(8) = R && en_by(6) && en_now`, `nmi(9) = R && en_by(7) && en_now`, and
    /// false on all other cycles. `en_now` includes an `NMIEN` write in this cycle (REQ-334).
    #[must_use]
    pub const fn nmi_level(&self) -> bool {
        self.nmi_armed & self.nmien != 0
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
        self.dma_clock_normal = 0;
        self.dl_fetched = false;
        self.dli_this_line = false;
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

    /// The DMA request of cycles 0-7: missiles, display list and players (REQ-319,
    /// REQ-339).
    fn special_request(&self) -> Option<(DmaKind, u16)> {
        match self.cur_cycle {
            0 if self.dmactl_late & 0x0C != 0 => Some((DmaKind::Missile, self.pm_address(None))),
            1 if self.new_mode_line && !self.jvb_wait && self.dmactl_late & 0x20 != 0 => {
                Some((DmaKind::DisplayList, self.dlist))
            }
            c @ 2..=5 if self.dmactl_late & 0x08 != 0 => {
                let player = low_byte(c - 2);
                Some((DmaKind::Player(player), self.pm_address(Some(player))))
            }
            6 | 7 if self.address_fetch() => Some((DmaKind::DisplayListAddress, self.dlist)),
            _ => None,
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

    /// The DMA cycle: one read for all requests of the cycle. Two or more requests read
    /// from the bitwise AND of their addresses, and all get the byte (REQ-345).
    fn dma_cycle(
        &mut self,
        mem: &mut impl DmaMemory,
        out: &mut CycleOutput,
        special: Option<(DmaKind, u16)>,
        pf: &PfRequest,
    ) {
        let name_addr = (pf.real && pf.from_memory).then_some(self.memscan);
        let font_addr = (pf.real && pf.char_data).then_some(pf.font);
        let requests = [special.map(|(_, addr)| addr), name_addr, font_addr];
        #[cfg(test)]
        {
            self.requests = requests;
        };
        let [special_addr, ..] = requests;
        let addr = and_requests(and_requests(special_addr, name_addr), font_addr);
        let value = addr.map(|addr| mem.dma_read(addr));
        if let Some(byte) = value {
            out.dma = match (special, name_addr.is_some()) {
                (Some((kind, _)), _) => kind,
                (None, true) if is_char(pf.mode) => DmaKind::CharacterName,
                (None, true) => DmaKind::Bitmap,
                (None, false) => DmaKind::CharacterData,
            };
            out.dma_data = byte;
        }
        match self.cur_cycle {
            1 if self.in_display() => self.apply_instruction(special.and(value)),
            6 | 7 => {
                if let (Some(_), Some(byte)) = (special, value) {
                    self.apply_address(byte);
                }
            }
            _ => {}
        }
        self.apply_playfield(pf, if pf.real { value } else { None });
    }

    /// Tick 1: the instruction of a new mode line, if `fetched` (REQ-319, REQ-322, REQ-326).
    const fn apply_instruction(&mut self, fetched: Option<u8>) {
        if self.new_mode_line {
            self.new_mode_line = false;
            if let Some(ir) = fetched {
                self.ir = ir;
                self.dlist = inc10(self.dlist);
                self.dl_fetched = true;
            }
            self.row = if self.vs() && !self.prev_vs {
                self.vscrol
            } else {
                0
            };
        }
    }

    /// Ticks 6 and 7 fetch an LMS or jump address (REQ-321, REQ-322, REQ-323, REQ-330).
    const fn address_fetch(&self) -> bool {
        let mode = mode(self.ir);
        let lms = mode >= 2 && self.ir & 0x40 != 0;
        // REQ-330: a jump line that vertical scrolling extends fetches its address again
        // on each of its later scan lines.
        let refetch = mode == 1 && !self.first_line && self.dmactl_late & 0x20 != 0;
        (self.dl_fetched && (mode == 1 || lms)) || refetch
    }

    /// Ticks 6 and 7: one byte of the LMS or jump address.
    const fn apply_address(&mut self, value: u8) {
        self.dlist = inc10(self.dlist);
        if self.cur_cycle == 6 {
            self.addr_lo = value;
            return;
        }
        let addr = u16::from_le_bytes([self.addr_lo, value]);
        if mode(self.ir) == 1 {
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

    /// The slot in `names` of the tick `cycles_before` cycles before this tick.
    const fn name_slot(&self, cycles_before: u64) -> usize {
        (self.total_cycles.wrapping_sub(cycles_before).to_le_bytes()[0] & 3) as usize
    }

    /// The playfield requests of the DMA clock in this tick (REQ-309, REQ-311, REQ-345).
    fn playfield_request(&self) -> PfRequest {
        let mode = mode(self.pf_ir);
        let mut req = PfRequest {
            mode,
            ..PfRequest::default()
        };
        if !self.in_display() || mode < 2 {
            return req;
        }
        let stages = if is_char(mode) {
            STAGE_NAME | STAGE_CHAR_DATA
        } else {
            STAGE_BITMAP
        };
        let active = self.dma_clock & stages;
        if active == 0 {
            return req;
        }
        req.name_or_bitmap = active & (STAGE_NAME | STAGE_BITMAP) != 0;
        req.char_data = active & STAGE_CHAR_DATA != 0;
        req.from_memory = req.name_or_bitmap && self.first_line;
        // HRM 4.12 and OI-018: normal playfield DMA is real up to cycle 105, abnormal
        // playfield DMA only on cycles 1-104.
        let cycle = self.cur_cycle;
        let normal = active & !self.dma_clock_normal == 0;
        let in_window =
            (1..LAST_REAL_DMA_CYCLE).contains(&cycle) || (cycle == LAST_REAL_DMA_CYCLE && normal);
        req.real = self.pf_width() != 0 && in_window && (req.from_memory || req.char_data);
        req.name = self.names[self.name_slot(3)];
        let frow = font_row(mode, req.name, self.row, self.chactl);
        req.shown = frow.is_some();
        req.font = font_address(mode, req.name, frow.unwrap_or(0), self.chbase_late);
        req
    }

    /// One cycle of the playfield DMA clock (HRM 4.12): the shift, then the start and stop
    /// events. Each stage moves to the next stage. Stage `P - 1` also enters stage 0 again,
    /// where `P` is the period of the mode. A bit leaves after stage 7.
    const fn clock_step(&mut self) {
        let period = period(mode(self.pf_ir));
        self.dma_clock = (self.dma_clock << 1) | ((self.dma_clock >> (period - 1)) & 1);
        self.dma_clock_normal =
            (self.dma_clock_normal << 1) | ((self.dma_clock_normal >> (period - 1)) & 1);
        // HRM 4.12: blank and jump lines and vertical blank clear the DMA clock.
        if !self.in_display() || mode(self.ir) < 2 {
            self.dma_clock = 0;
            self.dma_clock_normal = 0;
            return;
        }
        self.clock_events();
    }

    /// The `HSCROL` delay of the start and stop events in cycles.
    const fn hscrol_cycles(&self) -> u32 {
        if self.ir & 0x10 == 0 {
            0
        } else {
            (self.hscrol >> 1) as u32
        }
    }

    /// The start and stop events of the playfield DMA clock (HRM 4.11, HRM 4.12).
    const fn clock_events(&mut self) {
        let width = self.dmactl & 3;
        let delay = self.hscrol_cycles();
        if width != 0 && self.cur_cycle == start_cycle(fetch_width(width, self.ir)) + delay {
            self.dma_clock |= 1;
            self.dma_clock_normal |= 1;
            if !self.pf_started {
                self.pf_started = true;
                self.lb_index = 0;
                self.win_left = window(width).0;
                self.start_width = width;
            }
        }
        let stop_width = if width == 0 { self.start_width } else { width };
        // HRM 4.11: the wide stop position is always active.
        let wide_stop = self.cur_cycle == stop_cycle(3) + delay;
        if wide_stop
            || (stop_width != 0
                && self.cur_cycle == stop_cycle(fetch_width(stop_width, self.ir)) + delay)
        {
            self.dma_clock &= !1;
            self.dma_clock_normal &= !1;
        }
        // The right edge of the display window comes from the displayed width without
        // `HSCROL`. On a line without scrolling, this is the cycle of the stop event. On a
        // scrolled line, the stop event is later than the right edge (REQ-327).
        if stop_width != 0 && self.cur_cycle == stop_cycle(stop_width) {
            self.win_right = window(stop_width).1;
        }
    }

    /// The effects of the playfield requests. `value` is the byte of a real DMA cycle, else
    /// the requests wait for the data bus value (REQ-311, REQ-344, REQ-347).
    fn apply_playfield(&mut self, pf: &PfRequest, value: Option<u8>) {
        let cycle = self.cur_cycle;
        let fetch = (cycle, pf.mode, self.name_slot(0));
        let mut deferred = Deferred {
            cycle,
            mode: pf.mode,
            slot: fetch.2,
            name_or_bitmap: None,
            char_data: None,
        };
        if pf.name_or_bitmap {
            let index = self.lb_index;
            // HRM 4.12: the line buffer address counter has 63 states.
            self.lb_index = if index + 1 >= LB_STATES { 0 } else { index + 1 };
            if pf.from_memory {
                self.memscan = inc12(self.memscan);
                match value {
                    Some(byte) => self.apply_name_or_bitmap(fetch, Some(index), byte),
                    None => deferred.name_or_bitmap = Some(index),
                }
            } else {
                // States 48-62 of the counter read as `$FF`.
                let byte = self
                    .line_buffer
                    .get(usize::from(index))
                    .copied()
                    .unwrap_or(0xFF);
                self.apply_name_or_bitmap(fetch, None, byte);
            }
        }
        if pf.char_data {
            match value {
                Some(byte) => self.apply_char_data(cycle, pf.mode, (pf.name, pf.shown), byte),
                None => deferred.char_data = Some((pf.name, pf.shown)),
            }
        }
        if deferred.name_or_bitmap.is_some() || deferred.char_data.is_some() {
            self.deferred = Some(deferred);
        }
    }

    /// Uses the name or bitmap byte of the fetch `(cycle, mode, slot)`. `store` is the line
    /// buffer index for a byte from memory or from the data bus.
    fn apply_name_or_bitmap(
        &mut self,
        (cycle, mode, slot): (u32, u8, usize),
        store: Option<u8>,
        byte: u8,
    ) {
        if let Some(index) = store
            && let Some(entry) = self.line_buffer.get_mut(usize::from(index))
        {
            *entry = byte;
        }
        if is_char(mode) {
            if let Some(name) = self.names.get_mut(slot) {
                *name = byte;
            }
        } else {
            let codes = mapped_codes(mode, byte);
            self.place(2 * cycle as usize + 8, codes.as_slice());
        }
    }

    /// Decodes a character data byte for `(name, shown)`. `shown` is `false` for the `$00`
    /// rows of mode 3.
    fn apply_char_data(&mut self, cycle: u32, mode: u8, (name, shown): (u8, bool), byte: u8) {
        let data = blink_invert(mode, name, if shown { byte } else { 0 }, self.chactl);
        let codes = char_codes(mode, name, data);
        self.place(2 * cycle as usize + 6, codes.as_slice());
    }

    /// Puts the codes of one byte at `position`. The odd `HSCROL` delay is in the output
    /// (`codes_at`, HRM 4.7).
    fn place(&mut self, position: usize, codes: &[AnCode]) {
        for (slot, code) in self.pixels.iter_mut().skip(position).zip(codes) {
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

    /// `NMIST` and the NMI level (REQ-331 to REQ-334, OI-017). `out.nmi` is the rising edge
    /// of the level of the cycle before this tick, for tests and tracing.
    const fn nmi(&mut self, out: &mut CycleOutput) {
        let high = self.nmi_level();
        out.nmi = high && !self.nmi_was_high;
        self.nmi_was_high = high;
        self.nmi_armed = 0;
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
                self.nmien_prev = self.nmien;
            }
            8 => {
                self.nmi_armed = self.nmi_source & self.nmien_prev;
                self.nmien_prev = self.nmien;
            }
            9 => self.nmi_armed = self.nmi_source & self.nmien_prev,
            _ => {}
        }
    }

    /// The codes for the horizontal positions `first` and `first + 1` of the last tick
    /// (REQ-306 to REQ-308). The checks that do not depend on the position are made once
    /// for the two positions.
    fn codes_at(&self, first: usize) -> [AnCode; 2] {
        let positions = [first, first + 1];
        let width = self.pf_width();
        // REQ-346, OI-015: the bogus codes cover the playfield window of the `DMACTL` width.
        let (left, right) = window(width);
        let bug = |x: usize| self.hires_bug && width != 0 && (left..=right).contains(&x);
        let hblank = if is_hires(mode(self.ir)) {
            AnCode::HBLANK_HIRES
        } else {
            AnCode::HBLANK_LORES
        };
        if self.in_vsync() {
            return positions.map(|x| {
                if bug(x) {
                    AnCode::VSYNC.with_an2()
                } else {
                    AnCode::VSYNC
                }
            });
        }
        if !self.in_display() {
            return positions.map(|x| if bug(x) { hblank.with_an2() } else { hblank });
        }
        // HRM 4.7: odd `HSCROL` values delay the playfield encoder by one color clock.
        let odd = if self.ir & 0x10 == 0 {
            0
        } else {
            usize::from(self.hscrol & 1)
        };
        positions.map(|x| {
            if !(FIRST_VISIBLE..=LAST_VISIBLE).contains(&x) {
                return hblank;
            }
            let in_window = self.pf_started && x >= self.win_left && x <= self.win_right;
            if width == 0 || !in_window {
                return AnCode::BACKGROUND;
            }
            self.pixels.get(x - odd).copied().unwrap_or_default()
        })
    }

    /// Advances the counters to the next cycle.
    const fn advance(&mut self) {
        self.dmactl_late = self.dmactl;
        self.chbase_late = self.chbase;
        self.pf_ir = self.ir;
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
