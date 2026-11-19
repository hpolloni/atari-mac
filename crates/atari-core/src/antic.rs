//! ANTIC: the display list processor.
//!
//! Design: `docs/design/antic.md`. Requirements: REQ-300 to REQ-347. The interface with
//! GTIA is decided in OI-001: the machine forwards [`CycleOutput`] to `Gtia::clock`.

#![expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    reason = "WP-0 stubs: delete this when the work package replaces the stubs"
)]

use crate::timing::{CYCLES_PER_LINE, VBLANK_START_LINE, VideoStandard};

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
    /// The kind of DMA cycle (REQ-311).
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

/// The ANTIC chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Antic {
    /// The TV standard (it sets the number of scan lines).
    standard: VideoStandard,
    /// The scan line of the next `tick`.
    line: u32,
    /// The cycle of the next `tick`.
    cycle: u32,
}

impl Antic {
    /// Creates ANTIC in the power-on state (REQ-303).
    #[must_use]
    pub const fn new(standard: VideoStandard) -> Self {
        Self {
            standard,
            line: 0,
            cycle: 0,
        }
    }

    /// The RESET line: power-on and the RESET key (REQ-302). Clears `NMIEN`, `DMACTL` and
    /// the playfield DMA clock only.
    pub const fn reset(&mut self) {
        // WP-0 STUB: WP-2 implements the registers.
    }

    /// Reads a register. `reg` is `addr & 0x0F` (REQ-300, REQ-301). No side effects.
    #[must_use]
    pub const fn read(&self, reg: u8) -> u8 {
        // WP-0 STUB: only VCOUNT.
        match reg & 0x0F {
            0x0B => {
                let [half, ..] = (self.line / 2).to_le_bytes();
                half
            }
            _ => 0xFF,
        }
    }

    /// Writes a register. `reg` is `addr & 0x0F`.
    pub const fn write(&mut self, _reg: u8, _value: u8) {
        // WP-0 STUB.
    }

    /// Runs one machine cycle (see `docs/design/antic.md`, "Call order").
    pub fn tick(&mut self, mem: &mut impl DmaMemory) -> CycleOutput {
        // WP-0 STUB: counts cycles and lines, and makes no DMA.
        let _: &mut _ = mem;
        let out = CycleOutput {
            rdy: true,
            frame_done: self.cycle == 0 && self.line == VBLANK_START_LINE,
            line: self.line,
            cycle: self.cycle,
            video: [AnCode::HBLANK_LORES; 2],
            ..CycleOutput::default()
        };
        self.cycle += 1;
        if self.cycle == CYCLES_PER_LINE {
            self.cycle = 0;
            self.line += 1;
            if self.line == self.standard.lines_per_frame() {
                self.line = 0;
            }
        }
        out
    }

    /// Ends the machine cycle. `data_bus` is the value on the data bus in this cycle.
    pub const fn end_cycle(&mut self, _data_bus: u8) {}

    /// Sets the light pen input. `true` = asserted (REQ-342).
    pub const fn set_light_pen(&mut self, _asserted: bool) {}

    /// The CPU started an IRQ or BRK interrupt sequence in this cycle (REQ-335).
    pub const fn irq_sequence_started(&mut self) {}

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
}
