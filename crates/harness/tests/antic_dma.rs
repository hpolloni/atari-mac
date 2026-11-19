//! ANTIC DMA system tests with the CPU (REQ-310).
//!
//! Method of `docs/design/antic.md`, "Test plan", as in `antic_wsync.rs`: the test runs one
//! machine cycle at a time and finds the cycle of a CPU write as the first cycle after which
//! the RAM byte has the new value. The program waits for frame 1, then puts the marker write
//! `STA $0601` on cycle 107 of scan line 31 with two `STA WSYNC`. The test checks the marker
//! position, so the count of the free cycles after it is known.

/// The tests and their helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod dma {
    use std::iter;

    use atari_core::Machine;
    use atari_harness::system::{BootOptions, boot_program};

    /// `WSYNC`.
    const WSYNC: u16 = 0xD40A;
    /// The RAM byte of the marker write.
    const MARKER: u16 = 0x0601;
    /// The RAM byte of the `STX` write.
    const TARGET: u16 = 0x0600;
    /// The display list address.
    const DLIST: u16 = 0x3000;
    /// The scan line of the marker write.
    const MARKER_LINE: u32 = 31;
    /// The scan line of the test.
    const TEST_LINE: u32 = 32;
    /// The cycles in which the program is expected to reach its last write.
    const LIMIT: u64 = 4 * 29_868;

    /// The scan line and the cycle of machine cycle `n` (NTSC).
    fn position(n: u64) -> (u32, u32) {
        let line = u32::try_from((n / 114) % 262).unwrap();
        let cycle = u32::try_from(n % 114).unwrap();
        (line, cycle)
    }

    /// Runs cycles until `addr` is not 0 in RAM. Returns the position of the write cycle.
    fn write_position(m: &mut Machine, addr: u16) -> (u32, u32) {
        while m.cycle_count() < LIMIT {
            let n = m.cycle_count();
            m.run_cycle();
            if m.peek_ram(addr) != 0 {
                return position(n);
            }
        }
        panic!("no write to ${addr:04X} in {LIMIT} cycles");
    }

    /// The 3 bytes of an absolute instruction.
    fn abs(opcode: u8, addr: u16) -> [u8; 3] {
        let [lo, hi] = addr.to_le_bytes();
        [opcode, lo, hi]
    }

    /// A test program: `setup` (register writes), the wait for scan line 30, the marker write on
    /// cycle 107 of scan line 31, `fill` free cycles (2-cycle `NOP`s and one 3-cycle `LDA $00`
    /// if `fill` is odd), then `tail`, then a `JMP` to itself. `X` is `$5A`.
    fn program(setup: &[(u16, u8)], fill: u32, tail: &[u8]) -> Vec<u8> {
        let mut code = Vec::new();
        for &(addr, value) in setup {
            code.extend([0xA9, value]); // LDA #value
            code.extend(abs(0x8D, addr)); // STA addr
        }
        code.extend([0xA2, 0x5A]); // LDX #$5A
        for vcount in [0x7C, 0x0F] {
            code.extend(abs(0xAD, 0xD40B)); // wait: LDA VCOUNT
            code.extend([0xC9, vcount]); // CMP #vcount
            code.extend([0xD0, 0xF9]); // BNE wait (7 bytes back)
        }
        code.extend(abs(0x8D, WSYNC)); // STA WSYNC: resume on cycle 105 of scan line 30
        code.extend(abs(0x8D, WSYNC)); // STA WSYNC on cycle 107: resume on scan line 31
        code.extend(abs(0x8D, MARKER)); // the marker write on cycle 107 of scan line 31
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

    /// The machine for `program`, with a display list of 24 blank scan lines and the mode line
    /// `ir` (with LMS `$4000`) at scan line 32.
    fn machine(code: &[u8], ir: u8) -> Machine {
        let mut m = boot_program(
            code,
            BootOptions {
                basic: false,
                ..BootOptions::default()
            },
        )
        .unwrap();
        let list = [0x70, 0x70, 0x70, ir | 0x40, 0x00, 0x40, 0x41, 0x00, 0x30];
        for (addr, byte) in (DLIST..).zip(list) {
            m.poke_ram(addr, byte);
        }
        m
    }

    /// The display list and `DMACTL` register writes.
    fn display_setup(dmactl: u8) -> Vec<(u16, u8)> {
        let [lo, hi] = DLIST.to_le_bytes();
        vec![(0xD402, lo), (0xD403, hi), (0xD400, dmactl)]
    }

    /// Runs `code` with the mode line `ir` at scan line 32. Checks the marker write and returns
    /// the position of the `STX $0600` write.
    fn stx_write(code: &[u8], ir: u8) -> (u32, u32) {
        let mut m = machine(code, ir);
        assert_eq!(
            write_position(&mut m, MARKER),
            (MARKER_LINE, 107),
            "the marker write"
        );
        write_position(&mut m, TARGET)
    }

    /// Verifies REQ-310: a CPU write waits for the DMA cycle (criterion 2). With `DMACTL` =
    /// `$00`, an `STA $0600` that would write on the refresh cycle 25 writes on cycle 26. On the
    /// first scan line of a normal mode E line, an `STA $0600` that would write on the `Bitmap`
    /// cycle 20 writes on cycle 21.
    #[test]
    fn cpu_write_waits_for_dma() {
        let stx = abs(0x8E, TARGET);
        // 28 free cycles from cycle 108 of scan line 31 reach cycle 21 of scan line 32, so the
        // `STX` runs on cycles 22, 23 and 24 and would write on cycle 25.
        assert_eq!(
            stx_write(&program(&[], 28, &stx), 0x0E),
            (TEST_LINE, 26),
            "DMACTL = $00: the write after the refresh cycle 25"
        );
        // The mode E line halts the CPU on cycles 1, 6 and 7 before cycle 20. 20 free cycles
        // reach cycle 16, so the `STX` runs on cycles 17, 18 and 19 and would write on cycle 20.
        assert_eq!(
            stx_write(&program(&display_setup(0x22), 20, &stx), 0x0E),
            (TEST_LINE, 21),
            "mode E: the write after the Bitmap cycle 20"
        );
    }
}
