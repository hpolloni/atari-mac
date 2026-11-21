//! ANTIC register system tests on the full machine (REQ-300, REQ-302).
//!
//! The position of the machine is `cycle_count() % 114` (cycle) and
//! `(cycle_count() / 114) % 262` (NTSC scan line), because ANTIC starts at scan line 0,
//! cycle 0 at power-on (`docs/design/antic.md`, "Test plan"). This is the next cycle to run.
//! A CPU read in a cycle runs after the ANTIC tick of that cycle, so `peek` after the cycle
//! gives the value of a CPU read in it.

/// The tests and their helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod registers {
    use atari_core::gtia::{FRAME_HEIGHT, FRAME_WIDTH};
    use atari_core::{Machine, MachineConfig, VideoStandard};
    use atari_harness::system::{BootOptions, TEST_SAMPLE_RATE, boot_program, test_os_rom};

    /// The cycles of one NTSC frame.
    const FRAME: u64 = 29_868;

    /// `VCOUNT`.
    const VCOUNT: u16 = 0xD40B;

    /// A machine with [`test_os_rom`] of `program` and no BASIC.
    fn machine(program: &[u8]) -> Machine {
        boot_program(
            program,
            BootOptions {
                basic: false,
                ..BootOptions::default()
            },
        )
        .unwrap()
    }

    /// Runs cycles until the next cycle to run is `cycle` of scan line `line`.
    fn run_to(m: &mut Machine, line: u64, cycle: u64) {
        let target = line * 114 + cycle;
        while m.cycle_count() % FRAME != target {
            m.run_cycle();
        }
    }

    /// Verifies REQ-300: on scan line 100, cycle 50, a read of `$D47B` returns `$32`, and a read
    /// of `$D40B` returns `$32` (criterion 1).
    #[test]
    fn vcount_mirror_d47b_reads_vcount() {
        let mut m = machine(&[0x4C, 0x00, 0xC0]); // JMP $C000
        run_to(&mut m, 100, 51);
        assert_eq!(m.peek(0xD47B), 0x32, "$D47B");
        assert_eq!(m.peek(VCOUNT), 0x32, "$D40B");
    }

    /// Verifies REQ-302: the Reset key resets ANTIC, but not its position: after a reset on scan
    /// line 128, cycle 50, `VCOUNT` reads `$40`, and it changes to `$41` on cycle 111 of scan
    /// line 129 (criterion 4).
    #[test]
    fn reset_key_keeps_vcount() {
        let mut m = machine(&[0x4C, 0x00, 0xC0]); // JMP $C000
        run_to(&mut m, 128, 50);
        let start = m.cycle_count();
        m.press_reset();
        assert_eq!(m.cycle_count(), start, "the reset runs no cycle");
        assert_eq!(m.peek(VCOUNT), 0x40, "VCOUNT after the reset");
        while m.peek(VCOUNT) == 0x40 {
            m.run_cycle();
        }
        assert_eq!(m.peek(VCOUNT), 0x41, "the next VCOUNT value");
        assert_eq!(
            (m.cycle_count() - 1) % FRAME,
            129 * 114 + 111,
            "the first read of $41 is on cycle 111 of scan line 129"
        );
    }

    /// The program of `press_reset`. At the first start, it sets a display list of mode 2 lines
    /// with DLI bits, `COLBK` = `$46`, `COLPF2` = `$94`, `NMIEN` = `$C0` and `DMACTL` = `$3E`.
    /// After a reset (`$0600` is not 0), it writes no register. The NMI handler at `$C040`
    /// increments `$0601`.
    const RESET_PROGRAM: [u8; 46] = [
        0xAD, 0x00, 0x06, // LDA $0600
        0xD0, 0x26, // BNE idle ($C02B)
        0xEE, 0x00, 0x06, // INC $0600
        0xA9, 0x00, 0x8D, 0x02, 0xD4, // LDA #$00, STA DLISTL
        0xA9, 0x30, 0x8D, 0x03, 0xD4, // LDA #$30, STA DLISTH
        0xA9, 0x46, 0x8D, 0x1A, 0xD0, // LDA #$46, STA COLBK
        0xA9, 0x94, 0x8D, 0x18, 0xD0, // LDA #$94, STA COLPF2
        0xA9, 0xC0, 0x8D, 0x0E, 0xD4, // LDA #$C0, STA NMIEN
        0xA9, 0x3E, 0x8D, 0x00, 0xD4, // LDA #$3E, STA DMACTL
        0xEA, 0xEA, 0xEA, 0xEA, 0xEA, // NOP padding
        0x4C, 0x2B, 0xC0, // idle: JMP idle
    ];

    /// The NMI handler: `INC $0601`, `RTI`.
    const NMI_HANDLER: [u8; 4] = [0xEE, 0x01, 0x06, 0x40];

    /// `true` if the last frame has `color` in any visible column.
    fn frame_has(m: &Machine, color: u8) -> bool {
        m.frame()
            .chunks(FRAME_WIDTH)
            .any(|row| row[4..380].contains(&color))
    }

    /// Verifies REQ-302: the Reset key clears `NMIEN` and `DMACTL` (criteria 1 and 2 on the full
    /// machine). A reset on scan line 100, cycle 50 stops the DLIs and the VBIs for the rest of
    /// the frame and the next frame, and the next frame has no playfield: all visible pixels are
    /// `COLBK`.
    #[test]
    fn press_reset() {
        let mut program = RESET_PROGRAM.to_vec();
        program.resize(0x40, 0xEA);
        program.extend(NMI_HANDLER);
        let mut os_rom = test_os_rom(&program);
        os_rom[0x3FFA..0x3FFC].copy_from_slice(&[0x40, 0xC0]); // NMI vector: $C040
        let mut m = Machine::new(MachineConfig {
            os_rom,
            basic_rom: None,
            cartridge: None,
            disk: None,
            fast_sio: false,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap();
        let mut list = vec![0x70, 0x70, 0x70, 0xC2, 0x00, 0x40];
        list.extend([0x82; 23]);
        list.extend([0x41, 0x00, 0x30]);
        for (addr, byte) in (0x3000_u16..).zip(list) {
            m.poke_ram(addr, byte);
        }
        for _ in 0..3 {
            m.run_frame();
        }
        assert!(m.peek_ram(0x0601) != 0, "NMIs before the reset");
        assert!(frame_has(&m, 0x94), "the playfield shows before the reset");
        run_to(&mut m, 100, 50);
        m.press_reset();
        let nmis = m.peek_ram(0x0601);
        m.run_frame();
        m.run_frame();
        assert_eq!(m.peek_ram(0x0601), nmis, "no NMI after the reset");
        assert_eq!(m.frame().len(), FRAME_WIDTH * FRAME_HEIGHT, "frame size");
        for (r, row) in m.frame().chunks(FRAME_WIDTH).enumerate() {
            assert!(
                row[4..380].iter().all(|&b| b == 0x46),
                "row {r}: background only, no playfield DMA"
            );
        }
    }
}
