//! The opcode table of the NMOS 6502 (HRM 3.5, Table 2).
//!
//! Each opcode has an operation and an addressing mode. The addressing mode gives the cycles
//! that compute the effective address; the operation gives the bus pattern at that address
//! (read, write or read-modify-write) and the result.

/// The operation of an opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Op {
    /// Add with carry.
    Adc,
    /// AND with accumulator.
    And,
    /// Shift left.
    Asl,
    /// Bit test.
    Bit,
    /// Conditional branch (the condition comes from the opcode).
    Branch,
    /// Break.
    Brk,
    /// Clear carry.
    Clc,
    /// Clear decimal.
    Cld,
    /// Clear interrupt disable.
    Cli,
    /// Clear overflow.
    Clv,
    /// Compare with accumulator.
    Cmp,
    /// Compare with X.
    Cpx,
    /// Compare with Y.
    Cpy,
    /// Decrement memory.
    Dec,
    /// Decrement X.
    Dex,
    /// Decrement Y.
    Dey,
    /// Exclusive OR with accumulator.
    Eor,
    /// Increment memory.
    Inc,
    /// Increment X.
    Inx,
    /// Increment Y.
    Iny,
    /// Jump, absolute.
    JmpAbs,
    /// Jump, indirect (with the page wrap of the pointer).
    JmpInd,
    /// Jump to subroutine.
    Jsr,
    /// Load accumulator.
    Lda,
    /// Load X.
    Ldx,
    /// Load Y.
    Ldy,
    /// Shift right.
    Lsr,
    /// No operation (also the undocumented `NOP` opcodes with an operand).
    Nop,
    /// OR with accumulator.
    Ora,
    /// Push accumulator.
    Pha,
    /// Push processor status.
    Php,
    /// Pull accumulator.
    Pla,
    /// Pull processor status.
    Plp,
    /// Rotate left.
    Rol,
    /// Rotate right.
    Ror,
    /// Return from interrupt.
    Rti,
    /// Return from subroutine.
    Rts,
    /// Subtract with borrow.
    Sbc,
    /// Set carry.
    Sec,
    /// Set decimal.
    Sed,
    /// Set interrupt disable.
    Sei,
    /// Store accumulator.
    Sta,
    /// Store X.
    Stx,
    /// Store Y.
    Sty,
    /// Transfer A to X.
    Tax,
    /// Transfer A to Y.
    Tay,
    /// Transfer S to X.
    Tsx,
    /// Transfer X to A.
    Txa,
    /// Transfer X to S.
    Txs,
    /// Transfer Y to A.
    Tya,
    /// Undocumented: AND, then copy N to C.
    Anc,
    /// Undocumented (unstable): `A = (A | $EE) & X & imm`.
    Ane,
    /// Undocumented: AND, then rotate right with special flags.
    Arr,
    /// Undocumented: AND, then shift right (also called `ALR`).
    Asr,
    /// Undocumented: decrement memory, then compare.
    Dcp,
    /// Undocumented: increment memory, then subtract.
    Isb,
    /// Undocumented: the CPU locks (`KIL`, `JAM`).
    Kil,
    /// Undocumented: `A = X = S = memory & S`.
    Las,
    /// Undocumented: load A and X.
    Lax,
    /// Undocumented (unstable): `A = X = (A | $EE) & imm`.
    Lxa,
    /// Undocumented: rotate memory left, then AND.
    Rla,
    /// Undocumented: rotate memory right, then add.
    Rra,
    /// Undocumented: store `A & X`.
    Sax,
    /// Undocumented: `X = (A & X) - imm`.
    Sbx,
    /// Undocumented (unstable): store `A & X & (H + 1)`.
    Sha,
    /// Undocumented (unstable): `S = A & X`, store `S & (H + 1)`.
    Shs,
    /// Undocumented (unstable): store `X & (H + 1)`.
    Shx,
    /// Undocumented (unstable): store `Y & (H + 1)`.
    Shy,
    /// Undocumented: shift memory left, then OR.
    Slo,
    /// Undocumented: shift memory right, then exclusive OR.
    Sre,
}

/// The bus pattern of an operation at the effective address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// One read of the operand.
    Read,
    /// One write.
    Write,
    /// Read, write of the old value, write of the new value (REQ-104).
    Rmw,
}

impl Op {
    /// The bus pattern of the operation at the effective address.
    pub(super) const fn kind(self) -> Kind {
        match self {
            Self::Sta
            | Self::Stx
            | Self::Sty
            | Self::Sax
            | Self::Sha
            | Self::Shs
            | Self::Shx
            | Self::Shy => Kind::Write,
            Self::Asl
            | Self::Lsr
            | Self::Rol
            | Self::Ror
            | Self::Inc
            | Self::Dec
            | Self::Slo
            | Self::Rla
            | Self::Sre
            | Self::Rra
            | Self::Dcp
            | Self::Isb => Kind::Rmw,
            _ => Kind::Read,
        }
    }
}

/// The addressing mode of an opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Am {
    /// Implied: one dummy read of `PC`.
    Imp,
    /// Accumulator: like implied, the operation works on `A`.
    Acc,
    /// Immediate `#imm`.
    Imm,
    /// Zero page `zp`.
    Zp,
    /// Zero page indexed `zp,X`.
    Zpx,
    /// Zero page indexed `zp,Y`.
    Zpy,
    /// Absolute `abs`.
    Abs,
    /// Absolute indexed `abs,X`.
    Abx,
    /// Absolute indexed `abs,Y`.
    Aby,
    /// Indexed indirect `(zp,X)`.
    Izx,
    /// Indirect indexed `(zp),Y`.
    Izy,
    /// Relative (branches).
    Rel,
    /// A fixed sequence of the operation (stack, jumps, `BRK`, `KIL`).
    Special,
}

/// The decoded table, indexed by opcode.
pub(super) const TABLE: [(Op, Am); 256] = {
    let mut table = [(Op::Kil, Am::Special); 256];
    let mut opcode: u8 = 0;
    loop {
        table[opcode as usize] = decode(opcode);
        if opcode == u8::MAX {
            break;
        }
        opcode += 1;
    }
    table
};

/// The addressing modes of the `cc = 01` and `cc = 11` columns, by bits 2-4 of the opcode.
const ALU_MODES: [Am; 8] = [
    Am::Izx,
    Am::Zp,
    Am::Imm,
    Am::Abs,
    Am::Izy,
    Am::Zpx,
    Am::Aby,
    Am::Abx,
];

/// Decodes one opcode.
pub(super) const fn decode(opcode: u8) -> (Op, Am) {
    let aaa = (opcode >> 5) as usize;
    let bbb = ((opcode >> 2) & 7) as usize;
    match opcode & 3 {
        0 => decode_00(opcode),
        1 => {
            if opcode == 0x89 {
                (Op::Nop, Am::Imm)
            } else {
                let ops = [
                    Op::Ora,
                    Op::And,
                    Op::Eor,
                    Op::Adc,
                    Op::Sta,
                    Op::Lda,
                    Op::Cmp,
                    Op::Sbc,
                ];
                (ops[aaa], ALU_MODES[bbb])
            }
        }
        2 => decode_10(opcode),
        _ => decode_11(opcode),
    }
}

/// Decodes the opcodes with bits 0-1 = `00`.
const fn decode_00(opcode: u8) -> (Op, Am) {
    match opcode {
        0x00 => (Op::Brk, Am::Special),
        0x20 => (Op::Jsr, Am::Special),
        0x40 => (Op::Rti, Am::Special),
        0x60 => (Op::Rts, Am::Special),
        0x08 => (Op::Php, Am::Special),
        0x28 => (Op::Plp, Am::Special),
        0x48 => (Op::Pha, Am::Special),
        0x68 => (Op::Pla, Am::Special),
        0x4C => (Op::JmpAbs, Am::Special),
        0x6C => (Op::JmpInd, Am::Special),
        0x10 | 0x30 | 0x50 | 0x70 | 0x90 | 0xB0 | 0xD0 | 0xF0 => (Op::Branch, Am::Rel),
        0x18 => (Op::Clc, Am::Imp),
        0x38 => (Op::Sec, Am::Imp),
        0x58 => (Op::Cli, Am::Imp),
        0x78 => (Op::Sei, Am::Imp),
        0xB8 => (Op::Clv, Am::Imp),
        0xD8 => (Op::Cld, Am::Imp),
        0xF8 => (Op::Sed, Am::Imp),
        0x88 => (Op::Dey, Am::Imp),
        0x98 => (Op::Tya, Am::Imp),
        0xA8 => (Op::Tay, Am::Imp),
        0xC8 => (Op::Iny, Am::Imp),
        0xE8 => (Op::Inx, Am::Imp),
        0x24 => (Op::Bit, Am::Zp),
        0x2C => (Op::Bit, Am::Abs),
        0x84 => (Op::Sty, Am::Zp),
        0x8C => (Op::Sty, Am::Abs),
        0x94 => (Op::Sty, Am::Zpx),
        0x9C => (Op::Shy, Am::Abx),
        0xA0 => (Op::Ldy, Am::Imm),
        0xA4 => (Op::Ldy, Am::Zp),
        0xAC => (Op::Ldy, Am::Abs),
        0xB4 => (Op::Ldy, Am::Zpx),
        0xBC => (Op::Ldy, Am::Abx),
        0xC0 => (Op::Cpy, Am::Imm),
        0xC4 => (Op::Cpy, Am::Zp),
        0xCC => (Op::Cpy, Am::Abs),
        0xE0 => (Op::Cpx, Am::Imm),
        0xE4 => (Op::Cpx, Am::Zp),
        0xEC => (Op::Cpx, Am::Abs),
        0x80 => (Op::Nop, Am::Imm),
        0x04 | 0x44 | 0x64 => (Op::Nop, Am::Zp),
        0x0C => (Op::Nop, Am::Abs),
        0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => (Op::Nop, Am::Zpx),
        // 0x1C, 0x3C, 0x5C, 0x7C, 0xDC, 0xFC.
        _ => (Op::Nop, Am::Abx),
    }
}

/// Decodes the opcodes with bits 0-1 = `10`.
const fn decode_10(opcode: u8) -> (Op, Am) {
    match opcode {
        0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 | 0x92 | 0xB2 | 0xD2 | 0xF2 => {
            (Op::Kil, Am::Special)
        }
        0x82 | 0xC2 | 0xE2 => (Op::Nop, Am::Imm),
        0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA | 0xEA => (Op::Nop, Am::Imp),
        0x0A => (Op::Asl, Am::Acc),
        0x2A => (Op::Rol, Am::Acc),
        0x4A => (Op::Lsr, Am::Acc),
        0x6A => (Op::Ror, Am::Acc),
        0x8A => (Op::Txa, Am::Imp),
        0x9A => (Op::Txs, Am::Imp),
        0xAA => (Op::Tax, Am::Imp),
        0xBA => (Op::Tsx, Am::Imp),
        0xCA => (Op::Dex, Am::Imp),
        0x86 => (Op::Stx, Am::Zp),
        0x8E => (Op::Stx, Am::Abs),
        0x96 => (Op::Stx, Am::Zpy),
        0x9E => (Op::Shx, Am::Aby),
        0xA2 => (Op::Ldx, Am::Imm),
        0xA6 => (Op::Ldx, Am::Zp),
        0xAE => (Op::Ldx, Am::Abs),
        0xB6 => (Op::Ldx, Am::Zpy),
        0xBE => (Op::Ldx, Am::Aby),
        _ => {
            // The read-modify-write column: zp, abs, zp,X and abs,X.
            let ops = [
                Op::Asl,
                Op::Rol,
                Op::Lsr,
                Op::Ror,
                Op::Stx,
                Op::Ldx,
                Op::Dec,
                Op::Inc,
            ];
            let modes = [
                Am::Imm,
                Am::Zp,
                Am::Acc,
                Am::Abs,
                Am::Special,
                Am::Zpx,
                Am::Imp,
                Am::Abx,
            ];
            (
                ops[(opcode >> 5) as usize],
                modes[((opcode >> 2) & 7) as usize],
            )
        }
    }
}

/// Decodes the opcodes with bits 0-1 = `11` (all undocumented).
const fn decode_11(opcode: u8) -> (Op, Am) {
    match opcode {
        0x0B | 0x2B => (Op::Anc, Am::Imm),
        0x4B => (Op::Asr, Am::Imm),
        0x6B => (Op::Arr, Am::Imm),
        0x8B => (Op::Ane, Am::Imm),
        0xAB => (Op::Lxa, Am::Imm),
        0xCB => (Op::Sbx, Am::Imm),
        0xEB => (Op::Sbc, Am::Imm),
        0x93 => (Op::Sha, Am::Izy),
        0x9F => (Op::Sha, Am::Aby),
        0x9B => (Op::Shs, Am::Aby),
        0x97 => (Op::Sax, Am::Zpy),
        0xB7 => (Op::Lax, Am::Zpy),
        0xBB => (Op::Las, Am::Aby),
        0xBF => (Op::Lax, Am::Aby),
        _ => {
            let ops = [
                Op::Slo,
                Op::Rla,
                Op::Sre,
                Op::Rra,
                Op::Sax,
                Op::Lax,
                Op::Dcp,
                Op::Isb,
            ];
            (
                ops[(opcode >> 5) as usize],
                ALU_MODES[((opcode >> 2) & 7) as usize],
            )
        }
    }
}
