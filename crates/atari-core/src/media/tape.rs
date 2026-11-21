//! CAS tape images (`docs/design/media.md`, "Cassette deck" and "CAS loading").
//!
//! REQ-639 gives the loading and REQ-640 the errors. The supported chunk types are `FUJI`,
//! `baud`, `data` and `fsk ` (OI-048, option 2). All numbers are little-endian (CAS-FMT).

use super::MediaError;
use crate::state::{Reader, State, StateError, Writer};

/// The size of a CAS chunk header: the type, the data length and the aux value.
const CHUNK_HEADER: usize = 8;

/// The baud rate of a `data` chunk with no `baud` chunk before it (REQ-639).
const DEFAULT_BAUD: u16 = 600;

/// One block of a tape: a `data` or an `fsk ` chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TapeBlock {
    /// A `data` chunk.
    Data {
        /// The inter-record gap before the record, in ms.
        gap_ms: u16,
        /// The baud rate of the last `baud` chunk before it, or 600.
        baud: u16,
        /// The record bytes, as the file gives them.
        bytes: Vec<u8>,
    },
    /// An `fsk ` chunk.
    Signal {
        /// The gap before the signal, in ms.
        gap_ms: u16,
        /// The signal lengths in 0.1 ms. The first signal is 0 (space), then they alternate.
        lengths: Vec<u16>,
    },
}

/// A cassette tape: the blocks of a CAS image (REQ-639).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tape {
    /// The text of the first `FUJI` chunk (UTF-8, lossy).
    description: String,
    /// One block for each `data` and `fsk ` chunk, in file order. At least one.
    blocks: Vec<TapeBlock>,
}

/// The little-endian 16-bit value at `offset`, or `None` past the end of `bytes`.
fn le16(bytes: &[u8], offset: usize) -> Option<u16> {
    let pair = bytes.get(offset..offset.checked_add(2)?)?;
    <[u8; 2]>::try_from(pair).ok().map(u16::from_le_bytes)
}

/// One chunk of a CAS file whose header and data are inside the file.
struct Chunk<'a> {
    /// The 4-byte chunk type.
    kind: [u8; 4],
    /// The aux value (header bytes 6 and 7).
    aux: u16,
    /// The data bytes.
    data: &'a [u8],
}

/// The chunk at `offset` of `file`, or `CasTruncated` if its header is not complete or its
/// data goes past the end of the file.
fn chunk_at(file: &[u8], offset: usize) -> Result<Chunk<'_>, MediaError> {
    let truncated = MediaError::CasTruncated { offset };
    let header = offset
        .checked_add(CHUNK_HEADER)
        .and_then(|end| file.get(offset..end))
        .ok_or(truncated)?;
    let kind = header
        .get(..4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .ok_or(truncated)?;
    let length = le16(header, 4).ok_or(truncated)?;
    let aux = le16(header, 6).ok_or(truncated)?;
    let start = offset + CHUNK_HEADER;
    let data = start
        .checked_add(usize::from(length))
        .and_then(|end| file.get(start..end))
        .ok_or(truncated)?;
    Ok(Chunk { kind, aux, data })
}

impl Tape {
    /// Loads a CAS image (REQ-639). The errors and their order are those of REQ-640.
    ///
    /// # Errors
    ///
    /// `CasHeader` for a file shorter than 8 bytes, `CasSignature` if the first chunk is not
    /// `FUJI`, then for each chunk in file order `CasTruncated`, `CasChunkType`, `CasBaud`
    /// and `CasFskLength`, and `CasEmpty` if the file has no `data` and no `fsk ` chunk.
    pub fn from_cas(file: &[u8]) -> Result<Self, MediaError> {
        if file.len() < CHUNK_HEADER {
            return Err(MediaError::CasHeader);
        }
        if file.get(..4) != Some(b"FUJI".as_slice()) {
            return Err(MediaError::CasSignature);
        }
        let mut description = None;
        let mut blocks = Vec::new();
        let mut baud = DEFAULT_BAUD;
        let mut offset = 0;
        while offset < file.len() {
            let chunk = chunk_at(file, offset)?;
            match &chunk.kind {
                b"FUJI" => {
                    if description.is_none() {
                        description = Some(String::from_utf8_lossy(chunk.data).into_owned());
                    }
                }
                b"baud" => {
                    if chunk.aux == 0 {
                        return Err(MediaError::CasBaud { offset });
                    }
                    baud = chunk.aux;
                }
                b"data" => blocks.push(TapeBlock::Data {
                    gap_ms: chunk.aux,
                    baud,
                    bytes: chunk.data.to_vec(),
                }),
                b"fsk " => {
                    if chunk.data.len() % 2 != 0 {
                        let length = u16::try_from(chunk.data.len()).unwrap_or(u16::MAX);
                        return Err(MediaError::CasFskLength { offset, length });
                    }
                    let lengths = chunk
                        .data
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|&pair| u16::from_le_bytes(pair))
                        .collect();
                    blocks.push(TapeBlock::Signal {
                        gap_ms: chunk.aux,
                        lengths,
                    });
                }
                _ => {
                    return Err(MediaError::CasChunkType {
                        offset,
                        chunk_type: chunk.kind,
                    });
                }
            }
            offset += CHUNK_HEADER + chunk.data.len();
        }
        if blocks.is_empty() {
            return Err(MediaError::CasEmpty);
        }
        Ok(Self {
            description: description.unwrap_or_default(),
            blocks,
        })
    }

    /// A tape with no blocks, which a save state load fills.
    pub(super) const fn empty() -> Self {
        Self {
            description: String::new(),
            blocks: Vec::new(),
        }
    }

    /// The text of the first `FUJI` chunk (UTF-8, lossy).
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The blocks of the tape, in file order. At least one.
    #[must_use]
    pub fn blocks(&self) -> &[TapeBlock] {
        &self.blocks
    }
}

// An enum with data: the index of the variant as `u64`, then the data of the variant.
impl TapeBlock {
    /// Writes the block to a save state.
    fn save(&self, w: &mut Writer) {
        match self {
            Self::Data {
                gap_ms,
                baud,
                bytes,
            } => {
                0_usize.save(w);
                gap_ms.save(w);
                baud.save(w);
                bytes.save(w);
            }
            Self::Signal { gap_ms, lengths } => {
                1_usize.save(w);
                gap_ms.save(w);
                lengths.save(w);
            }
        }
    }

    /// Reads a block from a save state.
    fn load(r: &mut Reader<'_>) -> Result<Self, StateError> {
        let mut index = 0_usize;
        index.load(r)?;
        let mut gap_ms = 0_u16;
        gap_ms.load(r)?;
        match index {
            0 => {
                let mut baud = 0_u16;
                let mut bytes = Vec::new();
                baud.load(r)?;
                bytes.load(r)?;
                if baud == 0 {
                    return Err(StateError::Invalid);
                }
                Ok(Self::Data {
                    gap_ms,
                    baud,
                    bytes,
                })
            }
            1 => {
                let mut lengths = Vec::new();
                lengths.load(r)?;
                Ok(Self::Signal { gap_ms, lengths })
            }
            _ => Err(StateError::Invalid),
        }
    }
}

// The complete tape is in the save state: the frontend can insert another tape.
impl State for Tape {
    fn save(&self, w: &mut Writer) {
        let Self {
            description,
            blocks,
        } = self;
        description.save(w);
        blocks.len().save(w);
        for block in blocks {
            block.save(w);
        }
    }

    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        let Self {
            description,
            blocks,
        } = self;
        description.load(r)?;
        let mut count = 0_usize;
        count.load(r)?;
        if count == 0 {
            return Err(StateError::Invalid);
        }
        blocks.clear();
        for _ in 0..count {
            blocks.push(TapeBlock::load(r)?);
        }
        Ok(())
    }
}
