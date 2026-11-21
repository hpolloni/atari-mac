//! Executable files (XEX): the parser and the boot image (REQ-610 to REQ-614).
//!
//! Design: `docs/design/media.md`, "XEX parsing" and "XEX boot image". The format is the
//! Atari DOS 2 binary load format. The HRM does not describe it.

use core::ops::RangeInclusive;

use super::MediaError;
use super::disk::{DiskImage, Geometry};

/// The `$FF $FF` marker at the start of the file and before a segment header.
const MARKER: [u8; 2] = [0xFF, 0xFF];

/// `INITAD`: the init address that the loader calls after a segment (REQ-613).
const INITAD: u16 = 0x02E2;

/// `RUNAD`: the run address that the loader jumps to at the end (REQ-614).
const RUNAD: u16 = 0x02E0;

/// The loader memory: the boot sectors at `$0700-$087F` and the sector buffer at
/// `$0880-$08FF`.
const LOADER_MEMORY: RangeInclusive<u32> = 0x0700..=0x08FF;

/// The size of one sector of the boot image.
const SECTOR: usize = 128;

/// The number of boot sectors (the loader).
const LOADER_SECTORS: u16 = 3;

/// Record type: Data (`$00`, address, length - 1, data).
const RECORD_DATA: u8 = 0x00;
/// Record type: Init (`$01`): `JSR` to a `JMP ($02E2)`.
const RECORD_INIT: u8 = 0x01;
/// Record type: Run indirect (`$02`): `JMP ($02E0)`.
const RECORD_RUN_INDIRECT: u8 = 0x02;
/// Record type: Run direct (`$03`, address): `JMP address`.
const RECORD_RUN_DIRECT: u8 = 0x03;

/// The loader: 6502 code for sectors 1-3 of the boot image, loaded at `$0700`
/// (`docs/design/media.md`, "XEX boot image"). The bytes after the code are `$00`.
///
/// ```text
/// DOSVEC = $000A    DOSINI = $000C    INITAD = $02E2    RUNAD = $02E0
/// COLOR2 = $02C6    COLOR4 = $02C8    DSKINV = $E453
/// DUNIT = $0301     DCOMND = $0302    DSTATS = $0303
/// DBUFLO = $0304    DBUFHI = $0305    DAUX1 = $030A     DAUX2 = $030B
/// BUFFER = $0880
///
/// 0700  00          .BYTE $00          ; flags
/// 0701  03          .BYTE 3            ; boot sector count
/// 0702  00 07       .WORD $0700        ; load address
/// 0704  17 07       .WORD RTSA         ; init address (DOSINI)
/// 0706  A9 18       INIT   LDA #<MAIN  ; boot continuation, called by the OS
/// 0708  85 0A              STA DOSVEC
/// 070A  A9 07              LDA #>MAIN
/// 070C  85 0B              STA DOSVEC+1
/// 070E  A9 17              LDA #<RTSA
/// 0710  85 0C              STA DOSINI
/// 0712  A9 07              LDA #>RTSA
/// 0714  85 0D              STA DOSINI+1
/// 0716  18                 CLC         ; boot succeeded
/// 0717  60          RTSA   RTS
/// 0718  A9 04       MAIN   LDA #4      ; the OS jumps here through DOSVEC
/// 071A  8D DB 07           STA SECLO   ; the first stream sector is 4
/// 071D  A9 00              LDA #0
/// 071F  8D DC 07           STA SECHI
/// 0722  A9 80              LDA #$80    ; the buffer is empty
/// 0724  8D DD 07           STA BUFPOS
/// 0727  20 9A 07    NEXT   JSR GETBYTE ; the record type
/// 072A  F0 0F              BEQ DATA
/// 072C  C9 01              CMP #1
/// 072E  F0 44              BEQ INITR
/// 0730  C9 02              CMP #2
/// 0732  F0 49              BEQ RUNIND
/// 0734  C9 03              CMP #3
/// 0736  F0 48              BEQ RUNDIR
/// 0738  4C 8F 07           JMP FAIL
/// 073B  20 9A 07    DATA   JSR GETBYTE ; the address, into the STA below
/// 073E  8D 57 07           STA STORE+1
/// 0741  20 9A 07           JSR GETBYTE
/// 0744  8D 58 07           STA STORE+2
/// 0747  20 9A 07           JSR GETBYTE ; length - 1
/// 074A  8D DE 07           STA CNTLO
/// 074D  20 9A 07           JSR GETBYTE
/// 0750  8D DF 07           STA CNTHI
/// 0753  20 9A 07    LOOP   JSR GETBYTE
/// 0756  8D FF FF    STORE  STA $FFFF
/// 0759  EE 57 07           INC STORE+1
/// 075C  D0 03              BNE COUNT
/// 075E  EE 58 07           INC STORE+2
/// 0761  AD DE 07    COUNT  LDA CNTLO   ; the last byte when the count is 0
/// 0764  D0 08              BNE DECLO
/// 0766  AD DF 07           LDA CNTHI
/// 0769  F0 BC              BEQ NEXT
/// 076B  CE DF 07           DEC CNTHI
/// 076E  CE DE 07    DECLO  DEC CNTLO
/// 0771  4C 53 07           JMP LOOP
/// 0774  20 7A 07    INITR  JSR DOINIT
/// 0777  4C 27 07           JMP NEXT
/// 077A  6C E2 02    DOINIT JMP (INITAD)
/// 077D  6C E0 02    RUNIND JMP (RUNAD)
/// 0780  20 9A 07    RUNDIR JSR GETBYTE ; the address, into the JMP below
/// 0783  8D 8D 07           STA JMPD+1
/// 0786  20 9A 07           JSR GETBYTE
/// 0789  8D 8E 07           STA JMPD+2
/// 078C  4C FF FF    JMPD   JMP $FFFF
/// 078F  A9 34       FAIL   LDA #$34    ; red
/// 0791  8D C8 02           STA COLOR4
/// 0794  8D C6 02           STA COLOR2
/// 0797  4C 97 07    STOP   JMP STOP
/// 079A  AE DD 07    GETBYTE LDX BUFPOS ; the next stream byte in A
/// 079D  10 34              BPL HAVE
/// 079F  A9 01              LDA #1      ; read the next sector with DSKINV
/// 07A1  8D 01 03           STA DUNIT
/// 07A4  A9 52              LDA #$52    ; Read
/// 07A6  8D 02 03           STA DCOMND
/// 07A9  A9 80              LDA #<BUFFER
/// 07AB  8D 04 03           STA DBUFLO
/// 07AE  A9 08              LDA #>BUFFER
/// 07B0  8D 05 03           STA DBUFHI
/// 07B3  AD DB 07           LDA SECLO
/// 07B6  8D 0A 03           STA DAUX1
/// 07B9  AD DC 07           LDA SECHI
/// 07BC  8D 0B 03           STA DAUX2
/// 07BF  20 53 E4           JSR DSKINV
/// 07C2  AD 03 03           LDA DSTATS
/// 07C5  C9 01              CMP #1
/// 07C7  D0 C6              BNE FAIL
/// 07C9  EE DB 07           INC SECLO
/// 07CC  D0 03              BNE FIRST
/// 07CE  EE DC 07           INC SECHI
/// 07D1  A2 00       FIRST  LDX #0
/// 07D3  E8          HAVE   INX
/// 07D4  8E DD 07           STX BUFPOS
/// 07D7  BD 7F 08           LDA BUFFER-1,X ; sets Z and N for the caller
/// 07DA  60                 RTS
/// 07DB  00          SECLO  .BYTE 0     ; the next stream sector
/// 07DC  00          SECHI  .BYTE 0
/// 07DD  00          BUFPOS .BYTE 0     ; the next byte in the buffer, $80 = empty
/// 07DE  00          CNTLO  .BYTE 0     ; the bytes left in the Data record - 1
/// 07DF  00          CNTHI  .BYTE 0
/// ```
///
/// The loader uses no zero-page byte, so an init routine can use all of them. Its state is
/// in `$07DB-$07DF`.
const LOADER_CODE: [u8; 0xE0] = [
    0x00, 0x03, 0x00, 0x07, 0x17, 0x07, // header
    0xA9, 0x18, 0x85, 0x0A, 0xA9, 0x07, 0x85, 0x0B, // 0706 INIT
    0xA9, 0x17, 0x85, 0x0C, 0xA9, 0x07, 0x85, 0x0D, // 070E
    0x18, 0x60, // 0716 CLC, RTSA
    0xA9, 0x04, 0x8D, 0xDB, 0x07, // 0718 MAIN
    0xA9, 0x00, 0x8D, 0xDC, 0x07, // 071D
    0xA9, 0x80, 0x8D, 0xDD, 0x07, // 0722
    0x20, 0x9A, 0x07, // 0727 NEXT
    0xF0, 0x0F, 0xC9, 0x01, 0xF0, 0x44, // 072A
    0xC9, 0x02, 0xF0, 0x49, 0xC9, 0x03, 0xF0, 0x48, // 0730
    0x4C, 0x8F, 0x07, // 0738
    0x20, 0x9A, 0x07, 0x8D, 0x57, 0x07, // 073B DATA
    0x20, 0x9A, 0x07, 0x8D, 0x58, 0x07, // 0741
    0x20, 0x9A, 0x07, 0x8D, 0xDE, 0x07, // 0747
    0x20, 0x9A, 0x07, 0x8D, 0xDF, 0x07, // 074D
    0x20, 0x9A, 0x07, // 0753 LOOP
    0x8D, 0xFF, 0xFF, // 0756 STORE
    0xEE, 0x57, 0x07, 0xD0, 0x03, 0xEE, 0x58, 0x07, // 0759
    0xAD, 0xDE, 0x07, 0xD0, 0x08, // 0761 COUNT
    0xAD, 0xDF, 0x07, 0xF0, 0xBC, // 0766
    0xCE, 0xDF, 0x07, // 076B
    0xCE, 0xDE, 0x07, 0x4C, 0x53, 0x07, // 076E DECLO
    0x20, 0x7A, 0x07, 0x4C, 0x27, 0x07, // 0774 INITR
    0x6C, 0xE2, 0x02, // 077A DOINIT
    0x6C, 0xE0, 0x02, // 077D RUNIND
    0x20, 0x9A, 0x07, 0x8D, 0x8D, 0x07, // 0780 RUNDIR
    0x20, 0x9A, 0x07, 0x8D, 0x8E, 0x07, // 0786
    0x4C, 0xFF, 0xFF, // 078C JMPD
    0xA9, 0x34, 0x8D, 0xC8, 0x02, 0x8D, 0xC6, 0x02, // 078F FAIL
    0x4C, 0x97, 0x07, // 0797 STOP
    0xAE, 0xDD, 0x07, 0x10, 0x34, // 079A GETBYTE
    0xA9, 0x01, 0x8D, 0x01, 0x03, // 079F
    0xA9, 0x52, 0x8D, 0x02, 0x03, // 07A4
    0xA9, 0x80, 0x8D, 0x04, 0x03, // 07A9
    0xA9, 0x08, 0x8D, 0x05, 0x03, // 07AE
    0xAD, 0xDB, 0x07, 0x8D, 0x0A, 0x03, // 07B3
    0xAD, 0xDC, 0x07, 0x8D, 0x0B, 0x03, // 07B9
    0x20, 0x53, 0xE4, // 07BF
    0xAD, 0x03, 0x03, 0xC9, 0x01, 0xD0, 0xC6, // 07C2
    0xEE, 0xDB, 0x07, 0xD0, 0x03, 0xEE, 0xDC, 0x07, // 07C9
    0xA2, 0x00, // 07D1 FIRST
    0xE8, 0x8E, 0xDD, 0x07, 0xBD, 0x7F, 0x08, 0x60, // 07D3 HAVE
    0x00, 0x00, 0x00, 0x00, 0x00, // 07DB variables
];

/// One segment of an executable file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XexSegment {
    /// The start address.
    pub start: u16,
    /// The data, 1 to 65536 bytes.
    pub data: Vec<u8>,
}

impl XexSegment {
    /// `true` if the address range of the segment includes `addr`.
    fn covers(&self, addr: u16) -> bool {
        let start = u32::from(self.start);
        let addr = u32::from(addr);
        addr >= start && addr - start < self.len_u32()
    }

    /// The number of data bytes.
    fn len_u32(&self) -> u32 {
        u32::try_from(self.data.len()).unwrap_or(u32::MAX)
    }

    /// `true` if the address range of the segment overlaps `range`.
    fn overlaps(&self, range: &RangeInclusive<u32>) -> bool {
        let start = u32::from(self.start);
        let end = start + self.len_u32().saturating_sub(1);
        start <= *range.end() && end >= *range.start()
    }
}

/// The kind of a defective segment (REQ-611).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentDefect {
    /// Fewer than 4 bytes remain for the segment header.
    ShortHeader,
    /// The end address is less than the start address.
    EndBeforeStart,
    /// Fewer data bytes remain than the header gives.
    ShortData,
}

impl SegmentDefect {
    /// The defect in words, for messages.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::ShortHeader => "short segment header",
            Self::EndBeforeStart => "end address before start address",
            Self::ShortData => "short segment data",
        }
    }
}

/// The first defective segment of a file (REQ-611).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XexDefect {
    /// The index of the first byte of the defective segment header in the file.
    pub offset: usize,
    /// The kind of defect.
    pub kind: SegmentDefect,
}

/// A parsed executable file: the complete segments in file order (REQ-610).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xex {
    /// The complete segments, in file order. At least one.
    segments: Vec<XexSegment>,
    /// The first defect.
    defect: Option<XexDefect>,
}

/// The little-endian `u16` at `offset` of `bytes`, if both bytes are there.
fn le16(bytes: &[u8], offset: usize) -> Option<u16> {
    let pair = bytes.get(offset..offset.checked_add(2)?)?;
    <[u8; 2]>::try_from(pair).ok().map(u16::from_le_bytes)
}

impl Xex {
    /// Parses an executable file (REQ-610, REQ-611).
    ///
    /// # Errors
    ///
    /// [`MediaError::XexSignature`] if the file does not start with `$FF $FF`, and
    /// [`MediaError::XexNoSegment`] if it has no complete segment.
    pub fn parse(file: &[u8]) -> Result<Self, MediaError> {
        if !file.starts_with(&MARKER) {
            return Err(MediaError::XexSignature);
        }
        let mut segments = Vec::new();
        let mut defect = None;
        let mut p = MARKER.len();
        while p < file.len() {
            if file.get(p..p + 2) == Some(&MARKER[..]) {
                p += 2;
                continue;
            }
            let defect_at = |kind| Some(XexDefect { offset: p, kind });
            let (Some(start), Some(end)) = (le16(file, p), le16(file, p + 2)) else {
                defect = defect_at(SegmentDefect::ShortHeader);
                break;
            };
            if end < start {
                defect = defect_at(SegmentDefect::EndBeforeStart);
                break;
            }
            let data_start = p + 4;
            let data_end = data_start + usize::from(end - start) + 1;
            let Some(data) = file.get(data_start..data_end) else {
                defect = defect_at(SegmentDefect::ShortData);
                break;
            };
            segments.push(XexSegment {
                start,
                data: data.to_vec(),
            });
            p = data_end;
        }
        if segments.is_empty() {
            return Err(MediaError::XexNoSegment(defect));
        }
        Ok(Self { segments, defect })
    }

    /// The complete segments, in file order.
    #[must_use]
    pub fn segments(&self) -> &[XexSegment] {
        &self.segments
    }

    /// The first defect. The segments before it are kept (REQ-611).
    #[must_use]
    pub const fn defect(&self) -> Option<XexDefect> {
        self.defect
    }

    /// The start address of the first segment that overlaps the loader memory `$0700-$08FF`.
    #[must_use]
    pub fn loader_conflict(&self) -> Option<u16> {
        self.segments
            .iter()
            .find(|segment| segment.overlaps(&LOADER_MEMORY))
            .map(|segment| segment.start)
    }

    /// The load stream of the boot image: the records of `docs/design/media.md`, "XEX boot
    /// image". One Data record for each segment in file order, an Init record after each
    /// segment that includes `INITAD` (REQ-613), and at the end Run indirect if a segment
    /// includes `RUNAD`, else Run direct to the first segment (REQ-614).
    #[must_use]
    pub(super) fn load_stream(&self) -> Vec<u8> {
        let mut stream = Vec::new();
        let mut runad = false;
        for segment in &self.segments {
            let [count_lo, count_hi] = u16::try_from(segment.data.len().saturating_sub(1))
                .unwrap_or(u16::MAX)
                .to_le_bytes();
            let [lo, hi] = segment.start.to_le_bytes();
            stream.extend([RECORD_DATA, lo, hi, count_lo, count_hi]);
            stream.extend(&segment.data);
            if segment.covers(INITAD) || segment.covers(INITAD + 1) {
                stream.push(RECORD_INIT);
            }
            runad |= segment.covers(RUNAD) || segment.covers(RUNAD + 1);
        }
        if runad {
            stream.push(RECORD_RUN_INDIRECT);
        } else {
            let first = self.segments.first().map_or(0, |segment| segment.start);
            let [lo, hi] = first.to_le_bytes();
            stream.extend([RECORD_RUN_DIRECT, lo, hi]);
        }
        stream
    }

    /// A boot disk image that loads and runs this program (REQ-612 to REQ-614): the loader in
    /// sectors 1-3 and the load stream in sectors 4 to N, 128 bytes for each sector, the
    /// last sector padded with `$00`.
    ///
    /// A stream that needs more than 65,532 sectors (about 8 MB) is cut at that size. The
    /// loader then gets a read error at the end and shows a red screen.
    #[must_use]
    pub fn boot_image(&self) -> DiskImage {
        let stream = self.load_stream();
        let max_stream_sectors = usize::from(u16::MAX - LOADER_SECTORS);
        let stream_sectors = stream.len().div_ceil(SECTOR).min(max_stream_sectors);
        let sectors =
            LOADER_SECTORS + u16::try_from(stream_sectors).unwrap_or(u16::MAX - LOADER_SECTORS);
        let mut image = DiskImage::blank(Geometry {
            sectors,
            sector_size: 128,
        });
        let mut loader = [0_u8; SECTOR * 3];
        for (slot, &byte) in loader.iter_mut().zip(&LOADER_CODE) {
            *slot = byte;
        }
        for (n, chunk) in (1..).zip(loader.chunks(SECTOR)) {
            let _written = image.write_sector(n, chunk);
        }
        for (n, chunk) in (LOADER_SECTORS + 1..=sectors).zip(stream.chunks(SECTOR)) {
            let mut sector = [0_u8; SECTOR];
            for (slot, &byte) in sector.iter_mut().zip(chunk) {
                *slot = byte;
            }
            let _written = image.write_sector(n, &sector);
        }
        image.clear_changed();
        image
    }
}
