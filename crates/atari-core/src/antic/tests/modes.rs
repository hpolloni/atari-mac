//! Blank and sync codes, the playfield codes and the character and mapped modes (REQ-306,
//! REQ-307, REQ-315 to REQ-318).

use super::{FONT, Rec, Rig, SCREEN, codes, cycles_of, on_line};
use crate::antic::DmaKind;
use crate::timing::{VBLANK_START_LINE, VideoStandard};

/// The 4 hi-res codes of a byte (mode 2, 3 and F, REQ-307).
fn hires(byte: u8) -> [u8; 4] {
    [6, 4, 2, 0].map(|shift| 0b100 | ((byte >> shift) & 3))
}

/// The 4 two-bit codes of a byte: `%00` = `BAK`, then `PF0`-`PF2`, or `PF3` for `%11` if
/// `pf3` (REQ-307, REQ-315, REQ-318).
fn two_bit(byte: u8, pf3: bool) -> [u8; 4] {
    [6, 4, 2, 0].map(|shift| match (byte >> shift) & 3 {
        0 => 0b000,
        3 if pf3 => 0b111,
        p => 0b100 | (p - 1),
    })
}

/// The 8 one-bit codes of a byte: 0 = `BAK`, 1 = `on`.
fn one_bit(byte: u8, on: u8) -> [u8; 8] {
    [7, 6, 5, 4, 3, 2, 1, 0].map(|bit| if (byte >> bit) & 1 == 1 { on } else { 0 })
}

/// A normal-width rig with one mode line `mode` (LMS `SCREEN`) on scan line 8 and the
/// bytes `screen` at `SCREEN`.
fn mode_rig(mode: u8, screen: &[u8]) -> Rig {
    let mut rig = Rig::with_dl(0x22, &[0x40 | mode, 0x00, 0x20, 0x41, 0x00, 0x10]);
    rig.poke(SCREEN, screen);
    rig
}

/// The font address of the first `CharacterData` fetch of each scan line.
fn first_font_reads(lines: &[Vec<Rec>]) -> Vec<Option<u16>> {
    lines
        .iter()
        .map(|line| {
            line.iter()
                .find(|r| r.out.dma == DmaKind::CharacterData)
                .and_then(|r| r.read)
        })
        .collect()
}

/// A display list with a mode 2 line on scan lines 40-47 and a mode A line on scan lines
/// 60-63.
const HBLANK_DL: [u8; 15] = [
    0x70, 0x70, 0x70, 0x70, 0x42, 0x00, 0x20, 0x70, 0x30, 0x4A, 0x00, 0x20, 0x41, 0x00, 0x10,
];

/// Verifies REQ-306: on scan line 41 of a mode 2 line, positions `$00-$21` and `$DE-$E3`
/// have code `%011`; on scan line 61 of a mode A line, the same positions have `%010`.
#[test]
fn hblank_code_shows_hires_mode() {
    let mut rig = Rig::with_dl(0x22, &HBLANK_DL);
    rig.fill(SCREEN, 48, 0xFF);
    let recs = rig.run_to(70, 0);
    assert_eq!(
        on_line(&recs, 40)[1].out.dma,
        DmaKind::DisplayList,
        "mode 2"
    );
    assert_eq!(
        on_line(&recs, 60)[1].out.dma,
        DmaKind::DisplayList,
        "mode A"
    );
    for (line, code) in [(41, 0b011), (61, 0b010)] {
        let line_codes = codes(&on_line(&recs, line));
        for x in (0x00..=0x21).chain(0xDE..=0xE3) {
            assert_eq!(line_codes[x], code, "scan line {line} position ${x:02X}");
        }
    }
}

/// The codes of each scan line of the frame after the start, by scan line, for a display
/// list `dl` with `DMACTL` = `dmactl`.
fn frame_codes(standard: VideoStandard, dmactl: u8, dl: &[u8]) -> Vec<(u32, [u8; 228])> {
    let mut rig = Rig::new(standard);
    rig.start_dl(dmactl, dl);
    let frame = rig.frame();
    let lines = standard.lines_per_frame();
    (0..lines)
        .map(|line| (line, codes(&on_line(&frame, line))))
        .collect()
}

/// Checks that each scan line in `lines` has `code` on all 228 color clocks.
fn check_lines(frame: &[(u32, [u8; 228])], lines: &[u32], code: u8, what: &str) {
    for &line in lines {
        let (_, line_codes) = frame[usize::try_from(line).unwrap()];
        assert!(
            line_codes.iter().all(|&c| c == code),
            "{what}: scan line {line}: {line_codes:?}"
        );
    }
}

/// The JVB display list: IR = `$41` at scan line 248.
const JVB: [u8; 3] = [0x41, 0x00, 0x10];

/// Verifies REQ-306: with IR = `$41` at scan line 248 (NTSC), every color clock of scan
/// lines 251-253 has code `%001`.
#[test]
fn vsync_lines_ntsc_251_to_253() {
    let frame = frame_codes(VideoStandard::Ntsc, 0x22, &JVB);
    check_lines(&frame, &[251, 252, 253], 0b001, "NTSC");
}

/// Verifies REQ-306: with IR = `$41` at scan line 248 (PAL), every color clock of scan
/// lines 275-277 has code `%001`.
#[test]
fn vsync_lines_pal_275_to_277() {
    let frame = frame_codes(VideoStandard::Pal, 0x22, &JVB);
    check_lines(&frame, &[275, 276, 277], 0b001, "PAL");
}

/// The vertical blank scan lines without the vertical sync lines.
fn blank_lines(standard: VideoStandard) -> Vec<u32> {
    let (sync, last) = match standard {
        VideoStandard::Ntsc => (251..=253, 261),
        VideoStandard::Pal => (275..=277, 311),
    };
    (VBLANK_START_LINE..=last)
        .chain(0..8)
        .filter(|line| !sync.contains(line))
        .collect()
}

/// Verifies REQ-306: with IR = `$41` at scan line 248, every color clock of the vertical
/// blank scan lines other than the vertical sync lines has code `%010` (NTSC and PAL).
#[test]
fn vblank_lines_send_hblank_code() {
    for standard in [VideoStandard::Ntsc, VideoStandard::Pal] {
        let frame = frame_codes(standard, 0x22, &JVB);
        check_lines(&frame, &blank_lines(standard), 0b010, "vertical blank");
    }
}

/// Verifies REQ-306: with IR = `$02` at scan line 248 and `DMACTL` = `$20`, the vertical
/// blank scan lines have code `%011`, and the vertical sync scan lines have `%001`.
#[test]
fn vblank_hires_ir_without_playfield_sends_011() {
    let mut dl = vec![0x42, 0x00, 0x20];
    dl.extend([0x02; 29]);
    dl.extend(JVB);
    let frame = frame_codes(VideoStandard::Ntsc, 0x20, &dl);
    check_lines(&frame, &blank_lines(VideoStandard::Ntsc), 0b011, "blank");
    check_lines(&frame, &[251, 252, 253], 0b001, "sync");
}

/// The codes of the first scan line of a normal-width mode line `mode` with all bytes
/// `byte`.
fn filled_line(mode: u8, byte: u8) -> [u8; 228] {
    let mut rig = mode_rig(mode, &[byte; 48]);
    codes(&rig.line())
}

/// Checks that positions `$30-$CF` repeat `group`.
fn check_groups(line: &[u8; 228], group: &[u8], what: &str) {
    for x in 0x30..=0xCF {
        let expected = group[(x - 0x30) % group.len()];
        assert_eq!(line[x], expected, "{what} position ${x:02X}");
    }
}

/// Verifies REQ-307: a normal-width mode F line with bytes `$1B` gives the codes `%100`,
/// `%101`, `%110` and `%111` in each group of 4 color clocks from `$30` to `$CF`.
#[test]
fn hires_mode_sends_pair_codes() {
    let line = filled_line(0x0F, 0x1B);
    check_groups(&line, &[0b100, 0b101, 0b110, 0b111], "mode F $1B");
}

/// Verifies REQ-307: a mode F line with bytes `$00`, and a mode 2 line with font data
/// `$00`, have code `%100` at every position from `$30` to `$CF`.
#[test]
fn hires_zero_pair_is_100() {
    check_groups(&filled_line(0x0F, 0x00), &[0b100], "mode F $00");
    check_groups(&filled_line(0x02, 0x00), &[0b100], "mode 2 font $00");
}

/// Verifies REQ-307: a normal-width mode E line with bytes `$1B` gives the codes `%000`,
/// `%100`, `%101` and `%110` in each group of 4 color clocks from `$30` to `$CF`.
#[test]
fn lores_background_is_000() {
    let line = filled_line(0x0E, 0x1B);
    check_groups(&line, &[0b000, 0b100, 0b101, 0b110], "mode E $1B");
}

/// Verifies REQ-307: on the mode F and mode E lines, positions `$22-$2F` and `$D0-$DD` have
/// code `%000`.
#[test]
fn border_is_background() {
    for mode in [0x0F, 0x0E] {
        let line = filled_line(mode, 0x1B);
        for x in (0x22..=0x2F).chain(0xD0..=0xDD) {
            assert_eq!(line[x], 0b000, "mode {mode:X} position ${x:02X}");
        }
    }
}

/// The codes at `$30-$33` of the first scan line of a mode `mode` line whose first
/// character is `name`, with the font byte `byte` on all rows of that name.
fn first_char(mode: u8, name: u8, byte: u8) -> [u8; 8] {
    let mut rig = mode_rig(mode, &[name]);
    let index = if matches!(mode, 6 | 7) {
        name & 0x3F
    } else {
        name & 0x7F
    };
    rig.fill(FONT + u16::from(index) * 8, 8, byte);
    let line = codes(&rig.line());
    let mut out = [0; 8];
    out.copy_from_slice(&line[0x30..0x38]);
    out
}

/// Verifies REQ-307 and REQ-315: in a mode 4 line, a character with name `$81` and font
/// byte `$FF` has code `%111` (`PF3`) on its 4 color clocks; with font byte `$1B` it has
/// the codes `%000`, `%100`, `%101` and `%111`.
#[test]
fn mode_4_bit7_selects_pf3() {
    assert_eq!(first_char(4, 0x81, 0xFF)[..4], [0b111; 4], "$81 with $FF");
    assert_eq!(
        first_char(4, 0x81, 0x1B)[..4],
        [0b000, 0b100, 0b101, 0b111],
        "$81 with $1B"
    );
}

/// Verifies REQ-315: in a mode 4 line, a character with name `$01` and font byte `$1B` has
/// the codes `%000`, `%100`, `%101` and `%110`, left to right.
#[test]
fn mode_4_color_pairs() {
    assert_eq!(
        first_char(4, 0x01, 0x1B)[..4],
        [0b000, 0b100, 0b101, 0b110],
        "$01 with $1B"
    );
}

/// Verifies REQ-315: with `CHBASE` = `$E0`, the row 0 font fetch of a mode 4 character with
/// name `$C1` reads `$E208`.
#[test]
fn mode_4_font_address() {
    let mut rig = mode_rig(4, &[0xC1]);
    let reads = first_font_reads(&[rig.line()]);
    assert_eq!(reads, [Some(0xE208)], "row 0 of $C1");
}

/// A different font byte for each name `k`.
fn font_byte(k: u8) -> u8 {
    k.wrapping_mul(37).wrapping_add(11)
}

/// A normal-width line of mode `mode` with the names `0..count` and the row 0 font byte
/// `font_byte(k)` for name k. Returns the codes of the first scan line.
fn names_line(mode: u8, count: u8) -> [u8; 228] {
    let names: Vec<u8> = (0..count).collect();
    let mut rig = mode_rig(mode, &names);
    for k in 0..count {
        rig.poke(FONT + u16::from(k) * 8, &[font_byte(k)]);
    }
    codes(&rig.line())
}

/// Verifies REQ-315: in a normal-width mode 4 line with names `$00-$27`, positions
/// `$30 + 4k` to `$33 + 4k` show the font byte of name k, for k = 0 to 39.
#[test]
fn mode_4_forty_characters() {
    let line = names_line(4, 40);
    for k in 0..40_u8 {
        let x = 0x30 + 4 * usize::from(k);
        assert_eq!(
            line[x..x + 4],
            two_bit(font_byte(k), false),
            "character {k}"
        );
    }
}

/// The font rows that `rows` scan lines of a mode `mode` line with name `$01` read.
fn font_rows(mode: u8, rows: usize) -> Vec<Option<u16>> {
    let mut rig = mode_rig(mode, &[0x01]);
    first_font_reads(&rig.lines(rows))
        .iter()
        .map(|read| read.map(|addr| addr - (FONT + 8)))
        .collect()
}

/// The expected font rows: `row >> shift` for each row.
fn expected_rows(rows: u16, shift: u16) -> Vec<Option<u16>> {
    (0..rows).map(|r| Some(r >> shift)).collect()
}

/// Verifies REQ-315: in mode 4, row r (0-7) reads font row r; in mode 5, row r (0-15)
/// reads font row r >> 1.
#[test]
fn mode_5_double_rows() {
    assert_eq!(font_rows(4, 8), expected_rows(8, 0), "mode 4");
    assert_eq!(font_rows(5, 16), expected_rows(16, 1), "mode 5");
}

/// Verifies REQ-316: in a mode 6 line, name `$01` with font byte `$80` has code `%100` on
/// its first color clock and `%000` on its other 7 color clocks.
#[test]
fn mode_6_zero_bit_is_background() {
    assert_eq!(
        first_char(6, 0x01, 0x80),
        [0b100, 0, 0, 0, 0, 0, 0, 0],
        "$01 with $80"
    );
}

/// Verifies REQ-316: with font byte `$80`, the names `$01`, `$41`, `$81` and `$C1` give
/// code `%100`, `%101`, `%110` and `%111` on the first color clock.
#[test]
fn mode_6_color_from_name_bits() {
    for (name, code) in [(0x01, 0b100), (0x41, 0b101), (0x81, 0b110), (0xC1, 0b111)] {
        assert_eq!(first_char(6, name, 0x80)[0], code, "name ${name:02X}");
    }
}

/// Verifies REQ-316: in a normal-width mode 6 line with names `$00-$13`, positions
/// `$30 + 8k` to `$37 + 8k` show the font byte of name k, for k = 0 to 19.
#[test]
fn mode_6_twenty_characters() {
    let line = names_line(6, 20);
    for k in 0..20_u8 {
        let x = 0x30 + 8 * usize::from(k);
        assert_eq!(
            line[x..x + 8],
            one_bit(font_byte(k), 0b100),
            "character {k}"
        );
    }
}

/// Verifies REQ-316: with `CHBASE` = `$E2` and name `$C1`, the row 0 font fetch reads
/// `$E208`.
#[test]
fn mode_6_font_address_512_aligned() {
    let mut rig = mode_rig(6, &[0xC1]);
    rig.write(0x09, 0xE2);
    let reads = first_font_reads(&[rig.line()]);
    assert_eq!(reads, [Some(0xE208)], "row 0 of $C1");
}

/// Verifies REQ-316: in mode 6, row r (0-7) reads font row r; in mode 7, row r (0-15)
/// reads font row r >> 1.
#[test]
fn mode_7_double_rows() {
    assert_eq!(font_rows(6, 8), expected_rows(8, 0), "mode 6");
    assert_eq!(font_rows(7, 16), expected_rows(16, 1), "mode 7");
}

/// The address of the `CharacterData` fetch on cycle 51 of the first scan line of a
/// normal-width mode 2 line, with a write of `$E4` to `CHBASE` on cycle `write_cycle`.
fn chbase_fetch_51(write_cycle: u32) -> u16 {
    let mut rig = mode_rig(2, &[0x01; 40]);
    rig.seek_after(8, write_cycle);
    rig.write(0x09, 0xE4);
    let recs = rig.run_to(9, 0);
    let fetch = recs[usize::try_from(51 - write_cycle - 1).unwrap()];
    assert_eq!(fetch.out.cycle, 51, "the record of cycle 51");
    assert_eq!(
        fetch.out.dma,
        DmaKind::CharacterData,
        "character data on 51"
    );
    fetch.read.unwrap()
}

/// Verifies REQ-317: in a normal-width mode 2 line with `CHBASE` = `$E0`, a write of `$E4`
/// on cycle 50 does not change the character data fetch on cycle 51 (it reads in
/// `$E000-$E3FF`).
#[test]
fn chbase_write_one_cycle_before_is_too_late() {
    let addr = chbase_fetch_51(50);
    assert!((0xE000..=0xE3FF).contains(&addr), "read ${addr:04X}");
}

/// Verifies REQ-317: a write of `$E4` to `CHBASE` on cycle 49 changes the character data
/// fetch on cycle 51 (it reads in `$E400-$E7FF`).
#[test]
fn chbase_takes_effect_after_two_cycles() {
    let addr = chbase_fetch_51(49);
    assert!((0xE400..=0xE7FF).contains(&addr), "read ${addr:04X}");
}

/// Verifies REQ-317: after a write of `$E2` to `CHBASE`, mode 2 name `$00` reads row 0 from
/// `$E000`, and in the next mode line, a mode 6 line, name `$00` reads row 0 from `$E200`
/// with no new write.
#[test]
fn chbase_bit1_latent_for_mode_6() {
    let mut rig = Rig::with_dl(0x22, &[0x42, 0x00, 0x20, 0x06, 0x41, 0x00, 0x10]);
    rig.write(0x09, 0xE2);
    let lines = rig.lines(9);
    let reads = first_font_reads(&lines);
    assert_eq!(reads[0], Some(0xE000), "mode 2 row 0");
    assert_eq!(reads[8], Some(0xE200), "mode 6 row 0");
    assert_eq!(rig.antic.debug_state().chbase, 0xE2, "CHBASE keeps bit 1");
}

/// Verifies REQ-318: in modes 8, A, D and E, a byte `$1B` gives the codes `%000`, `%100`,
/// `%101` and `%110`, with pixels 4 (mode 8), 2 (mode A) or 1 (modes D, E) color clocks
/// wide.
#[test]
fn mapped_two_bit_encoding() {
    for (mode, width) in [(0x08, 4), (0x0A, 2), (0x0D, 1), (0x0E, 1)] {
        let line = filled_line(mode, 0x1B);
        let group: Vec<u8> = [0b000, 0b100, 0b101, 0b110]
            .iter()
            .flat_map(|&code| [code].repeat(width))
            .collect();
        check_groups(&line, &group, &format!("mode {mode:X}"));
    }
}

/// Verifies REQ-318: in modes 9, B and C, a byte `$55` gives the codes `%000` and `%100` in
/// turn, with pixels 2 (mode 9) or 1 (modes B, C) color clocks wide.
#[test]
fn mapped_one_bit_encoding() {
    for (mode, width) in [(0x09, 2), (0x0B, 1), (0x0C, 1)] {
        let line = filled_line(mode, 0x55);
        let group: Vec<u8> = [0b000, 0b100]
            .iter()
            .flat_map(|&code| [code].repeat(width))
            .collect();
        check_groups(&line, &group, &format!("mode {mode:X}"));
    }
}

/// Verifies REQ-318: in mode F, a byte `$1B` gives the codes `%100`, `%101`, `%110` and
/// `%111`, each for 1 color clock.
#[test]
fn mode_f_hires_pairs() {
    let line = filled_line(0x0F, 0x1B);
    assert_eq!(hires(0x1B), [0b100, 0b101, 0b110, 0b111], "table");
    check_groups(&line, &hires(0x1B), "mode F");
}

/// Verifies REQ-318: a normal-width line fetches 10 bytes in modes 8 and 9, 20 in modes A
/// to C and 40 in modes D to F, and byte k starts at `$30 + 16k`, `$30 + 8k` or `$30 + 4k`.
#[test]
fn mapped_bytes_per_line() {
    for mode in 0x08..=0x0F_u8 {
        let (count, step) = match mode {
            8 | 9 => (10, 16),
            0x0A..=0x0C => (20, 8),
            _ => (40, 4),
        };
        let bytes: Vec<u8> = (0..48).map(font_byte).collect();
        let mut rig = mode_rig(mode, &bytes);
        let line = rig.line();
        let fetches = cycles_of(&line, DmaKind::Bitmap);
        assert_eq!(fetches.len(), count, "mode {mode:X} bytes");
        let line_codes = codes(&line);
        for k in 0..count {
            let byte = font_byte(u8::try_from(k).unwrap());
            let expected: Vec<u8> = match mode {
                8 => two_bit(byte, false).iter().flat_map(|&c| [c; 4]).collect(),
                9 => one_bit(byte, 0b100).iter().flat_map(|&c| [c; 2]).collect(),
                0x0A => two_bit(byte, false).iter().flat_map(|&c| [c; 2]).collect(),
                0x0B | 0x0C => one_bit(byte, 0b100).to_vec(),
                0x0F => hires(byte).to_vec(),
                _ => two_bit(byte, false).to_vec(),
            };
            let x = 0x30 + step * k;
            assert_eq!(
                line_codes[x..x + step],
                expected[..],
                "mode {mode:X} byte {k}"
            );
        }
    }
}
