//! Clock and frame timing constants shared by all chips.
//!
//! One "cycle" in this crate is one CPU clock cycle (one machine cycle). ANTIC color clocks
//! run at twice that rate.

use crate::state::impl_state_enum;

/// Machine cycles in one scanline (both NTSC and PAL).
pub const CYCLES_PER_LINE: u32 = 114;

/// Color clocks in one scanline.
pub const COLOR_CLOCKS_PER_LINE: u32 = CYCLES_PER_LINE * 2;

/// First scanline of vertical blank. ANTIC asserts the VBI NMI on this line.
pub const VBLANK_START_LINE: u32 = 248;

/// The TV standard of the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoStandard {
    /// 262 lines, 59.92 Hz, 1.7897725 MHz = 14.31818 MHz / 8 (North America).
    #[default]
    Ntsc,
    /// 312 lines, 49.86 Hz, 1.7734470 MHz (Europe).
    Pal,
}

impl_state_enum!(VideoStandard { Ntsc, Pal });

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

    /// CPU clock frequency in Hz (REQ-200). PAL uses the value that HRM 3.3 states.
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
    use super::{COLOR_CLOCKS_PER_LINE, CYCLES_PER_LINE, VideoStandard};

    /// Verifies REQ-200: the machine clock is 14.31818 MHz / 8 (NTSC) or 1.773447 MHz (PAL).
    #[test]
    fn machine_clock_values() {
        let ntsc = VideoStandard::Ntsc.cpu_clock_hz();
        assert!(
            (ntsc - 14_318_180.0 / 8.0).abs() < 0.5,
            "NTSC machine clock is {ntsc}"
        );
        let pal = VideoStandard::Pal.cpu_clock_hz();
        assert!(
            (pal - 1_773_447.0).abs() < 0.5,
            "PAL machine clock is {pal}"
        );
        assert_eq!(VideoStandard::default(), VideoStandard::Ntsc, "default");
    }

    /// Verifies REQ-200: one machine cycle is 2 color clocks.
    #[test]
    fn color_clocks_per_line() {
        assert_eq!(COLOR_CLOCKS_PER_LINE, 228, "color clocks per line");
        assert_eq!(
            COLOR_CLOCKS_PER_LINE,
            2 * CYCLES_PER_LINE,
            "2 per machine cycle"
        );
    }

    /// Verifies REQ-201: 114 cycles per line, 262 or 312 lines, 59.92 Hz or 49.86 Hz
    /// (criterion 4).
    #[test]
    fn frame_rates_match_the_hardware() {
        assert_eq!(CYCLES_PER_LINE, 114, "cycles per line");
        assert_eq!(VideoStandard::Ntsc.lines_per_frame(), 262, "NTSC lines");
        assert_eq!(VideoStandard::Pal.lines_per_frame(), 312, "PAL lines");
        assert_eq!(
            VideoStandard::Ntsc.cycles_per_frame(),
            29_868,
            "NTSC cycles"
        );
        assert_eq!(VideoStandard::Pal.cycles_per_frame(), 35_568, "PAL cycles");
        let ntsc = VideoStandard::Ntsc.frames_per_second();
        let pal = VideoStandard::Pal.frames_per_second();
        assert!((ntsc - 59.92).abs() < 0.01, "NTSC frame rate is {ntsc}");
        assert!((pal - 49.86).abs() < 0.01, "PAL frame rate is {pal}");
    }
}
