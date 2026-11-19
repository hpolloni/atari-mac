//! Golden frame tests: frame comparisons with reference PNG files (REQ-026).
//!
//! The reference files in `crates/harness/golden/` come from this emulator: run
//! `UPDATE_GOLDEN=1 cargo test -p atari-harness --test golden` to write them again, and
//! review each changed image before the commit. The tests of the comparison itself use
//! temporary directories, so that they never change the reference files.

use std::path::{Path, PathBuf};
use std::process;
use std::{env, fs, io};

use atari_core::{VideoStandard, palette};
use atari_harness::HarnessError;
use atari_harness::image::{
    FAILURE_DIR, GOLDEN_DIR, GoldenPaths, RGB_FRAME_BYTES, compare_golden, compare_rgb, diff_image,
    frame_to_rgb, load_png, save_png,
};
use atari_harness::screen::has_row;
use atari_harness::system::{BOOT_FRAMES, BootOptions, boot, run_frames};

/// A temporary directory for one test, empty at the start. The test removes it at the end.
fn temp_dir(test: &str) -> io::Result<PathBuf> {
    let dir = env::temp_dir().join(format!("atari-harness-golden-{}-{test}", process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Comparison paths in `dir`: references in `dir/golden`, failures in `dir/failures`.
fn paths_in(dir: &Path, update: bool) -> GoldenPaths {
    GoldenPaths {
        reference_dir: dir.join("golden"),
        failure_dir: dir.join("failures"),
        update,
    }
}

/// A test frame: each pixel has a different color from its neighbors.
fn test_frame() -> Vec<u8> {
    (0..RGB_FRAME_BYTES)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect()
}

/// Verifies REQ-026: the BASIC `Ready` screen after boot, in NTSC and in PAL, passes only
/// when every pixel is equal to the reference PNG (criterion 1).
#[test]
fn boot_screen_matches_golden() {
    for (standard, name) in [
        (VideoStandard::Ntsc, "boot-ready-ntsc"),
        (VideoStandard::Pal, "boot-ready-pal"),
    ] {
        let mut machine = boot(BootOptions {
            basic: true,
            standard,
        })
        .unwrap();
        run_frames(&mut machine, BOOT_FRAMES);
        assert!(has_row(&machine, "Ready"), "{name}: the Ready prompt shows");
        if let Err(error) = compare_golden(name, &machine) {
            panic!("{name}: {error}");
        }
    }
}

/// Verifies REQ-026: the reference files of the repository are 384 x 240 RGB images, and
/// the comparison uses `crates/harness/golden/` and `target/golden-failures/` (criteria 1
/// and 2).
#[test]
fn golden_paths_and_reference_files() {
    let paths = GoldenPaths::from_env();
    assert!(
        paths.reference_dir.ends_with(GOLDEN_DIR),
        "reference directory"
    );
    assert!(
        paths.failure_dir.ends_with(FAILURE_DIR),
        "failure directory"
    );
    assert_eq!(
        FAILURE_DIR, "target/golden-failures",
        "failure directory name"
    );
    for name in ["boot-ready-ntsc", "boot-ready-pal"] {
        let (width, height, rgb) = load_png(&paths.reference_dir.join(format!("{name}.png")))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!((width, height), (384, 240), "{name}: size");
        assert_eq!(rgb.len(), RGB_FRAME_BYTES, "{name}: bytes");
    }
}

/// Verifies REQ-026: an equal frame passes; a difference of one channel in one pixel fails,
/// and writes `<name>-actual.png` and `<name>-diff.png` to the failure directory
/// (criteria 1 and 2).
#[test]
fn difference_writes_failure_images() {
    let dir = temp_dir("difference").unwrap();
    let paths = paths_in(&dir, false);
    let reference = test_frame();
    save_png(&paths.reference_dir.join("frame.png"), &reference).unwrap();
    compare_rgb("frame", &reference, &paths).unwrap();
    assert!(
        !paths.failure_dir.exists(),
        "no failure images for an equal frame"
    );

    let mut actual = reference.clone();
    let pixel = 1000 * 3 + 1; // the green channel of pixel 1000 (x = 232, y = 2)
    actual[pixel] = actual[pixel].wrapping_add(1);
    let error = compare_rgb("frame", &actual, &paths).unwrap_err();
    let actual_path = paths.failure_dir.join("frame-actual.png");
    let diff_path = paths.failure_dir.join("frame-diff.png");
    assert_eq!(
        error,
        HarnessError::GoldenMismatch {
            name: "frame".to_owned(),
            pixels: 1,
            actual: actual_path.clone(),
            diff: diff_path.clone(),
        },
        "error"
    );
    assert!(
        error.to_string().contains("frame-diff.png"),
        "message: {error}"
    );
    let (_, _, written) = load_png(&actual_path).unwrap();
    assert!(written == actual, "the actual image is the actual frame");
    let (_, _, diff) = load_png(&diff_path).unwrap();
    assert_eq!(
        diff[3000..3003],
        [0xFF, 0x00, 0x00],
        "the differing pixel is red"
    );
    assert_eq!(
        diff[..3],
        [reference[0] / 4, reference[1] / 4, reference[2] / 4],
        "an equal pixel is dark"
    );
    let (_, _, unchanged) = load_png(&paths.reference_dir.join("frame.png")).unwrap();
    assert!(unchanged == reference, "the reference file did not change");
    fs::remove_dir_all(&dir).unwrap();
}

/// Verifies REQ-026: a reference of another size, and a missing reference, fail (criterion
/// 1).
#[test]
fn wrong_size_or_missing_reference_fails() {
    let (diff, pixels) = diff_image(&test_frame(), &[]);
    assert_eq!(pixels, 384 * 240, "every pixel differs");
    assert_eq!(diff.len(), RGB_FRAME_BYTES, "the diff has the frame size");

    let dir = temp_dir("missing").unwrap();
    let paths = paths_in(&dir, false);
    let error = compare_rgb("absent", &test_frame(), &paths).unwrap_err();
    assert!(
        matches!(&error, HarnessError::Missing { fetch_command, .. }
            if fetch_command.contains("UPDATE_GOLDEN=1")),
        "error: {error:?}"
    );
    assert!(
        paths.failure_dir.join("absent-actual.png").exists(),
        "the actual frame is written"
    );
    assert!(
        matches!(
            save_png(&dir.join("short.png"), &[0; 3]),
            Err(HarnessError::Write { .. })
        ),
        "a buffer of the wrong size is not written"
    );
    fs::remove_dir_all(&dir).unwrap();
}

/// Verifies REQ-026: with `UPDATE_GOLDEN=1` (`GoldenPaths::update`), the comparison writes
/// the reference PNG and passes, also over a reference that differs; the next comparison
/// without the update mode passes (criterion 3).
#[test]
fn update_mode_writes_reference() {
    let dir = temp_dir("update").unwrap();
    let frame = test_frame();
    let reference = dir.join("golden").join("frame.png");
    compare_rgb("frame", &frame, &paths_in(&dir, true)).unwrap();
    let (_, _, written) = load_png(&reference).unwrap();
    assert!(written == frame, "a new reference file");

    let mut changed = frame;
    changed[0] ^= 0xFF;
    compare_rgb("frame", &changed, &paths_in(&dir, true)).unwrap();
    let (_, _, written) = load_png(&reference).unwrap();
    assert!(written == changed, "the reference file is replaced");
    compare_rgb("frame", &changed, &paths_in(&dir, false)).unwrap();
    assert!(!dir.join("failures").exists(), "no failure images");
    fs::remove_dir_all(&dir).unwrap();
}

/// Verifies REQ-026: `frame_to_rgb` gives 3 bytes for each pixel of the frame, with the
/// palette of the machine standard.
#[test]
fn frame_to_rgb_uses_palette() {
    let machine = boot(BootOptions::default()).unwrap();
    let rgb = frame_to_rgb(&machine);
    assert_eq!(rgb.len(), RGB_FRAME_BYTES, "bytes");
    let color = machine.frame()[0];
    let [_, r, g, b] = palette::rgb(machine.standard(), color).to_be_bytes();
    assert_eq!(rgb[..3], [r, g, b], "first pixel");
}
