//! Cartridge and executable file formats.
//!
//! Design: `docs/design/media.md`. Requirements: REQ-600 to REQ-638. The cartridge
//! interface is decided in OI-003. Only the standard 8 KB and 16 KB cartridges exist now.

#![expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    reason = "WP-0 stubs: delete this when the work package replaces the stubs"
)]

use core::error::Error;
use core::fmt;

/// An error in a media file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaError {
    /// The file has a size that no cartridge kind has.
    BadSize(usize),
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSize(size) => write!(f, "unsupported cartridge size: {size} bytes"),
        }
    }
}

impl Error for MediaError {}

/// The kind of a cartridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartKind {
    /// 8 KB at `$A000-$BFFF`.
    Standard8k,
    /// 16 KB at `$8000-$BFFF`.
    Standard16k,
}

/// A cartridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartridge {
    /// The kind.
    kind: CartKind,
    /// The ROM contents.
    rom: Vec<u8>,
}

impl Cartridge {
    /// Creates a cartridge from a raw ROM image of 8192 or 16384 bytes (REQ-604).
    ///
    /// # Errors
    ///
    /// [`MediaError::BadSize`] for any other size.
    pub fn from_raw(data: Vec<u8>) -> Result<Self, MediaError> {
        let kind = match data.len() {
            0x2000 => CartKind::Standard8k,
            0x4000 => CartKind::Standard16k,
            size => return Err(MediaError::BadSize(size)),
        };
        Ok(Self { kind, rom: data })
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> CartKind {
        self.kind
    }

    /// CPU read of `$8000-$BFFF`. `None` if the cartridge does not drive the bus there.
    pub fn read(&mut self, addr: u16) -> Option<u8> {
        self.peek(addr)
    }

    /// The same value as `read`, without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> Option<u8> {
        let offset = match (self.kind, addr) {
            (CartKind::Standard8k, 0xA000..=0xBFFF) => addr - 0xA000,
            (CartKind::Standard16k, 0x8000..=0xBFFF) => addr - 0x8000,
            _ => return None,
        };
        self.rom.get(usize::from(offset)).copied()
    }

    /// CPU write to `$8000-$BFFF` while the cartridge shows ROM there.
    pub const fn write(&mut self, _addr: u16, _value: u8) {}

    /// One bus access to `$D500-$D5FF`, from the CPU or from ANTIC DMA. Returns `Some` only
    /// for a read that the cartridge drives.
    pub const fn access_control(&mut self, _addr: u16, _value: Option<u8>) -> Option<u8> {
        None
    }

    /// `true` while the cartridge maps `$8000-$9FFF`.
    #[must_use]
    pub const fn rd4(&self) -> bool {
        matches!(self.kind, CartKind::Standard16k)
    }

    /// `true` while the cartridge maps `$A000-$BFFF`.
    #[must_use]
    pub const fn rd5(&self) -> bool {
        true
    }

    /// Power-on state (REQ-607).
    pub const fn cold_reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::{CartKind, Cartridge, MediaError};

    /// Verifies REQ-604: a raw image of 8 KB or 16 KB is a standard cartridge.
    #[test]
    fn raw_sizes_select_kind() {
        let small = Cartridge::from_raw(vec![1; 0x2000]).unwrap();
        assert_eq!(small.kind(), CartKind::Standard8k, "8 KB");
        let big = Cartridge::from_raw(vec![2; 0x4000]).unwrap();
        assert_eq!(big.kind(), CartKind::Standard16k, "16 KB");
        assert_eq!(Cartridge::from_raw(vec![0; 100]), Err(MediaError::BadSize(100)), "100 bytes");
        assert_eq!(MediaError::BadSize(100).to_string(), "unsupported cartridge size: 100 bytes");
    }

    /// Verifies REQ-600: the standard cartridges map `$A000-$BFFF`, and 16 KB also `$8000`.
    #[test]
    fn standard_mapping() {
        let data: Vec<u8> = (0..0x4000_usize).map(|i| u8::try_from(i >> 8).unwrap()).collect();
        let mut big = Cartridge::from_raw(data).unwrap();
        assert_eq!(big.read(0x8000), Some(0x00), "first byte");
        assert_eq!(big.peek(0xBFFF), Some(0x3F), "last byte");
        assert!(big.rd4() && big.rd5(), "RD4 and RD5");
        let mut small = Cartridge::from_raw(vec![0xCC; 0x2000]).unwrap();
        assert_eq!(small.peek(0x9FFF), None, "8 KB does not map $8000-$9FFF");
        assert_eq!(small.peek(0xA000), Some(0xCC), "8 KB at $A000");
        assert!(!small.rd4() && small.rd5(), "RD5 only");
        small.write(0xA000, 0);
        small.cold_reset();
        assert_eq!(small.access_control(0xD500, None), None, "no bus drive at $D5xx");
        assert_eq!(small.peek(0xA000), Some(0xCC), "ROM unchanged");
    }
}
