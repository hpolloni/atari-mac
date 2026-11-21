//! Joint ANTIC and CPU system tests (REQ-334, OI-017).
//!
//! The joint test of OI-017 runs the four cases of the WP-5 measurement ("Context added")
//! with the real CPU and ANTIC in the machine cycle order of `docs/design/system.md`,
//! "Timing" (step 4a). The method is that of `antic_nmi.rs`: two `STA WSYNC` put a marker
//! write on cycle 107 of the scan line before the test scan line, then a known count of free
//! cycles follows.

/// The test and its helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod joint {
    use std::iter;

    use atari_core::{Machine, MachineConfig, VideoStandard};
    use atari_harness::system::{TEST_SAMPLE_RATE, test_os_rom};

    /// `WSYNC`.
    const WSYNC: u16 = 0xD40A;
    /// `NMIEN`.
    const NMIEN: u16 = 0xD40E;
    /// The RAM byte of the marker write.
    const MARKER: u16 = 0x0601;
    /// The RAM byte that the NMI handler increments.
    const NMI_COUNT: u16 = 0x0610;
    /// The RAM byte that the IRQ and `BRK` handler increments.
    const BRK_COUNT: u16 = 0x0611;
    /// The NMI handler: `INC NMI_COUNT`, `STA NMIRES`, `RTI`.
    const NMI_HANDLER: u16 = 0xF000;
    /// The IRQ and `BRK` handler: `INC BRK_COUNT`, `RTI`.
    const IRQ_HANDLER: u16 = 0xF100;
    /// The display list address: 24 blank scan lines, a `$80` line (DLI) at scan line 32.
    const DLIST: u16 = 0x3000;
    /// The cycles in which a program is expected to reach its marker write.
    const LIMIT: u64 = 4 * 29_868;

    /// The scan line and the cycle of machine cycle `n` (NTSC).
    fn position(n: u64) -> (u32, u32) {
        let line = u32::try_from((n / 114) % 262).unwrap();
        let cycle = u32::try_from(n % 114).unwrap();
        (line, cycle)
    }

    /// The 3 bytes of an absolute instruction.
    fn abs(opcode: u8, addr: u16) -> [u8; 3] {
        let [lo, hi] = addr.to_le_bytes();
        [opcode, lo, hi]
    }

    /// A test program: `setup` (register writes), `LDY #y`, the waits for `VCOUNT` =
    /// `waits[0]` and `waits[1]`, two `STA WSYNC`, the marker write, `fill` free cycles
    /// (`NOP`s and one `LDA $00` if `fill` is odd), `tail`, 20 `NOP`s and a `JMP` to itself.
    fn program(setup: &[(u16, u8)], y: u8, waits: [u8; 2], fill: u32, tail: &[u8]) -> Vec<u8> {
        let mut code = Vec::new();
        for &(addr, value) in setup {
            code.extend([0xA9, value]); // LDA #value
            code.extend(abs(0x8D, addr)); // STA addr
        }
        code.extend([0xA0, y]); // LDY #y
        for vcount in waits {
            code.extend(abs(0xAD, 0xD40B)); // wait: LDA VCOUNT
            code.extend([0xC9, vcount]); // CMP #vcount
            code.extend([0xD0, 0xF9]); // BNE wait
        }
        code.extend(abs(0x8D, WSYNC));
        code.extend(abs(0x8D, WSYNC));
        code.extend(abs(0x8D, MARKER));
        let mut nops = fill;
        if fill % 2 == 1 {
            code.extend([0xA5, 0x00]); // LDA $00
            nops -= 3;
        }
        code.extend(iter::repeat_n(0xEA, usize::try_from(nops / 2).unwrap()));
        code.extend(tail);
        code.extend([0xEA; 20]);
        let idle = 0xC000 + u16::try_from(code.len()).unwrap();
        code.extend(abs(0x4C, idle)); // JMP idle
        code
    }

    /// The machine for `code`, with the NMI and IRQ handlers and the display list.
    fn machine(code: &[u8]) -> Machine {
        let mut rom = test_os_rom(code);
        let nmi = [abs(0xEE, NMI_COUNT), abs(0x8D, 0xD40F)].concat();
        let handlers = [
            (NMI_HANDLER, [nmi.as_slice(), &[0x40]].concat()),
            (
                IRQ_HANDLER,
                [abs(0xEE, BRK_COUNT).as_slice(), &[0x40]].concat(),
            ),
            (0xFFFA, NMI_HANDLER.to_le_bytes().to_vec()),
            (0xFFFE, IRQ_HANDLER.to_le_bytes().to_vec()),
        ];
        for (addr, bytes) in handlers {
            let start = usize::from(addr - 0xC000);
            rom[start..start + bytes.len()].copy_from_slice(&bytes);
        }
        let mut m = Machine::new(MachineConfig {
            os_rom: rom,
            basic_rom: None,
            cartridge: None,
            disk: None,
            fast_sio: false,
            fast_tape: false,
            tape: None,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap();
        for (addr, byte) in (DLIST..).zip([0x70, 0x70, 0x70, 0x80, 0x41, 0x00, 0x30]) {
            m.poke_ram(addr, byte);
        }
        m
    }

    /// What happened after the marker, up to cycle 0 of scan line `line - 1` of the next
    /// frame.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Run {
        /// The position of the first interrupt or `BRK` sequence (its first stack write
        /// minus 2), if any.
        start: Option<(u32, u32)>,
        /// The `P` value that the first sequence pushed.
        pushed_p: Option<u8>,
        /// The runs of the NMI handler.
        nmis: u8,
        /// The runs of the IRQ and `BRK` handler.
        brks: u8,
    }

    /// Runs `code`, checks that the marker write is on cycle 107 of scan line `line - 1`, and
    /// observes the run until scan line `line - 1` of the next frame.
    fn run(code: &[u8], line: u32) -> Run {
        let mut m = machine(code);
        while m.peek_ram(MARKER) == 0 {
            let n = m.cycle_count();
            assert!(n < LIMIT, "no marker write in {LIMIT} cycles");
            m.run_cycle();
            if m.peek_ram(MARKER) != 0 {
                assert_eq!(position(n), (line - 1, 107), "the marker write");
            }
        }
        let s = m.cpu().registers().s;
        let (nmis, brks) = (m.peek_ram(NMI_COUNT), m.peek_ram(BRK_COUNT));
        let mut start = None;
        let mut pushed_p = None;
        loop {
            let n = m.cycle_count();
            m.run_cycle();
            let pos = position(n);
            let now = m.cpu().registers().s;
            if start.is_none() && now != s {
                start = Some((pos.0, pos.1 - 2));
            }
            if pushed_p.is_none() && now == s.wrapping_sub(3) {
                pushed_p = Some(m.peek_ram(0x0100 | u16::from(s.wrapping_sub(2))));
            }
            if pos == (line - 1, 0) {
                break;
            }
        }
        Run {
            start,
            pushed_p,
            nmis: m.peek_ram(NMI_COUNT).wrapping_sub(nmis),
            brks: m.peek_ram(BRK_COUNT).wrapping_sub(brks),
        }
    }

    /// The DLI program: `DMACTL` = `$20`, `NMIEN` = `nmien`, `Y` = `y`, scan line 32.
    fn dli(nmien: u8, y: u8, fill: u32, tail: &[u8]) -> Run {
        let [lo, hi] = DLIST.to_le_bytes();
        let setup = [(0xD402, lo), (0xD403, hi), (0xD400, 0x20), (NMIEN, nmien)];
        run(&program(&setup, y, [0x7C, 0x0F], fill, tail), 32)
    }

    /// Verifies REQ-334: with the real CPU, the NMI sequence of a DLI starts on cycle 10 after
    /// a `NOP` that starts on cycle 8, and on cycle 11 after one that starts on cycle 9; an
    /// `NMIEN` write of `$00` on cycle 8 stops the NMI and one on cycle 9 does not; a `BRK`
    /// on cycle 5 of scan line 248 is taken over by the VBI NMI (joint ANTIC and CPU test,
    /// OI-017, cases 1 to 4 of the WP-5 measurement).
    #[test]
    fn nmi_start_cycles_with_real_cpu() {
        // Cases 1 and 2: the free cycles of scan line 32 are 0 and 2-24 (the display list
        // fetch is on cycle 1). 13 free cycles from cycle 108 put a `NOP` on cycle 8, 14 on 9.
        let on_8 = dli(0xC0, 0, 13, &[]);
        assert_eq!(on_8.start, Some((32, 10)), "NOP on 8");
        assert_eq!(
            (on_8.nmis, on_8.brks),
            (2, 0),
            "NOP on 8: the DLI and the VBI"
        );
        assert_eq!(dli(0xC0, 0, 14, &[]).start, Some((32, 11)), "NOP on 9");
        // Case 4: `STY NMIEN` with `Y` = 0 on cycles 5-8 (10 free cycles) or 6-9 (11).
        let sty = abs(0x8C, NMIEN);
        let off_8 = dli(0xC0, 0x00, 10, &sty);
        assert_eq!((off_8.start, off_8.nmis), (None, 0), "disable on 8: no NMI");
        // The poll at the end of cycle 8 (the third cycle of `STY`) sees the level of cycle 8.
        let off_9 = dli(0xC0, 0x00, 11, &sty);
        assert_eq!(off_9.start, Some((32, 10)), "disable on 9: NMI");
        assert_eq!(off_9.nmis, 1, "disable on 9: one NMI");
        // Case 3: `DMACTL` = `$00`, `NMIEN` = `$40`, a `BRK` opcode fetch on cycle 5 of scan
        // line 248 (6 free cycles on scan line 247 and 0-4). The NMI takes over the `BRK`
        // sequence: the NMI handler runs, the `BRK` handler does not, and the pushed `P` has
        // bit 4 = 1 (REQ-122).
        let setup = [(0xD400, 0x00), (NMIEN, 0x40), (0xD40F, 0x00)];
        let brk = run(&program(&setup, 0, [0x0F, 0x7B], 11, &[0x00, 0x00]), 248);
        assert_eq!(brk.start, Some((248, 5)), "BRK sequence");
        assert_eq!(
            (brk.nmis, brk.brks),
            (1, 0),
            "BRK on 5: the NMI handler only"
        );
        assert_eq!(brk.pushed_p.map(|p| p & 0x10), Some(0x10), "pushed B");
    }
}
