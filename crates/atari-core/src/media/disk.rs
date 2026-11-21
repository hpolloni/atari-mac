//! Disk images: the sector store of D1: (`docs/design/media.md`, "Disk images" and
//! "ATR loading").
//!
//! The ATR load (REQ-625) and its errors (REQ-626), the export (REQ-637), the format
//! (REQ-632) and the sector access. ATR-FMT gives the file format. The HRM does not describe
//! it.

use super::MediaError;
use crate::state::impl_state;

/// The size of the ATR header (REQ-625).
const ATR_HEADER: usize = 16;

/// The ATR signature word `$0296` (bytes `$96 $02`).
const ATR_SIGNATURE: u16 = 0x0296;

/// The size of sectors 1-3, which are always 128 bytes (HRM 10.1).
const BOOT_SECTOR_SIZE: usize = 128;

/// The number of boot sectors that are always 128 bytes.
const BOOT_SECTORS: u16 = 3;

/// The sector count and the sector size of a disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// The number of sectors, numbered from 1.
    pub sectors: u16,
    /// The size of sectors 4 and up: 128 or 256 bytes.
    pub sector_size: u16,
}

impl_state!(Geometry {
    sectors,
    sector_size,
});

impl Geometry {
    /// Single density: 720 sectors of 128 bytes.
    pub const SD: Self = Self {
        sectors: 720,
        sector_size: 128,
    };
    /// Enhanced density: 1040 sectors of 128 bytes.
    pub const ED: Self = Self {
        sectors: 1040,
        sector_size: 128,
    };
    /// Double density: 720 sectors of 256 bytes (sectors 1-3 of 128 bytes).
    pub const DD: Self = Self {
        sectors: 720,
        sector_size: 256,
    };

    /// The data length of sector `n`: 128 for sectors 1-3, else `sector_size`.
    /// `None` if `n` is 0 or above `sectors`.
    #[must_use]
    pub const fn sector_len(self, n: u16) -> Option<usize> {
        if n == 0 || n > self.sectors {
            None
        } else if n <= BOOT_SECTORS {
            Some(BOOT_SECTOR_SIZE)
        } else {
            Some(self.sector_size as usize)
        }
    }

    /// The offset of sector `n` in the sector data (the ATR layout).
    const fn offset(self, n: u16) -> usize {
        if n <= BOOT_SECTORS {
            (n as usize - 1) * BOOT_SECTOR_SIZE
        } else {
            BOOT_SECTORS as usize * BOOT_SECTOR_SIZE
                + (n - BOOT_SECTORS - 1) as usize * self.sector_size as usize
        }
    }

    /// The size of all sector data in the ATR layout (sectors 1-3 of 128 bytes).
    const fn data_len(self) -> usize {
        if self.sectors <= BOOT_SECTORS {
            self.sectors as usize * BOOT_SECTOR_SIZE
        } else {
            self.offset(self.sectors) + self.sector_size as usize
        }
    }
}

/// A disk image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskImage {
    /// The sector count and the sector size.
    geometry: Geometry,
    /// The sector data in the ATR layout (sectors 1-3 of 128 bytes).
    data: Vec<u8>,
    /// ATR header bytes 7-15, kept on export (REQ-637).
    header_extra: [u8; 9],
    /// The input stored sectors 1-3 as 256 bytes ("ATR loading").
    full_boot_sectors: bool,
    /// Set by the frontend. Power-on: `false`.
    write_protected: bool,
    /// `true` after a successful write or format since `from_atr` or `blank`.
    changed: bool,
}

// The image is in the save state: the writes change it, and the frontend can insert another
// image at run time.
impl_state!(DiskImage {
    geometry,
    data,
    header_extra,
    full_boot_sectors,
    write_protected,
    changed,
});

/// The little-endian 16-bit value at `offset` of `bytes`. The caller checks the length.
fn le16(bytes: &[u8], offset: usize) -> u16 {
    let low = bytes.get(offset).copied().unwrap_or(0);
    let high = bytes.get(offset + 1).copied().unwrap_or(0);
    u16::from_le_bytes([low, high])
}

impl DiskImage {
    /// Loads an ATR file (REQ-625). The bytes after the data are ignored. The errors are
    /// checked in the order of REQ-626 criterion 9, and the first one is given.
    ///
    /// # Errors
    ///
    /// `AtrHeader`, `AtrSignature`, `AtrSectorSize`, `AtrTruncated`, `AtrDataSize` or
    /// `AtrSectorCount` (REQ-626).
    pub fn from_atr(file: &[u8]) -> Result<Self, MediaError> {
        let Some((header, body)) = file.split_at_checked(ATR_HEADER) else {
            return Err(MediaError::AtrHeader);
        };
        let signature = le16(header, 0);
        if signature != ATR_SIGNATURE {
            return Err(MediaError::AtrSignature(signature));
        }
        let sector_size = le16(header, 4);
        if sector_size != 128 && sector_size != 256 {
            return Err(MediaError::AtrSectorSize(sector_size));
        }
        let units = usize::from(le16(header, 2)) | usize::from(header[6]) << 16;
        let size = units * 16;
        let Some(sector_data) = body.get(..size) else {
            return Err(MediaError::AtrTruncated {
                expected: size,
                actual: body.len(),
            });
        };
        let boot_part = usize::from(BOOT_SECTORS) * BOOT_SECTOR_SIZE;
        let (count, full_boot_sectors) = if sector_size == 128 {
            if !size.is_multiple_of(128) {
                return Err(MediaError::AtrDataSize(size));
            }
            (size / 128, false)
        } else if size >= boot_part && (size - boot_part).is_multiple_of(256) {
            (usize::from(BOOT_SECTORS) + (size - boot_part) / 256, false)
        } else if size.is_multiple_of(256) {
            (size / 256, true)
        } else {
            return Err(MediaError::AtrDataSize(size));
        };
        // Logical sector numbers are 16-bit values, so 65535 is the largest count (HRM 10.1).
        let sectors = match u16::try_from(count) {
            Ok(sectors) if sectors > 0 => sectors,
            _ => return Err(MediaError::AtrSectorCount(count)),
        };
        let geometry = Geometry {
            sectors,
            sector_size,
        };
        let data = if full_boot_sectors {
            // Keep the first 128 bytes of each 256-byte boot sector (REQ-625 Notes, "To
            // verify").
            let boot = usize::from(sectors.min(BOOT_SECTORS));
            let mut data = Vec::with_capacity(geometry.data_len());
            for chunk in sector_data.chunks(256).take(boot) {
                data.extend_from_slice(chunk.get(..BOOT_SECTOR_SIZE).unwrap_or(chunk));
            }
            data.extend_from_slice(sector_data.get(256 * boot..).unwrap_or_default());
            data
        } else {
            sector_data.to_vec()
        };
        let mut header_extra = [0; 9];
        header_extra.copy_from_slice(&header[7..ATR_HEADER]);
        Ok(Self {
            geometry,
            data,
            header_extra,
            full_boot_sectors,
            write_protected: false,
            changed: false,
        })
    }

    /// An image of `geometry` with all bytes `$00`.
    #[must_use]
    pub fn blank(geometry: Geometry) -> Self {
        Self {
            geometry,
            data: vec![0; geometry.data_len()],
            header_extra: [0; 9],
            full_boot_sectors: false,
            write_protected: false,
            changed: false,
        }
    }

    /// The ATR file bytes (REQ-637): the header from the geometry and the kept header bytes
    /// 7-15, then the sector data. An image that was loaded with 256-byte boot sectors gets
    /// sectors 1-3 as 256 bytes again, with the second half `$00`.
    #[must_use]
    pub fn to_atr(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(self.data.len() + 3 * BOOT_SECTOR_SIZE);
        if self.full_boot_sectors {
            let boot = usize::from(self.geometry.sectors.min(BOOT_SECTORS));
            let boot_end = (boot * BOOT_SECTOR_SIZE).min(self.data.len());
            let (boot_data, rest) = self.data.split_at(boot_end);
            for sector in boot_data.chunks(BOOT_SECTOR_SIZE) {
                body.extend_from_slice(sector);
                body.resize(body.len() + 256 - sector.len(), 0);
            }
            body.extend_from_slice(rest);
        } else {
            body.extend_from_slice(&self.data);
        }
        let units = body.len() / 16;
        let mut file = Vec::with_capacity(ATR_HEADER + body.len());
        file.extend_from_slice(&ATR_SIGNATURE.to_le_bytes());
        file.extend_from_slice(&units.to_le_bytes()[..2]);
        file.extend_from_slice(&self.geometry.sector_size.to_le_bytes());
        file.push(units.to_le_bytes()[2]);
        file.extend_from_slice(&self.header_extra);
        file.extend_from_slice(&body);
        file
    }

    /// The sector count and the sector size.
    #[must_use]
    pub const fn geometry(&self) -> Geometry {
        self.geometry
    }

    /// The data of sector `n`. `None` if `n` is 0 or above the sector count.
    #[must_use]
    pub fn sector(&self, n: u16) -> Option<&[u8]> {
        let len = self.geometry.sector_len(n)?;
        let start = self.geometry.offset(n);
        self.data.get(start..start + len)
    }

    /// Replaces sector `n`. Gives `false` and changes nothing if `n` or the length is wrong.
    pub fn write_sector(&mut self, n: u16, data: &[u8]) -> bool {
        let Some(len) = self.geometry.sector_len(n) else {
            return false;
        };
        if data.len() != len {
            return false;
        }
        let start = self.geometry.offset(n);
        let Some(slot) = self.data.get_mut(start..start + len) else {
            return false;
        };
        slot.copy_from_slice(data);
        self.changed = true;
        true
    }

    /// Replaces the image with a blank image of `geometry` (REQ-632). The kept ATR header
    /// bytes 7-15 and the write protection do not change.
    pub fn format(&mut self, geometry: Geometry) {
        self.geometry = geometry;
        self.data.clear();
        self.data.resize(geometry.data_len(), 0);
        self.full_boot_sectors = false;
        self.changed = true;
    }

    /// `true` while the frontend protects the image against writes.
    #[must_use]
    pub const fn write_protected(&self) -> bool {
        self.write_protected
    }

    /// Protects the image against writes, or removes the protection.
    pub const fn set_write_protected(&mut self, on: bool) {
        self.write_protected = on;
    }

    /// `true` after a successful write or format since `from_atr` or `blank`.
    #[must_use]
    pub const fn changed(&self) -> bool {
        self.changed
    }

    /// Clears the changed flag: for an image that the emulator made (the XEX boot image).
    pub(super) const fn clear_changed(&mut self) {
        self.changed = false;
    }
}
