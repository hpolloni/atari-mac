//! Save states (REQ-018): the byte format of the machine state.
//!
//! Each chip module writes and reads its own fields with the `State` trait. The macros
//! `impl_state` and `impl_state_enum` list the fields of a struct and the variants of an
//! enum. The struct macro destructures the struct without `..`, so a new field that is not
//! in the list (or in its `skip` list) is a compile error. `Machine::save_state` adds the
//! header and the checksum (`docs/design/architecture.md`, "Save states").

use core::error::Error;
use core::fmt;
use std::collections::VecDeque;

/// An error of `Machine::load_state`. The machine does not change when a load fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateError {
    /// The bytes do not start with the save state signature.
    Signature,
    /// The format version is not the version of this build.
    Version(u16),
    /// The bytes end before the state is complete.
    Truncated,
    /// The checksum does not match the bytes.
    Checksum,
    /// The state is from a machine with other ROMs, another video standard or another
    /// audio sample rate.
    Config,
    /// A value in the state is not valid (for example an unknown enum value).
    Invalid,
    /// The bytes continue after the end of the state.
    TrailingBytes,
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Signature => write!(f, "not a save state"),
            Self::Version(version) => write!(f, "save state version {version} is not supported"),
            Self::Truncated => write!(f, "the save state is truncated"),
            Self::Checksum => write!(f, "the save state checksum does not match"),
            Self::Config => write!(
                f,
                "the save state is from a machine with another configuration"
            ),
            Self::Invalid => write!(f, "the save state has an invalid value"),
            Self::TrailingBytes => write!(f, "the save state has bytes after its end"),
        }
    }
}

impl Error for StateError {}

/// The bytes of a save state, written in order.
#[derive(Debug, Default)]
pub(crate) struct Writer {
    /// The bytes so far.
    bytes: Vec<u8>,
}

impl Writer {
    /// The bytes written.
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Appends raw bytes.
    pub(crate) fn put(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// Appends a length or an index as a `u64`.
    fn put_len(&mut self, len: usize) {
        self.put(&(len as u64).to_le_bytes());
    }
}

/// Reads the bytes of a save state in order.
#[derive(Debug)]
pub(crate) struct Reader<'a> {
    /// The bytes not read yet.
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    /// A reader at the start of `bytes`.
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    /// `true` when all bytes are read.
    pub(crate) const fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    /// The next `N` bytes.
    pub(crate) fn take<const N: usize>(&mut self) -> Result<[u8; N], StateError> {
        let (head, rest) = self
            .rest
            .split_first_chunk::<N>()
            .ok_or(StateError::Truncated)?;
        self.rest = rest;
        Ok(*head)
    }

    /// A length written by `Writer::put_len`. Each element takes at least one byte, so a
    /// length larger than the bytes that are left is truncated data.
    fn take_len(&mut self) -> Result<usize, StateError> {
        let len = usize::try_from(u64::from_le_bytes(self.take()?))
            .map_err(|_overflow| StateError::Invalid)?;
        if len > self.rest.len() {
            return Err(StateError::Truncated);
        }
        Ok(len)
    }
}

/// A value that is part of the save state. `load` reads the value in place, in the same
/// order as `save` wrote it.
pub(crate) trait State {
    /// Writes the value.
    fn save(&self, w: &mut Writer);
    /// Reads the value.
    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError>;
}

/// `State` for a primitive number type, as little-endian bytes.
macro_rules! impl_state_number {
    ($($ty:ty),*) => {
        $(
            impl State for $ty {
                fn save(&self, w: &mut Writer) {
                    w.put(&self.to_le_bytes());
                }
                fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
                    *self = Self::from_le_bytes(r.take()?);
                    Ok(())
                }
            }
        )*
    };
}

impl_state_number!(u8, u16, u32, u64, f32, f64);

impl State for bool {
    fn save(&self, w: &mut Writer) {
        w.put(&[u8::from(*self)]);
    }
    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        *self = match r.take::<1>()? {
            [0] => false,
            [1] => true,
            _ => return Err(StateError::Invalid),
        };
        Ok(())
    }
}

impl State for usize {
    fn save(&self, w: &mut Writer) {
        w.put_len(*self);
    }
    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        *self = Self::try_from(u64::from_le_bytes(r.take()?))
            .map_err(|_overflow| StateError::Invalid)?;
        Ok(())
    }
}

// A text as its UTF-8 bytes, in the format of `Vec<u8>` (the description of a tape).
impl State for String {
    fn save(&self, w: &mut Writer) {
        w.put_len(self.len());
        w.put(self.as_bytes());
    }
    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        let mut bytes = Vec::<u8>::new();
        bytes.load(r)?;
        *self = Self::from_utf8(bytes).map_err(|_bad| StateError::Invalid)?;
        Ok(())
    }
}

impl<T: State, const N: usize> State for [T; N] {
    fn save(&self, w: &mut Writer) {
        for item in self {
            item.save(w);
        }
    }
    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        for item in self {
            item.load(r)?;
        }
        Ok(())
    }
}

impl<A: State, B: State> State for (A, B) {
    fn save(&self, w: &mut Writer) {
        self.0.save(w);
        self.1.save(w);
    }
    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        self.0.load(r)?;
        self.1.load(r)
    }
}

impl<T: State + Default> State for Option<T> {
    fn save(&self, w: &mut Writer) {
        match self {
            None => w.put(&[0]),
            Some(value) => {
                w.put(&[1]);
                value.save(w);
            }
        }
    }
    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        match r.take::<1>()? {
            [0] => *self = None,
            [1] => self.get_or_insert_with(T::default).load(r)?,
            _ => return Err(StateError::Invalid),
        }
        Ok(())
    }
}

impl<T: State + Default> State for Vec<T> {
    fn save(&self, w: &mut Writer) {
        w.put_len(self.len());
        for item in self {
            item.save(w);
        }
    }
    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        let len = r.take_len()?;
        self.resize_with(len, T::default);
        for item in self {
            item.load(r)?;
        }
        Ok(())
    }
}

impl<T: State + Default> State for VecDeque<T> {
    fn save(&self, w: &mut Writer) {
        w.put_len(self.len());
        for item in self {
            item.save(w);
        }
    }
    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        let len = r.take_len()?;
        self.resize_with(len, T::default);
        for item in self {
            item.load(r)?;
        }
        Ok(())
    }
}

/// Implements [`State`] for a struct: `impl_state!(Type { field, ... })` saves the fields
/// in the order of the list. `impl_state!(Type { field, ... } skip { field, ... })` does not
/// save the fields of the `skip` list (for example a constant table), and
/// `impl_state!(Type { field, ... } test_only { field, ... })` does not save the fields that
/// only the unit tests have (`#[cfg(test)]`). Each field of the struct must be in one of the
/// lists.
macro_rules! impl_state {
    ($ty:ident { $($field:ident),* $(,)? }) => {
        impl $crate::state::State for $ty {
            fn save(&self, w: &mut $crate::state::Writer) {
                let Self { $($field),* } = self;
                $( $crate::state::State::save($field, w); )*
            }
            fn load(
                &mut self,
                r: &mut $crate::state::Reader<'_>,
            ) -> Result<(), $crate::state::StateError> {
                let Self { $($field),* } = self;
                $( $crate::state::State::load($field, r)?; )*
                Ok(())
            }
        }
    };
    ($ty:ident { $($field:ident),* $(,)? } skip { $($skip:ident),* $(,)? }) => {
        impl $crate::state::State for $ty {
            #[expect(
                clippy::unneeded_field_pattern,
                reason = "the pattern names each field, so a new field must be listed"
            )]
            fn save(&self, w: &mut $crate::state::Writer) {
                let Self { $($field,)* $($skip: _),* } = self;
                $( $crate::state::State::save($field, w); )*
            }
            #[expect(
                clippy::unneeded_field_pattern,
                reason = "the pattern names each field, so a new field must be listed"
            )]
            fn load(
                &mut self,
                r: &mut $crate::state::Reader<'_>,
            ) -> Result<(), $crate::state::StateError> {
                let Self { $($field,)* $($skip: _),* } = self;
                $( $crate::state::State::load($field, r)?; )*
                Ok(())
            }
        }
    };
    ($ty:ident { $($field:ident),* $(,)? } test_only { $($test:ident),* $(,)? }) => {
        impl $crate::state::State for $ty {
            #[cfg_attr(
                test,
                expect(
                    clippy::unneeded_field_pattern,
                    reason = "the pattern names each field, so a new field must be listed"
                )
            )]
            fn save(&self, w: &mut $crate::state::Writer) {
                let Self { $($field,)* $(#[cfg(test)] $test: _),* } = self;
                $( $crate::state::State::save($field, w); )*
            }
            #[cfg_attr(
                test,
                expect(
                    clippy::unneeded_field_pattern,
                    reason = "the pattern names each field, so a new field must be listed"
                )
            )]
            fn load(
                &mut self,
                r: &mut $crate::state::Reader<'_>,
            ) -> Result<(), $crate::state::StateError> {
                let Self { $($field,)* $(#[cfg(test)] $test: _),* } = self;
                $( $crate::state::State::load($field, r)?; )*
                Ok(())
            }
        }
    };
}

/// Implements [`State`] for an enum without data: `impl_state_enum!(Type { A, B, ... })`
/// saves the index of the variant in the list. The list must name each variant.
macro_rules! impl_state_enum {
    ($ty:ident { $($variant:ident),+ $(,)? }) => {
        impl $crate::state::State for $ty {
            fn save(&self, w: &mut $crate::state::Writer) {
                /// The variants, in the order of their index.
                const ALL: &[$ty] = &[$($ty::$variant),+];
                match self {
                    $( Self::$variant )|+ => {}
                }
                let index = ALL.iter().position(|v| v == self).unwrap_or(0);
                $crate::state::State::save(&index, w);
            }
            fn load(
                &mut self,
                r: &mut $crate::state::Reader<'_>,
            ) -> Result<(), $crate::state::StateError> {
                /// The variants, in the order of their index.
                const ALL: &[$ty] = &[$($ty::$variant),+];
                let mut index = 0_usize;
                $crate::state::State::load(&mut index, r)?;
                *self = *ALL.get(index).ok_or($crate::state::StateError::Invalid)?;
                Ok(())
            }
        }
    };
}

pub(crate) use {impl_state, impl_state_enum};

/// The 64-bit FNV-1a hash of `bytes`, continued from `hash` (start with [`FNV_START`]).
pub(crate) fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    /// The FNV-1a 64-bit prime.
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    bytes
        .iter()
        .fold(hash, |h, &b| (h ^ u64::from(b)).wrapping_mul(PRIME))
}

/// The FNV-1a 64-bit offset basis.
pub(crate) const FNV_START: u64 = 0xCBF2_9CE4_8422_2325;

/// Each cycle counter of a loaded state is below this limit (REQ-061): 2^62 cycles are more
/// than 70,000 years of emulated time, so the counters cannot overflow in a run.
pub(crate) const TIME_LIMIT: u64 = 1 << 62;

#[cfg(test)]
mod tests {
    use super::{Reader, State as _, StateError, Writer};

    /// A test enum.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    enum Color {
        /// Index 0.
        #[default]
        Red,
        /// Index 1.
        Green,
    }

    impl_state_enum!(Color { Red, Green });

    /// A test struct.
    #[derive(Debug, Clone, PartialEq, Default)]
    struct Sample {
        /// A number.
        a: u16,
        /// A flag.
        b: bool,
        /// A list.
        c: Vec<Option<(u8, f64)>>,
        /// An enum.
        d: [Color; 2],
        /// Not saved.
        e: u8,
    }

    impl_state!(Sample { a, b, c, d } skip { e });

    /// Verifies REQ-018: the values round-trip, a skipped field keeps its value, and bad
    /// bytes give an error.
    #[test]
    fn values_round_trip() {
        let value = Sample {
            a: 0x1234,
            b: true,
            c: vec![None, Some((7, -1.5))],
            d: [Color::Green, Color::Red],
            e: 9,
        };
        let mut w = Writer::default();
        value.save(&mut w);
        let bytes = w.into_bytes();
        let mut copy = Sample::default();
        let mut r = Reader::new(&bytes);
        copy.load(&mut r).unwrap();
        assert!(r.is_empty(), "all bytes read");
        assert_eq!(copy, Sample { e: 0, ..value }, "round trip");

        let mut short = Reader::new(&bytes[..bytes.len() - 1]);
        assert_eq!(
            Sample::default().load(&mut short),
            Err(StateError::Truncated),
            "truncated"
        );
        let mut bad = bytes.clone();
        bad[2] = 2;
        assert_eq!(
            Sample::default().load(&mut Reader::new(&bad)),
            Err(StateError::Invalid),
            "bool 2"
        );
    }
}
