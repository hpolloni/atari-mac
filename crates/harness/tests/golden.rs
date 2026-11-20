//! Golden frame tests: frame comparisons with reference PNG files (REQ-026).
//!
//! The reference files in `crates/harness/golden/` come from this emulator: run
//! `UPDATE_GOLDEN=1 cargo test -p atari-harness --test golden` to write them again, and
//! review each changed image before the commit. The tests of the comparison itself use
//! temporary directories, so that they never change the reference files.

use std::path::{Path, PathBuf};
use std::process;
use std::{env, fs, io};

use atari_core::{Machine, VideoStandard, palette};
use atari_harness::image::{
    FAILURE_DIR, GOLDEN_DIR, GoldenPaths, RGB_FRAME_BYTES, compare_golden, compare_rgb, diff_image,
    frame_to_rgb, load_png, save_png,
};
use atari_harness::screen::has_row;
use atari_harness::system::{BOOT_FRAMES, BootOptions, boot, boot_program, run_frames};
use atari_harness::{HarnessError, data};

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

// --- Test screens of ANTIC and GTIA modes (SLICE-02, "Done when" 1 and 2) ---------------

/// The address of the display list of a test screen (1K aligned: the display list counter
/// does not cross a 1K boundary).
const DL_ADDR: u16 = 0x2000;
/// The address of the character set of a test screen (`CHBASE` = `$30`).
const CHSET_ADDR: u16 = 0x3000;
/// The address of the screen data of a test screen. The memory scan counter does not cross
/// a 4K boundary, so the data must stay below `$5000`.
const DATA_ADDR: u16 = 0x4000;
/// `PMBASE` of the player/missile test (2K aligned for single line resolution).
const PM_ADDR: u16 = 0x5000;

// ANTIC and GTIA registers that the test screens write.
/// Register `HPOSP0`.
const HPOSP0: u16 = 0xD000;
/// Register `HPOSM0`.
const HPOSM0: u16 = 0xD004;
/// Register `SIZEP0`.
const SIZEP0: u16 = 0xD008;
/// Register `SIZEM`.
const SIZEM: u16 = 0xD00C;
/// Register `COLPM0`.
const COLPM0: u16 = 0xD012;
/// Register `COLPF0`.
const COLPF0: u16 = 0xD016;
/// Register `COLPF1`.
const COLPF1: u16 = 0xD017;
/// Register `COLPF2`.
const COLPF2: u16 = 0xD018;
/// Register `COLPF3`.
const COLPF3: u16 = 0xD019;
/// Register `COLBK`.
const COLBK: u16 = 0xD01A;
/// Register `PRIOR`.
const PRIOR: u16 = 0xD01B;
/// Register `GRACTL`.
const GRACTL: u16 = 0xD01D;
/// Register `DMACTL`.
const DMACTL: u16 = 0xD400;
/// Register `CHACTL`.
const CHACTL: u16 = 0xD401;
/// Register `DLISTL`.
const DLISTL: u16 = 0xD402;
/// Register `DLISTH`.
const DLISTH: u16 = 0xD403;
/// Register `HSCROL`.
const HSCROL: u16 = 0xD404;
/// Register `VSCROL`.
const VSCROL: u16 = 0xD405;
/// Register `PMBASE`.
const PMBASE: u16 = 0xD407;
/// Register `CHBASE`.
const CHBASE: u16 = 0xD409;
/// Register `WSYNC`.
const WSYNC: u16 = 0xD40A;
/// Register `VCOUNT`.
const VCOUNT: u16 = 0xD40B;

/// Display list instruction bit: load memory scan.
const LMS: u8 = 0x40;
/// Display list instruction bit: vertical scrolling.
const VSCROLL: u8 = 0x20;
/// Display list instruction bit: horizontal scrolling.
const HSCROLL: u8 = 0x10;

/// The low and high bytes of `addr`.
const fn le(addr: u16) -> [u8; 2] {
    addr.to_le_bytes()
}

/// The bytes of one mode line of `mode` at normal playfield width (HRM 4.4 and 4.5).
const fn line_bytes(mode: u8) -> usize {
    match mode {
        0x6 | 0x7 | 0xA | 0xB | 0xC => 20,
        0x8 | 0x9 => 10,
        _ => 40,
    }
}

/// A display list at [`DL_ADDR`] and its screen data from [`DATA_ADDR`].
#[derive(Default)]
struct Screen {
    dl: Vec<u8>,
    data: Vec<u8>,
}

impl Screen {
    /// The address of the next screen data byte.
    fn next_addr(&self) -> u16 {
        DATA_ADDR + len16(self.data.len())
    }

    /// `lines` blank scan lines (1 to 8).
    fn blank(&mut self, lines: u8) {
        self.dl.push((lines - 1) << 4);
    }

    /// A mode line with LMS: instruction `ir` (mode and scroll bits) shows `row`.
    fn lms_line(&mut self, ir: u8, row: &[u8]) {
        let [lo, hi] = le(self.next_addr());
        self.dl.extend([ir | LMS, lo, hi]);
        self.data.extend_from_slice(row);
    }

    /// A band of mode lines of `mode`, one for each row: LMS on the first line, the memory
    /// scan counter continues for the others. Each row has the size of the mode line.
    fn band(&mut self, mode: u8, rows: &[Vec<u8>]) {
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), line_bytes(mode), "mode {mode:X} row size");
            if i == 0 {
                self.lms_line(mode, row);
            } else {
                self.dl.push(mode);
                self.data.extend_from_slice(row);
            }
        }
    }

    /// `lines` mode lines of `mode` that all show `row`: each line has LMS to the same
    /// address.
    fn repeat(&mut self, mode: u8, row: &[u8], lines: usize) {
        assert_eq!(row.len(), line_bytes(mode), "mode {mode:X} row size");
        let [lo, hi] = le(self.next_addr());
        for _ in 0..lines {
            self.dl.extend([mode | LMS, lo, hi]);
        }
        self.data.extend_from_slice(row);
    }

    /// Ends the display list with a jump and wait for vertical blank to [`DL_ADDR`], and
    /// writes the display list and the data to RAM.
    fn install(mut self, machine: &mut Machine) {
        let [lo, hi] = le(DL_ADDR);
        self.dl.extend([0x41, lo, hi]);
        assert!(
            self.dl.len() <= 0x400,
            "the display list stays in its 1K block"
        );
        assert!(self.data.len() <= 0x1000, "the data stays in its 4K block");
        poke(machine, DL_ADDR, &self.dl);
        poke(machine, DATA_ADDR, &self.data);
    }
}

/// `len` as a 16-bit value (`u16::MAX` if it is larger).
fn len16(len: usize) -> u16 {
    u16::try_from(len).unwrap_or(u16::MAX)
}

/// Writes `bytes` to RAM from `addr`.
fn poke(machine: &mut Machine, addr: u16, bytes: &[u8]) {
    for (offset, &byte) in (0_u16..).zip(bytes) {
        machine.poke_ram(addr + offset, byte);
    }
}

/// The internal character codes of `text` (ASCII `$20`-`$7F`, HRM 4.4), each with the
/// bits of `bits` set.
fn chars(text: &str, bits: u8) -> Vec<u8> {
    text.bytes()
        .map(|c| if (0x20..=0x5F).contains(&c) { c - 0x20 } else { c } | bits)
        .collect()
}

/// `row` padded with spaces (code `$00`) to `len` bytes.
fn pad(mut row: Vec<u8>, len: usize) -> Vec<u8> {
    row.resize(len, 0);
    row
}

/// Appends a write of each value: `LDA #value; STA addr`.
fn stores(code: &mut Vec<u8>, writes: &[(u16, u8)]) {
    for &(addr, value) in writes {
        let [lo, hi] = le(addr);
        code.extend([0xA9, value, 0x8D, lo, hi]);
    }
}

/// A 6502 program for the test OS ROM (at `$C000`): `SEI`, `CLD`, a write of each `init`
/// value, then a loop over `raster`. Each raster entry waits for its `VCOUNT` value, then
/// for `WSYNC` (the CPU continues at cycle 105) and three `NOP`, so that the first write
/// comes in the horizontal blank at the start of the next scan line, left of the frame.
/// The entries must be in `VCOUNT` order. With no raster entry, the loop is a `JMP` to
/// itself.
fn program(init: &[(u16, u8)], raster: &[(u8, &[(u16, u8)])]) -> Vec<u8> {
    const ORIGIN: u16 = 0xC000;
    let mut code = vec![0x78, 0xD8]; // SEI; CLD
    stores(&mut code, init);
    let loop_addr = ORIGIN + len16(code.len());
    let [vlo, vhi] = le(VCOUNT);
    let [wlo, whi] = le(WSYNC);
    for &(vcount, writes) in raster {
        // LDA #vcount; wait: CMP VCOUNT; BNE wait; STA WSYNC; NOP; NOP; NOP
        code.extend([
            0xA9, vcount, 0xCD, vlo, vhi, 0xD0, 0xFB, 0x8D, wlo, whi, 0xEA, 0xEA, 0xEA,
        ]);
        stores(&mut code, writes);
    }
    let [lo, hi] = le(loop_addr);
    code.extend([0x4C, lo, hi]); // JMP loop
    code
}

/// The character set of the `AltirraOS` ROM (`$E000`-`$E3FF`), with two test glyphs: `$3E`
/// has the bit pairs 00, 01, 10, 11 in each row (`$1B`), and `$3F` is a checkerboard of
/// single pixels (`$AA`, `$55`).
fn test_charset() -> Result<Vec<u8>, HarnessError> {
    let os = data::os_rom()?;
    let mut set = os[0x2000..0x2400].to_vec();
    set[0x3E * 8..0x3F * 8].fill(0x1B);
    for (row, byte) in set[0x3F * 8..0x40 * 8].iter_mut().enumerate() {
        *byte = if row % 2 == 0 { 0xAA } else { 0x55 };
    }
    Ok(set)
}

/// Runs `code` from the test OS ROM on a machine of `standard`, with `screen`, the
/// [`test_charset`] at [`CHSET_ADDR`] and each `extra` block in RAM, for three frames, and
/// compares the last frame with the reference `name`.
///
/// # Errors
///
/// [`HarnessError::Missing`] without the ROMs; [`HarnessError::GoldenMismatch`] if the
/// frame differs.
fn golden_screen(
    name: &str,
    standard: VideoStandard,
    code: &[u8],
    screen: Screen,
    extra: &[(u16, Vec<u8>)],
) -> Result<(), HarnessError> {
    let mut machine = boot_program(
        code,
        BootOptions {
            basic: false,
            standard,
        },
    )?;
    screen.install(&mut machine);
    poke(&mut machine, CHSET_ADDR, &test_charset()?);
    for (addr, bytes) in extra {
        poke(&mut machine, *addr, bytes);
    }
    run_frames(&mut machine, 3);
    compare_golden(name, &machine)
}

/// The usual setup of a test screen: the display list, the character set, normal
/// inversion, and the colors PF0 red (`$46`), PF1 green with luminance `$A` (`$CA`), PF2
/// blue (`$94`), PF3 orange (`$2A`) and BAK dark gray (`$02`). `DMACTL` comes last.
fn base_init(dmactl: u8) -> Vec<(u16, u8)> {
    let [lo, hi] = le(DL_ADDR);
    vec![
        (DLISTL, lo),
        (DLISTH, hi),
        (CHBASE, le(CHSET_ADDR)[1]),
        (CHACTL, 0x02),
        (COLPF0, 0x46),
        (COLPF1, 0xCA),
        (COLPF2, 0x94),
        (COLPF3, 0x2A),
        (COLBK, 0x02),
        (DMACTL, dmactl),
    ]
}

/// The rows of a map mode band of `lines` mode lines: `even` and `odd` bytes on alternate
/// mode lines.
fn map_rows(mode: u8, lines: usize, even: u8, odd: u8) -> Vec<Vec<u8>> {
    (0..lines)
        .map(|i| vec![if i % 2 == 0 { even } else { odd }; line_bytes(mode)])
        .collect()
}

/// One band of each ANTIC mode 2 to F, with a blank scan line between bands.
fn antic_modes_screen() -> Screen {
    let mut s = Screen::default();
    s.blank(8);
    // Character modes: text, inverse text, lower case with descenders, and the test glyphs.
    let mut row2 = chars("MODE 2 ", 0);
    row2.extend(chars("INVERSE", 0x80));
    row2.extend(chars(" gjpqy ", 0));
    row2.extend([0x3F; 4]);
    row2.extend([0x3E; 4]);
    s.band(0x2, &[pad(row2, 40)]);
    s.blank(1);
    let mut row3 = chars("MODE 3 gjpqy Descenders ", 0);
    row3.extend(chars("INV", 0x80));
    s.band(0x3, &[pad(row3, 40)]);
    s.blank(1);
    for mode in [0x4, 0x5] {
        // Glyph $3E without and with bit 7: the pair 11 is PF2, then PF3.
        let mut row = chars(&format!("MODE {mode} "), 0);
        row.extend([0x3E; 8]);
        row.extend([0xBE; 8]);
        row.extend([0x3F; 4]);
        s.band(mode, &[pad(row, 40)]);
        s.blank(1);
    }
    for mode in [0x6, 0x7] {
        // Bits 6-7 of each name select PF0 to PF3.
        let mut row: Vec<u8> = chars(&format!("MODE{mode}"), 0)
            .into_iter()
            .zip([0x00, 0x40, 0x80, 0xC0, 0x00])
            .map(|(c, color)| c | color)
            .collect();
        row.push(0);
        row.extend([0x3E, 0x7E, 0xBE, 0xFE, 0x3F, 0x7F, 0xBF, 0xFF]);
        s.band(mode, &[pad(row, 20)]);
        s.blank(1);
    }
    // Map modes: the four color modes show BAK, PF0, PF1, PF2 forward and backward on
    // alternate mode lines; the two color modes show a checkerboard of single pixels. Each
    // band has 16 scan lines.
    let bands: [(u8, usize, u8, u8); 8] = [
        (0x8, 2, 0x1B, 0xE4),
        (0x9, 4, 0xAA, 0x55),
        (0xA, 4, 0x1B, 0xE4),
        (0xB, 8, 0xAA, 0x55),
        (0xC, 16, 0xAA, 0x55),
        (0xD, 8, 0x1B, 0xE4),
        (0xE, 16, 0x1B, 0xE4),
        (0xF, 16, 0xAA, 0x55),
    ];
    for (i, (mode, lines, even, odd)) in bands.into_iter().enumerate() {
        if i > 0 {
            s.blank(1);
        }
        s.band(mode, &map_rows(mode, lines, even, odd));
    }
    s
}

/// The ANTIC mode screen in `standard`, compared with the reference `name`.
fn check_antic_modes(standard: VideoStandard, name: &str) -> Result<(), HarnessError> {
    let code = program(&base_init(0x22), &[]);
    golden_screen(name, standard, &code, antic_modes_screen(), &[])
}

/// Verifies REQ-026: a band of each ANTIC mode 2 to F (text, inverse text, descenders,
/// multicolor characters, the four color and two color map modes, mode F single pixels)
/// in NTSC is equal to the reference PNG.
#[test]
fn golden_antic_modes_ntsc() {
    if let Err(error) = check_antic_modes(VideoStandard::Ntsc, "antic-modes-ntsc") {
        panic!("antic-modes-ntsc: {error}");
    }
}

/// Verifies REQ-026: the screen of `golden_antic_modes_ntsc` in PAL is equal to the
/// reference PNG.
#[test]
fn golden_antic_modes_pal() {
    if let Err(error) = check_antic_modes(VideoStandard::Pal, "antic-modes-pal") {
        panic!("antic-modes-pal: {error}");
    }
}

/// Verifies REQ-026: modes 2 and D, each without scrolling, with `HSCROL` = 2 and with
/// `VSCROL` = 1 (three lines with LMS each), in NTSC, are equal to the reference PNG.
#[test]
fn golden_fine_scroll() {
    // One source row for each mode, 48 bytes (a horizontally scrolled normal line fetches
    // a wide line). The text and the marks show the shift.
    let mut text = chars("<>HSCROL 0123456789 ABCDEFGHIJKLMNOPQRSTUVWX", 0);
    text.extend(chars("YZ", 0x80));
    let text = pad(text, 48);
    let map: Vec<u8> = (0..48)
        .map(|i| if i % 8 == 0 { 0xFF } else { 0x1B })
        .collect();

    let mut s = Screen::default();
    s.blank(8);
    for (mode, row) in [(0x2, &text), (0xD, &map)] {
        // Without scrolling, then horizontally scrolled, then vertically scrolled. In the
        // vertical region, the first line starts at row VSCROL and the third line (bit 5
        // clear) ends at row VSCROL (HRM 4.7, figure 3).
        for ir in [mode, mode | HSCROLL, mode | VSCROLL] {
            for i in 0..3 {
                s.lms_line(if i == 2 { ir & !VSCROLL } else { ir }, row);
            }
            s.blank(4);
        }
    }
    let mut init = vec![(HSCROL, 2), (VSCROL, 1)];
    init.extend(base_init(0x22));
    let code = program(&init, &[]);
    if let Err(error) = golden_screen("fine-scroll", VideoStandard::Ntsc, &code, s, &[]) {
        panic!("fine-scroll: {error}");
    }
}

/// Verifies REQ-026: the four players and the four missiles with sizes normal, double and
/// quad and their own colors, with `PRIOR` `$01`, `$02`, `$04` and `$08` in four bands,
/// single line resolution in the upper half and double line resolution in the lower half
/// (NTSC), are equal to the reference PNG. In each band, each player shows a box with a
/// cross twice: once over the background, and once over a mode D playfield of BAK, PF0, PF1
/// and PF2 stripes, where the priority shows.
#[test]
fn golden_players_missiles() {
    /// The offset of the double line resolution data (PMBASE `$58`).
    const DOUBLE: usize = 0x800;
    /// The first scan line of each box with a cross. The boxes are 8 scan lines high in
    /// single line resolution (the first four) and 16 in double line resolution. The first
    /// box of each band is over the background, the second over the stripes.
    const BOXES: [usize; 8] = [32, 48, 80, 96, 128, 148, 176, 196];
    /// The scan lines of the stripes (start, end exclusive): 4 or more lines above and below
    /// each second box.
    const STRIPES: [(usize, usize); 4] = [(44, 60), (92, 108), (144, 168), (192, 216)];

    // Playfield: 96 mode D lines (scan lines 24 to 215), each of 4 color clock stripes BAK,
    // PF0, PF1, PF2 (from color clock `$30`) in the `STRIPES` ranges, or of BAK.
    let mut s = Screen::default();
    s.blank(8);
    s.blank(8);
    let stripes: Vec<u8> = [0x00, 0x55, 0xAA, 0xFF]
        .into_iter()
        .cycle()
        .take(40)
        .collect();
    let rows: Vec<Vec<u8>> = (0..96)
        .map(|i| {
            let line = 24 + 2 * i;
            if STRIPES.iter().any(|&(a, b)| (a..b).contains(&line)) {
                stripes.clone()
            } else {
                vec![0; 40]
            }
        })
        .collect();
    s.band(0xD, &rows);

    // Each player: a box with a cross (8 bytes) at each line of `BOXES`. Single line
    // resolution (PMBASE `$50`): one byte for each scan line. Double line resolution (PMBASE
    // `$58`): one byte for two scan lines, at index line / 2. The missiles (2 bits each, all
    // bits set) are bars at the same lines as the boxes.
    let shape = [0xFF, 0xC3, 0xA5, 0x99, 0x99, 0xA5, 0xC3, 0xFF];
    let mut pm = vec![0_u8; 0xC00];
    for (i, &first) in BOXES.iter().enumerate() {
        for (row, &byte) in shape.iter().enumerate() {
            if i < 4 {
                let line = first + row;
                pm[0x300 + line] = 0xFF;
                for n in 0..4 {
                    pm[0x400 + n * 0x100 + line] = byte;
                }
            } else {
                let index = first / 2 + row;
                pm[DOUBLE + 0x180 + index] = 0xFF;
                for n in 0..4 {
                    pm[DOUBLE + 0x200 + n * 0x80 + index] = byte;
                }
            }
        }
    }
    let [_, single_base] = le(PM_ADDR);
    let double_base = single_base + 0x08;

    let mut init = base_init(0x3E);
    init.extend([
        (COLPF0, 0x42),
        (COLPF1, 0x74),
        (COLPF2, 0xB4),
        (COLBK, 0x00),
        (COLPM0, 0x36),
        (COLPM0 + 1, 0x88),
        (COLPM0 + 2, 0xDA),
        (COLPM0 + 3, 0x1E),
        (PMBASE, single_base),
        (GRACTL, 0x03),
        // P0 normal ($40-$47), P1 double ($50-$5F), P2 quad ($68-$87), P3 double
        // ($90-$9F): no player overlaps another.
        (HPOSP0, 0x40),
        (HPOSP0 + 1, 0x50),
        (HPOSP0 + 2, 0x68),
        (HPOSP0 + 3, 0x90),
        (SIZEP0, 0),
        (SIZEP0 + 1, 1),
        (SIZEP0 + 2, 3),
        (SIZEP0 + 3, 1),
        // M0 normal ($A8-$A9), M1 double ($AE-$B1), M2 quad ($B6-$BD), M3 normal
        // ($C4-$C5).
        (HPOSM0, 0xA8),
        (HPOSM0 + 1, 0xAE),
        (HPOSM0 + 2, 0xB6),
        (HPOSM0 + 3, 0xC4),
        (SIZEM, 0x34),
    ]);
    // The bands start at scan lines 9, 73, 121 and 169 (VCOUNT * 2 + 1).
    let raster: [(u8, &[(u16, u8)]); 4] = [
        (4, &[(PRIOR, 0x01), (DMACTL, 0x3E), (PMBASE, single_base)]),
        (36, &[(PRIOR, 0x02)]),
        (60, &[(PRIOR, 0x04), (DMACTL, 0x2E), (PMBASE, double_base)]),
        (84, &[(PRIOR, 0x08)]),
    ];
    let code = program(&init, &raster);
    if let Err(error) = golden_screen(
        "players-missiles",
        VideoStandard::Ntsc,
        &code,
        s,
        &[(PM_ADDR, pm)],
    ) {
        panic!("players-missiles: {error}");
    }
}

/// Verifies REQ-026: a mode F playfield with the pixel values 0 to 15 (two GTIA pixels
/// each), in four bands: `PRIOR` `$00` (mode F), `$40` (GTIA mode 9, hue of BAK `$90`),
/// `$80` (mode 10, the nine color registers) and `$C0` (mode 11, luminance of BAK `$06`),
/// in NTSC, is equal to the reference PNG.
#[test]
fn golden_gtia_modes() {
    let mut s = Screen::default();
    s.blank(8);
    let row: Vec<u8> = (0_u8..16).map(|v| v * 0x11).cycle().take(40).collect();
    // Four bands of 40 scan lines: scan lines 16-55, 56-95, 96-135 and 136-175.
    s.repeat(0xF, &row, 160);
    let mut init = base_init(0x22);
    init.extend([
        (COLPM0, 0x36),
        (COLPM0 + 1, 0x88),
        (COLPM0 + 2, 0xDA),
        (COLPM0 + 3, 0x1E),
    ]);
    let raster: [(u8, &[(u16, u8)]); 4] = [
        (4, &[(PRIOR, 0x00), (COLBK, 0x02)]),
        (27, &[(PRIOR, 0x40), (COLBK, 0x90)]),
        (47, &[(PRIOR, 0x80), (COLBK, 0x02)]),
        (67, &[(PRIOR, 0xC0), (COLBK, 0x06)]),
    ];
    let code = program(&init, &raster);
    if let Err(error) = golden_screen("gtia-modes", VideoStandard::Ntsc, &code, s, &[]) {
        panic!("gtia-modes: {error}");
    }
}

/// Verifies REQ-026: a scene with two paddles and a ball (NTSC) is equal to the reference
/// PNG. Over a black background, a mode D playfield shows a top wall and a bottom wall
/// (4 scan lines each, the normal playfield width, color clocks `$30-$CF`) and a dashed
/// center line (color clocks `$7F-$80`, 4 scan lines on, 4 off). Player 0 (`HPOSP0` `$38`)
/// and player 1 (`HPOSP1` `$C6`) are paddles of 2 color clocks by 32 scan lines, mirrored
/// about the center, and missile 2 (`HPOSM2` `$83`) is a ball of 2 color clocks by 4 scan
/// lines at the vertical center, 4 color clocks right of the horizontal center. Single line
/// resolution, all objects color `$0E`.
#[test]
fn golden_pong_scene() {
    /// The first scan line of the top wall (24 to 27). The field between the walls is scan
    /// lines 28 to 211, and the bottom wall is scan lines 212 to 215.
    const TOP_WALL: usize = 24;
    /// The mode D lines (2 scan lines each) between the walls.
    const FIELD_LINES: usize = 92;
    /// The first scan line of each paddle: 32 scan lines (104 to 135), centered about the
    /// field center (between scan lines 119 and 120).
    const PADDLE: usize = 104;
    /// The first scan line of the ball: 4 scan lines (118 to 121), centered about the same
    /// point.
    const BALL: usize = 118;

    // Playfield: two blank groups (scan lines 8 to 23), the top wall (2 mode D lines of
    // PF0), the field (mode D lines off, on, on, off for each 8 scan lines, so that the
    // dashes are at 30-33, 38-41, ... 206-209), and the bottom wall.
    let mut s = Screen::default();
    s.blank(8);
    s.blank(8);
    let wall = vec![0x55; 40];
    // Pixels 79 and 80 (color clocks $7F and $80) are PF0: bits 1-0 of byte 19 and bits
    // 7-6 of byte 20.
    let mut dash = vec![0; 40];
    dash[19] = 0x01;
    dash[20] = 0x40;
    let mut rows = vec![wall.clone(), wall.clone()];
    rows.extend((0..FIELD_LINES).map(|i| {
        if matches!(i % 4, 1 | 2) {
            dash.clone()
        } else {
            vec![0; 40]
        }
    }));
    rows.extend([wall.clone(), wall]);
    assert_eq!(
        TOP_WALL + 2 * rows.len(),
        216,
        "the bottom wall ends at 215"
    );
    s.band(0xD, &rows);

    // Single line resolution (PMBASE `$50`): missiles at `$300`, player n at `$400` + n *
    // `$100`, one byte for each scan line.
    let mut pm = vec![0_u8; 0x800];
    for line in PADDLE..PADDLE + 32 {
        pm[0x400 + line] = 0xC0;
        pm[0x500 + line] = 0xC0;
    }
    for line in BALL..BALL + 4 {
        // Missile 2: bits 5-4.
        pm[0x300 + line] = 0x30;
    }
    let [_, pm_base] = le(PM_ADDR);

    let mut init = base_init(0x3E);
    init.extend([
        (COLPF0, 0x0E),
        (COLBK, 0x00),
        (COLPM0, 0x0E),
        (COLPM0 + 1, 0x0E),
        (COLPM0 + 2, 0x0E),
        (PMBASE, pm_base),
        (GRACTL, 0x03),
        (PRIOR, 0x00),
        // P0 at color clocks $38-$39 and P1 at $C6-$C7: 8 color clocks from each playfield
        // edge, mirrored about the center $80 ($80 - $38 = $C8 - $80).
        (HPOSP0, 0x38),
        (HPOSP0 + 1, 0xC6),
        (SIZEP0, 0),
        (SIZEP0 + 1, 0),
        // M2 at color clocks $83-$84, normal width.
        (HPOSM0 + 2, 0x83),
        (SIZEM, 0x00),
    ]);
    let code = program(&init, &[]);
    if let Err(error) = golden_screen(
        "pong-scene",
        VideoStandard::Ntsc,
        &code,
        s,
        &[(PM_ADDR, pm)],
    ) {
        panic!("pong-scene: {error}");
    }
}
