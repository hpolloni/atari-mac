//! The 6502 disassembler (REQ-087).
//!
//! The module does not read memory: the caller gives the three bytes at the address. The
//! table has one entry (mnemonic, addressing mode) for each of the 256 opcodes. The names of
//! the undocumented opcodes are the names of HRM 3.5 Undocumented instructions, Table 2. Two
//! places where the HRM text and Table 2 do not agree follow Table 2: `$71` is `ADC ($nn),Y`,
//! and `$3A`, `$5C`, `$C2` and `$E2` are `NOP`.
//!
//! Design: `docs/design/debugger.md`, "Disassembler" and "Undocumented names".

use core::fmt;

/// The addressing mode of an opcode, for the operand text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// No operand.
    Implied,
    /// The accumulator: `A`.
    Accumulator,
    /// `#$nn`.
    Immediate,
    /// `$nn`.
    ZeroPage,
    /// `$nn,X`.
    ZeroPageX,
    /// `$nn,Y`.
    ZeroPageY,
    /// `$nnnn`.
    Absolute,
    /// `$nnnn,X`.
    AbsoluteX,
    /// `$nnnn,Y`.
    AbsoluteY,
    /// `($nnnn)`.
    Indirect,
    /// `($nn,X)`.
    IndirectX,
    /// `($nn),Y`.
    IndirectY,
    /// A branch: the text is the target address `$nnnn`.
    Relative,
}

impl Mode {
    /// The length in bytes of an instruction with this mode, 1 to 3.
    #[must_use]
    pub const fn length(self) -> u8 {
        match self {
            Self::Implied | Self::Accumulator => 1,
            Self::Immediate
            | Self::ZeroPage
            | Self::ZeroPageX
            | Self::ZeroPageY
            | Self::IndirectX
            | Self::IndirectY
            | Self::Relative => 2,
            Self::Absolute | Self::AbsoluteX | Self::AbsoluteY | Self::Indirect => 3,
        }
    }
}

/// One disassembled instruction. `Display` writes the text of REQ-087 criterion 2, for
/// example `LDA $1234,X`, with no allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disassembly {
    /// The address of the opcode.
    pc: u16,
    /// The opcode and the operand bytes. Only the first `len()` bytes are valid.
    bytes: [u8; 3],
}

impl Disassembly {
    /// The address of the opcode.
    #[must_use]
    pub const fn pc(self) -> u16 {
        self.pc
    }

    /// The length in bytes, 1 to 3 (REQ-087 criterion 1).
    #[must_use]
    #[expect(
        clippy::len_without_is_empty,
        reason = "an instruction always has 1 to 3 bytes; it is never empty"
    )]
    pub const fn len(self) -> u8 {
        length(self.bytes[0])
    }

    /// The mnemonic in upper case: `LDA`, `SLO`, `KIL`.
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        TABLE[self.bytes[0] as usize].0
    }

    /// The addressing mode.
    #[must_use]
    pub const fn mode(self) -> Mode {
        TABLE[self.bytes[0] as usize].1
    }

    /// The instruction bytes. Only the first `len()` bytes are valid.
    #[must_use]
    pub const fn bytes(self) -> [u8; 3] {
        self.bytes
    }

    /// The target address of a branch: `pc + 2 + offset`, with the offset as a signed byte
    /// and a wrap at 16 bits.
    const fn branch_target(self) -> u16 {
        let offset = self.bytes[1].cast_signed();
        self.pc.wrapping_add(2).wrapping_add_signed(offset as i16)
    }
}

impl fmt::Display for Disassembly {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [_, lo, hi] = self.bytes;
        let word = u16::from_le_bytes([lo, hi]);
        f.write_str(self.mnemonic())?;
        match self.mode() {
            Mode::Implied => Ok(()),
            Mode::Accumulator => f.write_str(" A"),
            Mode::Immediate => write!(f, " #${lo:02X}"),
            Mode::ZeroPage => write!(f, " ${lo:02X}"),
            Mode::ZeroPageX => write!(f, " ${lo:02X},X"),
            Mode::ZeroPageY => write!(f, " ${lo:02X},Y"),
            Mode::Absolute => write!(f, " ${word:04X}"),
            Mode::AbsoluteX => write!(f, " ${word:04X},X"),
            Mode::AbsoluteY => write!(f, " ${word:04X},Y"),
            Mode::Indirect => write!(f, " (${word:04X})"),
            Mode::IndirectX => write!(f, " (${lo:02X},X)"),
            Mode::IndirectY => write!(f, " (${lo:02X}),Y"),
            Mode::Relative => write!(f, " ${:04X}", self.branch_target()),
        }
    }
}

/// Disassembles the instruction at `pc`. `bytes` are the bytes at `pc`, `pc + 1` and
/// `pc + 2` (wrap at `$FFFF`). The disassembly uses only the bytes that the opcode needs.
#[must_use]
pub const fn disassemble(pc: u16, bytes: [u8; 3]) -> Disassembly {
    Disassembly { pc, bytes }
}

/// The length in bytes of `opcode`, 1 to 3.
#[must_use]
pub const fn length(opcode: u8) -> u8 {
    TABLE[opcode as usize].1.length()
}

/// The mnemonic and the addressing mode of each opcode (HRM 3.5, Table 2).
const TABLE: [(&str, Mode); 256] = [
    ("BRK", Mode::Implied),     // $00
    ("ORA", Mode::IndirectX),   // $01
    ("KIL", Mode::Implied),     // $02
    ("SLO", Mode::IndirectX),   // $03
    ("NOP", Mode::ZeroPage),    // $04
    ("ORA", Mode::ZeroPage),    // $05
    ("ASL", Mode::ZeroPage),    // $06
    ("SLO", Mode::ZeroPage),    // $07
    ("PHP", Mode::Implied),     // $08
    ("ORA", Mode::Immediate),   // $09
    ("ASL", Mode::Accumulator), // $0A
    ("ANC", Mode::Immediate),   // $0B
    ("NOP", Mode::Absolute),    // $0C
    ("ORA", Mode::Absolute),    // $0D
    ("ASL", Mode::Absolute),    // $0E
    ("SLO", Mode::Absolute),    // $0F
    ("BPL", Mode::Relative),    // $10
    ("ORA", Mode::IndirectY),   // $11
    ("KIL", Mode::Implied),     // $12
    ("SLO", Mode::IndirectY),   // $13
    ("NOP", Mode::ZeroPageX),   // $14
    ("ORA", Mode::ZeroPageX),   // $15
    ("ASL", Mode::ZeroPageX),   // $16
    ("SLO", Mode::ZeroPageX),   // $17
    ("CLC", Mode::Implied),     // $18
    ("ORA", Mode::AbsoluteY),   // $19
    ("NOP", Mode::Implied),     // $1A
    ("SLO", Mode::AbsoluteY),   // $1B
    ("NOP", Mode::AbsoluteX),   // $1C
    ("ORA", Mode::AbsoluteX),   // $1D
    ("ASL", Mode::AbsoluteX),   // $1E
    ("SLO", Mode::AbsoluteX),   // $1F
    ("JSR", Mode::Absolute),    // $20
    ("AND", Mode::IndirectX),   // $21
    ("KIL", Mode::Implied),     // $22
    ("RLA", Mode::IndirectX),   // $23
    ("BIT", Mode::ZeroPage),    // $24
    ("AND", Mode::ZeroPage),    // $25
    ("ROL", Mode::ZeroPage),    // $26
    ("RLA", Mode::ZeroPage),    // $27
    ("PLP", Mode::Implied),     // $28
    ("AND", Mode::Immediate),   // $29
    ("ROL", Mode::Accumulator), // $2A
    ("ANC", Mode::Immediate),   // $2B
    ("BIT", Mode::Absolute),    // $2C
    ("AND", Mode::Absolute),    // $2D
    ("ROL", Mode::Absolute),    // $2E
    ("RLA", Mode::Absolute),    // $2F
    ("BMI", Mode::Relative),    // $30
    ("AND", Mode::IndirectY),   // $31
    ("KIL", Mode::Implied),     // $32
    ("RLA", Mode::IndirectY),   // $33
    ("NOP", Mode::ZeroPageX),   // $34
    ("AND", Mode::ZeroPageX),   // $35
    ("ROL", Mode::ZeroPageX),   // $36
    ("RLA", Mode::ZeroPageX),   // $37
    ("SEC", Mode::Implied),     // $38
    ("AND", Mode::AbsoluteY),   // $39
    ("NOP", Mode::Implied),     // $3A
    ("RLA", Mode::AbsoluteY),   // $3B
    ("NOP", Mode::AbsoluteX),   // $3C
    ("AND", Mode::AbsoluteX),   // $3D
    ("ROL", Mode::AbsoluteX),   // $3E
    ("RLA", Mode::AbsoluteX),   // $3F
    ("RTI", Mode::Implied),     // $40
    ("EOR", Mode::IndirectX),   // $41
    ("KIL", Mode::Implied),     // $42
    ("SRE", Mode::IndirectX),   // $43
    ("NOP", Mode::ZeroPage),    // $44
    ("EOR", Mode::ZeroPage),    // $45
    ("LSR", Mode::ZeroPage),    // $46
    ("SRE", Mode::ZeroPage),    // $47
    ("PHA", Mode::Implied),     // $48
    ("EOR", Mode::Immediate),   // $49
    ("LSR", Mode::Accumulator), // $4A
    ("ASR", Mode::Immediate),   // $4B
    ("JMP", Mode::Absolute),    // $4C
    ("EOR", Mode::Absolute),    // $4D
    ("LSR", Mode::Absolute),    // $4E
    ("SRE", Mode::Absolute),    // $4F
    ("BVC", Mode::Relative),    // $50
    ("EOR", Mode::IndirectY),   // $51
    ("KIL", Mode::Implied),     // $52
    ("SRE", Mode::IndirectY),   // $53
    ("NOP", Mode::ZeroPageX),   // $54
    ("EOR", Mode::ZeroPageX),   // $55
    ("LSR", Mode::ZeroPageX),   // $56
    ("SRE", Mode::ZeroPageX),   // $57
    ("CLI", Mode::Implied),     // $58
    ("EOR", Mode::AbsoluteY),   // $59
    ("NOP", Mode::Implied),     // $5A
    ("SRE", Mode::AbsoluteY),   // $5B
    ("NOP", Mode::AbsoluteX),   // $5C
    ("EOR", Mode::AbsoluteX),   // $5D
    ("LSR", Mode::AbsoluteX),   // $5E
    ("SRE", Mode::AbsoluteX),   // $5F
    ("RTS", Mode::Implied),     // $60
    ("ADC", Mode::IndirectX),   // $61
    ("KIL", Mode::Implied),     // $62
    ("RRA", Mode::IndirectX),   // $63
    ("NOP", Mode::ZeroPage),    // $64
    ("ADC", Mode::ZeroPage),    // $65
    ("ROR", Mode::ZeroPage),    // $66
    ("RRA", Mode::ZeroPage),    // $67
    ("PLA", Mode::Implied),     // $68
    ("ADC", Mode::Immediate),   // $69
    ("ROR", Mode::Accumulator), // $6A
    ("ARR", Mode::Immediate),   // $6B
    ("JMP", Mode::Indirect),    // $6C
    ("ADC", Mode::Absolute),    // $6D
    ("ROR", Mode::Absolute),    // $6E
    ("RRA", Mode::Absolute),    // $6F
    ("BVS", Mode::Relative),    // $70
    ("ADC", Mode::IndirectY),   // $71
    ("KIL", Mode::Implied),     // $72
    ("RRA", Mode::IndirectY),   // $73
    ("NOP", Mode::ZeroPageX),   // $74
    ("ADC", Mode::ZeroPageX),   // $75
    ("ROR", Mode::ZeroPageX),   // $76
    ("RRA", Mode::ZeroPageX),   // $77
    ("SEI", Mode::Implied),     // $78
    ("ADC", Mode::AbsoluteY),   // $79
    ("NOP", Mode::Implied),     // $7A
    ("RRA", Mode::AbsoluteY),   // $7B
    ("NOP", Mode::AbsoluteX),   // $7C
    ("ADC", Mode::AbsoluteX),   // $7D
    ("ROR", Mode::AbsoluteX),   // $7E
    ("RRA", Mode::AbsoluteX),   // $7F
    ("NOP", Mode::Immediate),   // $80
    ("STA", Mode::IndirectX),   // $81
    ("NOP", Mode::Immediate),   // $82
    ("SAX", Mode::IndirectX),   // $83
    ("STY", Mode::ZeroPage),    // $84
    ("STA", Mode::ZeroPage),    // $85
    ("STX", Mode::ZeroPage),    // $86
    ("SAX", Mode::ZeroPage),    // $87
    ("DEY", Mode::Implied),     // $88
    ("NOP", Mode::Immediate),   // $89
    ("TXA", Mode::Implied),     // $8A
    ("ANE", Mode::Immediate),   // $8B
    ("STY", Mode::Absolute),    // $8C
    ("STA", Mode::Absolute),    // $8D
    ("STX", Mode::Absolute),    // $8E
    ("SAX", Mode::Absolute),    // $8F
    ("BCC", Mode::Relative),    // $90
    ("STA", Mode::IndirectY),   // $91
    ("KIL", Mode::Implied),     // $92
    ("SHA", Mode::IndirectY),   // $93
    ("STY", Mode::ZeroPageX),   // $94
    ("STA", Mode::ZeroPageX),   // $95
    ("STX", Mode::ZeroPageY),   // $96
    ("SAX", Mode::ZeroPageY),   // $97
    ("TYA", Mode::Implied),     // $98
    ("STA", Mode::AbsoluteY),   // $99
    ("TXS", Mode::Implied),     // $9A
    ("SHS", Mode::AbsoluteY),   // $9B
    ("SHY", Mode::AbsoluteX),   // $9C
    ("STA", Mode::AbsoluteX),   // $9D
    ("SHX", Mode::AbsoluteY),   // $9E
    ("SHA", Mode::AbsoluteY),   // $9F
    ("LDY", Mode::Immediate),   // $A0
    ("LDA", Mode::IndirectX),   // $A1
    ("LDX", Mode::Immediate),   // $A2
    ("LAX", Mode::IndirectX),   // $A3
    ("LDY", Mode::ZeroPage),    // $A4
    ("LDA", Mode::ZeroPage),    // $A5
    ("LDX", Mode::ZeroPage),    // $A6
    ("LAX", Mode::ZeroPage),    // $A7
    ("TAY", Mode::Implied),     // $A8
    ("LDA", Mode::Immediate),   // $A9
    ("TAX", Mode::Implied),     // $AA
    ("LXA", Mode::Immediate),   // $AB
    ("LDY", Mode::Absolute),    // $AC
    ("LDA", Mode::Absolute),    // $AD
    ("LDX", Mode::Absolute),    // $AE
    ("LAX", Mode::Absolute),    // $AF
    ("BCS", Mode::Relative),    // $B0
    ("LDA", Mode::IndirectY),   // $B1
    ("KIL", Mode::Implied),     // $B2
    ("LAX", Mode::IndirectY),   // $B3
    ("LDY", Mode::ZeroPageX),   // $B4
    ("LDA", Mode::ZeroPageX),   // $B5
    ("LDX", Mode::ZeroPageY),   // $B6
    ("LAX", Mode::ZeroPageY),   // $B7
    ("CLV", Mode::Implied),     // $B8
    ("LDA", Mode::AbsoluteY),   // $B9
    ("TSX", Mode::Implied),     // $BA
    ("LAS", Mode::AbsoluteY),   // $BB
    ("LDY", Mode::AbsoluteX),   // $BC
    ("LDA", Mode::AbsoluteX),   // $BD
    ("LDX", Mode::AbsoluteY),   // $BE
    ("LAX", Mode::AbsoluteY),   // $BF
    ("CPY", Mode::Immediate),   // $C0
    ("CMP", Mode::IndirectX),   // $C1
    ("NOP", Mode::Immediate),   // $C2
    ("DCP", Mode::IndirectX),   // $C3
    ("CPY", Mode::ZeroPage),    // $C4
    ("CMP", Mode::ZeroPage),    // $C5
    ("DEC", Mode::ZeroPage),    // $C6
    ("DCP", Mode::ZeroPage),    // $C7
    ("INY", Mode::Implied),     // $C8
    ("CMP", Mode::Immediate),   // $C9
    ("DEX", Mode::Implied),     // $CA
    ("SBX", Mode::Immediate),   // $CB
    ("CPY", Mode::Absolute),    // $CC
    ("CMP", Mode::Absolute),    // $CD
    ("DEC", Mode::Absolute),    // $CE
    ("DCP", Mode::Absolute),    // $CF
    ("BNE", Mode::Relative),    // $D0
    ("CMP", Mode::IndirectY),   // $D1
    ("KIL", Mode::Implied),     // $D2
    ("DCP", Mode::IndirectY),   // $D3
    ("NOP", Mode::ZeroPageX),   // $D4
    ("CMP", Mode::ZeroPageX),   // $D5
    ("DEC", Mode::ZeroPageX),   // $D6
    ("DCP", Mode::ZeroPageX),   // $D7
    ("CLD", Mode::Implied),     // $D8
    ("CMP", Mode::AbsoluteY),   // $D9
    ("NOP", Mode::Implied),     // $DA
    ("DCP", Mode::AbsoluteY),   // $DB
    ("NOP", Mode::AbsoluteX),   // $DC
    ("CMP", Mode::AbsoluteX),   // $DD
    ("DEC", Mode::AbsoluteX),   // $DE
    ("DCP", Mode::AbsoluteX),   // $DF
    ("CPX", Mode::Immediate),   // $E0
    ("SBC", Mode::IndirectX),   // $E1
    ("NOP", Mode::Immediate),   // $E2
    ("ISB", Mode::IndirectX),   // $E3
    ("CPX", Mode::ZeroPage),    // $E4
    ("SBC", Mode::ZeroPage),    // $E5
    ("INC", Mode::ZeroPage),    // $E6
    ("ISB", Mode::ZeroPage),    // $E7
    ("INX", Mode::Implied),     // $E8
    ("SBC", Mode::Immediate),   // $E9
    ("NOP", Mode::Implied),     // $EA
    ("SBC", Mode::Immediate),   // $EB
    ("CPX", Mode::Absolute),    // $EC
    ("SBC", Mode::Absolute),    // $ED
    ("INC", Mode::Absolute),    // $EE
    ("ISB", Mode::Absolute),    // $EF
    ("BEQ", Mode::Relative),    // $F0
    ("SBC", Mode::IndirectY),   // $F1
    ("KIL", Mode::Implied),     // $F2
    ("ISB", Mode::IndirectY),   // $F3
    ("NOP", Mode::ZeroPageX),   // $F4
    ("SBC", Mode::ZeroPageX),   // $F5
    ("INC", Mode::ZeroPageX),   // $F6
    ("ISB", Mode::ZeroPageX),   // $F7
    ("SED", Mode::Implied),     // $F8
    ("SBC", Mode::AbsoluteY),   // $F9
    ("NOP", Mode::Implied),     // $FA
    ("ISB", Mode::AbsoluteY),   // $FB
    ("NOP", Mode::AbsoluteX),   // $FC
    ("SBC", Mode::AbsoluteX),   // $FD
    ("INC", Mode::AbsoluteX),   // $FE
    ("ISB", Mode::AbsoluteX),   // $FF
];
