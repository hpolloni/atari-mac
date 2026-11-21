//! Disk images: the sector store of D1: (`docs/design/media.md`, "Disk images").
//!
//! This part has the geometry and the sector access that the XEX boot image needs
//! (REQ-612). The ATR load and export (REQ-625, REQ-637) and the format (REQ-632) come with
//! the disk drive.

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

    /// The size of all sector data.
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
    /// Set by the frontend. Power-on: `false`.
    write_protected: bool,
    /// `true` after a successful write since `blank`.
    changed: bool,
}

impl DiskImage {
    /// An image of `geometry` with all bytes `$00`.
    #[must_use]
    pub fn blank(geometry: Geometry) -> Self {
        Self {
            geometry,
            data: vec![0; geometry.data_len()],
            write_protected: false,
            changed: false,
        }
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

    /// `true` while the frontend protects the image against writes.
    #[must_use]
    pub const fn write_protected(&self) -> bool {
        self.write_protected
    }

    /// Protects the image against writes, or removes the protection.
    pub const fn set_write_protected(&mut self, on: bool) {
        self.write_protected = on;
    }

    /// `true` after a successful write since `blank`.
    #[must_use]
    pub const fn changed(&self) -> bool {
        self.changed
    }

    /// Clears the changed flag: for an image that the emulator made (the XEX boot image).
    pub(super) const fn clear_changed(&mut self) {
        self.changed = false;
    }
}
