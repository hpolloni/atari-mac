//! ANTIC `WSYNC` system tests with the CPU (REQ-336).
//!
//! Method of `docs/design/antic.md`, "Test plan": the test runs one machine cycle at a time
//! and finds the cycle of a CPU write as the first cycle after which the RAM byte has the new
//! value. The position of a cycle is `cycle_count() % 114` (cycle) and
//! `(cycle_count() / 114) % 262` (scan line), read before the cycle runs.
//!
//! Each test program waits for frame 1, then for `VCOUNT` = `$0F` (scan line 30). Two
//! `STA WSYNC` put the marker write `STA $0601` on cycle 107 of scan line 31. The test checks
//! the marker position, so the count of the free cycles after it is known. Scan line 32 is
//! the first scan line of the display list instruction of the test, if there is one.

/// The tests and their helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod wsync {
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

    /// Runs the machine of `code` to the marker write and checks its position. Then finds the
    /// `STX $0600` at the end of `code`: returns the positions of its opcode fetch (the first
    /// cycle after which the CPU program counter is past the opcode) and of its write.
    fn stx_cycles(code: &[u8], ir: u8) -> ((u32, u32), (u32, u32)) {
        let mut m = machine(code, ir);
        let stx = 0xC000 + u16::try_from(code.len() - 6).unwrap();
        assert_eq!(
            write_position(&mut m, MARKER),
            (MARKER_LINE, 107),
            "the marker write"
        );
        let fetch = loop {
            let n = m.cycle_count();
            assert!(n < LIMIT, "no STX opcode fetch in {LIMIT} cycles");
            m.run_cycle();
            if m.cpu().registers().pc == stx + 1 {
                break position(n);
            }
        };
        (fetch, write_position(&mut m, TARGET))
    }

    /// `STA WSYNC`, then `STX $0600`.
    fn wsync_stx() -> Vec<u8> {
        let mut tail = abs(0x8D, WSYNC).to_vec();
        tail.extend(abs(0x8E, TARGET));
        tail
    }

    /// Verifies REQ-336: with `DMACTL` = `$00`, `STA WSYNC` with its write on cycle 50 lets the
    /// `STX` opcode fetch run on cycle 51, and the `STX` write is on cycle 107 (criterion 1).
    #[test]
    fn wsync_next_instruction_on_104() {
        // From cycle 108 of scan line 31, the free cycles are 108-113, 0-24, then 26-28, 30-32
        // and so on between the refresh cycles 25, 29, ..., 57. 46 free cycles reach cycle 44,
        // so `STA WSYNC` runs on cycles 46, 47, 48 and 50.
        let (fetch, write) = stx_cycles(&program(&[], 46, &wsync_stx()), 0x0E);
        assert_eq!(fetch, (TEST_LINE, 51), "the STX opcode fetch");
        assert_eq!(write, (TEST_LINE, 107), "the STX write");
    }

    /// Verifies REQ-336: with `DMACTL` = `$00`, `STA WSYNC` with its write on cycle 56, and the
    /// refresh DMA on cycle 57: the `STX` write is on cycle 108 (criterion 2).
    #[test]
    fn wsync_blocked_next_cycle_starts_on_105() {
        // 51 free cycles from cycle 108 reach cycle 51, so `STA WSYNC` runs on cycles 52, 54,
        // 55 and 56 (53 is a refresh cycle).
        let (fetch, write) = stx_cycles(&program(&[], 51, &wsync_stx()), 0x0E);
        assert_eq!(fetch, (TEST_LINE, 105), "the STX opcode fetch");
        assert_eq!(write, (TEST_LINE, 108), "the STX write");
    }

    /// Verifies REQ-336: on the first scan line of a mode 6 line with `DMACTL` = `$23`
    /// (character data fetch on cycle 105), `STA WSYNC` with its write on cycle 99: the `STX`
    /// write is on cycle 108 (criterion 3).
    #[test]
    fn wsync_delayed_by_playfield_105() {
        // The halt cycles of scan line 32 are in `halt_on_cycles_105_106_setups` (ANTIC unit
        // test). 6 free cycles on scan line 31 and 39 free cycles before cycle 92 on scan line
        // 32 make 45, so `STA WSYNC` runs on cycles 92, 95, 96 and 99.
        let (fetch, write) = stx_cycles(&program(&display_setup(0x23), 45, &wsync_stx()), 0x06);
        assert_eq!(fetch, (TEST_LINE, 100), "the STX opcode fetch");
        assert_eq!(write, (TEST_LINE, 108), "the STX write");
    }

    /// Verifies REQ-336: on the first scan line of a `$12` line with `DMACTL` = `$21` and
    /// `HSCROL` = 14 (refresh on cycle 105), `STA WSYNC` with its write on cycle 20: the `STX`
    /// write is on cycle 108 (criterion 4).
    #[test]
    fn wsync_delayed_by_refresh_105() {
        // Scan line 32 halts on cycles 1, 6 and 7 before cycle 25. 20 free cycles reach cycle
        // 16, so `STA WSYNC` runs on cycles 17-20.
        let mut setup = display_setup(0x21);
        setup.push((0xD404, 14));
        let (fetch, write) = stx_cycles(&program(&setup, 20, &wsync_stx()), 0x12);
        assert_eq!(fetch, (TEST_LINE, 21), "the STX opcode fetch");
        assert_eq!(write, (TEST_LINE, 108), "the STX write");
    }

    /// Verifies REQ-336: on the first scan line of a mode 2 line with `DMACTL` = `$23`
    /// (character data fetch on cycle 105 and refresh on cycle 106), `STA WSYNC` with its write
    /// on cycle 8: the `STX` write is on cycle 109 (criterion 4).
    #[test]
    fn wsync_delayed_by_refresh_106() {
        // Scan line 32 halts on cycles 1, 6 and 7. 8 free cycles reach cycle 2, so `STA WSYNC`
        // runs on cycles 3, 4, 5 and 8.
        let (fetch, write) = stx_cycles(&program(&display_setup(0x23), 8, &wsync_stx()), 0x02);
        assert_eq!(fetch, (TEST_LINE, 9), "the STX opcode fetch");
        assert_eq!(write, (TEST_LINE, 109), "the STX write");
    }
}

/// The planned tests (`docs/design/spec-gates.md`, "Planned test stubs"). The module has
/// `cfg(test)`, so that the helper can panic as the tests do.
#[cfg(test)]
mod stubs {
    /// Fails the calling test: a planned stub never counts as a pass.
    fn planned() {
        panic!("planned test: not written yet");
    }

    /// Verifies REQ-337: with `DMACTL` = `$00`, `STA WSYNC` with its write on cycle 104 of
    /// scan line N, then `STX $0600`: the `STX` opcode fetch is on cycle 105 of scan line N and
    /// the `STX` write on cycle 107 of scan line N + 1.
    #[test]
    #[ignore = "planned: SLICE-02"]
    fn wsync_write_on_104_next_line() {
        planned();
    }

    /// Verifies REQ-337: on the first scan line of a mode A line with `DMACTL` = `$23`,
    /// `INC WSYNC` with its write cycles on cycles 102 and 103, then `STX $0600`: the `STX`
    /// opcode fetch is on cycle 105 and the `STX` write on cycle 108.
    #[test]
    #[ignore = "planned: SLICE-02"]
    fn inc_wsync_ignores_dma() {
        planned();
    }

    /// Verifies REQ-337: with `DMACTL` = `$00`, `INC WSYNC` with its write cycles on cycles 103
    /// and 104 of scan line N, then `STX $0600`: the `STX` opcode fetch is on cycle 105 of
    /// scan line N and the `STX` write on cycle 107 of scan line N + 1.
    #[test]
    #[ignore = "planned: SLICE-02"]
    fn inc_wsync_on_103_104() {
        planned();
    }

    /// Verifies REQ-337: with `DMACTL` = `$00`, `STA WSYNC` with its write on cycle 113 of
    /// scan line N, then `STX $0600`: the `STX` opcode fetch is on cycle 0 of scan line N + 1
    /// and the `STX` write on cycle 107 of scan line N + 1.
    #[test]
    #[ignore = "planned: SLICE-02"]
    fn wsync_write_on_113() {
        planned();
    }
}
