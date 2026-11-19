//! Cartridge system tests (REQ-206, REQ-232, REQ-608).
//!
//! Method of `docs/design/media.md`, "System test method for cartridges": a synthetic OS ROM
//! ([`test_os_rom`]) runs the test code from `$C000`, then the test runs 10,000 cycles and
//! checks `peek` and `peek_ram`.

/// The tests and their helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod cartridges {
    use atari_core::media::{CartKind, Cartridge};
    use atari_core::{Machine, MachineConfig, VideoStandard};
    use atari_harness::system::{TEST_SAMPLE_RATE, test_os_rom};

    /// The cycles that a test runs.
    const CYCLES: u32 = 10_000;

    /// A machine with `code` (followed by a `JMP` to itself) in the OS ROM, a BASIC ROM of `$BB`
    /// bytes and `cartridge`, after 10,000 cycles.
    fn run(code: &[u8], cartridge: Option<Cartridge>, ram: &[(u16, u8)]) -> Machine {
        let mut program = code.to_vec();
        let [lo, hi] = (0xC000 + u16::try_from(program.len()).unwrap()).to_le_bytes();
        program.extend([0x4C, lo, hi]);
        let mut m = Machine::new(MachineConfig {
            os_rom: test_os_rom(&program),
            basic_rom: Some(vec![0xBB; 0x2000]),
            cartridge,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap();
        for &(addr, value) in ram {
            m.poke_ram(addr, value);
        }
        for _ in 0..CYCLES {
            m.run_cycle();
        }
        m
    }

    /// `LDA #value`, `STA addr`.
    fn store(addr: u16, value: u8) -> Vec<u8> {
        let [lo, hi] = addr.to_le_bytes();
        vec![0xA9, value, 0x8D, lo, hi]
    }

    /// The code that sets the `PORTB` pins to `value`. It writes the output register first
    /// (`PBCTL` = `$04`), then `DDRB` = `$FF` (`PBCTL` = `$00`), then `PBCTL` = `$04` again. With
    /// `DDRB` first, the output register `$00` of the reset would turn the OS ROM off.
    fn set_portb(value: u8) -> Vec<u8> {
        let mut code = store(0xD303, 0x04);
        code.extend(store(0xD301, value));
        code.extend(store(0xD303, 0x00));
        code.extend(store(0xD301, 0xFF));
        code.extend(store(0xD303, 0x04));
        code
    }

    /// `LDA addr`, `STA to`: a CPU read of `addr`, kept in RAM at `to`.
    fn copy(addr: u16, to: u16) -> Vec<u8> {
        let [lo, hi] = addr.to_le_bytes();
        let [to_lo, to_hi] = to.to_le_bytes();
        vec![0xAD, lo, hi, 0x8D, to_lo, to_hi]
    }

    /// `LDA addr`: one CPU read.
    fn read(addr: u16) -> Vec<u8> {
        let [lo, hi] = addr.to_le_bytes();
        vec![0xAD, lo, hi]
    }

    /// A fill cartridge: all bytes `$CC` (8 KB or 16 KB).
    fn fill_cartridge(size: usize) -> Cartridge {
        Cartridge::from_raw(vec![0xCC; size]).unwrap()
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
        let cart = Cartridge::from_car(&file).unwrap();
        assert_eq!(cart.kind(), CartKind::MaxFlash1Mbit, "CAR type 41");
        cart
    }

    /// Verifies REQ-206: a CPU write to cartridge ROM does not change the RAM under it, and a
    /// CPU read gives the cartridge byte (criteria 3 and 5).
    #[test]
    fn write_to_cart_rom_ignored() {
        let mut code = store(0xB000, 0x55);
        code.extend(copy(0xB000, 0x0600));
        let m = run(&code, Some(fill_cartridge(0x2000)), &[(0xB000, 0xAA)]);
        assert_eq!(m.peek_ram(0xB000), 0xAA, "3: RAM at $B000");
        assert_eq!(m.peek_ram(0x0600), 0xCC, "3: the CPU read of $B000");
        assert_eq!(m.peek(0xB000), 0xCC, "3: peek of $B000");

        let mut code = store(0x8000, 0x00);
        code.extend(copy(0x8000, 0x0600));
        let m = run(&code, Some(fill_cartridge(0x4000)), &[(0x8000, 0x77)]);
        assert_eq!(m.peek_ram(0x8000), 0x77, "5: RAM at $8000");
        assert_eq!(m.peek_ram(0x0600), 0xCC, "5: the CPU read of $8000");
        assert_eq!(m.peek(0x8000), 0xCC, "5: peek of $8000");
    }

    /// Verifies REQ-206: with the cartridge RD5 inactive and BASIC off, a CPU write to `$A000`
    /// goes to RAM (criterion 6). The `MaxFlash` 1Mbit cartridge (REQ-608) turns RD5 off on an
    /// access to `$D510`.
    #[test]
    fn disabled_cart_write_goes_to_ram() {
        let mut code = set_portb(0xFF);
        code.extend(copy(0xA000, 0x0600));
        code.extend(read(0xD510));
        code.extend(store(0xA000, 0x33));
        code.extend(copy(0xA000, 0x0601));
        let m = run(&code, Some(maxflash_fill()), &[]);
        assert_eq!(m.peek_ram(0x0600), 0xDD, "the cartridge shows before $D510");
        assert_eq!(m.peek_ram(0xA000), 0x33, "RAM at $A000");
        assert_eq!(m.peek_ram(0x0601), 0x33, "the CPU read of $A000");
        assert_eq!(m.peek(0xA000), 0x33, "peek of $A000");
    }

    /// Verifies REQ-232: `TRIG3` follows RD5 when a cartridge switches it (criteria 3 and 8,
    /// with REQ-608). The `MaxFlash` 1Mbit cartridge turns RD5 off on an access to `$D510` and on
    /// again on an access to `$D501`.
    #[test]
    fn trig3_follows_bank_switch() {
        let mut code = copy(0xD013, 0x0600);
        code.extend(read(0xD510));
        code.extend(copy(0xD013, 0x0601));
        code.extend(read(0xD501));
        code.extend(copy(0xD013, 0x0602));
        code.extend(store(0x0603, 0xA5));
        let m = run(&code, Some(maxflash_fill()), &[]);
        assert_eq!(m.peek_ram(0x0603), 0xA5, "the test code ran");
        assert_eq!(m.peek_ram(0x0600), 0x01, "TRIG3 with RD5 active");
        assert_eq!(m.peek_ram(0x0601), 0x00, "3: TRIG3 after LDA $D510");
        assert_eq!(m.peek_ram(0x0602), 0x01, "8: TRIG3 after LDA $D501");
    }
}
