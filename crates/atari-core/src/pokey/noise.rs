//! The 4-bit, 5-bit and 9/17-bit polynomial counters and `RANDOM` (HRM 5.5).
//!
//! Design: `docs/design/pokey.md`, "Noise generators". Requirements: REQ-512 to REQ-514.
//!
//! All counters shift right by one bit in each machine cycle, and the new bit enters at the
//! top. In initialization mode the new bit is 0, so the registers clear to 0. The 4-bit and
//! 5-bit counters are XNOR counters, and the 9/17-bit counter is stored so that `RANDOM`
//! reads its bits inverted (HRM 14.4 `RANDOM` table, OI-031).

use crate::state::impl_state;

/// The noise bits that one audio channel sees in a cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NoiseBits {
    /// The 4-bit counter bit.
    pub(super) poly4: bool,
    /// The 5-bit counter bit (1 lets an audio clock through, REQ-509).
    pub(super) poly5: bool,
    /// The 9/17-bit counter bit.
    pub(super) poly17: bool,
}

/// The three polynomial counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Noise {
    /// The 4-bit counter (bits 3-0).
    poly4: u8,
    /// The 5-bit counter (bits 4-0).
    poly5: u8,
    /// The 9/17-bit register (bits 16-0). The 9-bit counter uses bits 16-8.
    poly17: u32,
    /// The output bits of the 4-bit counter: bit k is the output of k cycles ago.
    hist4: u8,
    /// The output bits of the 5-bit counter: bit k is the output of k cycles ago.
    hist5: u8,
}

impl_state!(Noise {
    poly4,
    poly5,
    poly17,
    hist4,
    hist5
});

impl Noise {
    /// The power-on state: all counters zero.
    pub(super) const fn new() -> Self {
        Self {
            poly4: 0,
            poly5: 0,
            poly17: 0,
            hist4: 0,
            hist5: 0,
        }
    }

    /// One machine cycle. `init` is initialization mode, `nine_bit` is `AUDCTL` bit 7.
    pub(super) const fn tick(&mut self, init: bool, nine_bit: bool) {
        let (new4, new5, new17) = if init {
            (0, 0, 0)
        } else {
            // 1 + x^3 + x^4 and 1 + x^3 + x^5, XNOR feedback into the top bit.
            let new4 = !(self.poly4 ^ (self.poly4 >> 1)) & 1;
            let new5 = !(self.poly5 ^ (self.poly5 >> 2)) & 1;
            let new17 = if nine_bit {
                // 1 + x^4 + x^9 on bits 16-8.
                !((self.poly17 >> 8) ^ (self.poly17 >> 13)) & 1
            } else {
                // 1 + x^12 + x^17 on bits 16-0.
                !(self.poly17 ^ (self.poly17 >> 5)) & 1
            };
            (new4, new5, new17)
        };
        self.poly4 = (self.poly4 >> 1) | (new4 << 3);
        self.poly5 = (self.poly5 >> 1) | (new5 << 4);
        self.poly17 = (self.poly17 >> 1) | (new17 << 16);
        self.hist4 = ((self.hist4 << 1) | (self.poly4 & 1)) & 0x0F;
        self.hist5 = ((self.hist5 << 1) | (self.poly5 & 1)) & 0x0F;
    }

    /// `RANDOM`: bits 15-8 of the register, inverted (HRM 14.4).
    pub(super) const fn random(self) -> u8 {
        let [low, ..] = (self.poly17 >> 8).to_le_bytes();
        !low
    }

    /// The noise bits of channel `n` (0-3) in this cycle. Channel n gets each bit n cycles
    /// after channel 1 (REQ-512). In initialization mode the 5-bit bit is 1 (HRM 5.5).
    pub(super) const fn bits(self, n: usize, init: bool) -> NoiseBits {
        NoiseBits {
            poly4: (self.hist4 >> n) & 1 != 0,
            poly5: init || (self.hist5 >> n) & 1 != 0,
            // Bit 7 is bit 8 (`RANDOM` bit 0, inverted) of one cycle ago.
            poly17: (self.poly17 >> (7 - n)) & 1 != 0,
        }
    }
}
