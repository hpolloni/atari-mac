//! ANTIC NMI system tests: the OS vertical blank with `AltirraOS` (REQ-333), and the NMI
//! timing with the CPU (REQ-334, REQ-335).

use atari_harness::screen::has_row;
use atari_harness::system::{BOOT_FRAMES, BootOptions, boot, run_until};

/// The byte of the OS real-time clock `RTCLOK` (`$12-$14`) that the OS vertical blank
/// handler increments in each frame (REQ-333).
const RTCLOK_LOW: u16 = 0x14;

/// Verifies REQ-333: with `AltirraOS`, after the boot, the byte at `$14` increases by 1 in each
/// frame, for 10 frames (criterion 3).
#[test]
fn os_vbi_increments_rtclok() {
    let mut machine = boot(BootOptions::default()).unwrap();
    let frames = run_until(&mut machine, BOOT_FRAMES, |m| has_row(m, "Ready"));
    assert!(frames.is_some(), "no Ready row after {BOOT_FRAMES} frames");
    let mut last = machine.peek(RTCLOK_LOW);
    for frame in 0..10 {
        machine.run_frame();
        let now = machine.peek(RTCLOK_LOW);
        assert_eq!(
            now,
            last.wrapping_add(1),
            "frame {frame}: $14 increases by 1"
        );
        last = now;
    }
}

/// The tests of the NMI timing with the CPU (REQ-334, REQ-335) and their helpers. The
/// module has `cfg(test)`, so that the helpers can use `unwrap` as the tests do.
///
/// Method of `docs/design/antic.md`, "Test plan": the test runs one machine cycle at a time.
/// The position of a cycle is `cycle_count() % 114` (cycle) and `(cycle_count() / 114) % 262`
/// (scan line), read before the cycle runs. Each test program waits for two `VCOUNT` values,
/// then two `STA WSYNC` put a marker write on cycle 107 of the scan line before the test scan
/// line. The test checks the marker position, so the count of the free cycles after it is
/// known.
#[cfg(test)]
mod timing {
    use std::iter;

    use atari_core::{Machine, MachineConfig, VideoStandard};
    use atari_harness::system::{TEST_SAMPLE_RATE, test_os_rom};

    /// `WSYNC`.
    const WSYNC: u16 = 0xD40A;
    /// `NMIEN`.
    const NMIEN: u16 = 0xD40E;
    /// `NMIST` and `NMIRES`.
    const NMIST: u16 = 0xD40F;
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
    /// The display list address.
    const DLIST: u16 = 0x3000;
    /// The cycles in which the program is expected to reach its marker write.
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

    /// A test program: `setup` (register writes), `LDY #$C0`, the waits for `VCOUNT` =
    /// `waits[0]` and then `waits[1]`, two `STA WSYNC`, the marker write, `fill` free cycles
    /// (2-cycle `NOP`s and one 3-cycle `LDA $00` if `fill` is odd), then `tail`, then a `JMP`
    /// to itself.
    fn program(setup: &[(u16, u8)], waits: [u8; 2], fill: u32, tail: &[u8]) -> Vec<u8> {
        let mut code = Vec::new();
        for &(addr, value) in setup {
            code.extend([0xA9, value]); // LDA #value
            code.extend(abs(0x8D, addr)); // STA addr
        }
        code.extend([0xA0, 0xC0]); // LDY #$C0
        for vcount in waits {
            code.extend(abs(0xAD, 0xD40B)); // wait: LDA VCOUNT
            code.extend([0xC9, vcount]); // CMP #vcount
            code.extend([0xD0, 0xF9]); // BNE wait (7 bytes back)
        }
        code.extend(abs(0x8D, WSYNC)); // STA WSYNC
        code.extend(abs(0x8D, WSYNC)); // STA WSYNC: resume on the scan line of the marker
        code.extend(abs(0x8D, MARKER)); // the marker write on cycle 107
        let mut nops = fill;
        if fill % 2 == 1 {
            code.extend([0xA5, 0x00]); // LDA $00
            nops -= 3;
        }
        code.extend(iter::repeat_n(0xEA, usize::try_from(nops / 2).unwrap())); // NOP
        code.extend(tail);
        let idle = 0xC000 + u16::try_from(code.len()).unwrap();
        code.extend(abs(0x4C, idle)); // JMP idle
        code
    }

    /// The machine for `code`, with the handlers at `NMI_HANDLER` and `IRQ_HANDLER`, and a
    /// display list of 24 blank scan lines, a `$80` line (DLI) at scan line 32 and a JVB.
    fn machine(code: &[u8]) -> Machine {
        let mut rom = test_os_rom(code);
        let nmi: Vec<u8> = [abs(0xEE, NMI_COUNT), abs(0x8D, NMIST)]
            .concat()
            .into_iter()
            .chain([0x40])
            .collect();
        let irq: Vec<u8> = abs(0xEE, BRK_COUNT).into_iter().chain([0x40]).collect();
        for (handler, bytes) in [(NMI_HANDLER, nmi), (IRQ_HANDLER, irq)] {
            let start = usize::from(handler - 0xC000);
            rom[start..start + bytes.len()].copy_from_slice(&bytes);
        }
        for (vector, handler) in [(0xFFFA_u16, NMI_HANDLER), (0xFFFE, IRQ_HANDLER)] {
            let start = usize::from(vector - 0xC000);
            rom[start..start + 2].copy_from_slice(&handler.to_le_bytes());
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
        let list = [0x70, 0x70, 0x70, 0x80, 0x41, 0x00, 0x30];
        for (addr, byte) in (DLIST..).zip(list) {
            m.poke_ram(addr, byte);
        }
        m
    }

    /// Runs the machine of `code` to the marker write and checks that it is on cycle 107 of
    /// scan line `line`.
    fn to_marker(code: &[u8], line: u32) -> Machine {
        let mut m = machine(code);
        while m.peek_ram(MARKER) == 0 {
            let n = m.cycle_count();
            assert!(n < LIMIT, "no marker write in {LIMIT} cycles");
            m.run_cycle();
            if m.peek_ram(MARKER) != 0 {
                assert_eq!(position(n), (line, 107), "the marker write");
            }
        }
        m
    }

    /// The DLI program of REQ-334: `DMACTL` = `$20`, `NMIEN` = `nmien`, the marker on scan
    /// line 31, `fill` free cycles, then `tail` and 20 `NOP` instructions.
    fn dli_program(nmien: u8, fill: u32, tail: &[u8]) -> Vec<u8> {
        let [lo, hi] = DLIST.to_le_bytes();
        let setup = [(0xD402, lo), (0xD403, hi), (0xD400, 0x20), (NMIEN, nmien)];
        let tail: Vec<u8> = tail.iter().copied().chain([0xEA; 20]).collect();
        program(&setup, [0x7C, 0x0F], fill, &tail)
    }

    /// The position of the first NMI sequence after the marker of `code`: the cycle of its
    /// first stack write (sequence cycle 3, the first change of `S`) minus 2. Scan line 32
    /// has no DMA on cycles 2-24.
    fn nmi_start(code: &[u8]) -> (u32, u32) {
        let mut m = to_marker(code, 31);
        let s = m.cpu().registers().s;
        let count = m.peek_ram(NMI_COUNT);
        loop {
            let n = m.cycle_count();
            assert!(n < LIMIT, "no NMI sequence in {LIMIT} cycles");
            m.run_cycle();
            if m.cpu().registers().s != s {
                let (line, cycle) = position(n);
                // Run the handler: the sequence must be an NMI sequence.
                for _ in 0..30 {
                    m.run_cycle();
                }
                assert_eq!(m.peek_ram(NMI_COUNT), count.wrapping_add(1), "NMI handler");
                return (line, cycle - 2);
            }
        }
    }

    /// Verifies REQ-334: with `DMACTL` = `$20` and `NMIEN` = `$C0`, after an instruction `$80`,
    /// a `NOP` that starts on cycle 8 gives the NMI sequence start on cycle 10, and one on
    /// cycle 9 on cycle 11 (criterion 2); with `STA NMIEN` writing `$C0` on cycle 7 and `NOP`
    /// instructions on cycles 8, 10 and 12, the NMI sequence starts on cycle 12 (criterion 3).
    #[test]
    fn nmi_sequence_start_cycles() {
        // From cycle 108 of scan line 31, the free cycles are 108-113, 0 and 2-24 (the
        // display list fetch is on cycle 1). 13 free cycles put a `NOP` on cycle 8, 14 on 9.
        assert_eq!(nmi_start(&dli_program(0xC0, 13, &[])), (32, 10), "NOP on 8");
        assert_eq!(nmi_start(&dli_program(0xC0, 14, &[])), (32, 11), "NOP on 9");
        // 9 free cycles put `STY NMIEN` on cycles 4-7, then `NOP`s on 8, 10 and 12.
        let sty = abs(0x8C, NMIEN);
        assert_eq!(
            nmi_start(&dli_program(0x00, 9, &sty)),
            (32, 12),
            "enable on 7"
        );
    }

    /// The program of REQ-335: `DMACTL` = `$00`, `NMIEN` = `$40`, a write to `NMIRES` (the
    /// `BRK` is on scan line 248 of frame 0, before any VBI), the marker on scan line 247,
    /// `fill` free cycles, then `BRK`.
    fn brk_program(fill: u32) -> Vec<u8> {
        program(
            &[(0xD400, 0x00), (NMIEN, 0x40), (NMIST, 0x00)],
            [0x0F, 0x7B],
            fill,
            &[0x00, 0x00],
        )
    }

    /// Runs the program of a `BRK` with its opcode fetch on `cycle` of scan line 248 to scan
    /// line 247 of the next frame. Returns the runs of the NMI handler and of the `BRK`
    /// handler, and the `NMIST` value after each cycle of scan line 248.
    fn brk_on_248(cycle: u32) -> (u8, u8, Vec<u8>) {
        // From cycle 108 of scan line 247, the free cycles are 108-113 and 0-24.
        let code = brk_program(6 + cycle);
        let mut m = to_marker(&code, 247);
        let brk = 0xC000 + u16::try_from(code.len() - 5).unwrap();
        let (nmis, brks) = (m.peek_ram(NMI_COUNT), m.peek_ram(BRK_COUNT));
        let mut fetch = None;
        let mut nmist = Vec::new();
        loop {
            let n = m.cycle_count();
            m.run_cycle();
            let pos = position(n);
            if fetch.is_none() && m.cpu().registers().pc == brk + 1 {
                // The first change of `PC` past the `BRK` opcode is its opcode fetch.
                fetch = Some(pos);
            }
            if pos.0 == 248 {
                nmist.push(m.peek(NMIST));
            }
            if pos == (247, 0) {
                break;
            }
        }
        assert_eq!(fetch, Some((248, cycle)), "the BRK opcode fetch");
        (
            m.peek_ram(NMI_COUNT).wrapping_sub(nmis),
            m.peek_ram(BRK_COUNT).wrapping_sub(brks),
            nmist,
        )
    }

    /// Verifies REQ-335: with `DMACTL` = `$00` and `NMIEN` = `$40`, a `BRK` with its opcode
    /// fetch on cycle 4 of scan line 248 runs the `BRK` handler, the NMI handler does not run
    /// in that frame, and `NMIST` bit 6 reads 1 from cycle 7 of scan line 248 (criterion 1).
    #[test]
    fn brk_on_cycle_4_skips_vbi_handler() {
        let (nmis, brks, nmist) = brk_on_248(4);
        assert_eq!((nmis, brks), (0, 1), "NMI and BRK handler runs");
        assert_eq!(nmist.len(), 114, "scan line 248");
        for (cycle, value) in nmist.iter().enumerate() {
            assert_eq!(
                value & 0x40 != 0,
                cycle >= 7,
                "NMIST bit 6 on cycle {cycle}"
            );
        }
    }

    /// Verifies REQ-335: in the same setup, with the `BRK` opcode fetch on cycle 3 or on
    /// cycle 5, the NMI handler runs once in that frame (criterion 2).
    #[test]
    fn brk_on_cycle_3_or_5_keeps_vbi_handler() {
        let (nmis, brks, _) = brk_on_248(3);
        assert_eq!(
            (nmis, brks),
            (1, 1),
            "cycle 3: the BRK handler, then the NMI handler"
        );
        // On cycle 5, the NMI takes over the `BRK` sequence (REQ-122).
        let (nmis, brks, _) = brk_on_248(5);
        assert_eq!((nmis, brks), (1, 0), "cycle 5: the NMI handler only");
    }
}
