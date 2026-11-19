//! The arithmetic and logic operations of the NMOS 6502 (HRM 3.1, 3.2, 3.5).
//!
//! Each function changes the registers and the flags in the same way as the real CPU. The
//! decimal mode algorithms are the NMOS algorithms: `N`, `V` and `Z` come from the
//! intermediate result, `C` from the corrected high nibble (HRM 3.2).

use super::Registers;
use super::flags::{C, D, N, V, Z};

/// The low byte of `value`.
const fn low(value: u16) -> u8 {
    value.to_le_bytes()[0]
}

/// The low byte of the two's complement of `value`.
const fn low_signed(value: i16) -> u8 {
    value.to_le_bytes()[0]
}

/// Sets or clears `flag` in `P`.
const fn set_flag(r: &mut Registers, flag: u8, on: bool) {
    if on {
        r.p |= flag;
    } else {
        r.p &= !flag;
    }
}

/// Sets `N` and `Z` from `value`.
pub(super) const fn set_nz(r: &mut Registers, value: u8) {
    set_flag(r, N, value & 0x80 != 0);
    set_flag(r, Z, value == 0);
}

/// The carry flag as 0 or 1.
const fn carry(p: u8) -> u8 {
    p & C
}

/// `ADC`: `A = A + value + C`, binary or decimal by `D` (REQ-108).
pub(super) fn adc(r: &mut Registers, value: u8) {
    if r.p & D == 0 {
        adc_binary(r, value);
    } else {
        adc_decimal(r, value);
    }
}

/// `ADC` in binary mode.
fn adc_binary(r: &mut Registers, value: u8) {
    let sum = u16::from(r.a) + u16::from(value) + u16::from(carry(r.p));
    let result = low(sum);
    set_flag(r, V, (!(r.a ^ value) & (r.a ^ result) & 0x80) != 0);
    set_flag(r, C, sum > 0xFF);
    r.a = result;
    set_nz(r, result);
}

/// `ADC` in decimal mode (NMOS).
fn adc_decimal(r: &mut Registers, value: u8) {
    let a = u16::from(r.a);
    let m = u16::from(value);
    let c = u16::from(carry(r.p));
    let mut lo = (a & 0x0F) + (m & 0x0F) + c;
    let mut hi = (a & 0xF0) + (m & 0xF0);
    if lo > 0x09 {
        lo += 0x06;
    }
    if lo > 0x0F {
        hi += 0x10;
    }
    set_flag(r, Z, low(a + m + c) == 0);
    set_flag(r, N, hi & 0x80 != 0);
    set_flag(r, V, (!(a ^ m) & (a ^ hi) & 0x80) != 0);
    if hi > 0x90 {
        hi += 0x60;
    }
    set_flag(r, C, hi > 0xFF);
    r.a = low((hi & 0xF0) | (lo & 0x0F));
}

/// `SBC`: `A = A - value - (1 - C)`, binary or decimal by `D` (REQ-108).
pub(super) fn sbc(r: &mut Registers, value: u8) {
    let a = r.a;
    let c = carry(r.p);
    // The flags come from the binary result in both modes.
    adc_binary(r, !value);
    if r.p & D != 0 {
        let borrow = i16::from(1 - c);
        let mut lo = i16::from(a & 0x0F) - i16::from(value & 0x0F) - borrow;
        let mut hi = i16::from(a >> 4) - i16::from(value >> 4);
        if lo < 0 {
            lo -= 6;
            hi -= 1;
        }
        if hi < 0 {
            hi -= 6;
        }
        r.a = low_signed((hi << 4) | (lo & 0x0F));
    }
}

/// `CMP`, `CPX`, `CPY`: compares `reg` with `value`.
pub(super) const fn compare(r: &mut Registers, reg: u8, value: u8) {
    set_flag(r, C, reg >= value);
    set_nz(r, reg.wrapping_sub(value));
}

/// `BIT`: `Z` from `A & value`, `N` and `V` from bits 7 and 6 of `value`.
pub(super) const fn bit(r: &mut Registers, value: u8) {
    set_flag(r, Z, r.a & value == 0);
    set_flag(r, N, value & 0x80 != 0);
    set_flag(r, V, value & 0x40 != 0);
}

/// `ASL`.
pub(super) const fn asl(r: &mut Registers, value: u8) -> u8 {
    set_flag(r, C, value & 0x80 != 0);
    let result = value << 1;
    set_nz(r, result);
    result
}

/// `LSR`.
pub(super) const fn lsr(r: &mut Registers, value: u8) -> u8 {
    set_flag(r, C, value & 0x01 != 0);
    let result = value >> 1;
    set_nz(r, result);
    result
}

/// `ROL`.
pub(super) const fn rol(r: &mut Registers, value: u8) -> u8 {
    let result = (value << 1) | carry(r.p);
    set_flag(r, C, value & 0x80 != 0);
    set_nz(r, result);
    result
}

/// `ROR`.
pub(super) const fn ror(r: &mut Registers, value: u8) -> u8 {
    let result = (value >> 1) | (carry(r.p) << 7);
    set_flag(r, C, value & 0x01 != 0);
    set_nz(r, result);
    result
}

/// `INC` (memory). `D` has no effect (REQ-109).
pub(super) const fn inc(r: &mut Registers, value: u8) -> u8 {
    let result = value.wrapping_add(1);
    set_nz(r, result);
    result
}

/// `DEC` (memory). `D` has no effect (REQ-109).
pub(super) const fn dec(r: &mut Registers, value: u8) -> u8 {
    let result = value.wrapping_sub(1);
    set_nz(r, result);
    result
}

/// `ANC`: `A = A & value`, then `C = N`.
pub(super) const fn anc(r: &mut Registers, value: u8) {
    r.a &= value;
    set_nz(r, r.a);
    set_flag(r, C, r.a & 0x80 != 0);
}

/// `ARR`: `A = (A & value) ROR 1`, with the flags of HRM 3.5, binary or decimal by `D`.
pub(super) fn arr(r: &mut Registers, value: u8) {
    let t = r.a & value;
    let mut result = (t >> 1) | (carry(r.p) << 7);
    if r.p & D == 0 {
        set_nz(r, result);
        set_flag(r, C, result & 0x40 != 0);
        set_flag(r, V, ((result >> 6) ^ (result >> 5)) & 1 != 0);
    } else {
        set_flag(r, N, carry(r.p) != 0);
        set_flag(r, Z, result == 0);
        set_flag(r, V, (t ^ result) & 0x40 != 0);
        if (t & 0x0F) + (t & 0x01) > 0x05 {
            result = (result & 0xF0) | (result.wrapping_add(0x06) & 0x0F);
        }
        let high = u16::from(t & 0xF0) + u16::from(t & 0x10);
        set_flag(r, C, high > 0x50);
        if high > 0x50 {
            result = result.wrapping_add(0x60);
        }
    }
    r.a = result;
}

/// `SBX`: `X = (A & X) - value`, with `C` and the flags of a compare.
pub(super) const fn sbx(r: &mut Registers, value: u8) {
    let t = r.a & r.x;
    set_flag(r, C, t >= value);
    r.x = t.wrapping_sub(value);
    set_nz(r, r.x);
}
