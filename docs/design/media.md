# Media design

## Scope

This document tells how the emulator satisfies the media requirements: cartridges, executable
files (XEX), the SIO bus, the disk drive D1: and ATR disk images. It covers REQ-600 to REQ-638.

| Topic | Requirements |
| --- | --- |
| Cartridges | REQ-600 to REQ-609 |
| Executable files | REQ-610 to REQ-614 |
| SIO bus protocol | REQ-615 to REQ-623 |
| Disk drive and ATR images | REQ-624 to REQ-638 |

The POKEY serial port (REQ-500 range), the PIA CB2 output and the memory map (REQ-200 range)
are not in this document. This document gives the calls that those modules make to the media
module.

All code is in `crates/atari-core/src/media.rs`. If the file becomes too large, it can become
the directory `media/` with the submodules `cart`, `xex`, `atr`, `sio` and `disk`. The public
names stay the same. The core has no file I/O (REQ-015): all functions take and give bytes.

## Interfaces

### Errors

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
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
    CarSize { car_type: u32, expected: usize, actual: usize },
    /// The CAR checksum does not agree with the data. `stored` is header bytes 8-11,
    /// `computed` is the sum of the data bytes.
    CarChecksum { stored: u32, computed: u32 },
    /// An XEX file does not start with `$FF $FF`.
    XexSignature,
    /// An XEX file has no complete segment. The defect, if any, is the first one.
    XexNoSegment(Option<XexDefect>),
    /// An ATR file is shorter than its 16-byte header.
    AtrHeader,
    /// An ATR file does not start with `$96 $02`. The value is the word that was found.
    AtrSignature(u16),
    /// An ATR sector size is not 128 or 256.
    AtrSectorSize(u16),
    /// The ATR data size is not a whole number of sectors.
    AtrDataSize(usize),
    /// The ATR file has fewer data bytes than the header gives.
    AtrTruncated { expected: usize, actual: usize },
    /// The ATR sector count is 0 or more than 65535.
    AtrSectorCount(usize),
}
```

`MediaError` implements `Display` and `core::error::Error`. No function in this module panics
for any input (REQ-016).

### Cartridge

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartKind { Standard8k, Standard16k, MaxFlash1Mbit, MaxFlash8Mbit }

impl CartKind {
    /// The kind for a CAR type number: 1, 2, 41, 42. `None` for other numbers.
    pub const fn from_car_type(car_type: u32) -> Option<Self>;
    /// The ROM size in bytes: 8192, 16384, 131072, 1048576.
    pub const fn rom_size(self) -> usize;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartridge { /* see State */ }

impl Cartridge {
    pub fn from_raw(data: Vec<u8>) -> Result<Self, MediaError>;
    pub fn from_car(file: &[u8]) -> Result<Self, MediaError>;
    pub const fn kind(&self) -> CartKind;
    /// The byte at `addr` in `$8000-$BFFF`, or `None` if the cartridge does not map `addr`.
    pub fn read(&self, addr: u16) -> Option<u8>;
    /// One bus access to `$D500-$D5FF`. `value` is `Some` for a write, `None` for a read.
    pub fn access_control(&mut self, addr: u16, value: Option<u8>);
    /// `true` while the cartridge maps `$8000-$9FFF`.
    pub const fn rd4(&self) -> bool;
    /// `true` while the cartridge maps `$A000-$BFFF`.
    pub const fn rd5(&self) -> bool;
    /// Power-on state: bank 0, enabled (REQ-607).
    pub fn cold_reset(&mut self);
}
```

Call contract:

1. `read` has no side effects. The machine calls it for each CPU read and each ANTIC DMA read
   in `$8000-$BFFF`, and to find if a write goes to ROM (REQ-602). The machine shows RAM or
   BASIC when `read` gives `None` (REQ-601).
2. The machine calls `access_control` once for each bus access to `$D500-$D5FF`. This includes
   CPU dummy reads (the CPU already makes all its bus accesses through `Bus::read`) and ANTIC
   DMA reads (REQ-609).
3. After each `access_control` call, at insertion and at each cold reset, the machine sets the
   GTIA trigger input 3 from `rd5()`: `gtia.set_trigger(3, !cart.rd5())` (REQ-603). With no
   cartridge, the trigger is "pressed" (`TRIG3` = 0).
4. A read of `$D5xx` gives `$FF` to the CPU. No supported cartridge drives the data bus in this
   range.
5. The RESET key does not call any cartridge function (REQ-607).

### Executable files

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XexSegment { pub start: u16, pub data: Vec<u8> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentDefect { ShortHeader, EndBeforeStart, ShortData }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XexDefect { pub offset: usize, pub kind: SegmentDefect }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xex { /* segments, defect */ }

impl Xex {
    pub fn parse(file: &[u8]) -> Result<Self, MediaError>;
    pub fn segments(&self) -> &[XexSegment];
    /// The first defect. The segments before it are kept (REQ-611).
    pub const fn defect(&self) -> Option<XexDefect>;
    /// The first address of a segment that overlaps the loader memory `$0700-$08FF`.
    pub fn loader_conflict(&self) -> Option<u16>;
    /// A boot disk image that loads and runs this program (REQ-612 to REQ-614).
    pub fn boot_image(&self) -> DiskImage;
}
```

The frontend shows `defect()` and `loader_conflict()` to the user as warnings.

### Disk images

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry { pub sectors: u16, pub sector_size: u16 }

impl Geometry {
    pub const SD: Self = Self { sectors: 720, sector_size: 128 };
    pub const ED: Self = Self { sectors: 1040, sector_size: 128 };
    pub const DD: Self = Self { sectors: 720, sector_size: 256 };
    /// The data length of sector `n`: 128 for sectors 1-3, else `sector_size`.
    /// `None` if `n` is 0 or above `sectors`.
    pub const fn sector_len(self, n: u16) -> Option<usize>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskImage { /* see State */ }

impl DiskImage {
    pub fn from_atr(file: &[u8]) -> Result<Self, MediaError>;
    /// An image of `geometry` with all bytes `$00`.
    pub fn blank(geometry: Geometry) -> Self;
    /// The ATR file bytes (REQ-637).
    pub fn to_atr(&self) -> Vec<u8>;
    pub const fn geometry(&self) -> Geometry;
    pub fn sector(&self, n: u16) -> Option<&[u8]>;
    /// Replaces sector `n`. Gives `false` and changes nothing if `n` or the length is wrong.
    pub fn write_sector(&mut self, n: u16, data: &[u8]) -> bool;
    /// Replaces the image with a blank image of `geometry` (REQ-632).
    pub fn format(&mut self, geometry: Geometry);
    pub const fn write_protected(&self) -> bool;
    pub const fn set_write_protected(&mut self, on: bool);
    /// `true` after a successful write or format since `from_atr` or `blank`.
    pub const fn changed(&self) -> bool;
}
```

### SIO bus and disk drive

```rust
/// One serial byte on the bus: 1 start bit, 8 data bits LSB first, 1 stop bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerialByte {
    pub value: u8,
    /// The machine cycle where the start bit begins.
    pub start: u64,
    /// The length of one bit in machine cycles.
    pub bit_cycles: u32,
}

impl SerialByte {
    /// The machine cycle where the stop bit ends: `start + 10 * bit_cycles`.
    pub const fn end(&self) -> u64;
}

#[derive(Debug, Clone)]
pub struct SioBus { /* see State */ }

impl SioBus {
    pub fn new(standard: VideoStandard) -> Self;
    pub const fn drive(&self) -> &DiskDrive;
    pub const fn drive_mut(&mut self) -> &mut DiskDrive;
    /// The PIA CB2 output changed. `low` = command line asserted.
    pub fn set_command_line(&mut self, low: bool, now: u64);
    /// POKEY finished sending `byte`. `now` is `byte.end()`.
    pub fn host_byte(&mut self, byte: SerialByte, now: u64);
    /// The next byte that a device sends, if one is scheduled.
    pub fn peek_device_byte(&self) -> Option<SerialByte>;
    /// Removes and gives the next device byte if its start cycle is at or before `now`.
    pub fn take_device_byte(&mut self, now: u64) -> Option<SerialByte>;
    /// A complete transaction at once, for fast SIO (REQ-623).
    pub fn transact(&mut self, request: &SioRequest<'_>) -> SioOutcome;
    /// Bus state to idle (cold reset). The inserted image does not change.
    pub fn reset(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandFrame { pub device: u8, pub command: u8, pub aux1: u8, pub aux2: u8 }

pub enum SioData<'a> { None, Receive(usize), Send(&'a [u8]) }

pub struct SioRequest<'a> { pub frame: CommandFrame, pub data: SioData<'a> }

pub enum SioOutcome {
    /// No emulated device has this ID. The OS routine must run.
    NoDevice,
    /// The OS status byte and, for a receive, the bytes for the buffer.
    Done { status: u8, data: Vec<u8> },
}

#[derive(Debug, Clone, Default)]
pub struct DiskDrive { /* see State */ }

impl DiskDrive {
    pub fn insert(&mut self, image: DiskImage);
    pub fn eject(&mut self) -> Option<DiskImage>;
    pub const fn image(&self) -> Option<&DiskImage>;
    pub const fn image_mut(&mut self) -> Option<&mut DiskImage>;
    /// Command acceptance (ACK or NAK) and the data frame direction.
    pub fn accept(&mut self, frame: &CommandFrame) -> Accept;
    /// Command execution after the ACK (and the data frame of a write-type command).
    pub fn execute(&mut self, frame: &CommandFrame, data: Option<&[u8]>) -> Completion;
    /// A data frame from the computer had a checksum error (REQ-620).
    pub fn data_frame_error(&mut self);
}

pub enum Accept { Nak, Read, Write { len: usize } }

pub struct Completion {
    pub ok: bool,
    /// The data frame of a read-type command.
    pub data: Option<Vec<u8>>,
    /// The time of the operation in µs, before the Complete or Error byte.
    pub busy_us: u32,
}

/// The carry wrap-around checksum (REQ-616).
pub fn sio_checksum(bytes: &[u8]) -> u8;
```

`DiskDrive` has the command semantics. `SioBus` has the byte timing. The real serial path and
the fast path both call `accept` and `execute`, so both give the same result.

Call contract for the machine:

1. The machine calls `set_command_line` in the machine cycle where the PIA CB2 output changes.
   The PIA module must give the CB2 output level.
2. POKEY calls `host_byte` in the cycle where the stop bit of a byte from `SEROUT` ends.
3. In each machine cycle, the machine compares `peek_device_byte().start` with the cycle
   counter. At that cycle, it calls `take_device_byte` and gives the byte to POKEY. POKEY
   shifts the bits in with `bit_cycles` and sets `SERIN` and the serial input interrupt. This
   is in the POKEY area.
4. `SioBus` never reads the cycle counter by itself. All times come from the `now` and `start`
   arguments, so the core stays deterministic (REQ-014).

### Fast SIO

The machine owns the fast SIO hook, because it needs the CPU registers and memory.

```rust
impl Machine {
    pub fn set_fast_sio(&mut self, on: bool);
}
```

At each instruction boundary (before the opcode fetch cycle), the machine checks these
conditions:

1. Fast SIO is on.
2. `regs.pc == 0xE459` (SIOV).
3. `PORTB` bit 0 = 1 (OS ROM enabled).

If all are true, the machine reads the device control block at `$0300-$030B` with no side
effects. The bus ID is `DDEVIC + DUNIT - 1`, with 8-bit wrap. A `DDEVIC` of `$5F` is never
intercepted. The data direction comes from `DSTATS`: bit 7 = send, bit 6 = receive, neither =
no data. The machine then calls `SioBus::transact`.

If the result is `NoDevice`, the machine does nothing and the CPU executes the OS code. Else
the machine:

1. writes the received bytes to `DBUF` with the normal write rules (ROM is not changed),
2. writes `status` to `DSTATS` and to Y,
3. sets the N flag from bit 7 of `status`, and clears the Z flag,
4. pulls the return address from the stack (`S += 2`) and sets PC to that address + 1.

`transact` maps the drive results to OS status bytes:

| Drive result | Status |
| --- | --- |
| NAK for the command or the data frame | `$8B` |
| Complete | `$01` |
| Error | `$90` |
| Receive: the frame is longer than `DBYT` | `$8F` (the first `DBYT` bytes go to the buffer) |
| Receive: the frame is shorter than `DBYT` | `$8A` |
| Send: `DBYT` is shorter than the frame | `$8A` |
| Send: `DBYT` is longer than the frame | `$8B` |

The fast path takes no emulated time. The POKEY registers do not change.

### Machine configuration

`MachineConfig` gets these fields:

```rust
pub cartridge: Option<Cartridge>,
pub disk: Option<DiskImage>,
pub fast_sio: bool,
```

`Machine` gets these functions:

```rust
pub fn insert_disk(&mut self, image: DiskImage);
pub fn eject_disk(&mut self) -> Option<DiskImage>;
pub fn disk(&self) -> Option<&DiskImage>;
/// Puts the boot image of `xex` in D1:, does a cold reset and holds OPTION (REQ-612).
pub fn start_xex(&mut self, xex: &Xex);
```

## State

### Cartridge

| Field | Type | Size | Power-on value |
| --- | --- | --- | --- |
| `kind` | `CartKind` | 1 byte | From the file |
| `rom` | `Vec<u8>` | 8 KB to 1 MB | From the file |
| `bank` | `u8` | 1 byte | 0 |
| `enabled` | `bool` | 1 byte | `true` |

`bank` and `enabled` are used only by the MaxFlash kinds.

### Xex

| Field | Type | Contents |
| --- | --- | --- |
| `segments` | `Vec<XexSegment>` | The complete segments, in file order. At least one. |
| `defect` | `Option<XexDefect>` | The first defect. |

### DiskImage

| Field | Type | Contents |
| --- | --- | --- |
| `geometry` | `Geometry` | Sector count and sector size |
| `data` | `Vec<u8>` | Sector data in the ATR layout (sectors 1-3 of 128 bytes) |
| `header_extra` | `[u8; 9]` | ATR header bytes 7-15, kept on export |
| `full_boot_sectors` | `bool` | The input stored sectors 1-3 as 256 bytes (see Behavior) |
| `write_protected` | `bool` | Set by the frontend. Power-on: `false`. |
| `changed` | `bool` | `false` after load |

### DiskDrive

| Field | Type | Power-on value | Contents |
| --- | --- | --- | --- |
| `image` | `Option<DiskImage>` | From the configuration | The inserted image |
| `error_bits` | `u8` | 0 | Status byte 0, bits 0-3 (REQ-627) |
| `format_geometry` | `Option<Geometry>` | `None` | From Write Percom block (REQ-635). Cleared by `insert`. |
| `head_track` | `u16` | 0 | For REQ-638 |

### SioBus

| Field | Type | Power-on value | Contents |
| --- | --- | --- | --- |
| `drive` | `DiskDrive` | See above | Drive D1: |
| `timing` | `SioTiming` | From the video standard | The cycle counts of the Timing section |
| `command_low` | `bool` | `false` | Command line level |
| `phase` | `BusPhase` | `Idle` | Protocol state |
| `frame` | `[u8; 5]`, `u8` | Empty | Command frame bytes and count |
| `data_in` | `Vec<u8>` | Empty | Data frame from the computer |
| `out` | `VecDeque<SerialByte>` | Empty | Scheduled device bytes |

```rust
enum BusPhase {
    Idle,
    /// The command line is low. Bytes go into `frame`.
    Command { bad: bool },
    /// ACK sent for a write-type command. Waiting for `len + 1` bytes.
    DataIn { frame: CommandFrame, len: usize },
    /// Bytes are scheduled in `out`. New host bytes are ignored.
    Reply,
}
```

All state types derive `Clone`, for save states (REQ-018).

## Behavior

### Cartridge mapping (HRM 8.1, HRM 2.6, HRM 2.8)

`read(addr)`:

| Kind | `$8000-$9FFF` | `$A000-$BFFF` |
| --- | --- | --- |
| `Standard8k` | `None` | `rom[addr - $A000]` |
| `Standard16k` | `rom[addr - $8000]` | `rom[addr - $8000]` |
| `MaxFlash1Mbit`, `MaxFlash8Mbit` | `None` | `rom[bank * 8192 + addr - $A000]` if `enabled`, else `None` |

`rd4()` is `true` only for `Standard16k`. `rd5()` is `true` for the standard kinds, and equal to
`enabled` for the MaxFlash kinds.

`access_control(addr, _)` (HRM 8.2), with `lo = addr & $FF`:

| Kind | Condition | Result |
| --- | --- | --- |
| `MaxFlash1Mbit` | `lo < $20` | `bank = lo & $0F`, `enabled = lo & $10 == 0` |
| `MaxFlash8Mbit` | any `lo` | `bank = lo & $7F`, `enabled = lo & $80 == 0` |
| Other | any | No change |

Reads and writes have the same effect. The data of a write is ignored.

### Raw and CAR loading

`from_raw`: 8192 bytes give `Standard8k`, 16384 bytes give `Standard16k`, other sizes give
`BadSize` (REQ-604).

`from_car` checks, in this order (REQ-605, REQ-606):

1. Length at least 16, else `CarHeader`.
2. Bytes 0-3 are `CART`, else `CarSignature`.
3. `CartKind::from_car_type(be32(4))` is `Some`, else `CarType`.
4. `len - 16 == kind.rom_size()`, else `CarSize`.
5. The wrapping `u32` sum of the data equals `be32(8)`, else `CarChecksum`.

### XEX parsing (REQ-610, REQ-611)

The parser keeps an offset `p`:

1. If the length is less than 2 or bytes 0-1 are not `$FF $FF`, give `XexSignature`. Set `p = 2`.
2. Loop while `p < len`:
   1. If bytes `p` and `p+1` are `$FF $FF`, add 2 to `p` and continue.
   2. If fewer than 4 bytes remain, record `ShortHeader` at `p` and stop.
   3. Read `start` and `end` (low byte first). If `end < start`, record `EndBeforeStart` at `p`
      and stop.
   4. If fewer than `end - start + 1` bytes remain after the header, record `ShortData` at `p`
      and stop.
   5. Add the segment. Set `p` to the byte after the data.
3. If there are no segments, give `XexNoSegment(defect)`.

A `$FF $FF` that is a segment start address cannot occur, because rule 2.1 always takes it as
a marker. A segment can have up to 65536 bytes (`$0000-$FFFF`).

### XEX boot image (REQ-612 to REQ-614)

`boot_image` makes a disk image with 128-byte sectors:

| Sectors | Contents |
| --- | --- |
| 1-3 | The loader: 384 bytes of 6502 code |
| 4 to N | The load stream, 128 bytes for each sector, the last sector padded with `$00` |

The geometry is `sectors = 3 + ceil(stream_len / 128)`. The loader code is a constant byte
array in `media.rs`, with its assembler listing in a comment. The core has no assembler
dependency (REQ-015). The stream length goes into the loader at a fixed offset.

The Rust side converts the segments into a load stream of records. Thus the 6502 code stays
small, and the INITAD and RUNAD rules are in Rust, where unit tests can check them.

| Record | Bytes | Loader action |
| --- | --- | --- |
| Data | `$00`, address (2), length - 1 (2), data | Copy the data to memory |
| Init | `$01` | `JSR` to a `JMP ($02E2)` |
| Run indirect | `$02` | `JMP ($02E0)` |
| Run direct | `$03`, address (2) | `JMP address` |

Record rules:

1. One Data record for each segment, in file order.
2. An Init record after each segment whose range includes `$02E2` or `$02E3` (REQ-613).
3. At the end, Run indirect if a segment range includes `$02E0` or `$02E1`, else Run direct with
   the start address of the first segment (REQ-614).

Boot sequence:

1. `Machine::start_xex` inserts the image in D1:, does a cold reset, and holds the OPTION key.
   The XL/XE OS disables BASIC when OPTION is held at a cold start. The machine releases OPTION
   when D1: gets its first command (either path), or after 3 seconds of emulated time.
2. The OS boot reads sector 1. The header has 3 boot sectors and the load address `$0700`. The
   OS reads sectors 2 and 3 and calls the loader init entry at `$0706`.
3. The init entry sets `DOSVEC` (`$000A`) to the loader main entry, sets `DOSINI` to an `RTS`,
   clears the carry and returns. The OS completes its initialization and jumps through `DOSVEC`.
4. The main entry reads sectors 4 to N with `DSKINV` (`$E453`) into the buffer at
   `$0880-$08FF`, and does the records.
5. If `DSKINV` gives an error, the loader sets the background color to red and stops.

Loader memory: `$0700-$08FF`. `Xex::loader_conflict` reports a segment in this range.

### ATR loading (REQ-625, REQ-626)

`from_atr` checks, in this order:

1. Length at least 16, else `AtrHeader`.
2. Word 0 is `$0296`, else `AtrSignature`.
3. Sector size (word 4) is 128 or 256, else `AtrSectorSize`.
4. `size = (byte2 | byte3 << 8 | byte6 << 16) * 16`. If `len - 16 < size`, give `AtrTruncated`.
5. Sector count:
   - Sector size 128: `size % 128 == 0`, `n = size / 128`.
   - Sector size 256, normal layout: `size >= 384` and `(size - 384) % 256 == 0`, `n = 3 + (size - 384) / 256`.
   - Sector size 256, full boot sectors: `size % 256 == 0`, `n = size / 256`. The image keeps the
     first 128 bytes of sectors 1-3. This rule is "to verify" (see REQ-625).
   - Else `AtrDataSize`.
6. `n` from 1 to 65535, else `AtrSectorCount`.

The two layouts for 256-byte sectors cannot both match the same size, because 384 is not a
multiple of 256.

`to_atr` writes the header from `geometry` and `header_extra`, then the data. For an image with
`full_boot_sectors`, it writes sectors 1-3 as 256 bytes, with the second half `$00`. A `format`
clears `full_boot_sectors`.

### Command frame reception (REQ-615, REQ-617)

| Event | Phase | Action |
| --- | --- | --- |
| Command line goes low | any | Clear `frame` and `out`. Phase `Command { bad: false }`. |
| Host byte | `Command` | If the bit period is outside 90-98 cycles, set `bad`. Else append the byte if the count is below 5, else set `bad`. |
| Command line goes high | `Command` | If `bad`, the count is not 5, the checksum is wrong, or the ID is not `$31`: phase `Idle`. Else call `drive.accept` and schedule the reply. |
| Host byte | `DataIn` | Append to `data_in`. At `len + 1` bytes, check the checksum and schedule the rest of the reply. |
| Host byte | `Idle`, `Reply` | Ignore. |

Bit period 90-98 cycles is 94 ± 5 percent, rounded inward. A command line edge to low in any
phase stops the current transaction. This is the resynchronization of HRM 9.1. The drive has
no receive timeout: in `DataIn` it waits until the next command line edge.

### Reply scheduling (REQ-618 to REQ-622)

All delays are in the Timing section. `t` is the cycle where the command line goes high.

Read-type command (`Accept::Read`):

1. ACK at `t + ACK_DELAY`.
2. `execute`. Complete or Error at `ack.end() + max(COMPLETE_MIN, busy)`.
3. Data bytes at `ce.start + k * BYTE_PERIOD` for k = 1 to `len`, then the checksum.

Write-type command (`Accept::Write { len }`):

1. ACK at `t + ACK_DELAY`. Phase `DataIn`.
2. When the last byte arrives at cycle `e`: if the checksum is wrong, call `data_frame_error`
   and send NAK at `e + DATA_ACK_DELAY`. Else send ACK at `e + DATA_ACK_DELAY`, call `execute`
   with the data, and send Complete or Error at `ack.end() + max(COMPLETE_MIN, busy)`.

NAK (`Accept::Nak`): NAK at `t + ACK_DELAY`, then no more bytes.

Each scheduled byte has `bit_cycles = BIT_PERIOD`. The phase becomes `Idle` after the last
byte ends.

### Drive commands (REQ-627 to REQ-635)

`accept(frame)`: `sector = aux1 | aux2 << 8`. With no image, the sector range is 1-65535.

| Command | Accept |
| --- | --- |
| `$53` Status | `Read` |
| `$52` Read | `Read` if the sector is in range, else `Nak` |
| `$50` Put, `$57` Write | `Write { len: sector_len(sector) }` if in range, else `Nak`. `len` is 128 with no image. |
| `$21` Format, `$22` Format medium, `$4E` Read Percom block | `Read` |
| `$4F` Write Percom block | `Write { len: 12 }` |
| Other | `Nak` |

A `Nak` sets `error_bits` bit 0.

`execute(frame, data)`:

| Command | No image | Write-protected | Normal |
| --- | --- | --- | --- |
| Status | Frame of REQ-627. Then clear `error_bits`. | Same | Same |
| Read | Error, 128 bytes of `$00` | Read as normal | Complete, sector data |
| Put, Write | Error | Error, set `error_bits` bit 3 | `write_sector`, Complete |
| Format | Error, 128 bytes of `$FF` | Error, set bit 3, sector-size bytes of `$FF` | `format`, Complete, `$FF` bytes of the new sector size |
| Read Percom | Complete, SD block | Block of the image | Block of the image (REQ-634) |
| Write Percom | Complete | Same as normal | SD, ED or DD block: keep it, Complete. Else Error. |

A successful command other than Status clears `error_bits`. `data_frame_error` sets bit 1.

The Format geometry: for `$22`, ED. For `$21`, `format_geometry` if it is DD, else SD.

### Seek and verify timing (REQ-638)

`busy_us` for Read, Put and Write is `OP_BASE_US`, plus `40_000 + 20_000 * (moves - 1)` if the
head moves `moves > 0` tracks, plus `208_000` for Write. The track is `(sector - 1) / 18`, or
`(sector - 1) / 26` for ED. `head_track` then becomes the new track. Format sets `head_track`
to 0. This model is enabled when REQ-638 is implemented; before that, `busy_us` is
`OP_BASE_US`.

## Timing

`SioTiming::new(standard)` converts µs to machine cycles with `round(us * clock / 1e6)`, with
the clock from `VideoStandard::cpu_clock_hz`. `COMPLETE_MIN` rounds up. The 16 ms limit of REQ-618 rounds down (28,636 NTSC, 28,375 PAL cycles).

| Constant | Value | Source | NTSC cycles | PAL cycles |
| --- | --- | --- | --- | --- |
| Host bit period, nominal | 94 cycles | HRM 10.3 (divisor 40) | 94 | 94 |
| Host bit period, accepted | 90-98 cycles | HRM 10.5 (5 percent) | 90-98 | 90-98 |
| `ACK_DELAY` | 1000 µs | Project decision, in the 0-16 ms window of HRM 9.1 | 1790 | 1773 |
| `DATA_ACK_DELAY` | 1000 µs | Project decision, in the 850 µs-16 ms window of HRM 9.1 | 1790 | 1773 |
| `COMPLETE_MIN` | 250 µs | HRM 9.1 step 5 | 448 | 444 |
| `BIT_PERIOD` | 51 µs | HRM 10.15 | 91 | 90 |
| `BYTE_PERIOD` | 549 µs | HRM 10.15 | 983 | 974 |
| `OP_BASE_US` | 1000 µs | Project decision | 1790 | 1773 |
| Format time | 1 s | Project decision | 1,789,773 | 1,773,448 |

The clocks are 1,789,772.5 Hz (NTSC) and 1,773,447.5 Hz (PAL) from HRM 10.3. `timing.rs` has
1,773,447.0 Hz for PAL. The difference does not change any value in this table.

Example: a Read of one SD sector at NTSC takes about 2.6 ms for the command frame, 1 ms ACK
delay, 0.5 ms ACK, 1 ms operation, and 129 * 549 µs = 70.8 ms for the Complete byte and the
data. The total is about 76 ms, near the 68 ms data time of HRM Figure 18.

## Test plan

Unit tests are in `crates/atari-core/src/media.rs`, module `tests`. System tests are in
`crates/harness/tests/media.rs`. Each test names its requirement ID in its documentation
comment (REQ-027). The tests make all images in code, so no test data is downloaded, except
the AltirraOS and BASIC ROM files of `scripts/fetch-roms.sh`.

### Test images

The test helpers make these images. The requirements give the same definitions.

| Name | Contents | Used by |
| --- | --- | --- |
| Offset pattern cartridge | 8 KB or 16 KB. Byte `i` holds `(i >> 8) & $FF`. | REQ-600, REQ-604, REQ-605, REQ-606 |
| Bank pattern cartridge | MaxFlash 1Mbit or 8Mbit. Each byte of bank `k` holds `k`. | REQ-607 to REQ-609 |
| Fill cartridge | All bytes `$CC` (8 KB, 16 KB) or `$DD` (MaxFlash 1Mbit). BASIC ROM: 8192 bytes of `$BB`. | REQ-601, REQ-602 |
| CAR file | 16-byte header (`CART`, type, checksum, `00 00 00 00`) and the offset pattern data. | REQ-605, REQ-606 |
| SD test image | 720 x 128. Each byte of sector `n` holds `n & $FF`. | REQ-615 to REQ-624, REQ-631 |
| ATR pattern files | SD, ED and DD files of REQ-625. Data byte `i` holds `i & $FF`. | REQ-625 to REQ-638 |
| Boot image | SD or DD image. Sector 1 starts with the boot header `00 nn 00 07 00 07` (flags, boot sector count `nn`, load address `$0700`, init address `$0700`), then the test code at offset 6. To verify: the header layout. The HRM does not give it. | REQ-623, REQ-636 |

### Unit test method for the SIO bus

The SIO unit tests drive `SioBus` directly. The helper `send_frame(bus, bytes, t, bit_cycles)`
calls `set_command_line(true, t)`, then `host_byte` for each byte with no gap, then
`set_command_line(false, ...)` after a delay that the test gives (default 1,600 cycles). The
helper `device_bytes(bus, until)` calls `take_device_byte` until the cycle `until` and gives
each `SerialByte` with its `start` and `bit_cycles`. A write-type test sends its data frame
with `host_byte` after the first ACK. The timing tests compare `start` and `end()` values. A
timing test runs for NTSC and for PAL when its requirement names both.

### System test method for cartridges

The cartridge system tests need CPU accesses. They use the public `Machine` API (system
design) with a synthetic OS ROM that the test builds: 16,384 bytes, the test code at `$C000`
(ROM offset 0), the reset vector `$FFFC-$FFFD` = `$C000`, and the NMI and IRQ vectors to an
`RTI`. The test code ends with a `JMP` to itself. The test calls `Machine::new` with the
cartridge in `MachineConfig`, runs `run_cycle` for a fixed number of cycles (10,000 unless the
test gives another value), and then checks `peek` and `peek_ram`. To set `PORTB`, the test
code writes `$FF` to `$D301` (DDRB, selected after reset), `$04` to `$D303` (PBCTL) and then
the value to `$D301`. A `PORTB` value always keeps bit 0 = 1, because the code runs from the
OS ROM.

### System test method for disks and executables

The disk and XEX system tests use AltirraOS (`os_rom()`) and, if the requirement names it,
the BASIC ROM (`basic_rom()`). They call `Machine::new` with `disk` and `fast_sio` in
`MachineConfig`, or `Machine::start_xex`. They wait with `run_until` for a marker byte that
the test code writes (usually `$A5` at `$0600`), with the frame limit of the requirement.

### Tests

| Requirement | Test |
| --- | --- |
| REQ-600 | `crates/harness/tests/media.rs` `req600_std8k_maps_left_window` (criteria 1, 2), `req600_std16k_maps_both_windows` (3, 4) |
| REQ-601 | `crates/harness/tests/media.rs` `req601_cart_over_basic_over_ram` (1-3), `req601_disabled_cart_shows_basic_or_ram` (4, 5) |
| REQ-602 | `crates/harness/tests/media.rs` `req602_write_to_cart_rom_ignored` (1, 2), `req602_maxflash_disabled_write_goes_to_ram` (3) |
| REQ-603 | `crates/harness/tests/media.rs` `req603_trig3_follows_rd5` (1-4), `req603_trig3_follows_bank_switch` (5) |
| REQ-604 | `media.rs` `req604_raw_sizes` (1, 2), `req604_raw_bad_sizes` (3) |
| REQ-605 | `media.rs` `req605_car_type1` (1), `req605_car_type2` (2), `req605_car_unused_bytes_ignored` (3). The reads use `Cartridge::read`. |
| REQ-606 | `media.rs` `req606_car_errors` (1-5), `req606_car_error_order` (6) |
| REQ-607 | `crates/harness/tests/media.rs` `req607_bank_kept_on_reset_key` (1, 2), `req607_cold_reset_bank0` (3, 4). The test calls `press_reset` or `power_on` and then `peek` with no `run_cycle` between. |
| REQ-608 | `media.rs` `req608_maxflash1_banks` (1-5, 8), `req608_maxflash8_banks` (6-8); `crates/harness/tests/media.rs` `req608_flash_write_ignored` (9) |
| REQ-609 | `crates/harness/tests/media.rs` `req609_dummy_read_switches_bank` (1), `req609_antic_dma_switches_bank` (2: the test code writes the display list to `$0600`, `DLISTL`/`DLISTH` and `DMACTL`, and the test runs 2 frames) |
| REQ-610 | `media.rs` `req610_parse_segments` (1-6) |
| REQ-611 | `media.rs` `req611_xex_signature_errors` (1-3), `req611_xex_no_segment` (4-6), `req611_xex_defects_keep_segments` (7-9) |
| REQ-612 | `crates/harness/tests/media.rs` `req612_xex_loads_segments` (1-3), `req612_xex_file_order` (4), each with fast SIO off and on (5); `media.rs` `req612_boot_image_records` (the records and the geometry of `boot_image`) |
| REQ-613 | `crates/harness/tests/media.rs` `req613_initad_called_per_segment` (1, 2); `media.rs` `req613_init_records` (an Init record after segments C and E only) |
| REQ-614 | `crates/harness/tests/media.rs` `req614_runad` (1, 3), `req614_run_first_segment` (2, 3); `media.rs` `req614_run_records` (Run indirect or Run direct `$2000`) |
| REQ-615 | `media.rs` `req615_status_frame_reply` (1), `req615_frame_needs_command_line` (2), `req615_frame_restart` (3), `req615_baud_tolerance` (4, 5) |
| REQ-616 | `media.rs` `req616_checksum_values` (1-6) |
| REQ-617 | `media.rs` `req617_ignored_frames` (1-5) |
| REQ-618 | `media.rs` `req618_ack_after_deassert` (1), `req618_ack_within_16ms` (2), `req618_nak_timing` (3), each for NTSC and PAL (4) |
| REQ-619 | `media.rs` `req619_read_sequence` (1-3, 5, 6), `req619_write_sequence` (4, 7) |
| REQ-620 | `media.rs` `req620_data_checksum_nak` (1-3) |
| REQ-621 | `media.rs` `req621_complete_delay` (1, 2), for NTSC and PAL (3) |
| REQ-622 | `media.rs` `req622_byte_timing_ntsc`, `req622_byte_timing_pal` (1-4) |
| REQ-623 | `crates/harness/tests/media.rs` `req623_fast_sio_read` (1, 2), `req623_fast_sio_put` (3), `req623_fast_sio_not_intercepted` (4-6), `req623_fast_sio_xl_os` (7), `req623_fast_sio_boot_faster` (8). The test code is in a boot image (see Test images). |
| REQ-624 | `media.rs` `req624_only_device_31` (1, 2) |
| REQ-625 | `media.rs` `req625_atr_sd_ed_dd` (1-5), `req625_atr_extra_bytes` (6), `req625_atr_large` (7), `req625_atr_dd_boot_sectors` (the full boot sector rule of ATR loading) |
| REQ-626 | `media.rs` `req626_atr_errors` (1-8), `req626_atr_error_order` (9) |
| REQ-627 | `media.rs` `req627_status_frames` (1-6, 9), `req627_status_error_bits` (7, 8) |
| REQ-628 | `media.rs` `req628_nak_commands` (1), `req628_nak_sector_range` (2-6) |
| REQ-629 | `media.rs` `req629_read_sd_ed` (1-3, 6), `req629_read_dd` (4-6) |
| REQ-630 | `media.rs` `req630_put_write_roundtrip` (1, 2, 4, 5), `req630_put_dd_frame_sizes` (3) |
| REQ-631 | `media.rs` `req631_write_protect` (1-6) |
| REQ-632 | `media.rs` `req632_format_sd` (3), `req632_format_ed` (1, 2, 5), `req632_format_dd` (4) |
| REQ-633 | `media.rs` `req633_empty_drive` (1-7) |
| REQ-634 | `media.rs` `req634_percom_blocks` (1-4) |
| REQ-635 | `media.rs` `req635_write_percom` (1-6) |
| REQ-636 | `crates/harness/tests/media.rs` `req636_boot_sd_ntsc` (1), `req636_boot_sd_pal` (2), `req636_boot_fast_sio` (3), `req636_boot_dd` (4) |
| REQ-637 | `media.rs` `req637_export_unchanged` (1, 2, 5), `req637_export_after_write` (3, 5), `req637_export_after_format` (4, 5) |
| REQ-638 | `media.rs` `req638_seek_time` (1-4, 6), `req638_write_verify_time` (5) |

`req623_fast_sio_xl_os` uses a user-supplied Atari XL/XE OS ROM (REQ-004). When the ROM is not
there, the test prints a message that names the ROM and passes with no check.

The REQ-638 tests have the attribute `#[ignore = "needs seek model: REQ-638"]` until the seek
model is enabled. The REQ-608 and REQ-609 tests wait for the MaxFlash kinds in the same way.

## Open issues

1. POKEY interface. The POKEY design must agree with `SerialByte`, `host_byte` and
   `take_device_byte`. The POKEY side sets `SERIN`, the serial interrupts and the receive tone.
2. PIA interface. The PIA module must give the CB2 output level and tell the machine when it
   changes.
3. ANTIC DMA reads. `DmaMemory::dma_read` takes `&self` and gives `$FF` for `$D000-$D7FF`. To
   satisfy REQ-609, a DMA read of `$D5xx` must call `Cartridge::access_control`. The trait must
   take `&mut self`, or ANTIC must report the DMA address to the machine.
4. Loader memory. The XEX loader uses `$0700-$08FF`. A later version can move itself to the top
   of RAM. The zero-page bytes that the loader uses are not chosen yet.
5. RESET after an XEX start. The OS jumps through `DOSVEC` again. If the program did not change
   `DOSVEC`, the loader runs again. The loader could set `DOSVEC` to the run address before it
   runs the program.
6. The values of `ACK_DELAY`, `DATA_ACK_DELAY`, `OP_BASE_US` and the format time are project
   decisions. Measurements of a real 1050 can replace them.
7. To verify: CAR type numbers 41 and 42, and the power-on bank of the Atarimax 8Mbit types.
8. To verify: the fast SIO return state (Y, N flag, `DSTATS`) and the status codes for both OS
   ROMs, and the bus ID formula `DDEVIC + DUNIT - 1`.
9. To verify: when a real 1050 clears status byte 0 bits 0-3, and whether it sets bit 2 for a
   write-protect error.
10. Out of scope for now: drives D2: to D8:, high-speed SIO (US Doubler, XF551), sector-level
    rotational timing, the motor model and status bit 4, other cartridge types, ATX images,
    and cassette.
11. Cartridge interface conflict. The system design (`system.md`, "Cartridge port") asks for
    `read(&mut self)`, `peek(&self)`, `write` and `access_control(..) -> Option<u8>`. This
    document gives `read(&self)` and `access_control(..)` with no return value. The two
    documents must agree.
12. ANTIC DMA side effects. The system design gives DMA reads through `Cartridge::peek` with
    no side effects (REQ-213). REQ-609 criterion 2 needs a DMA read of `$D500-$D5FF` to switch
    the bank (see open issue 3).
13. To decide: no requirement covers the load of CAR types 41 and 42 (MaxFlash). REQ-605
    covers types 1 and 2 only, and REQ-608 covers only the bank switching.
14. To decide: REQ-632 uses a kept Percom geometry for Format `$21` only if it is DD. REQ-635
    says that the drive keeps the SD, ED or DD geometry for the next Format `$21`. The result
    of an ED block is not agreed.
