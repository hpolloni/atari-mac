//! The `SingleStepTests` 6502 suite (REQ-023).
//!
//! Each file `test-data/single-step/6502/<opcode>.json` holds 10,000 cases for one opcode.
//! A case gives the start state, the end state and every bus cycle of one instruction.

use core::fmt;
use core::fmt::Write as _;
use core::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::thread;

use atari_core::cpu::{Bus, Cpu, Registers};
use serde::Deserialize;

use crate::{HarnessError, data};

/// The directory of the suite, relative to the repository root.
pub const DIR: &str = "test-data/single-step/6502";

/// The 12 `JAM` (`KIL`) opcodes.
pub const KIL_OPCODES: [u8; 12] = [
    0x02, 0x12, 0x22, 0x32, 0x42, 0x52, 0x62, 0x72, 0x92, 0xB2, 0xD2, 0xF2,
];

/// The 151 documented NMOS 6502 opcodes.
pub const DOCUMENTED_OPCODES: [u8; 151] = [
    0x00, 0x01, 0x05, 0x06, 0x08, 0x09, 0x0A, 0x0D, 0x0E, 0x10, 0x11, 0x15, 0x16, 0x18, 0x19, 0x1D,
    0x1E, 0x20, 0x21, 0x24, 0x25, 0x26, 0x28, 0x29, 0x2A, 0x2C, 0x2D, 0x2E, 0x30, 0x31, 0x35, 0x36,
    0x38, 0x39, 0x3D, 0x3E, 0x40, 0x41, 0x45, 0x46, 0x48, 0x49, 0x4A, 0x4C, 0x4D, 0x4E, 0x50, 0x51,
    0x55, 0x56, 0x58, 0x59, 0x5D, 0x5E, 0x60, 0x61, 0x65, 0x66, 0x68, 0x69, 0x6A, 0x6C, 0x6D, 0x6E,
    0x70, 0x71, 0x75, 0x76, 0x78, 0x79, 0x7D, 0x7E, 0x81, 0x84, 0x85, 0x86, 0x88, 0x8A, 0x8C, 0x8D,
    0x8E, 0x90, 0x91, 0x94, 0x95, 0x96, 0x98, 0x99, 0x9A, 0x9D, 0xA0, 0xA1, 0xA2, 0xA4, 0xA5, 0xA6,
    0xA8, 0xA9, 0xAA, 0xAC, 0xAD, 0xAE, 0xB0, 0xB1, 0xB4, 0xB5, 0xB6, 0xB8, 0xB9, 0xBA, 0xBC, 0xBD,
    0xBE, 0xC0, 0xC1, 0xC4, 0xC5, 0xC6, 0xC8, 0xC9, 0xCA, 0xCC, 0xCD, 0xCE, 0xD0, 0xD1, 0xD5, 0xD6,
    0xD8, 0xD9, 0xDD, 0xDE, 0xE0, 0xE1, 0xE4, 0xE5, 0xE6, 0xE8, 0xE9, 0xEA, 0xEC, 0xED, 0xEE, 0xF0,
    0xF1, 0xF5, 0xF6, 0xF8, 0xF9, 0xFD, 0xFE,
];

/// The 7 unstable undocumented opcodes (REQ-112).
pub const UNSTABLE_OPCODES: [u8; 7] = [0x8B, 0xAB, 0x93, 0x9F, 0x9E, 0x9C, 0x9B];

/// The kind of a bus cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    /// A read cycle.
    Read,
    /// A write cycle.
    Write,
}

/// A CPU and memory state of a case.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct State {
    /// Program counter.
    pub pc: u16,
    /// Stack pointer.
    pub s: u8,
    /// Accumulator.
    pub a: u8,
    /// Index register X.
    pub x: u8,
    /// Index register Y.
    pub y: u8,
    /// Processor status.
    pub p: u8,
    /// Memory bytes as `(address, value)`.
    pub ram: Vec<(u16, u8)>,
}

/// One case of the suite.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Case {
    /// The case name.
    pub name: String,
    /// The start state.
    pub initial: State,
    /// The expected end state.
    pub r#final: State,
    /// The expected bus cycles as `(address, value, kind)`.
    pub cycles: Vec<(u16, u8, Access)>,
}

/// A difference between the CPU and a case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch(pub String);

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A bus with 64 KB of RAM that records every access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatBus {
    /// 64 KB of RAM.
    pub ram: Vec<u8>,
    /// The accesses since the last clear, as `(address, value, kind)`.
    pub log: Vec<(u16, u8, Access)>,
}

impl Default for FlatBus {
    fn default() -> Self {
        Self::new()
    }
}

impl FlatBus {
    /// A bus with all RAM `$00` and an empty log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ram: vec![0; 0x1_0000],
            log: Vec::new(),
        }
    }

    /// The RAM byte at `addr`.
    #[must_use]
    pub fn byte(&self, addr: u16) -> u8 {
        self.ram.get(usize::from(addr)).copied().unwrap_or(0)
    }

    /// Sets the RAM byte at `addr`.
    pub fn set_byte(&mut self, addr: u16, value: u8) {
        if let Some(slot) = self.ram.get_mut(usize::from(addr)) {
            *slot = value;
        }
    }
}

impl Bus for FlatBus {
    fn read(&mut self, addr: u16) -> u8 {
        let value = self.byte(addr);
        self.log.push((addr, value, Access::Read));
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.set_byte(addr, value);
        self.log.push((addr, value, Access::Write));
    }
}

/// The path of the file for `opcode`.
#[must_use]
pub fn file_path(opcode: u8) -> PathBuf {
    data::repo_root()
        .join(DIR)
        .join(format!("{opcode:02x}.json"))
}

/// Reads and parses the cases of the file at `path` (for example [`file_path`]).
///
/// # Errors
///
/// [`HarnessError::Missing`] if the suite is not downloaded, [`HarnessError::Read`] if the
/// file does not parse.
pub fn load_cases(path: &Path) -> Result<Vec<Case>, HarnessError> {
    let bytes = data::read_file(path, data::FETCH_TEST_DATA)?;
    serde_json::from_slice(&bytes).map_err(|error| HarnessError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

/// A CPU at an instruction boundary: a new CPU after its reset sequence.
#[must_use]
pub fn ready_cpu() -> Cpu {
    reset_cpu(&mut FlatBus::new())
}

/// A new CPU that has run its reset sequence on `bus`. The reset sequence makes only reads
/// (REQ-124), so `bus` keeps its RAM; the log gets the reads.
fn reset_cpu(bus: &mut FlatBus) -> Cpu {
    let mut cpu = Cpu::new();
    let _step = cpu.step(bus);
    cpu
}

/// Runs one case on a new CPU at an instruction boundary and on `bus`, then clears the bytes
/// that the case used.
///
/// # Errors
///
/// A [`Mismatch`] that names the first difference.
pub fn run_case(case: &Case, bus: &mut FlatBus) -> Result<(), Mismatch> {
    let mut cpu = reset_cpu(bus);
    *cpu.registers_mut() = Registers {
        a: case.initial.a,
        x: case.initial.x,
        y: case.initial.y,
        s: case.initial.s,
        p: case.initial.p,
        pc: case.initial.pc,
    };
    for &(addr, value) in &case.initial.ram {
        bus.set_byte(addr, value);
    }
    bus.log.clear();
    for _ in &case.cycles {
        let _tick = cpu.tick(bus, true);
    }
    let result = compare(case, &cpu, bus);
    for &(addr, _) in case.initial.ram.iter().chain(&case.r#final.ram) {
        bus.set_byte(addr, 0);
    }
    let touched: Vec<u16> = bus.log.iter().map(|&(addr, _, _)| addr).collect();
    for addr in touched {
        bus.set_byte(addr, 0);
    }
    result
}

/// Compares the CPU, the memory and the bus log with the case.
fn compare(case: &Case, cpu: &Cpu, bus: &FlatBus) -> Result<(), Mismatch> {
    let e = &case.r#final;
    let r = cpu.registers();
    let expected = (e.a, e.x, e.y, e.s, e.p, e.pc);
    let actual = (r.a, r.x, r.y, r.s, r.p, r.pc);
    if expected != actual {
        return Err(Mismatch(format!(
            "{}: registers (a, x, y, s, p, pc) are {actual:02X?}, expected {expected:02X?}",
            case.name
        )));
    }
    if let Some(index) = bus.log.iter().zip(&case.cycles).position(|(a, b)| a != b) {
        return Err(Mismatch(format!(
            "{}: bus cycle {index} is {:?}, expected {:?}",
            case.name,
            bus.log.get(index),
            case.cycles.get(index)
        )));
    }
    if bus.log.len() != case.cycles.len() {
        return Err(Mismatch(format!(
            "{}: {} bus cycles, expected {}",
            case.name,
            bus.log.len(),
            case.cycles.len()
        )));
    }
    if !cpu.at_boundary() {
        return Err(Mismatch(format!(
            "{}: the CPU is not at an instruction boundary after {} cycles",
            case.name,
            case.cycles.len()
        )));
    }
    for &(addr, value) in &e.ram {
        if bus.byte(addr) != value {
            return Err(Mismatch(format!(
                "{}: memory ${addr:04X} is ${:02X}, expected ${value:02X}",
                case.name,
                bus.byte(addr)
            )));
        }
    }
    Ok(())
}

/// The result of one opcode file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReport {
    /// The opcode.
    pub opcode: u8,
    /// The number of cases in the file.
    pub cases: usize,
    /// The number of failed cases.
    pub failures: usize,
    /// The first failure, if any.
    pub first: Option<Mismatch>,
}

/// Runs all cases of each opcode in `opcodes`, in parallel threads. Returns one report per
/// opcode, in the order of `opcodes`.
///
/// # Errors
///
/// The first [`HarnessError`] of a file that cannot be loaded (REQ-021).
pub fn run_opcodes(opcodes: &[u8]) -> Result<Vec<FileReport>, HarnessError> {
    let threads = thread::available_parallelism().map_or(4, NonZeroUsize::get);
    let chunk = opcodes.len().div_ceil(threads).max(1);
    let results: Vec<Result<Vec<FileReport>, HarnessError>> = thread::scope(|scope| {
        let mut handles = Vec::new();
        for part in opcodes.chunks(chunk) {
            handles.push(scope.spawn(move || part.iter().map(|&op| run_file(op)).collect()));
        }
        let mut joined = Vec::with_capacity(handles.len());
        for handle in handles {
            joined.push(handle.join().unwrap_or_else(|_| {
                Err(HarnessError::Read {
                    path: PathBuf::from(DIR),
                    message: "a test thread panicked".to_owned(),
                })
            }));
        }
        joined
    });
    let mut reports = Vec::with_capacity(opcodes.len());
    for result in results {
        reports.extend(result?);
    }
    Ok(reports)
}

/// Runs all cases of one opcode file.
fn run_file(opcode: u8) -> Result<FileReport, HarnessError> {
    let cases = load_cases(&file_path(opcode))?;
    let mut bus = FlatBus::new();
    let mut failures = 0;
    let mut first = None;
    for case in &cases {
        if let Err(mismatch) = run_case(case, &mut bus) {
            failures += 1;
            if first.is_none() {
                first = Some(mismatch);
            }
        }
    }
    Ok(FileReport {
        opcode,
        cases: cases.len(),
        failures,
        first,
    })
}

/// A text summary of the failed reports. Empty if no case failed.
#[must_use]
pub fn failure_summary(reports: &[FileReport]) -> String {
    let mut summary = String::new();
    for report in reports.iter().filter(|report| report.failures > 0) {
        // Writing to a `String` cannot fail; stop on an error to satisfy the type.
        if writeln!(
            summary,
            "opcode ${:02X}: {} of {} cases fail; first: {}",
            report.opcode,
            report.failures,
            report.cases,
            report.first.as_ref().map_or("?", |m| m.0.as_str())
        )
        .is_err()
        {
            break;
        }
    }
    summary
}
