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
