//! Robustness tests: random media files and random programs (REQ-016).
//!
//! The random bytes come from a `SplitMix64` generator with a fixed seed, so each run tests
//! the same inputs.

use atari_core::media::{Cartridge, Tape};
use atari_core::{Machine, MachineConfig, VideoStandard};
use atari_harness::system::TEST_SAMPLE_RATE;
use atari_harness::{HarnessError, data};

/// The seed of the random media files.
const FILE_SEED: u64 = 0x0016_F11E_5EED_0001;

/// The seed of the random programs.
const PROGRAM_SEED: u64 = 0x0016_C0DE_5EED_0002;

/// The number of random byte arrays for each media type (REQ-016 criterion 2).
const FILES: usize = 1_000;

/// The largest random array length in bytes (REQ-016 criterion 2).
const MAX_FILE_LEN: u32 = 70_000;

/// The frames that a random program runs (REQ-016 criterion 3).
const PROGRAM_FRAMES: u32 = 600;

/// A `SplitMix64` random number generator (repeatable, no dependency).
struct Rng(u64);

impl Rng {
    /// The next 64 random bits.
    const fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A random number in `0..=max`.
    fn below_or_eq(&mut self, max: u32) -> usize {
        let value = self.next() % (u64::from(max) + 1);
        usize::try_from(value).unwrap_or(0)
    }

    /// `len` random bytes.
    fn bytes(&mut self, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len + 8);
        while out.len() < len {
            out.extend_from_slice(&self.next().to_le_bytes());
        }
        out.truncate(len);
        out
    }
}

/// A machine with the free ROMs and `cartridge`.
fn machine(cartridge: Option<Cartridge>, standard: VideoStandard) -> Result<Machine, HarnessError> {
    Ok(Machine::new(MachineConfig {
        os_rom: data::os_rom()?,
        basic_rom: Some(data::basic_rom()?),
        cartridge,
        standard,
        audio_sample_rate: TEST_SAMPLE_RATE,
    })?)
}

/// Runs `frames` frames of `machine` and reads its outputs, as the frontend does.
fn run(machine: &mut Machine, frames: u32) {
    let mut audio = Vec::new();
    for _ in 0..frames {
        machine.run_frame();
        machine.drain_audio(&mut audio);
        audio.clear();
        assert!(!machine.frame().is_empty(), "frame buffer");
    }
}

/// One random media file: random bytes, or random bytes after a header start of the format
/// (so that the checks after the signature also run).
fn random_file(rng: &mut Rng, index: usize) -> Vec<u8> {
    let len = rng.below_or_eq(MAX_FILE_LEN);
    let mut file = rng.bytes(len);
    // Each fourth file starts with the CAR signature and a supported CAR type.
    if index % 4 == 1 && file.len() >= 8 {
        file[..4].copy_from_slice(b"CART");
        let car_type = [1_u32, 2, 41, 42][index / 4 % 4];
        file[4..8].copy_from_slice(&car_type.to_be_bytes());
    }
    // Each fourth file has a valid raw cartridge size.
    if index % 4 == 2 {
        file.resize([0x2000, 0x4000][index / 4 % 2], 0);
    }
    // Each fourth file is a CAS image of random chunks with supported types and short
    // lengths, so that the chunk checks after the signature also run.
    if index % 4 == 3 {
        file = random_cas(rng, &file);
    }
    file
}

/// A CAS image: a `FUJI` chunk, then chunks of the supported types (and sometimes an
/// unsupported one) with random lengths, aux values and data bytes from `bytes`. The last
/// chunk can go past the end of the file.
fn random_cas(rng: &mut Rng, bytes: &[u8]) -> Vec<u8> {
    const KINDS: [&[u8; 4]; 5] = [b"FUJI", b"baud", b"data", b"fsk ", b"pwms"];
    let mut file = b"FUJI".to_vec();
    file.extend_from_slice(&[0, 0, 0, 0]);
    let mut rest = bytes;
    for _ in 0..rng.below_or_eq(40) {
        let kind = KINDS[rng.below_or_eq(4)];
        let length = rng.below_or_eq(200).min(rest.len());
        let aux = rng.below_or_eq(3) * 300;
        file.extend_from_slice(kind);
        file.extend_from_slice(&u16::try_from(length).unwrap_or(u16::MAX).to_le_bytes());
        file.extend_from_slice(&u16::try_from(aux).unwrap_or(u16::MAX).to_le_bytes());
        let (data, tail) = rest.split_at(length);
        file.extend_from_slice(data);
        rest = tail;
    }
    // Cut the file at a random length.
    let len = file.len() - rng.below_or_eq(8).min(file.len());
    file.truncate(len);
    file
}

/// With a fixed seed, 1,000 random byte arrays of 0 to 70,000 bytes load as each media type
/// with no panic (criterion 2). Each load returns `Ok` or an `Err` value. A cartridge that
/// loads is also inserted in a machine, which runs 2 frames.
///
/// The media types with a loader now: raw cartridge (`Cartridge::from_raw`), CAR file
/// (`Cartridge::from_car`) and CAS tape image (`Tape::from_cas`, SLICE-08 WP-1). The XEX
/// loader (`Xex::parse`, SLICE-04 WP-1) and the ATR loader (`DiskImage::from_atr`, SLICE-05)
/// join this test when they exist.
///
/// Verifies REQ-016
#[test]
fn random_files_do_not_panic() {
    let mut rng = Rng(FILE_SEED);
    let mut loaded = 0_u32;
    let mut tapes = 0_u32;
    for index in 0..FILES {
        let file = random_file(&mut rng, index);
        if let Ok(tape) = Tape::from_cas(&file) {
            assert!(!tape.blocks().is_empty(), "a tape has at least one block");
            tapes += 1;
        }
        let cartridges = [
            Cartridge::from_raw(file.clone()),
            Cartridge::from_car(&file),
        ];
        for cartridge in cartridges.into_iter().flatten() {
            loaded += 1;
            let mut machine = machine(Some(cartridge), VideoStandard::Ntsc).unwrap();
            run(&mut machine, 2);
        }
    }
    assert!(loaded > 0, "some random raw cartridges load and run");
    assert!(tapes > 0, "some random CAS images load");
}

/// With a fixed seed, random bytes in RAM and `PC` = `$0600` run for 600 frames with no
/// panic (criterion 3). The machine first runs one frame from power-on, so that the CPU reset
/// sequence is over, then gets 65,536 random RAM bytes and `PC` = `$0600` at an instruction
/// boundary. This runs once for NTSC and once for PAL. A random program soon stops at a
/// JAM opcode, so for more instructions, 100 more random programs then run 6 frames each,
/// each after a new power-on.
///
/// Verifies REQ-016
#[test]
fn random_programs_do_not_panic() {
    let mut rng = Rng(PROGRAM_SEED);
    for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
        let mut machine = machine(None, standard).unwrap();
        machine.run_frame();
        random_program(&mut machine, &mut rng);
        run(&mut machine, PROGRAM_FRAMES);
        for addr in 0..=u16::MAX {
            let _value = machine.peek(addr);
        }
        assert_eq!(
            machine.frame_count(),
            u64::from(PROGRAM_FRAMES) + 1,
            "{standard:?}: frames"
        );
    }
    let mut machine = machine(None, VideoStandard::Ntsc).unwrap();
    let mut jammed = 0_u32;
    for _ in 0..100 {
        machine.power_on();
        machine.run_frame();
        random_program(&mut machine, &mut rng);
        run(&mut machine, 6);
        jammed += u32::from(machine.cpu().is_jammed());
    }
    assert!(
        jammed < 100,
        "some random programs run without a JAM opcode for 6 frames"
    );
}

/// Fills all 65,536 RAM bytes of `machine` with random bytes and sets `PC` to `$0600` at the
/// next instruction boundary.
fn random_program(machine: &mut Machine, rng: &mut Rng) {
    while !machine.cpu().at_boundary() {
        machine.run_cycle();
    }
    let ram = rng.bytes(0x1_0000);
    for (addr, &value) in (0..=u16::MAX).zip(&ram) {
        machine.poke_ram(addr, value);
    }
    machine.cpu_mut().registers_mut().pc = 0x0600;
}
