//! The disk drive D1: and its command semantics (`docs/design/media.md`, "SIO bus and disk
//! drive", "Drive commands" and "Seek and verify timing").
//!
//! `DiskDrive` decides the acceptance (ACK or NAK) of each command frame and executes the
//! command on the inserted image. It has no byte timing: [`super::SioBus`] schedules the
//! bytes. The real serial path and the SIOV shortcut both call `accept` and `execute`.

use super::disk::{DiskImage, Geometry};
use super::sio::CommandFrame;

/// Format disk (REQ-632).
const CMD_FORMAT: u8 = 0x21;
/// Format medium density (REQ-632).
const CMD_FORMAT_MEDIUM: u8 = 0x22;
/// Read Percom block (REQ-634).
const CMD_READ_PERCOM: u8 = 0x4E;
/// Write Percom block (REQ-635).
const CMD_WRITE_PERCOM: u8 = 0x4F;
/// Put sector, with no verify (REQ-630).
const CMD_PUT: u8 = 0x50;
/// Read sector (REQ-629).
const CMD_READ: u8 = 0x52;
/// Status (REQ-627).
const CMD_STATUS: u8 = 0x53;
/// Write sector, with verify (REQ-630).
const CMD_WRITE: u8 = 0x57;

/// The size of a Percom block (REQ-634).
const PERCOM_LEN: usize = 12;

/// The data frame of a read with no disk (REQ-633 Notes) and the size of the bad sector list
/// with no image.
const NO_DISK_FRAME: usize = 128;

/// Status byte 0, bit 0: the drive sent NAK for the last command frame (REQ-627).
const ERR_NAK: u8 = 0x01;
/// Status byte 0, bit 1: the last data frame from the computer had an error (REQ-627).
const ERR_DATA_FRAME: u8 = 0x02;
/// Status byte 0, bit 3: the last command failed because the disk is write-protected
/// (REQ-627, HRM Table 44).
const ERR_WRITE_PROTECT: u8 = 0x08;

/// Status byte 2: the format timeout (REQ-627).
const FORMAT_TIMEOUT: u8 = 0xE0;

/// The time of a Read, Put or Write before the Complete or Error byte, with no head move
/// (`OP_BASE_US`, a project decision in the Timing section of the design).
const OP_BASE_US: u32 = 1_000;
/// The time of a format of the image (project decision, Timing section).
const FORMAT_US: u32 = 1_000_000;
/// The first track step and the head settle time: 20 ms + 20 ms (REQ-638).
const SEEK_FIRST_US: u32 = 40_000;
/// Each more track step: 20 ms (REQ-638).
const SEEK_STEP_US: u32 = 20_000;
/// The verify of Write: one disk revolution at 288 RPM (REQ-638).
const VERIFY_US: u32 = 208_000;

/// The sectors on one track of an SD or DD disk (REQ-638 Notes).
const SECTORS_PER_TRACK: u16 = 18;
/// The sectors on one track of an ED disk (REQ-638 Notes).
const SECTORS_PER_TRACK_ED: u16 = 26;

/// The acceptance of a command frame by the drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accept {
    /// The drive sends NAK. No more bytes follow.
    Nak,
    /// A read-type command: ACK, then Complete or Error and the data frame of `execute`.
    Read,
    /// A write-type command: ACK, then the computer sends a data frame of `len` bytes and
    /// its checksum.
    Write {
        /// The length of the data frame from the computer.
        len: usize,
    },
}

/// The result of a command after its ACK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// `true` for Complete (`$43`), `false` for Error (`$45`).
    pub ok: bool,
    /// The data frame of a read-type command.
    pub data: Option<Vec<u8>>,
    /// The time of the operation in µs, before the Complete or Error byte.
    pub busy_us: u32,
}

/// The emulated disk drive D1: (REQ-624 to REQ-638).
#[derive(Debug, Clone, Default)]
pub struct DiskDrive {
    /// The inserted image.
    image: Option<DiskImage>,
    /// Status byte 0, bits 0-3 (REQ-627).
    error_bits: u8,
    /// The geometry of the last valid Write Percom block (REQ-635). Cleared by `insert`.
    format_geometry: Option<Geometry>,
    /// The track of the head, for the seek time (REQ-638).
    head_track: u16,
}

/// The sector number of a command frame: AUX1 is the low byte, AUX2 the high byte.
const fn sector_of(frame: CommandFrame) -> u16 {
    u16::from_le_bytes([frame.aux1, frame.aux2])
}

/// The Percom block of `geometry` (REQ-634).
fn percom_block(geometry: Geometry) -> [u8; PERCOM_LEN] {
    let (tracks, per_track) = if geometry == Geometry::SD || geometry == Geometry::DD {
        (40, SECTORS_PER_TRACK)
    } else if geometry == Geometry::ED {
        (40, SECTORS_PER_TRACK_ED)
    } else {
        // An arbitrary geometry shows a single track (REQ-634 Notes).
        (1, geometry.sectors)
    };
    // Density: `$04` for ED and DD (REQ-634 table), and for each 256-byte geometry.
    let density = if geometry == Geometry::ED || geometry.sector_size == 256 {
        0x04
    } else {
        0x00
    };
    let [spt_hi, spt_lo] = per_track.to_be_bytes();
    let [size_hi, size_lo] = geometry.sector_size.to_be_bytes();
    [
        tracks, 0x00, spt_hi, spt_lo, 0x00, density, size_hi, size_lo, 0xFF, 0x00, 0x00, 0x00,
    ]
}

/// The geometry that a Write Percom block selects: SD, ED or DD (REQ-635). The drive
/// compares only the tracks, sectors per track, sides, density and sector size fields.
fn percom_geometry(block: &[u8]) -> Option<Geometry> {
    /// The compared fields: byte 0 and bytes 2-7.
    fn fields(block: &[u8]) -> Option<(u8, &[u8])> {
        Some((*block.first()?, block.get(2..8)?))
    }
    let given = fields(block)?;
    [Geometry::SD, Geometry::ED, Geometry::DD]
        .into_iter()
        .find(|&geometry| fields(&percom_block(geometry)) == Some(given))
}

impl DiskDrive {
    /// Inserts `image`. A kept Write Percom geometry is cleared (REQ-635 criterion 5).
    pub fn insert(&mut self, image: DiskImage) {
        self.image = Some(image);
        self.format_geometry = None;
    }

    /// Removes the image and gives it back.
    pub const fn eject(&mut self) -> Option<DiskImage> {
        self.image.take()
    }

    /// The inserted image.
    #[must_use]
    pub const fn image(&self) -> Option<&DiskImage> {
        self.image.as_ref()
    }

    /// The inserted image, for changes by the frontend (for example the write protection).
    pub const fn image_mut(&mut self) -> Option<&mut DiskImage> {
        self.image.as_mut()
    }

    /// `true` if `sector` is a valid sector number: 1 to the sector count of the image, or
    /// 1-65535 with no image (REQ-628, REQ-633).
    fn sector_in_range(&self, sector: u16) -> bool {
        self.image.as_ref().map_or(sector != 0, |image| {
            image.geometry().sector_len(sector).is_some()
        })
    }

    /// Command acceptance (ACK or NAK) and the data frame direction (REQ-628). A NAK sets
    /// status byte 0 bit 0.
    pub fn accept(&mut self, frame: CommandFrame) -> Accept {
        let sector = sector_of(frame);
        let accept = match frame.command {
            CMD_STATUS | CMD_FORMAT | CMD_FORMAT_MEDIUM | CMD_READ_PERCOM => Accept::Read,
            CMD_READ if self.sector_in_range(sector) => Accept::Read,
            CMD_PUT | CMD_WRITE if self.sector_in_range(sector) => Accept::Write {
                len: self
                    .image
                    .as_ref()
                    .and_then(|image| image.geometry().sector_len(sector))
                    .unwrap_or(NO_DISK_FRAME),
            },
            CMD_WRITE_PERCOM => Accept::Write { len: PERCOM_LEN },
            _ => Accept::Nak,
        };
        if accept == Accept::Nak {
            self.error_bits |= ERR_NAK;
        }
        accept
    }

    /// A data frame from the computer had a checksum error (REQ-620): status byte 0 bit 1.
    pub const fn data_frame_error(&mut self) {
        self.error_bits |= ERR_DATA_FRAME;
    }

    /// Command execution after the ACK, and after the data frame of a write-type command
    /// (REQ-627, REQ-629 to REQ-635, REQ-638). A successful command other than Status clears
    /// status byte 0 bits 0-3.
    pub fn execute(&mut self, frame: CommandFrame, data: Option<&[u8]>) -> Completion {
        let completion = match frame.command {
            CMD_STATUS => {
                let completion = Completion {
                    ok: true,
                    data: Some(self.status_frame().to_vec()),
                    busy_us: 0,
                };
                self.error_bits = 0;
                return completion;
            }
            CMD_READ => self.read(sector_of(frame)),
            CMD_PUT | CMD_WRITE => self.write(sector_of(frame), data, frame.command == CMD_WRITE),
            CMD_FORMAT | CMD_FORMAT_MEDIUM => self.format(frame.command),
            CMD_READ_PERCOM => Completion {
                ok: true,
                data: Some(
                    percom_block(
                        self.image
                            .as_ref()
                            .map_or(Geometry::SD, DiskImage::geometry),
                    )
                    .to_vec(),
                ),
                busy_us: 0,
            },
            CMD_WRITE_PERCOM => self.write_percom(data),
            _ => Completion {
                ok: false,
                data: None,
                busy_us: 0,
            },
        };
        if completion.ok {
            self.error_bits = 0;
        }
        completion
    }

    /// The four bytes of the status frame (REQ-627).
    fn status_frame(&self) -> [u8; 4] {
        let (byte0, byte1) = self.image.as_ref().map_or((0x00, 0x7F), |image| {
            let geometry = image.geometry();
            let mut byte0 = 0;
            if geometry == Geometry::ED {
                byte0 |= 0x80;
            }
            if geometry.sector_size == 256 {
                byte0 |= 0x20;
            }
            let byte1 = if image.write_protected() { 0xBF } else { 0xFF };
            (byte0, byte1)
        });
        [byte0 | self.error_bits, byte1, FORMAT_TIMEOUT, 0x00]
    }

    /// The seek time to the track of `sector`, and the new head track (REQ-638).
    fn seek_us(&mut self, geometry: Geometry, sector: u16) -> u32 {
        let per_track = if geometry == Geometry::ED {
            SECTORS_PER_TRACK_ED
        } else {
            SECTORS_PER_TRACK
        };
        let track = sector.saturating_sub(1) / per_track;
        let moves = u32::from(track.abs_diff(self.head_track));
        self.head_track = track;
        if moves == 0 {
            0
        } else {
            SEEK_FIRST_US + SEEK_STEP_US * (moves - 1)
        }
    }

    /// Read sector (REQ-629). With no image: Error and 128 bytes of `$00` (REQ-633).
    fn read(&mut self, sector: u16) -> Completion {
        let Some(geometry) = self.image.as_ref().map(DiskImage::geometry) else {
            return Completion {
                ok: false,
                data: Some(vec![0; NO_DISK_FRAME]),
                busy_us: OP_BASE_US,
            };
        };
        let busy_us = OP_BASE_US + self.seek_us(geometry, sector);
        let data = self
            .image
            .as_ref()
            .and_then(|image| image.sector(sector))
            .map(<[u8]>::to_vec);
        Completion {
            ok: data.is_some(),
            data: Some(data.unwrap_or_else(|| vec![0; NO_DISK_FRAME])),
            busy_us,
        }
    }

    /// Put or Write sector (REQ-630, REQ-631, REQ-633). Write adds one revolution for the
    /// verify (REQ-638).
    fn write(&mut self, sector: u16, data: Option<&[u8]>, verify: bool) -> Completion {
        let error = Completion {
            ok: false,
            data: None,
            busy_us: OP_BASE_US,
        };
        let Some(image) = self.image.as_ref() else {
            return error;
        };
        if image.write_protected() {
            self.error_bits |= ERR_WRITE_PROTECT;
            return error;
        }
        let geometry = image.geometry();
        let mut busy_us = OP_BASE_US + self.seek_us(geometry, sector);
        if verify {
            busy_us += VERIFY_US;
        }
        let ok = match (self.image.as_mut(), data) {
            (Some(image), Some(data)) => image.write_sector(sector, data),
            _ => false,
        };
        Completion {
            ok,
            data: None,
            busy_us,
        }
    }

    /// Format `$21` or Format medium `$22` (REQ-632, REQ-631, REQ-633). The data frame is
    /// the bad sector list: all bytes `$FF`, of the new sector size.
    fn format(&mut self, command: u8) -> Completion {
        let Some(image) = self.image.as_mut() else {
            return Completion {
                ok: false,
                data: Some(vec![0xFF; NO_DISK_FRAME]),
                busy_us: 0,
            };
        };
        if image.write_protected() {
            let size = usize::from(image.geometry().sector_size);
            self.error_bits |= ERR_WRITE_PROTECT;
            return Completion {
                ok: false,
                data: Some(vec![0xFF; size]),
                busy_us: 0,
            };
        }
        let geometry = if command == CMD_FORMAT_MEDIUM {
            Geometry::ED
        } else if self.format_geometry == Some(Geometry::DD) {
            Geometry::DD
        } else {
            Geometry::SD
        };
        image.format(geometry);
        self.head_track = 0;
        Completion {
            ok: true,
            data: Some(vec![0xFF; usize::from(geometry.sector_size)]),
            busy_us: FORMAT_US,
        }
    }

    /// Write Percom block (REQ-635): keeps an SD, ED or DD geometry for the next Format
    /// `$21`. Another block gives Error, but with no image the drive always gives Complete.
    fn write_percom(&mut self, data: Option<&[u8]>) -> Completion {
        let geometry = data.and_then(percom_geometry);
        if geometry.is_some() {
            self.format_geometry = geometry;
        }
        Completion {
            ok: geometry.is_some() || self.image.is_none(),
            data: None,
            busy_us: 0,
        }
    }
}
