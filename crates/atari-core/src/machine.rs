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

use crate::antic::{Antic, DmaKind, DmaMemory};
use crate::cpu::{Bus, Cpu};
use crate::gtia::{ConsoleKeys, Gtia};
use crate::media::Cartridge;
use crate::pia::Pia;
use crate::pokey::Pokey;
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
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.data = value;
        let [reg, page] = addr.to_le_bytes();
        match page {
            0xD0 => {
                self.gtia.write(reg, value);
                // The speaker line is high when CONSOL bit 3 was written as 0.
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
    /// The TV standard.
    standard: VideoStandard,
    /// The last value on the data bus (the floating bus value for an idle cycle).
    data_bus: u8,
    /// Machine cycles since power-on.
    cycles: u64,
    /// Frames completed since power-on.
    frames: u64,
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
            standard: config.standard,
            data_bus: 0xFF,
            cycles: 0,
            frames: 0,
        };
        machine.power_on();
        Ok(machine)
    }

    /// Power off and on (REQ-215, REQ-216): RAM is `$00`, all chips are reset, and the CPU
    /// starts its reset sequence.
    pub fn power_on(&mut self) {
        self.mem.ram.fill(0);
        self.antic.reset();
        self.gtia.reset();
        self.pokey.reset();
        self.pia.reset();
        self.mem.portb = self.pia.port_b_pins();
        if let Some(cart) = self.mem.cart.as_mut() {
            cart.cold_reset();
        }
        self.update_trig3();
        self.cpu = Cpu::new();
    }

    /// The System Reset button (REQ-217): resets the CPU, ANTIC and the PIA.
    pub const fn press_reset(&mut self) {
        self.cpu.reset();
        self.antic.reset();
        self.pia.reset();
        self.mem.portb = self.pia.port_b_pins();
    }

    /// Runs one machine cycle, in the order of `docs/design/system.md`, "Timing".
    pub fn run_cycle(&mut self) {
        let out = self.antic.tick(&mut self.mem);
        // The SIO bus is not in SLICE-01: the DATA IN line is idle (high).
        self.pokey.set_serial_in(true);
        self.pokey.tick();
        self.cpu.set_nmi(out.nmi);
        self.cpu.set_irq(self.pokey.irq() || self.pia.irq());
        let mut data = self.data_bus;
        if out.halt {
            if out.dma != DmaKind::Refresh {
                data = out.dma_data;
            }
        } else {
            let mut bus = CpuBus {
                mem: &mut self.mem,
                antic: &mut self.antic,
                gtia: &mut self.gtia,
                pokey: &mut self.pokey,
                pia: &mut self.pia,
                data,
            };
            let _tick = self.cpu.tick(&mut bus, out.rdy);
            data = bus.data;
        }
        self.data_bus = data;
        self.antic.end_cycle(data);
        self.gtia.clock(&out, data);
        self.pia.tick();
        self.update_trig3();
        self.cycles += 1;
        if out.frame_done {
            self.frames += 1;
        }
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

    /// Moves all audio samples produced since the last call into `out`.
    pub const fn drain_audio(&mut self, out: &mut Vec<f32>) {
        self.pokey.drain_samples(out);
    }
}

#[cfg(test)]
mod tests {
    use super::{CpuBus, Machine, MachineConfig};
    use crate::antic::DmaMemory as _;
    use crate::cpu::Bus as _;
    use crate::gtia::ConsoleKeys;
    use crate::media::Cartridge;
    use crate::timing::VideoStandard;

    /// The OS ROM of the tests: byte `i` is `i >> 8` (the page number in the ROM).
    fn os_rom() -> Vec<u8> {
        (0..super::OS_ROM_SIZE).map(|i| u8::try_from(i >> 8).unwrap()).collect()
    }

    /// A machine with the test OS ROM, BASIC of `$BB` bytes, and an optional cartridge.
    fn machine_with(cartridge: Option<Cartridge>) -> Machine {
        Machine::new(MachineConfig {
            os_rom: os_rom(),
            basic_rom: Some(vec![0xBB; super::BASIC_ROM_SIZE]),
            cartridge,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: 48_000,
        })
        .unwrap()
    }

    /// A machine with the test OS ROM and BASIC.
    fn machine() -> Machine {
        machine_with(None)
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

    /// Verifies REQ-201: one frame is 29,868 cycles (NTSC) or 35,568 cycles (PAL).
    #[test]
    fn frame_length_matches_standard() {
        for (standard, cycles) in [(VideoStandard::Ntsc, 29_868), (VideoStandard::Pal, 35_568)] {
            let mut m = Machine::new(MachineConfig {
                os_rom: os_rom(),
                basic_rom: None,
                cartridge: None,
                standard,
                audio_sample_rate: 48_000,
            })
            .unwrap();
            m.run_frame();
            let start = m.cycle_count();
            m.run_frame();
            assert_eq!(m.cycle_count() - start, cycles, "{standard:?} frame length");
            assert_eq!(m.standard(), standard, "standard");
        }
    }

    /// Verifies REQ-204: with all ROMs off, each address outside `$D000-$D7FF` is RAM.
    #[test]
    fn all_ram_with_roms_off() {
        let mut m = machine();
        set_portb(&mut m, 0x82); // OS off, BASIC off, self-test off
        for addr in (0x0000..=0xFFFF_u16).step_by(0x100) {
            if (0xD000..=0xD7FF).contains(&addr) {
                continue;
            }
            cpu_write(&mut m, addr, 0x5A);
            assert_eq!(cpu_read(&mut m, addr), 0x5A, "RAM at ${addr:04X}");
        }
    }

    /// Verifies REQ-205: the I/O pages reach the chips and unused pages read `$FF`.
    #[test]
    fn io_pages_reach_chips() {
        let mut m = machine();
        assert_eq!(cpu_read(&mut m, 0xD014), 0x0F, "GTIA PAL register (NTSC)");
        assert_eq!(cpu_read(&mut m, 0xD0F4), 0x0F, "GTIA mirror");
        assert_eq!(cpu_read(&mut m, 0xD20F) & 0x04, 0x04, "POKEY SKSTAT: no key");
        assert_eq!(cpu_read(&mut m, 0xD40B), m.peek(0xD40B), "ANTIC VCOUNT");
        for addr in [0xD100, 0xD5FF, 0xD600, 0xD7FF] {
            assert_eq!(cpu_read(&mut m, addr), 0xFF, "unused I/O ${addr:04X}");
        }
    }

    /// Verifies REQ-206: a write to a ROM address does not change the RAM under it.
    #[test]
    fn write_to_rom_keeps_ram() {
        let mut m = machine_with(Some(Cartridge::from_raw(vec![0xCC; 0x2000]).unwrap()));
        for addr in [0xE000, 0xB000] {
            m.poke_ram(addr, 0xAA);
            cpu_write(&mut m, addr, 0x55);
            assert_eq!(m.peek_ram(addr), 0xAA, "RAM under ROM at ${addr:04X}");
        }
        assert_eq!(m.peek(0xB000), 0xCC, "the cartridge shows at $B000");
    }

    /// Verifies REQ-207 and REQ-211: `PORTB` bit 0 switches the OS ROM.
    #[test]
    fn portb_bit0_switches_os_rom() {
        let mut m = machine();
        m.poke_ram(0xE000, 0xAA);
        assert_eq!(m.peek(0xE000), 0x20, "OS ROM after power-on (pins $FF)");
        set_portb(&mut m, 0xFE);
        assert_eq!(m.peek(0xE000), 0xAA, "RAM with OS ROM off");
        assert_eq!(m.peek(0xC000), m.peek_ram(0xC000), "RAM at $C000 with OS ROM off");
    }

    /// Verifies REQ-208: `PORTB` bit 1 = 0 maps BASIC at `$A000-$BFFF`.
    #[test]
    fn portb_bit1_switches_basic() {
        let mut m = machine();
        assert_eq!(m.peek(0xA000), 0x00, "BASIC is off after power-on (pins $FF)");
        set_portb(&mut m, 0xFD);
        assert_eq!(m.peek(0xA000), 0xBB, "BASIC on");
        assert_eq!(m.peek(0xBFFF), 0xBB, "BASIC end");
    }

    /// Verifies REQ-209: `PORTB` bit 7 = 0 maps the self-test ROM, only with the OS ROM on.
    #[test]
    fn portb_bit7_switches_self_test() {
        let mut m = machine();
        set_portb(&mut m, 0x7F);
        assert_eq!(m.peek(0x5000), 0x10, "self-test ROM = OS ROM offset $1000");
        set_portb(&mut m, 0x7E);
        assert_eq!(m.peek(0x5000), 0x00, "no self-test ROM without the OS ROM");
    }

    /// Verifies REQ-210: cartridge before BASIC before RAM.
    #[test]
    fn cartridge_before_basic() {
        let mut m = machine_with(Some(Cartridge::from_raw(vec![0xCC; 0x4000]).unwrap()));
        set_portb(&mut m, 0xFD);
        assert_eq!(m.peek(0xA000), 0xCC, "cartridge before BASIC");
        assert_eq!(m.peek(0x8000), 0xCC, "16 KB cartridge at $8000 (RD4)");
        let mut plain = machine();
        set_portb(&mut plain, 0xFF);
        cpu_write(&mut plain, 0xA000, 0xC3);
        assert_eq!(plain.peek(0xA000), 0xC3, "RAM with no cartridge and BASIC off");
    }

    /// Verifies REQ-213: ANTIC DMA reads use the CPU memory map.
    #[test]
    fn dma_map_equals_cpu_map() {
        let mut m = machine();
        for portb in [0xFF, 0xFE, 0xFD, 0x7F] {
            set_portb(&mut m, portb);
            for addr in (0x0000..=0xFFFF_u16).step_by(0x80) {
                if (0xD000..=0xD7FF).contains(&addr) {
                    continue;
                }
                let cpu = m.peek(addr);
                assert_eq!(m.mem.dma_read(addr), cpu, "PORTB ${portb:02X} at ${addr:04X}");
            }
        }
        assert_eq!(m.mem.dma_read(0xD500), 0xFF, "DMA read of cartridge control");
        assert_eq!(m.mem.dma_read(0xD40B), 0xFF, "DMA read of other I/O");
    }

    /// Verifies REQ-215 and REQ-216: power-on clears RAM and resets the PIA.
    #[test]
    fn power_on_clears_ram_and_pia() {
        let mut m = machine();
        m.poke_ram(0x1234, 0x99);
        set_portb(&mut m, 0xFE);
        m.power_on();
        assert_eq!(m.peek_ram(0x1234), 0x00, "RAM is $00");
        assert_eq!(m.peek(0xE000), 0x20, "OS ROM on again (PIA reset)");
        assert_eq!(m.frame_count(), 0, "no frame yet");
    }

    /// Verifies REQ-217: the Reset button resets the PIA, so the OS ROM is on again.
    #[test]
    fn reset_button_resets_pia() {
        let mut m = machine();
        m.poke_ram(0x1234, 0x99);
        set_portb(&mut m, 0xFE);
        m.press_reset();
        assert_eq!(m.peek(0xE000), 0x20, "OS ROM on");
        assert_eq!(m.peek_ram(0x1234), 0x99, "RAM is kept");
    }

    /// Verifies REQ-232: `TRIG3` is 1 with a cartridge and 0 without.
    #[test]
    fn trig3_follows_cartridge() {
        let mut with = machine_with(Some(Cartridge::from_raw(vec![0; 0x2000]).unwrap()));
        assert_eq!(cpu_read(&mut with, 0xD013), 0x01, "cartridge present");
        let mut without = machine();
        assert_eq!(cpu_read(&mut without, 0xD013), 0x00, "no cartridge");
    }

    /// Verifies REQ-221: the joystick sets the PIA port A pins and the trigger.
    #[test]
    fn joystick_reaches_pia_and_trigger() {
        let mut m = machine();
        let state = super::JoystickState { up: true, left: true, fire: true, ..Default::default() };
        m.set_joystick(0, state);
        cpu_write(&mut m, 0xD302, 0x04); // PACTL: select ORA
        assert_eq!(cpu_read(&mut m, 0xD300), 0xFA, "up and left pressed on stick 1");
        assert_eq!(cpu_read(&mut m, 0xD010), 0x00, "TRIG0 pressed");
        m.set_joystick(1, super::JoystickState { down: true, right: true, ..Default::default() });
        assert_eq!(cpu_read(&mut m, 0xD300), 0x5A, "stick 2 down and right");
        m.set_joystick(2, super::JoystickState::default());
        assert_eq!(cpu_read(&mut m, 0xD300), 0x5A, "no third port");
    }

    /// Verifies REQ-437: the console keys reach `CONSOL` through the machine.
    #[test]
    fn console_keys_reach_gtia() {
        let mut m = machine();
        m.set_console_keys(ConsoleKeys { option: true, ..ConsoleKeys::default() });
        assert_eq!(cpu_read(&mut m, 0xD01F), 0x0B, "OPTION pressed");
        cpu_write(&mut m, 0xD01F, 0x08);
        assert_eq!(cpu_read(&mut m, 0xD01F), 0x0B, "speaker bit does not change the keys");
    }
}
