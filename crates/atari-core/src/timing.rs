//! Clock and frame timing constants shared by all chips.
//!
//! One "cycle" in this crate is one CPU clock cycle (one machine cycle). ANTIC color clocks
//! run at twice that rate.

/// Machine cycles in one scanline (both NTSC and PAL).
pub const CYCLES_PER_LINE: u32 = 114;

/// Color clocks in one scanline.
pub const COLOR_CLOCKS_PER_LINE: u32 = CYCLES_PER_LINE * 2;

/// First scanline of vertical blank. ANTIC asserts the VBI NMI on this line.
pub const VBLANK_START_LINE: u32 = 248;

/// The TV standard of the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoStandard {
    /// 262 lines, 59.92 Hz, 1.7897725 MHz (North America).
    #[default]
    Ntsc,
    /// 312 lines, 49.86 Hz, 1.7734470 MHz (Europe).
    Pal,
}

impl VideoStandard {
    /// Scanlines in one frame.
    #[must_use]
    pub const fn lines_per_frame(self) -> u32 {
        match self {
            Self::Ntsc => 262,
            Self::Pal => 312,
        }
    }

    /// Machine cycles in one frame.
    #[must_use]
    pub const fn cycles_per_frame(self) -> u32 {
        self.lines_per_frame() * CYCLES_PER_LINE
    }

    /// CPU clock frequency in Hz.
    #[must_use]
    pub const fn cpu_clock_hz(self) -> f64 {
        match self {
            Self::Ntsc => 1_789_772.5,
            Self::Pal => 1_773_447.0,
        }
    }

    /// Frames per second.
    #[must_use]
    pub fn frames_per_second(self) -> f64 {
        self.cpu_clock_hz() / f64::from(self.cycles_per_frame())
    }
}

#[cfg(test)]
mod tests {
    use super::VideoStandard;

    #[test]
    fn frame_rates_match_the_hardware() {
        let ntsc = VideoStandard::Ntsc.frames_per_second();
        let pal = VideoStandard::Pal.frames_per_second();
        assert!((ntsc - 59.92).abs() < 0.01, "NTSC frame rate is {ntsc}");
        assert!((pal - 49.86).abs() < 0.01, "PAL frame rate is {pal}");
    }
}
