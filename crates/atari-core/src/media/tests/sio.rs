//! Planned tests of the SIO bus: command frames, checksums and reply timing
//! (`docs/design/media.md`, "Unit test method for the SIO bus").

use crate::planned::planned;

/// Verifies REQ-615: the Status frame `31 53 00 00 84`, sent while the command line is low,
/// gets `41` and the Status reply after the command line goes high (criterion 1).
#[test]
#[ignore = "planned: SLICE-05"]
fn req615_status_frame_reply() {
    planned();
}

/// Verifies REQ-615: the Status frame sent with the command line high gets no reply
/// (criterion 2).
#[test]
#[ignore = "planned: SLICE-05"]
fn req615_frame_needs_command_line() {
    planned();
}

/// Verifies REQ-615: a partial frame, a new falling edge of the command line and a full
/// Status frame give one Status reply only, after the second rising edge (criterion 3).
#[test]
#[ignore = "planned: SLICE-05"]
fn req615_frame_restart() {
    planned();
}

/// Verifies REQ-615: Status frames at bit periods of 90 and 98 cycles get the reply; a
/// frame with byte 2 at 89 or 99 cycles gets no reply (criteria 4, 5).
#[test]
#[ignore = "planned: SLICE-05"]
fn req615_baud_tolerance() {
    planned();
}

/// Verifies REQ-616: the checksums of `31 52 01 00` (`$84`), `FF 02` (`$02`), `FF FF`
/// (`$FF`), 128 bytes of `$FF` (`$FF`), 128 bytes of `$00` and the empty frame (`$00`)
/// (criteria 1-6).
#[test]
#[ignore = "planned: SLICE-05"]
fn req616_checksum_values() {
    planned();
}

/// Verifies REQ-617: a frame for device `$32`, a frame with a bad checksum, a frame with a
/// framing error and a frame of four bytes give no byte, and a Status frame after each one
/// gets the full reply (criteria 1-5).
#[test]
#[ignore = "planned: SLICE-05"]
fn req617_ignored_frames() {
    planned();
}

/// Verifies REQ-618: the start bit of the ACK byte `41` begins at or after the rising edge
/// of the command line, for NTSC and PAL (criteria 1, 4).
#[test]
#[ignore = "planned: SLICE-05"]
fn req618_ack_after_deassert() {
    planned();
}

/// Verifies REQ-618: the start bit of the ACK byte begins no more than 28,636 (NTSC) or
/// 28,375 (PAL) machine cycles after the command line goes high (criteria 2, 4).
#[test]
#[ignore = "planned: SLICE-05"]
fn req618_ack_within_16ms() {
    planned();
}

/// Verifies REQ-618: the NAK byte `4E` for the frame `31 00 00 00 31` has the timing of
/// criteria 1 and 2, for NTSC and PAL (criteria 3, 4).
#[test]
#[ignore = "planned: SLICE-05"]
fn req618_nak_timing() {
    planned();
}

/// Verifies REQ-619: the reply bytes of Status, Read sector 1 (with and without an image),
/// Format and Read Percom block with the SD test image (criteria 1-3, 5, 6).
#[test]
#[ignore = "planned: SLICE-05"]
fn req619_read_sequence() {
    planned();
}

/// Verifies REQ-619: Put sector 1 and Write Percom block: `41`, the data frame from the
/// computer, then `41` and `43` (criteria 4, 7).
#[test]
#[ignore = "planned: SLICE-05"]
fn req619_write_sequence() {
    planned();
}

/// Verifies REQ-620: a Put data frame with a bad checksum gets `41`, `4E` and then no byte
/// for 100 ms; the sector and the change flag stay the same, and the next Status gives
/// `02 FF E0 00` (criteria 1-3).
#[test]
#[ignore = "planned: SLICE-05"]
fn req620_data_checksum_nak() {
    planned();
}

/// Verifies REQ-621: the Complete byte `43` (Status, Read, Put) and the Error byte `45`
/// (empty drive) begin at least 448 (NTSC) or 444 (PAL) cycles after the ACK byte
/// (criteria 1-3).
#[test]
#[ignore = "planned: SLICE-05"]
fn req621_complete_delay() {
    planned();
}

/// Verifies REQ-622: NTSC: a bit period of 91 cycles, 983 cycles from start bit to start bit
/// in the data frame, and the first data byte 983 cycles after the Complete byte
/// (criteria 1-4).
#[test]
#[ignore = "planned: SLICE-05"]
fn req622_byte_timing_ntsc() {
    planned();
}

/// Verifies REQ-622: PAL: a bit period of 90 cycles, 974 cycles from start bit to start bit
/// in the data frame, and the first data byte 974 cycles after the Complete byte
/// (criteria 1-4).
#[test]
#[ignore = "planned: SLICE-05"]
fn req622_byte_timing_pal() {
    planned();
}

/// Verifies REQ-624: the Status frame with ID `$31` gets the reply, and the IDs `$00`, `$30`,
/// `$32`, `$3F`, `$40`, `$4F`, `$50`, `$5F` and `$FF` get no byte for 100 ms (criteria 1, 2).
#[test]
#[ignore = "planned: SLICE-05"]
fn req624_only_device_31() {
    planned();
}
