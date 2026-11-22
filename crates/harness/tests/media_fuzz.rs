//! The media fuzz tests: the parsers of the media and the machine never panic on random
//! input (REQ-061). The test names follow the test plan in `docs/design/code-gates.md`.
//!
//! Each test runs `proptest` strategies with the configuration of [`config`]: 1000 cases
//! for each strategy, a fixed seed, and no failure persistence file. Each format has a
//! strategy of random bytes and a strategy of a valid header with random bytes after it.
//! A machine with the free ROMs runs 10 frames with each input that a parser accepts.
//!
//! A panic that a test finds is a parser defect. The input then stays in this file as a
//! normal test case.

use std::cell::Cell;

use atari_core::machine::{BASIC_ROM_SIZE, OS_ROM_SIZE};
use atari_core::media::{Cartridge, DiskImage, Geometry, Tape, Xex};
use atari_core::{Machine, MachineConfig, StateError, VideoStandard};
use atari_harness::system::TEST_SAMPLE_RATE;
use atari_harness::{HarnessError, data};
use proptest::arbitrary::any;
use proptest::collection::vec;
use proptest::option;
use proptest::prop_oneof;
use proptest::strategy::{Just, Strategy, ValueTree as _};
use proptest::test_runner::{Config, RngSeed, TestCaseError, TestRunner};

/// The fixed seed of the random generator (REQ-061 criterion 4).
const SEED: u64 = 0x0061_F022_5EED_0001;

/// The cases of each strategy (REQ-061 criterion 2).
const CASES: u32 = 1_000;

/// The largest length of a random input (REQ-061 criterion 2).
const MAX_LEN: usize = 4_096;

/// The frames that a machine runs with each accepted input (REQ-061 criterion 3).
const FRAMES: u32 = 10;

/// The length of the save state header: the signature (8 bytes), the format version
/// (`u16`) and the configuration hash (`u64`) (REQ-018, `docs/design/architecture.md`,
/// "Save states").
const STATE_HEADER: usize = 18;

/// The length of the save state checksum at the end of the state.
const STATE_SUM: usize = 8;

/// The configuration of each test: [`CASES`] cases, the seed [`SEED`], and no failure
/// persistence file (REQ-061 criterion 4).
fn config() -> Config {
    let mut config = Config::with_cases(CASES);
    config.rng_seed = RngSeed::Fixed(SEED);
    config.failure_persistence = None;
    config
}

/// Runs `test` with the [`config`] cases of `strategy`, and fails with the smallest input
/// that panics or fails. Returns the number of cases for which `test` returned `true` (the
/// cases with an accepted input).
fn check<S: Strategy>(name: &str, strategy: &S, test: impl Fn(S::Value) -> bool) -> u32 {
    let accepted = Cell::new(0_u32);
    let mut runner = TestRunner::new(config());
    let failure = runner
        .run(strategy, |input| {
            if test(input) {
                accepted.set(accepted.get() + 1);
            }
            Ok::<(), TestCaseError>(())
        })
        .err()
        .map(|error| error.to_string());
    assert_eq!(failure, None, "{name}: no input fails");
    accepted.get()
}

/// Random bytes of 0 to [`MAX_LEN`] bytes.
fn random_bytes() -> impl Strategy<Value = Vec<u8>> {
    vec(any::<u8>(), 0..=MAX_LEN)
}

/// `prefix`, repeated to `len` bytes (`$00` bytes if `prefix` is empty).
fn fill(prefix: &[u8], len: usize) -> Vec<u8> {
    if prefix.is_empty() {
        return vec![0; len];
    }
    prefix.iter().copied().cycle().take(len).collect()
}

/// The free ROMs of `roms/` (REQ-061 criterion 3, as the other harness tests do).
struct Roms {
    /// The OS ROM.
    os: Vec<u8>,
    /// The BASIC ROM.
    basic: Vec<u8>,
}

impl Roms {
    /// Reads the free ROMs.
    fn read() -> Result<Self, HarnessError> {
        Ok(Self {
            os: data::os_rom()?,
            basic: data::basic_rom()?,
        })
    }

    /// A configuration with the free ROMs and no media.
    fn config(&self, fast: bool) -> MachineConfig {
        MachineConfig {
            os_rom: self.os.clone(),
            basic_rom: Some(self.basic.clone()),
            cartridge: None,
            disk: None,
            fast_sio: fast,
            fast_tape: fast,
            tape: None,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        }
    }

    /// A machine with `config`, which has the free ROMs.
    #[expect(
        clippy::unwrap_used,
        reason = "the free ROMs have the sizes of the ROMs"
    )]
    fn machine(config: MachineConfig) -> Machine {
        Machine::new(config).unwrap()
    }
}

/// Runs [`FRAMES`] frames of `machine` and reads its outputs, as the frontend does.
fn run(machine: &mut Machine) {
    let mut audio = Vec::new();
    for _ in 0..FRAMES {
        machine.run_frame();
        machine.drain_audio(&mut audio);
        audio.clear();
        assert!(!machine.frame().is_empty(), "frame buffer");
    }
}

// ---------------------------------------------------------------------------------------
// Cartridges (REQ-604, REQ-605)
// ---------------------------------------------------------------------------------------

/// The supported CAR types and their data sizes (REQ-605).
const CAR_TYPES: [(u32, usize); 4] = [(1, 0x2000), (2, 0x4000), (41, 0x2_0000), (42, 0x10_0000)];

/// A raw cartridge of a valid size (REQ-604): 8192 or 16384 random bytes.
fn raw_cartridge() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![vec(any::<u8>(), 0x2000), vec(any::<u8>(), 0x4000)]
}

/// The fields of a CAR file with a valid header (REQ-605).
#[derive(Debug, Clone)]
struct CarFile {
    /// The CAR type: a supported type, or a random value.
    car_type: u32,
    /// The data size: the size of the type, or a random size.
    size: usize,
    /// The checksum agrees with the data.
    good_sum: bool,
    /// The header bytes 12-15.
    extra: [u8; 4],
    /// The random data, repeated to `size`.
    data: Vec<u8>,
}

impl CarFile {
    /// The bytes of the file.
    fn bytes(&self) -> Vec<u8> {
        let data = fill(&self.data, self.size);
        let sum = if self.good_sum {
            data.iter()
                .fold(0_u32, |sum, &byte| sum.wrapping_add(u32::from(byte)))
        } else {
            0x1234_5678
        };
        let mut file = b"CART".to_vec();
        file.extend_from_slice(&self.car_type.to_be_bytes());
        file.extend_from_slice(&sum.to_be_bytes());
        file.extend_from_slice(&self.extra);
        file.extend_from_slice(&data);
        file
    }
}

/// A CAR file with the signature, a type, a checksum and random data (REQ-605). Most files
/// have a supported type, the size of the type and a checksum that agrees.
fn car_file() -> impl Strategy<Value = CarFile> {
    let car_type = prop_oneof![
        8 => (0..CAR_TYPES.len()).prop_map(|index| CAR_TYPES[index]),
        1 => any::<u32>().prop_map(|car_type| (car_type, 0x2000)),
    ];
    (
        car_type,
        prop_oneof![8 => Just(None), 1 => (0..=MAX_LEN).prop_map(Some)],
        prop_oneof![8 => Just(true), 1 => Just(false)],
        any::<[u8; 4]>(),
        random_bytes(),
    )
        .prop_map(
            |((car_type, size), random_size, good_sum, extra, data)| CarFile {
                car_type,
                size: random_size.unwrap_or(size),
                good_sum,
                extra,
                data,
            },
        )
}

/// Loads `file` with `Cartridge::from_raw` and `Cartridge::from_car`, and runs a machine
/// with each cartridge that loads. Returns `true` if a parser accepted the file.
fn use_cartridge(roms: &Roms, file: &[u8]) -> bool {
    let cartridges = [
        Cartridge::from_raw(file.to_vec()),
        Cartridge::from_car(file),
    ];
    let mut accepted = false;
    for cartridge in cartridges.into_iter().flatten() {
        accepted = true;
        let mut machine = Roms::machine(MachineConfig {
            cartridge: Some(cartridge),
            ..roms.config(false)
        });
        run(&mut machine);
    }
    accepted
}

/// Verifies REQ-061: `Cartridge::from_raw` and `Cartridge::from_car` each get at least 1000
/// random inputs of 0 to 4096 bytes and at least 1000 inputs that start with a valid header
/// of the format and continue with random bytes (criteria 1, 2). Each call returns and no
/// call panics, and for each input that is accepted a machine that uses the result runs 10
/// frames and does not panic (criterion 3).
#[test]
fn cartridge_parsers_never_panic() {
    let roms = Roms::read().unwrap();
    let _random = check("random bytes", &random_bytes(), |file| {
        use_cartridge(&roms, &file)
    });
    let raw = check("raw cartridge", &raw_cartridge(), |file| {
        use_cartridge(&roms, &file)
    });
    assert_eq!(raw, CASES, "each raw cartridge of a valid size loads");
    let car = check("CAR file", &car_file(), |file| {
        use_cartridge(&roms, &file.bytes())
    });
    assert!(car > CASES / 2, "most CAR files load: {car}");
}

// ---------------------------------------------------------------------------------------
// ATR disk images (REQ-625)
// ---------------------------------------------------------------------------------------

/// The fields of an ATR file with a valid header (REQ-625).
#[derive(Debug, Clone)]
struct AtrFile {
    /// The sector size field: 128, 256 or a random value.
    sector_size: u16,
    /// The size field in 16-byte units: from the sector count, or random.
    units: u32,
    /// The header bytes 7-15.
    extra: [u8; 9],
    /// The sector data and the bytes after it.
    body: Vec<u8>,
    /// The SIOV shortcut (REQ-623) is on in the machine.
    fast_sio: bool,
}

impl AtrFile {
    /// The bytes of the file.
    fn bytes(&self) -> Vec<u8> {
        let [units_lo, units_mid, units_hi, _] = self.units.to_le_bytes();
        let mut file = vec![0x96, 0x02, units_lo, units_mid];
        file.extend_from_slice(&self.sector_size.to_le_bytes());
        file.push(units_hi);
        file.extend_from_slice(&self.extra);
        file.extend_from_slice(&self.body);
        file
    }
}

/// The data size in bytes of `sectors` sectors of `sector_size` bytes. With 256-byte sectors,
/// `full_boot` gives the boot sectors 1-3 256 bytes, else 128 bytes (REQ-625).
fn atr_size(sectors: usize, sector_size: u16, full_boot: bool) -> usize {
    if sector_size == 256 && !full_boot {
        sectors.min(3) * 128 + sectors.saturating_sub(3) * 256
    } else {
        sectors * usize::from(sector_size)
    }
}

/// An ATR file with the signature, a sector size, a size field and random data (REQ-625).
/// Most files have a size field that agrees with the sector count and data of that size.
fn atr_file() -> impl Strategy<Value = AtrFile> {
    let sector_size = prop_oneof![4 => Just(128_u16), 4 => Just(256), 1 => any::<u16>()];
    (
        sector_size,
        1..=16_usize,
        any::<bool>(),
        prop_oneof![8 => Just(None), 1 => (0..=0xFF_FFFF_u32).prop_map(Some)],
        any::<[u8; 9]>(),
        vec(any::<u8>(), MAX_LEN),
        vec(any::<u8>(), 0..=64),
        any::<bool>(),
    )
        .prop_map(
            |(sector_size, sectors, full_boot, random_units, extra, data, tail, fast_sio)| {
                let size = atr_size(sectors, sector_size, full_boot).min(data.len());
                let mut body = data[..size].to_vec();
                body.extend_from_slice(&tail);
                let units =
                    random_units.unwrap_or_else(|| u32::try_from(size / 16).unwrap_or(u32::MAX));
                AtrFile {
                    sector_size,
                    units,
                    extra,
                    body,
                    fast_sio,
                }
            },
        )
}

/// Loads `file` with `DiskImage::from_atr`, and runs a machine with the disk in D1: if it
/// loads. Returns `true` if the parser accepted the file.
fn use_atr(roms: &Roms, file: &[u8], fast_sio: bool) -> bool {
    let Ok(disk) = DiskImage::from_atr(file) else {
        return false;
    };
    let mut machine = Roms::machine(MachineConfig {
        disk: Some(disk),
        ..roms.config(fast_sio)
    });
    run(&mut machine);
    true
}

/// Verifies REQ-061: `Disk::from_atr` gets at least 1000 random inputs of 0 to 4096 bytes and
/// at least 1000 inputs that start with a valid ATR header and continue with random bytes
/// (criteria 1, 2). Each call returns and no call panics, and for each input that is accepted
/// a machine that uses the disk runs 10 frames and does not panic (criterion 3).
#[test]
fn atr_parser_never_panics() {
    let roms = Roms::read().unwrap();
    let _random = check("random bytes", &random_bytes(), |file| {
        use_atr(&roms, &file, false)
    });
    let atr = check("ATR file", &atr_file(), |file| {
        use_atr(&roms, &file.bytes(), file.fast_sio)
    });
    assert!(atr > CASES / 2, "most ATR files load: {atr}");
}

// ---------------------------------------------------------------------------------------
// CAS tape images (REQ-639)
// ---------------------------------------------------------------------------------------

/// The chunk types of a CAS image (REQ-639).
const CAS_KINDS: [[u8; 4]; 4] = [*b"FUJI", *b"baud", *b"data", *b"fsk "];

/// One chunk of a CAS image: the type, the aux value and the data.
type CasChunk = ([u8; 4], u16, Vec<u8>);

/// The fields of a CAS image with a valid header (REQ-639).
#[derive(Debug, Clone)]
struct CasFile {
    /// The chunks after the signature chunk. The first chunk is the `FUJI` chunk with its
    /// description.
    chunks: Vec<CasChunk>,
    /// The bytes cut from the end of the file.
    cut: usize,
    /// The tape shortcut (REQ-644) is on in the machine.
    fast_tape: bool,
}

impl CasFile {
    /// The bytes of the file.
    fn bytes(&self) -> Vec<u8> {
        let mut file = Vec::new();
        for (kind, aux, data) in &self.chunks {
            file.extend_from_slice(kind);
            file.extend_from_slice(&u16::try_from(data.len()).unwrap_or(u16::MAX).to_le_bytes());
            file.extend_from_slice(&aux.to_le_bytes());
            file.extend_from_slice(data);
        }
        file.truncate(file.len().saturating_sub(self.cut));
        file
    }
}

/// One CAS chunk: mostly a supported type, sometimes a random type. The aux value is a
/// gap, a baud rate or a random value. Most `fsk ` chunks have an even data length.
fn cas_chunk() -> impl Strategy<Value = CasChunk> {
    let kind = prop_oneof![
        31 => (0..CAS_KINDS.len()).prop_map(|index| CAS_KINDS[index]),
        1 => any::<[u8; 4]>(),
    ];
    let aux = prop_oneof![Just(300_u16), Just(600), any::<u16>()];
    let odd = prop_oneof![15 => Just(false), 1 => Just(true)];
    (kind, aux, vec(any::<u8>(), 0..=256), odd).prop_map(|(kind, aux, mut data, odd)| {
        if &kind == b"fsk " && !odd {
            data.truncate(data.len() & !1);
        }
        (kind, aux, data)
    })
}

/// A CAS image: the `FUJI` signature chunk with a random description, then random chunks.
/// Some files are cut at the end.
fn cas_file() -> impl Strategy<Value = CasFile> {
    (
        vec(any::<u8>(), 0..=64),
        vec(cas_chunk(), 0..=16),
        prop_oneof![8 => Just(0_usize), 1 => 1..=8_usize],
        any::<bool>(),
    )
        .prop_map(|(description, mut chunks, cut, fast_tape)| {
            chunks.insert(0, (*b"FUJI", 0, description));
            CasFile {
                chunks,
                cut,
                fast_tape,
            }
        })
}

/// Loads `file` with `Tape::from_cas`, and runs a machine that boots from the tape if it
/// loads. Returns `true` if the parser accepted the file.
fn use_cas(roms: &Roms, file: &[u8], fast_tape: bool) -> bool {
    let Ok(tape) = Tape::from_cas(file) else {
        return false;
    };
    let mut machine = Roms::machine(roms.config(fast_tape));
    machine.start_tape(tape);
    run(&mut machine);
    true
}

/// Verifies REQ-061: `Tape::from_cas` gets at least 1000 random inputs of 0 to 4096 bytes and
/// at least 1000 inputs that start with a valid CAS header and continue with random bytes
/// (criteria 1, 2). Each call returns and no call panics, and for each input that is accepted
/// a machine that uses the tape runs 10 frames and does not panic (criterion 3).
#[test]
fn cas_parser_never_panics() {
    let roms = Roms::read().unwrap();
    let _random = check("random bytes", &random_bytes(), |file| {
        use_cas(&roms, &file, false)
    });
    let cas = check("CAS file", &cas_file(), |file| {
        use_cas(&roms, &file.bytes(), file.fast_tape)
    });
    assert!(cas > CASES / 2, "most CAS files load: {cas}");
}

// ---------------------------------------------------------------------------------------
// XEX executables (REQ-612)
// ---------------------------------------------------------------------------------------

/// One segment of an executable: a `$FF $FF` marker before it, the start address and the
/// data.
type XexSegment = (bool, u16, Vec<u8>);

/// The fields of an executable with a valid header (REQ-612).
#[derive(Debug, Clone)]
struct XexFile {
    /// The segments. The first one has the `$FF $FF` signature.
    segments: Vec<XexSegment>,
    /// Random bytes after the last segment.
    tail: Vec<u8>,
    /// The SIOV shortcut (REQ-623) is on in the machine.
    fast_sio: bool,
}

impl XexFile {
    /// The bytes of the file. A segment ends at `$FFFF` at the latest.
    fn bytes(&self) -> Vec<u8> {
        let mut file = vec![0xFF, 0xFF];
        for (marker, start, data) in &self.segments {
            if *marker {
                file.extend_from_slice(&[0xFF, 0xFF]);
            }
            let room = usize::from(u16::MAX - start) + 1;
            let data = &data[..data.len().min(room)];
            let end = *start + (u16::try_from(data.len()).unwrap_or(1) - 1);
            file.extend_from_slice(&start.to_le_bytes());
            file.extend_from_slice(&end.to_le_bytes());
            file.extend_from_slice(data);
        }
        file.extend_from_slice(&self.tail);
        file
    }
}

/// One segment: random data at a random address, at `RUNAD` or `INITAD`, or in RAM.
fn xex_segment() -> impl Strategy<Value = XexSegment> {
    let start = prop_oneof![
        any::<u16>(),
        Just(0x02E0_u16),
        Just(0x02E2),
        0x2000..0xC000_u16
    ];
    (any::<bool>(), start, vec(any::<u8>(), 1..=512))
}

/// An executable: the `$FF $FF` signature, 1 to 8 segments with valid headers, and random
/// bytes after them.
fn xex_file() -> impl Strategy<Value = XexFile> {
    (
        vec(xex_segment(), 1..=8),
        vec(any::<u8>(), 0..=64),
        any::<bool>(),
    )
        .prop_map(|(segments, tail, fast_sio)| XexFile {
            segments,
            tail,
            fast_sio,
        })
}

/// Parses `file` with `Xex::parse`, and runs a machine that loads the program if it parses.
/// Returns `true` if the parser accepted the file.
fn use_xex(roms: &Roms, file: &[u8], fast_sio: bool) -> bool {
    let Ok(xex) = Xex::parse(file) else {
        return false;
    };
    let mut machine = Roms::machine(roms.config(fast_sio));
    machine.start_xex(&xex);
    run(&mut machine);
    true
}

/// Verifies REQ-061: `Xex::parse` gets at least 1000 random inputs of 0 to 4096 bytes and at
/// least 1000 inputs that start with a valid XEX header and continue with random bytes
/// (criteria 1, 2). Each call returns and no call panics, and for each input that is accepted
/// a machine that loads the program runs 10 frames and does not panic (criterion 3).
#[test]
fn xex_parser_never_panics() {
    let roms = Roms::read().unwrap();
    let _random = check("random bytes", &random_bytes(), |file| {
        use_xex(&roms, &file, false)
    });
    let xex = check("XEX file", &xex_file(), |file| {
        use_xex(&roms, &file.bytes(), file.fast_sio)
    });
    assert_eq!(xex, CASES, "each executable with a complete segment loads");
}

// ---------------------------------------------------------------------------------------
// ROMs (`Machine::new`)
// ---------------------------------------------------------------------------------------

/// The ROM bytes of a [`MachineConfig`].
#[derive(Debug, Clone)]
struct RomSet {
    /// The OS ROM.
    os: Vec<u8>,
    /// The BASIC ROM, if any.
    basic: Option<Vec<u8>>,
    /// The TV standard.
    standard: VideoStandard,
}

/// The TV standards.
fn standard() -> impl Strategy<Value = VideoStandard> {
    prop_oneof![Just(VideoStandard::Ntsc), Just(VideoStandard::Pal)]
}

/// Random ROM bytes of 0 to 4096 bytes.
fn random_roms() -> impl Strategy<Value = RomSet> {
    (random_bytes(), option::of(random_bytes()), standard()).prop_map(|(os, basic, standard)| {
        RomSet {
            os,
            basic,
            standard,
        }
    })
}

/// One ROM: the first bytes of the free ROM `rom`, then random bytes repeated to the ROM
/// size. Some ROMs have a random size.
fn rom(rom: Vec<u8>) -> impl Strategy<Value = Vec<u8>> {
    let size = rom.len();
    (
        0..=size,
        random_bytes(),
        prop_oneof![8 => Just(None), 1 => (0..=size + 16).prop_map(Some)],
    )
        .prop_map(move |(kept, random, random_size)| {
            let len = random_size.unwrap_or(size);
            let mut bytes = rom[..kept.min(len)].to_vec();
            bytes.extend(fill(&random, len - bytes.len()));
            bytes
        })
}

/// ROMs that start with the content of the free ROMs and continue with random bytes.
fn valid_roms(roms: &Roms) -> impl Strategy<Value = RomSet> {
    (
        rom(roms.os.clone()),
        option::of(rom(roms.basic.clone())),
        standard(),
    )
        .prop_map(|(os, basic, standard)| RomSet {
            os,
            basic,
            standard,
        })
}

/// Makes a machine with `set` and runs it if `Machine::new` accepts the ROMs. Returns `true`
/// if it accepted them.
fn use_roms(set: RomSet) -> bool {
    let Ok(mut machine) = Machine::new(MachineConfig {
        os_rom: set.os,
        basic_rom: set.basic,
        cartridge: None,
        disk: None,
        fast_sio: false,
        fast_tape: false,
        tape: None,
        standard: set.standard,
        audio_sample_rate: TEST_SAMPLE_RATE,
    }) else {
        return false;
    };
    run(&mut machine);
    true
}

/// Verifies REQ-061: `Machine::new` gets at least 1000 random ROM byte sets in
/// `MachineConfig` of 0 to 4096 bytes and at least 1000 that start with valid ROM content and
/// continue with random bytes (criteria 1, 2). Each call returns and no call panics, and each
/// machine that is made runs 10 frames and does not panic (criterion 3).
#[test]
fn rom_check_never_panics() {
    let roms = Roms::read().unwrap();
    assert_eq!(roms.os.len(), OS_ROM_SIZE, "OS ROM size");
    assert_eq!(roms.basic.len(), BASIC_ROM_SIZE, "BASIC ROM size");
    let _random = check("random ROMs", &random_roms(), use_roms);
    let valid = check("valid ROMs", &valid_roms(&roms), use_roms);
    assert!(valid > CASES / 2, "most ROM sets make a machine: {valid}");
}

// ---------------------------------------------------------------------------------------
// Save states (REQ-018)
// ---------------------------------------------------------------------------------------

/// The 64-bit FNV-1a hash of `bytes`: the checksum of a save state (REQ-018,
/// `docs/design/architecture.md`, "Save states").
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xCBF2_9CE4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3)
    })
}

/// `body` with its save state checksum at the end.
fn with_sum(mut body: Vec<u8>) -> Vec<u8> {
    let sum = fnv1a(&body);
    body.extend_from_slice(&sum.to_le_bytes());
    body
}

/// The offset of the bytes of the byte array `bytes` in `state`: the first place where its
/// length as a `u64` and its bytes are.
fn array_at(state: &[u8], bytes: &[u8]) -> Option<usize> {
    let mut field = u64::try_from(bytes.len()).ok()?.to_le_bytes().to_vec();
    field.extend_from_slice(bytes);
    state
        .windows(field.len())
        .position(|window| window == field)
        .map(|at| at + 8)
}

/// The machine of a [`StateBase`].
#[derive(Debug, Clone, Copy)]
enum Setup {
    /// A disk and a tape inserted, after 20 frames (the OS has not used them yet).
    Media,
    /// A disk and a tape inserted and a cold reset, after 120 frames: the OS boots from the
    /// disk.
    DiskBoot,
    /// A tape boot (`start_tape`), after 150 frames: the OS reads the tape.
    TapeBoot,
}

/// A CAS image of four data records of 132 bytes.
fn boot_tape() -> Vec<u8> {
    let mut cas = b"FUJI\x04\x00\x00\x00test".to_vec();
    for _ in 0..4 {
        cas.extend_from_slice(b"data\x84\x00\x10\x00\x55\x55\xFC");
        cas.extend_from_slice(&[0x11; 0x81]);
    }
    cas
}

/// A machine and a save state of it, to change.
struct StateBase {
    /// A new machine with the free ROMs, which loads the states.
    machine: Machine,
    /// The state of a machine with a disk and a tape, during the boot.
    state: Vec<u8>,
    /// The positions in `state` after the header and before the checksum, without the
    /// bytes of the RAM and of the frame buffers. The changes go to these positions, so that
    /// most of them change a register, a counter, a length or an enum value.
    positions: Vec<usize>,
}

impl StateBase {
    /// The machine, and the state of a machine of `setup` (the audio samples drained).
    #[expect(
        clippy::unwrap_used,
        reason = "the tapes are valid CAS images, and the state has the RAM and the frame"
    )]
    fn new(roms: &Roms, setup: Setup) -> Self {
        let machine = Roms::machine(roms.config(false));
        let mut saved = machine.clone();
        let disk = DiskImage::blank(Geometry {
            sectors: 8,
            sector_size: 128,
        });
        let frames = match setup {
            Setup::Media => {
                let mut cas = b"FUJI\x04\x00\x00\x00test".to_vec();
                cas.extend_from_slice(b"data\x04\x00\x10\x00\x55\x55\xFC\x00");
                saved.insert_disk(disk);
                saved.insert_tape(Tape::from_cas(&cas).unwrap());
                20
            }
            Setup::DiskBoot => {
                saved.insert_disk(disk);
                saved.insert_tape(Tape::from_cas(&boot_tape()).unwrap());
                saved.power_on();
                120
            }
            Setup::TapeBoot => {
                saved.start_tape(Tape::from_cas(&boot_tape()).unwrap());
                150
            }
        };
        let mut audio = Vec::new();
        for _ in 0..frames {
            saved.run_frame();
            saved.drain_audio(&mut audio);
        }
        let state = saved.save_state();
        // A byte array is its length as a `u64`, then its bytes. The RAM, the GTIA frame
        // buffer `back` and the frame buffer `front` (the last frame) are such arrays, and
        // `back` comes just before `front`. Their bytes are not in the positions; their
        // length fields are.
        let ram: Vec<u8> = (0..=u16::MAX).map(|addr| saved.peek_ram(addr)).collect();
        let frame = saved.frame();
        let ram_at = array_at(&state, &ram).unwrap();
        // The first match is `back` if `front` follows it (both frames can be the same).
        let first = array_at(&state, frame).unwrap();
        let marker = u64::try_from(frame.len()).unwrap().to_le_bytes();
        let back_at = if state.get(first + frame.len()..first + frame.len() + 8) == Some(&marker) {
            first
        } else {
            first - 8 - frame.len()
        };
        let front_at = back_at + frame.len() + 8;
        let skipped = [
            ram_at..ram_at + ram.len(),
            back_at..back_at + frame.len(),
            front_at..front_at + frame.len(),
        ];
        let positions: Vec<usize> = (STATE_HEADER..state.len() - STATE_SUM)
            .filter(|position| !skipped.iter().any(|range| range.contains(position)))
            .collect();
        Self {
            machine,
            state,
            positions,
        }
    }

    /// The state with the bytes of `changes` (a position index in `positions` and a value)
    /// changed, and the checksum made again.
    fn changed(&self, changes: &[(usize, u8)]) -> Vec<u8> {
        let mut body = self.state[..self.state.len() - STATE_SUM].to_vec();
        for &(index, value) in changes {
            body[self.positions[index % self.positions.len()]] = value;
        }
        with_sum(body)
    }

    /// The state with the 8-byte values of `changes` (a position index in `positions` and a
    /// value) written little-endian from the position (cut at the checksum), and the
    /// checksum made again. A value of 8 bytes changes a whole counter or length.
    fn changed_words(&self, changes: &[(usize, u64)]) -> Vec<u8> {
        let mut body = self.state[..self.state.len() - STATE_SUM].to_vec();
        for &(index, value) in changes {
            let position = self.positions[index % self.positions.len()];
            for (byte, new) in body[position..].iter_mut().zip(value.to_le_bytes()) {
                *byte = new;
            }
        }
        with_sum(body)
    }

    /// The state with the bytes at the offsets of `changes` (an offset in the state and a
    /// value) changed, and the checksum made again.
    fn changed_at(&self, changes: &[(usize, u8)]) -> Vec<u8> {
        let mut body = self.state[..self.state.len() - STATE_SUM].to_vec();
        for &(offset, value) in changes {
            body[offset] = value;
        }
        with_sum(body)
    }

    /// Loads `bytes` in a copy of the machine, and runs the copy if the load succeeds.
    /// Returns `true` if `Machine::load_state` accepted the bytes.
    fn load(&self, bytes: &[u8]) -> bool {
        let mut machine = self.machine.clone();
        if machine.load_state(bytes).is_err() {
            return false;
        }
        run(&mut machine);
        true
    }
}

/// Verifies REQ-061: `Machine::load_state` gets at least 1000 random inputs of 0 to 4096
/// bytes and at least 1000 inputs that start with a valid save state header and continue
/// with random bytes (criteria 1, 2). Each call returns and no call panics, and each machine
/// that accepts a state runs 10 frames and does not panic (criterion 3).
///
/// The inputs with a valid header have the checksum of their bytes at the end, so that the
/// load reads the random bytes. Two more strategies change 1 to 8 bytes, or 1 to 4 values of
/// 8 bytes, after the header of a real state (not the RAM and frame buffer bytes) and make
/// the checksum again, so that the load reads each part of the state. The real state is
/// one of three: media inserted, a disk boot, and a tape boot ([`Setup`]).
#[test]
fn load_state_never_panics() {
    let roms = Roms::read().unwrap();
    let base = StateBase::new(&roms, Setup::Media);
    let _random = check("random bytes", &random_bytes(), |bytes| base.load(&bytes));
    let header = base.state[..STATE_HEADER].to_vec();
    let _header = check("valid header", &random_bytes(), |random| {
        let mut body = header.clone();
        body.extend_from_slice(&random);
        base.load(&with_sum(body))
    });
    let bases = [
        base,
        StateBase::new(&roms, Setup::DiskBoot),
        StateBase::new(&roms, Setup::TapeBoot),
    ];
    let edits = (0..bases.len(), vec((any::<usize>(), any::<u8>()), 1..=8));
    let changed = check("changed state", &edits, |(index, changes)| {
        bases[index].load(&bases[index].changed(&changes))
    });
    assert!(changed > CASES / 4, "some changed states load: {changed}");
    let word = prop_oneof![Just(0_u64), Just(u64::MAX), 0..1_000_u64, any::<u64>()];
    let word_edits = (0..bases.len(), vec((any::<usize>(), word), 1..=4));
    let words = check("changed words", &word_edits, |(index, changes)| {
        bases[index].load(&bases[index].changed_words(&changes))
    });
    assert!(
        words > CASES / 4,
        "some states with changed words load: {words}"
    );
}

/// Verifies REQ-061: a state with a resampler phase above the clock rate is an error, not an
/// overflow in the audio output (a panic that `load_state_never_panics` found: the change
/// is the minimal failing input of `proptest`, one byte of the phase).
#[test]
fn state_with_bad_audio_phase_is_an_error() {
    let roms = Roms::read().unwrap();
    let base = StateBase::new(&roms, Setup::Media);
    let bytes = base.changed_at(&[(250_909, 57)]);
    let mut machine = base.machine.clone();
    assert_eq!(
        machine.load_state(&bytes),
        Err(StateError::Invalid),
        "a damaged audio phase"
    );
    assert!(
        base.load(&base.state),
        "the state that the change damaged loads"
    );
}

/// Verifies REQ-061: a state with an ANTIC cycle counter outside the scan line is an error,
/// not a frame that runs for 2^31 cycles and then overflows in GTIA (a panic that a longer
/// run of the changed state strategy found).
#[test]
fn state_with_bad_antic_cycle_is_an_error() {
    let roms = Roms::read().unwrap();
    let base = StateBase::new(&roms, Setup::Media);
    let bytes = base.changed_at(&[(65_611, 0xE7), (137_239, 0xEB)]);
    let mut machine = base.machine;
    assert_eq!(
        machine.load_state(&bytes),
        Err(StateError::Invalid),
        "a damaged ANTIC cycle"
    );
}

/// 8 bytes `$FF` from `offset`: a counter of 8 bytes at its largest value. The offsets of
/// the tests are offsets in the state of [`StateBase`].
fn max_word(offset: usize) -> Vec<(usize, u8)> {
    (offset..offset + 8).map(|at| (at, 0xFF)).collect()
}

/// Verifies REQ-061: a state with the largest ANTIC cycle count is an error, not an
/// overflow in the next cycle (a panic that a run of the changed words strategy found).
#[test]
fn state_with_bad_antic_count_is_an_error() {
    let roms = Roms::read().unwrap();
    let base = StateBase::new(&roms, Setup::Media);
    let bytes = base.changed_at(&max_word(65_621));
    let mut machine = base.machine;
    assert_eq!(
        machine.load_state(&bytes),
        Err(StateError::Invalid),
        "a damaged ANTIC cycle count"
    );
}

/// Verifies REQ-061: a state with the largest machine cycle count is an error, not an
/// overflow in `load_state` (a panic that a run of the changed words strategy found).
#[test]
fn state_with_bad_machine_count_is_an_error() {
    let roms = Roms::read().unwrap();
    let base = StateBase::new(&roms, Setup::Media);
    let bytes = base.changed_at(&max_word(252_209));
    let mut machine = base.machine;
    assert_eq!(
        machine.load_state(&bytes),
        Err(StateError::Invalid),
        "a damaged machine cycle count"
    );
}

/// Verifies REQ-061: a state with the largest POKEY initialization exit cycle is an error,
/// not an overflow in the timing of the serial port (a panic that a run of the changed
/// words strategy found).
#[test]
fn state_with_bad_pokey_exit_cycle_is_an_error() {
    let roms = Roms::read().unwrap();
    let base = StateBase::new(&roms, Setup::DiskBoot);
    let bytes = base.changed_at(&max_word(250_650));
    let mut machine = base.machine;
    assert_eq!(
        machine.load_state(&bytes),
        Err(StateError::Invalid),
        "a damaged initialization exit cycle"
    );
}

/// Verifies REQ-061: a state with a serial output bit count above the frame length is an
/// error, not an overflow in the shift register (a panic that a run of the changed bytes
/// strategy found).
#[test]
fn state_with_bad_serial_bit_count_is_an_error() {
    let roms = Roms::read().unwrap();
    let base = StateBase::new(&roms, Setup::DiskBoot);
    let bytes = base.changed_at(&[(250_866, 185)]);
    let mut machine = base.machine;
    assert_eq!(
        machine.load_state(&bytes),
        Err(StateError::Invalid),
        "a damaged serial bit count"
    );
}

/// Verifies REQ-061: a state with a keyboard scan counter above 63 is an error, not an
/// overflow in the key matrix shift (a panic that a run of the changed bytes strategy
/// found).
#[test]
fn state_with_bad_keyboard_counter_is_an_error() {
    let roms = Roms::read().unwrap();
    let base = StateBase::new(&roms, Setup::TapeBoot);
    let bytes = base.changed_at(&[(250_848, 169)]);
    let mut machine = base.machine;
    assert_eq!(
        machine.load_state(&bytes),
        Err(StateError::Invalid),
        "a damaged keyboard scan counter"
    );
}

// ---------------------------------------------------------------------------------------
// Determinism (criterion 4)
// ---------------------------------------------------------------------------------------

/// The first 20 values of `strategy` with a new runner of `config`. A strategy of this
/// file rejects no value, so each tree is made.
fn values<S: Strategy>(strategy: &S, config: Config) -> Vec<S::Value> {
    let mut runner = TestRunner::new(config);
    (0..20)
        .filter_map(|_| strategy.new_tree(&mut runner).ok())
        .map(|tree| tree.current())
        .collect()
}

/// Verifies REQ-061: the random generator has a fixed seed, and the same seed gives the same
/// inputs on each run (criterion 4).
#[test]
fn fuzz_inputs_are_deterministic() {
    let config = config();
    assert_eq!(config.rng_seed, RngSeed::Fixed(SEED), "fixed seed");
    assert!(config.failure_persistence.is_none(), "no persistence file");
    assert!(config.cases >= 1_000, "at least 1000 cases");
    let first = values(&random_bytes(), config.clone());
    assert_eq!(first.len(), 20, "20 inputs");
    assert_eq!(
        first,
        values(&random_bytes(), config.clone()),
        "the same seed gives the same inputs"
    );
    let first_atr: Vec<Vec<u8>> = values(&atr_file(), config.clone())
        .iter()
        .map(AtrFile::bytes)
        .collect();
    let again_atr: Vec<Vec<u8>> = values(&atr_file(), config.clone())
        .iter()
        .map(AtrFile::bytes)
        .collect();
    assert_eq!(
        first_atr, again_atr,
        "the same seed gives the same ATR files"
    );
    let mut other = config;
    other.rng_seed = RngSeed::Fixed(SEED + 1);
    assert_ne!(
        first,
        values(&random_bytes(), other),
        "another seed gives other inputs"
    );
}
