//! The SIOV shortcut (REQ-623, `docs/design/media.md`, "SIOV shortcut").
//!
//! At the opcode fetch of the instruction at SIOV, with the OS ROM enabled, the machine does
//! the request of the device control block at once with `SioBus::transact`. For a device
//! that the emulator does not have, the OS code runs.

use super::{CpuBus, Machine, PORTB_OS_ROM};
use crate::cpu::Bus as _;
use crate::media::{CommandFrame, SioData, SioOutcome, SioRequest};

/// The OS SIO entry point (TRN OS-APP-J).
const SIOV: u16 = 0xE459;

/// The device control block: `DDEVIC` at `$0300` to `DAUX2` at `$030B`.
const DCB: u16 = 0x0300;

/// The length of the device control block.
const DCB_LEN: usize = 12;

/// `DSTATS`: the direction in, the status out.
const DSTATS: u16 = 0x0303;

/// The `DDEVIC` value of the cassette, which the OS sends to its cassette code (HRM 9.1,
/// OI-046). It is never intercepted.
const DDEVIC_CASSETTE: u8 = 0x5F;

/// `DSTATS` bit 7: the computer sends a data frame.
const DIR_SEND: u8 = 0x80;
/// `DSTATS` bit 6: the computer receives a data frame.
const DIR_RECEIVE: u8 = 0x40;

/// The N flag of the processor status.
const FLAG_N: u8 = 0x80;
/// The Z flag of the processor status.
const FLAG_Z: u8 = 0x02;

/// Does the SIOV shortcut if the CPU is about to execute the instruction at SIOV with the
/// OS ROM enabled and the device control block names an emulated device. The caller checks
/// that the shortcut is on and that the next tick is an opcode fetch.
pub(super) fn shortcut(machine: &mut Machine) {
    if machine.cpu.registers().pc != SIOV || machine.mem.portb & PORTB_OS_ROM == 0 {
        return;
    }
    let mut dcb = [0_u8; DCB_LEN];
    for (addr, slot) in (DCB..).zip(dcb.iter_mut()) {
        *slot = machine.peek(addr);
    }
    let [
        ddevic,
        dunit,
        dcomnd,
        dstats,
        buf_lo,
        buf_hi,
        _dtimlo,
        _unused,
        len_lo,
        len_hi,
        aux1,
        aux2,
    ] = dcb;
    if ddevic == DDEVIC_CASSETTE {
        return;
    }
    let buffer = u16::from_le_bytes([buf_lo, buf_hi]);
    let len = usize::from(u16::from_le_bytes([len_lo, len_hi]));
    let frame = CommandFrame {
        device: ddevic.wrapping_add(dunit).wrapping_sub(1),
        command: dcomnd,
        aux1,
        aux2,
    };
    let send: Vec<u8>;
    let data = if dstats & DIR_SEND != 0 {
        send = (buffer..=u16::MAX)
            .chain(0..buffer)
            .take(len)
            .map(|addr| machine.peek(addr))
            .collect();
        SioData::Send(&send)
    } else if dstats & DIR_RECEIVE != 0 {
        SioData::Receive(len)
    } else {
        SioData::None
    };
    let SioOutcome::Done { status, data } = machine.sio.transact(&SioRequest { frame, data })
    else {
        return;
    };
    let mut bus = CpuBus {
        mem: &mut machine.mem,
        antic: &mut machine.antic,
        gtia: &mut machine.gtia,
        pokey: &mut machine.pokey,
        pia: &mut machine.pia,
        data: machine.data_bus,
        access: None,
    };
    for (addr, byte) in (buffer..=u16::MAX).chain(0..buffer).zip(data) {
        bus.write(addr, byte);
    }
    bus.write(DSTATS, status);
    let regs = machine.cpu.registers_mut();
    regs.y = status;
    regs.p = (regs.p & !(FLAG_N | FLAG_Z)) | (status & FLAG_N);
    let low = machine
        .mem
        .ram_byte(0x0100 | u16::from(regs.s.wrapping_add(1)));
    let high = machine
        .mem
        .ram_byte(0x0100 | u16::from(regs.s.wrapping_add(2)));
    regs.s = regs.s.wrapping_add(2);
    regs.pc = u16::from_le_bytes([low, high]).wrapping_add(1);
}
