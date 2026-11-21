//! End-to-end tests for the `atari-mac` binary.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use atari_mac::atari_core::gtia::{FRAME_HEIGHT, FRAME_WIDTH};
use atari_mac::atari_core::keyboard::ascii_to_kbcode;
use atari_mac::atari_core::screen::{has_row, screen_text};
use atari_mac::atari_core::{Machine, MachineConfig, VideoStandard, palette};
use atari_mac::bas::{self, KEY_DOWN_FRAMES, KEY_UP_FRAMES, Phase, Typist};
use atari_mac::cli::Options;
use atari_mac::frontend::{Flow, Frontend};
use atari_mac::start;
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
