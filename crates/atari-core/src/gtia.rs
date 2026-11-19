//! GTIA: color generation, player/missile graphics, collisions, triggers and console keys.
//!
//! Design: `docs/design/gtia.md`. Requirements: REQ-400 to REQ-441. The machine calls
//! [`Gtia::clock`] once per machine cycle, after the CPU access (OI-001).

#![expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    reason = "WP-0 stubs: delete this when the work package replaces the stubs"
)]

use crate::antic::CycleOutput;
use crate::timing::VideoStandard;

/// Frame width in half color clocks (HPOS `$20-$DF`).
pub const FRAME_WIDTH: usize = 384;
/// Frame height in scan lines (scan lines 8-247).
pub const FRAME_HEIGHT: usize = 240;
/// The scan line that is row 0 of the frame buffer.
pub const FIRST_VISIBLE_LINE: u32 = 8;
/// The HPOS that is column pair 0 of the frame buffer.
pub const FIRST_FRAME_HPOS: u32 = 0x20;

/// The state of the three console keys. `true` = pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConsoleKeys {
    /// START key.
    pub start: bool,
    /// SELECT key.
    pub select: bool,
    /// OPTION key.
    pub option: bool,
}

/// The GTIA chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gtia {
    /// The TV standard (it sets the `PAL` register).
    standard: VideoStandard,
    /// The last complete frame.
    front: Vec<u8>,
    /// Trigger inputs `TRIG0`-`TRIG3`. `true` = active (reads 0).
    triggers: [bool; 4],
    /// Console key inputs.
    console: ConsoleKeys,
    /// The last `CONSOL` write, bits 0-3.
    consol_out: u8,
}

impl Gtia {
    /// Creates GTIA in the power-on state (REQ-402). `standard` sets `PAL` (REQ-403).
    #[must_use]
    pub fn new(standard: VideoStandard) -> Self {
        Self {
            standard,
            front: vec![0; FRAME_WIDTH * FRAME_HEIGHT],
            triggers: [false; 4],
            console: ConsoleKeys::default(),
            consol_out: 0,
        }
    }

    /// Power-on reset (REQ-402). The machine does not call it for the RESET key.
    pub const fn reset(&mut self) {
        self.consol_out = 0;
    }

    /// CPU read. `reg` is `addr & 0x1F` (REQ-400). No side effects (REQ-401).
    #[must_use]
    pub fn read(&self, reg: u8) -> u8 {
        // WP-0 STUB: only TRIGn, PAL and CONSOL. WP-2 implements the rest.
        match reg & 0x1F {
            r @ 0x10..=0x13 => {
                let active = self
                    .triggers
                    .get(usize::from(r - 0x10))
                    .copied()
                    .unwrap_or(false);
                u8::from(!active)
            }
            0x14 => match self.standard {
                VideoStandard::Ntsc => 0x0F,
                VideoStandard::Pal => 0x01,
            },
            0x1F => {
                let mut keys = 0;
                if self.console.start {
                    keys |= 0x01;
                }
                if self.console.select {
                    keys |= 0x02;
                }
                if self.console.option {
                    keys |= 0x04;
                }
                0x0F & !self.consol_out & !keys
            }
            _ => 0x0F,
        }
    }

    /// CPU write. `reg` is `addr & 0x1F`.
    pub const fn write(&mut self, reg: u8, value: u8) {
        // WP-0 STUB: only CONSOL.
        if reg & 0x1F == 0x1F {
            self.consol_out = value & 0x0F;
        }
    }

    /// One machine cycle, after the CPU access of the cycle. `antic` is the output of
    /// `Antic::tick` for this cycle. `data_bus` is the data bus value of this cycle.
    pub const fn clock(&mut self, _antic: &CycleOutput, _data_bus: u8) {
        // WP-0 STUB: WP-2 makes the color clocks.
    }

    /// The last complete frame (REQ-405): one GTIA color byte per high-resolution pixel.
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        &self.front
    }

    /// Trigger input `index` 0-3. `true` = active (the register reads 0).
    pub fn set_trigger(&mut self, index: usize, active: bool) {
        if let Some(trigger) = self.triggers.get_mut(index) {
            *trigger = active;
        }
    }

    /// Console key inputs (REQ-437).
    pub const fn set_console_keys(&mut self, keys: ConsoleKeys) {
        self.console = keys;
    }

    /// The speaker line: `CONSOL` bit 3 of the last write (REQ-438).
    #[must_use]
    pub const fn speaker(&self) -> bool {
        self.consol_out & 0x08 != 0
    }
}
