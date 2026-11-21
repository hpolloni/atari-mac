//! Cartridge system tests (REQ-206, REQ-232, REQ-608), and the planned cartridge, executable
//! and disk system tests.
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

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

/// Planned cartridge system tests (`docs/design/media.md`, "Tests").
#[cfg(test)]
mod planned_cartridges {
    use super::planned;

    /// Verifies REQ-600: with an 8 KB cartridge, `$A100` reads `$01` and `$BFFF` reads `$1F`,
    /// and a CPU write to `$8000` goes to RAM (criteria 1, 2).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req600_std8k_maps_left_window() {
        planned();
    }

    /// Verifies REQ-600: with a 16 KB cartridge, `$8100`, `$9FFF`, `$A000` and `$BFFF` read
    /// `$01`, `$1F`, `$20` and `$3F` (criteria 3, 4).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req600_std16k_maps_both_windows() {
        planned();
    }

    /// Verifies REQ-210: with RD5 active and BASIC on, `$A000` reads the cartridge byte; with
    /// no cartridge and BASIC off, a CPU write to `$A000` goes to RAM (criteria 1, 2, 4).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn cart_over_basic_over_ram() {
        planned();
    }

    /// Verifies REQ-210: a `MaxFlash` cartridge that disables itself shows BASIC at `$A000`
    /// with `PORTB` pins = `$FD`, and RAM with `PORTB` pins = `$FF` (criteria 2, 4).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn disabled_cart_shows_basic_or_ram() {
        planned();
    }

    /// Verifies REQ-232: `TRIG3` reads `$00` with no cartridge (also with BASIC on) and `$01`
    /// with an 8 KB or a 16 KB cartridge (criteria 1, 2, 4, 7).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn trig3_follows_rd5() {
        planned();
    }

    /// Verifies REQ-607: a `MaxFlash` 1Mbit cartridge keeps its bank (`$D505`) and its
    /// disabled state (`$D510`) over a press of the RESET key (criteria 1, 2).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req607_bank_kept_on_reset_key() {
        planned();
    }

    /// Verifies REQ-607: a cold reset sets the `MaxFlash` 1Mbit and 8Mbit cartridges to bank
    /// 0, enabled: `$A000` reads `$00` and `TRIG3` reads `$01` (criteria 3, 4).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req607_cold_reset_bank0() {
        planned();
    }

    /// Verifies REQ-608: after a read of `$D503`, a CPU write of `$00` to `$A000` does not
    /// change the flash data: `$A000` reads `$03` (criterion 9).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req608_flash_write_ignored() {
        planned();
    }

    /// Verifies REQ-609: `LDA $D5FF,X` with X = `$08` makes a false read of `$D507`, which
    /// selects bank 7 of a `MaxFlash` 1Mbit cartridge (criterion 1).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req609_dummy_read_switches_bank() {
        planned();
    }

    /// Verifies REQ-609: ANTIC playfield DMA from `$D540-$D55F` selects banks of a `MaxFlash`
    /// 8Mbit cartridge; after one frame, `$A000` reads `$5F` (criterion 2).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req609_antic_dma_switches_bank() {
        planned();
    }
}

/// Planned executable file system tests (`docs/design/media.md`, "System test method for
/// disks and executables").
#[cfg(test)]
mod planned_executables {
    use super::planned;

    /// Verifies REQ-612: the segments at `$2000` and `$A000` are in RAM when the program
    /// starts, BASIC is off and RAM was cleared by a cold reset, with fast SIO off and on
    /// (criteria 1-3, 5).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req612_xex_loads_segments() {
        planned();
    }

    /// Verifies REQ-612: two segments at `$4000` load in file order, so `$4000` holds `$22`,
    /// with fast SIO off and on (criteria 4, 5).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req612_xex_file_order() {
        planned();
    }

    /// Verifies REQ-613: the OS calls `INITAD` after the segment that sets it: `$0600` holds
    /// `$11` within 600 frames, with fast SIO off and on (criteria 1, 2).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req613_initad_called_per_segment() {
        planned();
    }

    /// Verifies REQ-614: a `RUNAD` segment starts the program at `$3000`: `$0600` holds
    /// `$A5`, with fast SIO off and on (criteria 1, 3).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req614_runad() {
        planned();
    }

    /// Verifies REQ-614: with no `RUNAD` segment, the program starts at the first segment
    /// (`$2000`): `$0600` holds `$A5`, with fast SIO off and on (criteria 2, 3).
    #[test]
    #[ignore = "planned: SLICE-04"]
    fn req614_run_first_segment() {
        planned();
    }
}

/// Planned disk system tests (`docs/design/media.md`, "System test method for disks and
/// executables").
#[cfg(test)]
mod planned_disks {
    use super::planned;

    /// Verifies REQ-623: with `AltirraOS` and fast SIO on, a Read of sector 2 fills
    /// `$4000-$407F` and gives status `$01`; a Read of sector 0 gives `$8B` (criteria 1, 2).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req623_fast_sio_read() {
        planned();
    }

    /// Verifies REQ-623: with fast SIO on, a Put of sector 5 gives status `$01`, and sector 5
    /// of the image then holds 128 bytes of `$5A` (criterion 3).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req623_fast_sio_put() {
        planned();
    }

    /// Verifies REQ-623: fast SIO does not handle a request for another device, a request
    /// whose bus ID wraps to `$31`, or a call of `$E459` with the OS ROM off (criteria 4-6).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req623_fast_sio_not_intercepted() {
        planned();
    }

    /// Verifies REQ-623: criteria 1 and 2 pass with a user-supplied Atari XL/XE OS ROM
    /// (criterion 7). The real test needs the ROM (`docs/design/media.md`, "Tests").
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req623_fast_sio_xl_os() {
        planned();
    }

    /// Verifies REQ-623: the SD boot test reaches `$0600` = `$A5` in fewer frames with fast
    /// SIO on than with fast SIO off (criterion 8).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req623_fast_sio_boot_faster() {
        planned();
    }

    /// Verifies REQ-636: NTSC: the SD test image boots after a cold reset, and its boot code
    /// writes `$A5` to `$0600` within the frame limit of the criterion (criterion 1).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req636_boot_sd_ntsc() {
        planned();
    }

    /// Verifies REQ-636: the SD boot of criterion 1 also passes for PAL, within 250 frames
    /// (criterion 2).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req636_boot_sd_pal() {
        planned();
    }

    /// Verifies REQ-636: the SD boots of criteria 1 and 2 pass with fast SIO on
    /// (criterion 3).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req636_boot_fast_sio() {
        planned();
    }

    /// Verifies REQ-636: the DD test image (720 sectors of 256 bytes, 3 boot sectors) boots,
    /// and its code in sector 3 writes `$A5` to `$0600` (criterion 4).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req636_boot_dd() {
        planned();
    }
}

/// Planned cassette system tests (`docs/design/media.md`, "Test method for the cassette
/// deck").
#[cfg(test)]
mod planned_cassette {
    use super::planned;

    /// Verifies REQ-641: the test code writes `$34` to `PACTL` and, 100,000 cycles later,
    /// `$3C`; the tape position is then 100,000, within 2 cycles (criterion 4).
    #[test]
    #[ignore = "planned: SLICE-08"]
    fn req641_pactl_moves_tape() {
        planned();
    }

    /// Verifies REQ-641: `Machine::press_reset` does not change the tape position;
    /// `Machine::power_on` sets it to 0, and the tape stays inserted (criterion 5).
    #[test]
    #[ignore = "planned: SLICE-08"]
    fn req641_reset_and_power_on() {
        planned();
    }

    /// Verifies REQ-642: NTSC, tape A, the test code writes `$34` to `PACTL` and reads
    /// `SKSTAT` on each cycle: bit 4 is 1 in the gap, and its first 0 is 178,977 cycles after
    /// the motor starts, within 4 cycles (criterion 7).
    #[test]
    #[ignore = "planned: SLICE-08"]
    fn req642_skstat_sees_tape() {
        planned();
    }

    /// Verifies REQ-643: NTSC, `AltirraOS` and BASIC, the boot test tape, START until frame
    /// 60 and RETURN in frames 180-186: `$0600` holds `$A5` within 2,400 frames; the motor
    /// line is high at the end of each frame before frame 180 and low at the end of a frame
    /// from 181 to 300 (criteria 1, 3).
    #[test]
    #[ignore = "planned: SLICE-08"]
    fn req643_boot_tape_ntsc() {
        planned();
    }

    /// Verifies REQ-643: PAL, START until frame 60 and RETURN in frames 150-156: `$0600`
    /// holds `$A5` within 2,000 frames (criterion 2).
    #[test]
    #[ignore = "planned: SLICE-08"]
    fn req643_boot_tape_pal() {
        planned();
    }

    /// Verifies REQ-643: the boot test tape with one bit of the record 2 checksum changed:
    /// after 2,400 frames (NTSC), `$0600` does not hold `$A5` (criterion 4).
    #[test]
    #[ignore = "planned: SLICE-08"]
    fn req643_bad_checksum_no_boot() {
        planned();
    }

    /// Verifies REQ-643: the NTSC boot of criterion 1 also passes with the SIOV shortcut
    /// on (criterion 5).
    #[test]
    #[ignore = "planned: SLICE-08"]
    fn req643_boot_tape_siov_shortcut() {
        planned();
    }
}
