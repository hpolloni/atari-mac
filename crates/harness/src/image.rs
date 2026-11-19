//! Frame images and golden frame comparisons (REQ-026).
//!
//! A golden frame test compares the RGB image of a frame with a reference PNG file in
//! [`GOLDEN_DIR`]. A frame passes only when every pixel is equal. On a difference, the
//! comparison writes `<name>-actual.png` and `<name>-diff.png` to [`FAILURE_DIR`] and fails.
//! With `UPDATE_GOLDEN=1`, it writes the reference file and passes.

use std::env;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use atari_core::Machine;
use atari_core::gtia::{FRAME_HEIGHT, FRAME_WIDTH};
use atari_core::palette;

use crate::{HarnessError, data};

/// The directory of the reference PNG files, relative to the repository root.
pub const GOLDEN_DIR: &str = "crates/harness/golden";

/// The directory of the failure images, relative to the repository root.
pub const FAILURE_DIR: &str = "target/golden-failures";

/// The environment variable that selects the update mode: the value `1` writes the
/// reference files.
pub const UPDATE_VARIABLE: &str = "UPDATE_GOLDEN";

/// The command that writes the reference files.
pub const UPDATE_COMMAND: &str = "UPDATE_GOLDEN=1 cargo test -p atari-harness --test golden";

/// The number of bytes of one RGB frame: 3 bytes for each pixel.
pub const RGB_FRAME_BYTES: usize = FRAME_WIDTH * FRAME_HEIGHT * 3;

/// The color of a differing pixel in the difference image (red).
const DIFF_COLOR: [u8; 3] = [0xFF, 0x00, 0x00];

/// The last complete frame of `machine` as RGB bytes.
///
/// The frame has [`FRAME_WIDTH`] x [`FRAME_HEIGHT`] pixels, row by row, 3 bytes (red, green,
/// blue) for each pixel. Each pixel is [`palette::rgb`] of the frame buffer byte with the
/// standard of the machine.
#[must_use]
pub fn frame_to_rgb(machine: &Machine) -> Vec<u8> {
    let standard = machine.standard();
    machine
        .frame()
        .iter()
        .flat_map(|&color| {
            let [_, r, g, b] = palette::rgb(standard, color).to_be_bytes();
            [r, g, b]
        })
        .collect()
}

/// A write error for `path`.
fn write_error(path: &Path, message: &impl ToString) -> HarnessError {
    HarnessError::Write {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

/// A read error for `path`.
fn read_error(path: &Path, message: &impl ToString) -> HarnessError {
    HarnessError::Read {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

/// Writes `rgb` (one frame, [`RGB_FRAME_BYTES`] bytes) as an 8-bit RGB PNG file of
/// [`FRAME_WIDTH`] x [`FRAME_HEIGHT`] pixels. Creates the parent directory if necessary.
///
/// # Errors
///
/// [`HarnessError::Write`] if `rgb` has the wrong length or the file cannot be written.
pub fn save_png(path: &Path, rgb: &[u8]) -> Result<(), HarnessError> {
    if rgb.len() != RGB_FRAME_BYTES {
        return Err(write_error(
            path,
            &format!("{} RGB bytes, expected {RGB_FRAME_BYTES}", rgb.len()),
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| write_error(path, &e))?;
    }
    let file = File::create(path).map_err(|e| write_error(path, &e))?;
    let width = u32::try_from(FRAME_WIDTH).map_err(|e| write_error(path, &e))?;
    let height = u32::try_from(FRAME_HEIGHT).map_err(|e| write_error(path, &e))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| write_error(path, &e))?;
    writer
        .write_image_data(rgb)
        .map_err(|e| write_error(path, &e))?;
    writer.finish().map_err(|e| write_error(path, &e))
}

/// Reads an 8-bit RGB PNG file. Returns the width, the height and the RGB bytes.
///
/// # Errors
///
/// [`HarnessError::Read`] if the file cannot be read or is not an 8-bit RGB PNG file.
pub fn load_png(path: &Path) -> Result<(u32, u32, Vec<u8>), HarnessError> {
    let file = File::open(path).map_err(|e| read_error(path, &e))?;
    let mut reader = png::Decoder::new(BufReader::new(file))
        .read_info()
        .map_err(|e| read_error(path, &e))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| read_error(path, &"the image is too large"))?;
    let mut buffer = vec![0; size];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|e| read_error(path, &e))?;
    if info.color_type != png::ColorType::Rgb || info.bit_depth != png::BitDepth::Eight {
        return Err(read_error(path, &"not an 8-bit RGB image"));
    }
    buffer.truncate(info.buffer_size());
    Ok((info.width, info.height, buffer))
}

/// Where a golden comparison reads and writes its files, and whether it updates the
/// reference files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenPaths {
    /// The directory of the reference files `<name>.png`.
    pub reference_dir: PathBuf,
    /// The directory of the failure images `<name>-actual.png` and `<name>-diff.png`.
    pub failure_dir: PathBuf,
    /// Write the reference file instead of comparing.
    pub update: bool,
}

impl GoldenPaths {
    /// The directories of the repository ([`GOLDEN_DIR`], [`FAILURE_DIR`]). The update mode
    /// is on when the environment variable `UPDATE_GOLDEN` is `1`.
    #[must_use]
    pub fn from_env() -> Self {
        let root = data::repo_root();
        Self {
            reference_dir: root.join(GOLDEN_DIR),
            failure_dir: root.join(FAILURE_DIR),
            update: env::var(UPDATE_VARIABLE).is_ok_and(|value| value == "1"),
        }
    }
}

/// The difference image of `actual` and `reference`, and the number of differing pixels.
///
/// A differing pixel is red, an equal pixel is the actual pixel at a quarter of its
/// brightness. A reference of a different size differs in every pixel.
#[must_use]
pub fn diff_image(actual: &[u8], reference: &[u8]) -> (Vec<u8>, usize) {
    let same_size = actual.len() == reference.len();
    let mut differing = 0;
    let mut image = Vec::with_capacity(actual.len());
    for (index, pixel) in actual.chunks(3).enumerate() {
        let start = index * 3;
        let equal = same_size && reference.get(start..start + pixel.len()) == Some(pixel);
        if equal {
            image.extend(pixel.iter().map(|&channel| channel / 4));
        } else {
            differing += 1;
            image.extend(DIFF_COLOR.iter().take(pixel.len()));
        }
    }
    (image, differing)
}

/// Compares `rgb` (one frame, as [`frame_to_rgb`] gives it) with the reference file
/// `<name>.png` in `paths.reference_dir` (REQ-026).
///
/// With `paths.update`, it writes `rgb` as the reference file and passes. Otherwise the frame
/// passes only when every pixel is equal to the reference. On a difference, it writes the
/// actual frame and a difference image (see [`diff_image`]) as `<name>-actual.png` and
/// `<name>-diff.png` to `paths.failure_dir`. A missing reference file also writes the actual
/// frame.
///
/// # Errors
///
/// [`HarnessError::GoldenMismatch`] on a difference, [`HarnessError::Missing`] if the
/// reference file does not exist, [`HarnessError::Read`] or [`HarnessError::Write`] if a
/// file cannot be read or written.
pub fn compare_rgb(name: &str, rgb: &[u8], paths: &GoldenPaths) -> Result<(), HarnessError> {
    let reference_path = paths.reference_dir.join(format!("{name}.png"));
    if paths.update {
        return save_png(&reference_path, rgb);
    }
    let actual_path = paths.failure_dir.join(format!("{name}-actual.png"));
    if !reference_path.exists() {
        save_png(&actual_path, rgb)?;
        return Err(HarnessError::Missing {
            path: reference_path,
            fetch_command: UPDATE_COMMAND.to_owned(),
        });
    }
    let (width, height, reference) = load_png(&reference_path)?;
    let same_size = usize::try_from(width).is_ok_and(|w| w == FRAME_WIDTH)
        && usize::try_from(height).is_ok_and(|h| h == FRAME_HEIGHT);
    let (diff, pixels) = diff_image(rgb, if same_size { &reference } else { &[] });
    if pixels == 0 {
        return Ok(());
    }
    let diff_path = paths.failure_dir.join(format!("{name}-diff.png"));
    save_png(&actual_path, rgb)?;
    save_png(&diff_path, &diff)?;
    Err(HarnessError::GoldenMismatch {
        name: name.to_owned(),
        pixels,
        actual: actual_path,
        diff: diff_path,
    })
}

/// Compares the last frame of `machine` with `crates/harness/golden/<name>.png`, as
/// [`compare_rgb`] with [`GoldenPaths::from_env`] (REQ-026).
///
/// # Errors
///
/// As [`compare_rgb`].
pub fn compare_golden(name: &str, machine: &Machine) -> Result<(), HarnessError> {
    compare_rgb(name, &frame_to_rgb(machine), &GoldenPaths::from_env())
}
