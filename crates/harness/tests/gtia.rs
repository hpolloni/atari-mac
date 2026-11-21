//! GTIA system tests on the full machine (REQ-400, REQ-402, REQ-408, REQ-435, REQ-438).
//!
//! A test runs a small 6502 program from a synthetic OS ROM ([`test_os_rom`]) and checks the
//! frame buffer or memory (`docs/design/gtia.md`, "Test plan").

/// The tests and their helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod system {
    use std::iter::repeat_n;

    use atari_core::gtia::{ConsoleKeys, FRAME_WIDTH};
    use atari_core::media::{CartKind, Cartridge};
    use atari_core::{JoystickState, Machine, MachineConfig, VideoStandard};
    use atari_harness::input::{KEY_DOWN_FRAMES, type_text};
    use atari_harness::screen::has_row;
    use atari_harness::system::{
        BOOT_FRAMES, BootOptions, TEST_SAMPLE_RATE, boot, run_until, test_os_rom,
    };

    /// The display list address.
    const DLIST: u16 = 0x3000;
    /// The screen memory address.
    const SCREEN: u16 = 0x4000;

    /// A machine with `code` (followed by a `JMP` to itself) in the OS ROM, a BASIC ROM of `$BB`
    /// bytes and `cartridge`, after power-on.
    fn machine(code: &[u8], cartridge: Option<Cartridge>) -> Machine {
        let mut program = code.to_vec();
        let [lo, hi] = (0xC000 + u16::try_from(program.len()).unwrap()).to_le_bytes();
        program.extend([0x4C, lo, hi]);
        Machine::new(MachineConfig {
            os_rom: test_os_rom(&program),
            basic_rom: Some(vec![0xBB; 0x2000]),
            cartridge,
            disk: None,
            fast_sio: false,
            fast_tape: false,
            tape: None,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap()
    }

    /// Runs `n` machine cycles.
    fn run_cycles(m: &mut Machine, n: u32) {
        for _ in 0..n {
            m.run_cycle();
        }
    }

    /// Runs `n` frames.
    fn run_frames(m: &mut Machine, n: u32) {
        for _ in 0..n {
            m.run_frame();
        }
    }

    /// `LDA #value`, `STA addr`.
    fn store(addr: u16, value: u8) -> Vec<u8> {
        let [lo, hi] = addr.to_le_bytes();
        vec![0xA9, value, 0x8D, lo, hi]
    }

    /// `LDA addr`, `ORA #$80`, `STA to`: a CPU read of `addr`, kept in RAM at `to` with bit 7
    /// set, so that the test sees that the code ran.
    fn copy(addr: u16, to: u16) -> Vec<u8> {
        let [lo, hi] = addr.to_le_bytes();
        let [to_lo, to_hi] = to.to_le_bytes();
        vec![0xAD, lo, hi, 0x09, 0x80, 0x8D, to_lo, to_hi]
    }

    /// `LDA addr`: one CPU read.
    fn read(addr: u16) -> Vec<u8> {
        let [lo, hi] = addr.to_le_bytes();
        vec![0xAD, lo, hi]
    }

    /// The code that sets the `PORTB` pins to `value` (output register first, so that the OS
    /// ROM stays on).
    fn set_portb(value: u8) -> Vec<u8> {
        let mut code = store(0xD303, 0x04);
        code.extend(store(0xD301, value));
        code.extend(store(0xD303, 0x00));
        code.extend(store(0xD301, 0xFF));
        code.extend(store(0xD303, 0x04));
        code
    }

    /// The code that sets the display list address to `DLIST`.
    fn set_dlist() -> Vec<u8> {
        let [lo, hi] = DLIST.to_le_bytes();
        let mut code = store(0xD402, lo);
        code.extend(store(0xD403, hi));
        code
    }

    /// Puts a display list with 24 blank scan lines, one mode line `ir` with LMS `SCREEN` (scan
    /// line 32, row 24 of the frame) and a JVB into RAM, and `screen` at `SCREEN`.
    fn put_display(m: &mut Machine, ir: u8, screen: &[u8]) {
        let [lo, hi] = SCREEN.to_le_bytes();
        let [dl_lo, dl_hi] = DLIST.to_le_bytes();
        let list = [0x70, 0x70, 0x70, ir | 0x40, lo, hi, 0x41, dl_lo, dl_hi];
        for (addr, &byte) in (DLIST..).zip(&list) {
            m.poke_ram(addr, byte);
        }
        for (addr, &byte) in (SCREEN..).zip(screen) {
            m.poke_ram(addr, byte);
        }
    }

    /// The two frame buffer columns of HPOS `h` in frame row `row`.
    fn pixel(m: &Machine, row: usize, h: usize) -> [u8; 2] {
        let index = row * FRAME_WIDTH + 2 * (h - 0x20);
        [m.frame()[index], m.frame()[index + 1]]
    }

    /// Verifies REQ-400: `$D020-$D0FF` are mirrors of `$D000-$D01F` (criteria 1-4).
    #[test]
    fn registers_mirror_at_d020_d0ff() {
        // Players 0-3 and missiles 0-3 at HPOS $80 over a mode E line of PF2 bytes, so that each
        // of the 16 collision registers has a bit set. `COLBK` is written at its mirror `$D0FA`.
        let mut code = set_dlist();
        for n in 0..4 {
            code.extend(store(0xD000 + n, 0x80)); // HPOSPn
            code.extend(store(0xD004 + n, 0x80)); // HPOSMn
            code.extend(store(0xD00D + n, 0xFF)); // GRAFPn
        }
        code.extend(store(0xD011, 0xFF)); // GRAFM
        code.extend(store(0xD0FA, 0x34)); // COLBK at a mirror address
        code.extend(store(0xD400, 0x22)); // DMACTL
        let mut m = machine(&code, None);
        put_display(&mut m, 0x0E, &[0xFF; 40]);
        run_frames(&mut m, 3);
        for row in [0, 100, 239] {
            assert_eq!(
                pixel(&m, row, 0x40),
                [0x34; 2],
                "1: background in row {row}"
            );
        }
        for reg in 0xD000..=0xD00F {
            assert_ne!(m.peek(reg), 0, "4: a collision bit at ${reg:04X}");
        }
        let mirrors_equal = |m: &Machine, case: &str| {
            for addr in 0xD020..=0xD0FF_u16 {
                assert_eq!(
                    m.peek(addr),
                    m.peek(0xD000 + (addr & 0x1F)),
                    "{case}: ${addr:04X}"
                );
            }
        };
        mirrors_equal(&m, "4");
        for fire in [true, false] {
            m.set_joystick(
                0,
                JoystickState {
                    fire,
                    ..JoystickState::default()
                },
            );
            assert_eq!(m.peek(0xD0F0), u8::from(!fire), "2: $D0F0, fire {fire}");
            mirrors_equal(&m, "button");
        }
        for keys in 0..8_u8 {
            m.set_console_keys(ConsoleKeys {
                start: keys & 1 != 0,
                select: keys & 2 != 0,
                option: keys & 4 != 0,
            });
            assert_eq!(m.peek(0xD07F), m.peek(0xD01F), "3: $D07F, keys {keys}");
            mirrors_equal(&m, "console keys");
        }
    }

    /// Verifies REQ-402: the Reset key keeps the GTIA state: a collision bit and `COLBK`
    /// (criteria 6 and 7).
    #[test]
    fn reset_key_keeps_gtia_state() {
        // At the first start ($0600 = 0), players 0 and 1 overlap at HPOS $80 until scan line 224
        // (`VCOUNT` = $70), then their graphics are cleared. After the reset, the code writes no
        // GTIA register.
        let mut code = vec![
            0xAD, 0x00, 0x06, // LDA $0600
            0xD0, 0x00, // BNE idle (offset set below)
            0xEE, 0x00, 0x06, // INC $0600
        ];
        code.extend(store(0xD01A, 0x46)); // COLBK
        for n in 0..2 {
            code.extend(store(0xD000 + n, 0x80)); // HPOSPn
            code.extend(store(0xD00D + n, 0xFF)); // GRAFPn
        }
        let wait = code.len();
        code.extend(read(0xD40B)); // wait: LDA VCOUNT
        code.extend([0xC9, 0x70, 0xD0, 0xF9]); // CMP #$70, BNE wait
        assert_eq!(code.len() - wait, 7, "the branch goes back 7 bytes");
        code.extend(store(0xD00D, 0x00));
        code.extend(store(0xD00E, 0x00));
        code[4] = u8::try_from(code.len() - 5).unwrap();
        let mut m = machine(&code, None);
        run_frames(&mut m, 3);
        assert_eq!(m.peek(0xD00C), 0x02, "P0PL before the reset");
        assert_eq!(m.peek(0xD00D), 0x01, "P1PL before the reset");
        m.press_reset();
        run_frames(&mut m, 3);
        assert_eq!(m.peek_ram(0x0600), 1, "the code saw the warm start");
        assert_eq!(m.peek(0xD00C), 0x02, "6: P0PL after the reset");
        assert_eq!(m.peek(0xD00D), 0x01, "6: P1PL after the reset");
        for row in [0, 100, 239] {
            assert_eq!(
                pixel(&m, row, 0x40),
                [0x46; 2],
                "7: background in row {row}"
            );
        }
    }

    /// Verifies REQ-435: `TRIG2` reads `$01` after power-on, with both joystick buttons pressed,
    /// and with `GRACTL` = `$04` (criterion 2).
    #[test]
    fn trig2_always_inactive() {
        // The code reads TRIG2 at power-on, then waits until the test sets $0700, then reads
        // TRIG2 with GRACTL = $00 and with GRACTL = $04.
        let mut code = copy(0xD012, 0x0600);
        code.extend([0xAD, 0x00, 0x07, 0xF0, 0xFB]); // wait: LDA $0700, BEQ wait
        code.extend(copy(0xD012, 0x0601));
        code.extend(store(0xD01D, 0x04)); // GRACTL
        code.extend(copy(0xD012, 0x0602));
        let mut m = machine(&code, None);
        run_cycles(&mut m, 1_000);
        assert_eq!(m.peek_ram(0x0600), 0x81, "after power-on");
        let pressed = JoystickState {
            fire: true,
            ..JoystickState::default()
        };
        m.set_joystick(0, pressed);
        m.set_joystick(1, pressed);
        m.poke_ram(0x0700, 1);
        run_cycles(&mut m, 1_000);
        assert_eq!(m.peek_ram(0x0601), 0x81, "both buttons pressed");
        assert_eq!(m.peek_ram(0x0602), 0x81, "GRACTL = $04");
        assert_eq!(
            [m.peek(0xD010), m.peek(0xD011)],
            [0x00, 0x00],
            "TRIG0 and TRIG1 see the buttons"
        );
    }

    /// A `MaxFlash` 1Mbit cartridge (CAR type 41) with all bytes `$DD`.
    fn maxflash_fill() -> Cartridge {
        let data = vec![0xDD_u8; CartKind::MaxFlash1Mbit.rom_size()];
        let sum = data
            .iter()
            .fold(0_u32, |sum, &byte| sum.wrapping_add(u32::from(byte)));
        let mut file = b"CART".to_vec();
        file.extend(41_u32.to_be_bytes());
        file.extend(sum.to_be_bytes());
        file.extend([0; 4]);
        file.extend(data);
        Cartridge::from_car(&file).unwrap()
    }

    /// Verifies REQ-435: `TRIG3` follows the cartridge ROM at `$A000-$BFFF` (criteria 3-5).
    #[test]
    fn trig3_follows_cartridge() {
        let trig3 = copy(0xD013, 0x0600);
        let m = {
            let mut m = machine(
                &trig3,
                Some(Cartridge::from_raw(vec![0xCC; 0x2000]).unwrap()),
            );
            run_cycles(&mut m, 1_000);
            m
        };
        assert_eq!(m.peek_ram(0x0600), 0x81, "3: an 8K cartridge");
        for (portb, basic) in [(0xFD, "on"), (0xFF, "off")] {
            let mut code = set_portb(portb);
            code.extend(copy(0xD013, 0x0600));
            code.extend(copy(0xA000, 0x0601));
            let mut m = machine(&code, None);
            run_cycles(&mut m, 1_000);
            assert_eq!(m.peek_ram(0x0600), 0x80, "4: no cartridge, BASIC {basic}");
            let expected = if portb == 0xFD { 0xBB } else { 0x80 };
            assert_eq!(m.peek_ram(0x0601), expected, "4: $A000 with BASIC {basic}");
        }
        // The `MaxFlash` 1Mbit cartridge (REQ-608): `$D510` disables the ROM, `$D500` enables it.
        let mut code = copy(0xD013, 0x0600);
        code.extend(read(0xD510));
        code.extend(copy(0xD013, 0x0601));
        code.extend(read(0xD500));
        code.extend(copy(0xD013, 0x0602));
        let mut m = machine(&code, Some(maxflash_fill()));
        run_cycles(&mut m, 1_000);
        assert_eq!(m.peek_ram(0x0600), 0x81, "5: the cartridge is enabled");
        assert_eq!(
            m.peek_ram(0x0601),
            0x80,
            "5: the first read after the disable"
        );
        assert_eq!(
            m.peek_ram(0x0602),
            0x81,
            "5: the first read after the enable"
        );
    }

    /// Verifies REQ-408: a mode F line in high resolution colors (criterion 1). With `COLBK` =
    /// `$02`, `COLPF1` = `$CA`, `COLPF2` = `$94`, `PRIOR` = `$00`, normal width, `HSCROL` = 0
    /// and first byte `$AA`, columns 32-39 (HPOS `$30-$33`) are `$9A`, `$94`, `$9A`, `$94`,
    /// `$9A`, `$94`, `$9A`, `$94`.
    #[test]
    fn mode_f_hires_colors() {
        let mut code = set_dlist();
        code.extend(store(0xD01A, 0x02)); // COLBK
        code.extend(store(0xD017, 0xCA)); // COLPF1
        code.extend(store(0xD018, 0x94)); // COLPF2
        code.extend(store(0xD01B, 0x00)); // PRIOR
        code.extend(store(0xD404, 0x00)); // HSCROL
        code.extend(store(0xD400, 0x22)); // DMACTL
        let mut m = machine(&code, None);
        put_display(&mut m, 0x0F, &[0xAA]);
        run_frames(&mut m, 3);
        let row = 24;
        let columns = &m.frame()[row * FRAME_WIDTH + 32..row * FRAME_WIDTH + 40];
        assert_eq!(
            columns,
            [0x9A, 0x94, 0x9A, 0x94, 0x9A, 0x94, 0x9A, 0x94],
            "columns 32-39 of the mode F line"
        );
        assert_eq!(pixel(&m, row, 0x34), [0x94; 2], "byte 1 is $00: code %100");
        assert_eq!(pixel(&m, row - 1, 0x30), [0x02; 2], "the blank line above");
    }

    /// Verifies REQ-407: with `COLBK` = `$02`, `COLPF0-3` = `$14`, `$26`, `$38`, `$4A`,
    /// `PRIOR` = `$00`, `HSCROL` = 0 and an ANTIC mode E line at normal width with first byte
    /// `$1B`, HPOS `$30`, `$31`, `$32`, `$33` show `$02`, `$14`, `$26`, `$38` (criterion 2).
    #[test]
    fn mode_e_colors() {
        let mut code = set_dlist();
        for (reg, value) in [
            (0xD01A, 0x02), // COLBK
            (0xD016, 0x14), // COLPF0
            (0xD017, 0x26), // COLPF1
            (0xD018, 0x38), // COLPF2
            (0xD019, 0x4A), // COLPF3
            (0xD01B, 0x00), // PRIOR
            (0xD404, 0x00), // HSCROL
            (0xD400, 0x22), // DMACTL
        ] {
            code.extend(store(reg, value));
        }
        let mut m = machine(&code, None);
        put_display(&mut m, 0x0E, &[0x1B]);
        run_frames(&mut m, 3);
        let shown: Vec<[u8; 2]> = (0x30..=0x33).map(|h| pixel(&m, 24, h)).collect();
        assert_eq!(
            shown,
            [[0x02; 2], [0x14; 2], [0x26; 2], [0x38; 2]],
            "HPOS $30-$33 of the mode E line"
        );
    }

    /// Verifies REQ-414: with one-line P/M DMA in `DMACTL`, `GRACTL` = `$03` and player 0 data
    /// in memory that is `$80` for even scanlines and `$40` for odd scanlines, each even row
    /// shows `COLPM0` at HPOS `$80` only and each odd row at HPOS `$81` only (criterion 6).
    #[test]
    fn pm_dma_one_line_resolution() {
        let mut code = set_dlist();
        for (reg, value) in [
            (0xD000, 0x80), // HPOSP0
            (0xD004, 0x90), // HPOSM0
            (0xD012, 0x46), // COLPM0
            (0xD01A, 0x02), // COLBK
            (0xD01D, 0x03), // GRACTL
            (0xD407, 0x50), // PMBASE: one-line P/M data at $5000
            (0xD400, 0x3E), // DMACTL: normal width, missiles, players, one-line, DL
        ] {
            code.extend(store(reg, value));
        }
        let mut m = machine(&code, None);
        put_display(&mut m, 0x0E, &[0x00; 40]);
        for y in 0..=255_u16 {
            m.poke_ram(0x5300 + y, 0x00);
            m.poke_ram(0x5400 + y, if y % 2 == 0 { 0x80 } else { 0x40 });
        }
        run_frames(&mut m, 3);
        for row in 0..240 {
            let player: Vec<usize> = (0x22..=0xDD)
                .filter(|&h| pixel(&m, row, h) == [0x46; 2])
                .collect();
            // Row r is scan line r + 8, so even rows are even scan lines.
            let expected = if row % 2 == 0 { 0x80 } else { 0x81 };
            assert_eq!(player, [expected], "row {row}");
        }
    }

    /// Verifies REQ-428: ANTIC mode F and mode E lines with `PRIOR` = `$40`: a mode F first
    /// byte `$1F` shows `$01` at HPOS `$30-$31` and `$0F` at HPOS `$32-$33`, and a mode E line
    /// of `$FF` bytes shows `$0A` at HPOS `$30-$CF` (criteria 1 and 2).
    #[test]
    fn gtia_mode_pixels_from_antic() {
        let mut code = set_dlist();
        for (reg, value) in [
            (0xD01A, 0x00), // COLBK
            (0xD01B, 0x40), // PRIOR
            (0xD404, 0x00), // HSCROL
            (0xD400, 0x22), // DMACTL
        ] {
            code.extend(store(reg, value));
        }
        let mut m = machine(&code, None);
        // Mode F at scan line 32 (row 24), mode E at scan line 33 (row 25).
        let [lo, hi] = SCREEN.to_le_bytes();
        let [e_lo, e_hi] = (SCREEN + 0x100).to_le_bytes();
        let [dl_lo, dl_hi] = DLIST.to_le_bytes();
        let list = [
            0x70, 0x70, 0x70, 0x4F, lo, hi, 0x4E, e_lo, e_hi, 0x41, dl_lo, dl_hi,
        ];
        for (addr, &byte) in (DLIST..).zip(&list) {
            m.poke_ram(addr, byte);
        }
        m.poke_ram(SCREEN, 0x1F);
        for addr in SCREEN + 0x100..SCREEN + 0x100 + 40 {
            m.poke_ram(addr, 0xFF);
        }
        run_frames(&mut m, 3);
        assert_eq!(pixel(&m, 24, 0x30), [0x01; 2], "1: mode F HPOS $30");
        assert_eq!(pixel(&m, 24, 0x31), [0x01; 2], "1: mode F HPOS $31");
        assert_eq!(pixel(&m, 24, 0x32), [0x0F; 2], "1: mode F HPOS $32");
        assert_eq!(pixel(&m, 24, 0x33), [0x0F; 2], "1: mode F HPOS $33");
        for h in 0x30..=0xCF {
            assert_eq!(pixel(&m, 25, h), [0x0A; 2], "2: mode E HPOS ${h:02X}");
        }
    }

    /// The program of `mid_line_colbk_change`: `COLBK` = `$02`, a wait for scan line 100
    /// (`VCOUNT` = 50), `STA WSYNC`, one `LDA $80` (3 cycles) if `odd`, `nops` `NOP`
    /// instructions and `STX COLBK` with X = `$46`. Returns the code and the address after
    /// the `STX`.
    fn colbk_program(odd: bool, nops: usize) -> (Vec<u8>, u16) {
        let mut code = store(0xD01A, 0x02);
        code.extend([0xA2, 0x46]); // LDX #$46
        code.extend(read(0xD40B)); // wait: LDA VCOUNT
        code.extend([0xC9, 50, 0xD0, 0xF9]); // CMP #50, BNE wait
        code.extend([0x8D, 0x0A, 0xD4]); // STA WSYNC
        if odd {
            code.extend([0xA5, 0x80]); // LDA $80
        }
        code.extend(repeat_n(0xEA, nops));
        code.extend([0x8E, 0x1A, 0xD0]); // STX COLBK
        let after = 0xC000 + u16::try_from(code.len()).unwrap();
        (code, after)
    }

    /// Runs the machine until the `STX COLBK` of `colbk_program` is done, then to the end of
    /// the frame. Returns the scan line and cycle of the write.
    fn run_to_colbk_write(m: &mut Machine, after: u16) -> (u32, u32) {
        let mut cycles = 0;
        while !(m.cpu().at_boundary() && m.cpu().registers().pc == after) {
            m.run_cycle();
            cycles += 1;
            assert!(cycles < 200_000, "the program reaches the write");
        }
        // The write was the last cycle. ANTIC ends the frame in the cycle of scan line 248,
        // cycle 0: `(248 - line) * 114 - cycle` cycles later.
        let frames = m.frame_count();
        let mut k = 0_u32;
        while m.frame_count() == frames {
            m.run_cycle();
            k += 1;
        }
        let cycle = (114 - k % 114) % 114;
        (248 - (k + cycle) / 114, cycle)
    }

    /// Verifies REQ-434: a 6502 program writes `$46` to `COLBK` (old value `$02`) on machine
    /// cycle 30 of a scanline, and HPOS `$3A` of that row shows `$02` and HPOS `$3B` shows
    /// `$46` (criterion 7, on the full machine).
    #[test]
    fn mid_line_colbk_change() {
        let programs = (0..40).flat_map(|nops| [(false, nops), (true, nops)]);
        let found = programs.into_iter().find_map(|(odd, nops)| {
            let (code, after) = colbk_program(odd, nops);
            let mut m = machine(&code, None);
            let (line, cycle) = run_to_colbk_write(&mut m, after);
            (cycle == 30).then_some((m, line))
        });
        let (m, line) = found.expect("a NOP count that puts the write on cycle 30");
        let row = usize::try_from(line - 8).unwrap();
        assert_eq!(
            pixel(&m, row, 0x3A),
            [0x02; 2],
            "HPOS $3A of scan line {line}"
        );
        assert_eq!(
            pixel(&m, row, 0x3B),
            [0x46; 2],
            "HPOS $3B of scan line {line}"
        );
        assert_eq!(pixel(&m, row - 1, 0x80), [0x02; 2], "the scan line before");
        assert_eq!(pixel(&m, row + 1, 0x22), [0x46; 2], "the scan line after");
    }

    /// Verifies REQ-438: after boot to the BASIC prompt with all POKEY channels silent, the
    /// audio samples of the 3 frames in which `type_text("A")` presses the key contain at
    /// least two different values (criterion 3).
    #[test]
    fn key_click_makes_audio() {
        let mut m = boot(BootOptions::default()).unwrap();
        let ready = run_until(&mut m, BOOT_FRAMES, |m| has_row(m, "Ready"));
        assert!(ready.is_some(), "no BASIC prompt");
        // At the prompt, all POKEY channels are silent and the speaker does not change.
        run_frames(&mut m, 30);
        let mut idle = Vec::new();
        m.drain_audio(&mut idle);
        idle.clear();
        run_frames(&mut m, 3);
        m.drain_audio(&mut idle);
        let spread = |s: &[f32]| {
            let max = s.iter().copied().fold(f32::MIN, f32::max);
            let min = s.iter().copied().fold(f32::MAX, f32::min);
            max - min
        };
        assert!(spread(&idle) < 0.001, "idle audio spread {}", spread(&idle));
        // The samples of the 3 frames in which `type_text` holds the key down.
        let start = m.cycle_count();
        let frame = u64::from(VideoStandard::Ntsc.cycles_per_frame());
        let count = |cycles: u64| cycles * u64::from(TEST_SAMPLE_RATE) / 1_789_773;
        let down =
            usize::try_from(count(start + u64::from(KEY_DOWN_FRAMES) * frame) - count(start))
                .unwrap();
        let mut audio = Vec::new();
        type_text(&mut m, "A").unwrap();
        m.drain_audio(&mut audio);
        let pressed = &audio[..down];
        let first = pressed[0];
        assert!(
            pressed.iter().any(|s| s.to_bits() != first.to_bits()),
            "the samples of the key press are all equal"
        );
        assert!(
            spread(pressed) > 0.1,
            "key click spread {}",
            spread(pressed)
        );
    }
}
