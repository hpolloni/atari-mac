//! The disassembler of the core (REQ-087). The test names follow the test plan in
//! `docs/design/debugger.md`.

/// The tests and their helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod disassembler {
    use atari_core::disasm::{Mode, disassemble, length};
    use atari_core::media::Cartridge;
    use atari_core::{Machine, MachineConfig, VideoStandard};
    use atari_harness::system::{TEST_SAMPLE_RATE, test_os_rom};

    /// The opcode, the length and the text of each opcode with the operand bytes `$34 $12`
    /// at `$2000`. The names of the undocumented opcodes are the names of HRM 3.5, Table 2
    /// (`docs/design/debugger.md`, "Undocumented names").
    const EXPECTED: [(u8, u8, &str); 256] = [
        (0x00, 1, "BRK"),
        (0x01, 2, "ORA ($34,X)"),
        (0x02, 1, "KIL"),
        (0x03, 2, "SLO ($34,X)"),
        (0x04, 2, "NOP $34"),
        (0x05, 2, "ORA $34"),
        (0x06, 2, "ASL $34"),
        (0x07, 2, "SLO $34"),
        (0x08, 1, "PHP"),
        (0x09, 2, "ORA #$34"),
        (0x0A, 1, "ASL A"),
        (0x0B, 2, "ANC #$34"),
        (0x0C, 3, "NOP $1234"),
        (0x0D, 3, "ORA $1234"),
        (0x0E, 3, "ASL $1234"),
        (0x0F, 3, "SLO $1234"),
        (0x10, 2, "BPL $2036"),
        (0x11, 2, "ORA ($34),Y"),
        (0x12, 1, "KIL"),
        (0x13, 2, "SLO ($34),Y"),
        (0x14, 2, "NOP $34,X"),
        (0x15, 2, "ORA $34,X"),
        (0x16, 2, "ASL $34,X"),
        (0x17, 2, "SLO $34,X"),
        (0x18, 1, "CLC"),
        (0x19, 3, "ORA $1234,Y"),
        (0x1A, 1, "NOP"),
        (0x1B, 3, "SLO $1234,Y"),
        (0x1C, 3, "NOP $1234,X"),
        (0x1D, 3, "ORA $1234,X"),
        (0x1E, 3, "ASL $1234,X"),
        (0x1F, 3, "SLO $1234,X"),
        (0x20, 3, "JSR $1234"),
        (0x21, 2, "AND ($34,X)"),
        (0x22, 1, "KIL"),
        (0x23, 2, "RLA ($34,X)"),
        (0x24, 2, "BIT $34"),
        (0x25, 2, "AND $34"),
        (0x26, 2, "ROL $34"),
        (0x27, 2, "RLA $34"),
        (0x28, 1, "PLP"),
        (0x29, 2, "AND #$34"),
        (0x2A, 1, "ROL A"),
        (0x2B, 2, "ANC #$34"),
        (0x2C, 3, "BIT $1234"),
        (0x2D, 3, "AND $1234"),
        (0x2E, 3, "ROL $1234"),
        (0x2F, 3, "RLA $1234"),
        (0x30, 2, "BMI $2036"),
        (0x31, 2, "AND ($34),Y"),
        (0x32, 1, "KIL"),
        (0x33, 2, "RLA ($34),Y"),
        (0x34, 2, "NOP $34,X"),
        (0x35, 2, "AND $34,X"),
        (0x36, 2, "ROL $34,X"),
        (0x37, 2, "RLA $34,X"),
        (0x38, 1, "SEC"),
        (0x39, 3, "AND $1234,Y"),
        (0x3A, 1, "NOP"),
        (0x3B, 3, "RLA $1234,Y"),
        (0x3C, 3, "NOP $1234,X"),
        (0x3D, 3, "AND $1234,X"),
        (0x3E, 3, "ROL $1234,X"),
        (0x3F, 3, "RLA $1234,X"),
        (0x40, 1, "RTI"),
        (0x41, 2, "EOR ($34,X)"),
        (0x42, 1, "KIL"),
        (0x43, 2, "SRE ($34,X)"),
        (0x44, 2, "NOP $34"),
        (0x45, 2, "EOR $34"),
        (0x46, 2, "LSR $34"),
        (0x47, 2, "SRE $34"),
        (0x48, 1, "PHA"),
        (0x49, 2, "EOR #$34"),
        (0x4A, 1, "LSR A"),
        (0x4B, 2, "ASR #$34"),
        (0x4C, 3, "JMP $1234"),
        (0x4D, 3, "EOR $1234"),
        (0x4E, 3, "LSR $1234"),
        (0x4F, 3, "SRE $1234"),
        (0x50, 2, "BVC $2036"),
        (0x51, 2, "EOR ($34),Y"),
        (0x52, 1, "KIL"),
        (0x53, 2, "SRE ($34),Y"),
        (0x54, 2, "NOP $34,X"),
        (0x55, 2, "EOR $34,X"),
        (0x56, 2, "LSR $34,X"),
        (0x57, 2, "SRE $34,X"),
        (0x58, 1, "CLI"),
        (0x59, 3, "EOR $1234,Y"),
        (0x5A, 1, "NOP"),
        (0x5B, 3, "SRE $1234,Y"),
        (0x5C, 3, "NOP $1234,X"),
        (0x5D, 3, "EOR $1234,X"),
        (0x5E, 3, "LSR $1234,X"),
        (0x5F, 3, "SRE $1234,X"),
        (0x60, 1, "RTS"),
        (0x61, 2, "ADC ($34,X)"),
        (0x62, 1, "KIL"),
        (0x63, 2, "RRA ($34,X)"),
        (0x64, 2, "NOP $34"),
        (0x65, 2, "ADC $34"),
        (0x66, 2, "ROR $34"),
        (0x67, 2, "RRA $34"),
        (0x68, 1, "PLA"),
        (0x69, 2, "ADC #$34"),
        (0x6A, 1, "ROR A"),
        (0x6B, 2, "ARR #$34"),
        (0x6C, 3, "JMP ($1234)"),
        (0x6D, 3, "ADC $1234"),
        (0x6E, 3, "ROR $1234"),
        (0x6F, 3, "RRA $1234"),
        (0x70, 2, "BVS $2036"),
        (0x71, 2, "ADC ($34),Y"),
        (0x72, 1, "KIL"),
        (0x73, 2, "RRA ($34),Y"),
        (0x74, 2, "NOP $34,X"),
        (0x75, 2, "ADC $34,X"),
        (0x76, 2, "ROR $34,X"),
        (0x77, 2, "RRA $34,X"),
        (0x78, 1, "SEI"),
        (0x79, 3, "ADC $1234,Y"),
        (0x7A, 1, "NOP"),
        (0x7B, 3, "RRA $1234,Y"),
        (0x7C, 3, "NOP $1234,X"),
        (0x7D, 3, "ADC $1234,X"),
        (0x7E, 3, "ROR $1234,X"),
        (0x7F, 3, "RRA $1234,X"),
        (0x80, 2, "NOP #$34"),
        (0x81, 2, "STA ($34,X)"),
        (0x82, 2, "NOP #$34"),
        (0x83, 2, "SAX ($34,X)"),
        (0x84, 2, "STY $34"),
        (0x85, 2, "STA $34"),
        (0x86, 2, "STX $34"),
        (0x87, 2, "SAX $34"),
        (0x88, 1, "DEY"),
        (0x89, 2, "NOP #$34"),
        (0x8A, 1, "TXA"),
        (0x8B, 2, "ANE #$34"),
        (0x8C, 3, "STY $1234"),
        (0x8D, 3, "STA $1234"),
        (0x8E, 3, "STX $1234"),
        (0x8F, 3, "SAX $1234"),
        (0x90, 2, "BCC $2036"),
        (0x91, 2, "STA ($34),Y"),
        (0x92, 1, "KIL"),
        (0x93, 2, "SHA ($34),Y"),
        (0x94, 2, "STY $34,X"),
        (0x95, 2, "STA $34,X"),
        (0x96, 2, "STX $34,Y"),
        (0x97, 2, "SAX $34,Y"),
        (0x98, 1, "TYA"),
        (0x99, 3, "STA $1234,Y"),
        (0x9A, 1, "TXS"),
        (0x9B, 3, "SHS $1234,Y"),
        (0x9C, 3, "SHY $1234,X"),
        (0x9D, 3, "STA $1234,X"),
        (0x9E, 3, "SHX $1234,Y"),
        (0x9F, 3, "SHA $1234,Y"),
        (0xA0, 2, "LDY #$34"),
        (0xA1, 2, "LDA ($34,X)"),
        (0xA2, 2, "LDX #$34"),
        (0xA3, 2, "LAX ($34,X)"),
        (0xA4, 2, "LDY $34"),
        (0xA5, 2, "LDA $34"),
        (0xA6, 2, "LDX $34"),
        (0xA7, 2, "LAX $34"),
        (0xA8, 1, "TAY"),
        (0xA9, 2, "LDA #$34"),
        (0xAA, 1, "TAX"),
        (0xAB, 2, "LXA #$34"),
        (0xAC, 3, "LDY $1234"),
        (0xAD, 3, "LDA $1234"),
        (0xAE, 3, "LDX $1234"),
        (0xAF, 3, "LAX $1234"),
        (0xB0, 2, "BCS $2036"),
        (0xB1, 2, "LDA ($34),Y"),
        (0xB2, 1, "KIL"),
        (0xB3, 2, "LAX ($34),Y"),
        (0xB4, 2, "LDY $34,X"),
        (0xB5, 2, "LDA $34,X"),
        (0xB6, 2, "LDX $34,Y"),
        (0xB7, 2, "LAX $34,Y"),
        (0xB8, 1, "CLV"),
        (0xB9, 3, "LDA $1234,Y"),
        (0xBA, 1, "TSX"),
        (0xBB, 3, "LAS $1234,Y"),
        (0xBC, 3, "LDY $1234,X"),
        (0xBD, 3, "LDA $1234,X"),
        (0xBE, 3, "LDX $1234,Y"),
        (0xBF, 3, "LAX $1234,Y"),
        (0xC0, 2, "CPY #$34"),
        (0xC1, 2, "CMP ($34,X)"),
        (0xC2, 2, "NOP #$34"),
        (0xC3, 2, "DCP ($34,X)"),
        (0xC4, 2, "CPY $34"),
        (0xC5, 2, "CMP $34"),
        (0xC6, 2, "DEC $34"),
        (0xC7, 2, "DCP $34"),
        (0xC8, 1, "INY"),
        (0xC9, 2, "CMP #$34"),
        (0xCA, 1, "DEX"),
        (0xCB, 2, "SBX #$34"),
        (0xCC, 3, "CPY $1234"),
        (0xCD, 3, "CMP $1234"),
        (0xCE, 3, "DEC $1234"),
        (0xCF, 3, "DCP $1234"),
        (0xD0, 2, "BNE $2036"),
        (0xD1, 2, "CMP ($34),Y"),
        (0xD2, 1, "KIL"),
        (0xD3, 2, "DCP ($34),Y"),
        (0xD4, 2, "NOP $34,X"),
        (0xD5, 2, "CMP $34,X"),
        (0xD6, 2, "DEC $34,X"),
        (0xD7, 2, "DCP $34,X"),
        (0xD8, 1, "CLD"),
        (0xD9, 3, "CMP $1234,Y"),
        (0xDA, 1, "NOP"),
        (0xDB, 3, "DCP $1234,Y"),
        (0xDC, 3, "NOP $1234,X"),
        (0xDD, 3, "CMP $1234,X"),
        (0xDE, 3, "DEC $1234,X"),
        (0xDF, 3, "DCP $1234,X"),
        (0xE0, 2, "CPX #$34"),
        (0xE1, 2, "SBC ($34,X)"),
        (0xE2, 2, "NOP #$34"),
        (0xE3, 2, "ISB ($34,X)"),
        (0xE4, 2, "CPX $34"),
        (0xE5, 2, "SBC $34"),
        (0xE6, 2, "INC $34"),
        (0xE7, 2, "ISB $34"),
        (0xE8, 1, "INX"),
        (0xE9, 2, "SBC #$34"),
        (0xEA, 1, "NOP"),
        (0xEB, 2, "SBC #$34"),
        (0xEC, 3, "CPX $1234"),
        (0xED, 3, "SBC $1234"),
        (0xEE, 3, "INC $1234"),
        (0xEF, 3, "ISB $1234"),
        (0xF0, 2, "BEQ $2036"),
        (0xF1, 2, "SBC ($34),Y"),
        (0xF2, 1, "KIL"),
        (0xF3, 2, "ISB ($34),Y"),
        (0xF4, 2, "NOP $34,X"),
        (0xF5, 2, "SBC $34,X"),
        (0xF6, 2, "INC $34,X"),
        (0xF7, 2, "ISB $34,X"),
        (0xF8, 1, "SED"),
        (0xF9, 3, "SBC $1234,Y"),
        (0xFA, 1, "NOP"),
        (0xFB, 3, "ISB $1234,Y"),
        (0xFC, 3, "NOP $1234,X"),
        (0xFD, 3, "SBC $1234,X"),
        (0xFE, 3, "INC $1234,X"),
        (0xFF, 3, "ISB $1234,X"),
    ];

    /// Verifies REQ-087: each of the 256 opcodes with the operand bytes `$34 $12` at
    /// `$2000` gives the length (1 to 3) and the text of a table in this test. The undocumented
    /// opcodes have the names of `docs/design/debugger.md`, "Undocumented names" (HRM 3.5), and
    /// each KIL opcode is `KIL` (criteria 1 to 4).
    #[test]
    fn disassembles_all_opcodes() {
        for (index, &(opcode, len, text)) in EXPECTED.iter().enumerate() {
            assert_eq!(usize::from(opcode), index, "table order");
            let d = disassemble(0x2000, [opcode, 0x34, 0x12]);
            assert_eq!(d.len(), len, "length of ${opcode:02X}");
            assert_eq!(length(opcode), len, "length() of ${opcode:02X}");
            assert_eq!(d.to_string(), text, "text of ${opcode:02X}");
            assert_eq!(
                text.split(' ').next(),
                Some(d.mnemonic()),
                "mnemonic of ${opcode:02X}"
            );
            assert_eq!(d.bytes(), [opcode, 0x34, 0x12], "bytes of ${opcode:02X}");
        }
        assert_eq!(disassemble(0x2000, [0x00, 0x34, 0x12]).len(), 1, "BRK");
        assert_eq!(
            disassemble(0, [0x6C, 0, 0]).mode(),
            Mode::Indirect,
            "JMP ()"
        );
        assert_eq!(
            disassemble(0, [0x0A, 0, 0]).mode(),
            Mode::Accumulator,
            "ASL A"
        );
        assert_eq!(disassemble(0, [0xD0, 0, 0]).mode(), Mode::Relative, "BNE");
        let kil = EXPECTED.iter().filter(|entry| entry.2 == "KIL").count();
        assert_eq!(kil, 12, "KIL opcodes");
        assert!(
            EXPECTED.iter().all(|entry| (1..=3).contains(&entry.1)),
            "lengths 1 to 3"
        );
    }

    /// Verifies REQ-087: a branch shows its target address with a wrap at 16 bits:
    /// `BNE` at `$FFF0` with the offset `$7F` is `BNE $0071`, and at `$0010` with the offset `$80`
    /// is `BNE $FF92` (criterion 2).
    #[test]
    fn branch_targets_wrap() {
        assert_eq!(
            disassemble(0xFFF0, [0xD0, 0x7F, 0x00]).to_string(),
            "BNE $0071"
        );
        assert_eq!(
            disassemble(0x0010, [0xD0, 0x80, 0x00]).to_string(),
            "BNE $FF92"
        );
        assert_eq!(
            disassemble(0x2000, [0xD0, 0xFE, 0x00]).to_string(),
            "BNE $2000"
        );
        assert_eq!(
            disassemble(0x2000, [0xA9, 0x0A, 0x00]).to_string(),
            "LDA #$0A",
            "upper case hexadecimal digits"
        );
    }

    /// A `MaxFlash` 1Mbit cartridge (CAR type 41): a read of `$D5xx` switches its bank
    /// (HRM, Cartridges), so a read with a side effect changes the machine. Each byte of bank
    /// `k` holds `k`.
    fn bank_cartridge() -> Cartridge {
        let data: Vec<u8> = (0..0x2_0000_usize)
            .map(|i| u8::try_from(i >> 13).unwrap())
            .collect();
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

    /// Verifies REQ-087: `Machine::disassemble_at` and `Machine::debug_peek`
    /// for each address `$D000` to `$D7FF` leave the machine equal to a copy from before the reads
    /// (criterion 5).
    #[test]
    fn disassembly_reads_without_side_effects() {
        // The program enables the POKEY timer IRQ and ANTIC DMA, so the chips have state that
        // a read with side effects changes, then loops.
        let program = [
            0xA9, 0x03, 0x8D, 0x0F, 0xD2, // LDA #$03, STA SKCTL
            0xA9, 0x01, 0x8D, 0x0E, 0xD2, // LDA #$01, STA IRQEN
            0xA9, 0x22, 0x8D, 0x00, 0xD4, // LDA #$22, STA DMACTL
            0x4C, 0x0F, 0xC0, // JMP $C00F
        ];
        let mut m = Machine::new(MachineConfig {
            os_rom: test_os_rom(&program),
            basic_rom: None,
            cartridge: Some(bank_cartridge()),
            disk: None,
            fast_sio: false,
            fast_tape: false,
            tape: None,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap();
        for _ in 0..2 {
            m.run_frame();
        }
        for _ in 0..123 {
            m.run_cycle();
        }
        let before = m.clone();
        for addr in 0xD000..=0xD7FF_u16 {
            let byte = m.debug_peek(addr);
            let d = m.disassemble_at(addr);
            if (0xD500..=0xD5FF).contains(&addr) {
                assert_eq!(byte, None, "cartridge control ${addr:04X}");
                assert_eq!(d.bytes()[0], 0xFF, "None reads as $FF at ${addr:04X}");
            } else {
                assert_eq!(byte, Some(m.peek(addr)), "debug_peek ${addr:04X}");
                assert_eq!(d.bytes()[0], m.peek(addr), "disassemble_at ${addr:04X}");
            }
            assert_eq!(d.pc(), addr, "pc of ${addr:04X}");
        }
        assert!(m == before, "the reads changed the machine");
    }
}
