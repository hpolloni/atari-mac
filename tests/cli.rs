//! End-to-end tests for the `atari-mac` binary.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use atari_mac::atari_core::gtia::{FRAME_HEIGHT, FRAME_WIDTH};
use atari_mac::atari_core::{Machine, MachineConfig, VideoStandard, palette};
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

/// A 100-byte cartridge gives a message that names the file and exit status 2. `--xex`
/// and `--disk` are not supported yet and also exit with status 2.
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
    for option in ["--xex", "--disk"] {
        let output = run(&["--headless", "--frames", "1", option, "x"]);
        assert_eq!(output.status.code(), Some(2), "{option}");
        assert!(
            stderr(&output).contains("not supported yet"),
            "{option}: {}",
            stderr(&output)
        );
    }
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
