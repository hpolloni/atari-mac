//! POKEY serial input with the OS SIO routines (REQ-526).
//!
//! The test plays a device on the SIO bus: it waits for the command frame of the OS and
//! sends the reply bytes on SIO DATA IN with `SioBus::send_device_byte`. POKEY receives the
//! bits from the line level, and the OS SIO routine reads them from `SERIN`.

use atari_core::Machine;
use atari_harness::input::type_text;
use atari_harness::screen::has_row;
use atari_harness::system::{BOOT_FRAMES, BootOptions, boot, run_until};

/// The test program: it waits for the start flag, reads 128 bytes from device `$31` with
/// command `$52` and aux `$0001` into `BUFFER` through SIOV, and stores Y and a marker.
const PROGRAM: &[u8] = &[
    0x68, // PLA (the USR argument count)
    0xAD, 0x82, 0x06, // wait: LDA START
    0xF0, 0xFB, // BEQ wait
    0xA9, 0x31, 0x8D, 0x00, 0x03, // LDA #$31, STA DDEVIC
    0xA9, 0x01, 0x8D, 0x01, 0x03, // LDA #$01, STA DUNIT
    0xA9, 0x52, 0x8D, 0x02, 0x03, // LDA #$52, STA DCOMND
    0xA9, 0x40, 0x8D, 0x03, 0x03, // LDA #$40, STA DSTATS (receive)
    0xA9, 0x00, 0x8D, 0x04, 0x03, // LDA #<BUFFER, STA DBUFLO
    0xA9, 0x07, 0x8D, 0x05, 0x03, // LDA #>BUFFER, STA DBUFHI
    0x8D, 0x06, 0x03, // STA DTIMLO (7 s)
    0xA9, 0x80, 0x8D, 0x08, 0x03, // LDA #128, STA DBYTLO
    0xA9, 0x01, 0x8D, 0x0A, 0x03, // LDA #$01, STA DAUX1
    0xA9, 0x00, 0x8D, 0x09, 0x03, // LDA #$00, STA DBYTHI
    0x8D, 0x0B, 0x03, // STA DAUX2
    0x20, 0x59, 0xE4, // JSR SIOV
    0x8C, 0x80, 0x06, // STY STATUS
    0xA9, 0xA5, 0x8D, 0x81, 0x06, // LDA #$A5, STA DONE
    0x60, // RTS
];
/// The address of the program.
const PROGRAM_AT: u16 = 0x0600;
/// The SIO status (Y) after SIOV.
const STATUS: u16 = 0x0680;
/// `$A5` when the program is done.
const DONE: u16 = 0x0681;
/// The test writes 1 here to start the SIO request.
const START: u16 = 0x0682;
/// The receive buffer.
const BUFFER: u16 = 0x0700;
/// The bit cell of the device bytes: 19200 baud (`docs/design/media.md`, "Timing").
const BIT_CYCLES: u32 = 94;
/// From the end of the command frame to the ACK: 1000 µs (`ACK_DELAY`).
const ACK_DELAY: u64 = 1790;
/// From the end of the ACK to the Complete byte: 250 µs (`COMPLETE_MIN`).
const COMPLETE_DELAY: u64 = 448;
/// The distance of the data bytes: 549 µs (`BYTE_PERIOD`).
const BYTE_PERIOD: u64 = 983;
/// The cycles of one byte on the line.
const BYTE_CYCLES: u64 = 10 * BIT_CYCLES as u64;
/// `SERIN`.
const SERIN: u16 = 0xD20D;
/// `IRQST`.
const IRQST: u16 = 0xD20E;

/// The SIO checksum: the sum of the bytes with the carry added back (HRM 9.1).
fn checksum(bytes: &[u8]) -> u8 {
    let mut sum = 0_u16;
    for byte in bytes {
        sum += u16::from(*byte);
        sum = (sum & 0xFF) + (sum >> 8);
    }
    let [low, _high] = sum.to_le_bytes();
    low
}

/// The 128 data bytes of the device.
fn data() -> Vec<u8> {
    (0..128_u8).map(|i| i ^ 0x5A).collect()
}

/// Runs cycles until the command line goes high after the OS sent `frame`. Returns the cycle.
fn wait_for_command(machine: &mut Machine, frame: &[u8], max_cycles: u64) -> Option<u64> {
    let mut low = machine.sio().command_line_low();
    for _ in 0..max_cycles {
        machine.run_cycle();
        let now_low = machine.sio().command_line_low();
        if low && !now_low && machine.sio().host_bytes() == frame {
            return Some(machine.cycle_count());
        }
        low = now_low;
    }
    None
}

/// Verifies REQ-526: System check: the OS SIO routine receives a frame from a device on SIO DATA
/// IN; each byte reaches `SERIN` with the serial input IRQ (`IRQST` bit 5), and the OS gets the
/// correct data. The test plays the device with `SioBus::send_device_byte`.
#[test]
fn os_sio_receives_frame() {
    let mut machine = boot(BootOptions::default()).unwrap();
    let ready = run_until(&mut machine, BOOT_FRAMES, |m| has_row(m, "Ready"));
    assert!(ready.is_some(), "no Ready row after {BOOT_FRAMES} frames");
    for (addr, byte) in (PROGRAM_AT..).zip(PROGRAM.iter().copied()) {
        machine.poke_ram(addr, byte);
    }
    machine.poke_ram(START, 0);
    type_text(&mut machine, "X=USR(1536)\n").unwrap();
    machine.poke_ram(START, 1);
    // The command frame: device, command, aux1, aux2, checksum.
    let mut frame = vec![0x31, 0x52, 0x01, 0x00];
    frame.push(checksum(&frame));
    let t = wait_for_command(&mut machine, &frame, 2_000_000).expect("the command frame");
    // The reply: ACK, Complete, the data frame and its checksum.
    let data = data();
    let ack = t + ACK_DELAY;
    let complete = ack + BYTE_CYCLES + COMPLETE_DELAY;
    let mut reply = vec![(b'A', ack), (b'C', complete)];
    for (k, byte) in (1..).zip(data.iter().copied().chain([checksum(&data)])) {
        reply.push((byte, complete + k * BYTE_PERIOD));
    }
    for (value, start) in &reply {
        machine
            .sio_mut()
            .send_device_byte(*value, *start, BIT_CYCLES);
    }
    // Each byte reaches `SERIN` with the serial input IRQ.
    let mut received = Vec::new();
    let mut irq = false;
    let end = reply.last().map(|(_, start)| start + BYTE_CYCLES).unwrap();
    while machine.cycle_count() < end + 100_000 && machine.peek_ram(DONE) != 0xA5 {
        machine.run_cycle();
        let now_irq = machine.peek(IRQST) & 0x20 == 0;
        if now_irq && !irq {
            received.push(machine.peek(SERIN));
        }
        irq = now_irq;
    }
    let sent: Vec<u8> = reply.iter().map(|(value, _)| *value).collect();
    assert_eq!(received, sent, "the bytes in SERIN with the IRQ");
    assert_eq!(machine.peek_ram(DONE), 0xA5, "SIOV returned");
    assert_eq!(machine.peek_ram(STATUS), 0x01, "SIOV status");
    let buffer: Vec<u8> = (BUFFER..BUFFER + 128)
        .map(|a| machine.peek_ram(a))
        .collect();
    assert_eq!(buffer, data, "the buffer");
}
