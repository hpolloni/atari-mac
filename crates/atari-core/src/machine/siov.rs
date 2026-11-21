//! The SIOV shortcut (REQ-623, `docs/design/media.md`, "SIOV shortcut").
//!
//! At the opcode fetch of the instruction at SIOV, with the OS ROM enabled, the machine does
//! the request of the device control block at once with `SioBus::transact`. For a device
//! that the emulator does not have, the OS code runs.
//!
//! The tape shortcut (REQ-644, `docs/design/media.md`, "Tape shortcut") uses the same hook:
//! a cassette read (`DDEVIC` = `$60`, `DCOMND` = `$52`) gets the next data record of the
//! tape at once.

use super::{CpuBus, Machine, PORTB_OS_ROM};
use crate::cpu::Bus as _;
use crate::events::{self, record};
use crate::media::{CommandFrame, SioData, SioOutcome, SioRequest, TapeBlock, sio_checksum};

/// The OS SIO entry point (TRN OS-APP-J).
const SIOV: u16 = 0xE459;

/// The device control block: `DDEVIC` at `$0300` to `DAUX2` at `$030B`.
const DCB: u16 = 0x0300;

/// The length of the device control block.
const DCB_LEN: usize = 12;

/// `DSTATS`: the direction in, the status out.
const DSTATS: u16 = 0x0303;

/// The `DDEVIC` value of the cassette, which the OS sends to its cassette code (OI-046:
/// `$60`; with `DUNIT` = 0 the bus ID is the `$5F` of HRM 9.1). The SIOV shortcut never
/// handles it; only the tape shortcut does.
const DDEVIC_CASSETTE: u8 = 0x60;

/// `DCOMND` of a cassette read (REQ-644).
const DCOMND_READ: u8 = 0x52;

/// The status of a complete request (REQ-623).
const STATUS_COMPLETE: u8 = 0x01;
/// The timeout status: no record remains on the tape (REQ-644 criterion 3).
const STATUS_TIMEOUT: u8 = 0x8A;
/// The checksum error status (REQ-644 criteria 2, 3).
const STATUS_CHECKSUM: u8 = 0x8F;

/// `DSTATS` bit 7: the computer sends a data frame.
const DIR_SEND: u8 = 0x80;
/// `DSTATS` bit 6: the computer receives a data frame.
const DIR_RECEIVE: u8 = 0x40;

/// The N flag of the processor status.
const FLAG_N: u8 = 0x80;
/// The Z flag of the processor status.
const FLAG_Z: u8 = 0x02;

/// Does the SIOV shortcut or the tape shortcut if the CPU is about to execute the
/// instruction at SIOV with the OS ROM enabled and the device control block names an
/// emulated device or a cassette read. The caller checks that one of the shortcuts is on
/// and that the next tick is an opcode fetch.
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
    let buffer = u16::from_le_bytes([buf_lo, buf_hi]);
    let len = usize::from(u16::from_le_bytes([len_lo, len_hi]));
    if ddevic == DDEVIC_CASSETTE {
        if machine.fast_tape && dcomnd == DCOMND_READ && machine.sio.tape_deck().tape().is_some() {
            tape_read(machine, buffer, len);
        }
        return;
    }
    if !machine.fast_sio {
        return;
    }
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
    let outcome = machine.sio.transact(&SioRequest { frame, data });
    // The shortcut runs in the cycle in progress, number `cycles + 1` (REQ-084).
    let (log_frame, now) = (machine.frames, machine.cycles + 1);
    let CommandFrame {
        device,
        command,
        aux1,
        aux2,
    } = frame;
    let SioOutcome::Done { status, data } = outcome else {
        record!(
            info,
            events::SIO,
            log_frame,
            now,
            "SIOV device ${device:02X} command ${command:02X} aux1 ${aux1:02X} aux2 ${aux2:02X} no device"
        );
        return;
    };
    record!(
        info,
        events::SIO,
        log_frame,
        now,
        "SIOV device ${device:02X} command ${command:02X} aux1 ${aux1:02X} aux2 ${aux2:02X} status ${status:02X}"
    );
    // D1: got a command: `start_xex` releases OPTION (REQ-612).
    if machine.xex_option_until.is_some() {
        machine.release_xex_option();
    }
    finish(machine, buffer, &data, status);
}

/// The tape shortcut (REQ-644): reads the next data record of the tape into the buffer at
/// `buffer` (at most `len` bytes), moves the tape to the end of the record and returns to
/// the caller with the status.
fn tape_read(machine: &mut Machine, buffer: u16, len: usize) {
    // The shortcut runs in the cycle in progress, number `cycles + 1` (REQ-084). The tape
    // continues from the next cycle.
    let (log_frame, now) = (machine.frames, machine.cycles + 1);
    let deck = machine.sio.tape_deck();
    let record = deck
        .next_record(deck.position(now))
        .and_then(|(index, end)| {
            let tape = deck.tape()?;
            match tape.blocks().get(index)? {
                TapeBlock::Data { bytes, .. } => Some((index, end, bytes.clone())),
                TapeBlock::Signal { .. } => None,
            }
        });
    let Some((index, end, bytes)) = record else {
        record!(
            info,
            events::TAPE,
            log_frame,
            now,
            "tape shortcut: no record, status ${STATUS_TIMEOUT:02X}"
        );
        finish(machine, buffer, &[], STATUS_TIMEOUT);
        machine.plan_tape_mark(now + 1);
        return;
    };
    let data = bytes.get(..len).unwrap_or(&bytes);
    let status = match bytes.get(len) {
        Some(&sum) if sio_checksum(data) == sum => STATUS_COMPLETE,
        _ => STATUS_CHECKSUM,
    };
    record!(
        info,
        events::TAPE,
        log_frame,
        now,
        "tape shortcut: record {index}, {} bytes, status ${status:02X}",
        bytes.len()
    );
    machine.sio.tape_deck_mut().seek(end, now + 1);
    finish(machine, buffer, data, status);
    machine.plan_tape_mark(now + 1);
}

/// Ends a shortcut: writes `data` to the buffer at `buffer` and `status` to `DSTATS` and Y,
/// sets the N flag from bit 7 of `status`, and returns to the address after the `JSR` of
/// the caller (REQ-623, REQ-644).
fn finish(machine: &mut Machine, buffer: u16, data: &[u8], status: u8) {
    let mut bus = CpuBus {
        mem: &mut machine.mem,
        antic: &mut machine.antic,
        gtia: &mut machine.gtia,
        pokey: &mut machine.pokey,
        pia: &mut machine.pia,
        data: machine.data_bus,
        access: None,
    };
    for (addr, &byte) in (buffer..=u16::MAX).chain(0..buffer).zip(data) {
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
