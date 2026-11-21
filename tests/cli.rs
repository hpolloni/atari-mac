//! End-to-end tests for the `atari-mac` binary.

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use atari_mac::atari_core::cpu::{Boundary, Registers};
use atari_mac::atari_core::debug::{InstructionStart, Position, Sequence, SequenceKind};
use atari_mac::atari_core::gtia::{FRAME_HEIGHT, FRAME_WIDTH};
use atari_mac::atari_core::keyboard::ascii_to_kbcode;
use atari_mac::atari_core::screen::{has_row, screen_text};
use atari_mac::atari_core::{Machine, MachineConfig, VideoStandard, palette};
use atari_mac::bas::{self, KEY_DOWN_FRAMES, KEY_UP_FRAMES, Phase, Typist};
use atari_mac::cli::{self, Options};
use atari_mac::frontend::{Flow, Frontend};
use atari_mac::logging;
use atari_mac::start;
use atari_mac::trace::{InstructionLine, SequenceLine, TraceWriter};
// The binary uses this dependency; this test does not.
use minifb as _;

/// The free OS ROM (fetched by `scripts/fetch-roms.sh`).
const OS_ROM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/roms/altirraos-xl.rom");
/// The free BASIC ROM.
const BASIC_ROM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/roms/altirra-basic.rom");

/// A new, empty directory for one test.
fn temp_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("remove the old test directory");
    }
    fs::create_dir_all(&dir).expect("create the test directory");
    dir
}

/// Runs the binary with the free ROMs and `args`.
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_atari-mac"))
        .args(["--os", OS_ROM, "--basic", BASIC_ROM])
        .args(args)
        .output()
        .expect("failed to run the atari-mac binary")
}

/// Standard error of a run.
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// `--headless --frames 180 --screenshot` writes a 384 x 240 RGB PNG file whose pixels are
/// `palette::rgb` of the frame buffer after 180 frames from power-on.
///
/// Verifies REQ-013
#[test]
fn headless_writes_screenshot() {
    let png_path = temp_dir("headless_writes_screenshot").join("out.png");
    let png_arg = png_path.to_str().expect("UTF-8 path");
    let output = run(&["--headless", "--frames", "180", "--screenshot", png_arg]);
    assert!(
        output.status.success(),
        "exit status {}: {}",
        output.status,
        stderr(&output)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).starts_with("180 frames in "),
        "elapsed time on standard output"
    );

    let decoder = png::Decoder::new(BufReader::new(File::open(&png_path).expect("PNG file")));
    let mut reader = decoder.read_info().expect("PNG header");
    let mut pixels = vec![0; reader.output_buffer_size().expect("PNG size")];
    let info = reader.next_frame(&mut pixels).expect("PNG data");
    assert_eq!((info.width, info.height), (384, 240), "PNG size in pixels");
    assert_eq!(info.color_type, png::ColorType::Rgb, "RGB PNG");
    assert_eq!(info.bit_depth, png::BitDepth::Eight, "8 bits per channel");

    let mut machine = Machine::new(MachineConfig {
        os_rom: fs::read(OS_ROM).expect("OS ROM"),
        basic_rom: Some(fs::read(BASIC_ROM).expect("BASIC ROM")),
        cartridge: None,
        disk: None,
        fast_sio: false,
        fast_tape: false,
        tape: None,
        standard: VideoStandard::Ntsc,
        audio_sample_rate: 48_000,
    })
    .expect("machine");
    for _ in 0..180 {
        machine.run_frame();
    }
    let frame = machine.frame();
    assert_eq!(frame.len(), FRAME_WIDTH * FRAME_HEIGHT, "frame size");
    for (i, (&color, rgb)) in frame.iter().zip(pixels.as_chunks::<3>().0).enumerate() {
        let [_, r, g, b] = palette::rgb(VideoStandard::Ntsc, color).to_be_bytes();
        assert_eq!(
            *rgb,
            [r, g, b],
            "pixel ({}, {}) of color byte ${color:02X}",
            i % FRAME_WIDTH,
            i / FRAME_WIDTH
        );
    }
}

/// `--os` with a file of 100 bytes writes a message with `16384` to standard error and
/// exits with status 2.
///
/// Verifies REQ-004
#[test]
fn rejects_bad_os_rom() {
    let dir = temp_dir("rejects_bad_os_rom");
    let os = dir.join("small-os.rom");
    fs::write(&os, [0_u8; 100]).expect("write the small ROM");
    let basic = dir.join("big-basic.rom");
    fs::write(&basic, [0_u8; 8193]).expect("write the big ROM");
    for (option, file, size) in [("--os", &os, "16384"), ("--basic", &basic, "8192")] {
        let output = run(&[
            option,
            file.to_str().expect("UTF-8 path"),
            "--headless",
            "--frames",
            "1",
        ]);
        let message = stderr(&output);
        assert_eq!(output.status.code(), Some(2), "{option}: exit status");
        assert!(message.contains(size), "{option}: {message}");
        assert!(
            message.contains(file.file_name().and_then(|n| n.to_str()).expect("name")),
            "{option}: the message names the file: {message}"
        );
    }
}

/// `--cart` with an 8 KB and a 16 KB raw cartridge image exits with status 0.
///
/// REQ-012 is verified when `--xex` and `--disk` work too; this test does not name it yet.
#[test]
fn loads_media() {
    let dir = temp_dir("loads_media");
    for size in [0x2000, 0x4000] {
        let cart = dir.join(format!("cart-{size}.rom"));
        fs::write(&cart, vec![0_u8; size]).expect("write the cartridge");
        let output = run(&[
            "--headless",
            "--frames",
            "1",
            "--cart",
            cart.to_str().expect("path"),
        ]);
        assert!(
            output.status.success(),
            "{size}-byte cartridge: {}",
            stderr(&output)
        );
    }
}

/// A 100-byte cartridge gives a message that names the file and exit status 2. A missing
/// `--xex` or `--disk` file gives a message that names the file and exit status 2.
#[test]
fn rejects_bad_media() {
    let bad = temp_dir("rejects_bad_media").join("bad.car");
    fs::write(&bad, [0_u8; 100]).expect("write the bad cartridge");
    let output = run(&[
        "--headless",
        "--frames",
        "1",
        "--cart",
        bad.to_str().expect("path"),
    ]);
    assert_eq!(output.status.code(), Some(2), "100-byte cartridge");
    assert!(stderr(&output).contains("bad.car"), "{}", stderr(&output));
    let missing = temp_dir("rejects_bad_media").join("missing.xex");
    let output = run(&[
        "--headless",
        "--frames",
        "1",
        "--xex",
        missing.to_str().expect("path"),
    ]);
    assert_eq!(output.status.code(), Some(2), "--xex with a missing file");
    assert!(
        stderr(&output).contains("missing.xex"),
        "--xex: {}",
        stderr(&output)
    );
    let missing = temp_dir("rejects_bad_media").join("missing.atr");
    let output = run(&[
        "--headless",
        "--frames",
        "1",
        "--disk",
        missing.to_str().expect("path"),
    ]);
    assert_eq!(output.status.code(), Some(2), "--disk with a missing file");
    assert!(
        stderr(&output).contains("missing.atr"),
        "--disk: {}",
        stderr(&output)
    );
}

#[test]
fn usage_errors_and_help() {
    let output = run(&["--frames"]);
    assert_eq!(output.status.code(), Some(2), "missing value");
    assert!(
        stderr(&output).contains("--frames needs a value"),
        "{}",
        stderr(&output)
    );
    let output = run(&["--help"]);
    assert!(output.status.success(), "--help");
    assert!(
        String::from_utf8_lossy(&output.stdout).starts_with("Usage: atari-mac"),
        "usage text"
    );
    let output = run(&["--headless", "--frames", "1", "--os", "/nonexistent/os.rom"]);
    assert_eq!(output.status.code(), Some(2), "missing file");
    assert!(
        stderr(&output).contains("/nonexistent/os.rom"),
        "{}",
        stderr(&output)
    );
    let output = run(&[
        "--headless",
        "--frames",
        "1",
        "--screenshot",
        "/nonexistent/dir/out.png",
    ]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "screenshot in a missing directory"
    );
}

/// A valid executable: one segment with `JMP $0600` at `$0600`, and `RUNAD` = `$0600`.
const VALID_XEX: [u8; 15] = [
    0xFF, 0xFF, 0x00, 0x06, 0x02, 0x06, 0x4C, 0x00, 0x06, 0xE0, 0x02, 0xE1, 0x02, 0x00, 0x06,
];

/// A valid ATR image: 720 sectors of 128 bytes, all `$00` (single density).
fn valid_atr() -> Vec<u8> {
    let data_len: usize = 720 * 128;
    let paragraphs = u16::try_from(data_len / 16).expect("paragraphs fit in 16 bits");
    let mut atr = vec![0_u8; 16 + data_len];
    atr[..2].copy_from_slice(&[0x96, 0x02]);
    atr[2..4].copy_from_slice(&paragraphs.to_le_bytes());
    atr[4..6].copy_from_slice(&128_u16.to_le_bytes());
    atr
}

/// Verifies REQ-012: `--headless --frames 1` exits with status 0 for a valid 8,192-byte
/// cartridge (`--cart`), a valid executable (`--xex`) and a valid ATR image (`--disk`)
/// (criterion 1).
#[test]
fn loads_cart_xex_and_disk() {
    let dir = temp_dir("loads_cart_xex_and_disk");
    for (option, name, bytes) in [
        ("--cart", "game.rom", vec![0_u8; 0x2000]),
        ("--xex", "game.xex", VALID_XEX.to_vec()),
        ("--disk", "game.atr", valid_atr()),
    ] {
        let file = dir.join(name);
        fs::write(&file, bytes).expect("write the media file");
        let output = run(&[
            "--headless",
            "--frames",
            "1",
            option,
            file.to_str().expect("UTF-8 path"),
        ]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{option}: {}",
            stderr(&output)
        );
    }
}

/// Verifies REQ-012: with a 100-byte file, each of `--cart`, `--xex` and `--disk` writes a
/// message that names the file to standard error and exits with status 2 (criterion 2).
#[test]
fn rejects_bad_cart_xex_and_disk() {
    let dir = temp_dir("rejects_bad_cart_xex_and_disk");
    for (option, name) in [
        ("--cart", "bad-cart.rom"),
        ("--xex", "bad-program.xex"),
        ("--disk", "bad-disk.atr"),
    ] {
        let file = dir.join(name);
        fs::write(&file, [0_u8; 100]).expect("write the 100-byte file");
        let output = run(&[
            "--headless",
            "--frames",
            "1",
            option,
            file.to_str().expect("UTF-8 path"),
        ]);
        let message = stderr(&output);
        assert_eq!(output.status.code(), Some(2), "{option}: exit status");
        assert!(
            message.contains(name),
            "{option}: the message names the file: {message}"
        );
    }
}

/// One CAS chunk: the 8-byte header (`kind`, the data length, `aux`) and `data`.
fn cas_chunk(kind: [u8; 4], aux: u16, data: &[u8]) -> Vec<u8> {
    let mut bytes = kind.to_vec();
    bytes.extend(
        u16::try_from(data.len())
            .expect("chunk length")
            .to_le_bytes(),
    );
    bytes.extend(aux.to_le_bytes());
    bytes.extend(data);
    bytes
}

/// One tape record: `55 55`, the control byte `control` (`$FC` data, `$FE` end of file),
/// the 128 bytes of `data` (`$00` after its end) and the checksum of the 131 bytes before
/// it (REQ-616: the sum with the carry added back).
fn cas_record(control: u8, data: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x55, 0x55, control];
    bytes.extend(data);
    bytes.resize(131, 0);
    let sum = bytes.iter().fold(0_u8, |sum, &byte| {
        let (value, carry) = sum.overflowing_add(byte);
        value + u8::from(carry)
    });
    bytes.push(sum);
    bytes
}

/// Writes the boot test tape of REQ-643 (a CAS file: two boot records and the end-of-file
/// record) in the directory of `test` and gives its path.
fn boot_tape_file(test: &str) -> PathBuf {
    let mut file = cas_chunk(*b"FUJI", 0, b"");
    file.extend(cas_chunk(*b"baud", 600, &[]));
    file.extend(cas_chunk(
        *b"data",
        20_000,
        &cas_record(
            0xFC,
            &[0x00, 0x02, 0x00, 0x07, 0x86, 0x07, 0x4C, 0x80, 0x07],
        ),
    ));
    file.extend(cas_chunk(
        *b"data",
        3_000,
        &cas_record(0xFC, &[0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x18, 0x60]),
    ));
    file.extend(cas_chunk(*b"data", 3_000, &cas_record(0xFE, &[])));
    let path = temp_dir(test).join("boot.cas");
    fs::write(&path, file).expect("write the CAS file");
    path
}

/// The options of a run with the free ROMs and the tape `cas`.
fn cas_options(cas: &Path, standard: VideoStandard) -> Options {
    Options {
        os: OS_ROM.into(),
        basic: BASIC_ROM.into(),
        cas: Some(cas.to_owned()),
        standard,
        ..Options::default()
    }
}

/// Verifies REQ-081: `--headless --frames 1 --cas <file>` exits with status 0 for the boot
/// test tape of REQ-643 (criterion 1).
#[test]
fn cas_loads_tape() {
    let tape = boot_tape_file("cas_loads_tape");
    let output = run(&[
        "--headless",
        "--frames",
        "1",
        "--cas",
        tape.to_str().expect("UTF-8 path"),
    ]);
    assert!(
        output.status.success(),
        "exit status {}: {}",
        output.status,
        stderr(&output)
    );
}

/// Verifies REQ-081: a file of 100 bytes of `$00` gives exit code 2 and a message that names
/// the file and `CasSignature`; a missing file gives exit code 2 and a message that names
/// the file (criterion 2).
#[test]
fn cas_rejects_bad_files() {
    let dir = temp_dir("cas_rejects_bad_files");
    let zeros = dir.join("zeros.cas");
    fs::write(&zeros, [0_u8; 100]).expect("write the 100-byte file");
    let missing = dir.join("missing.cas");
    for (path, name) in [(&zeros, "CasSignature"), (&missing, "missing.cas")] {
        let output = run(&[
            "--headless",
            "--frames",
            "1",
            "--cas",
            path.to_str().expect("UTF-8 path"),
        ]);
        let message = stderr(&output);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}: exit status",
            path.display()
        );
        assert!(
            message.contains(path.to_str().expect("UTF-8 path")),
            "the message names the file: {message}"
        );
        assert!(
            message.contains(name),
            "the message names {name}: {message}"
        );
        assert!(
            output.stdout.is_empty(),
            "the machine does not start: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

/// Verifies REQ-081: with `--cas`, START is held from the power-on cold reset until frame
/// 60; OPTION, RETURN and the other keys are not pressed, so BASIC stays on (criterion 3).
///
/// The test reads `CONSOL` (`$D01F`, bit 0 is 0 while START is down) and `SKSTAT` bit 2 (0
/// while a key is down) after each frame, and at the end the BASIC ROM at `$A000`.
#[test]
fn cas_key_presses() {
    let tape = boot_tape_file("cas_key_presses");
    let mut frontend = start(&cas_options(&tape, VideoStandard::Ntsc)).expect("start");
    assert_eq!(frontend.machine().frame_count(), 0, "the cold reset");
    for frame in 0..300 {
        let consol = frontend.machine().peek(0xD01F) & 0x07;
        let expected = if frame < 60 { 0x06 } else { 0x07 };
        assert_eq!(consol, expected, "CONSOL in frame {frame}");
        assert_ne!(
            frontend.machine().peek(0xD20F) & 0x04,
            0,
            "no key is down in frame {frame}"
        );
        frontend.run_frames(1).expect("run");
    }
    let basic = fs::read(BASIC_ROM).expect("BASIC ROM");
    let visible: Vec<u8> = (0xA000..0xA100)
        .map(|a| frontend.machine().peek(a))
        .collect();
    assert_eq!(visible, basic[..0x100], "BASIC stays on");
}

/// Verifies REQ-081: with the boot test tape, the test presses RETURN in frames 180-186
/// (NTSC) or 150-156 (PAL); then `$0600` holds `$A5` within 2,400 frames (NTSC) and 2,000
/// frames (PAL) after the start (criterion 4).
#[test]
fn cas_boots_tape() {
    let tape = boot_tape_file("cas_boots_tape");
    for (standard, press, limit) in [
        (VideoStandard::Ntsc, 180, 2_400),
        (VideoStandard::Pal, 150, 2_000),
    ] {
        let mut frontend = start(&cas_options(&tape, standard)).expect("start");
        let mut done = None;
        for frame in 0..limit {
            if frame == press {
                frontend
                    .machine_mut()
                    .key_down(ascii_to_kbcode('\n').expect("RETURN"));
            }
            if frame == press + 6 {
                frontend.machine_mut().key_up();
            }
            frontend.run_frames(1).expect("run");
            if frontend.machine().peek_ram(0x0600) == 0xA5 {
                done = Some(frame + 1);
                break;
            }
        }
        assert!(
            done.is_some(),
            "{standard:?}: no $A5 at $0600 after {limit} frames"
        );
    }
}

/// Verifies REQ-081: `--help` lists `--cas <file>` (criterion 5).
#[test]
fn cas_help_lists_option() {
    let output = run(&["--help"]);
    assert!(output.status.success(), "--help exit status");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("--cas <file>"),
        "--help lists --cas <file>"
    );
}

/// Verifies REQ-081: `--cas` with `--bas` gives exit code 2 and a message that names both
/// options (criterion 6).
#[test]
fn cas_with_bas_rejected() {
    let output = run(&[
        "--headless",
        "--frames",
        "1",
        "--cas",
        "a.cas",
        "--bas",
        "a.bas",
    ]);
    let message = stderr(&output);
    assert_eq!(output.status.code(), Some(2), "exit status");
    assert!(
        message.contains("--cas") && message.contains("--bas"),
        "the message names both options: {message}"
    );
}

/// The options of the command line `--cas <file>` with `extra`, with the free ROMs.
fn parse_cas(cas: &Path, extra: &[&str]) -> Options {
    let mut args = vec![
        "--os",
        OS_ROM,
        "--basic",
        BASIC_ROM,
        "--cas",
        cas.to_str().expect("UTF-8 path"),
    ];
    args.extend(extra);
    match cli::parse(args.into_iter().map(OsString::from)) {
        Ok(cli::Command::Run(options)) => *options,
        other => panic!("the command line: {other:?}"),
    }
}

/// Starts the frontend with `options`, presses RETURN in frames 180-186 (NTSC, REQ-081
/// criterion 4) and gives the frontend and the frames until `$0600` holds `$A5`, at most
/// 2,400.
fn cas_boot_frames(options: &Options) -> (Frontend, Option<u64>) {
    let mut frontend = start(options).expect("start");
    for frame in 0..2_400 {
        if frame == 180 {
            frontend
                .machine_mut()
                .key_down(ascii_to_kbcode('\n').expect("RETURN"));
        }
        if frame == 186 {
            frontend.machine_mut().key_up();
        }
        frontend.run_frames(1).expect("run");
        if frontend.machine().peek_ram(0x0600) == 0xA5 {
            return (frontend, Some(frame + 1));
        }
    }
    (frontend, None)
}

/// Verifies REQ-091: without `--no-fast-tape`, the machine of the frontend has the tape
/// shortcut on, and the boot test tape boots with it (criterion 1).
#[test]
fn fast_tape_default_on() {
    let tape = boot_tape_file("fast_tape_default_on");
    let options = parse_cas(&tape, &[]);
    assert!(options.fast_tape, "the option is on");
    let (frontend, done) = cas_boot_frames(&options);
    assert!(frontend.machine().fast_tape(), "1: the shortcut is on");
    assert!(done.is_some(), "1: the tape boots");
}

/// Verifies REQ-091: with `--no-fast-tape`, the machine has the tape shortcut off, and the
/// boot test tape loads on the serial path as in REQ-643: within 2,400 frames, and in more
/// frames than with the shortcut (criterion 2).
#[test]
fn no_fast_tape_option() {
    let tape = boot_tape_file("no_fast_tape_option");
    let (fast, fast_done) = cas_boot_frames(&parse_cas(&tape, &[]));
    let (slow, slow_done) = cas_boot_frames(&parse_cas(&tape, &["--no-fast-tape"]));
    assert!(fast.machine().fast_tape(), "without the option");
    assert!(!slow.machine().fast_tape(), "2: the shortcut is off");
    let (fast_done, slow_done) = (fast_done.unwrap(), slow_done.unwrap());
    assert!(
        fast_done < slow_done,
        "2: with the shortcut {fast_done} frames, without {slow_done} frames"
    );
}

/// Verifies REQ-091: `--help` lists `--no-fast-tape` (criterion 3).
#[test]
fn no_fast_tape_help() {
    let output = run(&["--help"]);
    assert!(output.status.success(), "--help exit status");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("--no-fast-tape"),
        "--help lists --no-fast-tape"
    );
}

/// Runs `--headless --frames 240 --screenshot` with `flags` and returns the PNG pixels as
/// `0x00RRGGBB` values.
fn screenshot_240(test: &str, flags: &[&str]) -> Vec<u32> {
    let png_path = temp_dir(test).join("out.png");
    let png_arg = png_path.to_str().expect("UTF-8 path");
    let mut args = vec!["--headless", "--frames", "240", "--screenshot", png_arg];
    args.extend(flags);
    let output = run(&args);
    assert!(output.status.success(), "{flags:?}: {}", stderr(&output));
    let decoder = png::Decoder::new(BufReader::new(File::open(&png_path).expect("PNG file")));
    let mut reader = decoder.read_info().expect("PNG header");
    let mut pixels = vec![0; reader.output_buffer_size().expect("PNG size")];
    let info = reader.next_frame(&mut pixels).expect("PNG data");
    assert_eq!((info.width, info.height), (384, 240), "PNG size in pixels");
    pixels
        .as_chunks::<3>()
        .0
        .iter()
        .map(|&[r, g, b]| u32::from_be_bytes([0, r, g, b]))
        .collect()
}

/// The frame buffer of a `standard` machine with the free ROMs after 240 frames from
/// power-on.
fn frame_after_240(standard: VideoStandard) -> Vec<u8> {
    let mut machine = Machine::new(MachineConfig {
        os_rom: fs::read(OS_ROM).expect("OS ROM"),
        basic_rom: Some(fs::read(BASIC_ROM).expect("BASIC ROM")),
        cartridge: None,
        disk: None,
        fast_sio: false,
        fast_tape: false,
        tape: None,
        standard,
        audio_sample_rate: 48_000,
    })
    .expect("machine");
    for _ in 0..240 {
        machine.run_frame();
    }
    machine.frame().to_vec()
}

/// Verifies REQ-019: without the flags, the screenshot of `--headless --frames 240
/// --screenshot` has each pixel equal to `palette::rgb` of the frame buffer byte
/// (criterion 1).
#[test]
fn filters_off_without_flags() {
    let pixels = screenshot_240("filters_off_without_flags", &[]);
    let frame = frame_after_240(VideoStandard::Ntsc);
    for (i, (&color, &pixel)) in frame.iter().zip(&pixels).enumerate() {
        assert_eq!(
            pixel,
            palette::rgb(VideoStandard::Ntsc, color),
            "pixel ({}, {})",
            i % FRAME_WIDTH,
            i / FRAME_WIDTH
        );
    }
}

/// Verifies REQ-019: with `--artifacting` on an NTSC machine, the screenshot equals the
/// output of `palette::artifact_ntsc_row` for each row (criterion 2).
#[test]
fn artifacting_flag_filters_screenshot() {
    let pixels = screenshot_240("artifacting_flag_filters_screenshot", &["--artifacting"]);
    let frame = frame_after_240(VideoStandard::Ntsc);
    let mut expected = [0; FRAME_WIDTH];
    for (y, (row, out)) in frame
        .chunks(FRAME_WIDTH)
        .zip(pixels.chunks(FRAME_WIDTH))
        .enumerate()
    {
        palette::artifact_ntsc_row(row, &mut expected);
        assert_eq!(out, expected, "row {y}");
    }
    let plain: Vec<u32> = frame
        .iter()
        .map(|&color| palette::rgb(VideoStandard::Ntsc, color))
        .collect();
    assert_ne!(pixels, plain, "the filter changes the hires text");
}

/// Verifies REQ-019: with `--pal-blending` on a PAL machine (`--pal`), the screenshot equals
/// the output of `palette::pal_blend_row` for each row (criterion 3). Row 0 has no row above
/// it and is not blended (OI-021, REQ-412 criterion 6).
#[test]
fn pal_blending_flag_filters_screenshot() {
    let pixels = screenshot_240(
        "pal_blending_flag_filters_screenshot",
        &["--pal", "--pal-blending"],
    );
    let frame = frame_after_240(VideoStandard::Pal);
    let rows: Vec<&[u8]> = frame.chunks(FRAME_WIDTH).collect();
    let out_rows: Vec<&[u32]> = pixels.chunks(FRAME_WIDTH).collect();
    assert_eq!(out_rows.len(), FRAME_HEIGHT, "rows in the screenshot");
    let plain_row_0: Vec<u32> = rows[0]
        .iter()
        .map(|&color| palette::rgb(VideoStandard::Pal, color))
        .collect();
    assert_eq!(out_rows[0], plain_row_0, "row 0 is not blended");
    let mut expected = [0; FRAME_WIDTH];
    for y in 1..FRAME_HEIGHT {
        palette::pal_blend_row(rows[y - 1], rows[y], &mut expected);
        assert_eq!(out_rows[y], expected, "row {y}");
    }
    let plain: Vec<u32> = frame
        .iter()
        .map(|&color| palette::rgb(VideoStandard::Pal, color))
        .collect();
    assert_ne!(pixels, plain, "the filter changes the screen");
}

/// Verifies REQ-002: `--headless --frames 50 --pal` runs a PAL machine and prints the run
/// time for 50 frames of 35,568 cycles; without `--pal` the frames have the NTSC length of
/// 29,868 cycles; `--help` lists `--pal` (criterion 4).
#[test]
fn pal_flag_selects_pal() {
    for (flags, line) in [
        (&["--pal"][..], " s (PAL, last frame 35568 cycles)"),
        (&[], " s (NTSC, last frame 29868 cycles)"),
    ] {
        let mut args = vec!["--headless", "--frames", "50"];
        args.extend(flags);
        let output = run(&args);
        assert!(output.status.success(), "{flags:?}: {}", stderr(&output));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.starts_with("50 frames in ") && stdout.trim_end().ends_with(line),
            "{flags:?}: {stdout}"
        );
    }
    let output = run(&["--help"]);
    assert!(output.status.success(), "--help");
    let usage = String::from_utf8_lossy(&output.stdout);
    assert!(usage.contains("--pal "), "--pal in {usage}");
    assert!(
        !usage.contains("(NTSC)"),
        "the help does not say NTSC only: {usage}"
    );
}

/// Verifies REQ-019: `--help` lists both flags (criterion 4).
#[test]
fn help_lists_filter_flags() {
    let output = run(&["--help"]);
    assert!(output.status.success(), "--help");
    let usage = String::from_utf8_lossy(&output.stdout);
    for flag in ["--artifacting", "--pal-blending"] {
        assert!(usage.contains(flag), "{flag} in {usage}");
    }
}

/// Verifies REQ-019: `--pal-blending` on an NTSC machine, or `--artifacting` on a PAL machine
/// (`--pal`), gives exit code 2 and a message that names the flag and the video standard
/// (criterion 5).
#[test]
fn filter_flag_needs_matching_standard() {
    for (flags, flag, standard) in [
        (&["--pal-blending"][..], "--pal-blending", "NTSC"),
        (&["--pal", "--artifacting"], "--artifacting", "PAL"),
    ] {
        let mut args = vec!["--headless", "--frames", "1"];
        args.extend(flags);
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{flags:?}: exit status");
        let message = stderr(&output);
        assert!(
            message.contains(flag) && message.contains(&format!("the machine is {standard}")),
            "{flags:?}: {message}"
        );
    }
}

/// Writes the BASIC program file `name` with `contents` in the directory of `test`.
fn write_bas(test: &str, name: &str, contents: &str) -> PathBuf {
    let path = temp_dir(test).join(name);
    fs::write(&path, contents).expect("write the BASIC program file");
    path
}

/// The options of an in-process run with the free ROMs and the program file `bas`.
fn bas_options(bas: &Path, no_run: bool) -> Options {
    Options {
        os: OS_ROM.into(),
        basic: BASIC_ROM.into(),
        bas: Some(bas.to_owned()),
        no_run,
        ..Options::default()
    }
}

/// The screen rows of `machine` without the margin and the trailing spaces.
fn screen_rows(machine: &Machine) -> Vec<String> {
    screen_text(machine)
        .iter()
        .map(|row| row.trim().to_owned())
        .collect()
}

/// Runs `atari-mac --headless --frames <frames> --bas <bas> --screenshot` with `flags`, and
/// returns the screen text rows of the last frame.
///
/// The binary writes only a screenshot, not the screen text. So the same run is made in
/// process (`atari_mac::start`, `Frontend::run_frames`), the screenshot is checked to be
/// the frame of that run, and the rows are the `screen_text` of that run. The emulation is
/// deterministic, so both runs are the same.
fn bas_screen(test: &str, bas: &Path, flags: &[&str], frames: u64) -> Vec<String> {
    let png_path = temp_dir(&format!("{test}-png")).join("out.png");
    let frames_arg = frames.to_string();
    let mut args = vec![
        "--headless",
        "--frames",
        &frames_arg,
        "--bas",
        bas.to_str().expect("UTF-8 path"),
        "--screenshot",
        png_path.to_str().expect("UTF-8 path"),
    ];
    args.extend(flags);
    let output = run(&args);
    assert!(output.status.success(), "{args:?}: {}", stderr(&output));

    let mut frontend = start(&bas_options(bas, flags.contains(&"--no-run"))).expect("start");
    frontend.run_frames(frames).expect("run");
    let decoder = png::Decoder::new(BufReader::new(File::open(&png_path).expect("PNG file")));
    let mut reader = decoder.read_info().expect("PNG header");
    let mut pixels = vec![0; reader.output_buffer_size().expect("PNG size")];
    let info = reader.next_frame(&mut pixels).expect("PNG data");
    assert_eq!((info.width, info.height), (384, 240), "PNG size in pixels");
    let expected: Vec<u8> = frontend
        .machine()
        .frame()
        .iter()
        .flat_map(|&color| {
            let [_, r, g, b] = palette::rgb(VideoStandard::Ntsc, color).to_be_bytes();
            [r, g, b]
        })
        .collect();
    assert!(pixels == expected, "the screenshot is the in-process frame");
    screen_rows(frontend.machine())
}

/// The index of the first row from row `from` that equals `text`.
fn row_after(rows: &[String], from: usize, text: &str) -> Option<usize> {
    rows.iter()
        .enumerate()
        .skip(from)
        .find(|(_, row)| *row == text)
        .map(|(index, _)| index)
}

/// The frames that run before the typist presses the first key (at the `Ready` prompt).
fn frames_to_first_key(bas: &Path) -> u64 {
    let mut frontend = start(&bas_options(bas, false)).expect("start");
    while matches!(
        frontend.typist().map(Typist::phase),
        Some(Phase::WaitReady(_))
    ) {
        frontend.run_frames(1).expect("run");
    }
    frontend.frames_run() - 1
}

/// Verifies REQ-080: with a file with `10 PRINT 2+2`, `--headless --frames 600 --bas`
/// ends with the rows `RUN`, `4` and `Ready` in this order (criterion 1).
#[test]
fn bas_runs_program() {
    let bas = write_bas("bas_runs_program", "sum.bas", "10 PRINT 2+2\n");
    let rows = bas_screen("bas_runs_program", &bas, &[], 600);
    let run_row = row_after(&rows, 0, "RUN");
    let four = run_row.and_then(|index| row_after(&rows, index + 1, "4"));
    let ready = four.and_then(|index| row_after(&rows, index + 1, "Ready"));
    assert!(ready.is_some(), "RUN, 4, Ready in {rows:#?}");
    assert!(rows.iter().any(|row| row == "10 PRINT 2+2"), "{rows:#?}");
}

/// Verifies REQ-080: with `--no-run`, the program line and `Ready` are on the screen, and
/// no row is `4` (criterion 2).
#[test]
fn bas_no_run_only_types() {
    let bas = write_bas("bas_no_run_only_types", "sum.bas", "10 PRINT 2+2\n");
    let rows = bas_screen("bas_no_run_only_types", &bas, &["--no-run"], 600);
    for text in ["10 PRINT 2+2", "Ready"] {
        assert!(rows.iter().any(|row| row == text), "{text} in {rows:#?}");
    }
    assert!(!rows.iter().any(|row| row == "4"), "no row 4: {rows:#?}");
    assert!(!rows.iter().any(|row| row == "RUN"), "no RUN: {rows:#?}");
}

/// Verifies REQ-080: LF and CR LF line ends give the same key presses, empty lines and
/// lines with only spaces are not typed, and a missing newline at the end has no effect
/// (criterion 3). The key codes are compared, and two runs show the same screen.
#[test]
fn bas_line_endings() {
    let test = "bas_line_endings";
    let lf = write_bas(test, "lf.bas", "10 PRINT 2+2\n20 PRINT 3*3\n");
    let crlf = lf.with_file_name("crlf.bas");
    fs::write(&crlf, "\r\n10 PRINT 2+2\r\n   \r\n\r\n20 PRINT 3*3").expect("write");
    let no_end = lf.with_file_name("no-end.bas");
    fs::write(&no_end, "10 PRINT 2+2\n  \n20 PRINT 3*3").expect("write");
    let expected: Vec<u8> = "10 PRINT 2+2\n20 PRINT 3*3\nRUN\n"
        .chars()
        .map(|c| ascii_to_kbcode(c).expect("key"))
        .collect();
    for path in [&lf, &crlf, &no_end] {
        assert_eq!(
            bas::load(path, true).expect("load"),
            expected,
            "key codes of {}",
            path.display()
        );
    }
    let lf_rows = bas_screen(test, &lf, &[], 600);
    assert!(lf_rows.iter().any(|row| row == "9"), "{lf_rows:#?}");
    assert_eq!(bas_screen(test, &crlf, &[], 600), lf_rows, "CR LF screen");
}

/// Verifies REQ-080: a character without an Atari key gives exit code 2 and a message with
/// the file, the line number and the character, before the machine starts (criterion 4).
/// The OS ROM file does not exist: the machine would fail with a message about it, so the
/// message about the program shows that the check comes first.
#[test]
fn bas_rejects_unknown_character() {
    let bas = write_bas(
        "bas_rejects_unknown_character",
        "brace.bas",
        "10 PRINT 1\n20 PRINT \"{\"\n",
    );
    let output = run(&[
        "--os",
        "/nonexistent/os.rom",
        "--headless",
        "--frames",
        "600",
        "--bas",
        bas.to_str().expect("UTF-8 path"),
    ]);
    assert_eq!(output.status.code(), Some(2), "exit status");
    let message = stderr(&output);
    assert!(
        message.contains("brace.bas") && message.contains("line 2") && message.contains("'{'"),
        "{message}"
    );
    assert!(
        !message.contains("os.rom"),
        "the machine did not start: {message}"
    );
    assert!(output.stdout.is_empty(), "no frames ran");
}

/// Verifies REQ-080: a file that does not exist, or that cannot be read (a directory),
/// gives exit code 2 and a message that names the file (criterion 5).
#[test]
fn bas_missing_file() {
    let dir = temp_dir("bas_missing_file");
    let missing = dir.join("missing.bas");
    let directory = dir.join("dir.bas");
    fs::create_dir_all(&directory).expect("create the directory");
    for path in [&missing, &directory] {
        let name = path.to_str().expect("UTF-8 path");
        let output = run(&["--headless", "--frames", "1", "--bas", name]);
        assert_eq!(output.status.code(), Some(2), "{name}");
        let message = stderr(&output);
        assert!(message.contains(name), "{message}");
    }
}

/// Verifies REQ-080: the typing starts only after the `Ready` prompt is on the screen, and
/// without `Ready` the emulator stops after 600 frames with exit code 1 and a message that
/// BASIC did not start (criterion 6). The first part runs the frontend frame by frame and
/// reads the POKEY key bit (`SKSTAT` bit 2 is 0 while a key is down). For the second part,
/// a BASIC ROM of zero bytes never shows `Ready`.
#[test]
fn bas_waits_for_ready() {
    let bas = write_bas("bas_waits_for_ready", "sum.bas", "10 PRINT 2+2\n");
    let mut frontend = start(&bas_options(&bas, false)).expect("start");
    let mut typing = false;
    for _ in 0..600 {
        let ready_before = has_row(frontend.machine(), "Ready");
        frontend.run_frames(1).expect("run");
        if frontend.machine().peek(0xD20F) & 0x04 == 0 {
            assert!(ready_before, "a key is down before Ready");
            typing = true;
            break;
        }
        assert!(!ready_before, "the typing starts at once after Ready");
    }
    assert!(typing, "the typing started");

    let zero = temp_dir("bas_waits_for_ready-rom").join("zero.rom");
    fs::write(&zero, [0_u8; 0x2000]).expect("write the ROM");
    let output = run(&[
        "--basic",
        zero.to_str().expect("UTF-8 path"),
        "--headless",
        "--frames",
        "2000",
        "--bas",
        bas.to_str().expect("UTF-8 path"),
    ]);
    assert_eq!(output.status.code(), Some(1), "exit status");
    let message = stderr(&output);
    assert!(
        message.contains("BASIC did not start") && message.contains("600 frames"),
        "{message}"
    );
}

/// Verifies REQ-080: the frontend keeps showing frames while it types (criterion 7).
///
/// A test cannot open the window, so this test checks the two parts that the window uses.
/// First, `Frontend::step`, which the window calls once per host frame before it shows the
/// frame, runs exactly one frame per call while the program is typed, and the screen
/// between two calls shows a growing part of the program line. Second, a headless run that
/// stops in the middle of the typing (a small `--frames`) gives a screenshot of a part of
/// the line.
#[test]
fn bas_window_shows_typing() {
    let test = "bas_window_shows_typing";
    let line = "10 PRINT 2+2";
    let bas = write_bas(test, "two.bas", "10 PRINT 2+2\n20 PRINT 3*3\n");
    let mut frontend = start(&bas_options(&bas, false)).expect("start");
    let mut parts: Vec<String> = Vec::new();
    while frontend.typist().is_some() {
        let frames = frontend.frames_run();
        assert_eq!(frontend.step(&[]).expect("step"), Flow::Continue, "step");
        assert_eq!(frontend.frames_run(), frames + 1, "one frame per step");
        let rows = screen_rows(frontend.machine());
        if let Some(part) = rows
            .iter()
            .find(|row| row.len() > 2 && line.starts_with(row.as_str()))
            && parts.last() != Some(part)
        {
            parts.push(part.clone());
        }
    }
    assert!(
        parts.len() >= 8 && parts.last().map(String::as_str) == Some(line),
        "the line grows while it is typed: {parts:?}"
    );

    let frames = frames_to_first_key(&bas) + u64::from(KEY_DOWN_FRAMES + KEY_UP_FRAMES) * 7;
    let rows = bas_screen(test, &bas, &[], frames);
    assert!(
        rows.iter()
            .any(|row| row.len() > 2 && row.len() < line.len() && line.starts_with(row.as_str())),
        "a part of the line after {frames} frames: {rows:#?}"
    );
}

/// Verifies REQ-080: `--help` lists `--bas <file>` and `--no-run`, and `--no-run` without
/// `--bas` gives exit code 2 (criterion 8).
#[test]
fn bas_help_and_no_run_alone() {
    let output = run(&["--help"]);
    assert!(output.status.success(), "--help");
    let usage = String::from_utf8_lossy(&output.stdout);
    for option in ["--bas <file>", "--no-run"] {
        assert!(usage.contains(option), "{option} in {usage}");
    }
    let output = run(&["--headless", "--frames", "1", "--no-run"]);
    assert_eq!(output.status.code(), Some(2), "--no-run alone");
    assert!(
        stderr(&output).contains("--no-run needs --bas"),
        "{}",
        stderr(&output)
    );
}

/// A program with every character that has a key (the letters in upper case), split over
/// lines that fit in the 38 columns of the screen, and lines with the same character two or
/// more times in a row.
const ALL_KEYS_PROGRAM: &str = "10 REM ABCDEFGHIJKLMNOPQRSTUVWXYZ
20 REM 0123456789 ;+*-=,./<>:\\^_
30 REM |[]?!\"#$%&'@()
40 PRINT \"AA11  ;;\"
50 REM AABBCCDDEEFFGGHHIIJJKKLLMMNN
60 REM OOPPQQRRSSTTUUVVWWXXYYZZ
70 REM 00112233445566778899;;++**--
80 REM ==,,..//<<>>::\\\\^^__||[[]]??
90 REM !!\"\"##$$%%&&''@@(())
100 REM AAA111   ;;;:::
";

/// Runs `frontend` until its typist is done, then for `frames` more frames.
fn run_until_typed(frontend: &mut Frontend, frames: u64) {
    while frontend.typist().is_some() {
        frontend.run_frames(1).expect("run");
    }
    frontend.run_frames(frames).expect("run");
}

/// Verifies REQ-080: with `--no-run`, the typed program is exactly the program of the file:
/// after `LIST`, the screen shows each line of the file, for a file with every character
/// that has a key and with lines with the same character two or more times in a row
/// (criterion 9). `LIST` is typed with a second typist, after the first one is done.
#[test]
fn bas_typed_listing_equals_file() {
    let bas = write_bas(
        "bas_typed_listing_equals_file",
        "keys.bas",
        ALL_KEYS_PROGRAM,
    );
    let mut frontend = start(&bas_options(&bas, true)).expect("start");
    run_until_typed(&mut frontend, 1);
    let list: Vec<u8> = "LIST\n"
        .chars()
        .map(|c| ascii_to_kbcode(c).expect("key"))
        .collect();
    let mut frontend = frontend.with_typist(Typist::new(list));
    run_until_typed(&mut frontend, 120);
    let rows = screen_rows(frontend.machine());
    let list_row = rows.iter().rposition(|row| row == "LIST");
    assert!(list_row.is_some(), "a row LIST: {rows:#?}");
    let listing: Vec<&str> = rows[list_row.map_or(0, |index| index + 1)..]
        .iter()
        .take_while(|row| *row != "Ready")
        .map(String::as_str)
        .filter(|row| !row.is_empty())
        .collect();
    let file: Vec<&str> = ALL_KEYS_PROGRAM.lines().collect();
    assert_eq!(listing, file, "the listing on the screen: {rows:#?}");
}

/// Verifies REQ-080: the typing of `examples/pong.bas` ends within 2,300 frames after the
/// `Ready` prompt (criterion 10). The typing includes `RUN`; after it, the game shows its
/// score row.
#[test]
fn bas_pong_typing_frames() {
    let pong = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/pong.bas");
    let ready = frames_to_first_key(&pong);
    let mut frontend = start(&bas_options(&pong, false)).expect("start");
    run_until_typed(&mut frontend, 0);
    let typing = frontend.frames_run() - ready;
    assert!(typing <= 2300, "the typing took {typing} frames");
    frontend.run_frames(120).expect("run");
    let rows = screen_rows(frontend.machine());
    assert!(
        rows.iter().any(|row| row.starts_with("SCORE 0")),
        "the game runs: {rows:#?}"
    );
}

/// A program with lower case letters: in a `REM` and a string, mixed with upper case
/// letters in a line and from one line to the next, and doubled.
const LOWER_CASE_PROGRAM: &str = "10 REM abcdefghijklmnopqrstuvwxyz
20 PRINT \"Hello, World\"
30 REM aA bB zZ aa AA
40 REM x
50 REM X
";

/// Verifies REQ-080: with `--no-run`, a program with lower case letters is typed exactly:
/// after `LIST`, the screen shows each line of the file with its lower case letters
/// (criterion 9). After the program, the keyboard is in upper case again, so the second
/// typist types `LIST` with the same key codes as before.
#[test]
fn bas_typed_listing_keeps_lower_case() {
    let bas = write_bas(
        "bas_typed_listing_keeps_lower_case",
        "lower.bas",
        LOWER_CASE_PROGRAM,
    );
    let mut frontend = start(&bas_options(&bas, true)).expect("start");
    run_until_typed(&mut frontend, 1);
    let list: Vec<u8> = "LIST\n"
        .chars()
        .map(|c| ascii_to_kbcode(c).expect("key"))
        .collect();
    let mut frontend = frontend.with_typist(Typist::new(list));
    run_until_typed(&mut frontend, 120);
    let rows = screen_rows(frontend.machine());
    let list_row = rows.iter().rposition(|row| row == "LIST");
    assert!(list_row.is_some(), "a row LIST: {rows:#?}");
    let listing: Vec<&str> = rows[list_row.map_or(0, |index| index + 1)..]
        .iter()
        .take_while(|row| *row != "Ready")
        .map(String::as_str)
        .filter(|row| !row.is_empty())
        .collect();
    let file: Vec<&str> = LOWER_CASE_PROGRAM.lines().collect();
    assert_eq!(listing, file, "the listing on the screen: {rows:#?}");
}

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

/// The fields of a statistics line of REQ-083 criterion 1.
#[derive(Debug)]
struct StatsLine {
    /// The frames since the start.
    frame: u64,
    /// The frames in the last second.
    fps: f64,
    /// The speed in percent.
    speed: u64,
    /// The frames that the window showed.
    shown: u64,
    /// The mean emulation time of a frame in ms.
    emu: f64,
    /// `on` or `off`.
    turbo: String,
    /// `on`, `off` or `-`.
    motor: String,
    /// The audio underruns.
    underruns: u64,
}

/// The fields of a statistics line, or `None` when the line does not have the format of
/// REQ-083 criterion 1.
fn stats_fields(line: &str) -> Option<StatsLine> {
    let words: Vec<&str> = line.split(' ').collect();
    let [
        "stats:",
        "frame",
        frame,
        "fps",
        fps,
        "speed",
        speed,
        "shown",
        shown,
        "emu",
        emu,
        "ms",
        "turbo",
        turbo,
        "motor",
        motor,
        "underruns",
        underruns,
    ] = words[..]
    else {
        return None;
    };
    let decimals = |text: &str, places: usize| {
        text.split_once('.')
            .is_some_and(|(_, fraction)| fraction.len() == places)
    };
    if !decimals(fps, 1) || !decimals(emu, 2) {
        return None;
    }
    if !["on", "off"].contains(&turbo) || !["on", "off", "-"].contains(&motor) {
        return None;
    }
    Some(StatsLine {
        frame: frame.parse().ok()?,
        fps: fps.parse().ok()?,
        speed: speed.strip_suffix('%')?.parse().ok()?,
        shown: shown.parse().ok()?,
        emu: emu.parse().ok()?,
        turbo: turbo.to_owned(),
        motor: motor.to_owned(),
        underruns: underruns.parse().ok()?,
    })
}

/// The elapsed seconds of a run with `--frames`, from standard output
/// (`<n> frames in <s> s (...)`).
fn elapsed_seconds(output: &Output) -> f64 {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    stdout
        .split_once(" frames in ")
        .and_then(|(_, rest)| rest.split_once(" s "))
        .and_then(|(seconds, _)| seconds.parse().ok())
        .unwrap_or_else(|| panic!("the elapsed time on standard output: {stdout}"))
}

/// Verifies REQ-083: `--headless --stats` with enough frames for more than 2 s of host time
/// writes lines to standard error in the format of criterion 1, with `shown 0`,
/// `underruns 0`, `turbo off` and `motor -`. The frame numbers increase, and the number of
/// lines is the elapsed seconds of the run (from standard output), rounded down, or one
/// less (criteria 1, 4 and 7).
#[test]
fn stats_headless_lines() {
    // A debug build runs about 500 frames each second: 2400 frames take more than 2 s
    // unless the host is much faster. The run doubles the frames until it takes 2 s.
    let mut frames: u64 = 2400;
    let (output, elapsed) = loop {
        let output = run(&["--headless", "--stats", "--frames", &frames.to_string()]);
        assert!(output.status.success(), "exit status: {}", stderr(&output));
        let elapsed = elapsed_seconds(&output);
        if elapsed > 2.0 {
            break (output, elapsed);
        }
        frames *= 2;
    };
    let message = stderr(&output);
    let lines: Vec<&str> = message.lines().collect();
    let mut last_frame = 0;
    for line in &lines {
        let Some(StatsLine {
            frame,
            fps,
            speed,
            shown,
            emu,
            turbo,
            motor,
            underruns,
        }) = stats_fields(line)
        else {
            panic!("the format of criterion 1: {line:?}");
        };
        assert!(
            frame > last_frame && frame <= frames,
            "the frame number increases: {line}"
        );
        last_frame = frame;
        assert!(fps > 0.0, "frames ran: {line}");
        assert!(speed > 0, "a speed: {line}");
        assert!(emu > 0.0, "an emulation time: {line}");
        assert_eq!(
            (shown, turbo.as_str(), motor.as_str(), underruns),
            (0, "off", "-", 0),
            "headless, no tape: {line}"
        );
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the elapsed seconds are positive and small"
    )]
    let seconds = elapsed.floor() as usize;
    assert!(
        lines.len() == seconds || lines.len() + 1 == seconds,
        "{} lines in {elapsed} s: {message}",
        lines.len()
    );
}

/// Verifies REQ-083: a headless run without `--stats` writes no line that starts with
/// `stats:` to standard error (criterion 8).
#[test]
fn no_stats_without_option() {
    let output = run(&["--headless", "--frames", "600"]);
    assert!(output.status.success(), "exit status: {}", stderr(&output));
    assert!(
        !stderr(&output)
            .lines()
            .any(|line| line.starts_with("stats:")),
        "no statistics line: {}",
        stderr(&output)
    );
}

/// Verifies REQ-083: `--help` lists `--stats` (criterion 9).
#[test]
fn stats_help_lists_option() {
    let output = run(&["--help"]);
    assert!(output.status.success(), "--help exit status");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("  --stats  "),
        "--help lists --stats"
    );
}

/// The environment variable that makes `log_writes_tape_lines` the child process. Its value
/// is the path of the tape.
const LOG_CHILD: &str = "ATARI_MAC_LOG_TAPE_CHILD";

/// The child process of `log_writes_tape_lines`: parses `--headless --frames 2400 --log tape
/// --no-fast-tape --cas <tape>` with the free ROMs (the serial path, so that each record
/// start has a line), installs the logger of `--log` as `run` does, and boots
/// the tape with RETURN in frames 180-186 (as `cas_boots_tape`). The headless binary presses
/// no key, so the test process itself is the child. It stops 5 frames after the boot code
/// wrote `$A5` to `$0600`.
fn log_tape_child(tape: &str) {
    let args = [
        "--os",
        OS_ROM,
        "--basic",
        BASIC_ROM,
        "--headless",
        "--frames",
        "2400",
        "--log",
        "tape",
        "--no-fast-tape",
        "--cas",
        tape,
    ];
    let options = match cli::parse(args.map(OsString::from)) {
        Ok(cli::Command::Run(options)) => options,
        other => panic!("the command line: {other:?}"),
    };
    logging::install(options.log.expect("--log tape"));
    let mut frontend = start(&options).expect("start");
    let mut after = None;
    for frame in 0..2_400 {
        if frame == 180 {
            frontend
                .machine_mut()
                .key_down(ascii_to_kbcode('\n').expect("RETURN"));
        }
        if frame == 186 {
            frontend.machine_mut().key_up();
        }
        frontend.run_frames(1).expect("run");
        if after.is_none() && frontend.machine().peek_ram(0x0600) == 0xA5 {
            after = Some(frame + 5);
        }
        if after == Some(frame) {
            return;
        }
    }
    panic!("no $A5 at $0600 after 2,400 frames");
}

/// Verifies REQ-085: `--log tape --cas` with the tape of `boot_tape_file` and RETURN after
/// the prompt tone writes lines `[frame <f> cycle <c>] tape: <message>` to standard error:
/// the motor on line, the three record lines and the tape ends line, with frame and cycle
/// counts that increase (criterion 2). The OS does not stop the motor after this boot, so
/// no motor off line comes (`crates/harness/tests/event_log.rs` `tape_records` presses
/// RESET to get it).
///
/// The headless binary presses no key, so the test runs its own test binary as the child
/// process (`log_tape_child`, with the variable `LOG_CHILD`). The child installs the logger
/// of `--log` in its own process, so the level of this process stays `Off`.
#[test]
fn log_writes_tape_lines() {
    if let Some(tape) = env::var_os(LOG_CHILD) {
        log_tape_child(tape.to_str().expect("UTF-8 path"));
        return;
    }
    let tape = boot_tape_file("log_writes_tape_lines");
    let output = Command::new(env::current_exe().expect("the test binary"))
        .args(["--exact", "log_writes_tape_lines", "--nocapture"])
        .env(LOG_CHILD, &tape)
        .output()
        .expect("run the child process");
    let message = stderr(&output);
    assert!(output.status.success(), "the child: {message}");
    let mut lines = Vec::new();
    for line in message.lines().filter(|line| line.starts_with("[frame")) {
        let rest = line.strip_prefix("[frame ").expect("the prefix");
        let (frame, rest) = rest.split_once(" cycle ").expect("the cycle");
        let (cycle, text) = rest.split_once("] tape: ").expect("the category");
        let frame: u64 = frame.parse().expect("the frame count");
        let cycle: u64 = cycle.parse().expect("the cycle count");
        lines.push((frame, cycle, text.to_owned()));
    }
    let texts: Vec<&str> = lines.iter().map(|(_, _, text)| text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "motor on, record 0 byte 0",
            "record 0 starts: 132 bytes at 600 baud",
            "record 1 starts: 132 bytes at 600 baud",
            "record 2 starts: 132 bytes at 600 baud",
            "tape ends",
        ],
        "the tape lines: {message}"
    );
    for pair in lines.windows(2) {
        assert!(
            pair[0].0 <= pair[1].0 && pair[0].1 < pair[1].1,
            "the counts increase: {pair:?}"
        );
    }
    assert!(
        lines.first().is_some_and(|(frame, _, _)| *frame >= 180),
        "the motor starts after RETURN: {lines:?}"
    );
}

/// Verifies REQ-085: `--log tape,disk` and `--log sio:trace` give exit status 2 before the
/// machine starts, and standard error names `disk` and `sio:trace` (criterion 4).
#[test]
fn log_rejects_unknown_category() {
    for (value, item) in [("tape,disk", "disk"), ("sio:trace", "sio:trace")] {
        let output = run(&["--headless", "--frames", "1", "--log", value]);
        let message = stderr(&output);
        assert_eq!(output.status.code(), Some(2), "--log {value}: {message}");
        assert!(
            message.contains(&format!("unknown --log category \"{item}\"")),
            "--log {value}: the message names {item}: {message}"
        );
        assert!(
            output.stdout.is_empty(),
            "--log {value}: no frame ran: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

/// Verifies REQ-085: a headless run with a tape and a disk without `--log` writes no line
/// that starts with `[frame` to standard error. `atari_mac::run` in the test process without
/// `--log` leaves `log::max_level()` at `Off` (criterion 5).
#[test]
fn no_log_without_option() {
    let tape = boot_tape_file("no_log_without_option");
    let disk = tape.with_file_name("disk.atr");
    fs::write(&disk, valid_atr()).expect("write the ATR file");
    let tape = tape.to_str().expect("UTF-8 path");
    let disk = disk.to_str().expect("UTF-8 path");
    let args = [
        "--headless",
        "--frames",
        "120",
        "--cas",
        tape,
        "--disk",
        disk,
    ];
    let output = run(&args);
    assert!(output.status.success(), "exit status: {}", stderr(&output));
    assert!(
        !stderr(&output)
            .lines()
            .any(|line| line.starts_with("[frame")),
        "no log line: {}",
        stderr(&output)
    );
    let mut out = Vec::new();
    let result = atari_mac::run(
        ["--os", OS_ROM, "--basic", BASIC_ROM]
            .into_iter()
            .chain(args)
            .map(OsString::from),
        &mut out,
    );
    assert!(result.is_ok(), "run in the test process: {result:?}");
    assert_eq!(
        log::max_level(),
        log::LevelFilter::Off,
        "the level stays Off"
    );
}

/// Verifies REQ-085: `--help` lists `--log <categories>` (criterion 6).
#[test]
fn log_help_lists_option() {
    let output = run(&["--help"]);
    assert!(output.status.success(), "--help exit status");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("--log <categories>"),
        "--help lists --log <categories>"
    );
}

/// The base cycle count of each opcode (NMOS 6502, HRM 3.5), without the page-crossing and
/// branch cycles. A KIL opcode has 0.
const BASE_CYCLES: [u8; 256] = [
    7, 6, 0, 8, 3, 3, 5, 5, 3, 2, 2, 2, 4, 4, 6, 6, // $0x
    2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // $1x
    6, 6, 0, 8, 3, 3, 5, 5, 4, 2, 2, 2, 4, 4, 6, 6, // $2x
    2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // $3x
    6, 6, 0, 8, 3, 3, 5, 5, 3, 2, 2, 2, 3, 4, 6, 6, // $4x
    2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // $5x
    6, 6, 0, 8, 3, 3, 5, 5, 4, 2, 2, 2, 5, 4, 6, 6, // $6x
    2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // $7x
    2, 6, 2, 6, 3, 3, 3, 3, 2, 2, 2, 2, 4, 4, 4, 4, // $8x
    2, 6, 0, 6, 4, 4, 4, 4, 2, 5, 2, 5, 5, 5, 5, 5, // $9x
    2, 6, 2, 6, 3, 3, 3, 3, 2, 2, 2, 2, 4, 4, 4, 4, // $Ax
    2, 5, 0, 5, 4, 4, 4, 4, 2, 4, 2, 4, 4, 4, 4, 4, // $Bx
    2, 6, 2, 8, 3, 3, 5, 5, 2, 2, 2, 2, 4, 4, 6, 6, // $Cx
    2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // $Dx
    2, 6, 2, 8, 3, 3, 5, 5, 2, 2, 2, 2, 4, 4, 6, 6, // $Ex
    2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // $Fx
];

/// The free ROMs as `Options`, without media.
fn rom_options() -> Options {
    Options {
        os: OS_ROM.into(),
        basic: BASIC_ROM.into(),
        ..Options::default()
    }
}

/// Verifies REQ-086: `InstructionLine` and `SequenceLine` of fixed events give the
/// example lines of `docs/design/debugger.md`, "Trace writer":
/// `1 8 42 30822 E477: A9 00    LDA #$00  A=00 X=FF Y=01 S=FD P=N.-B.IZ.` and
/// `1 248 7 58147 NMI vector $E7B2` (criteria 1 and 2).
#[test]
fn trace_line_format() {
    let at = Position {
        frame: 1,
        scanline: 8,
        hpos: 42,
        cycle: 30822,
    };
    let regs = Registers {
        a: 0x00,
        x: 0xFF,
        y: 0x01,
        s: 0xFD,
        p: 0xB6,
        pc: 0xE477,
    };
    let line = |bytes: [u8; 3], p: u8| {
        InstructionLine(&InstructionStart {
            at,
            regs: Registers { p, ..regs },
            bytes,
        })
        .to_string()
    };
    assert_eq!(
        line([0xA9, 0x00, 0x8D], 0xB6),
        "1 8 42 30822 E477: A9 00    LDA #$00  A=00 X=FF Y=01 S=FD P=N.-B.IZ."
    );
    assert_eq!(
        line([0x8D, 0x0A, 0xD4], 0xFF),
        "1 8 42 30822 E477: 8D 0A D4 STA $D40A  A=00 X=FF Y=01 S=FD P=NV-BDIZC"
    );
    assert_eq!(
        line([0xEA, 0x00, 0x00], 0x20),
        "1 8 42 30822 E477: EA       NOP  A=00 X=FF Y=01 S=FD P=..-....."
    );
    let sequence = |kind, handler| {
        SequenceLine(&Sequence {
            at: Position {
                frame: 1,
                scanline: 248,
                hpos: 7,
                cycle: 58147,
            },
            kind,
            handler,
        })
        .to_string()
    };
    assert_eq!(
        sequence(SequenceKind::Nmi, 0xE7B2),
        "1 248 7 58147 NMI vector $E7B2"
    );
    assert_eq!(
        sequence(SequenceKind::Irq, 0x00AB),
        "1 248 7 58147 IRQ vector $00AB"
    );
    assert_eq!(
        sequence(SequenceKind::Reset, 0xC000),
        "1 248 7 58147 RESET vector $C000"
    );
}

/// The trace lines of frame 1 of the boot from a cycle-by-cycle run with `run_cycle`, and
/// the opcode of each instruction line. An instruction starts in the cycle after which
/// `Cpu::at_opcode_fetch` is no longer true. A sequence starts in the cycle after which the
/// boundary of an interrupt is gone, and its line comes before the next instruction line.
fn reference_trace_of_frame_1() -> Vec<(String, Option<u8>)> {
    let mut machine = start(&rom_options()).expect("machine").machine().clone();
    machine.run_frame();
    let mut lines = Vec::new();
    let mut pending: Option<Position> = None;
    while machine.frame_count() == 1 {
        let cycle = machine.cycle_count();
        let at = Position {
            frame: 1,
            scanline: u32::try_from((cycle / 114) % 262).expect("line"),
            hpos: u32::try_from(cycle % 114).expect("cycle"),
            cycle,
        };
        let fetch = machine.cpu().at_opcode_fetch();
        let interrupt = machine.cpu().boundary() == Some(Boundary::Interrupt);
        let regs = *machine.cpu().registers();
        let byte = |offset: u16| {
            machine
                .debug_peek(regs.pc.wrapping_add(offset))
                .unwrap_or(0xFF)
        };
        let bytes = [byte(0), byte(1), byte(2)];
        machine.run_cycle();
        if fetch && !machine.cpu().at_opcode_fetch() {
            if let Some(start) = pending.take() {
                let kind = if machine.cpu().vector() == 0xFFFA {
                    SequenceKind::Nmi
                } else {
                    SequenceKind::Irq
                };
                let sequence = Sequence {
                    at: start,
                    kind,
                    handler: regs.pc,
                };
                lines.push((SequenceLine(&sequence).to_string(), None));
            }
            let start = InstructionStart { at, regs, bytes };
            lines.push((InstructionLine(&start).to_string(), Some(bytes[0])));
        } else if interrupt && machine.cpu().boundary().is_none() {
            pending = Some(at);
        }
    }
    lines
}

/// Verifies REQ-086: `--headless --frames 3 --trace <file> --trace-from 1
/// --trace-frames 1` writes a line for each instruction that starts in frame 1 and only these.
/// The first line is the first instruction of frame 1. The cycle of each line minus the cycle
/// of the line before is at least the base cycle count of the instruction before it, and equals
/// the value of a cycle-by-cycle run of the same boot (criteria 3 and 4).
#[test]
fn trace_writes_selected_frame() {
    let path = temp_dir("trace_writes_selected_frame").join("trace.txt");
    let path_arg = path.to_str().expect("UTF-8 path");
    let output = run(&[
        "--headless",
        "--frames",
        "3",
        "--trace",
        path_arg,
        "--trace-from",
        "1",
        "--trace-frames",
        "1",
    ]);
    assert!(output.status.success(), "exit status: {}", stderr(&output));
    let text = fs::read_to_string(&path).expect("trace file");
    assert!(text.ends_with('\n'), "each line ends with a line end");
    let lines: Vec<&str> = text.lines().collect();

    let expected = reference_trace_of_frame_1();
    assert!(expected.len() > 1000, "{} reference lines", expected.len());
    let expected_text: Vec<&str> = expected.iter().map(|(line, _)| line.as_str()).collect();
    assert_eq!(lines, expected_text, "the trace of frame 1");
    assert!(
        expected[0].1.is_some(),
        "the first line is an instruction: {}",
        lines[0]
    );

    let mut last: Option<(u64, u8)> = None;
    for line in &lines {
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(fields[0], "1", "frame of {line}");
        let hpos: u64 = fields[2].parse().expect("hpos");
        let cycle: u64 = fields[3].parse().expect("cycle");
        assert!(hpos < 114, "hpos of {line}");
        if fields[4].ends_with(':') {
            let opcode = u8::from_str_radix(fields[5], 16).expect("opcode");
            if let Some((before, previous)) = last {
                assert!(
                    cycle - before >= u64::from(BASE_CYCLES[usize::from(previous)]),
                    "{line} after the opcode ${previous:02X} at cycle {before}"
                );
            }
            last = Some((cycle, opcode));
        }
    }
}

/// Verifies REQ-086: the frame buffer and the RAM after 3 frames are the same
/// with and without the trace (criterion 5).
#[test]
fn trace_does_not_change_machine() {
    let path = temp_dir("trace_does_not_change_machine").join("trace.txt");
    let mut plain = start(&rom_options()).expect("plain frontend");
    let writer = TraceWriter::new(BufWriter::new(File::create(&path).expect("file")), 0, 3);
    let mut traced = start(&rom_options())
        .expect("traced frontend")
        .with_trace(writer);
    plain.run_frames(3).expect("plain frames");
    traced.run_frames(3).expect("traced frames");
    traced
        .take_trace()
        .expect("the writer")
        .finish()
        .expect("trace written");
    assert!(
        fs::read_to_string(&path).expect("trace").lines().count() > 1000,
        "the trace has lines"
    );
    let (a, b) = (plain.machine(), traced.machine());
    assert!(a.frame() == b.frame(), "frame buffer");
    assert!(
        (0..=0xFFFF_u16).all(|addr| a.peek_ram(addr) == b.peek_ram(addr)),
        "RAM"
    );
    assert!(a == b, "machine");
}

/// Verifies REQ-086: `--trace` with a file in a directory that does not exist gives
/// exit code 2 and a message on standard error that names the file (criterion 6).
#[test]
fn trace_file_error_exit_code() {
    let path = temp_dir("trace_file_error_exit_code")
        .join("missing")
        .join("trace.txt");
    let path_arg = path.to_str().expect("UTF-8 path");
    let output = run(&["--headless", "--frames", "1", "--trace", path_arg]);
    assert_eq!(output.status.code(), Some(2), "exit code");
    assert!(
        stderr(&output).contains(path_arg),
        "the message names the file: {}",
        stderr(&output)
    );
    for args in [
        &["--headless", "--frames", "1", "--trace-from", "1"][..],
        &[
            "--headless",
            "--frames",
            "1",
            "--trace",
            path_arg,
            "--trace-frames",
            "0",
        ][..],
    ] {
        assert_eq!(run(args).status.code(), Some(2), "usage error for {args:?}");
    }
}

/// Verifies REQ-086: `--help` lists `--trace <file>`, `--trace-from <frame>` and
/// `--trace-frames <n>` (criterion 7).
#[test]
fn trace_help_lists_options() {
    let output = run(&["--help"]);
    assert!(output.status.success(), "--help exit status");
    let text = String::from_utf8_lossy(&output.stdout);
    for option in [
        "--trace <file>",
        "--trace-from <frame>",
        "--trace-frames <n>",
    ] {
        assert!(text.contains(option), "--help lists {option}");
    }
}

/// Verifies REQ-088: `Monitor::open` on a running machine stops it before
/// an instruction (`Machine::is_held`) and gives the instruction line of REQ-086 criterion 1 of
/// `Machine::held_instruction` (criterion 1).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_open_shows_next_instruction() {
    planned();
}

/// Verifies REQ-088: the command `s` runs one instruction and shows the next
/// instruction line. Its cycle minus the old cycle is the cycle count of the instruction plus
/// the halt cycles before the next opcode fetch (criterion 2).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_step_runs_one_instruction() {
    planned();
}

/// Verifies REQ-088: the command `f` stops at the first instruction of the next
/// frame: the line has the next frame number, and no instruction of that frame comes before it
/// (criterion 3).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_frame_runs_to_next_frame() {
    planned();
}

/// Verifies REQ-088: the command `g` gives `Reply::resume` true, closes the monitor,
/// and the next frame runs to its end (criterion 4).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_go_continues() {
    planned();
}

/// Verifies REQ-088: 20 `s` commands, 2 `f` commands and a `g` in
/// frame 2, then a run to the cycle count of 5 frames, give the frame buffer and the RAM of a
/// plain run to the same cycle count (criterion 5).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_steps_keep_machine_deterministic() {
    planned();
}

/// Verifies REQ-088: while the monitor is open, `Frontend::step` runs no frame,
/// the audio sink gets `stop_sound`, and no underrun is counted (criterion 6).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_open_silences_audio() {
    planned();
}

/// Verifies REQ-089: `b <address>` at an instruction of a test
/// program stops the run before that instruction, with `PC` at the address and the line of
/// REQ-088 criterion 1 (criterion 1).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_breakpoint_stops_before_instruction() {
    planned();
}

/// Verifies REQ-089: `ww` and `wr` on a RAM address of a test program stop
/// the run after the instruction that writes or reads it. The output names the address, the
/// value and the instruction line (criterion 2).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_watches_stop_after_access() {
    planned();
}

/// Verifies REQ-089: `bl` lists the breakpoints and the watches with the
/// numbers 1, 2 and 3. `bd 2` deletes the second, and the next `bl` shows the numbers 1 and 2
/// (criterion 3).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_lists_and_deletes_points() {
    planned();
}

/// Verifies REQ-089: `--break 0600` and `--break $E477` parse to
/// `Options::breaks` `[$0600, $E477]`, and the monitor has the two breakpoints before the
/// first frame (criterion 4).
#[test]
#[ignore = "planned: SLICE-11"]
fn break_option_sets_breakpoint() {
    planned();
}

/// Verifies REQ-089: with a breakpoint at `$0600`, the boot of the boot
/// test tape of REQ-643 stops with `PC` = `$0600`, before the first instruction of the boot
/// program (criterion 5).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_breakpoint_at_boot_tape_start() {
    planned();
}

/// Verifies REQ-089: with no breakpoints and no watches, `Monitor::active` is
/// false and the frontend runs the frames with `Machine::run_frame` (criterion 6).
#[test]
#[ignore = "planned: SLICE-11"]
fn no_points_use_plain_run() {
    planned();
}

/// Verifies REQ-090: the command `x` shows `PC`, `A`, `X`, `Y`, `S` and `P` in the
/// format of REQ-086 criterion 1, and the scan line and the horizontal position (criterion 1).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_shows_registers() {
    planned();
}

/// Verifies REQ-090: `m 0600` shows 8 lines of 16 bytes (the default length 128), in
/// hexadecimal and in ATASCII; `m 0600 20` shows 2 lines (criterion 2).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_shows_memory() {
    planned();
}

/// Verifies REQ-090: `d E477` shows 16 instruction lines (the default count), and
/// `d E477 3` shows 3, each line at the address of the line before plus its length
/// (criterion 3).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_disassembles_memory() {
    planned();
}

/// Verifies REQ-090: `e 0600 A9 00` writes the two bytes to RAM, and `r`
/// changes each of `A`, `X`, `Y`, `S`, `P` and `PC` (criterion 4).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_changes_memory_and_registers() {
    planned();
}

/// Verifies REQ-090: `m D000 2048` and `d D000 100` leave the machine equal
/// to a copy from before, and `$D500` shows `--` (criterion 5).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_reads_without_side_effects() {
    planned();
}

/// Verifies REQ-090: `m G000`, `e 10000 1` and `r Q 1` each give a line
/// `error: ...` and change nothing (criterion 6).
#[test]
#[ignore = "planned: SLICE-11"]
fn monitor_rejects_bad_input() {
    planned();
}
