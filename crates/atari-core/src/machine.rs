//! The 65XE machine: memory map, bus scheduling and interrupts.
//!
//! Design: `docs/design/system.md`. Requirements: REQ-200 to REQ-217, REQ-232.
//!
//! # Memory map (REQ-204 to REQ-210, HRM 2.6)
//!
//! | Range | Contents |
//! | --- | --- |
//! | `$0000-$4FFF` | RAM |
//! | `$5000-$57FF` | Self-test ROM (OS ROM `$1000-$17FF`) if `PORTB` bit 7 = 0 and bit 0 = 1, else RAM |
//! | `$5800-$7FFF` | RAM |
//! | `$8000-$9FFF` | Cartridge (RD4), else RAM |
//! | `$A000-$BFFF` | Cartridge (RD5), else BASIC if `PORTB` bit 1 = 0, else RAM |
//! | `$C000-$CFFF` | OS ROM if `PORTB` bit 0 = 1, else RAM |
//! | `$D000-$D7FF` | I/O: GTIA `$D0xx`, POKEY `$D2xx`, PIA `$D3xx`, ANTIC `$D4xx`, cartridge control `$D5xx` |
//! | `$D800-$FFFF` | OS ROM if `PORTB` bit 0 = 1, else RAM |
//!
//! A write to an address that shows ROM does not change RAM (REQ-206).

use core::error::Error;
use core::fmt;

use crate::antic::{Antic, CycleOutput, DmaKind, DmaMemory};
use crate::cpu::{Bus, Cpu};
use crate::gtia::{ConsoleKeys, Gtia};
use crate::media::{Cartridge, DiskImage, SioBus};
use crate::pia::Pia;
use crate::pokey::Pokey;
use crate::state::StateError;
use crate::timing::VideoStandard;

/// Size of the OS ROM.
pub const OS_ROM_SIZE: usize = 0x4000;

/// Size of the BASIC ROM.
pub const BASIC_ROM_SIZE: usize = 0x2000;

/// `PORTB` bit 0: OS ROM enabled.
const PORTB_OS_ROM: u8 = 0x01;
/// `PORTB` bit 1: BASIC disabled.
const PORTB_BASIC_OFF: u8 = 0x02;
/// `PORTB` bit 7: self-test ROM disabled.
const PORTB_SELF_TEST_OFF: u8 = 0x80;

/// The configuration of a [`Machine`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineConfig {
    /// The 16 KB XL/XE OS ROM image (`$C000-$FFFF`).
    pub os_rom: Vec<u8>,
    /// The 8 KB BASIC ROM image (`$A000-$BFFF`), if any.
    pub basic_rom: Option<Vec<u8>>,
    /// The cartridge in the slot, if any.
    pub cartridge: Option<Cartridge>,
    /// The disk image in drive D1:, if any. With `None`, D1: is not on the SIO bus until
    /// [`Machine::insert_disk`] (`docs/design/media.md`, "Machine configuration").
    pub disk: Option<DiskImage>,
    /// The SIOV shortcut (REQ-623) is on.
    pub fast_sio: bool,
    /// The TV standard.
    pub standard: VideoStandard,
    /// The audio output sample rate in Hz.
    pub audio_sample_rate: u32,
}

/// An error in a [`MachineConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// The OS ROM is not 16384 bytes.
    OsRomSize(usize),
    /// The BASIC ROM is not 8192 bytes.
    BasicRomSize(usize),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OsRomSize(size) => {
                write!(f, "OS ROM must be {OS_ROM_SIZE} bytes, not {size} bytes")
            }
            Self::BasicRomSize(size) => {
                write!(
                    f,
                    "BASIC ROM must be {BASIC_ROM_SIZE} bytes, not {size} bytes"
                )
            }
        }
    }
}

impl Error for ConfigError {}

/// The state of one joystick. `true` = pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per physical switch"
)]
pub struct JoystickState {
    /// Up.
    pub up: bool,
    /// Down.
    pub down: bool,
    /// Left.
    pub left: bool,
    /// Right.
    pub right: bool,
    /// Fire button.
    pub fire: bool,
}

impl JoystickState {
    /// The 4 direction bits as the PIA pins see them (bit = 0 means pressed, HRM 7.1).
    const fn pins(self) -> u8 {
        let mut bits = 0x0F;
        if self.up {
            bits &= !0x01;
        }
        if self.down {
            bits &= !0x02;
        }
        if self.left {
            bits &= !0x04;
        }
        if self.right {
            bits &= !0x08;
        }
        bits
    }
}

/// RAM, ROMs and the cartridge: everything that the memory map selects between.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Memory {
    /// 64 KB of RAM.
    ram: Vec<u8>,
    /// The OS ROM.
    os: Vec<u8>,
    /// The BASIC ROM, if any.
    basic: Option<Vec<u8>>,
    /// The cartridge, if any.
    cart: Option<Cartridge>,
    /// The PIA port B pin levels (the memory control byte, REQ-211).
    portb: u8,
}

impl Memory {
    /// `true` while the OS ROM is enabled.
    const fn os_on(&self) -> bool {
        self.portb & PORTB_OS_ROM != 0
    }

    /// `true` while the self-test ROM is enabled (it needs the OS ROM too, HRM 2.6).
    const fn self_test_on(&self) -> bool {
        self.os_on() && self.portb & PORTB_SELF_TEST_OFF == 0
    }

    /// `true` while BASIC is enabled and present.
    const fn basic_on(&self) -> bool {
        self.basic.is_some() && self.portb & PORTB_BASIC_OFF == 0
    }

    /// The byte of a system ROM (OS, self-test, BASIC) at `addr`, if one shows there.
    fn system_rom(&self, addr: u16) -> Option<u8> {
        match addr {
            0x5000..=0x57FF if self.self_test_on() => {
                self.os.get(usize::from(addr - 0x5000) + 0x1000).copied()
            }
            0xA000..=0xBFFF if self.basic_on() => self
                .basic
                .as_ref()
                .and_then(|basic| basic.get(usize::from(addr - 0xA000)).copied()),
            0xC000..=0xCFFF | 0xD800..=0xFFFF if self.os_on() => {
                self.os.get(usize::from(addr - 0xC000)).copied()
            }
            _ => None,
        }
    }

    /// The RAM byte at `addr`.
    fn ram_byte(&self, addr: u16) -> u8 {
        self.ram.get(usize::from(addr)).copied().unwrap_or(0xFF)
    }

    /// A read outside `$D000-$D7FF`, with cartridge read side effects.
    fn read(&mut self, addr: u16) -> u8 {
        // Fast path: below `$8000`, only the self-test ROM at `$5000-$57FF` is not RAM.
        if addr < 0x5000 || (0x5800..0x8000).contains(&addr) {
            return self.ram_byte(addr);
        }
        if (0x8000..=0xBFFF).contains(&addr)
            && let Some(byte) = self.cart.as_mut().and_then(|cart| cart.read(addr))
        {
            return byte;
        }
        self.system_rom(addr).unwrap_or_else(|| self.ram_byte(addr))
    }

    /// A read outside `$D000-$D7FF`, without side effects.
    fn peek(&self, addr: u16) -> u8 {
        self.cart
            .as_ref()
            .and_then(|cart| cart.peek(addr))
            .or_else(|| self.system_rom(addr))
            .unwrap_or_else(|| self.ram_byte(addr))
    }

    /// A write outside `$D000-$D7FF` (REQ-206).
    fn write(&mut self, addr: u16, value: u8) {
        if let Some(cart) = self.cart.as_mut()
            && cart.peek(addr).is_some()
        {
            cart.write(addr, value);
            return;
        }
        if self.system_rom(addr).is_none()
            && let Some(byte) = self.ram.get_mut(usize::from(addr))
        {
            *byte = value;
        }
    }

    /// A bus access to the cartridge control area `$D500-$D5FF`.
    fn cart_control(&mut self, addr: u16, value: Option<u8>) -> u8 {
        self.cart
            .as_mut()
            .and_then(|cart| cart.access_control(addr, value))
            .unwrap_or(0xFF)
    }
}

impl DmaMemory for Memory {
    fn dma_read(&mut self, addr: u16) -> u8 {
        match addr {
            0xD500..=0xD5FF => self.cart_control(addr, None),
            0xD000..=0xD7FF => 0xFF,
            _ => self.read(addr),
        }
    }
}

/// One CPU bus access: for the debugger view of the last cycle and the bus log of the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct BusAccess {
    /// The address.
    addr: u16,
    /// The byte read or written.
    value: u8,
    /// `true` for a write.
    write: bool,
}

/// The CPU's view of the machine for one CPU cycle.
struct CpuBus<'a> {
    /// RAM, ROMs and cartridge.
    mem: &'a mut Memory,
    /// ANTIC.
    antic: &'a mut Antic,
    /// GTIA.
    gtia: &'a mut Gtia,
    /// POKEY.
    pokey: &'a mut Pokey,
    /// PIA.
    pia: &'a mut Pia,
    /// The value on the data bus after the access.
    data: u8,
    /// The access of this cycle, if the CPU made one.
    access: Option<BusAccess>,
}

impl Bus for CpuBus<'_> {
    fn read(&mut self, addr: u16) -> u8 {
        let [reg, page] = addr.to_le_bytes();
        let value = match page {
            0xD0 => self.gtia.read(reg),
            0xD2 => self.pokey.read(reg),
            0xD3 => self.pia.read(reg),
            0xD4 => self.antic.read(reg),
            0xD5 => self.mem.cart_control(addr, None),
            0xD1 | 0xD6 | 0xD7 => 0xFF,
            _ => self.mem.read(addr),
        };
        self.data = value;
        self.access = Some(BusAccess {
            addr,
            value,
            write: false,
        });
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.data = value;
        self.access = Some(BusAccess {
            addr,
            value,
            write: true,
        });
        let [reg, page] = addr.to_le_bytes();
        match page {
            0xD0 => {
                self.gtia.write(reg, value);
                // A `CONSOL` bit written as 1 pulls its line down, so the speaker line is high
                // when bit 3 was written as 0 (HRM 6.11, system.md "Timing").
                self.pokey.set_speaker(!self.gtia.speaker());
            }
            0xD2 => self.pokey.write(reg, value),
            0xD3 => {
                self.pia.write(reg, value);
                self.mem.portb = self.pia.port_b_pins();
            }
            0xD4 => self.antic.write(reg, value),
            0xD5 => {
                let _driven = self.mem.cart_control(addr, Some(value));
            }
            0xD1 | 0xD6 | 0xD7 => {}
            _ => self.mem.write(addr, value),
        }
    }
}

/// An Atari 65XE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    /// The CPU.
    cpu: Cpu,
    /// RAM, ROMs and cartridge.
    mem: Memory,
    /// ANTIC.
    antic: Antic,
    /// GTIA.
    gtia: Gtia,
    /// POKEY.
    pokey: Pokey,
    /// PIA.
    pia: Pia,
    /// The SIO bus: the SIO DATA IN level for POKEY, the bytes from POKEY and the command
    /// line (`docs/design/media.md`, "SIO bus and disk drive").
    sio: SioBus,
    /// The PIA CB2 level at the end of the last cycle, for the command line changes.
    cb2: bool,
    /// The SIOV shortcut is on (REQ-623). A setting of the frontend, not in the save state.
    fast_sio: bool,
    /// The TV standard.
    standard: VideoStandard,
    /// The last value on the data bus (the floating bus value for an idle cycle).
    data_bus: u8,
    /// Machine cycles since power-on.
    cycles: u64,
    /// Frames completed since power-on.
    frames: u64,
    /// The ANTIC output of the last machine cycle.
    last_out: CycleOutput,
    /// The CPU bus access of the last machine cycle, if the CPU had the bus.
    last_access: Option<BusAccess>,
}

impl Machine {
    /// Checks the ROM sizes, then does `power_on`.
    ///
    /// # Errors
    ///
    /// A [`ConfigError`] if a ROM has the wrong size.
    pub fn new(config: MachineConfig) -> Result<Self, ConfigError> {
        if config.os_rom.len() != OS_ROM_SIZE {
            return Err(ConfigError::OsRomSize(config.os_rom.len()));
        }
        if let Some(basic) = &config.basic_rom
            && basic.len() != BASIC_ROM_SIZE
        {
            return Err(ConfigError::BasicRomSize(basic.len()));
        }
        let mut machine = Self {
            cpu: Cpu::new(),
            mem: Memory {
                ram: vec![0; 0x1_0000],
                os: config.os_rom,
                basic: config.basic_rom,
                cart: config.cartridge,
                portb: 0xFF,
            },
            antic: Antic::new(config.standard),
            gtia: Gtia::new(config.standard),
            pokey: Pokey::new(config.standard, config.audio_sample_rate),
            pia: Pia::new(),
            sio: SioBus::new(config.standard),
            cb2: true,
            fast_sio: config.fast_sio,
            standard: config.standard,
            data_bus: 0xFF,
            cycles: 0,
            frames: 0,
            last_out: CycleOutput::default(),
            last_access: None,
        };
        // With no disk, D1: is not on the bus (the OS boot then sees no drive).
        machine.sio.set_drive_connected(false);
        if let Some(image) = config.disk {
            machine.insert_disk(image);
        }
        machine.power_on();
        Ok(machine)
    }

    /// Power off and on (REQ-215, REQ-216): RAM is `$00`, all chips are reset, and the CPU
    /// starts its reset sequence. The cycle and frame counters start again at 0.
    pub fn power_on(&mut self) {
        self.mem.ram.fill(0);
        // `Antic::reset` keeps the beam position, but a power-on must give the same machine
        // each time (REQ-216 criterion 3), so ANTIC also starts at line 0, cycle 0.
        self.antic = Antic::new(self.standard);
        self.antic.reset();
        self.gtia.reset();
        self.pokey.reset();
        self.pia.reset();
        self.mem.portb = self.pia.port_b_pins();
        self.sio.reset();
        self.cb2 = self.pia.cb2();
        if let Some(cart) = self.mem.cart.as_mut() {
            cart.cold_reset();
        }
        self.update_trig3();
        // The power-on registers of REQ-128 and a pending reset sequence (REQ-124).
        self.cpu = Cpu::new();
        self.cpu.reset();
        self.data_bus = 0xFF;
        self.cycles = 0;
        self.frames = 0;
        self.last_out = CycleOutput::default();
        self.last_access = None;
    }

    /// The System Reset button (REQ-217): resets the CPU, ANTIC and the PIA. RAM, GTIA and
    /// POKEY do not change, and ANTIC sends no NMI (HRM 2.4).
    pub fn press_reset(&mut self) {
        self.cpu.reset();
        self.antic.reset();
        self.pia.reset();
        self.mem.portb = self.pia.port_b_pins();
        self.update_trig3();
    }

    /// Runs one machine cycle, in the order of `docs/design/system.md`, "Timing".
    pub fn run_cycle(&mut self) {
        // 1. ANTIC, with the memory map of the end of the last cycle.
        let out = self.antic.tick(&mut self.mem);
        // 2. POKEY before the CPU access, with the SIO DATA IN level of this cycle. The bus
        //    counts the cycles as `Pokey::cycle` and `cycle_count` do: this cycle is
        //    number `cycles + 1`.
        let now = self.cycles + 1;
        self.pokey.set_serial_in(self.sio.data_in_level(now));
        self.pokey.tick();
        // 3. The IRQ line. The PIA level is the level of the end of the last cycle.
        self.cpu.set_irq(self.irq_line());
        // 4. The CPU access, if ANTIC does not halt the CPU (REQ-202).
        let mut data = self.data_bus;
        self.last_access = None;
        if out.halt {
            // 5. A refresh cycle drives no byte: the bus keeps the last value.
            if out.dma != DmaKind::Refresh {
                data = out.dma_data;
            }
        } else {
            // The SIOV shortcut, before the opcode fetch at SIOV (REQ-623).
            if self.fast_sio && self.cpu.at_opcode_fetch() {
                siov::shortcut(self);
            }
            let mut bus = CpuBus {
                mem: &mut self.mem,
                antic: &mut self.antic,
                gtia: &mut self.gtia,
                pokey: &mut self.pokey,
                pia: &mut self.pia,
                data,
                access: None,
            };
            let _tick = self.cpu.tick(&mut bus, out.rdy);
            data = bus.data;
            self.last_access = bus.access;
        }
        // 4a. The NMI level of this cycle, after the CPU access, also in a halted cycle: an
        //     `NMIEN` write of this cycle is included (OI-017).
        self.cpu.end_cycle(self.antic.nmi_level());
        // 5. The data bus value goes to ANTIC and GTIA.
        self.data_bus = data;
        self.antic.end_cycle(data);
        self.gtia.clock(&out, data);
        // 6. The PIA, then the bytes that POKEY sent go to the SIO bus, then the command
        //    line.
        self.pia.tick();
        while let Some(byte) = self.pokey.take_serial_byte() {
            self.sio.host_byte(byte);
        }
        let cb2 = self.pia.cb2();
        if cb2 != self.cb2 {
            self.cb2 = cb2;
            self.sio.set_command_line(!cb2, now);
        }
        // TRIG3 follows RD5 after each access that can switch the cartridge (REQ-232).
        self.update_trig3();
        // 7. The counters.
        self.cycles += 1;
        self.last_out = out;
        if out.frame_done {
            self.frames += 1;
        }
    }

    /// The CPU IRQ line: the PIA IRQ or the POKEY IRQ (REQ-229).
    const fn irq_line(&self) -> bool {
        self.pia.irq() || self.pokey.irq()
    }

    /// Runs machine cycles until ANTIC ends the frame.
    pub fn run_frame(&mut self) {
        let target = self.frames + 1;
        while self.frames < target {
            self.run_cycle();
        }
    }

    /// Sets the GTIA trigger 3 input from the cartridge RD5 line (REQ-232).
    fn update_trig3(&mut self) {
        let rd5 = self.mem.cart.as_ref().is_some_and(Cartridge::rd5);
        self.gtia.set_trigger(3, !rd5);
    }

    /// The last complete frame: GTIA color bytes, [`crate::gtia::FRAME_WIDTH`] x
    /// [`crate::gtia::FRAME_HEIGHT`]. Convert with [`crate::palette::rgb`].
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        self.gtia.frame()
    }

    /// The TV standard.
    #[must_use]
    pub const fn standard(&self) -> VideoStandard {
        self.standard
    }

    /// Machine cycles since power-on.
    #[must_use]
    pub const fn cycle_count(&self) -> u64 {
        self.cycles
    }

    /// Frames completed since power-on.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frames
    }

    /// The SIO bus.
    #[must_use]
    pub const fn sio(&self) -> &SioBus {
        &self.sio
    }

    /// The SIO bus, for the devices and for tests: a device sends its bytes on SIO DATA IN
    /// with [`SioBus::send_device_byte`].
    pub const fn sio_mut(&mut self) -> &mut SioBus {
        &mut self.sio
    }

    /// Inserts `image` in drive D1: and connects D1: to the SIO bus. An image in D1: is
    /// replaced. The machine is not reset.
    pub fn insert_disk(&mut self, image: DiskImage) {
        self.sio.drive_mut().insert(image);
        self.sio.set_drive_connected(true);
    }

    /// Removes the image from D1: and gives it back. The empty drive stays on the bus
    /// (REQ-633).
    pub const fn eject_disk(&mut self) -> Option<DiskImage> {
        self.sio.drive_mut().eject()
    }

    /// The image in D1:, with all writes and formats (REQ-637).
    #[must_use]
    pub const fn disk(&self) -> Option<&DiskImage> {
        self.sio.drive().image()
    }

    /// Turns the SIOV shortcut on or off (REQ-623).
    pub const fn set_fast_sio(&mut self, on: bool) {
        self.fast_sio = on;
    }

    /// The SIOV shortcut is on (REQ-623).
    #[must_use]
    pub const fn fast_sio(&self) -> bool {
        self.fast_sio
    }

    /// The CPU (for debuggers and tests).
    #[must_use]
    pub const fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// The CPU, for change (debuggers and tests).
    pub const fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    /// Memory as the CPU sees it, without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        let [reg, page] = addr.to_le_bytes();
        match page {
            0xD0 => self.gtia.read(reg),
            0xD2 => self.pokey.peek(reg),
            0xD3 => self.pia.peek(reg),
            0xD4 => self.antic.read(reg),
            0xD1 | 0xD5..=0xD7 => 0xFF,
            _ => self.mem.peek(addr),
        }
    }

    /// RAM under any ROM.
    #[must_use]
    pub fn peek_ram(&self, addr: u16) -> u8 {
        self.mem.ram_byte(addr)
    }

    /// Writes RAM under any ROM.
    pub fn poke_ram(&mut self, addr: u16, value: u8) {
        if let Some(byte) = self.mem.ram.get_mut(usize::from(addr)) {
            *byte = value;
        }
    }

    /// Presses a key: a full `KBCODE` with the SHIFT (`$40`) and CONTROL (`$80`) bits.
    pub const fn key_down(&mut self, kbcode: u8) {
        self.pokey.key_down(kbcode);
    }

    /// Releases the key and the modifiers that `key_down` pressed.
    pub const fn key_up(&mut self) {
        self.pokey.key_up();
    }

    /// Sets the state of the SHIFT key alone.
    pub const fn set_shift(&mut self, down: bool) {
        self.pokey.set_shift(down);
    }

    /// Presses (`down`) or releases one key of the matrix. `code` is a 6-bit key code; bits 6
    /// and 7 are ignored.
    pub const fn set_key(&mut self, code: u8, down: bool) {
        self.pokey.set_key(code, down);
    }

    /// Sets the state of the CONTROL key alone.
    pub const fn set_control(&mut self, down: bool) {
        self.pokey.set_control(down);
    }

    /// Sets the state of the BREAK key.
    pub const fn set_break(&mut self, down: bool) {
        self.pokey.set_break(down);
    }

    /// Sets the console keys (START, SELECT, OPTION).
    pub const fn set_console_keys(&mut self, keys: ConsoleKeys) {
        self.gtia.set_console_keys(keys);
    }

    /// Sets the state of joystick `port` (0 or 1) (REQ-439, REQ-440).
    pub fn set_joystick(&mut self, port: usize, state: JoystickState) {
        let current = self.pia.port_a_input();
        let pins = state.pins();
        let value = match port {
            0 => (current & 0xF0) | pins,
            1 => (current & 0x0F) | (pins << 4),
            _ => return,
        };
        self.pia.set_port_a_input(value);
        self.gtia.set_trigger(port, state.fire);
    }

    /// Sets paddle input `index` (0-3, `POT0-POT3`): the input reaches threshold at slow-scan
    /// count `count`. A count of 228 or more means no paddle (REQ-535).
    pub const fn set_paddle(&mut self, index: usize, count: u8) {
        self.pokey.set_pot(index, count);
    }

    /// Moves all audio samples produced since the last call into `out`.
    pub fn drain_audio(&mut self, out: &mut Vec<f32>) {
        self.pokey.drain_samples(out);
    }

    /// The complete machine state as bytes (REQ-018). The ROMs are not in the state:
    /// [`Machine::load_state`] needs a machine with the same [`MachineConfig`].
    #[must_use]
    pub fn save_state(&self) -> Vec<u8> {
        save::save(self)
    }

    /// Restores a state from [`Machine::save_state`] (REQ-018). After it, the machine runs
    /// as the saved machine did. The audio samples that were not drained are part of the
    /// state.
    ///
    /// # Errors
    ///
    /// A [`StateError`] if the bytes are not a save state of this format, or if the state is
    /// from a machine with another configuration. Then the machine does not change.
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), StateError> {
        save::load(self, bytes)
    }
}

mod save;
mod siov;

#[cfg(test)]
mod tests {
    mod halt;
    mod memory_map;
    mod triggers;

    use core::iter;

    use super::{BusAccess, ConfigError, CpuBus, JoystickState, Machine, MachineConfig};
    use crate::antic::DmaMemory as _;
    use crate::cpu::Bus as _;
    use crate::gtia::{ConsoleKeys, FRAME_HEIGHT, FRAME_WIDTH};
    use crate::media::Cartridge;
    use crate::timing::VideoStandard;

    /// The address of the test programs in the OS ROM.
    const PROGRAM: usize = 0x0200;

    /// What the machine did in one cycle, for the bus logs of the tests.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct CycleLog {
        /// ANTIC halted the CPU.
        halt: bool,
        /// The CPU bus access.
        access: Option<BusAccess>,
    }

    /// The OS ROM of the tests: byte `i` is `i >> 8` (the page number in the ROM).
    fn os_rom() -> Vec<u8> {
        (0..super::OS_ROM_SIZE)
            .map(|i| u8::try_from(i >> 8).unwrap())
            .collect()
    }

    /// A test OS ROM with `code` at `$C200`. The reset vector points to `$C200`, and the NMI
    /// and IRQ vectors point to an `RTI` at `$C100`.
    fn program_os(code: &[u8]) -> Vec<u8> {
        let mut os = os_rom();
        os[PROGRAM..PROGRAM + code.len()].copy_from_slice(code);
        os[0x0100] = 0x40;
        os[0x3FFA..].copy_from_slice(&[0x00, 0xC1, 0x00, 0xC2, 0x00, 0xC1]);
        os
    }

    /// A configuration with `os_rom`, BASIC of `$BB` bytes, and an optional cartridge.
    fn config(os_rom: Vec<u8>, cartridge: Option<Cartridge>) -> MachineConfig {
        MachineConfig {
            os_rom,
            basic_rom: Some(vec![0xBB; super::BASIC_ROM_SIZE]),
            cartridge,
            disk: None,
            fast_sio: false,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: 48_000,
        }
    }

    /// A machine with the test OS ROM, BASIC of `$BB` bytes, and an optional cartridge.
    fn machine_with(cartridge: Option<Cartridge>) -> Machine {
        Machine::new(config(os_rom(), cartridge)).unwrap()
    }

    /// A machine with the test OS ROM and BASIC.
    fn machine() -> Machine {
        machine_with(None)
    }

    /// A machine that runs `code` from `$C200` after power-on.
    fn program_machine(code: &[u8]) -> Machine {
        Machine::new(config(program_os(code), None)).unwrap()
    }

    /// An 8 KB cartridge of `$CC` bytes (RD5).
    fn cart_8k() -> Cartridge {
        Cartridge::from_raw(vec![0xCC; 0x2000]).unwrap()
    }

    /// A 16 KB cartridge of `$CC` bytes (RD4 and RD5).
    fn cart_16k() -> Cartridge {
        Cartridge::from_raw(vec![0xCC; 0x4000]).unwrap()
    }

    /// One CPU write through the machine bus.
    fn cpu_write(m: &mut Machine, addr: u16, value: u8) {
        let mut bus = CpuBus {
            mem: &mut m.mem,
            antic: &mut m.antic,
            gtia: &mut m.gtia,
            pokey: &mut m.pokey,
            pia: &mut m.pia,
            data: 0,
            access: None,
        };
        bus.write(addr, value);
    }

    /// One CPU read through the machine bus.
    fn cpu_read(m: &mut Machine, addr: u16) -> u8 {
        let mut bus = CpuBus {
            mem: &mut m.mem,
            antic: &mut m.antic,
            gtia: &mut m.gtia,
            pokey: &mut m.pokey,
            pia: &mut m.pia,
            data: 0,
            access: None,
        };
        bus.read(addr)
    }

    /// Sets the `PORTB` pins by CPU writes to the PIA: all bits output, then `value`.
    fn set_portb(m: &mut Machine, value: u8) {
        cpu_write(m, 0xD303, 0x00); // PBCTL: select DDRB
        cpu_write(m, 0xD301, 0xFF); // DDRB: all outputs
        cpu_write(m, 0xD303, 0x04); // PBCTL: select ORB
        cpu_write(m, 0xD301, value);
    }

    /// Sets `DDRB` and the port B output register by CPU writes; `PBCTL` = `$04` after.
    fn set_ddrb_orb(m: &mut Machine, ddrb: u8, orb: u8) {
        cpu_write(m, 0xD303, 0x04);
        cpu_write(m, 0xD301, orb);
        cpu_write(m, 0xD303, 0x00);
        cpu_write(m, 0xD301, ddrb);
        cpu_write(m, 0xD303, 0x04);
    }

    /// Runs `n` machine cycles and logs them.
    fn log_cycles(m: &mut Machine, n: u32) -> Vec<CycleLog> {
        (0..n)
            .map(|_| {
                m.run_cycle();
                CycleLog {
                    halt: m.last_out.halt,
                    access: m.last_access,
                }
            })
            .collect()
    }

    /// Runs machine cycles until ANTIC ends the frame, and logs them.
    fn log_frame(m: &mut Machine) -> Vec<CycleLog> {
        let mut log = Vec::new();
        loop {
            log.extend(log_cycles(m, 1));
            if m.last_out.frame_done {
                return log;
            }
        }
    }

    /// The CPU accesses in a log.
    fn accesses(log: &[CycleLog]) -> Vec<BusAccess> {
        log.iter().filter_map(|cycle| cycle.access).collect()
    }

    /// A machine whose program sets a display list of mode 2 lines at `$3000` and `DMACTL` =
    /// `$22`, and then runs `end` (from `$C20F`).
    fn dma_program(end: &[u8]) -> Machine {
        let mut code = vec![
            0xA9, 0x00, // LDA #$00
            0x8D, 0x02, 0xD4, // STA DLISTL
            0xA9, 0x30, // LDA #$30
            0x8D, 0x03, 0xD4, // STA DLISTH
            0xA9, 0x22, // LDA #$22
            0x8D, 0x00, 0xD4, // STA DMACTL
        ];
        code.extend_from_slice(end);
        let mut m = program_machine(&code);
        let mut list = vec![0x70, 0x70, 0x70, 0x42, 0x00, 0x40];
        list.extend([0x02; 23]);
        list.extend([0x41, 0x00, 0x30]);
        for (addr, byte) in (0x3000_u16..).zip(list) {
            m.poke_ram(addr, byte);
        }
        m
    }

    /// Checks the frame length of `standard` (REQ-201 criteria 2 and 3).
    fn check_frame_length(standard: VideoStandard, cycles: u32) {
        let mut m = Machine::new(MachineConfig {
            standard,
            basic_rom: None,
            ..config(os_rom(), None)
        })
        .unwrap();
        assert_eq!((m.antic.scanline(), m.antic.cycle()), (0, 0), "start");
        for _ in 0..cycles {
            m.run_cycle();
        }
        assert_eq!(
            m.cycle_count(),
            u64::from(cycles),
            "{standard:?} cycle counter"
        );
        assert_eq!(
            (m.antic.scanline(), m.antic.cycle()),
            (0, 0),
            "{standard:?}: line 0, cycle 0 again"
        );
        m.run_frame();
        let start = m.cycle_count();
        m.run_frame();
        assert_eq!(
            m.cycle_count() - start,
            u64::from(cycles),
            "{standard:?} run_frame"
        );
    }

    /// Verifies REQ-200: the machine uses the video standard of its configuration.
    #[test]
    fn machine_uses_configured_standard() {
        for (standard, cycles) in [(VideoStandard::Ntsc, 29_868), (VideoStandard::Pal, 35_568)] {
            let mut m = Machine::new(MachineConfig {
                standard,
                ..config(os_rom(), None)
            })
            .unwrap();
            m.run_frame();
            let start = m.cycle_count();
            m.run_frame();
            assert_eq!(m.cycle_count() - start, cycles, "{standard:?} frame length");
            assert_eq!(m.standard(), standard, "standard");
            assert_eq!(m.frame_count(), 2, "frames");
        }
    }

    /// Verifies REQ-201: 114 machine cycles are one scan line (criterion 1).
    #[test]
    fn scan_line_is_114_cycles() {
        let mut m = machine();
        assert_eq!((m.antic.scanline(), m.antic.cycle()), (0, 0), "start");
        for _ in 0..113 {
            m.run_cycle();
        }
        assert_eq!((m.antic.scanline(), m.antic.cycle()), (0, 113), "cycle 113");
        m.run_cycle();
        assert_eq!((m.antic.scanline(), m.antic.cycle()), (1, 0), "line 1");
        assert_eq!(m.cycle_count(), 114, "cycle counter");
    }

    /// Verifies REQ-201: an NTSC frame is 262 lines of 29,868 cycles (criterion 2).
    #[test]
    fn ntsc_frame_is_29868_cycles() {
        check_frame_length(VideoStandard::Ntsc, 29_868);
    }

    /// Verifies REQ-201: a PAL frame is 312 lines of 35,568 cycles (criterion 3).
    #[test]
    fn pal_frame_is_35568_cycles() {
        check_frame_length(VideoStandard::Pal, 35_568);
    }

    /// Verifies REQ-202: the CPU makes no access in the cycles in which ANTIC does DMA or
    /// refresh (criterion 1).
    #[test]
    fn dma_cycles_halt_cpu() {
        let mut m = dma_program(&[0x4C, 0x0F, 0xC2]); // JMP $C20F
        m.run_frame();
        m.run_frame();
        let log = log_frame(&mut m);
        let halted: Vec<_> = log.iter().filter(|cycle| cycle.halt).collect();
        assert!(
            halted.len() >= 192 * 40,
            "{} halted cycles: at least 40 for each mode 2 scan line",
            halted.len()
        );
        assert!(
            halted.iter().all(|cycle| cycle.access.is_none()),
            "no CPU access while ANTIC has the bus"
        );
    }

    /// Verifies REQ-202: the CPU makes exactly one access in each cycle without DMA or
    /// refresh, also while RDY is low (criterion 2).
    #[test]
    fn free_cycles_run_cpu() {
        let mut m = dma_program(&[0x4C, 0x0F, 0xC2]); // JMP $C20F
        m.run_frame();
        let log = log_frame(&mut m);
        assert_eq!(log.len(), 29_868, "one frame");
        for (cycle, entry) in log.iter().enumerate() {
            assert_eq!(
                entry.access.is_some(),
                !entry.halt,
                "cycle {cycle}: one CPU access exactly when ANTIC does not halt the CPU"
            );
        }
    }

    /// Verifies REQ-202: ANTIC never writes: RAM does not change while the CPU is jammed
    /// (criterion 3).
    #[test]
    fn antic_never_writes() {
        let mut m = dma_program(&[0x02, 0xEA, 0xEA]); // KIL
        m.run_frame();
        let before = m.mem.ram.clone();
        for _ in 0..10 {
            m.run_frame();
        }
        assert!(m.mem.ram == before, "RAM did not change in 10 frames");
    }

    /// Verifies REQ-203: a CPU bus access reaches the PIA with its side effects.
    #[test]
    fn bus_reads_have_side_effects() {
        let mut m = machine();
        set_ddrb_orb(&mut m, 0xFF, 0xFD);
        cpu_write(&mut m, 0xD303, 0x3C);
        m.pia.set_cb1(false);
        assert_eq!(m.peek(0xD303), 0xBC, "PBCTL bit 7 set");
        assert_eq!(m.peek(0xD301), 0xFD, "peek of PORTB");
        assert_eq!(m.peek(0xD303), 0xBC, "peek has no side effect");
        assert_eq!(cpu_read(&mut m, 0xD3F1), 0xFD, "CPU read of PORTB");
        assert_eq!(m.peek(0xD303), 0x3C, "the read cleared bit 7");
    }

    /// Verifies REQ-203: the dummy read of `LDA $D3F0,X` reads `$D301` and clears the PIA
    /// status (criterion 1).
    #[test]
    fn dummy_read_clears_pia_status() {
        let mut m = program_machine(&[
            0xA2, 0x11, // LDX #$11
            0xBD, 0xF0, 0xD3, // LDA $D3F0,X
            0x4C, 0x05, 0xC2, // JMP $C205
        ]);
        cpu_write(&mut m, 0xD303, 0x3C);
        m.pia.set_cb1(false);
        assert_eq!(m.peek(0xD303), 0xBC, "PBCTL bit 7 set");
        let log = accesses(&log_cycles(&mut m, 100));
        let reads: Vec<u16> = log
            .iter()
            .filter(|access| (0xD000..0xD800).contains(&access.addr))
            .map(|access| access.addr)
            .take(2)
            .collect();
        assert_eq!(reads, [0xD301, 0xD401], "dummy read, then the real read");
        assert_eq!(m.peek(0xD303) & 0xC0, 0, "PBCTL bits 7 and 6 are 0");
    }

    /// Verifies REQ-203: `INC $D301` writes `PORTB` twice (criterion 2).
    #[test]
    fn rmw_writes_portb_twice() {
        let mut m = program_machine(&[0x4C, 0x00, 0x20]); // JMP $2000
        for (addr, byte) in (0x2000_u16..).zip([0xEE, 0x01, 0xD3, 0x4C, 0x03, 0x20]) {
            m.poke_ram(addr, byte); // INC $D301, JMP $2003
        }
        m.poke_ram(0xC000, 0xAA);
        set_ddrb_orb(&mut m, 0xFF, 0xFD);
        cpu_write(&mut m, 0xD303, 0x3C);
        let log = accesses(&log_cycles(&mut m, 100));
        let writes: Vec<u8> = log
            .iter()
            .filter(|access| access.write && access.addr == 0xD301)
            .map(|access| access.value)
            .collect();
        assert_eq!(writes, [0xFD, 0xFE], "the PIA receives 2 writes");
        assert_eq!(m.peek(0xC000), 0xAA, "OS ROM disabled");
    }

    /// A `MaxFlash` 1Mbit cartridge (CAR type 41, REQ-608) with the bank pattern: each byte of
    /// bank `k` holds `k`.
    fn maxflash() -> Cartridge {
        let data: Vec<u8> = (0..0x2_0000_usize)
            .map(|i| u8::try_from(i >> 13).unwrap())
            .collect();
        let sum = data
            .iter()
            .fold(0_u32, |sum, &byte| sum.wrapping_add(u32::from(byte)));
        let mut file = b"CART".to_vec();
        file.extend(41_u32.to_be_bytes());
        file.extend(sum.to_be_bytes());
        file.extend([0; 4]);
        file.extend(data);
        Cartridge::from_car(&file).unwrap()
    }

    /// A machine that runs `code` from `$C200` with the `MaxFlash` 1Mbit cartridge.
    fn maxflash_machine(code: &[u8]) -> Machine {
        Machine::new(config(program_os(code), Some(maxflash()))).unwrap()
    }

    /// `true` while the cartridge maps `$A000-$BFFF`.
    fn rd5(m: &Machine) -> bool {
        m.mem.cart.as_ref().is_some_and(Cartridge::rd5)
    }

    /// Verifies REQ-203: the dummy read of `LDA $D5F0,X` with `X` = `$20` reaches the
    /// cartridge control area (criterion 3). The `MaxFlash` 1Mbit cartridge (REQ-608) turns RD5
    /// off on an access to `$D510`, so its state shows the access.
    #[test]
    fn dummy_read_reaches_cartridge_control() {
        let mut m = maxflash_machine(&[
            0xA2, 0x20, // LDX #$20
            0xBD, 0xF0, 0xD5, // LDA $D5F0,X
            0x4C, 0x05, 0xC2, // JMP $C205
        ]);
        assert!(rd5(&m), "RD5 after power-on");
        let log = accesses(&log_cycles(&mut m, 100));
        let io: Vec<(u16, bool)> = log
            .iter()
            .filter(|access| (0xD000..0xD800).contains(&access.addr))
            .map(|access| (access.addr, access.write))
            .collect();
        assert_eq!(
            io,
            [(0xD510, false), (0xD610, false)],
            "one dummy read of $D510, then the read of $D610"
        );
        assert!(!rd5(&m), "the cartridge received the read of $D510");
        assert_eq!(m.peek(0xD013), 0x00, "TRIG3 follows RD5");
        assert_eq!(m.peek(0xA000), 0x00, "RAM at $A000 (BASIC off)");
    }

    /// Verifies REQ-203: while RDY is low after `STA WSYNC`, the CPU makes one read in each
    /// stopped cycle and one more read, and then its read of `$D503` reaches the cartridge
    /// control area (criterion 4, in part).
    ///
    /// Criterion 4 runs the code from `$D500`. No cartridge kind of the design drives the bus
    /// at `$D5xx` (`docs/design/media.md`, `access_control`), so the code runs from the OS
    /// ROM: the repeated reads are of the `LDA` operand address, and the `MaxFlash` 1Mbit
    /// cartridge shows the read of `$D503` as bank 3.
    #[test]
    fn rdy_reads_reach_cartridge_control() {
        let mut m = maxflash_machine(&[
            0x8D, 0x0A, 0xD4, // STA WSYNC
            0xAD, 0x03, 0xD5, // LDA $D503
            0x4C, 0x06, 0xC2, // JMP $C206
        ]);
        let mut log = Vec::new();
        for _ in 0..300 {
            m.run_cycle();
            log.push((m.last_out.rdy, m.last_out.halt, m.last_access));
        }
        let wsync = log
            .iter()
            .position(|&(_, _, access)| access.is_some_and(|a| a.write && a.addr == 0xD40A))
            .unwrap();
        let after = &log[wsync + 1..];
        assert_eq!(
            after[0].2.map(|a| a.addr),
            Some(0xC203),
            "the LDA opcode fetch runs in the next cycle"
        );
        let stopped = after
            .iter()
            .filter(|&&(rdy, halt, _)| !rdy && !halt)
            .count();
        assert!(stopped > 0, "RDY is low");
        let operand_reads = after
            .iter()
            .filter(|&&(_, _, access)| access.is_some_and(|a| a.addr == 0xC204 && !a.write))
            .count();
        assert_eq!(
            operand_reads,
            stopped + 1,
            "one read per stopped cycle, plus one"
        );
        let control_reads = after
            .iter()
            .filter(|&&(_, _, access)| access.is_some_and(|a| a.addr == 0xD503))
            .count();
        assert_eq!(control_reads, 1, "one read of $D503");
        assert_eq!(m.peek(0xA000), 0x03, "the cartridge shows bank 3");
    }

    /// Verifies REQ-204: with all ROMs off, each address outside `$D000-$D7FF` is RAM
    /// (criterion 1).
    #[test]
    fn all_ram_visible_with_roms_off() {
        let mut m = machine();
        set_portb(&mut m, 0xFE); // OS off, BASIC off, self-test off
        for addr in 0x0000..=0xFFFF_u16 {
            if (0xD000..=0xD7FF).contains(&addr) {
                continue;
            }
            for value in [0xA5, 0x5A] {
                cpu_write(&mut m, addr, value);
                assert_eq!(cpu_read(&mut m, addr), value, "RAM at ${addr:04X}");
            }
        }
    }

    /// Verifies REQ-204: the low RAM is visible with all ROMs and a cartridge on
    /// (criterion 2).
    #[test]
    fn low_ram_always_visible() {
        let mut m = machine_with(Some(cart_8k()));
        set_portb(&mut m, 0x7D); // OS, BASIC and self-test on
        for addr in (0x0000..=0x4FFF).chain(0x5800..=0x9FFF_u16) {
            let [value, _] = addr.to_le_bytes();
            cpu_write(&mut m, addr, value ^ 0x5A);
            assert_eq!(cpu_read(&mut m, addr), value ^ 0x5A, "RAM at ${addr:04X}");
        }
    }

    /// Verifies REQ-205: `$D40B` reads `VCOUNT` for each `PORTB` value (criterion 1).
    #[test]
    fn io_area_cannot_be_disabled() {
        let mut m = machine();
        m.poke_ram(0xD40B, 0xFF);
        for portb in 0..=0xFF {
            set_portb(&mut m, portb);
            assert_eq!(
                cpu_read(&mut m, 0xD40B),
                m.antic.read(0x0B),
                "VCOUNT with PORTB ${portb:02X}"
            );
        }
    }

    /// Verifies REQ-205: writes to `$D000-$D7FF` do not change the RAM under the I/O area
    /// (criterion 2).
    #[test]
    fn io_write_does_not_change_ram() {
        let mut m = machine_with(Some(cart_8k()));
        set_portb(&mut m, 0xFE);
        for addr in 0xD000..=0xD7FF {
            m.poke_ram(addr, 0xAA);
        }
        for page in 0xD0..=0xD7_u16 {
            let addr = (page << 8) | 0xFF;
            cpu_write(&mut m, addr, 0x55);
            assert_eq!(m.peek_ram(addr), 0xAA, "RAM at ${addr:04X}");
        }
    }

    /// Verifies REQ-205: the I/O pages reach the chips at their mirror addresses
    /// (criteria 3 and 4).
    #[test]
    fn io_pages_reach_chips() {
        let mut m = machine();
        assert_eq!(cpu_read(&mut m, 0xD014), 0x0F, "GTIA PAL register (NTSC)");
        assert_eq!(cpu_read(&mut m, 0xD0F4), 0x0F, "GTIA mirror");
        assert_eq!(
            cpu_read(&mut m, 0xD20F) & 0x04,
            0x04,
            "POKEY SKSTAT: no key"
        );
        assert_eq!(cpu_read(&mut m, 0xD40B), m.peek(0xD40B), "ANTIC VCOUNT");
        assert_eq!(cpu_read(&mut m, 0xD47B), m.peek(0xD40B), "VCOUNT at $D47B");
        cpu_write(&mut m, 0xD3FF, 0x3C);
        assert_eq!(cpu_read(&mut m, 0xD303), 0x3C, "PBCTL written at $D3FF");
        assert_eq!(m.peek(0xD303), 0x3C, "peek of PBCTL");
        assert_eq!(m.peek(0xD20F) & 0x04, 0x04, "peek of SKSTAT");
        let mut mirror = machine();
        let mut base = machine();
        cpu_write(&mut mirror, 0xD2F0, 0xA0);
        cpu_write(&mut base, 0xD200, 0xA0);
        assert_eq!(mirror.pokey, base.pokey, "POKEY write at $D2F0 = at $D200");
        m.poke_ram(0xD013, 0xAA);
        assert_eq!(cpu_read(&mut m, 0xD013), 0x00, "TRIG3, not RAM");
    }

    /// Verifies REQ-206: a write to the OS ROM does not change the RAM under it
    /// (criterion 1).
    #[test]
    fn write_to_os_rom_ignored() {
        let mut m = machine();
        m.poke_ram(0xE000, 0xAA);
        set_portb(&mut m, 0xFF);
        cpu_write(&mut m, 0xE000, 0x55);
        set_portb(&mut m, 0xFE);
        assert_eq!(cpu_read(&mut m, 0xE000), 0xAA, "RAM under the OS ROM");
    }

    /// Verifies REQ-206: a write to BASIC does not change the RAM under it (criterion 2).
    #[test]
    fn write_to_basic_ignored() {
        let mut m = machine();
        m.poke_ram(0xA000, 0xAA);
        set_portb(&mut m, 0xFD);
        cpu_write(&mut m, 0xA000, 0x55);
        set_portb(&mut m, 0xFF);
        assert_eq!(cpu_read(&mut m, 0xA000), 0xAA, "RAM under BASIC");
    }

    /// Verifies REQ-206: a write to the cartridge ROM does not change the RAM under it
    /// (criteria 3 and 5).
    #[test]
    fn write_to_cartridge_rom_ignored() {
        let mut m = machine_with(Some(cart_8k()));
        m.poke_ram(0xB000, 0xAA);
        cpu_write(&mut m, 0xB000, 0x55);
        assert_eq!(m.peek_ram(0xB000), 0xAA, "3: RAM at $B000");
        assert_eq!(cpu_read(&mut m, 0xB000), 0xCC, "3: the cartridge byte");
        let mut big = machine_with(Some(cart_16k()));
        big.poke_ram(0x8000, 0x77);
        cpu_write(&mut big, 0x8000, 0x00);
        assert_eq!(big.peek_ram(0x8000), 0x77, "5: RAM at $8000");
        assert_eq!(cpu_read(&mut big, 0x8000), 0xCC, "5: the cartridge byte");
    }

    /// Verifies REQ-206: a write to the self-test ROM does not change the RAM under it
    /// (criterion 4).
    #[test]
    fn write_to_self_test_ignored() {
        let mut m = machine();
        m.poke_ram(0x5000, 0xAA);
        set_portb(&mut m, 0x7F);
        cpu_write(&mut m, 0x5000, 0x55);
        assert_eq!(m.peek_ram(0x5000), 0xAA, "RAM at $5000");
        assert_eq!(cpu_read(&mut m, 0x5000), 0x10, "OS image byte $1000");
    }

    /// Verifies REQ-207: the OS ROM shows at `$C000-$CFFF` and `$D800-$FFFF` (criterion 1).
    #[test]
    fn os_rom_mapping() {
        let mut m = machine();
        set_portb(&mut m, 0xFF);
        let os = os_rom();
        for (addr, offset) in [
            (0xC000, 0x0000),
            (0xCFFF, 0x0FFF),
            (0xD800, 0x1800),
            (0xFFFF, 0x3FFF),
        ] {
            assert_eq!(cpu_read(&mut m, addr), os[offset], "${addr:04X}");
        }
    }

    /// Verifies REQ-207: the I/O area hides the OS ROM at `$D000-$D7FF` (criterion 2).
    #[test]
    fn os_rom_hidden_under_io() {
        let mut m = machine();
        set_portb(&mut m, 0xFF);
        assert_ne!(m.antic.read(0x0B), 0x14, "the test needs VCOUNT != OS byte");
        assert_eq!(cpu_read(&mut m, 0xD40B), m.antic.read(0x0B), "VCOUNT");
    }

    /// Verifies REQ-207: with `PORTB` pin 0 = 0, RAM shows at the OS ROM addresses
    /// (criterion 3).
    #[test]
    fn os_rom_disabled_shows_ram() {
        let mut m = machine();
        set_portb(&mut m, 0xFE);
        for addr in (0xC000..=0xCFFF).chain(0xD800..=0xFFFF_u16) {
            let [value, _] = addr.to_le_bytes();
            cpu_write(&mut m, addr, !value);
            assert_eq!(cpu_read(&mut m, addr), !value, "RAM at ${addr:04X}");
        }
    }

    /// Verifies REQ-207: the OS ROM must be 16,384 bytes (criterion 4).
    #[test]
    fn os_rom_size_checked() {
        assert!(Machine::new(config(os_rom(), None)).is_ok(), "16,384 bytes");
        for size in [0x2000, 0x3FFF, 0x4001] {
            let error = Machine::new(config(vec![0; size], None)).unwrap_err();
            assert_eq!(error, ConfigError::OsRomSize(size), "{size} bytes");
            assert_eq!(
                error.to_string(),
                format!("OS ROM must be 16384 bytes, not {size} bytes"),
                "message"
            );
        }
        let error = Machine::new(MachineConfig {
            basic_rom: Some(vec![0; 100]),
            ..config(os_rom(), None)
        })
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "BASIC ROM must be 8192 bytes, not 100 bytes",
            "BASIC size"
        );
    }

    /// Verifies REQ-208: `PORTB` pin 1 = 0 shows BASIC (criterion 1).
    #[test]
    fn basic_enabled_by_bit_1_low() {
        let mut basic = vec![0xBB; super::BASIC_ROM_SIZE];
        basic[0] = 0x01;
        basic[0x1FFF] = 0x02;
        let mut m = Machine::new(MachineConfig {
            basic_rom: Some(basic),
            ..config(os_rom(), None)
        })
        .unwrap();
        set_portb(&mut m, 0xFD);
        assert_eq!(cpu_read(&mut m, 0xA000), 0x01, "BASIC byte $0000");
        assert_eq!(cpu_read(&mut m, 0xBFFF), 0x02, "BASIC byte $1FFF");
    }

    /// Verifies REQ-208: `PORTB` pin 1 = 1 shows RAM at `$A000-$BFFF` (criterion 2).
    #[test]
    fn basic_disabled_shows_ram() {
        let mut m = machine();
        set_portb(&mut m, 0xFF);
        for addr in 0xA000..=0xBFFF_u16 {
            let [value, _] = addr.to_le_bytes();
            cpu_write(&mut m, addr, value);
            assert_eq!(cpu_read(&mut m, addr), value, "RAM at ${addr:04X}");
        }
    }

    /// Verifies REQ-208: with no BASIC image, RAM shows at `$A000-$BFFF` (criterion 3).
    #[test]
    fn no_basic_image_shows_ram() {
        let mut m = Machine::new(MachineConfig {
            basic_rom: None,
            ..config(os_rom(), None)
        })
        .unwrap();
        set_portb(&mut m, 0xFD);
        for addr in 0xA000..=0xBFFF_u16 {
            let [value, _] = addr.to_le_bytes();
            cpu_write(&mut m, addr, value);
            assert_eq!(cpu_read(&mut m, addr), value, "RAM at ${addr:04X}");
        }
    }

    /// Verifies REQ-209: `PORTB` pins = `$7F` map the self-test ROM: CPU reads of `$5000`
    /// and `$57FF` give the OS image bytes `$1000` and `$17FF` (criterion 1).
    #[test]
    fn self_test_rom_mapping() {
        let mut os = os_rom();
        os[0x1000] = 0x5E;
        os[0x17FF] = 0x5F;
        let mut m = Machine::new(config(os, None)).unwrap();
        set_portb(&mut m, 0x7F);
        assert_eq!(cpu_read(&mut m, 0x5000), 0x5E, "OS image byte $1000");
        assert_eq!(cpu_read(&mut m, 0x57FF), 0x5F, "OS image byte $17FF");
        assert_eq!(m.peek(0x5000), 0x5E, "peek of $5000");
    }

    /// Verifies REQ-209: the self-test ROM needs the OS ROM: with `PORTB` pins = `$7E`, a CPU
    /// write and then a CPU read of each address in `$5000-$57FF` give the written value
    /// (criterion 2).
    #[test]
    fn self_test_needs_os_rom() {
        let mut m = machine();
        set_portb(&mut m, 0x7E);
        for addr in 0x5000..=0x57FF_u16 {
            let [lo, hi] = addr.to_le_bytes();
            let value = lo ^ hi ^ 0x3C;
            cpu_write(&mut m, addr, value);
            assert_eq!(cpu_read(&mut m, addr), value, "RAM at ${addr:04X}");
        }
    }

    /// Verifies REQ-210: cartridge before BASIC before RAM.
    #[test]
    fn cartridge_before_basic() {
        let mut m = machine_with(Some(cart_16k()));
        set_portb(&mut m, 0xFD);
        assert_eq!(m.peek(0xA000), 0xCC, "cartridge before BASIC");
        assert_eq!(
            cpu_read(&mut m, 0xA000),
            0xCC,
            "CPU read: cartridge before BASIC"
        );
        assert_eq!(m.peek(0x8000), 0xCC, "16 KB cartridge at $8000 (RD4)");
        assert_eq!(cpu_read(&mut m, 0x8000), 0xCC, "CPU read at $8000 (RD4)");
        let mut plain = machine();
        set_portb(&mut plain, 0xFF);
        cpu_write(&mut plain, 0xA000, 0xC3);
        assert_eq!(
            plain.peek(0xA000),
            0xC3,
            "RAM with no cartridge and BASIC off"
        );
    }

    /// Writes `$11` to the RAM at `$C000`, `$A000` and `$5000`, and returns what a CPU read
    /// of those addresses gives.
    fn map_view(m: &mut Machine) -> [u8; 3] {
        for addr in [0xC000, 0xA000, 0x5000] {
            m.poke_ram(addr, 0x11);
        }
        [0xC000, 0xA000, 0x5000].map(|addr| cpu_read(m, addr))
    }

    /// OS ROM at `$C000`, RAM at `$A000` and `$5000`.
    const RESET_MAP: [u8; 3] = [0x00, 0x11, 0x11];

    /// Verifies REQ-211: after a PIA reset, the OS ROM is on, BASIC and self-test are off
    /// (criteria 1 and 3).
    #[test]
    fn reset_memory_map() {
        let mut m = machine();
        assert_eq!(map_view(&mut m), RESET_MAP, "1: after power-on");
        set_ddrb_orb(&mut m, 0x00, 0x00);
        assert_eq!(map_view(&mut m), RESET_MAP, "3: DDRB = $00, ORB = $00");
    }

    /// Verifies REQ-211: output bits select the map (criterion 2).
    #[test]
    fn portb_output_selects_map() {
        let mut m = machine();
        set_ddrb_orb(&mut m, 0xFF, 0xFC);
        assert_eq!(map_view(&mut m), [0x11, 0xBB, 0x11], "OS off, BASIC on");
    }

    /// Verifies REQ-211: input bits 0, 1 and 7 are pulled up (criteria 1 and 3).
    #[test]
    fn portb_inputs_pulled_up() {
        let mut m = machine();
        set_ddrb_orb(&mut m, 0x7C, 0x00);
        assert_eq!(m.mem.portb & 0x83, 0x83, "pins 0, 1 and 7 high");
        assert_eq!(map_view(&mut m), RESET_MAP, "map of the pull-ups");
    }

    /// Verifies REQ-211: a `DDRB` write changes the map (criterion 4).
    #[test]
    fn ddrb_write_changes_map() {
        let mut m = machine();
        set_ddrb_orb(&mut m, 0x00, 0x00);
        assert_eq!(map_view(&mut m)[0], 0x00, "OS ROM before the write");
        cpu_write(&mut m, 0xD303, 0x00);
        cpu_write(&mut m, 0xD301, 0x01);
        assert_eq!(map_view(&mut m)[0], 0x11, "RAM after the write");
    }

    /// Verifies REQ-211: `DDRB` = `$80` with output register `$00` maps the self-test ROM
    /// and keeps BASIC off (criterion 5).
    #[test]
    fn mixed_inputs_and_outputs_select_map() {
        let mut m = machine();
        set_ddrb_orb(&mut m, 0x80, 0x00);
        assert_eq!(cpu_read(&mut m, 0x5000), 0x10, "OS image byte $1000");
        assert_eq!(map_view(&mut m), [0x00, 0x11, 0x10], "OS on, BASIC off");
    }

    /// Verifies REQ-212: the next DMA read after a PIA write uses the new map (criterion 3).
    #[test]
    fn bank_switch_next_dma_access() {
        let mut m = machine();
        m.poke_ram(0xE000, 0xAA);
        assert_eq!(m.mem.dma_read(0xE000), 0x20, "OS ROM before");
        set_portb(&mut m, 0xFE);
        assert_eq!(m.mem.dma_read(0xE000), 0xAA, "RAM after the write");
    }

    /// Verifies REQ-213: for `PORTB` pins `$FF`, `$FE`, `$FD`, `$7F` and `$7E`, with no
    /// cartridge, an 8 KB cartridge and a 16 KB cartridge (RD4 and RD5), an ANTIC DMA read of
    /// each address in `$0000-$CFFF` and `$D800-$FFFF` gives the CPU read value (criterion
    /// 3).
    #[test]
    fn dma_map_equals_cpu_map() {
        for cart in [None, Some(cart_8k()), Some(cart_16k())] {
            let mut m = machine_with(cart);
            // RAM with a pattern, so that RAM and ROM bytes differ.
            for addr in 0..=0xFFFF_u16 {
                m.poke_ram(addr, addr.to_le_bytes()[0] ^ 0x5A);
            }
            for portb in [0xFF, 0xFE, 0xFD, 0x7F, 0x7E] {
                set_portb(&mut m, portb);
                let addrs = (0x0000..=0xFFFF_u16).filter(|addr| !(0xD000..=0xD7FF).contains(addr));
                for addr in addrs {
                    let cpu = cpu_read(&mut m, addr);
                    assert_eq!(
                        m.mem.dma_read(addr),
                        cpu,
                        "PORTB ${portb:02X} at ${addr:04X}"
                    );
                }
            }
            assert_eq!(
                m.mem.dma_read(0xD500),
                0xFF,
                "DMA read of cartridge control"
            );
            assert_eq!(m.mem.dma_read(0xD40B), 0xFF, "DMA read of other I/O");
        }
    }

    /// Verifies REQ-214: unmapped I/O pages, and `$D5xx` without a driving cartridge, read
    /// `$FF` for the CPU and for `peek`.
    #[test]
    fn unmapped_io_reads_ff() {
        for cart in [None, Some(cart_8k())] {
            let mut m = machine_with(cart);
            for addr in (0xD100..=0xD1FF).chain(0xD500..=0xD7FF) {
                assert_eq!(cpu_read(&mut m, addr), 0xFF, "CPU read of ${addr:04X}");
                assert_eq!(m.peek(addr), 0xFF, "peek of ${addr:04X}");
            }
        }
    }

    /// Verifies REQ-215: after power-on, all RAM is `$00` (criterion 1).
    #[test]
    fn power_on_ram_zero() {
        let m = machine();
        assert!(
            (0..=0xFFFF).all(|addr| m.peek_ram(addr) == 0),
            "all RAM is $00"
        );
    }

    /// Verifies REQ-215: a second power-on clears RAM (criterion 2).
    #[test]
    fn second_power_on_clears_ram() {
        let mut m = machine();
        cpu_write(&mut m, 0x1000, 0x55);
        m.poke_ram(0xE000, 0xFF);
        m.run_frame();
        m.power_on();
        assert!(
            (0..=0xFFFF).all(|addr| m.peek_ram(addr) == 0),
            "all RAM is $00"
        );
        assert_eq!((m.cycle_count(), m.frame_count()), (0, 0), "counters");
    }

    /// Verifies REQ-215: two machines with the same configuration have the same RAM after
    /// one frame (criterion 3).
    #[test]
    fn power_on_deterministic() {
        let mut a = machine();
        let mut b = machine();
        a.run_frame();
        b.run_frame();
        assert!(a.mem.ram == b.mem.ram, "the same RAM");
        assert_eq!(a, b, "the same machine");
    }

    /// Verifies REQ-216: after power-on, the CPU runs the reset sequence and fetches the
    /// opcode at the reset vector (criterion 1).
    #[test]
    fn power_on_starts_at_reset_vector() {
        let mut os = os_rom();
        os[0x3FFC] = 0x34;
        os[0x3FFD] = 0xC2;
        let mut m = Machine::new(config(os, None)).unwrap();
        let log = accesses(&log_cycles(&mut m, 20));
        assert!(log[..7].iter().all(|access| !access.write), "7 reads");
        assert_eq!((log[7].addr, log[7].write), (0xC234, false), "opcode fetch");
    }

    /// Verifies REQ-216: power-on resets the PIA (criterion 2).
    #[test]
    fn power_on_clears_pia() {
        let mut m = machine();
        cpu_write(&mut m, 0xD300, 0xFF);
        cpu_write(&mut m, 0xD301, 0xFF);
        cpu_write(&mut m, 0xD302, 0x3C);
        cpu_write(&mut m, 0xD303, 0x3C);
        m.power_on();
        for addr in 0xD300..=0xD303 {
            assert_eq!(cpu_read(&mut m, addr), 0x00, "${addr:04X}");
        }
        assert_eq!(m.peek(0xE000), 0x20, "OS ROM on");
    }

    /// Verifies REQ-216: a power-on resets all chips: a used machine after power-on makes
    /// the same bus accesses as a new machine (criterion 3).
    #[test]
    fn power_on_resets_chips() {
        let code = [
            0xA0, 0x46, // LDY #$46
            0xA2, 0x00, // LDX #$00
            0xCA, // DEX
            0xD0, 0xFD, // BNE $C204
            0x88, // DEY
            0xD0, 0xF8, // BNE $C202
            0xA9, 0xC0, // LDA #$C0
            0x8D, 0x0E, 0xD4, // STA NMIEN
            0xA9, 0x22, // LDA #$22
            0x8D, 0x00, 0xD4, // STA DMACTL
            0xA9, 0xFF, // LDA #$FF
            0xA2, 0x1F, // LDX #$1F
            0x9D, 0x00, 0xD0, // STA $D000,X
            0xCA, // DEX
            0x10, 0xFA, // BPL $C218
            0xA2, 0x0F, // LDX #$0F
            0x9D, 0x00, 0xD2, // STA $D200,X
            0xCA, // DEX
            0x10, 0xFA, // BPL $C220
            0x4C, 0x26, 0xC2, // JMP $C226
        ];
        let mut a = program_machine(&code);
        let mut b = program_machine(&code);
        for _ in 0..10 {
            b.run_frame();
        }
        b.power_on();
        let log_a = log_cycles(&mut a, 2 * 29_868);
        let log_b = log_cycles(&mut b, 2 * 29_868);
        assert!(log_a == log_b, "the same bus accesses in the same cycles");
    }

    /// Verifies REQ-217: the Reset button starts the reset sequence (criterion 1).
    #[test]
    fn reset_button_runs_reset_sequence() {
        let mut m = program_machine(&[0x4C, 0x00, 0x20]); // JMP $2000
        for addr in 0x2000..0x2010 {
            m.poke_ram(addr, 0xEA); // NOP
        }
        for (addr, byte) in (0x2010_u16..).zip([0x4C, 0x00, 0x20]) {
            m.poke_ram(addr, byte);
        }
        let _warmup = log_cycles(&mut m, 200);
        assert!(m.cpu().registers().pc < 0x2020, "running the NOPs in RAM");
        m.press_reset();
        let log = accesses(&log_cycles(&mut m, 20));
        assert!(log[..7].iter().all(|access| !access.write), "7 reads");
        assert_eq!((log[7].addr, log[7].write), (0xC200, false), "opcode fetch");
    }

    /// Verifies REQ-217: the Reset button resets the PIA and the memory map (criterion 2).
    #[test]
    fn reset_button_resets_pia() {
        let mut m = machine();
        m.poke_ram(0xA000, 0x99);
        set_ddrb_orb(&mut m, 0xFF, 0xFC);
        cpu_write(&mut m, 0xD302, 0x3C);
        cpu_write(&mut m, 0xD303, 0x3C);
        assert_eq!(m.peek(0xA000), 0xBB, "BASIC on before");
        m.press_reset();
        for addr in 0xD300..=0xD303 {
            assert_eq!(cpu_read(&mut m, addr), 0x00, "${addr:04X}");
        }
        assert_eq!(cpu_read(&mut m, 0xC000), 0x00, "OS image byte $0000");
        assert_eq!(cpu_read(&mut m, 0xA000), 0x99, "RAM at $A000");
    }

    /// Verifies REQ-217: the Reset button resets ANTIC: no NMI, and the same DMA as a new
    /// machine (criterion 3).
    #[test]
    fn reset_button_resets_antic() {
        let code = [0x4C, 0x00, 0xC2]; // JMP $C200
        let mut m = program_machine(&code);
        cpu_write(&mut m, 0xD40E, 0x40); // NMIEN: VBI
        cpu_write(&mut m, 0xD400, 0x22); // DMACTL
        m.run_frame();
        m.run_frame();
        m.press_reset();
        let mut after = log_frame(&mut m);
        after.extend(log_frame(&mut m));
        assert!(
            accesses(&after).iter().all(|access| access.addr != 0xFFFA),
            "no NMI sequence in 2 frames"
        );
        let second = log_frame(&mut m);
        let mut new = program_machine(&code);
        let _first = log_frame(&mut new);
        let _second = log_frame(&mut new);
        let new_second = log_frame(&mut new);
        let halts = |log: &[CycleLog]| log.iter().map(|cycle| cycle.halt).collect::<Vec<_>>();
        assert_eq!(halts(&second), halts(&new_second), "the same halt cycles");
    }

    /// Verifies REQ-217: the Reset button keeps RAM, GTIA and POKEY (criterion 4).
    #[test]
    fn reset_button_keeps_ram_gtia_pokey() {
        let mut m = machine();
        for addr in 0..=0xFFFF_u16 {
            let [low, high] = addr.to_le_bytes();
            m.poke_ram(addr, low ^ high);
        }
        cpu_write(&mut m, 0xD01F, 0x08);
        cpu_write(&mut m, 0xD01A, 0x46);
        cpu_write(&mut m, 0xD20E, 0x40);
        cpu_write(&mut m, 0xD20F, 0x03);
        m.run_frame();
        let ram = m.mem.ram.clone();
        let gtia = m.gtia.clone();
        let pokey = m.pokey.clone();
        m.press_reset();
        assert!(m.mem.ram == ram, "RAM");
        assert_eq!(m.gtia, gtia, "GTIA");
        assert_eq!(m.pokey, pokey, "POKEY");
    }

    /// Verifies REQ-217: the Reset button sends no reset NMI: `NMIST` bit 5 stays 0
    /// (criterion 5).
    #[test]
    fn reset_button_no_nmi() {
        let mut m = machine();
        cpu_write(&mut m, 0xD40F, 0x00); // NMIRES
        assert_eq!(cpu_read(&mut m, 0xD40F) & 0x20, 0, "NMIST bit 5 before");
        m.press_reset();
        assert_eq!(cpu_read(&mut m, 0xD40F) & 0x20, 0, "NMIST bit 5 after");
    }

    /// Verifies REQ-221: the joysticks set the PIA port A pins (criterion 2) and the
    /// triggers.
    #[test]
    fn joystick_bits_on_port_a() {
        let mut m = machine();
        cpu_write(&mut m, 0xD302, 0x3C); // PACTL: select ORA
        let up = JoystickState {
            up: true,
            ..JoystickState::default()
        };
        let right = JoystickState {
            right: true,
            ..JoystickState::default()
        };
        for (port, state, expected) in [
            (0, up, 0xFE),
            (0, right, 0xF7),
            (1, up, 0xEF),
            (1, right, 0x7F),
        ] {
            m.set_joystick(port, state);
            assert_eq!(cpu_read(&mut m, 0xD300), expected, "port {port}: {state:?}");
            m.set_joystick(port, JoystickState::default());
        }
        let state = JoystickState {
            down: true,
            left: true,
            fire: true,
            ..JoystickState::default()
        };
        m.set_joystick(0, state);
        assert_eq!(cpu_read(&mut m, 0xD300), 0xF9, "down and left on stick 1");
        assert_eq!(cpu_read(&mut m, 0xD010), 0x00, "TRIG0 pressed");
        m.set_joystick(2, state);
        assert_eq!(cpu_read(&mut m, 0xD300), 0xF9, "no third port");
    }

    /// Verifies REQ-225: with no SIO device, CA1 and CB1 stay high: no status bit in 10
    /// frames (criterion 5).
    #[test]
    fn ca1_cb1_idle_high() {
        for ctl in [0x3C, 0x3E] {
            let mut m = machine();
            cpu_write(&mut m, 0xD302, ctl);
            cpu_write(&mut m, 0xD303, ctl);
            for _ in 0..10 {
                m.run_frame();
            }
            assert_eq!(m.peek(0xD302), ctl, "PACTL ${ctl:02X}");
            assert_eq!(m.peek(0xD303), ctl, "PBCTL ${ctl:02X}");
        }
    }

    /// Verifies REQ-229: the CPU IRQ line is the PIA IRQ or the POKEY IRQ (criterion 5, the
    /// cases without POKEY).
    #[test]
    fn irq_line_is_pia_or_pokey() {
        let mut m = machine();
        assert!(!m.pokey.irq() && !m.pia.irq(), "no IRQ source");
        assert!(!m.irq_line(), "neither");
        cpu_write(&mut m, 0xD302, 0x3D);
        m.pia.set_ca1(false);
        assert!(m.irq_line(), "only the PIA");
        cpu_write(&mut m, 0xD302, 0x3D);
        let _read = cpu_read(&mut m, 0xD300);
        m.run_cycle();
        assert!(!m.irq_line(), "PIA IRQ acknowledged");
    }

    /// Verifies REQ-229: the CPU IRQ line with a POKEY IRQ (criterion 5, the cases with
    /// POKEY).
    #[test]
    fn irq_line_with_pokey() {
        let mut m = machine();
        cpu_write(&mut m, 0xD20F, 0x03); // SKCTL: keyboard scan
        cpu_write(&mut m, 0xD20E, 0x40); // IRQEN: keyboard
        m.key_down(0x3F);
        m.run_frame();
        assert!(m.pokey.irq() && !m.pia.irq(), "only POKEY");
        assert!(m.irq_line(), "only POKEY");
        cpu_write(&mut m, 0xD302, 0x3D);
        m.pia.set_ca1(false);
        assert!(m.irq_line(), "both");
        let _read = cpu_read(&mut m, 0xD300);
        assert!(m.irq_line(), "only POKEY again");
        cpu_write(&mut m, 0xD20E, 0x00);
        assert!(!m.irq_line(), "neither");
    }

    /// Verifies REQ-232: `TRIG3` is 0 without a cartridge, also with BASIC on.
    #[test]
    fn trig3_zero_without_cartridge() {
        let mut m = machine();
        assert_eq!(cpu_read(&mut m, 0xD013), 0x00, "no cartridge");
        set_portb(&mut m, 0xFD);
        m.run_cycle();
        assert_eq!(cpu_read(&mut m, 0xD013), 0x00, "BASIC on");
    }

    /// Verifies REQ-232: `TRIG3` is 1 with an 8 KB or a 16 KB cartridge, after power-on and
    /// after the Reset button. `TRIG2` is 1.
    #[test]
    fn trig3_after_power_on_and_reset() {
        for cart in [cart_8k(), cart_16k()] {
            let mut m = machine_with(Some(cart));
            assert_eq!(cpu_read(&mut m, 0xD013), 0x01, "after power-on");
            m.run_frame();
            m.press_reset();
            assert_eq!(cpu_read(&mut m, 0xD013), 0x01, "after the Reset button");
            assert_eq!(cpu_read(&mut m, 0xD012), 0x01, "TRIG2");
        }
    }

    /// Verifies REQ-437: the console keys reach `CONSOL` through the machine.
    #[test]
    fn console_keys_reach_gtia() {
        let mut m = machine();
        m.set_console_keys(ConsoleKeys {
            option: true,
            ..ConsoleKeys::default()
        });
        assert_eq!(cpu_read(&mut m, 0xD01F), 0x0B, "OPTION pressed");
        cpu_write(&mut m, 0xD01F, 0x08);
        // REQ-437 criterion 2: after a write of $08, OPTION gives $03.
        assert_eq!(
            cpu_read(&mut m, 0xD01F),
            0x03,
            "OPTION pressed after a write of $08"
        );
        assert!(m.gtia.speaker(), "the speaker line follows CONSOL bit 3");
    }

    /// The frontend functions reach the chips.
    #[test]
    fn frontend_functions_reach_chips() {
        let mut m = machine();
        m.run_frame();
        assert_eq!(m.frame().len(), FRAME_WIDTH * FRAME_HEIGHT, "frame size");
        let mut audio = Vec::new();
        m.drain_audio(&mut audio);
        m.key_down(0x3F);
        m.set_shift(true);
        m.set_break(false);
        m.key_up();
        let mut expected = m.pokey.clone();
        expected.set_key(0x3F, true);
        expected.set_control(true);
        m.set_key(0x3F, true);
        m.set_control(true);
        assert_eq!(m.pokey, expected, "set_key and set_control reach POKEY");
        m.cpu_mut().registers_mut().a = 0x12;
        assert_eq!(m.cpu().registers().a, 0x12, "CPU registers");
    }

    /// Verifies REQ-438: the speaker line is high when `CONSOL` bit 3 was written as 0, and
    /// low when it was written as 1 (HRM 6.11).
    #[test]
    fn consol_bit_3_drives_speaker_line() {
        let mut m = machine();
        cpu_write(&mut m, 0xD01F, 0x08);
        assert!(!m.pokey.test_speaker(), "bit 3 = 1 pulls the line down");
        cpu_write(&mut m, 0xD01F, 0x00);
        assert!(m.pokey.test_speaker(), "bit 3 = 0 leaves the line high");
    }

    /// The scan line and cycle of the `BRK` opcode fetch, and the reads of the NMI vector
    /// and of the IRQ vector up to the end of frame 0, for a program that waits for scan line
    /// 246, runs `nops` `NOP` instructions (and one `BIT $80` if `odd`) and then a `BRK`.
    fn brk_near_vbi(nops: usize, odd: bool) -> ((u64, u64), usize, usize) {
        let mut code = vec![
            0x78, // SEI
            0xA9, 0x00, // LDA #$00
            0x8D, 0x00, 0xD4, // STA DMACTL
            0x8D, 0x0F, 0xD4, // STA NMIRES
            0xA9, 0x40, // LDA #$40
            0x8D, 0x0E, 0xD4, // STA NMIEN (VBI)
            0xAD, 0x0B, 0xD4, // wait: LDA VCOUNT
            0xC9, 0x7B, // CMP #123 (scan lines 246 and 247)
            0xD0, 0xF9, // BNE wait
            0x8D, 0x0A, 0xD4, // STA WSYNC
            0x8D, 0x0A, 0xD4, // STA WSYNC
        ];
        code.extend(iter::repeat_n(0xEA, nops));
        if odd {
            code.extend([0x24, 0x80]); // BIT $80
        }
        let brk = 0xC200 + u16::try_from(code.len()).unwrap();
        let [low, high] = (brk + 2).to_le_bytes();
        code.extend([0x00, 0x00, 0x4C, low, high]); // BRK, JMP *
        let mut m = program_machine(&code);
        let frame = 29_868;
        let mut brk_at = (0, 0);
        let mut vectors = (0, 0);
        while m.cycle_count() < frame {
            let pc = m.cpu().registers().pc;
            m.run_cycle();
            let cycle = (m.cycle_count() - 1) % frame;
            if pc == brk && m.cpu().registers().pc == brk + 1 {
                brk_at = (cycle / 114, cycle % 114);
            }
            match m.last_access {
                Some(BusAccess { addr: 0xFFFA, .. }) => vectors.0 += 1,
                Some(BusAccess { addr: 0xFFFE, .. }) => vectors.1 += 1,
                _ => {}
            }
        }
        (brk_at, vectors.0, vectors.1)
    }

    /// The NMI and IRQ vector reads for a `BRK` opcode fetch on `cycle` of scan line 248.
    fn brk_on_vbi_line(cycle: u64) -> (usize, usize) {
        let runs: Vec<_> = (0..20)
            .flat_map(|nops| [brk_near_vbi(nops, false), brk_near_vbi(nops, true)])
            .collect();
        let run = runs.iter().find(|run| run.0 == (248, cycle));
        assert!(
            run.is_some(),
            "no BRK on cycle {cycle} of line 248: {runs:?}"
        );
        run.map_or((0, 0), |run| (run.1, run.2))
    }

    /// Verifies REQ-335: a `BRK` opcode fetch on cycle 4 of scan line 248 loses the VBI NMI
    /// (the CPU rule of REQ-122 with the NMI level of OI-017); on cycle 3, the NMI runs after
    /// the `BRK` handler.
    #[test]
    fn brk_on_cycle_4_loses_vbi_nmi() {
        assert_eq!(brk_on_vbi_line(4), (0, 1), "cycle 4: no NMI vector read");
        assert_eq!(brk_on_vbi_line(3), (1, 1), "cycle 3: NMI after the BRK");
    }

    /// Verifies REQ-113: a KIL opcode locks the CPU, but the machine keeps running: each
    /// `run_frame` advances the cycle counter by one frame.
    #[test]
    fn kil_machine_keeps_running() {
        let mut m = program_machine(&[0x02]);
        m.run_frame();
        assert!(
            m.cpu().is_jammed(),
            "KIL at the reset address locks the CPU"
        );
        for frame in 0..3 {
            let start = m.cycle_count();
            m.run_frame();
            assert_eq!(m.cycle_count() - start, 29_868, "frame {frame}");
            assert!(m.cpu().is_jammed(), "still locked after frame {frame}");
        }
    }

    /// Verifies REQ-347: on the first scan line of a `$1E` line with `DMACTL` = `$23` and
    /// `HSCROL` = 2, `STA WSYNC` writes on cycle 104 and the CPU waits on the opcode fetch of
    /// a `NOP` (`$EA`); the virtual fetch on cycle 107 loads `$EA` into line buffer byte 47.
    #[test]
    fn virtual_fetch_during_wsync_reads_cpu_byte() {
        let hit = (1..40).find_map(|nops| {
            let m = wsync_on_wide_line(nops);
            (m.cycle_count() > 0).then(|| m.antic.debug_state().line_buffer[47])
        });
        assert_eq!(hit, Some(0xEA), "line buffer byte 47 after cycle 107");
    }

    /// Runs a program with a `STA WSYNC`, `nops` `NOP`s, `STA WSYNC` and `NOP` loop on a
    /// display of `$1E` lines (`DMACTL` = `$23`, `HSCROL` = 2) for two frames, until a `STA
    /// WSYNC` writes on cycle 104 of a `$1E` line. Then it runs to the end of cycle 107 of
    /// that scan line. Returns a machine with no cycles if no write is on cycle 104.
    fn wsync_on_wide_line(nops: usize) -> Machine {
        let mut code = vec![
            0xA9, 0x00, // LDA #$00
            0x8D, 0x02, 0xD4, // STA DLISTL
            0xA9, 0x30, // LDA #$30
            0x8D, 0x03, 0xD4, // STA DLISTH
            0xA9, 0x02, // LDA #$02
            0x8D, 0x04, 0xD4, // STA HSCROL
            0xA9, 0x23, // LDA #$23
            0x8D, 0x00, 0xD4, // STA DMACTL
            0x8D, 0x0A, 0xD4, // loop: STA WSYNC
        ];
        code.extend(iter::repeat_n(0xEA, nops));
        code.extend([0x8D, 0x0A, 0xD4, 0xEA, 0x4C, 0x14, 0xC2]); // STA WSYNC, NOP, JMP loop
        let mut m = program_machine(&code);
        let mut list = vec![0x5E, 0x00, 0x40];
        list.extend([0x1E; 238]);
        list.extend([0x41, 0x00, 0x30]);
        for (addr, byte) in (0x3000_u16..).zip(list) {
            m.poke_ram(addr, byte);
        }
        for _ in 0..2 * 29_868 {
            m.run_cycle();
            let write = matches!(
                m.last_access,
                Some(BusAccess {
                    addr: 0xD40A,
                    write: true,
                    ..
                })
            );
            if write && m.last_out.cycle == 104 && (9..247).contains(&m.last_out.line) {
                for _ in 105..=107 {
                    m.run_cycle();
                }
                return m;
            }
        }
        program_machine(&[])
    }
}
