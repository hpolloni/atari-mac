//! GTIA: color generation, player/missile graphics, collisions, triggers and console keys.
//!
//! Design: `docs/design/gtia.md`. Requirements: REQ-400 to REQ-441. The machine calls
//! [`Gtia::clock`] once per machine cycle, after the CPU access (OI-001).
//!
//! The hardware source is chapter 6 of the Altirra Hardware Reference Manual (HRM).

mod priority;
#[cfg(test)]
mod tests;

use std::mem;
use std::ops::RangeInclusive;

use crate::antic::{AnCode, CycleOutput, DmaKind};
use crate::state::impl_state;
use crate::timing::{VBLANK_START_LINE, VideoStandard};

/// Frame width in half color clocks (HPOS `$20-$DF`).
pub const FRAME_WIDTH: usize = 384;
/// Frame height in scan lines (scan lines 8-247).
pub const FRAME_HEIGHT: usize = 240;
/// The scan line that is row 0 of the frame buffer.
pub const FIRST_VISIBLE_LINE: u32 = 8;
/// The HPOS that is column pair 0 of the frame buffer.
pub const FIRST_FRAME_HPOS: u32 = 0x20;

/// The last HPOS of the frame buffer.
const LAST_FRAME_HPOS: u32 = 0xDF;
/// The visible HPOS range, also the collision area (HRM 6.2, REQ-406, REQ-422).
const VISIBLE: RangeInclusive<u32> = 0x22..=0xDD;
/// The size of the pending write ring.
const PENDING: usize = 16;
/// The pseudo register for `PRIOR` bits 6-7, which have their own delay (REQ-434).
const PRIOR_HIGH: u8 = 0x20;

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

impl_state!(ConsoleKeys {
    start,
    select,
    option
});

/// A write that takes effect at a later color clock (REQ-434).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct PendingWrite {
    /// The absolute color clock of the effect.
    time: u64,
    /// The register (`$00-$1B`, or `PRIOR_HIGH`).
    reg: u8,
    /// The value.
    value: u8,
}

impl_state!(PendingWrite { time, reg, value });

/// The playfield inputs of one color clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Playfield {
    /// The code is blank (horizontal blank or vertical sync).
    blank: bool,
    /// `PF0-PF3` inputs to the priority logic (bits 0-3).
    inputs: u8,
    /// `PF0-PF3` inputs to the collision logic (bits 0-3).
    collide: u8,
    /// Mode 10 player inputs `P0-P3` (bits 0-3).
    players: u8,
    /// The hi-res pixel pair (bit 1 = left), if the color clock is hi-res.
    hires: Option<u8>,
}

/// The GTIA chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gtia {
    /// The TV standard (it sets the `PAL` register).
    standard: VideoStandard,
    /// `HPOSP0-3`.
    hposp: [u8; 4],
    /// `HPOSM0-3`.
    hposm: [u8; 4],
    /// `SIZEP0-3`, bits 0-1.
    sizep: [u8; 4],
    /// `SIZEM`.
    sizem: u8,
    /// `GRAFP0-3`.
    grafp: [u8; 4],
    /// `GRAFM`.
    grafm: u8,
    /// `COLPM0-3`, bit 0 cleared.
    colpm: [u8; 4],
    /// `COLPF0-3`, bit 0 cleared.
    colpf: [u8; 4],
    /// `COLBK`, bit 0 cleared.
    colbk: u8,
    /// `PRIOR` in effect.
    prior: u8,
    /// `VDELAY`.
    vdelay: u8,
    /// `GRACTL`.
    gractl: u8,
    /// The last `CONSOL` write, bits 0-3.
    consol_out: u8,
    /// Writes that are not in effect yet, in write order.
    pending: [PendingWrite; PENDING],
    /// The number of entries in `pending`.
    pending_len: usize,
    /// The shift registers: players 0-3, then missiles 0-3.
    shift: [u8; 8],
    /// The 2-bit shift states.
    shift_state: [u8; 8],
    /// `M0PF-M3PF`, `P0PF-P3PF`, `M0PL-M3PL`, `P0PL-P3PL`, bits 0-3.
    collisions: [u8; 16],
    /// The hires flip-flop: the last horizontal blank code was `%011`.
    hires: bool,
    /// The last GTIA mode pixel (for the mode 10 delay).
    mode_10_pixel: u8,
    /// ANTIC did a missile DMA cycle on cycle 0 of this scan line (REQ-441).
    missile_line: bool,
    /// Trigger inputs. `true` = active. `trig_input[2]` is always `false`.
    trig_input: [bool; 4],
    /// Trigger latches (REQ-436).
    trig_latch: [bool; 4],
    /// Console key inputs.
    console: ConsoleKeys,
    /// The absolute color clock of the first color clock of the next `clock`.
    clock: u64,
    /// The scan line of the last `clock`.
    line: u32,
    /// The cycle of the last `clock`.
    cycle: u32,
    /// The frame that GTIA makes.
    back: Vec<u8>,
    /// The last complete frame.
    front: Vec<u8>,
    /// The priority table (see `priority`).
    priority: Box<[u16]>,
}

impl_state!(Gtia {
    standard,
    hposp,
    hposm,
    sizep,
    sizem,
    grafp,
    grafm,
    colpm,
    colpf,
    colbk,
    prior,
    vdelay,
    gractl,
    consol_out,
    pending,
    pending_len,
    shift,
    shift_state,
    collisions,
    hires,
    mode_10_pixel,
    missile_line,
    trig_input,
    trig_latch,
    console,
    clock,
    line,
    cycle,
    back,
    front,
} skip {
    priority
});

/// The write delay of a register in color clocks (HRM 6.10 Table 18, REQ-434).
const fn write_delay(reg: u8) -> u64 {
    match reg {
        0x12..=0x1A => 1,
        0x1B => 2,
        0x0D..=0x11 => 3,
        PRIOR_HIGH => 4,
        _ => 5,
    }
}

/// The shift state mask for a 2-bit size: `state' = (state + 1) AND size` (HRM 6.5, REQ-419).
/// Size `%10` gives single width from state `%00`, and the lockup from states `%01` and `%10`.
const fn size_mask(size: u8) -> u8 {
    size & 3
}

impl Gtia {
    /// Creates GTIA in the power-on state (REQ-402). `standard` sets `PAL` (REQ-403).
    #[must_use]
    pub fn new(standard: VideoStandard) -> Self {
        Self {
            standard,
            hposp: [0; 4],
            hposm: [0; 4],
            sizep: [0; 4],
            sizem: 0,
            grafp: [0; 4],
            grafm: 0,
            colpm: [0; 4],
            colpf: [0; 4],
            colbk: 0,
            prior: 0,
            vdelay: 0,
            gractl: 0,
            consol_out: 0,
            pending: [PendingWrite::default(); PENDING],
            pending_len: 0,
            shift: [0; 8],
            shift_state: [0; 8],
            collisions: [0; 16],
            hires: false,
            mode_10_pixel: 0,
            missile_line: false,
            trig_input: [false; 4],
            trig_latch: [false; 4],
            console: ConsoleKeys::default(),
            clock: 0,
            line: 0,
            cycle: 0,
            back: vec![0; FRAME_WIDTH * FRAME_HEIGHT],
            front: vec![0; FRAME_WIDTH * FRAME_HEIGHT],
            priority: priority::table(),
        }
    }

    /// Power-on reset (REQ-402). The machine does not call it for the RESET key. The
    /// trigger and console key inputs are inputs, not state, so they stay.
    pub fn reset(&mut self) {
        let inputs = (self.trig_input, self.console);
        let priority = mem::take(&mut self.priority);
        *self = Self {
            priority,
            ..Self::new(self.standard)
        };
        (self.trig_input, self.console) = inputs;
    }

    /// CPU read. `reg` is `addr & 0x1F` (REQ-400). No side effects (REQ-401).
    #[must_use]
    pub fn read(&self, reg: u8) -> u8 {
        let reg = reg & 0x1F;
        match reg {
            0x00..=0x0F => self.collisions[usize::from(reg)],
            0x10..=0x13 => {
                let n = usize::from(reg - 0x10);
                u8::from(!(self.trig_input[n] || self.trig_latch[n]))
            }
            0x14 => match self.standard {
                VideoStandard::Ntsc => 0x0F,
                VideoStandard::Pal => 0x01,
            },
            0x1F => {
                let keys = u8::from(self.console.start)
                    | (u8::from(self.console.select) << 1)
                    | (u8::from(self.console.option) << 2);
                0x0F & !self.consol_out & !keys
            }
            _ => 0x0F,
        }
    }

    /// CPU write. `reg` is `addr & 0x1F`. Applies in the cycle of the last `clock`
    /// (REQ-434).
    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x1F {
            0x1C => self.vdelay = value,
            0x1D => {
                self.gractl = value & 0x07;
                if value & 0x04 == 0 {
                    self.trig_latch = [false; 4];
                } else {
                    for (latch, input) in self.trig_latch.iter_mut().zip(self.trig_input) {
                        *latch |= input;
                    }
                }
            }
            0x1E => self.collisions = [0; 16],
            0x1F => self.consol_out = value & 0x0F,
            0x1B => {
                self.schedule(0x1B, value);
                self.schedule(PRIOR_HIGH, value);
            }
            reg => self.schedule(reg, value),
        }
    }

    /// One machine cycle, after the CPU access of the cycle. `antic` is the output of
    /// `Antic::tick` for this cycle. `data_bus` is the data bus value of this cycle.
    pub fn clock(&mut self, antic: &CycleOutput, data_bus: u8) {
        self.line = antic.line;
        self.cycle = antic.cycle;
        if antic.line == VBLANK_START_LINE && antic.cycle == 0 {
            mem::swap(&mut self.front, &mut self.back);
        }
        self.pm_dma(antic, data_bus);
        let h0 = (2 * antic.cycle + 226) % 228;
        let [left, right] = antic.video;
        let mut pixel = ((left.bits() & 3) << 2) | (right.bits() & 3);
        // REQ-433: in mode 10, a `%000` right code mutes the playfield of the pixel: it gives
        // pixel 8 (background). Pixel 0 is `P0`, not playfield, so the border stays `P0`
        // (HRM 6.9, REQ-430).
        if self.prior >> 6 == 2 && right == AnCode::BACKGROUND && pixel != 0 {
            pixel = 8;
        }
        self.color_clock(h0, left, pixel, false);
        self.color_clock(h0 + 1, right, pixel, true);
        self.clock += 2;
    }

    /// The last complete frame (REQ-405): one GTIA color byte per high-resolution pixel.
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        &self.front
    }

    /// Trigger input `index` 0-3. `true` = active (the register reads 0). Trigger 2 is not
    /// connected on the 65XE (REQ-435).
    pub fn set_trigger(&mut self, index: usize, active: bool) {
        if index == 2 {
            return;
        }
        if let Some(input) = self.trig_input.get_mut(index) {
            *input = active;
            if active && self.gractl & 0x04 != 0 {
                self.trig_latch[index] = true;
            }
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

    /// Puts a write into `pending` with the delay of its register.
    fn schedule(&mut self, reg: u8, value: u8) {
        if self.pending_len == PENDING {
            let oldest = self.pending[0];
            self.apply(oldest.reg, oldest.value);
            self.pending.copy_within(1.., 0);
            self.pending_len -= 1;
        }
        self.pending[self.pending_len] = PendingWrite {
            time: self.clock + write_delay(reg),
            reg,
            value,
        };
        self.pending_len += 1;
    }

    /// Applies the pending writes up to color clock `now`, in write order.
    fn apply_pending(&mut self, now: u64) {
        if self.pending_len == 0 {
            return;
        }
        let mut kept = 0;
        for i in 0..self.pending_len {
            let write = self.pending[i];
            if write.time <= now {
                self.apply(write.reg, write.value);
            } else {
                self.pending[kept] = write;
                kept += 1;
            }
        }
        self.pending_len = kept;
    }

    /// A write takes effect.
    fn apply(&mut self, reg: u8, value: u8) {
        let color = value & 0xFE;
        match reg {
            0x00..=0x03 => self.hposp[usize::from(reg)] = value,
            0x04..=0x07 => self.hposm[usize::from(reg - 0x04)] = value,
            0x08..=0x0B => self.sizep[usize::from(reg - 0x08)] = value & 3,
            0x0C => self.sizem = value,
            0x0D..=0x10 => self.grafp[usize::from(reg - 0x0D)] = value,
            0x11 => self.grafm = value,
            0x12..=0x15 => self.colpm[usize::from(reg - 0x12)] = color,
            0x16..=0x19 => self.colpf[usize::from(reg - 0x16)] = color,
            0x1A => self.colbk = color,
            0x1B => self.prior = (self.prior & 0xC0) | (value & 0x3F),
            PRIOR_HIGH => {
                if self.prior & 0xC0 == 0 && value & 0xC0 != 0 {
                    // REQ-432: a GTIA mode turns the hires state off until the next %011.
                    self.hires = false;
                }
                self.prior = (self.prior & 0x3F) | (value & 0xC0);
            }
            _ => {}
        }
    }

    /// The P/M DMA bytes of this cycle (REQ-414, REQ-415, REQ-441).
    fn pm_dma(&mut self, antic: &CycleOutput, data_bus: u8) {
        let even = antic.line & 1 == 0;
        if antic.cycle == 0 {
            self.missile_line = antic.dma == DmaKind::Missile;
        }
        match antic.dma {
            DmaKind::Missile if self.gractl & 0x01 != 0 => {
                let mut value = data_bus;
                for n in 0..4 {
                    if even && self.vdelay & (1 << n) != 0 {
                        let mask = 3 << (2 * n);
                        value = (value & !mask) | (self.grafm & mask);
                    }
                }
                self.schedule(0x11, value);
            }
            DmaKind::Player(n) if self.gractl & 0x02 != 0 => self.load_player(n, data_bus, even),
            _ if self.missile_line && self.gractl & 0x02 != 0 => {
                if let Some(n) = antic.cycle.checked_sub(2).filter(|&n| n < 4) {
                    self.load_player(u8::try_from(n).unwrap_or(0), data_bus, even);
                }
            }
            _ => {}
        }
    }

    /// A player DMA byte, unless `VDELAY` masks it on an even scan line.
    fn load_player(&mut self, n: u8, value: u8, even: bool) {
        if !(even && self.vdelay & (0x10 << n) != 0) {
            self.schedule(0x0D + n, value);
        }
    }

    /// Runs the 8 shifters for one color clock. Bits 0-3 are players, bits 4-7 missiles.
    fn shift_objects(&mut self, h: u32) -> u8 {
        if let Ok(h) = u8::try_from(h)
            && !self.position_match(h)
        {
            return self.shift_unloaded();
        }
        let mut bits = 0;
        for i in 0..8 {
            let (position, latch, size) = if i < 4 {
                (self.hposp[i], self.grafp[i], self.sizep[i])
            } else {
                let n = i - 4;
                let latch = ((self.grafm >> (2 * n)) & 3) << 6;
                (self.hposm[n], latch, self.sizem >> (2 * n))
            };
            if h == u32::from(position) {
                self.shift[i] |= latch;
                self.shift_state[i] = 0;
            }
            if self.shift[i] & 0x80 != 0 {
                bits |= 1 << i;
            }
            self.shift_state[i] = (self.shift_state[i] + 1) & size_mask(size);
            if self.shift_state[i] == 0 {
                self.shift[i] <<= 1;
            }
        }
        bits
    }

    /// `true` if the horizontal position of a player or a missile is `h`.
    fn position_match(&self, h: u8) -> bool {
        /// The low bit of each byte.
        const LOW: u64 = 0x0101_0101_0101_0101;
        /// The high bit of each byte.
        const HIGH: u64 = 0x8080_8080_8080_8080;
        let [p0, p1, p2, p3] = self.hposp;
        let [m0, m1, m2, m3] = self.hposm;
        let positions = u64::from_le_bytes([p0, p1, p2, p3, m0, m1, m2, m3]);
        // A byte of `diff` is 0 where the position is `h`. The expression is not 0 exactly
        // when a byte of `diff` is 0.
        let diff = positions ^ LOW.wrapping_mul(u64::from(h));
        diff.wrapping_sub(LOW) & !diff & HIGH != 0
    }

    /// `shift_objects` for a color clock at which no position matches, so that no shifter
    /// loads: the same steps for the 8 shifters at once, one byte for each shifter.
    const fn shift_unloaded(&mut self) -> u8 {
        /// The low bit of each byte.
        const LOW: u64 = 0x0101_0101_0101_0101;
        let shift = u64::from_le_bytes(self.shift);
        // The output bits: bit 7 of each shifter byte, gathered into one byte.
        let [.., bits] = (((shift >> 7) & LOW).wrapping_mul(0x0102_0408_1020_4080)).to_le_bytes();
        let [s0, s1, s2, s3] = self.sizep;
        let m = self.sizem;
        let sizes = u64::from_le_bytes([s0, s1, s2, s3, m, m >> 2, m >> 4, m >> 6]);
        let masks = sizes & (LOW * 3);
        // Each state is 0-3, so the add does not carry into the next byte, and the operations
        // do not overflow.
        let state = u64::from_le_bytes(self.shift_state).wrapping_add(LOW) & masks;
        // 0xFF in each byte whose new state is 0, else 0.
        let zero = (!(state | (state >> 1)) & LOW).wrapping_mul(0xFF);
        let shifted = (shift << 1) & !LOW;
        self.shift = ((shift & !zero) | (shifted & zero)).to_le_bytes();
        self.shift_state = state.to_le_bytes();
        bits
    }

    /// Decodes the ANTIC code of one color clock (REQ-407, REQ-408, REQ-428 to REQ-433).
    fn decode(&mut self, code: AnCode, pixel: u8, odd: bool) -> Playfield {
        let bits = code.bits();
        if matches!(bits, 0b001..=0b011) {
            if bits & 0b010 != 0 {
                self.hires = bits == 0b011;
            }
            return Playfield {
                blank: true,
                ..Playfield::default()
            };
        }
        match self.prior >> 6 {
            0 if bits == 0 => Playfield::default(),
            0 if self.hires => Playfield {
                inputs: 1 << 2,
                collide: if AnCode::hires(bits) == AnCode::hires(0) {
                    0
                } else {
                    1 << 2
                },
                hires: Some(bits & 3),
                ..Playfield::default()
            },
            0 => {
                let pf = 1 << (bits & 3);
                Playfield {
                    inputs: pf,
                    collide: pf,
                    ..Playfield::default()
                }
            }
            2 => self.mode_10(pixel, odd),
            _ => Playfield::default(),
        }
    }

    /// GTIA mode 10: 9 colors with a one color clock delay (REQ-430, REQ-433).
    fn mode_10(&mut self, pixel: u8, odd: bool) -> Playfield {
        let p = if odd {
            self.mode_10_pixel = pixel;
            pixel
        } else {
            self.mode_10_pixel
        };
        match p {
            0..=3 => Playfield {
                players: 1 << p,
                ..Playfield::default()
            },
            4..=7 | 12..=15 => {
                let pf = 1 << (p & 3);
                Playfield {
                    inputs: pf,
                    collide: pf,
                    ..Playfield::default()
                }
            }
            _ => Playfield::default(),
        }
    }

    /// One color clock at HPOS `h` (see `docs/design/gtia.md`, "Color clock pipeline").
    fn color_clock(&mut self, h: u32, code: AnCode, pixel: u8, odd: bool) {
        self.apply_pending(self.clock + u64::from(odd));
        let objects = self.shift_objects(h);
        let pf = self.decode(code, pixel, odd);
        let visible_line = (FIRST_VISIBLE_LINE..VBLANK_START_LINE).contains(&self.line);
        let visible = visible_line && VISIBLE.contains(&h);
        if visible && !pf.blank {
            self.collide(objects, pf.collide);
        }
        if visible_line && (FIRST_FRAME_HPOS..=LAST_FRAME_HPOS).contains(&h) {
            // `color` has no side effects, so it runs only for a color clock that the frame
            // stores.
            let (left, right) = if visible && !pf.blank {
                self.color(objects, pf, pixel)
            } else {
                (0, 0)
            };
            let row = (self.line - FIRST_VISIBLE_LINE) as usize;
            let column = 2 * (h - FIRST_FRAME_HPOS) as usize;
            let index = row * FRAME_WIDTH + column;
            self.back[index] = left;
            self.back[index + 1] = right;
        }
    }

    /// Sets the collision bits of one color clock (REQ-420, REQ-421).
    fn collide(&mut self, objects: u8, playfield: u8) {
        if objects == 0 {
            return;
        }
        let players = objects & 0x0F;
        for n in 0..4 {
            if players & (1 << n) != 0 {
                self.collisions[4 + n] |= playfield;
                self.collisions[12 + n] |= players & !(1 << n);
            }
            if objects & (0x10 << n) != 0 {
                self.collisions[n] |= playfield;
                self.collisions[8 + n] |= players;
            }
        }
    }

    /// The left and right color of one color clock (REQ-408, REQ-424 to REQ-431). Missile n
    /// is the input `Pn`: GTIA ignores `PRIOR` bit 4 (the fifth player is a non-goal,
    /// OI-019).
    fn color(&self, objects: u8, pf: Playfield, pixel: u8) -> (u8, u8) {
        let players = (objects & 0x0F) | (objects >> 4) | pf.players;
        let inputs = players | (pf.inputs << 4);
        let mask = self.priority[priority::index(self.prior, inputs)];
        let bak = match self.prior >> 6 {
            1 => self.colbk | pixel,
            3 if pixel == 0 => self.colbk & 0xF0,
            3 => self.colbk | (pixel << 4),
            _ => self.colbk,
        };
        // A select for each mask bit, without branches: `0 - bit` is `$FF` for a bit of 1.
        let select = |bit: u16| 0_u8.wrapping_sub(u8::from(mask & bit != 0));
        let mut color = bak & select(priority::BAK);
        for n in 0..4 {
            color |= self.colpm[n] & select(1 << n);
            color |= self.colpf[n] & select(0x10 << n);
        }
        let lum = (color & 0xF0) | (self.colpf[1] & 0x0E);
        pf.hires.map_or((color, color), |pair| {
            let half = |bit: u8| if pair & bit == 0 { color } else { lum };
            (half(2), half(1))
        })
    }
}
