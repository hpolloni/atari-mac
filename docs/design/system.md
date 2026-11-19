# System design

## Scope

This document tells how the emulator satisfies the system requirements REQ-200 to REQ-232:
clocks, the memory bus, the memory map, bank switching with `PORTB`, reset, the PIA and the
cartridge sense line (HRM chapter 2, except 2.7). Extended memory, PBI devices and the other
models than the 65XE are out of scope.

| Requirement | Topic | Module |
| --- | --- | --- |
| REQ-200, REQ-201 | Clocks and frame length | `timing.rs` |
| REQ-202, REQ-203 | Bus masters and access side effects | `machine.rs` |
| REQ-204 to REQ-214 | Memory map and bank switching | `machine.rs` (`MemoryMap`) |
| REQ-215 to REQ-217 | Power-on, reset button | `machine.rs` |
| REQ-218 to REQ-231 | PIA | `pia.rs` |
| REQ-232 | `TRIG3` cartridge sense | `machine.rs` |

The CPU side of the bus is in [cpu.md](cpu.md).

## Interfaces

### timing.rs

```rust
/// Machine cycles in one scan line (NTSC and PAL).
pub const CYCLES_PER_LINE: u32 = 114;
/// Color clocks in one scan line (2 per machine cycle).
pub const COLOR_CLOCKS_PER_LINE: u32 = CYCLES_PER_LINE * 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoStandard { #[default] Ntsc, Pal }

impl VideoStandard {
    /// 262 (NTSC) or 312 (PAL).
    pub const fn lines_per_frame(self) -> u32;
    /// lines_per_frame * CYCLES_PER_LINE: 29,868 or 35,568.
    pub const fn cycles_per_frame(self) -> u32;
    /// 1,789,772.5 Hz (NTSC, 14.31818 MHz / 8) or 1,773,447.0 Hz (PAL, HRM 3.3 value).
    pub const fn cpu_clock_hz(self) -> f64;
    /// cpu_clock_hz / cycles_per_frame.
    pub fn frames_per_second(self) -> f64;
}
```

### pia.rs

```rust
pub struct Pia { /* see State */ }

impl Pia {
    /// The PIA after power-on: the same state as after `reset`.
    pub const fn new() -> Self;
    /// Clears all registers to $00 (REQ-230).
    pub fn reset(&mut self);
    /// CPU read. `reg` is the low address byte; the PIA uses `reg & 3` (REQ-218).
    /// Has side effects: clears interrupt status, starts CA2 pulses (REQ-227, REQ-228).
    pub fn read(&mut self, reg: u8) -> u8;
    /// Read without side effects, for debuggers.
    pub fn peek(&self, reg: u8) -> u8;
    /// CPU write. `reg` is the low address byte.
    pub fn write(&mut self, reg: u8, value: u8);
    /// Advances one machine cycle: pulse ends and the floating bit delay (REQ-223, REQ-227).
    pub fn tick(&mut self);
    /// External port A pin levels. Bit = 0: pulled low (pressed joystick direction).
    pub fn set_port_a_input(&mut self, pins: u8);
    /// Level of the SIO proceed line (CA1) and the SIO interrupt line (CB1). true = high.
    pub fn set_ca1(&mut self, high: bool);
    pub fn set_cb1(&mut self, high: bool);
    /// External level on CA2 and CB2 in input mode. true = high (the default).
    pub fn set_ca2_input(&mut self, high: bool);
    pub fn set_cb2_input(&mut self, high: bool);
    /// Level of the SIO motor line (CA2) and the SIO command line (CB2). true = high.
    pub const fn ca2(&self) -> bool;
    pub const fn cb2(&self) -> bool;
    /// Port B pin levels for the memory map (REQ-211).
    pub const fn port_b_pins(&self) -> u8;
    /// true while the PIA asserts IRQ (REQ-229).
    pub const fn irq(&self) -> bool;
}
```

Call contract: the machine calls `read` and `write` only from the CPU bus, once for each CPU
access to `$D3xx`, dummy accesses included (REQ-203). It calls `tick` once in each machine
cycle, after the CPU cycle. After each `write`, it reads `port_b_pins` again (REQ-212).

### Cartridge port (contract that the media design must give)

The `media::Cartridge` type is in the REQ-600 range. The system needs these functions:

```rust
impl Cartridge {
    /// CPU read of $8000-$BFFF. None if the cartridge does not drive the bus there.
    pub fn read(&mut self, addr: u16) -> Option<u8>;
    /// The same value without side effects, for ANTIC DMA and `peek`.
    pub fn peek(&self, addr: u16) -> Option<u8>;
    /// CPU write to $8000-$BFFF while the cartridge shows ROM there.
    pub fn write(&mut self, addr: u16, value: u8);
    /// CPU access to $D5xx. Returns Some only for a read that the cartridge drives.
    pub fn access_control(&mut self, addr: u16, write: Option<u8>) -> Option<u8>;
    /// true while the cartridge shows ROM at $8000-$9FFF (RD4) or $A000-$BFFF (RD5).
    pub fn rd4(&self) -> bool;
    pub fn rd5(&self) -> bool;
}
```

### ANTIC

The machine uses the ANTIC interface of `antic.md` ("Functions", "Call order", "Interface
with GTIA"), decided in OI-001: `Antic::tick(&mut self, mem: &mut impl DmaMemory) ->
CycleOutput`, then the CPU access, then `Antic::end_cycle(data_bus)` and
`Gtia::clock(&out, data_bus)`. `CycleOutput` gives `halt`, `rdy`, `nmi`, `frame_done`,
`line` and `cycle`.

```rust
pub trait DmaMemory {
    /// A DMA read with the CPU memory map (REQ-213). The only side effect: a read of
    /// `$D500-$D5FF` calls `Cartridge::access_control` (REQ-609, OI-002).
    fn dma_read(&mut self, addr: u16) -> u8;
}
```

### machine.rs

```rust
pub struct MachineConfig {
    /// 16,384 bytes: AltirraOS or an Atari XL/XE OS ROM.
    pub os_rom: Vec<u8>,
    /// 8,192 bytes, or None.
    pub basic_rom: Option<Vec<u8>>,
    pub cartridge: Option<Cartridge>,
    pub standard: VideoStandard,
    pub audio_sample_rate: u32,
}

impl Machine {
    /// Checks the ROM sizes, then does `power_on`.
    pub fn new(config: MachineConfig) -> Result<Self, ConfigError>;
    /// Power off and on (REQ-215, REQ-216).
    pub fn power_on(&mut self);
    /// The System Reset button (REQ-217).
    pub fn press_reset(&mut self);
    /// Runs one machine cycle (see Timing).
    pub fn run_cycle(&mut self);
    /// Runs machine cycles until ANTIC ends the frame.
    pub fn run_frame(&mut self);
    /// Machine cycles since power-on.
    pub const fn cycle_count(&self) -> u64;
    /// Memory as the CPU sees it, without side effects. Unmapped I/O reads as $FF.
    pub fn peek(&self, addr: u16) -> u8;
    /// RAM under any ROM.
    pub fn peek_ram(&self, addr: u16) -> u8;
    pub fn poke_ram(&mut self, addr: u16, value: u8);
}
```

The draft names `cold_reset`, `reset_key` and `step` change to `power_on`, `press_reset` and
`run_cycle`. The draft input functions (`set_joystick`, `key_down` and others) do not change.

## State

### Machine

| Field | Size | Power-on value |
| --- | --- | --- |
| `ram` | 65,536 bytes | all `$00` (REQ-215) |
| `os` | 16,384 bytes | from the configuration |
| `basic` | 8,192 bytes or none | from the configuration |
| `portb_pins` | `u8` | `$83` after the PIA reset: bits 0, 1 and 7 are pulled up (HRM 2.6), bits 2-6 float (REQ-222) |
| `cycles` | `u64` | 0 |
| `line`, `line_cycle` | `u32`, `u32` | 0, 0 (owned by ANTIC; the machine reads them) |

### PIA

| Field | Size | Reset value | Purpose |
| --- | --- | --- | --- |
| `ddra`, `ddrb` | `u8` | `$00` | Data direction. 1 = output. |
| `ora`, `orb` | `u8` | `$00` | Output registers. |
| `pactl`, `pbctl` | `u8` (bits 0-5) | `$00` | Control bits. |
| `irqa1`, `irqa2`, `irqb1`, `irqb2` | `bool` | false | Interrupt status (bits 7 and 6). |
| `port_a_in` | `u8` | `$FF` (not reset) | External port A pins. |
| `ca1`, `cb1`, `ca2_in`, `cb2_in` | `bool` | true (not reset) | External control line levels. |
| `ca2_low`, `cb2_low` | `bool` | false | Output low because of a pulse mode. |
| `cb2_rose` | `bool` | false | CB2 output went from `110` to `111` (REQ-231). |
| `ca2_pulse`, `cb2_pulse` | `u8` | 0 | Cycles left of a one-cycle pulse output (REQ-227). A write that changes the mode bits clears it (project decision). |
| `float_high` | `u8` | `$00` | Port B bits 2-6 that still read 1. |
| `float_timer` | `[u32; 8]` | 0 | Cycles left until each floating bit reads 0. |

## Behavior

### Clocks (REQ-200, REQ-201)

One machine cycle is the unit of time in the core. All chips run once in each machine cycle.
The color clock is not a separate counter. GTIA uses 2 color clocks for each machine cycle
(HRM 2.2). The video standard comes from `MachineConfig` and does not change while the
machine runs.

### Memory bus (REQ-202, REQ-203)

In each machine cycle, `CycleOutput::halt` decides the bus master. If `halt` is true, ANTIC
has the bus and the CPU does not tick. Otherwise the CPU makes one access. ANTIC never
writes (HRM 2.3). The CPU bus sends each access to the device of its address, with all side
effects, also for dummy accesses and repeated RDY reads.

### Address decode (REQ-204 to REQ-210, REQ-214)

The decode uses only the address, `portb_pins` and the cartridge RD4/RD5 state (HRM 2.6).
`os_on` = pin 0, `basic_on` = not pin 1, `self_test_on` = `os_on` and not pin 7.

| Address | Read source, in order of priority |
| --- | --- |
| `$0000-$4FFF` | RAM |
| `$5000-$57FF` | OS ROM offset `addr - $4000` if `self_test_on`, else RAM |
| `$5800-$7FFF` | RAM |
| `$8000-$9FFF` | cartridge if RD4, else RAM |
| `$A000-$BFFF` | cartridge if RD5, else BASIC offset `addr - $A000` if `basic_on` and BASIC is present, else RAM |
| `$C000-$CFFF` | OS ROM offset `addr - $C000` if `os_on`, else RAM |
| `$D000-$D0FF` | GTIA |
| `$D100-$D1FF` | `$FF` |
| `$D200-$D2FF` | POKEY |
| `$D300-$D3FF` | PIA |
| `$D400-$D4FF` | ANTIC |
| `$D500-$D5FF` | cartridge `access_control`, or `$FF` if it returns None |
| `$D600-$D7FF` | `$FF` |
| `$D800-$FFFF` | OS ROM offset `addr - $C000` if `os_on`, else RAM |

The self-test offset maps `$5000` to OS ROM offset `$1000`, which is the ROM content of
`$D000` (HRM 14.5). A write goes to the same source as a read. A write to a ROM source is
ignored, and RAM does not change (REQ-206). A write to the cartridge area goes to
`Cartridge::write` only.

### Bank switching (REQ-211, REQ-212)

`Pia::port_b_pins` returns `(orb & ddrb) | (!ddrb & $83)`. Bits 0, 1 and 7 are 1 for inputs
because of the pull-ups (HRM 2.6). Bits 2-6 are not used by the memory map. After each PIA
write, the machine copies `port_b_pins` to `portb_pins`. The next bus access, by the CPU or
by ANTIC, uses the new map.

### ANTIC DMA (REQ-213)

`Machine` implements `DmaMemory` with the same decode as the CPU. A DMA read of
`$8000-$BFFF` uses `Cartridge::read`. A DMA read of `$D500-$D5FF` calls
`Cartridge::access_control` (REQ-609, OI-002), then updates `TRIG3` from `rd5()`. A DMA
read of the other I/O pages in `$D000-$D7FF` has no side effect and gives `$FF`.

### Power-on (REQ-215, REQ-216)

`power_on` fills RAM with `$00`, replaces ANTIC with `Antic::new(standard)` (the RESET line
does not move the beam position, but a power-on starts at line 0, cycle 0, REQ-303), then
calls `reset` on ANTIC, GTIA, POKEY and the PIA, and `Cpu::reset`. The CPU registers get the power-on values of REQ-128. The CPU reset sequence
runs in the first 7 CPU cycles.

### Reset button (REQ-217)

`press_reset` calls `Cpu::reset`, `Antic::reset` and `Pia::reset`. It then copies
`port_b_pins` to `portb_pins`. It does not change RAM, GTIA or POKEY. ANTIC sends no reset
NMI on the XL/XE (HRM 2.4), so `press_reset` does not touch `NMIST`.

### PIA registers (REQ-218 to REQ-224)

| `reg & 3` | Read | Write |
| --- | --- | --- |
| 0 | `PACTL` bit 2 = 0: `ddra`. Bit 2 = 1: port A value (below), with side effects. | Bit 2 = 0: `ddra`. Bit 2 = 1: `ora`. |
| 1 | `PBCTL` bit 2 = 0: `ddrb`. Bit 2 = 1: port B value (below), with side effects. | Bit 2 = 0: `ddrb`. Bit 2 = 1: `orb`, and CB2 pulse modes. |
| 2 | `irqa1 << 7 \| irqa2 << 6 \| pactl` | `pactl = value & $3F`, with the mode change rules |
| 3 | `irqb1 << 7 \| irqb2 << 6 \| pbctl` | `pbctl = value & $3F`, with the mode change rules |

Port values:

- Port A: `(ora | !ddra) & port_a_in` (HRM 2.5). Port A has internal pull-ups, so
  `port_a_in` is `$FF` with no input.
- Port B: `(orb & ddrb) | (!ddrb & ($83 | float_high))` (HRM 2.5).

### Floating port B bits (REQ-223)

When a write to `ddrb` changes a bit in `$7C` from 1 to 0, and the `orb` bit is 1, the PIA
sets that bit in `float_high` and sets its timer to `FLOAT_DECAY_CYCLES`. If the `orb` bit
is 0, the bit is cleared at once. `tick` decrements the timers. At 0, the bit is cleared in
`float_high`. A write that makes the bit an output again also clears it.

`FLOAT_DECAY_CYCLES` = 536,932. This is 300 ms on NTSC and 303 ms on PAL, inside the HRM
range of 100-500 ms (project decision).

### Interrupt status and IRQ (REQ-225, REQ-226, REQ-228, REQ-229)

- `set_ca1` and `set_cb1` compare the new level with the old level. A change in the
  direction of control bit 1 (0 = high to low, 1 = low to high) sets `irqa1` or `irqb1`.
- In input mode (bit 5 = 0), `set_ca2_input` and `set_cb2_input` set `irqa2` or `irqb2` on a
  change in the direction of bit 4 (0 = high to low, 1 = low to high).
- A data register read (bit 2 = 1) clears both status bits of that port. A `DDR` read does
  not.
- A control write that sets bit 5 (output mode) clears `irqa2` or `irqb2`. A control write
  does not clear the status bits in other cases.
- `irq()` = `(irqa1 && pactl.0) || (irqa2 && pactl.5 == 0 && pactl.3) || (irqb1 && pbctl.0)
  || (irqb2 && pbctl.5 == 0 && pbctl.3)`.

The machine sets the CPU IRQ line to `pia.irq() || pokey.irq()` (REQ-229).

### CA2 and CB2 outputs (REQ-227)

| Bits 5-3 | CA2 (motor) | CB2 (command) |
| --- | --- | --- |
| `0xx` | Input. Level = external level, default high. | Same |
| `100` | Low after a port A data read, until the next active CA1 transition | Low after a port B data write, until the next active CB1 transition |
| `101` | Low for one cycle after a port A data read | Low for one cycle after a port B data write |
| `110` | Low | Low |
| `111` | High | High |

`ca2()` and `cb2()` give the levels to the SIO bus (REQ-600 range). In the `101` mode, the
access in cycle `c` sets `ca2_low` (or `cb2_low`). The `tick` of cycle `c` keeps it, and the
`tick` of cycle `c + 1` clears it. So the line is low at the end of cycle `c` and high at the
end of cycle `c + 1` (REQ-227).

### Spurious interrupts (REQ-231)

On a `PACTL` write: if the old bits 5-3 are `110` and the new bits are `010` or `011`, the
PIA sets `irqa2`. On a `PBCTL` write: a change from `110` to `111` sets `cb2_rose`. A change
to `110` clears it. If `cb2_rose` is true and the new bits 5-3 are `0xx`, the PIA sets
`irqb2` and clears `cb2_rose`.

### TRIG3 (REQ-232)

After each CPU access to `$8000-$BFFF` or `$D5xx`, and after `power_on` and `press_reset`,
the machine calls `gtia.set_trigger(3, !cart.rd5())`. `true` means "active" (`gtia.md`), so
`TRIG3` reads 1 while RD5 is active. With no cartridge the trigger is active (`TRIG3` = 0).
GTIA trigger 2 is never active (HRM 2.8).

## Timing

One call of `Machine::run_cycle` is one machine cycle `c`. The order is:

1. `let out = antic.tick(&mut memory)` gives the `CycleOutput` of cycle `c`. ANTIC DMA
   reads use the memory map as it was at the end of cycle `c - 1`.
2. `pokey.set_serial_in(sio.data_in_level(c))`, then `pokey.tick()`. POKEY runs before the
   CPU access, so a POKEY register access in cycle `c` sees the events of cycle `c`
   (`pokey.md`, call contract).
3. `cpu.set_irq(pokey.irq() || pia.irq())`. The POKEY level is the level of cycle `c`. The
   PIA level is the level from the end of cycle `c - 1`.
4. If `out.halt` is false: `cpu.tick(&mut bus, out.rdy)`. The bus decodes the address with
   `portb_pins`. A PIA write updates `portb_pins` at the end of the access. A GTIA write
   also calls `pokey.set_speaker(!gtia.speaker())`: a `CONSOL` bit written as 1 pulls its
   line down (HRM 6.11), so the speaker line is high when bit 3 was written as 0.
4a. `cpu.end_cycle(antic.nmi_level())`, also when `out.halt` is true. ANTIC computes the NMI
   level of cycle `c` after the CPU access, so a `NMIEN` write in cycle `c` is included
   (OI-017). The CPU latches the NMI edge and makes its interrupt poll for cycle `c` here.
5. The machine finds the data bus value of the cycle (`antic.md`, "Call order", step 3),
   then calls `antic.end_cycle(data_bus)` and `gtia.clock(&out, data_bus)`.
6. `pia.tick()`. Then each byte from `pokey.take_serial_byte()` goes to `sio.host_byte`. If
   the PIA CB2 level changed in this cycle, `sio.set_command_line(!pia.cb2(), c)`.
7. `cycles += 1`. If `out.frame_done` is true, the frame counter increments.

This is the only definition of the machine cycle order. The other design documents refer
to it.

A scan line has 114 machine cycles (0 to 113). A frame has 262 lines (NTSC) or 312 lines
(PAL). `run_frame` runs exactly `cycles_per_frame` cycles when it starts at the first cycle
of a frame.

## Test plan

The `machine.rs` unit tests use test OS images (16,384 bytes with a reset vector and a small
program) and test cartridges that implement the cartridge contract above. A test cartridge
can drive `$8000-$9FFF`, `$A000-$BFFF` and `$D5xx`, switch RD4 and RD5 on an access, and
count its accesses. "PORTB pins = `$xx`" in the requirements means that the test sets `DDRB`
= `$FF` and the port B output register to `$xx`.

| Requirement | Test |
| --- | --- |
| REQ-200 | `crates/atari-core/src/timing.rs` unit tests `machine_clock_values`, `color_clocks_per_line`; `machine.rs` unit test `machine_uses_configured_standard` |
| REQ-201 | `timing.rs` unit test `frame_rates_match_the_hardware`; `machine.rs` unit tests `scan_line_is_114_cycles`, `ntsc_frame_is_29868_cycles`, `pal_frame_is_35568_cycles` |
| REQ-202 | `machine.rs` unit tests `dma_cycles_halt_cpu`, `free_cycles_run_cpu`, `antic_never_writes` |
| REQ-203 | `machine.rs` unit tests `dummy_read_clears_pia_status`, `rmw_writes_portb_twice`, `dummy_read_reaches_cartridge_control`, `rdy_reads_reach_cartridge_control`; `machine.rs` unit test `bus_reads_have_side_effects` |
| REQ-204 | `machine.rs` unit tests `all_ram_visible_with_roms_off`, `low_ram_always_visible` |
| REQ-205 | `machine.rs` unit tests `io_area_cannot_be_disabled`, `io_write_does_not_change_ram`, `io_pages_reach_chips` |
| REQ-206 | `machine.rs` unit tests `write_to_os_rom_ignored`, `write_to_basic_ignored`, `write_to_cartridge_rom_ignored`, `write_to_self_test_ignored` |
| REQ-207 | `machine.rs` unit tests `os_rom_mapping`, `os_rom_hidden_under_io`, `os_rom_disabled_shows_ram`, `os_rom_size_checked` |
| REQ-208 | `machine.rs` unit tests `basic_enabled_by_bit_1_low`, `basic_disabled_shows_ram`, `no_basic_image_shows_ram` |
| REQ-209 | `machine.rs` unit tests `self_test_rom_mapping`, `self_test_needs_os_rom`, `self_test_disabled_by_bit_7` |
| REQ-210 | `machine.rs` unit tests `cartridge_before_basic`, `basic_after_cartridge_disabled`, `cartridge_rd4_before_ram` |
| REQ-211 | `machine.rs` unit tests `reset_memory_map`, `portb_output_selects_map`, `portb_inputs_pulled_up`, `ddrb_write_changes_map`, `mixed_inputs_and_outputs_select_map` |
| REQ-212 | `machine.rs` unit tests `bank_switch_next_cpu_access`, `ddrb_switch_next_cpu_access`, `bank_switch_next_dma_access` |
| REQ-213 | `machine.rs` unit tests `dma_reads_os_rom`, `dma_reads_ram_under_os`, `dma_map_equals_cpu_map`, `dma_read_no_side_effects` |
| REQ-214 | `machine.rs` unit tests `unmapped_io_reads_ff`, `cartridge_control_without_cartridge_reads_ff`, `undriven_cartridge_control_reads_ff` |
| REQ-215 | `machine.rs` unit tests `power_on_ram_zero`, `second_power_on_clears_ram`, `power_on_deterministic` |
| REQ-216 | `machine.rs` unit tests `power_on_starts_at_reset_vector`, `power_on_clears_pia`, `power_on_resets_chips`; `crates/harness/tests/boot.rs` `boots_to_basic_ready` |
| REQ-217 | `machine.rs` unit tests `reset_button_runs_reset_sequence`, `reset_button_resets_pia`, `reset_button_resets_antic`, `reset_button_keeps_ram_gtia_pokey`, `reset_button_no_nmi` |
| REQ-218 | `pia.rs` unit tests `registers_mirrored_every_four`, `mirror_read_side_effects` |
| REQ-219 | `pia.rs` unit tests `ddr_selected_by_bit_2_clear`, `data_register_selected_by_bit_2_set`, `ddr_bit_selects_direction`, `output_register_kept_while_ddr_selected`, `port_b_ddr_select` |
| REQ-220 | `pia.rs` unit tests `port_a_output_reads_and_of_pins`, `port_a_input_reads_pins` |
| REQ-221 | `pia.rs` unit tests `port_a_pulled_up`, `port_a_inputs_do_not_float`; `machine.rs` unit test `joystick_bits_on_port_a` |
| REQ-222 | `pia.rs` unit tests `port_b_output_reads_latch`, `port_b_inputs_after_reset`, `port_b_output_zero_not_pulled_up` |
| REQ-223 | `pia.rs` unit tests `floating_bit_reads_one_then_zero` (10 cycles, 100 ms and 500 ms, bits 2-6), `floating_bit_from_zero_reads_zero`; `machine.rs` unit test `floating_bits_do_not_change_map` |
| REQ-224 | `pia.rs` unit tests `control_write_ignores_bits_6_7`, `control_read_shows_status` |
| REQ-225 | `pia.rs` unit tests `ca1_negative_edge`, `ca1_positive_edge_ignored_in_negative_mode`, `cb1_positive_edge`, `ca1_status_without_enable`; `machine.rs` unit test `ca1_cb1_idle_high` |
| REQ-226 | `pia.rs` unit tests `ca2_input_negative_edge_no_irq`, `ca2_input_positive_edge_irq`, `cb2_input_negative_edge_irq`, `ca2_cb2_input_idle_high` |
| REQ-227 | `pia.rs` unit tests `cb2_command_line_levels`, `ca2_motor_line_levels`, `ca2_pulse_one_cycle_on_read`, `ca2_low_until_ca1`, `cb2_pulse_modes_on_write` |
| REQ-228 | `pia.rs` unit tests `data_read_clears_status`, `ddr_read_keeps_status`, `disable_keeps_status`, `output_mode_clears_bit_6` |
| REQ-229 | `pia.rs` unit tests `irq_from_ca1`, `irq_from_cb2`, `irq_released_after_read`, `no_irq_when_disabled`; `machine.rs` unit test `irq_line_is_pia_or_pokey`; `machine.rs` unit test `irq_line_with_pokey` |
| REQ-230 | `pia.rs` unit tests `reset_clears_all_registers`, `reset_selects_ddr`, `reset_clears_output_registers`, `reset_no_irq` |
| REQ-231 | `pia.rs` unit tests `spurious_ca2_positive_edge_mode`, `spurious_ca2_with_irq`, `spurious_cb2_after_rise`, `no_spurious_cb2_without_low` |
| REQ-232 | `machine.rs` unit tests `trig3_zero_without_cartridge`, `trig3_one_with_cartridge`, `trig3_follows_cartridge_bank_switch`, `trig3_ignores_basic`, `trig2_reads_one`, `trig3_after_power_on_and_reset` |

New tests in this plan, one line each:

- `machine_uses_configured_standard`: a PAL configuration gives 35,568-cycle frames, and an NTSC configuration gives 29,868-cycle frames.
- `scan_line_is_114_cycles`: 114 machine cycles from cycle 0 of line 0 reach cycle 0 of line 1.
- `free_cycles_run_cpu`: in one frame with `DMACTL` = `$22`, the CPU makes exactly one access in each cycle without DMA or refresh.
- `rdy_reads_reach_cartridge_control`: a test cartridge supplies `STA $D40A` and `NOP` in `$D5xx`, and counts one read of the `NOP` address for each stopped cycle plus one.
- `io_pages_reach_chips`: reads of `$D47B` and `$D013`, and writes to `$D3FF` and `$D2F0`, reach ANTIC, GTIA, the PIA and POKEY (REQ-205 criteria 3 and 4).
- `write_to_self_test_ignored`: REQ-206 criterion 4.
- `os_rom_size_checked`: `Machine::new` accepts a 16,384-byte OS image and gives `ConfigError` for 8,192, 16,383 and 16,385 bytes.
- `mixed_inputs_and_outputs_select_map`: `DDRB` = `$80` with output register `$00` maps the self-test ROM and keeps BASIC off.
- `ddrb_switch_next_cpu_access`: REQ-212 criterion 2.
- `dma_map_equals_cpu_map`: for 5 `PORTB` values, with and without a test cartridge, `dma_read` equals `peek` for all addresses outside `$D000-$D7FF`.
- `undriven_cartridge_control_reads_ff`: a test cartridge that returns `None` for `$D500` gives `$FF`.
- `second_power_on_clears_ram`: REQ-215 criterion 2.
- `power_on_starts_at_reset_vector`, `power_on_clears_pia`: REQ-216 criteria 1 and 2.
- `power_on_resets_chips`: REQ-216 criterion 3 (bus logs of a new machine and of a machine after a second power-on are equal for 2 frames).
- `reset_button_runs_reset_sequence`, `reset_button_resets_pia`, `reset_button_resets_antic`: REQ-217 criteria 1 to 3.
- `ddr_bit_selects_direction`, `port_b_ddr_select`: REQ-219 criteria 3 and 5.
- `port_a_inputs_do_not_float`: REQ-221 criterion 3.
- `port_b_output_zero_not_pulled_up`: REQ-222 criterion 3.
- `cb2_input_negative_edge_irq`: REQ-226 criterion 3.
- `no_irq_when_disabled`: REQ-229 criterion 4.
- `reset_clears_output_registers`: REQ-230 criterion 2.
- `trig3_after_power_on_and_reset`: REQ-232 criterion 6.

The boot tests in `crates/harness/tests/boot.rs` need the ROM files from
`scripts/fetch-roms.sh`. They are planned and do not exist yet.

## Open issues

1. The HRM does not say if the 65XE has data bus pull-ups. This design returns `$FF` for
   unmapped reads (REQ-214). A floating bus model can come later, if software needs it.
2. The machine does not model the floating bus during refresh cycles and during WSYNC
   (HRM 2.3). The value of ANTIC DMA reads in `$D000-$D7FF` is an ANTIC decision.
3. The draft `Machine::reset_key` does not reset ANTIC. HRM 2.4 requires the ANTIC reset.
4. The draft sets `TRIG3` only at cold reset. HRM 2.8 requires that `TRIG3` follows the
   cartridge bank switching. This design updates it after each cartridge access.
5. The draft `Pia::port_b_output` returns 1 for all input bits. That is correct for the
   memory map (bits 0, 1, 7), but a `PORTB` read must show bits 2-6 as floating (REQ-222,
   REQ-223).
6. For PAL, the HRM master clock of 14.18757 MHz / 8 gives 1,773,446.25 Hz, but the HRM
   states 1.773447 MHz. This design keeps the stated value (REQ-200). The draft value
   agrees with it.
7. The PLP+SEI case of the CPU (see [cpu.md](cpu.md), "Open issues") can change the IRQ
   timing of PIA interrupt handlers, but no change to the PIA is expected.
8. Resolved by OI-005: each chip type derives `Debug`, `Clone` and `PartialEq`, so a test
   can copy a chip and compare it. REQ-232 needs `GRACTL` bit 2 to turn the trigger latch
   off, which the GTIA design gives (REQ-436).
9. Resolved by OI-009: `Pokey::reset` sets the power-on state of REQ-501 (the same state as
   `Pokey::new`). The machine calls it only at power-on, never for the Reset button.
