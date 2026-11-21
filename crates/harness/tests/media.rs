//! Cartridge system tests (REQ-206, REQ-210, REQ-232, REQ-600, REQ-607 to REQ-609), and the
//! planned executable and disk system tests.
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
        run_for(code, cartridge, ram, CYCLES)
    }

    /// [`run`] for `cycles` machine cycles.
    fn run_for(
        code: &[u8],
        cartridge: Option<Cartridge>,
        ram: &[(u16, u8)],
        cycles: u32,
    ) -> Machine {
        let mut program = code.to_vec();
        let [lo, hi] = (0xC000 + u16::try_from(program.len()).unwrap()).to_le_bytes();
        program.extend([0x4C, lo, hi]);
        let mut m = Machine::new(MachineConfig {
            os_rom: test_os_rom(&program),
            basic_rom: Some(vec![0xBB; 0x2000]),
            cartridge,
            disk: None,
            fast_sio: false,
            tape: None,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap();
        for &(addr, value) in ram {
            m.poke_ram(addr, value);
        }
        for _ in 0..cycles {
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

    /// A CAR file of `car_type` with `data`.
    fn car_file(car_type: u32, data: &[u8]) -> Vec<u8> {
        let sum = data
            .iter()
            .fold(0_u32, |sum, &byte| sum.wrapping_add(u32::from(byte)));
        let mut file = b"CART".to_vec();
        file.extend(car_type.to_be_bytes());
        file.extend(sum.to_be_bytes());
        file.extend([0; 4]);
        file.extend(data);
        file
    }

    /// A `MaxFlash` 1Mbit cartridge (CAR type 41) with all bytes `$DD`.
    fn maxflash_fill() -> Cartridge {
        let data = vec![0xDD_u8; CartKind::MaxFlash1Mbit.rom_size()];
        let cart = Cartridge::from_car(&car_file(41, &data)).unwrap();
        assert_eq!(cart.kind(), CartKind::MaxFlash1Mbit, "CAR type 41");
        cart
    }

    /// A bank pattern cartridge of `kind` (CAR type 41 or 42): each byte of bank `k` holds
    /// `k`.
    fn maxflash_banks(kind: CartKind) -> Cartridge {
        let data: Vec<u8> = (0..kind.rom_size())
            .map(|i| u8::try_from(i >> 13).unwrap())
            .collect();
        let car_type = if kind == CartKind::MaxFlash1Mbit {
            41
        } else {
            42
        };
        let cart = Cartridge::from_car(&car_file(car_type, &data)).unwrap();
        assert_eq!(cart.kind(), kind, "CAR type {car_type}");
        cart
    }

    /// An offset pattern cartridge of `size` bytes: byte `i` holds `(i >> 8) & $FF`.
    fn offset_cartridge(size: usize) -> Cartridge {
        let data: Vec<u8> = (0..size)
            .map(|i| u8::try_from((i >> 8) & 0xFF).unwrap())
            .collect();
        Cartridge::from_raw(data).unwrap()
    }

    /// Verifies REQ-600: with an 8 KB cartridge, `$A100` reads `$01` and `$BFFF` reads `$1F`,
    /// and a CPU write to `$8000` goes to RAM (criteria 1, 2).
    #[test]
    fn req600_std8k_maps_left_window() {
        let mut code = copy(0xA100, 0x0600);
        code.extend(copy(0xBFFF, 0x0601));
        code.extend(store(0x8000, 0x5A));
        code.extend(copy(0x8000, 0x0602));
        code.extend(store(0x0603, 0xA5));
        let m = run(&code, Some(offset_cartridge(0x2000)), &[]);
        assert_eq!(m.peek_ram(0x0603), 0xA5, "the test code ran");
        assert_eq!(m.peek_ram(0x0600), 0x01, "1: $A100");
        assert_eq!(m.peek_ram(0x0601), 0x1F, "1: $BFFF");
        assert_eq!(m.peek_ram(0x0602), 0x5A, "2: the CPU read of $8000");
        assert_eq!(m.peek_ram(0x8000), 0x5A, "2: RAM at $8000");
    }

    /// Verifies REQ-600: with a 16 KB cartridge, `$8100`, `$9FFF`, `$A000` and `$BFFF` read
    /// `$01`, `$1F`, `$20` and `$3F` (criteria 3, 4).
    #[test]
    fn req600_std16k_maps_both_windows() {
        let mut code = copy(0x8100, 0x0600);
        code.extend(copy(0x9FFF, 0x0601));
        code.extend(copy(0xA000, 0x0602));
        code.extend(copy(0xBFFF, 0x0603));
        code.extend(store(0x0604, 0xA5));
        let m = run(&code, Some(offset_cartridge(0x4000)), &[]);
        assert_eq!(m.peek_ram(0x0604), 0xA5, "the test code ran");
        assert_eq!(m.peek_ram(0x0600), 0x01, "3: $8100");
        assert_eq!(m.peek_ram(0x0601), 0x1F, "3: $9FFF");
        assert_eq!(m.peek_ram(0x0602), 0x20, "3: $A000");
        assert_eq!(m.peek_ram(0x0603), 0x3F, "4: $BFFF");
    }

    /// Verifies REQ-210: with RD5 active and BASIC on, `$A000` reads the cartridge byte; with
    /// no cartridge and BASIC off, a CPU write to `$A000` goes to RAM (criteria 1, 2, 4).
    #[test]
    fn cart_over_basic_over_ram() {
        // Criterion 1: RD5 active, BASIC on.
        let mut code = set_portb(0xFD);
        code.extend(copy(0xA000, 0x0600));
        let m = run(&code, Some(fill_cartridge(0x2000)), &[(0xA000, 0x5A)]);
        assert_eq!(m.peek_ram(0x0600), 0xCC, "1: the cartridge byte");

        // Criterion 2: the cartridge makes RD5 inactive (`$D510`).
        let mut code = set_portb(0xFD);
        code.extend(copy(0xA000, 0x0600));
        code.extend(read(0xD510));
        code.extend(copy(0xA000, 0x0601));
        code.extend(set_portb(0xFF));
        code.extend(copy(0xA000, 0x0602));
        let m = run(&code, Some(maxflash_fill()), &[(0xA000, 0x5A)]);
        assert_eq!(
            m.peek_ram(0x0600),
            0xDD,
            "1: the cartridge byte before $D510"
        );
        assert_eq!(
            m.peek_ram(0x0601),
            0xBB,
            "2: BASIC byte $0000 with PORTB = $FD"
        );
        assert_eq!(m.peek_ram(0x0602), 0x5A, "2: RAM with PORTB = $FF");

        // Criterion 4: no cartridge, BASIC off.
        let mut code = set_portb(0xFF);
        code.extend(store(0xA000, 0xC3));
        code.extend(copy(0xA000, 0x0600));
        let m = run(&code, None, &[]);
        assert_eq!(m.peek_ram(0x0600), 0xC3, "4: the CPU read of $A000");
        assert_eq!(m.peek_ram(0xA000), 0xC3, "4: RAM at $A000");
    }

    /// Verifies REQ-210: a `MaxFlash` cartridge that disables itself shows BASIC at `$A000`
    /// with `PORTB` pins = `$FD`, and RAM with `PORTB` pins = `$FF` (criteria 2, 4).
    #[test]
    fn disabled_cart_shows_basic_or_ram() {
        for (kind, disable) in [
            (CartKind::MaxFlash1Mbit, 0xD510),
            (CartKind::MaxFlash8Mbit, 0xD580),
        ] {
            let mut code = set_portb(0xFD);
            code.extend(read(disable));
            code.extend(copy(0xA000, 0x0600));
            code.extend(copy(0xBFFF, 0x0601));
            code.extend(set_portb(0xFF));
            code.extend(store(0xA000, 0xC3));
            code.extend(copy(0xA000, 0x0602));
            code.extend(store(0x0603, 0xA5));
            let m = run(&code, Some(maxflash_banks(kind)), &[]);
            assert_eq!(m.peek_ram(0x0603), 0xA5, "{kind:?}: the test code ran");
            assert_eq!(m.peek_ram(0x0600), 0xBB, "{kind:?} 2: BASIC at $A000");
            assert_eq!(m.peek_ram(0x0601), 0xBB, "{kind:?} 2: BASIC at $BFFF");
            assert_eq!(m.peek_ram(0x0602), 0xC3, "{kind:?} 4: RAM at $A000");
            assert_eq!(
                m.peek_ram(0xA000),
                0xC3,
                "{kind:?} 4: the write went to RAM"
            );
        }
    }

    /// Verifies REQ-232: `TRIG3` reads `$00` with no cartridge (also with BASIC on) and `$01`
    /// with an 8 KB or a 16 KB cartridge (criteria 1, 2, 4, 7).
    #[test]
    fn trig3_follows_rd5() {
        let mut code = copy(0xD013, 0x0600);
        code.extend(set_portb(0xFD));
        code.extend(copy(0xD013, 0x0601));
        code.extend(store(0x0602, 0xA5));
        let m = run(&code, None, &[]);
        assert_eq!(m.peek_ram(0x0602), 0xA5, "the test code ran");
        assert_eq!(m.peek_ram(0x0600), 0x00, "1: no cartridge");
        assert_eq!(m.peek_ram(0x0601), 0x00, "4: no cartridge, BASIC on");
        assert_eq!(m.peek(0xA000), 0xBB, "4: BASIC is on");
        for (size, name) in [(0x2000, "2: 8 KB"), (0x4000, "7: 16 KB")] {
            let m = run(&copy(0xD013, 0x0600), Some(fill_cartridge(size)), &[]);
            assert_eq!(m.peek_ram(0x0600), 0x01, "{name} cartridge");
        }
    }

    /// Verifies REQ-607: a `MaxFlash` 1Mbit cartridge keeps its bank (`$D505`) and its
    /// disabled state (`$D510`) over a press of the RESET key (criteria 1, 2).
    #[test]
    fn req607_bank_kept_on_reset_key() {
        let mut m = run(
            &read(0xD505),
            Some(maxflash_banks(CartKind::MaxFlash1Mbit)),
            &[],
        );
        assert_eq!(m.peek(0xA000), 0x05, "bank 5 before the RESET key");
        m.press_reset();
        assert_eq!(m.peek(0xA000), 0x05, "1: bank 5 after the RESET key");
        assert_eq!(m.peek(0xD013), 0x01, "1: TRIG3");

        let mut m = run(
            &read(0xD510),
            Some(maxflash_banks(CartKind::MaxFlash1Mbit)),
            &[(0xA000, 0x5A), (0xBFFF, 0x5B)],
        );
        m.press_reset();
        assert_eq!(m.peek(0xA000), 0x5A, "2: RAM at $A000 (RD5 negated)");
        assert_eq!(m.peek(0xBFFF), 0x5B, "2: RAM at $BFFF (RD5 negated)");
        assert_eq!(m.peek(0xD013), 0x00, "2: TRIG3");
    }

    /// Verifies REQ-607: a cold reset sets the `MaxFlash` 1Mbit and 8Mbit cartridges to bank
    /// 0, enabled: `$A000` reads `$00` and `TRIG3` reads `$01` (criteria 3, 4).
    #[test]
    fn req607_cold_reset_bank0() {
        for (kind, access, name) in [
            (CartKind::MaxFlash1Mbit, 0xD515, "3: 1Mbit"),
            (CartKind::MaxFlash8Mbit, 0xD585, "4: 8Mbit"),
        ] {
            let mut m = run(&read(access), Some(maxflash_banks(kind)), &[]);
            assert_eq!(
                m.peek(0xD013),
                0x00,
                "{name}: TRIG3 before, bank 5 disabled"
            );
            m.power_on();
            // RAM is $00 after the power-on. A RAM byte of $77 shows that `$A000` is the
            // cartridge and not RAM.
            m.poke_ram(0xA000, 0x77);
            assert_eq!(m.peek(0xA000), 0x00, "{name}: bank 0 after the cold reset");
            assert_eq!(m.peek(0xD013), 0x01, "{name}: TRIG3 after the cold reset");
        }
    }

    /// Verifies REQ-608: after a read of `$D503`, a CPU write of `$00` to `$A000` does not
    /// change the flash data: `$A000` reads `$03` (criterion 9).
    #[test]
    fn req608_flash_write_ignored() {
        let mut code = read(0xD503);
        code.extend(store(0xA000, 0x00));
        code.extend(copy(0xA000, 0x0600));
        code.extend(store(0x0601, 0xA5));
        let m = run(&code, Some(maxflash_banks(CartKind::MaxFlash1Mbit)), &[]);
        assert_eq!(m.peek_ram(0x0601), 0xA5, "the test code ran");
        assert_eq!(m.peek_ram(0x0600), 0x03, "9: the CPU read of $A000");
        assert_eq!(m.peek(0xA000), 0x03, "9: peek of $A000");
    }

    /// Verifies REQ-609: `LDA $D5FF,X` with X = `$08` makes a false read of `$D507`, which
    /// selects bank 7 of a `MaxFlash` 1Mbit cartridge (criterion 1).
    #[test]
    fn req609_dummy_read_switches_bank() {
        let mut code = vec![
            0xA2, 0x08, // LDX #$08
            0xBD, 0xFF, 0xD5, // LDA $D5FF,X
        ];
        code.extend(copy(0xA000, 0x0600));
        code.extend(store(0x0601, 0xA5));
        let m = run(&code, Some(maxflash_banks(CartKind::MaxFlash1Mbit)), &[]);
        assert_eq!(m.peek_ram(0x0601), 0xA5, "the test code ran");
        assert_eq!(
            m.peek_ram(0x0600),
            0x07,
            "1: bank 7 after the false read of $D507"
        );
        assert_eq!(m.peek(0xA000), 0x07, "1: peek of $A000");
    }

    /// Verifies REQ-609: ANTIC playfield DMA from `$D540-$D55F` selects banks of a `MaxFlash`
    /// 8Mbit cartridge; after one frame, `$A000` reads `$5F` (criterion 2).
    #[test]
    fn req609_antic_dma_switches_bank() {
        let mut code = Vec::new();
        for (addr, byte) in (0x0600_u16..).zip([0x4F, 0x40, 0xD5, 0x41, 0x00, 0x06]) {
            code.extend(store(addr, byte));
        }
        code.extend(store(0xD402, 0x00)); // DLISTL
        code.extend(store(0xD403, 0x06)); // DLISTH
        code.extend(copy(0xA000, 0x0700));
        code.extend(store(0xD400, 0x21)); // DMACTL: narrow playfield, display list DMA
        code.extend(store(0x0701, 0xA5));
        let frame = VideoStandard::Ntsc.cycles_per_frame();
        let m = run_for(
            &code,
            Some(maxflash_banks(CartKind::MaxFlash8Mbit)),
            &[],
            CYCLES + 2 * frame,
        );
        assert_eq!(m.peek_ram(0x0701), 0xA5, "the test code ran");
        assert_eq!(m.peek_ram(0x0700), 0x00, "bank 0 before the DMA");
        assert_eq!(m.peek(0xA000), 0x5F, "2: the bank of the last fetch");
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

/// Tests of the 6502 loader of the XEX boot image (`docs/design/media.md`, "XEX boot
/// image") without the SIO bus: a synthetic OS ROM calls the loader as the OS boot does, and
/// its `DSKINV` copies the sectors of the boot image from RAM.
#[cfg(test)]
mod xex_loader {
    use atari_core::media::Xex;
    use atari_core::{Machine, MachineConfig, VideoStandard};
    use atari_harness::system::{TEST_SAMPLE_RATE, test_os_rom};

    /// The RAM address of boot image sector 4. Sector `n` is at `$8000 + (n - 4) * 128`.
    const STREAM: u16 = 0x8000;

    /// The OS ROM: at `$C000`, `JSR $0706` (the boot continuation of the loader) and
    /// `JMP ($000A)` (`DOSVEC`), as the OS does after a disk boot. At `$E453`, a `DSKINV` that
    /// checks the Read command, the buffer `$0880` and the sector number (at most the
    /// number in `$F2`), and copies the sector from RAM at `STREAM`. It gives status `$01`, or
    /// `$8A` for a bad request.
    fn os_rom() -> Vec<u8> {
        let mut program = vec![
            0x20, 0x06, 0x07, // JSR $0706
            0x6C, 0x0A, 0x00, // JMP ($000A)
        ];
        program.resize(0x2453, 0xEA);
        program.extend([
            0xAD, 0x02, 0x03, // LDA DCOMND
            0xC9, 0x52, // CMP #$52
            0xD0, 0x3C, // BNE ERR
            0xAD, 0x01, 0x03, // LDA DUNIT
            0xC9, 0x01, // CMP #$01
            0xD0, 0x35, // BNE ERR
            0xAD, 0x05, 0x03, // LDA DBUFHI
            0xC9, 0x08, // CMP #$08
            0xD0, 0x2E, // BNE ERR
            0xAD, 0x0B, 0x03, // LDA DAUX2
            0xD0, 0x29, // BNE ERR
            0xA5, 0xF2, // LDA $F2
            0xCD, 0x0A, 0x03, // CMP DAUX1
            0x90, 0x22, // BCC ERR (the sector is above the last sector)
            0xAD, 0x0A, 0x03, // LDA DAUX1
            0x38, // SEC
            0xE9, 0x04, // SBC #$04
            0x4A, // LSR A
            0xA0, 0x00, // LDY #$00
            0x90, 0x02, // BCC LOW
            0xA0, 0x80, // LDY #$80
            0x84, 0xF0, // LOW: STY $F0
            0x18, // CLC
            0x69, 0x80, // ADC #>STREAM
            0x85, 0xF1, // STA $F1
            0xA0, 0x7F, // LDY #$7F
            0xB1, 0xF0, // COPY: LDA ($F0),Y
            0x99, 0x80, 0x08, // STA $0880,Y
            0x88, // DEY
            0x10, 0xF8, // BPL COPY
            0xA9, 0x01, // LDA #$01
            0xD0, 0x02, // BNE DONE
            0xA9, 0x8A, // ERR: LDA #$8A
            0x8D, 0x03, 0x03, // DONE: STA DSTATS
            0xA8, // TAY
            0x60, // RTS
        ]);
        test_os_rom(&program)
    }

    /// Puts the boot image of `file` in RAM (sectors 1-3 at `$0700`, the stream at `STREAM`),
    /// sets the last readable sector to `last` (default: the last sector), and runs `cycles`
    /// machine cycles.
    fn run_loader(file: &[u8], last: Option<u8>, cycles: u32) -> Machine {
        let image = Xex::parse(file).unwrap().boot_image();
        let sectors = image.geometry().sectors;
        assert!(sectors <= 3 + 128, "the stream fits in $8000-$BFFF");
        let mut m = Machine::new(MachineConfig {
            os_rom: os_rom(),
            basic_rom: None,
            cartridge: None,
            disk: None,
            fast_sio: false,
            tape: None,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap();
        for n in 1..=sectors {
            let base = if n <= 3 {
                0x0700 + (n - 1) * 128
            } else {
                STREAM + (n - 4) * 128
            };
            for (addr, &byte) in (base..).zip(image.sector(n).unwrap()) {
                m.poke_ram(addr, byte);
            }
        }
        let last = last.unwrap_or_else(|| u8::try_from(sectors).unwrap());
        m.poke_ram(0x00F2, last);
        for _ in 0..cycles {
            m.run_cycle();
        }
        m
    }

    /// An XEX file of `segments`.
    fn xex_file(segments: &[(u16, &[u8])]) -> Vec<u8> {
        let mut file = vec![0xFF, 0xFF];
        for &(start, data) in segments {
            let end = start + u16::try_from(data.len() - 1).unwrap();
            file.extend(start.to_le_bytes());
            file.extend(end.to_le_bytes());
            file.extend(data);
        }
        file
    }

    /// The run code of REQ-612: `LDA #$A5`, `STA $0600`, `JMP $3005`.
    const RUN: [u8; 8] = [0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x4C, 0x05, 0x30];

    /// The cycles that a loader test runs.
    const CYCLES: u32 = 200_000;

    /// Verifies REQ-612: the loader writes the segments of the REQ-612 test files in file
    /// order, across sector ends, and jumps to `RUNAD` (criteria 1 and 4, with the synthetic
    /// OS of this module).
    #[test]
    fn loader_writes_segments_in_order() {
        let page: Vec<u8> = (0..=0xFF).collect();
        let high: Vec<u8> = (0xF0..=0xFF).collect();
        let m = run_loader(
            &xex_file(&[
                (0x2000, &page),
                (0xA000, &high),
                (0x3000, &RUN),
                (0x02E0, &[0x00, 0x30]),
            ]),
            None,
            CYCLES,
        );
        assert_eq!(m.peek_ram(0x0600), 0xA5, "the program started");
        assert_eq!(m.peek(0x000A), 0x18, "DOSVEC low");
        assert_eq!(m.peek(0x000B), 0x07, "DOSVEC high");
        assert_eq!(m.peek(0x000C), 0x17, "DOSINI low");
        assert_eq!(m.peek(0x000D), 0x07, "DOSINI high");
        for k in 0..=0xFF_u8 {
            assert_eq!(m.peek_ram(0x2000 + u16::from(k)), k, "1: $2000 + {k}");
        }
        for k in 0..16_u8 {
            assert_eq!(
                m.peek_ram(0xA000 + u16::from(k)),
                0xF0 + k,
                "1: $A000 + {k}"
            );
        }

        let m = run_loader(
            &xex_file(&[
                (0x4000, &[0x11]),
                (0x4000, &[0x22]),
                (0x3000, &RUN),
                (0x02E0, &[0x00, 0x30]),
            ]),
            None,
            CYCLES,
        );
        assert_eq!(m.peek_ram(0x0600), 0xA5, "4: the program started");
        assert_eq!(m.peek_ram(0x4000), 0x22, "4: file order");
    }

    /// Verifies REQ-613: the loader calls `INITAD` after segments C and E of the REQ-613 test
    /// file only, so `$0600` holds `$11` (criterion 1, with the synthetic OS of this module).
    #[test]
    fn loader_calls_initad() {
        let m = run_loader(
            &xex_file(&[
                (0x0600, &[0x00]),
                (0x2000, &[0xEE, 0x00, 0x06, 0x60]),
                (0x02E2, &[0x00, 0x20]),
                (0x0600, &[0x10]),
                (0x02E2, &[0x00, 0x20]),
                (0x3000, &[0xA9, 0xA5, 0x8D, 0x01, 0x06, 0x4C, 0x05, 0x30]),
                (0x02E0, &[0x00, 0x30]),
            ]),
            None,
            CYCLES,
        );
        assert_eq!(m.peek_ram(0x0601), 0xA5, "the program started");
        assert_eq!(m.peek_ram(0x0600), 0x11, "1: INITAD after C and E only");
    }

    /// Verifies REQ-614: the loader jumps to `RUNAD`, or to the first segment if no segment
    /// writes `RUNAD` (criteria 1 and 2, with the synthetic OS of this module).
    #[test]
    fn loader_jumps_to_run_address() {
        let first = [0xA9, 0x5A, 0x8D, 0x00, 0x06, 0x4C, 0x05, 0x20];
        let m = run_loader(
            &xex_file(&[(0x2000, &first), (0x3000, &RUN), (0x02E0, &[0x00, 0x30])]),
            None,
            CYCLES,
        );
        assert_eq!(m.peek_ram(0x0600), 0xA5, "1: RUNAD = $3000");
        let first = [0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x4C, 0x05, 0x20];
        let second = [0xA9, 0x5A, 0x8D, 0x00, 0x06, 0x4C, 0x05, 0x30];
        let m = run_loader(
            &xex_file(&[(0x2000, &first), (0x3000, &second)]),
            None,
            CYCLES,
        );
        assert_eq!(m.peek_ram(0x0600), 0xA5, "2: the first segment");
    }

    /// Verifies REQ-612: when `DSKINV` gives an error, the loader sets the background color
    /// to red and stops (`docs/design/media.md`, "XEX boot image", step 5).
    #[test]
    fn loader_read_error_shows_red() {
        let page: Vec<u8> = (0..=0xFF).collect();
        let m = run_loader(
            &xex_file(&[(0x2000, &page), (0x3000, &RUN), (0x02E0, &[0x00, 0x30])]),
            Some(4),
            CYCLES,
        );
        assert_eq!(m.peek_ram(0x0600), 0x00, "the program did not start");
        assert_eq!(m.peek_ram(0x02C8), 0x34, "COLOR4 is red");
        assert_eq!(m.peek_ram(0x02C6), 0x34, "COLOR2 is red");
        assert_eq!(
            m.peek_ram(0x2000 + 100),
            100,
            "the data of sector 4 is in memory"
        );
    }
}

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
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
    #[ignore = "planned: SLICE-05"]
    fn req612_xex_loads_segments() {
        planned();
    }

    /// Verifies REQ-612: two segments at `$4000` load in file order, so `$4000` holds `$22`,
    /// with fast SIO off and on (criteria 4, 5).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req612_xex_file_order() {
        planned();
    }

    /// Verifies REQ-613: the OS calls `INITAD` after the segment that sets it: `$0600` holds
    /// `$11` within 600 frames, with fast SIO off and on (criteria 1, 2).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req613_initad_called_per_segment() {
        planned();
    }

    /// Verifies REQ-614: a `RUNAD` segment starts the program at `$3000`: `$0600` holds
    /// `$A5`, with fast SIO off and on (criteria 1, 3).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req614_runad() {
        planned();
    }

    /// Verifies REQ-614: with no `RUNAD` segment, the program starts at the first segment
    /// (`$2000`): `$0600` holds `$A5`, with fast SIO off and on (criteria 2, 3).
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req614_run_first_segment() {
        planned();
    }
}

/// Disk system tests (`docs/design/media.md`, "System test method for disks and
/// executables"): `AltirraOS` and the BASIC ROM boot an ATR image from D1:, with the SIOV
/// shortcut off or on, and the test waits for a marker byte that the boot code writes.
#[cfg(test)]
mod disks {
    use std::path::Path;

    use atari_core::media::DiskImage;
    use atari_core::{Machine, MachineConfig, VideoStandard};
    use atari_harness::HarnessError;
    use atari_harness::data::{FETCH_ROMS, basic_rom, os_rom, read_test_file};
    use atari_harness::system::{TEST_SAMPLE_RATE, run_until};

    /// The marker address of the boot code.
    const MARKER: u16 = 0x0600;
    /// The marker value.
    const DONE: u8 = 0xA5;
    /// The SIOV test program stores Y here (REQ-623).
    const RESULT_Y: u16 = 0x0680;
    /// The SIOV test program stores the processor status here (REQ-623).
    const RESULT_P: u16 = 0x0681;
    /// `DSTATS`.
    const DSTATS: u16 = 0x0303;
    /// The buffer of the SIOV test requests.
    const BUFFER: u16 = 0x4000;
    /// The frame limit of the SIOV tests (REQ-623).
    const SIOV_FRAMES: u32 = 600;
    /// The user-supplied Atari XL/XE OS ROM of REQ-623 criterion 7 (not in git).
    const XL_OS_ROM: &str = "roms/atari-xl-os.rom";

    /// An ATR file with the sectors `sectors` (sector 1 first). Sectors 1-3 have 128 bytes,
    /// the others `sector_size` bytes.
    fn atr(sector_size: u16, sectors: &[Vec<u8>]) -> Vec<u8> {
        let data: Vec<u8> = sectors.iter().flatten().copied().collect();
        let paragraphs = u16::try_from(data.len() / 16).unwrap();
        let mut file = vec![0x96, 0x02];
        file.extend(paragraphs.to_le_bytes());
        file.extend(sector_size.to_le_bytes());
        file.extend([0; 10]);
        file.extend(data);
        file
    }

    /// Sector 1 of a boot image: the boot header `00 count 00 07 00 07` and `code` at
    /// offset 6, then `$00` bytes.
    fn boot_sector(count: u8, code: &[u8]) -> Vec<u8> {
        let mut sector = vec![0x00, count, 0x00, 0x07, 0x00, 0x07];
        sector.extend(code);
        assert!(sector.len() <= 128, "the boot code fits in sector 1");
        sector.resize(128, 0);
        sector
    }

    /// The SD boot image of REQ-636 criterion 1: 1 boot sector with `LDA #$A5`,
    /// `STA $0600`, `JMP $070B`; the other sectors hold `$00`.
    fn sd_boot_image() -> DiskImage {
        let mut sectors = vec![boot_sector(
            1,
            &[0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x4C, 0x0B, 0x07],
        )];
        sectors.resize(720, vec![0; 128]);
        DiskImage::from_atr(&atr(128, &sectors)).unwrap()
    }

    /// The DD boot image of REQ-636 criterion 4: 3 boot sectors, `JMP $0800` in sector 1,
    /// and `LDA #$A5`, `STA $0600`, `JMP $0805` at the start of sector 3.
    fn dd_boot_image() -> DiskImage {
        let mut sectors = vec![boot_sector(3, &[0x4C, 0x00, 0x08]), vec![0; 128]];
        let mut third = vec![0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x4C, 0x05, 0x08];
        third.resize(128, 0);
        sectors.push(third);
        sectors.resize(720, vec![0; 256]);
        let image = DiskImage::from_atr(&atr(256, &sectors)).unwrap();
        assert_eq!(image.geometry().sector_size, 256, "a DD image");
        image
    }

    /// The SD test image of REQ-619 (each byte of sector `n` holds `n & $FF`) with the boot
    /// header and `code` in sector 1 (REQ-623).
    fn siov_image(code: &[u8]) -> DiskImage {
        let mut sectors = vec![boot_sector(1, code)];
        for n in 2..=720_u16 {
            let [low, _] = n.to_le_bytes();
            sectors.push(vec![low; 128]);
        }
        DiskImage::from_atr(&atr(128, &sectors)).unwrap()
    }

    /// A machine with `os`, the BASIC ROM and no cartridge, `disk` in D1: and the SIOV
    /// shortcut `fast_sio`, after power-on.
    fn machine_with(
        os: Vec<u8>,
        disk: DiskImage,
        standard: VideoStandard,
        fast_sio: bool,
    ) -> Machine {
        Machine::new(MachineConfig {
            os_rom: os,
            basic_rom: Some(basic_rom().unwrap()),
            cartridge: None,
            disk: Some(disk),
            fast_sio,
            tape: None,
            standard,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap()
    }

    /// [`machine_with`] with `AltirraOS`.
    fn machine(disk: DiskImage, standard: VideoStandard, fast_sio: bool) -> Machine {
        machine_with(os_rom().unwrap(), disk, standard, fast_sio)
    }

    /// The frames from the cold reset until `$0600` holds `$A5`, at most `max_frames`.
    fn boot_frames(machine: &mut Machine, max_frames: u32) -> Option<u32> {
        run_until(machine, max_frames, |m| m.peek_ram(MARKER) == DONE)
    }

    /// `LDA #value`, `STA addr`.
    fn store(addr: u16, value: u8) -> Vec<u8> {
        let [lo, hi] = addr.to_le_bytes();
        vec![0xA9, value, 0x8D, lo, hi]
    }

    /// A device control block of the SIOV test program: buffer `$4000`, `DTIMLO` = 7 and
    /// `DBYT` = 128 (REQ-623).
    #[derive(Clone, Copy)]
    struct Dcb {
        /// `DDEVIC`.
        device: u8,
        /// `DUNIT`.
        unit: u8,
        /// `DCOMND`.
        command: u8,
        /// `DSTATS` (the direction).
        stats: u8,
        /// `DAUX1` (`DAUX2` is 0).
        aux: u8,
    }

    /// A Read of sector `aux` from D1: into `$4000`.
    const fn read_sector(aux: u8) -> Dcb {
        Dcb {
            device: 0x31,
            unit: 0x01,
            command: 0x52,
            stats: 0x40,
            aux,
        }
    }

    /// The SIOV test program of REQ-623: `prefix`, then it fills the device control block,
    /// executes `JSR $E459`, stores Y at `$0680` and the processor status at `$0681`, writes
    /// `$A5` to `$0600` and loops.
    fn siov_program(prefix: &[u8], dcb: Dcb) -> Vec<u8> {
        let mut code = prefix.to_vec();
        for (addr, value) in [
            (0x0300, dcb.device),
            (0x0301, dcb.unit),
            (0x0302, dcb.command),
            (0x0303, dcb.stats),
            (0x0304, 0x00),
            (0x0305, 0x40),
            (0x0306, 0x07),
            (0x0308, 0x80),
            (0x0309, 0x00),
            (0x030A, dcb.aux),
            (0x030B, 0x00),
        ] {
            code.extend(store(addr, value));
        }
        code.extend([
            0x20, 0x59, 0xE4, // JSR $E459
            0x8C, 0x80, 0x06, // STY $0680
            0x08, // PHP
            0x68, // PLA
            0x8D, 0x81, 0x06, // STA $0681
            0xA9, 0xA5, // LDA #$A5
            0x8D, 0x00, 0x06, // STA $0600
        ]);
        // JMP to itself: the code starts at $0706.
        let [lo, hi] = (0x0706 + u16::try_from(code.len()).unwrap()).to_le_bytes();
        code.extend([0x4C, lo, hi]);
        code
    }

    /// Fills `$4000-$407F` with `$5A`: `LDX #$7F`, `LDA #$5A`, `STA $4000,X`, `DEX`, `BPL`.
    const FILL_5A: [u8; 10] = [0xA2, 0x7F, 0xA9, 0x5A, 0x9D, 0x00, 0x40, 0xCA, 0x10, 0xFA];

    /// The 128 bytes at `$4000`.
    fn buffer(machine: &Machine) -> Vec<u8> {
        (BUFFER..BUFFER + 128)
            .map(|a| machine.peek_ram(a))
            .collect()
    }

    /// Boots the SIOV test program of `prefix` and `dcb` with `os` and the SIOV shortcut on,
    /// and waits for `$0600` = `$A5`.
    fn run_siov(os: Vec<u8>, prefix: &[u8], dcb: Dcb, name: &str) -> Machine {
        let mut m = machine_with(
            os,
            siov_image(&siov_program(prefix, dcb)),
            VideoStandard::Ntsc,
            true,
        );
        let frames = boot_frames(&mut m, SIOV_FRAMES);
        assert!(
            frames.is_some(),
            "{name}: no $A5 at $0600 after {SIOV_FRAMES} frames"
        );
        m
    }

    /// Checks criteria 1 and 2 of REQ-623 with the OS ROM `os`.
    fn check_read_and_nak(os: &[u8], os_name: &str) {
        let m = run_siov(os.to_vec(), &[], read_sector(2), os_name);
        assert_eq!(buffer(&m), vec![0x02; 128], "{os_name} 1: the buffer");
        assert_eq!(m.peek_ram(RESULT_Y), 0x01, "{os_name} 1: Y");
        assert_eq!(m.peek_ram(DSTATS), 0x01, "{os_name} 1: DSTATS");
        assert_eq!(m.peek_ram(RESULT_P) & 0x80, 0, "{os_name} 1: N flag");

        let m = run_siov(os.to_vec(), &[], read_sector(0), os_name);
        assert_eq!(m.peek_ram(RESULT_Y), 0x8B, "{os_name} 2: Y");
        assert_eq!(m.peek_ram(DSTATS), 0x8B, "{os_name} 2: DSTATS");
        assert_eq!(m.peek_ram(RESULT_P) & 0x80, 0x80, "{os_name} 2: N flag");
    }

    /// Verifies REQ-623: with `AltirraOS` and fast SIO on, a Read of sector 2 fills
    /// `$4000-$407F` and gives status `$01`; a Read of sector 0 gives `$8B` (criteria 1, 2).
    #[test]
    fn req623_fast_sio_read() {
        check_read_and_nak(&os_rom().unwrap(), "AltirraOS");
    }

    /// Verifies REQ-623: with fast SIO on, a Put of sector 5 gives status `$01`, and sector 5
    /// of the image then holds 128 bytes of `$5A` (criterion 3).
    #[test]
    fn req623_fast_sio_put() {
        let put = Dcb {
            device: 0x31,
            unit: 0x01,
            command: 0x50,
            stats: 0x80,
            aux: 5,
        };
        let m = run_siov(os_rom().unwrap(), &FILL_5A, put, "Put");
        assert_eq!(m.peek_ram(RESULT_Y), 0x01, "3: Y");
        let sector = m.disk().unwrap().sector(5).unwrap();
        assert_eq!(sector, [0x5A; 128].as_slice(), "3: sector 5 of the image");
    }

    /// Verifies REQ-623: fast SIO does not handle a request for another device, a request
    /// whose bus ID wraps to `$31`, or a call of `$E459` with the OS ROM off (criteria 4-6).
    ///
    /// Ignored: criterion 5 fails with `AltirraOS`. The shortcut does not handle `DDEVIC` =
    /// `$5F`, but the OS routine itself sends the request with `DUNIT` = `$D3` on the bus as
    /// bus ID `$31` (also with the shortcut off), and D1: answers on the real serial path,
    /// so `$4000-$407F` holds 128 bytes of `$02`. The HRM (9.1) says that the SIO routines
    /// intercept `DDEVIC` = `$5F`; OI-046 asks which value the OS checks. Criteria 4 and 6
    /// pass.
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req623_fast_sio_not_intercepted() {
        // 4: D2: is not emulated, so the OS routine runs and times out.
        let d2 = Dcb {
            unit: 0x02,
            ..read_sector(2)
        };
        let m = run_siov(os_rom().unwrap(), &[], d2, "4");
        assert_eq!(m.peek_ram(RESULT_Y), 0x8A, "4: the OS timeout status");

        // 5: DDEVIC = $5F is never intercepted, also when the bus ID wraps to $31.
        let cassette = Dcb {
            device: 0x5F,
            unit: 0xD3,
            ..read_sector(2)
        };
        let mut m = machine(
            siov_image(&siov_program(&[], cassette)),
            VideoStandard::Ntsc,
            true,
        );
        for _ in 0..300 {
            m.run_frame();
        }
        assert_ne!(
            buffer(&m),
            vec![0x02; 128],
            "5: the buffer after 300 frames"
        );

        // 6: with the OS ROM off, `$E459` is RAM with `LDY #$77`, `RTS`.
        let mut prefix = FILL_5A.to_vec();
        prefix.extend([
            0x78, // SEI
            0xA9, 0x00, 0x8D, 0x0E, 0xD4, // LDA #$00, STA NMIEN
            0xAD, 0x01, 0xD3, // LDA PORTB
            0x29, 0xFE, // AND #$FE
            0x8D, 0x01, 0xD3, // STA PORTB
        ]);
        prefix.extend(store(0xE459, 0xA0));
        prefix.extend(store(0xE45A, 0x77));
        prefix.extend(store(0xE45B, 0x60));
        let m = run_siov(os_rom().unwrap(), &prefix, read_sector(2), "6");
        assert_eq!(m.peek_ram(RESULT_Y), 0x77, "6: the RAM code ran");
        assert_eq!(buffer(&m), vec![0x5A; 128], "6: the buffer is not changed");
    }

    /// Verifies REQ-623: criteria 1 and 2 pass with a user-supplied Atari XL/XE OS ROM
    /// (criterion 7). The ROM is `roms/atari-xl-os.rom`. When the file is missing, the test
    /// prints a message that names the missing ROM and does no check (criterion 7). The
    /// test stays ignored until OI-040 is decided, so it never counts as a running test.
    #[test]
    #[ignore = "planned: SLICE-05"]
    fn req623_fast_sio_xl_os() {
        match read_test_file(XL_OS_ROM, FETCH_ROMS) {
            Ok(os) => check_read_and_nak(&os, "XL/XE OS"),
            Err(HarnessError::Missing { path, .. }) => {
                println!(
                    "req623_fast_sio_xl_os: no check: the Atari XL/XE OS ROM {} is missing",
                    Path::new(&path).display()
                );
            }
            Err(error) => panic!("{error}"),
        }
    }

    /// Verifies REQ-623: the SD boot test reaches `$0600` = `$A5` in fewer frames with fast
    /// SIO on than with fast SIO off (criterion 8).
    #[test]
    fn req623_fast_sio_boot_faster() {
        let slow = boot_frames(
            &mut machine(sd_boot_image(), VideoStandard::Ntsc, false),
            300,
        );
        let fast = boot_frames(
            &mut machine(sd_boot_image(), VideoStandard::Ntsc, true),
            300,
        );
        let (slow, fast) = (slow.unwrap(), fast.unwrap());
        assert!(
            fast < slow,
            "fast SIO: {fast} frames, real serial path: {slow} frames"
        );
    }

    /// Checks REQ-636 criterion 1 or 2: the SD image boots within `max_frames`.
    fn check_sd_boot(standard: VideoStandard, fast_sio: bool, max_frames: u32) {
        let mut m = machine(sd_boot_image(), standard, fast_sio);
        let frames = boot_frames(&mut m, max_frames);
        assert!(
            frames.is_some(),
            "{standard:?}, fast SIO {fast_sio}: no $A5 at $0600 after {max_frames} frames"
        );
    }

    /// Verifies REQ-636: NTSC: the SD test image boots after a cold reset, and its boot code
    /// writes `$A5` to `$0600` within the frame limit of the criterion (criterion 1).
    #[test]
    fn req636_boot_sd_ntsc() {
        check_sd_boot(VideoStandard::Ntsc, false, 300);
    }

    /// Verifies REQ-636: the SD boot of criterion 1 also passes for PAL, within 250 frames
    /// (criterion 2).
    #[test]
    fn req636_boot_sd_pal() {
        check_sd_boot(VideoStandard::Pal, false, 250);
    }

    /// Verifies REQ-636: the SD boots of criteria 1 and 2 pass with fast SIO on
    /// (criterion 3).
    #[test]
    fn req636_boot_fast_sio() {
        check_sd_boot(VideoStandard::Ntsc, true, 300);
        check_sd_boot(VideoStandard::Pal, true, 250);
    }

    /// Verifies REQ-636: the DD test image (720 sectors of 256 bytes, 3 boot sectors) boots,
    /// and its code in sector 3 writes `$A5` to `$0600` (criterion 4).
    #[test]
    fn req636_boot_dd() {
        let mut m = machine(dd_boot_image(), VideoStandard::Ntsc, false);
        let frames = boot_frames(&mut m, 300);
        assert!(frames.is_some(), "DD: no $A5 at $0600 after 300 frames");
    }
}

/// Cassette system tests (`docs/design/media.md`, "Test method for the cassette deck"): a
/// synthetic OS ROM runs the test code, with a tape in `MachineConfig`.
#[cfg(test)]
mod cassette {
    use std::iter;

    use atari_core::media::Tape;
    use atari_core::{Machine, MachineConfig, VideoStandard};
    use atari_harness::system::{TEST_SAMPLE_RATE, test_os_rom};

    /// `PACTL`.
    const PACTL: u16 = 0xD302;
    /// `SKSTAT`.
    const SKSTAT: u16 = 0xD20F;

    /// One CAS chunk: the 8-byte header (`kind`, the data length, `aux`) and `data`.
    fn chunk(kind: [u8; 4], aux: u16, data: &[u8]) -> Vec<u8> {
        let mut bytes = kind.to_vec();
        bytes.extend(u16::try_from(data.len()).unwrap().to_le_bytes());
        bytes.extend(aux.to_le_bytes());
        bytes.extend(data);
        bytes
    }

    /// Tape A of REQ-642: one `data` chunk, gap 100 ms, 600 baud, the bytes `55 A5`.
    fn tape_a() -> Tape {
        let mut file = chunk(*b"FUJI", 0, b"A");
        file.extend(chunk(*b"data", 100, &[0x55, 0xA5]));
        Tape::from_cas(&file).unwrap()
    }

    /// A machine with `code` (followed by a `JMP` to itself) in the OS ROM and tape A in
    /// the deck, after power-on.
    fn machine(code: &[u8]) -> Machine {
        let mut program = code.to_vec();
        let [lo, hi] = (0xC000 + u16::try_from(program.len()).unwrap()).to_le_bytes();
        program.extend([0x4C, lo, hi]);
        Machine::new(MachineConfig {
            os_rom: test_os_rom(&program),
            basic_rom: None,
            cartridge: None,
            disk: None,
            fast_sio: false,
            tape: Some(tape_a()),
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap()
    }

    /// `LDA #value`, `STA PACTL`.
    fn pactl(value: u8) -> Vec<u8> {
        let [lo, hi] = PACTL.to_le_bytes();
        vec![0xA9, value, 0x8D, lo, hi]
    }

    /// Runs cycles until the mode bits 5-0 of `PACTL` are `value`, at most `limit` cycles,
    /// and gives the cycle count after the cycle of the write.
    fn run_until_pactl(m: &mut Machine, value: u8, limit: u64) -> u64 {
        for _ in 0..limit {
            m.run_cycle();
            if m.peek(PACTL) & 0x3F == value {
                return m.cycle_count();
            }
        }
        panic!("no write of ${value:02X} to PACTL in {limit} cycles");
    }

    /// A delay loop: `LDY #outer`, then `outer` times an inner loop of `LDX #inner`,
    /// `DEX`, `BNE`, then `DEY`, `BNE`; then a loop of `LDX #tail`, `DEX`, `BNE`, and then
    /// `nops` times `NOP`.
    fn delay(outer: u8, inner: u8, tail: u8, nops: usize) -> Vec<u8> {
        let mut code = vec![0xA0, outer, 0xA2, inner, 0xCA, 0xD0, 0xFD, 0x88, 0xD0, 0xF8];
        code.extend([0xA2, tail, 0xCA, 0xD0, 0xFD]);
        code.extend(iter::repeat_n(0xEA, nops));
        code
    }

    /// Verifies REQ-641: the test code writes `$34` to `PACTL` and, 100,000 cycles later,
    /// `$3C`; the tape position is then 100,000, within 2 cycles (criterion 4).
    #[test]
    fn req641_pactl_moves_tape() {
        let mut code = pactl(0x34);
        code.extend(delay(DELAY_OUTER, DELAY_INNER, DELAY_TAIL, DELAY_NOPS));
        code.extend(pactl(0x3C));
        let mut m = machine(&code);
        assert_eq!(m.tape_position(), Some(0), "position 0 at power-on");
        let motor_on = run_until_pactl(&mut m, 0x34, 1_000);
        assert!(m.tape_motor_on(), "motor on after $34");
        let motor_off = run_until_pactl(&mut m, 0x3C, 200_000);
        assert!(!m.tape_motor_on(), "motor off after $3C");
        assert_eq!(
            motor_off - motor_on,
            100_000,
            "the writes are 100,000 cycles apart"
        );
        let position = m.tape_position().unwrap();
        assert!(position.abs_diff(100_000) <= 2, "position {position}");
        for _ in 0..10_000 {
            m.run_cycle();
        }
        assert_eq!(m.tape_position(), Some(position), "the tape stays");
    }

    /// Verifies REQ-641: `Machine::press_reset` does not change the tape position;
    /// `Machine::power_on` sets it to 0, and the tape stays inserted (criterion 5).
    #[test]
    fn req641_reset_and_power_on() {
        let mut m = machine(&pactl(0x34));
        for _ in 0..50_000 {
            m.run_cycle();
        }
        let position = m.tape_position().unwrap();
        assert!(position > 40_000, "the tape moved: {position}");
        m.press_reset();
        assert_eq!(
            m.tape_position(),
            Some(position),
            "RESET keeps the position"
        );
        // The reset code writes `$34` again: the tape moves on from the same position.
        for _ in 0..1_000 {
            m.run_cycle();
        }
        let after = m.tape_position().unwrap();
        assert!(
            after > position && after < position + 1_000,
            "on from the same position: {after}"
        );
        // Save states keep the tape and the position.
        let state = m.save_state();
        let mut copy = machine(&pactl(0x34));
        copy.load_state(&state).unwrap();
        assert_eq!(copy.tape_position(), Some(after), "save state position");
        assert_eq!(copy.tape_motor_on(), m.tape_motor_on(), "save state motor");
        m.power_on();
        assert_eq!(m.tape_position(), Some(0), "a cold reset rewinds");
        assert!(
            m.sio().tape_deck().tape().is_some(),
            "the tape stays inserted"
        );
        assert!(!m.tape_motor_on(), "the motor is off after power-on");
        let ejected = m.eject_tape();
        assert!(ejected.is_some(), "eject gives the tape");
        assert_eq!(m.tape_position(), None, "no tape");
        m.insert_tape(tape_a());
        assert_eq!(m.tape_position(), Some(0), "insert at position 0");
    }

    /// Verifies REQ-642: NTSC, tape A, the test code writes `$34` to `PACTL` and reads
    /// `SKSTAT` on each cycle: bit 4 is 1 in the gap, and its first 0 is 178,977 cycles after
    /// the motor starts, within 4 cycles (criterion 7).
    #[test]
    fn req642_skstat_sees_tape() {
        let mut m = machine(&pactl(0x34));
        let motor_on = run_until_pactl(&mut m, 0x34, 1_000);
        let first_zero = loop {
            m.run_cycle();
            if m.peek(SKSTAT) & 0x10 == 0 {
                break m.cycle_count();
            }
            assert!(m.cycle_count() < motor_on + 200_000, "SKSTAT bit 4 stays 1");
        };
        let after = first_zero - motor_on;
        assert!(after.abs_diff(178_977) <= 4, "first 0 after {after} cycles");
    }

    /// The delay loop of `req641_pactl_moves_tape`: with the writes, 100,000 machine cycles
    /// from the `$34` write to the `$3C` write, with the refresh cycles of ANTIC.
    const DELAY_OUTER: u8 = 71;
    /// See [`DELAY_OUTER`].
    const DELAY_INNER: u8 = 255;
    /// See [`DELAY_OUTER`].
    const DELAY_TAIL: u8 = 227;
    /// See [`DELAY_OUTER`].
    const DELAY_NOPS: usize = 5;
}

/// Planned cassette system tests (`docs/design/media.md`, "Test method for the cassette
/// deck").
#[cfg(test)]
mod planned_cassette {
    use super::planned;

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
