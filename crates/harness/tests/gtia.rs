//! GTIA system tests on the full machine (REQ-400, REQ-402, REQ-408, REQ-435).
//!
//! A test runs a small 6502 program from a synthetic OS ROM ([`test_os_rom`]) and checks the
//! frame buffer or memory (`docs/design/gtia.md`, "Test plan").

/// The tests and their helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod system {
    use atari_core::gtia::{ConsoleKeys, FRAME_WIDTH};
    use atari_core::media::{CartKind, Cartridge};
    use atari_core::{JoystickState, Machine, MachineConfig, VideoStandard};
    use atari_harness::system::{TEST_SAMPLE_RATE, test_os_rom};

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
}
