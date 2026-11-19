//! Cartridge and executable file formats.
//!
//! Design: `docs/design/media.md`. Requirements: REQ-600 to REQ-638. The cartridge
//! interface is decided in OI-003. The standard 8 KB and 16 KB cartridges and the Atarimax
//! `MaxFlash` cartridges exist now.

use core::error::Error;
use core::fmt;

/// The size of the CAR file header (REQ-605).
const CAR_HEADER: usize = 16;

/// The size of one bank of a bank-switched cartridge, also the size of the `$A000-$BFFF` window.
const BANK_SIZE: usize = 0x2000;

/// An error in a media file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaError {
    /// A raw cartridge image has a size that is not 8192 or 16384 bytes.
    BadSize(usize),
    /// A CAR file is shorter than its 16-byte header.
    CarHeader,
    /// A CAR file does not start with `CART`.
    CarSignature,
    /// A CAR file has a type that the emulator does not support.
    CarType(u32),
    /// The CAR data size does not agree with the type.
    CarSize {
        /// The type in the header.
        car_type: u32,
        /// The data size of the type.
        expected: usize,
        /// The data size of the file.
        actual: usize,
    },
    /// The CAR checksum does not agree with the data.
    CarChecksum {
        /// Header bytes 8-11.
        stored: u32,
        /// The wrapping sum of the data bytes.
        computed: u32,
    },
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSize(size) => write!(f, "unsupported cartridge size: {size} bytes"),
            Self::CarHeader => write!(f, "CAR file is shorter than its 16-byte header"),
            Self::CarSignature => write!(f, "CAR file does not start with CART"),
            Self::CarType(car_type) => write!(f, "unsupported CAR cartridge type {car_type}"),
            Self::CarSize {
                car_type,
                expected,
                actual,
            } => write!(
                f,
                "CAR type {car_type} needs {expected} data bytes, not {actual} bytes"
            ),
            Self::CarChecksum { stored, computed } => write!(
                f,
                "CAR checksum ${stored:08X} does not agree with the data sum ${computed:08X}"
            ),
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
    /// Atarimax `MaxFlash` 1Mbit: 16 banks of 8 KB at `$A000-$BFFF` (REQ-608).
    MaxFlash1Mbit,
    /// Atarimax `MaxFlash` 8Mbit: 128 banks of 8 KB at `$A000-$BFFF` (REQ-608).
    MaxFlash8Mbit,
}

impl CartKind {
    /// The kind for a CAR type number: 1, 2, 41, 42. `None` for other numbers.
    #[must_use]
    pub const fn from_car_type(car_type: u32) -> Option<Self> {
        match car_type {
            1 => Some(Self::Standard8k),
            2 => Some(Self::Standard16k),
            41 => Some(Self::MaxFlash1Mbit),
            42 => Some(Self::MaxFlash8Mbit),
            _ => None,
        }
    }

    /// The ROM size in bytes: 8192, 16384, 131072, 1048576.
    #[must_use]
    pub const fn rom_size(self) -> usize {
        match self {
            Self::Standard8k => 0x2000,
            Self::Standard16k => 0x4000,
            Self::MaxFlash1Mbit => 0x2_0000,
            Self::MaxFlash8Mbit => 0x10_0000,
        }
    }

    /// `true` for the bank-switched kinds.
    const fn is_maxflash(self) -> bool {
        matches!(self, Self::MaxFlash1Mbit | Self::MaxFlash8Mbit)
    }
}

/// A cartridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartridge {
    /// The kind.
    kind: CartKind,
    /// The ROM contents.
    rom: Vec<u8>,
    /// The selected bank (bank-switched kinds only).
    bank: u8,
    /// `true` while the cartridge maps `$A000-$BFFF` (bank-switched kinds only).
    enabled: bool,
}

/// The big-endian `u32` at `offset` of `bytes`, or 0 if `bytes` is too short.
fn be32(bytes: &[u8], offset: usize) -> u32 {
    bytes
        .get(offset..offset + 4)
        .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
        .map_or(0, u32::from_be_bytes)
}

impl Cartridge {
    /// A cartridge of `kind` with `rom`, in the power-on state.
    const fn with_rom(kind: CartKind, rom: Vec<u8>) -> Self {
        Self {
            kind,
            rom,
            bank: 0,
            enabled: true,
        }
    }

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
        Ok(Self::with_rom(kind, data))
    }

    /// Creates a cartridge from a CAR file (REQ-605, REQ-606). The checks run in the order
    /// header length, signature, type, data size, checksum, and the first failure is the
    /// error. Header bytes 12-15 are ignored.
    ///
    /// # Errors
    ///
    /// [`MediaError::CarHeader`], [`MediaError::CarSignature`], [`MediaError::CarType`],
    /// [`MediaError::CarSize`] or [`MediaError::CarChecksum`].
    pub fn from_car(file: &[u8]) -> Result<Self, MediaError> {
        let (header, data) = file
            .split_at_checked(CAR_HEADER)
            .ok_or(MediaError::CarHeader)?;
        if !header.starts_with(b"CART") {
            return Err(MediaError::CarSignature);
        }
        let car_type = be32(header, 4);
        let kind = CartKind::from_car_type(car_type).ok_or(MediaError::CarType(car_type))?;
        if data.len() != kind.rom_size() {
            return Err(MediaError::CarSize {
                car_type,
                expected: kind.rom_size(),
                actual: data.len(),
            });
        }
        let stored = be32(header, 8);
        let computed = data
            .iter()
            .fold(0_u32, |sum, &byte| sum.wrapping_add(u32::from(byte)));
        if stored != computed {
            return Err(MediaError::CarChecksum { stored, computed });
        }
        Ok(Self::with_rom(kind, data.to_vec()))
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> CartKind {
        self.kind
    }

    /// CPU read of `$8000-$BFFF`. `None` if the cartridge does not drive the bus there.
    /// No supported kind has a read side effect, so this gives the same value as `peek`.
    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "the cartridge contract of OI-003: a later kind can change state on a read"
    )]
    pub fn read(&mut self, addr: u16) -> Option<u8> {
        self.peek(addr)
    }

    /// The same value as `read`, without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> Option<u8> {
        let offset = match (self.kind, addr) {
            (CartKind::Standard8k, 0xA000..=0xBFFF) => usize::from(addr - 0xA000),
            (CartKind::Standard16k, 0x8000..=0xBFFF) => usize::from(addr - 0x8000),
            (CartKind::MaxFlash1Mbit | CartKind::MaxFlash8Mbit, 0xA000..=0xBFFF)
                if self.enabled =>
            {
                usize::from(self.bank) * BANK_SIZE + usize::from(addr - 0xA000)
            }
            _ => return None,
        };
        self.rom.get(offset).copied()
    }

    /// CPU write to `$8000-$BFFF` while the cartridge shows ROM there. No supported kind
    /// changes state: the bank-switched kinds ignore flash writes (REQ-608).
    #[expect(
        clippy::unused_self,
        clippy::needless_pass_by_ref_mut,
        reason = "the cartridge contract of OI-003: a later kind can change state on a write"
    )]
    pub const fn write(&mut self, _addr: u16, _value: u8) {}

    /// One bus access to `$D500-$D5FF`, from the CPU or from ANTIC DMA. `value` is `Some`
    /// for a write, `None` for a read. Returns `Some` only for a read that the cartridge
    /// drives. No supported kind drives the bus here, so it always returns `None`.
    ///
    /// The bank-switched kinds select the bank on a read and on a write, and ignore the data of a
    /// write (HRM 8.2, REQ-608).
    pub const fn access_control(&mut self, addr: u16, _value: Option<u8>) -> Option<u8> {
        let lo = addr.to_le_bytes()[0];
        match self.kind {
            CartKind::MaxFlash1Mbit if lo < 0x20 => {
                self.bank = lo & 0x0F;
                self.enabled = lo & 0x10 == 0;
            }
            CartKind::MaxFlash8Mbit => {
                self.bank = lo & 0x7F;
                self.enabled = lo & 0x80 == 0;
            }
            _ => {}
        }
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
        !self.kind.is_maxflash() || self.enabled
    }

    /// Power-on state: bank 0, enabled (REQ-607).
    pub const fn cold_reset(&mut self) {
        self.bank = 0;
        self.enabled = true;
    }
}

#[cfg(test)]
mod tests {
    mod atr;
    mod cartridge;
    mod drive;
    mod sio;
    mod xex;

    use super::{CartKind, Cartridge, MediaError};

    /// Verifies REQ-604: a raw image of 8 KB or 16 KB is a standard cartridge.
    #[test]
    fn raw_sizes_select_kind() {
        let small = Cartridge::from_raw(vec![1; 0x2000]).unwrap();
        assert_eq!(small.kind(), CartKind::Standard8k, "8 KB");
        let big = Cartridge::from_raw(vec![2; 0x4000]).unwrap();
        assert_eq!(big.kind(), CartKind::Standard16k, "16 KB");
        assert_eq!(
            Cartridge::from_raw(vec![0; 100]),
            Err(MediaError::BadSize(100)),
            "100 bytes"
        );
        assert_eq!(
            MediaError::BadSize(100).to_string(),
            "unsupported cartridge size: 100 bytes"
        );
    }

    /// Verifies REQ-600: the standard cartridges map `$A000-$BFFF`, and 16 KB also `$8000`.
    #[test]
    fn standard_mapping() {
        let mut big = Cartridge::from_raw(offset_pattern(0x4000)).unwrap();
        assert_eq!(big.read(0x8000), Some(0x00), "first byte");
        assert_eq!(big.peek(0xBFFF), Some(0x3F), "last byte");
        assert!(big.rd4() && big.rd5(), "RD4 and RD5");
        let mut small = Cartridge::from_raw(vec![0xCC; 0x2000]).unwrap();
        assert_eq!(small.peek(0x9FFF), None, "8 KB does not map $8000-$9FFF");
        assert_eq!(small.peek(0xA000), Some(0xCC), "8 KB at $A000");
        assert!(!small.rd4() && small.rd5(), "RD5 only");
        small.write(0xA000, 0);
        small.cold_reset();
        assert_eq!(
            small.access_control(0xD500, None),
            None,
            "no bus drive at $D5xx"
        );
        assert_eq!(small.peek(0xA000), Some(0xCC), "ROM unchanged");
    }

    /// The offset pattern: byte `i` holds `(i >> 8) & $FF`.
    fn offset_pattern(size: usize) -> Vec<u8> {
        (0..size)
            .map(|i| u8::try_from((i >> 8) & 0xFF).unwrap())
            .collect()
    }

    /// The bank pattern of `kind`: each byte of bank `k` holds `k`.
    fn bank_pattern(kind: CartKind) -> Vec<u8> {
        (0..kind.rom_size())
            .map(|i| u8::try_from(i >> 13).unwrap())
            .collect()
    }

    /// A CAR file: the header with `car_type` and the checksum of `data`, then `data`.
    fn car_file(car_type: u32, data: &[u8]) -> Vec<u8> {
        let sum = data
            .iter()
            .fold(0_u32, |sum, &byte| sum.wrapping_add(u32::from(byte)));
        let mut file = b"CART".to_vec();
        file.extend(car_type.to_be_bytes());
        file.extend(sum.to_be_bytes());
        file.extend([0; 4]);
        file.extend(data);
        file
    }

    /// A `MaxFlash` cartridge of `kind` with the bank pattern, from a CAR file.
    fn maxflash(kind: CartKind) -> Cartridge {
        let car_type = match kind {
            CartKind::MaxFlash1Mbit => 41,
            _ => 42,
        };
        let cart = Cartridge::from_car(&car_file(car_type, &bank_pattern(kind))).unwrap();
        assert_eq!(cart.kind(), kind, "kind from CAR type {car_type}");
        cart
    }

    /// One access to `$D5xx`. No supported kind drives the bus there.
    fn access(cart: &mut Cartridge, addr: u16, value: Option<u8>) {
        assert_eq!(
            cart.access_control(addr, value),
            None,
            "${addr:04X}: no bus drive"
        );
    }

    /// `true` if the cartridge maps neither `$8000-$9FFF` (RD4) nor `$A000-$BFFF` (RD5).
    fn maps_nothing(cart: &Cartridge) -> bool {
        !cart.rd4()
            && !cart.rd5()
            && [0x8000, 0x9FFF, 0xA000, 0xBFFF]
                .iter()
                .all(|&addr| cart.peek(addr).is_none())
    }

    /// Checks REQ-608 criterion 8: the cartridge does not map `$8000-$9FFF`.
    fn check_no_rd4(cart: &Cartridge, state: &str) {
        assert!(!cart.rd4(), "{state}: RD4 negated");
        assert_eq!(cart.peek(0x8000), None, "{state}: $8000");
        assert_eq!(cart.peek(0x9FFF), None, "{state}: $9FFF");
    }

    /// Verifies REQ-608: `MaxFlash` 1Mbit bank switching (criteria 1-5 and 8).
    #[test]
    fn req608_maxflash1_banks() {
        let mut cart = maxflash(CartKind::MaxFlash1Mbit);
        check_no_rd4(&cart, "power-on");
        assert_eq!(cart.access_control(0xD503, None), None, "no bus drive");
        assert_eq!(cart.read(0xA000), Some(0x03), "1: $A000 after $D503");
        assert_eq!(cart.read(0xBFFF), Some(0x03), "1: $BFFF after $D503");
        check_no_rd4(&cart, "bank 3");
        access(&mut cart, 0xD50F, Some(0x00));
        assert_eq!(cart.read(0xA000), Some(0x0F), "2: after a write to $D50F");
        access(&mut cart, 0xD502, Some(0xFF));
        assert_eq!(cart.read(0xA000), Some(0x02), "2: after a write to $D502");
        access(&mut cart, 0xD510, None);
        assert!(maps_nothing(&cart), "3: after $D510");
        check_no_rd4(&cart, "disabled");
        access(&mut cart, 0xD503, None);
        assert!(cart.rd5(), "4: RD5 again");
        assert_eq!(cart.read(0xA000), Some(0x03), "4: enabled again");
        for addr in [0xD520, 0xD55F, 0xD5FF] {
            access(&mut cart, addr, None);
            access(&mut cart, addr, Some(0x00));
            assert_eq!(cart.read(0xA000), Some(0x03), "5: after ${addr:04X}");
        }
        cart.cold_reset();
        assert_eq!(cart.read(0xA000), Some(0x00), "cold reset: bank 0");
    }

    /// Verifies REQ-608: `MaxFlash` 8Mbit bank switching (criteria 6-8).
    #[test]
    fn req608_maxflash8_banks() {
        let mut cart = maxflash(CartKind::MaxFlash8Mbit);
        check_no_rd4(&cart, "power-on");
        access(&mut cart, 0xD57F, None);
        assert_eq!(cart.read(0xA000), Some(0x7F), "6: after $D57F");
        check_no_rd4(&cart, "bank $7F");
        access(&mut cart, 0xD540, None);
        assert_eq!(cart.read(0xA000), Some(0x40), "6: after $D540");
        access(&mut cart, 0xD580, None);
        assert!(maps_nothing(&cart), "7: after $D580");
        access(&mut cart, 0xD540, None);
        assert_eq!(cart.read(0xBFFF), Some(0x40), "enabled again");
        access(&mut cart, 0xD5FF, Some(0x00));
        assert!(maps_nothing(&cart), "7: after $D5FF");
        check_no_rd4(&cart, "disabled");
    }

    /// The valid type 1 file of REQ-605 criterion 1.
    fn type1_file() -> Vec<u8> {
        let file = car_file(1, &offset_pattern(0x2000));
        assert_eq!(
            file[..16],
            [
                0x43, 0x41, 0x52, 0x54, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0xF0, 0x00, 0x00, 0x00,
                0x00, 0x00
            ],
            "the header of REQ-605 criterion 1"
        );
        file
    }

    /// Verifies REQ-605: a CAR file of type 1 is a standard 8 KB cartridge (criterion 1).
    #[test]
    fn req605_car_type1() {
        let mut cart = Cartridge::from_car(&type1_file()).unwrap();
        assert_eq!(cart.kind(), CartKind::Standard8k, "kind");
        assert_eq!(cart.read(0xA100), Some(0x01), "$A100");
        assert_eq!(cart.read(0xBFFF), Some(0x1F), "$BFFF");
    }

    /// Verifies REQ-605: a CAR file of type 2 is a standard 16 KB cartridge (criterion 2).
    #[test]
    fn req605_car_type2() {
        let file = car_file(2, &offset_pattern(0x4000));
        assert_eq!(
            file[..16],
            [
                0x43, 0x41, 0x52, 0x54, 0x00, 0x00, 0x00, 0x02, 0x00, 0x07, 0xE0, 0x00, 0x00, 0x00,
                0x00, 0x00
            ],
            "the header of REQ-605 criterion 2"
        );
        let mut cart = Cartridge::from_car(&file).unwrap();
        assert_eq!(cart.kind(), CartKind::Standard16k, "kind");
        assert_eq!(cart.read(0x8100), Some(0x01), "$8100");
        assert_eq!(cart.read(0xA000), Some(0x20), "$A000");
    }

    /// Verifies REQ-605: CAR header bytes 12-15 are ignored (criterion 3).
    #[test]
    fn req605_car_unused_bytes_ignored() {
        let mut file = type1_file();
        file[12..16].copy_from_slice(&[0xFF; 4]);
        let cart = Cartridge::from_car(&file).unwrap();
        assert_eq!(cart.kind(), CartKind::Standard8k, "kind");
    }

    /// Verifies REQ-606: each CAR file defect gives its error (criteria 1-5).
    #[test]
    fn req606_car_errors() {
        let valid = type1_file();
        assert_eq!(
            Cartridge::from_car(&valid[..15]),
            Err(MediaError::CarHeader),
            "1: 15 bytes"
        );
        let mut file = valid.clone();
        file[..4].copy_from_slice(&[0x43, 0x41, 0x52, 0x58]);
        assert_eq!(
            Cartridge::from_car(&file),
            Err(MediaError::CarSignature),
            "2: CARX"
        );
        let mut file = valid.clone();
        file[4..8].copy_from_slice(&[0x00, 0x00, 0x00, 0x63]);
        assert_eq!(
            Cartridge::from_car(&file),
            Err(MediaError::CarType(99)),
            "3: type 99"
        );
        assert_eq!(
            Cartridge::from_car(&valid[..valid.len() - 1]),
            Err(MediaError::CarSize {
                car_type: 1,
                expected: 8192,
                actual: 8191
            }),
            "4: 8191 data bytes"
        );
        let mut file = valid;
        file[8..12].copy_from_slice(&[0x00, 0x01, 0xF0, 0x01]);
        assert_eq!(
            Cartridge::from_car(&file),
            Err(MediaError::CarChecksum {
                stored: 0x0001_F001,
                computed: 0x0001_F000
            }),
            "5: checksum"
        );
        for (error, text) in [
            (MediaError::CarHeader, "16-byte header"),
            (MediaError::CarSignature, "CART"),
            (MediaError::CarType(99), "type 99"),
            (
                MediaError::CarSize {
                    car_type: 1,
                    expected: 8192,
                    actual: 8191,
                },
                "8191",
            ),
            (
                MediaError::CarChecksum {
                    stored: 0x0001_F001,
                    computed: 0x0001_F000,
                },
                "$0001F001",
            ),
        ] {
            assert!(error.to_string().contains(text), "{error:?}: {error}");
        }
    }

    /// Verifies REQ-606: the first error in the order of criteria 1-5 is reported
    /// (criterion 6).
    #[test]
    fn req606_car_error_order() {
        assert_eq!(
            Cartridge::from_car(b"CARX\0\0\0\0\0\0\0\0\0\0\0"),
            Err(MediaError::CarHeader),
            "15 bytes that start with CARX"
        );
        let mut file = type1_file();
        file[..4].copy_from_slice(b"CARX");
        file[4..8].copy_from_slice(&[0, 0, 0, 99]);
        file.truncate(file.len() - 1);
        assert_eq!(
            Cartridge::from_car(&file),
            Err(MediaError::CarSignature),
            "signature before type and size"
        );
        file[..4].copy_from_slice(b"CART");
        assert_eq!(
            Cartridge::from_car(&file),
            Err(MediaError::CarType(99)),
            "type before size"
        );
        file[4..8].copy_from_slice(&[0, 0, 0, 1]);
        file[8] = 0xFF;
        assert_eq!(
            Cartridge::from_car(&file),
            Err(MediaError::CarSize {
                car_type: 1,
                expected: 8192,
                actual: 8191
            }),
            "size before checksum"
        );
    }
}
