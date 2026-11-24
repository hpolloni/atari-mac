//! ANTIC unit tests (`docs/design/antic.md`, "Test plan").
//!
//! The tests call `tick`, `read`, `write`, `end_cycle` and `reset` directly. A fake memory
//! logs each DMA read. "Position x" is the color clock `(2C - 2) mod 228` or
//! `(2C - 1) mod 228` of the output of cycle `C`.

mod display;
mod dlist;
mod dma;
mod line_buffer;
mod load_state;
mod modes;
mod nmi;
mod nmi_timing;
mod pm;
mod registers;
mod scroll;

use super::{Antic, CycleOutput, DmaKind, DmaMemory};
use crate::timing::{CYCLES_PER_LINE, VBLANK_START_LINE, VideoStandard};

/// The address of the display list in the tests.
const DL: u16 = 0x1000;
/// The address of the screen memory in the tests.
const SCREEN: u16 = 0x2000;
/// The font base page in the tests (`CHBASE` = `$E0`).
const FONT: u16 = 0xE000;

/// 64 KB of memory that logs the DMA reads.
#[derive(Debug, Clone)]
struct FakeMem {
    /// The bytes.
    ram: Vec<u8>,
    /// The addresses of the DMA reads since the last clear.
    log: Vec<u16>,
}

impl DmaMemory for FakeMem {
    fn dma_read(&mut self, addr: u16) -> u8 {
        self.log.push(addr);
        self.ram[usize::from(addr)]
    }
}

/// The record of one cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rec {
    /// The output of `tick`.
    out: CycleOutput,
    /// The address of the DMA read in this cycle.
    read: Option<u16>,
}

/// ANTIC, its memory and the data bus value for cycles without DMA.
#[derive(Debug, Clone)]
struct Rig {
    /// The chip under test.
    antic: Antic,
    /// The fake memory.
    mem: FakeMem,
    /// The data bus value on cycles without a DMA read.
    bus: u8,
}

impl Rig {
    /// ANTIC at power-on.
    fn new(standard: VideoStandard) -> Self {
        Self {
            antic: Antic::new(standard),
            mem: FakeMem {
                ram: vec![0; 0x1_0000],
                log: Vec::new(),
            },
            bus: 0xFF,
        }
    }

    /// NTSC ANTIC that starts the display list `dl` (at `DL`) on scan line 8 of the next
    /// frame, with `DMACTL` = `dmactl` and `CHBASE` = `$E0`. The next tick is (8, 0).
    fn with_dl(dmactl: u8, dl: &[u8]) -> Self {
        let mut rig = Self::new(VideoStandard::Ntsc);
        rig.start_dl(dmactl, dl);
        rig
    }

    /// Loads `dl` in vertical blank and runs to scan line 8, cycle 0.
    fn start_dl(&mut self, dmactl: u8, dl: &[u8]) {
        self.start_dl_at(dmactl, DL, dl);
    }

    /// Like `start_dl`, with the display list at `addr`.
    fn start_dl_at(&mut self, dmactl: u8, addr: u16, dl: &[u8]) {
        self.seek(VBLANK_START_LINE, 0);
        self.poke(addr, dl);
        let [lo, hi] = addr.to_le_bytes();
        self.write(0x02, lo);
        self.write(0x03, hi);
        self.write(0x09, 0xE0);
        self.write(0x00, dmactl);
        self.seek(8, 0);
    }

    /// Writes bytes to memory.
    fn poke(&mut self, addr: u16, bytes: &[u8]) {
        let start = usize::from(addr);
        self.mem.ram[start..start + bytes.len()].copy_from_slice(bytes);
    }

    /// Fills memory.
    fn fill(&mut self, addr: u16, len: usize, value: u8) {
        let start = usize::from(addr);
        self.mem.ram[start..start + len].fill(value);
    }

    /// A register write.
    fn write(&mut self, reg: u8, value: u8) {
        self.antic.write(reg, value);
    }

    /// A register read.
    fn read(&self, reg: u8) -> u8 {
        self.antic.read(reg)
    }

    /// The position of the next tick.
    fn pos(&self) -> (u32, u32) {
        (self.antic.scanline(), self.antic.cycle())
    }

    /// One machine cycle in the machine call order.
    fn step(&mut self) -> Rec {
        self.mem.log.clear();
        let out = self.antic.tick(&mut self.mem);
        let read = self.mem.log.first().copied();
        let data = if out.halt && out.dma != DmaKind::Refresh {
            out.dma_data
        } else {
            self.bus
        };
        self.antic.end_cycle(data);
        Rec { out, read }
    }

    /// Runs until the next tick is (`line`, `cycle`).
    fn run_to(&mut self, line: u32, cycle: u32) -> Vec<Rec> {
        let mut recs = Vec::new();
        for _ in 0..2 * 35_568 {
            if self.pos() == (line, cycle) {
                return recs;
            }
            recs.push(self.step());
        }
        panic!("position ({line}, {cycle}) not reached");
    }

    /// Like `run_to`, without the records.
    fn seek(&mut self, line: u32, cycle: u32) {
        while self.pos() != (line, cycle) {
            let _rec = self.step();
        }
    }

    /// Like `after`, without the records.
    fn seek_after(&mut self, line: u32, cycle: u32) {
        let lines = self.antic.standard.lines_per_frame();
        if cycle + 1 == CYCLES_PER_LINE {
            self.seek((line + 1) % lines, 0);
        } else {
            self.seek(line, cycle + 1);
        }
    }

    /// Runs `cycles` cycles.
    fn skip(&mut self, cycles: usize) {
        for _ in 0..cycles {
            let _rec = self.step();
        }
    }

    /// Runs until the last tick was (`line`, `cycle`). A write after it is "on cycle".
    fn after(&mut self, line: u32, cycle: u32) -> Vec<Rec> {
        let lines = self.antic.standard.lines_per_frame();
        if cycle + 1 == CYCLES_PER_LINE {
            self.run_to((line + 1) % lines, 0)
        } else {
            self.run_to(line, cycle + 1)
        }
    }

    /// Runs `count` full scan lines from cycle 0.
    fn lines(&mut self, count: usize) -> Vec<Vec<Rec>> {
        assert_eq!(self.antic.cycle(), 0, "lines() starts on cycle 0");
        (0..count)
            .map(|_| (0..CYCLES_PER_LINE).map(|_| self.step()).collect())
            .collect()
    }

    /// Runs one full scan line from cycle 0.
    fn line(&mut self) -> Vec<Rec> {
        self.lines(1).remove(0)
    }

    /// Runs one full frame of cycles.
    fn frame(&mut self) -> Vec<Rec> {
        let cycles = self.antic.standard.cycles_per_frame();
        (0..cycles).map(|_| self.step()).collect()
    }
}

/// The codes of one scan line by position.
fn codes(line: &[Rec]) -> [u8; 228] {
    let mut out = [0xFF; 228];
    for rec in line {
        let x = (2 * rec.out.cycle as usize + 226) % 228;
        out[x] = rec.out.video[0].bits();
        out[x + 1] = rec.out.video[1].bits();
    }
    out
}

/// The cycles of `recs` with DMA of `kind`.
fn cycles_of(recs: &[Rec], kind: DmaKind) -> Vec<u32> {
    recs.iter()
        .filter(|rec| rec.out.dma == kind)
        .map(|rec| rec.out.cycle)
        .collect()
}

/// The cycles `first`, `first + step`, ... up to `last`.
fn every(first: u32, last: u32, step: usize) -> Vec<u32> {
    (first..=last).step_by(step).collect()
}

/// The refresh request cycles 25, 29, ..., 57.
fn refresh_cycles() -> Vec<u32> {
    every(25, 57, 4)
}

/// The scan lines of the display list instruction fetches in `recs`.
fn fetch_lines(recs: &[Rec]) -> Vec<u32> {
    recs.iter()
        .filter(|rec| rec.out.dma == DmaKind::DisplayList)
        .map(|rec| rec.out.line)
        .collect()
}

/// The records of scan line `line` in `recs`.
fn on_line(recs: &[Rec], line: u32) -> Vec<Rec> {
    recs.iter()
        .filter(|rec| rec.out.line == line)
        .copied()
        .collect()
}
