# POKEY design

## Scope

This document tells how the emulator satisfies the POKEY requirements REQ-500 to REQ-541. It
covers the modules `crates/atari-core/src/pokey.rs` and `crates/atari-core/src/keyboard.rs`.

| Topic | Requirements |
| --- | --- |
| Addressing and power-on | REQ-500, REQ-501 |
| Initialization mode and clocks | REQ-502, REQ-503, REQ-506 |
| Timers and `STIMER` | REQ-504 to REQ-508 |
| Audio channels and noise | REQ-509 to REQ-514 |
| Interrupts | REQ-515 to REQ-517 |
| Keyboard | REQ-518 to REQ-524 |
| Serial port (POKEY side) | REQ-525 to REQ-533 |
| Paddles | REQ-534 to REQ-537 |
| Host audio output | REQ-538 to REQ-541 |

The SIO bus, the command line and the devices are in REQ-600 and later. This document gives
only the signals that POKEY gives to the SIO bus model and gets from it. The console speaker
register (`CONSOL`) belongs to GTIA. The paddle trigger buttons belong to the PIA.

## Interfaces

### Module `pokey`

```rust
use crate::timing::VideoStandard;

/// The POKEY chip of the 65XE.
pub struct Pokey { /* see State */ }

/// A byte that the serial output shift register sent completely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerialByte {
    /// The 8 data bits.
    pub value: u8,
    /// The POKEY cycle (see `Pokey::cycle`) on which the start bit began.
    pub start_cycle: u64,
    /// The POKEY cycle (see `Pokey::cycle`) on which the stop bit ended.
    pub end_cycle: u64,
}

impl Pokey {
    /// Creates POKEY in the power-on state (REQ-501).
    /// `sample_rate` is the host audio rate in Hz, 8000..=192000 (REQ-538).
    pub fn new(standard: VideoStandard, sample_rate: u32) -> Self;

    /// Sets the power-on state again (REQ-501). The machine calls it only for a cold start.
    pub fn reset(&mut self);

    /// Advances one machine cycle.
    pub fn tick(&mut self);

    /// Reads a register. `reg` is the low byte of the address; POKEY uses `reg & 0x0F`.
    pub fn read(&mut self, reg: u8) -> u8;
    /// Returns the value that `read` would return. It has no side effects (debugger).
    pub fn peek(&self, reg: u8) -> u8;
    /// Writes a register. `reg` is the low byte of the address.
    pub fn write(&mut self, reg: u8, value: u8);

    /// The level of the IRQ output. `true` = asserted.
    pub fn irq(&self) -> bool;
    /// The number of `tick` calls since power-on.
    pub fn cycle(&self) -> u64;

    // Keyboard (REQ-518 to REQ-523). `code` is a 6-bit key code (bits 6 and 7 are ignored).
    pub fn set_key(&mut self, code: u8, down: bool);
    pub fn release_all_keys(&mut self);
    pub fn set_shift(&mut self, down: bool);
    pub fn set_control(&mut self, down: bool);
    pub fn set_break(&mut self, down: bool);
    /// Test helper: releases all keys, then presses `kbcode & 0x3F`, with Shift if bit 6 is
    /// set and Control if bit 7 is set.
    pub fn key_down(&mut self, kbcode: u8);
    /// Test helper: releases the key and the modifiers that `key_down` pressed.
    pub fn key_up(&mut self);

    // Paddles (REQ-535). `index` is 0..=3. `count` is the slow-scan count at which the
    // input reaches threshold. A value of 228 or more means "never" (no paddle).
    pub fn set_pot(&mut self, index: usize, count: u8);

    // Serial port, POKEY side (REQ-525 to REQ-533). `true` = high level (mark).
    pub fn set_serial_in(&mut self, high: bool);
    pub fn serial_out(&self) -> bool;
    pub fn serial_clock_out(&self) -> bool;
    pub fn set_serial_clock_in(&mut self, high: bool);
    /// The level that POKEY drives on the bidirectional SIO CLOCK IN line, or `None` when
    /// the line is an input (`SKCTL` bits 6-4 other than `010` and `110`, REQ-527).
    pub fn serial_clock_in_drive(&self) -> Option<bool>;
    /// Removes and returns the oldest byte that was sent completely.
    pub fn take_serial_byte(&mut self) -> Option<SerialByte>;

    // Audio (REQ-538 to REQ-541).
    /// The level of the GTIA console speaker line. `true` = line pulled up
    /// (`CONSOL` bit 3 written as 0).
    pub fn set_speaker(&mut self, high: bool);
    /// Moves all samples made since the last call to the end of `out`.
    pub fn drain_samples(&mut self, out: &mut Vec<f32>);
    /// Selects the nonlinear DAC model (REQ-541). Off at power-on.
    pub fn set_nonlinear_audio(&mut self, on: bool);
}
```

### Module `keyboard`

```rust
pub const SHIFT: u8 = 0x40;
pub const CONTROL: u8 = 0x80;
// One constant for each key of REQ-523, for example:
pub const L: u8 = 0x00;      pub const RETURN: u8 = 0x0C;  pub const HELP: u8 = 0x11;
pub const ESC: u8 = 0x1C;    pub const SPACE: u8 = 0x21;   pub const INVERSE: u8 = 0x27;
pub const TAB: u8 = 0x2C;    pub const BACK_SPACE: u8 = 0x34; pub const CAPS: u8 = 0x3C;
pub const A: u8 = 0x3F;
/// All 54 keys of the 65XE keyboard: (code, name).
pub const KEYS: [(u8, &str); 54];
/// Converts one ASCII character to a `KBCODE` value (REQ-524).
pub const fn ascii_to_kbcode(c: char) -> Option<u8>;
```

### Call contract with the machine

1. The machine calls `tick` exactly once in each machine cycle. This includes the cycles in
   which ANTIC halts the CPU, and the idle cycles of a jammed CPU.
2. In a cycle with a CPU access to `$D2xx`, the machine calls `tick` first and then `read` or
   `write`. So a write in cycle N is a "write on cycle N" in HRM 1.2 Conventions in this manual. A read in
   cycle N shows each event whose visible cycle is N or earlier.
3. The machine gives `irq()` to the CPU together with the PIA IRQ (logical OR). POKEY does not
   model the 2-3 cycle delay of HRM 5.7 Interrupt timing. The CPU design owns that delay.
4. The machine calls `set_speaker` after each GTIA write, with the level of `CONSOL` bit 3.
5. Input functions (`set_key`, `set_shift`, `set_control`, `set_break`, `set_pot`,
   `set_serial_in`, `set_serial_clock_in`) can be called between two `tick` calls. The new
   level applies from the next `tick`.
6. Serial data flow (OI-038). In each machine cycle, before `tick`, the machine calls
   `set_serial_in(sio.data_in_level(now))`. After `tick`, it moves each byte from
   `take_serial_byte` to `SioBus::host_byte`. `Pokey::cycle` counts the `tick` calls since
   power-on, so it equals the machine cycle count that the SIO bus uses. The FIFO of
   `take_serial_byte` holds 64 bytes. When it is full, the oldest byte is lost.
7. Force break (OI-029). POKEY does not put a byte into the `take_serial_byte` FIFO if
   `SKCTL` bit 7 was 1 in any cycle of that byte's frame, because SIO DATA OUT was 0 and no
   device can receive the byte (REQ-532).
7. The frontend calls `Machine::drain_audio`, which calls `drain_samples`, once per frame.
   The sample buffer grows until the call. It has no maximum size.
8. `Machine` keeps these wrappers: `key_down`, `key_up`, `set_shift`, `set_control`,
   `set_break(down)`, `set_key(code, down)`, `set_paddle(index, count)`.

### Differences from the draft skeleton

| Draft | This design | Reason |
| --- | --- | --- |
| `Pokey::new(sample_rate)` | `Pokey::new(standard, sample_rate)` | The sample count needs the machine clock rate, which differs for NTSC and PAL (REQ-538). |
| `read` returns `KBCODE` = `$FF` when no key is down | `KBCODE` keeps the last registered code | A key release does not change `KBCODE` (REQ-518). |
| `key_down` changes `KBCODE` and `SKSTAT` at once | The scan registers the key after 64-129 scanlines | REQ-518. `type_text` holds each key for 3 frames, which is enough. |
| `press_break()` (a single event) | `set_break(down)` | The scan detects BREAK only while it is down (REQ-522). |
| No paddle and no serial functions | `set_pot`, serial functions, `serial_clock_in_drive` | REQ-525 to REQ-537. |
| `read(&mut self)` | `read(&mut self)` and `peek(&self)` | POKEY reads have no side effects. `peek` serves the debugger. |

## State

All state is in `Pokey`. Power-on values are the values after `new` and `reset` (REQ-501).

| Field | Type | Power-on | Meaning |
| --- | --- | --- | --- |
| `audf` | `[u8; 4]` | 0 | `AUDF1-4` |
| `audc` | `[u8; 4]` | 0 | `AUDC1-4` |
| `audctl` | `u8` | 0 | `AUDCTL` |
| `skctl` | `u8` | 0 | `SKCTL` (0 = initialization mode) |
| `irqen` | `u8` | 0 | `IRQEN` |
| `irqen_hist` | `[u8; 4]` | 0 | `IRQEN` value at the end of cycles N-1 to N-4 (REQ-517) |
| `irqst` | `u8` | `$FF` | Latched `IRQST` bits 0-2 and 4-7. Bit 3 is computed. |
| `kbcode` | `u8` | `$FF` | `KBCODE` |
| `serin` | `u8` | `$00` | `SERIN` |
| `sk_err` | `u8` | `$E0` | `SKSTAT` bits 5-7 (1 = no error) |
| `cycle` | `u64` | 0 | Ticks since power-on |
| `exit_cycle` | `u64` | 0 | Cycle of the last write that ended initialization mode |
| `div64`, `div15` | `u8` | 0 | Slow clock dividers (held at 0 in initialization mode) |
| `counter` | `[u8; 4]` | 0 | Timer counters |
| `reload_at` | `[Option<u64>; 4]` | `None` | Cycle of the next scheduled reload |
| `fire_at` | `[[Option<u64>; 2]; 4]` | `None` | Visible cycles of scheduled fires |
| `poly4`, `poly5` | `u8` | 0 | 4-bit and 5-bit counters |
| `poly17` | `u32` | 0 | 17-bit register (bits 16-0) |
| `noise_hist` | `[u8; 2]` | 0 | Last 4 output bits of `poly4` and `poly5`, for the channel delays |
| `out_ff` | `[bool; 4]` | `true` | Channel output flip-flops |
| `hp_ff` | `[bool; 2]` | `true` | High-pass flip-flops of channels 1 and 2 |
| `matrix` | `u64` | 0 | Keys that are down (bit n = key code n) |
| `shift`, `control`, `brk` | `bool` | `false` | Modifier keys that are down |
| `typed_mods` | `u8` | 0 | Modifiers that `key_down` pressed |
| `kb_counter` | `u8` | 0 | Scan counter, 0-63 |
| `kb_compare` | `u8` | 0 | Compare register |
| `kb_state` | `u8` | 0 | Scan state: 0, 1, 2 or 3 |
| `kb_shift_latch`, `kb_ctrl_latch` | `bool` | `false` | Latched modifier states |
| `kb_key_down` | `bool` | `false` | `SKSTAT` bit 2 is 0 |
| `brk_prev` | `bool` | `false` | BREAK level at the last check |
| `pot_counter` | `u8` | 228 | Pot scan counter |
| `pot_active` | `bool` | `false` | A pot scan runs |
| `pot` | `[u8; 8]` | 0 | `POT0-7` |
| `allpot` | `u8` | 0 | `ALLPOT` |
| `pot_threshold` | `[Option<u32>; 8]` | `None` | Charge time in cycles to threshold; `None` = never |
| `pot_charge` | `[u32; 8]` | 0 | Charge time in cycles since the last dump |
| `serout`, `serout_pending` | `u8`, `bool` | 0, `false` | `SEROUT` and its pending flag |
| `out_shift`, `out_bits` | `u16`, `u8` | 0, 0 | Output shift register; bits left (0 = idle) |
| `out_level` | `bool` | `true` | SIO DATA OUT from the shift register |
| `tone_ff` | `bool` | `false` | Two-tone output flip-flop |
| `in_level`, `clk_in_level` | `bool` | `true` | SIO DATA IN and SIO CLOCK IN |
| `in_shift`, `in_bits` | `u16`, `u8` | 0, 0 | Input shift register; bits received (0 = waiting) |
| `in_busy` | `bool` | `false` | `SKSTAT` bit 1 is 0 |
| `ser_in_clk`, `ser_out_clk` | `bool` | `false` | Serial clock flip-flops |
| `sent` | `VecDeque<SerialByte>` | empty | Sent bytes for the SIO model |
| `out_start_cycle` | `u64` | 0 | Cycle where the start bit of the current output frame began |
| `out_break_seen` | `bool` | `false` | `SKCTL` bit 7 was 1 in a cycle of the current output frame (OI-029) |
| `speaker` | `bool` | `false` | Console speaker level |
| `nonlinear` | `bool` | `false` | REQ-541 option |
| `clock_hz`, `sample_rate` | `u32` | from `new` | 1789773 (NTSC) or 1773447 (PAL); host rate |
| `phase` | `u64` | 0 | Resampler phase, in units of 1 / (`clock_hz` x `sample_rate`) s |
| `acc_pokey`, `acc_spk` | `f64` | 0 | Sums for the current sample |
| `hp1`, `hp2` | `f32` | 0 | Capacitor states of the two high-pass stages |
| `samples` | `Vec<f32>` | empty | Samples not yet drained |

## Behavior

### Register access (HRM 5.1, HRM 14.4)

`read` and `write` use `reg & 0x0F` (REQ-500). The read and write maps are in REQ-500.
Reads of offsets `$B` and `$C` return `$FF`. Reads have no side effects, so `read` calls
`peek`.

The composed read values:

- `IRQST` = `irqst & !0x08`, plus bit 3 = 1 while `out_bits != 0` (REQ-516).
- `SKSTAT` = `sk_err` | `in_level << 4` | `!kb_shift_latch << 3` | `!kb_key_down << 2` |
  `!in_busy << 1` | 1. `in_busy` is true from the start-bit sample to the stop-bit sample.
- `RANDOM` = `!(poly17 >> 8) as u8` (see "Noise generators").

Writes:

- `AUDFx`, `AUDCx`, `AUDCTL`: stored. A new `AUDFx` takes effect at the next reload.
- `STIMER`: see "Timers".
- `SKRES`: `sk_err = 0xE0`.
- `POTGO`: see "Pot scan".
- `SEROUT`: `serout = value`, `serout_pending = true`.
- `IRQEN`: `irqen = value`; `irqst |= !value & 0xF7` (status of disabled sources goes to 1).
- `SKCTL`: see "Initialization mode". If bit 3 changes to 0, `tone_ff = false`.

### Initialization mode (HRM 5.2)

POKEY is in initialization mode while `skctl & 0x03 == 0`. In this mode:

- `div64` and `div15` stay at 0. No slow clock pulse occurs.
- The polynomial counters shift in 0 bits (see "Noise generators").
- The serial input waits for a start bit; `in_bits = 0`. The output stops: `out_bits = 0`,
  `serout_pending = false`. `out_level` keeps its value (REQ-531).
- The keyboard scan is held: `kb_state = 0`, `kb_counter = 0`.

A write that ends initialization mode sets `exit_cycle` to the current cycle. The slow clocks
then start with the phase in "Timing".

### Clocks and timers (HRM 5.3, HRM 5.4)

Each tick, POKEY computes three pulse flags for the cycle: `fast` (always true), `p64` and
`p15`. A timer counts on `fast` if it is timer 1 with `AUDCTL` bit 6, or timer 3 with bit 5.
Otherwise it counts on `p15` if `AUDCTL` bit 0 is 1, else on `p64`. The high timer of a
linked pair counts on the underflows of the low timer.

Timer model. Let U be the cycle on which a counter decrements from 0 (underflow):

1. An unlinked timer schedules a reload on cycle U+3 and a fire with visible cycle U+4.
2. Between U+1 and U+3, the counter does not decrement.
3. On the reload cycle, `counter = audf`. Decrements start on the next cycle.

So the 1.79 MHz period is (N+1) + 3 = N + 4 cycles. On a slow clock, the reload occurs before
the next pulse, so the period is (N + 1) pulses (REQ-504).

Linked pair (REQ-505). The low timer underflows on cycle L. It wraps to `$FF` and does not
reload. It schedules a fire on cycle L+4 for its own channel. The high timer decrements on
cycle L+3. When the high timer underflows on that cycle (H = L+3), both timers reload on cycle
H+3 and the high timer fires on cycle H+4. The 1.79 MHz period is
(lo + 1) + 256 x hi + 3 + 3 = N + 7.

`STIMER` written on cycle W (REQ-507, REQ-506):

1. `out_ff = [true; 4]`. This does not count as a channel clock.
2. All scheduled fires with a visible cycle of W+4 or later are cancelled.
3. All four timers schedule a reload on cycle W+3 and stop decrementing until then. The
   reload reads `audf` on cycle W+3, so an `AUDF` write on cycle W+2 is in time.

Check against HRM 5.3: with `AUDF1` = 0, the reloads are on T+3 and T+7 and the fire on T+8.
With `AUDF1` = 5, the reloads are on T+3 and T+12, and the fire on T+13. A second `STIMER` on
T+4 cancels the fire of T+8. A second `STIMER` on T+5 does not.

A fire on its visible cycle E does these actions, in this order:

1. Timer IRQ: timers 1, 2 and 4 latch `IRQST` bit 0, 1 or 2 (see "Interrupts").
2. Audio clock of its channel (see "Channel output").
3. High-pass capture: a fire of timer 3 sets `hp_ff[0] = out_ff[0]` if `AUDCTL` bit 2 is 1.
   A fire of timer 4 sets `hp_ff[1] = out_ff[1]` if `AUDCTL` bit 1 is 1.
4. Serial clocks: a fire of timer 4 (and of timer 2 in modes `11x`) toggles the serial clock
   flip-flops that Table 10 selects (see "Serial port").
5. Two-tone resync (REQ-533): if the fire toggles `tone_ff`, timers 1 and 2 schedule a reload
   2 cycles after the reload of the timer that fired. This reload does not fire.

Asynchronous receive (REQ-528): while `SKCTL` bit 4 is 1 and the input waits for a start bit,
timers 3 and 4 are held: `counter = audf`, no decrement, and no fire. At the falling edge of
SIO DATA IN, POKEY treats timers 3 and 4 like an `STIMER` write on that cycle.

### Channel output (HRM 5.3)

On an audio clock of channel n (index 0-3), POKEY evaluates `AUDCn`:

1. If bit 7 is 0 and the 5-bit noise bit of the channel is 0, the clock is ignored.
2. Else, if bit 5 is 1, `out_ff[n] = !out_ff[n]`.
3. Else, if bit 6 is 1, `out_ff[n]` = the 4-bit noise bit of the channel.
4. Else, `out_ff[n]` = the 9/17-bit noise bit of the channel.

The digital output of a channel in each cycle:

- channels 1 and 2: `out_ff[n] ^ hp_ff[n]`, with `hp_ff[n] = true` while the high-pass bit
  is 0;
- channels 3 and 4: `out_ff[n]`;
- if `AUDCn` bit 4 is 1: `true` (volume-only mode, REQ-510).

The channel level is `AUDCn & 0x0F` when the digital output is 1, else 0.

The 5-bit gate polarity (a clock goes through on 1) is a project decision (REQ-509).

### Noise generators (HRM 5.5)

All counters shift right by one bit in each cycle. The new bit enters at the top. In
initialization mode, the new bit is 0, so the registers clear to 0.

| Counter | Register | Feedback (new top bit) | Output |
| --- | --- | --- | --- |
| 4-bit | `poly4`, bits 3-0 | `!(b0 ^ b1)` into bit 3 | bit 0 |
| 5-bit | `poly5`, bits 4-0 | `!(b0 ^ b2)` into bit 4 | bit 0 (1 in initialization mode) |
| 9-bit (`AUDCTL` bit 7 = 1) | `poly17`, bits 16-8 | `!(b8 ^ b13)` into bit 16 | see below |
| 17-bit (`AUDCTL` bit 7 = 0) | `poly17`, bits 16-0 | `!(b0 ^ b5)` into bit 16 | see below |

`RANDOM` = `!(poly17 >> 8) & 0xFF`. The 9/17-bit noise bit of channel n (n = 0-3) is
`poly17` bit 7 - n. These bits are earlier values of bit 8, one cycle older for each channel.
So the CPU sees each bit in `RANDOM` bit 0 before channel 1 gets it, and channels 1-4 get it
one cycle apart (REQ-512). The 4-bit and 5-bit noise bits of channel n are the output bit
n cycles ago, from `noise_hist`.

This model was checked with a script against the HRM data:

- It gives the HRM 14.4 `RANDOM` table for all 511 entries. The zero state is entry 129.
- The read 4 cycles after the exit from initialization mode gives `$1F`.
- Reads 114 cycles apart give `$00`, `$DF`, `$EE`, `$16`, `$B9`.
- On entry to initialization mode, `RANDOM` reads `$FF` from 9 cycles after the write.
- The 4-bit and 5-bit outputs from the zero state give the patterns of HRM 5.5.
- The 17-bit feedback has a period of 131071. The 9-bit output has 255 ones in 511 bits.

### Interrupts (HRM 5.7)

An event latches with `irqst &= !bit` if the bit is in the latch mask of the event cycle E:
`mask(E) = irqen_hist[3] & irqen_hist[1]`, which are the `IRQEN` values at the end of cycles
E-4 and E-2. An enable is effective after 4 cycles and a disable after 2 cycles (REQ-517).
The `IRQEN` write itself sets the status bits of disabled sources to 1 at once.

`irq()` = `(!irqst & 0xF7) != 0` or (`IRQEN` bit 3 is 1 and `out_bits == 0`).

The serial input event also checks overrun: if `IRQST` bit 5 is already 0, `sk_err` bit 5
goes to 0 (REQ-529). The keyboard event does the same for bit 6 (REQ-521).

### Keyboard scan (HRM 5.8)

On each keyboard pulse (see "Timing"), if `SKCTL` bit 1 is 1:

1. `c = kb_counter`, `down = matrix` bit c.
2. If c = `$00`, `kb_ctrl_latch = control`. If c = `$10`, `kb_shift_latch = shift`.
   If c = `$30`, and `brk` and not `brk_prev`, latch `IRQST` bit 7. Then `brk_prev = brk`.
3. `match` = (c == `kb_compare`) or `SKCTL` bit 0 is 0.
4. State machine:
   - State 0: if `down`, `kb_compare = c`, go to state 1.
   - State 1: if `match` and not `down`, go to 0. If `match` and `down`: set
     `kbcode = c | shift_latch << 6 | ctrl_latch << 7`, `kb_key_down = true`, latch `IRQST`
     bit 6 (with overrun check), go to 3. If not `match` and `down`, go to 0.
   - State 3: if `match` and not `down`, go to 2.
   - State 2: if `match` and not `down`, `kb_key_down = false`, go to 0. If `match` and
     `down`, go to 3 (project decision, see "Open issues").
5. `kb_counter = (c + 1) & 63`.

If `SKCTL` bit 1 is 0: `kb_state = 0`, `kb_counter = 0`, `kb_key_down = false`. The latches
keep their values. BREAK is not checked.

`key_down(kbcode)` sets `matrix = 1 << (kbcode & 63)` and sets `shift` and `control` from
bits 6 and 7. It records these modifiers in `typed_mods`. `key_up` clears `matrix` and the
modifiers in `typed_mods`. `set_shift` and `set_control` set the host modifier levels. The
effective modifier level is the OR of both sources.

The emulator does not model phantom keys. Two keys down at the same time give the "two keys
rejected" behavior of the state machine.

### ASCII conversion (REQ-524)

`ascii_to_kbcode` is a `match` on the character. It returns the code from the REQ-523 table
for the unshifted legend, or the code + `SHIFT` for the shifted legend. Letters of both cases
give the unshifted letter key. It returns `None` for the other characters.

### Serial port (HRM 5.6)

Clocks. The serial input clock and output clock come from Table 10 (REQ-527). The internal
clock flip-flops toggle on the timer fires. External clocks follow `clk_in_level`. Writing
`SKCTL` bits 4-6 = `000` sets both flip-flops to 0.

Output. `out_bits` is the number of bits of the frame that are not complete, including
the bit on SIO DATA OUT. On each rising edge of the output clock:

1. If `out_bits > 0`: decrement `out_bits`. If it is still above 0, set `out_level` to the
   next bit of `out_shift`. If it is 0, the stop bit cell has ended: push a `SerialByte`
   with the start cycle of the frame and the current cycle to `sent`, unless force break
   was on during the frame (call contract item 7).
2. Then, if `out_bits == 0` and `serout_pending`: load `out_shift` with the start bit, the 8
   bits of `serout` and the stop bit. Set `out_level = false` (start bit), `out_bits = 10`
   and `serout_pending = false`. Latch `IRQST` bit 4.

Step 2 runs on the same edge as the end of step 1, so back-to-back bytes have no gap. When
step 2 does not load, the output is idle, and `IRQST` bit 3 reads 0.

SIO DATA OUT (`serial_out`):

- Two-tone mode (`SKCTL` bit 3 = 1): `tone_ff`. `tone_ff` toggles on a timer 2 fire, and on
  a timer 1 fire when the data bit is 1. Force break (bit 7) makes the data bit 0.
- Else, if `SKCTL` bit 7 is 1: `false`.
- Else: `out_level`.

`serial_clock_out` is the output clock flip-flop.

Input. On each falling edge of the input clock, do one of these steps:

- If `in_bits == 0` (waiting) and `in_level` is 0: this is the start bit. Set
  `in_bits = 1` and `in_busy = true`.
- If `1 <= in_bits <= 8`: put `in_level` into data bit `in_bits - 1` (bit 0 first), then
  increment `in_bits`.
- If `in_bits == 9`: this is the stop bit. Copy the data into `serin`. If `in_level` is 0,
  set `sk_err` bit 7 to 0. Latch `IRQST` bit 5 (with the overrun check). Set
  `in_busy = false` and `in_bits = 0`. In asynchronous mode, hold timers 3 and 4 again.

In synchronous mode, a 0 level on a falling edge while waiting starts a frame. So the start
bit can be sampled anywhere in its cell.

In asynchronous mode, the start edge presets the input clock. The first falling edge comes
one timer 4 period after the start edge, so the start bit is sampled in its middle.

`SKSTAT` bit 4 reads `in_level` directly in all modes (REQ-530).

### Pot scan (HRM 5.9, HRM 7.2)

Each input has a charge time `pot_charge[i]` in cycles and a threshold `pot_threshold[i]`.
`set_pot(i, c)` sets the threshold to `c x 114` cycles for c < 228, else `None`. Inputs 4-7
are always `None` (grounded, REQ-535).

Each tick:

1. Dump transistors: if `SKCTL` bit 2 is 0 and `pot_active` is false, `pot_charge = 0`.
   Else each charge time increments by 1.
2. Input i is above threshold if its threshold is `Some(t)` and `pot_charge[i] > t`.
3. If `pot_active`: the counter increments on each cycle (fast mode) or on each pot pulse
   (slow mode, see "Timing"). In slow mode, there are no pot pulses in initialization mode.
4. For each input with `ALLPOT` bit i = 1: `pot[i] = pot_counter`. Then, if the input is
   above threshold, clear the bit. The value of this cycle stays in `pot[i]`.
5. End of scan: on the cycle after the counter reaches 228, the scan stop is detected. In
   fast mode, the counter increments to 229 in that cycle first, so the active inputs latch
   229. In slow mode, they latch 228. On the next cycle, `ALLPOT` = 0 and `pot_active` =
   false.

With `>` in step 2, a slow scan from an empty capacitor gives exactly the count c of
`set_pot`: the window from the counter reset to the threshold cycle holds exactly c pot
pulses, for each phase of the 15 kHz clock.

`POTGO` written on cycle G: `pot_active = true`; on cycle G+1 the counter is 0. Each input
below threshold gets `ALLPOT` bit = 1 and `pot[i] = 0`. An input above threshold keeps its
`pot[i]` value and gets bit = 0.

The charge model gives the truncated and suspended scans of REQ-537, because the charge
continues while the counter restarts or stops.

### Audio output (REQ-538 to REQ-541)

Each tick, after the channel outputs:

1. `level` = sum of the four channel levels (0-60). With the nonlinear option, `level` is
   the sum of the DAC weights of the set volume bits (0.12, 0.26, 0.56, 1.12 for bits 0-3),
   divided by 8.24, then passed through the curve of HRM E.2; the result is in 0..1.
   Without the option, `x = level / 60`.
2. `s = 0.25` if `speaker`, else 0.
3. Resampling (box filter): `phase += sample_rate`. If `phase < clock_hz`, the whole cycle
   goes into the current sample: `acc_pokey += x`, `acc_spk += s`. Else the fraction
   `f = (phase - clock_hz) / sample_rate` of the cycle goes into the next sample. The rest
   goes into the current sample, which then completes. `phase -= clock_hz`.
4. A completed sample has the averages `xp = acc_pokey x sample_rate / clock_hz` and `xs`.
5. Stage 1: `h1 = xp - hp1`, then `hp1 += (xp - hp1) x a1`, with
   `a1 = 1 - exp(-1 / (0.0026 x sample_rate))`.
6. Stage 2: `u = h1 + xs`, `out = u - hp2`, `hp2 += (u - hp2) x a2`, with
   `a2 = 1 - exp(-1 / (0.0247 x sample_rate))`.
7. The sample is `out`, clamped to -1.0..=1.0, pushed to `samples`.

The integer phase gives exactly `floor(C x sample_rate / clock_hz)` samples after C cycles
(REQ-538). A script check of the box filter for a 224 kHz tone at 48000 Hz gave 0.018
peak-to-peak. A check of the two stages for a step gave 30.8 % of the peak after 2.6 ms and
36.8 % for a speaker step after 24.7 ms. These values are inside the REQ-539 and REQ-540
criteria.

## Timing

All cycles are machine cycles. W is the cycle of the `SKCTL` write that ends initialization
mode. T is the cycle of an `STIMER` write. G is the cycle of a `POTGO` write.

| Event | Cycle | Source |
| --- | --- | --- |
| 64 kHz timer decrement pulse | W + 20 + 28 k | HRM 5.2: IRQ on W+24, minus the 4-cycle fire delay |
| 15 kHz timer decrement pulse | W + 79 + 114 k | HRM 5.2: IRQ on W+83, minus the 4-cycle fire delay |
| Pot counter pulse (slow mode) | W + 81 + 114 k | HRM 5.9 Scan timing |
| Keyboard scan pulse | W + 81 + 114 k | Project decision (same pulse as the pot counter) |
| First reload after `STIMER` | T + 3 | HRM 5.3 Reload timing |
| Last `AUDF` write for the first period | T + 2 | HRM 5.3 Reload timing |
| First timer 1 IRQ, `AUDF1` = 0, 1.79 MHz | T + 8 | HRM 5.3 STIMER preemption |
| Last `STIMER` write that cancels a fire visible on E | E - 4 | HRM 5.3 STIMER preemption |
| Fire after underflow U | U + 4 | Model (gives the HRM values above) |
| High timer of a linked pair after low underflow L | L + 3 (underflow), L + 7 (fire) | HRM 5.3 Linked timer fire timing |
| Two-tone resync reload | reload of the toggling timer + 2 | HRM 5.6 Two-tone resync timing |
| Last enabling `IRQEN` write for an event on E | E - 4 | HRM 5.7 Enable/disable timing |
| Last disabling `IRQEN` write for an event on E | E - 2 | HRM 5.7 Enable/disable timing |
| `RANDOM` = `$FF` after init entry on cycle I | I + 9 | HRM 5.5 Initialization behavior |
| `RANDOM` = `$1F` (9-bit) | W + 4 | HRM 5.5 Initialization behavior |
| Fast pot scan: `ALLPOT` = `$00` | G + 231 | HRM 5.9 Scan timing |
| Slow pot scan: last increment (227 to 228) | first pulse after G, + 227 x 114 | HRM 5.9 Scan timing |
| Slow pot scan: `ALLPOT` = `$00` | last increment + 2 | HRM 5.9 Scan timing |

The divider model: `div64` and `div15` count the cycles since W. A slow clock pulse occurs
when `(cycle - W - 20) % 28 == 0` or `(cycle - W - 79) % 114 == 0`, for a non-negative
difference. The implementation can use down-counters with the same result.

## Test plan

Unit tests are in `#[cfg(test)] mod tests` of `crates/atari-core/src/pokey.rs` and
`crates/atari-core/src/keyboard.rs`. They drive `Pokey` directly with `write`, `tick`,
`read`, `peek`, `irq`, the input functions and `drain_samples`, and count cycles with
`cycle()`. Some tests read internal state through the test accessors below. System tests are
in `crates/harness/tests/`. They run the OS and BASIC, or drive `Machine`, and analyze the
samples from `Machine::drain_audio`.

### Test accessors

These functions are `#[cfg(test)]` methods of `Pokey`. They have no side effects. The
requirements call them "the test accessor".

| Accessor | Returns | Used by |
| --- | --- | --- |
| `test_write_regs()` | `AUDF1-4`, `AUDC1-4`, `AUDCTL`, `SKCTL`, `IRQEN` as stored | REQ-500, REQ-501, REQ-502 |
| `test_counters()` | The four timer counters | REQ-502, REQ-507 |
| `test_fires()` | Log of `(timer 1-4, visible cycle)` for each fire since the last call; it also records fires of disabled IRQs | REQ-503 to REQ-509, REQ-528, REQ-533 |
| `test_flip_flops()` | `out_ff` of the four channels | REQ-502, REQ-507, REQ-510, REQ-512, REQ-513 |
| `test_channel_output(n)` | The digital output of channel n in this cycle (after the high-pass XOR and the volume-only override) | REQ-507, REQ-509 to REQ-511, REQ-528 |
| `test_noise_bits(n)` | The 4-bit, 5-bit and 9/17-bit noise bits that channel n sees in this cycle | REQ-513 |
| `test_mix_level()` | The sum of the four channel levels, 0-60 | REQ-500, REQ-510, REQ-511, REQ-528 |
| `test_dac_sum()` | The normalized nonlinear DAC sum before the curve | REQ-541 |
| `test_curve(x)` | The saturation curve of HRM E.2 for input x | REQ-541 |

The harness audio tests use the measurement methods of REQ-503 Notes (measured frequency),
REQ-539 (peak-to-peak) and REQ-540 (step response). They put these methods in one helper
module of the test file, so that all audio tests use the same code.

### Test map

Each test name is followed by the numbers of the acceptance criteria that it checks, in
brackets. A test marked "new" was added in the review of the acceptance criteria; the line
after the table tells what it does.

| Requirement | Test |
| --- | --- |
| REQ-500 | `pokey.rs` `mirrors_select_same_register` [1-3], `unassigned_reads_return_ff` [4], `offset_c_write_has_no_effect` [5] (new), `register_map_matches_table` [6] (new) |
| REQ-501 | `pokey.rs` `power_on_write_registers_zero` [1] (new), `power_on_state` [2, 3], `power_on_is_deterministic` [4], `reset_equals_new` [5] (new) |
| REQ-502 | `pokey.rs` `init_mode_stops_slow_clocks` [1], `init_mode_keeps_fast_timers` [2], `init_mode_keeps_registers` [3], `init_mode_flushes_serout` [4], `init_mode_stops_slow_pot_counter` [5], `init_mode_random_ff_after_9_cycles` [6], `init_mode_holds_keyboard_scan` [7] (new), `init_mode_holds_serial_input` [8] (new) |
| REQ-503 | `pokey.rs` `slow_clock_pulse_spacing` [1], `timers_2_and_4_never_fast` [2], `fast_clock_bits_select_timer` [3] (new), `square_wave_period_4088` [4, 5] (new); `crates/harness/tests/pokey_audio.rs` `tone_437_hz_ntsc` [4], `tone_433_hz_pal` [5], `basic_sound_middle_c` (system check) |
| REQ-504 | `pokey.rs` `unlinked_fast_period` [1], `unlinked_64khz_period` [2], `unlinked_15khz_period` [3], `slow_timers_share_pulse` [4] |
| REQ-505 | `pokey.rs` `linked_fast_period` [1], `linked_period_05cc` [2], `linked_slow_periods` [3], `linked_low_timer_fires_hi_plus_one` [4], `linked_high_fires_3_after_low` [5], `linked_period_ignores_audc` [6] |
| REQ-506 | `pokey.rs` `stimer_first_irq_t8` [1], `stimer_first_irq_t13` [2], `stimer_preemption_window` [3], `audf_reload_deadline` [4], `init_exit_irq_w83` [5], `init_exit_irq_w24` [6], `linked_timer_irq_timing` [7] |
| REQ-507 | `pokey.rs` `stimer_sets_output_flip_flops` [1], `stimer_reloads_all_timers` [2, 3] (new), `stimer_does_not_fire` [4], `stimer_keeps_slow_clock_phase` [5] |
| REQ-508 | `pokey.rs` `timer_irq_bits` [1], `timer_3_has_no_irq` [2], `linked_timer_irq_once_per_period` [3], `timer_irq_lost_when_disabled` [4], `timer_irq_ignores_audc` [5] |
| REQ-509 | `pokey.rs` `distortion_pure_tone` [1], `distortion_poly4` [2], `distortion_poly9` [3], `distortion_poly17` [4] (new), `distortion_poly5_mask` [5], `distortion_poly5_masks_noise` [6] (new), `distortion_ignored_by_irq_and_serial` [7] |
| REQ-510 | `pokey.rs` `volume_only_level` [1], `volume_only_zero` [2], `volume_only_channels_add` [3], `output_zero_adds_nothing` [4] (new), `volume_only_keeps_flip_flop_running` [5], `mix_level_maximum_60` [6] (new) |
| REQ-511 | `pokey.rs` `ch1_ch3_cancel_without_high_pass` [1], `ch1_ch2_add_without_high_pass` [2], `high_pass_period_4032` [3] (new), `high_pass_channel_2` [4] (new), `high_pass_ignores_audc3` [5]; `crates/harness/tests/pokey_audio.rs` `high_pass_sawtooth_443_hz` [3] |
| REQ-512 | `pokey.rs` `noise_delay_ch1_ch2` [1], `noise_delay_ch1_ch3` [2] (new), `noise_delay_ch1_ch4` [3], `noise_follows_random_bit_0` [4] (new) |
| REQ-513 | `pokey.rs` `poly4_pattern` [1], `poly5_pattern` [2], `poly_outputs_in_init_mode` [3] |
| REQ-514 | `pokey.rs` `random_9bit_table` [1], `random_9bit_every_scanline` [2], `random_after_init_exit_1f` [3], `random_17bit_period` [4], `random_ff_in_init_mode` [5] |
| REQ-515 | `pokey.rs` `irq_latch_and_line` [1], `irqen_clear_resets_status` [2], `irq_line_until_all_cleared` [3], `irqst_read_no_side_effect` [4], `disabled_event_is_lost` [5] |
| REQ-516 | `pokey.rs` `serial_complete_idle_no_irq` [1], `serial_complete_irq_when_enabled` [2], `serial_complete_reads_1_while_shifting` [3, 4], `irqen_does_not_clear_bit_3` [5], `serial_complete_after_last_stop_bit` [6] |
| REQ-517 | `pokey.rs` `irqen_enable_timing` [1], `irqen_disable_timing` [2] |
| REQ-518 | `pokey.rs` `key_press_sets_kbcode_irq_skstat` [1], `key_press_latency` [2], `key_release_latency` [3], `two_keys_rejected` [4], `second_key_ignored` [5], `same_key_repeats_irq` [6]; `crates/harness/tests/keyboard.rs` `basic_prints_sum` (system check) |
| REQ-519 | `pokey.rs` `shift_control_bits_in_kbcode` [1], `late_shift_keeps_kbcode` [2], `skstat_shift_follows_key` [3], `modifiers_alone_no_irq` [4], `modifiers_ignore_debounce` [5] |
| REQ-520 | `pokey.rs` `debounce_off_single_key` [1], `debounce_off_adjacent_keys` [2], `scan_disabled_state` [3], `debounce_off_releases_key` [4] |
| REQ-521 | `pokey.rs` `keyboard_overrun` [1], `skres_clears_keyboard_overrun` [2], `kbcode_read_no_side_effect` [3], `modifiers_no_overrun` [4] |
| REQ-522 | `pokey.rs` `break_irq_on_key_down` [1], `break_irq_once_while_held` [2], `break_lost_when_disabled` [3], `break_irq_on_second_press` [4] (new), `break_no_kbcode_change` [5], `break_needs_scan` [6], `break_ignores_debounce` [7]; `crates/harness/tests/keyboard.rs` `break_stops_basic_program` (system check) |
| REQ-523 | `pokey.rs` `each_key_latches_its_code` [1]; `keyboard.rs` `key_constants_match_table` [2], `no_key_codes_unused` [3, 4] |
| REQ-524 | `keyboard.rs` `ascii_letters` [1], `ascii_digits_and_space` [2], `ascii_shifted_symbols` [3], `ascii_unshifted_symbols` [4, 5], `ascii_control_characters` [6], `ascii_without_key_is_none` [7], `ascii_never_sets_control` [8] (new) |
| REQ-525 | `pokey.rs` `serial_output_load_and_ready_irq` [1], `serial_output_frame_4f` [2], `serial_output_back_to_back` [3], `serout_overwrite_before_load` [4], `no_ready_irq_at_end` [5], `serial_byte_reported_with_cycle` [6] |
| REQ-526 | `pokey.rs` `serial_input_frame_43` [1], `serial_input_irq_timing` [2], `serial_input_busy_bit` [3], `serin_read_no_side_effect` [4], `serial_input_256_bytes` [5], `serial_input_without_irq` [6] (new); `crates/harness/tests/pokey_serial.rs` `os_sio_receives_frame` (system check) |
| REQ-527 | `pokey.rs` `serial_clock_mode_table` [1, 3], `async_output_clock_stops` [2] (new), `bidirectional_clock_drive` [4] (new), `external_clock_shifts_on_clock_in` [5], `serial_clock_edges` [6], `serial_bit_rate_half_fire_rate` [7], `serial_clock_ignores_audc` [8] |
| REQ-528 | `pokey.rs` `async_holds_timers_3_4` [1], `async_first_timer4_fire` [2] (new), `async_19_timer4_fires_per_byte` [3], `async_tolerates_3_percent` [4], `async_silences_ch3_ch4` [5] |
| REQ-529 | `pokey.rs` `framing_error` [1], `serial_overrun` [2], `serin_read_does_not_prevent_overrun` [3], `no_overrun_when_disabled` [4], `skres_clears_serial_errors` [5] |
| REQ-530 | `pokey.rs` `skstat_direct_serial_in` [1], `skstat_bit0_always_1` [2] |
| REQ-531 | `pokey.rs` `init_keeps_data_out_level` [1], `init_discards_pending_serout` [2], `clock_reset_keeps_shift_registers` [3], `two_tone_off_resets_tone_ff` [4], `stimer_keeps_tone_ff` [5] (new) |
| REQ-532 | `pokey.rs` `force_break_drives_low` [1], `force_break_keeps_timing` [2, 3] |
| REQ-533 | `pokey.rs` `two_tone_frequencies` [1], `two_tone_byte_55` [2] (new), `two_tone_resync_8_14_20` [3], `two_tone_resync_no_fire` [4], `two_tone_force_break_uses_timer2` [5], `two_tone_ignores_audc` [6] |
| REQ-534 | `pokey.rs` `potgo_starts_scan` [1], `slow_pot_latches_count` [2], `slow_pot_scan_end` [3], `pot_increment_phase` [4], `allpot_updates_each_cycle` [5]; `crates/harness/tests/pokey_paddle.rs` `basic_paddle_reads_host_value` (system check) |
| REQ-535 | `pokey.rs` `paddle_host_value_range` [1], `no_paddle_reads_228` [2], `grounded_pots_read_228_and_229` [3], `pot_change_during_scan` [4] |
| REQ-536 | `pokey.rs` `fast_pot_scan_end_231` [1], `fast_pot_paddle_count` [2], `fast_pot_no_dump` [3], `fast_pot_ignores_init_mode` [4] |
| REQ-537 | `pokey.rs` `truncated_pot_scan` [1], `suspended_pot_scan` [2], `restarted_scan_runs_full` [3], `restarted_scan_keeps_latched_input` [4] (new) |
| REQ-538 | `pokey.rs` `sample_count_ntsc` [1], `sample_count_pal` [2], `samples_in_range` [3], `drain_split_invariant` [4], `sample_count_other_rates` [5] |
| REQ-539 | `crates/harness/tests/pokey_audio.rs` `square_wave_level` [1], `volume_ratio` [2], `speaker_level` [3]; `pokey.rs` `ultrasonic_tone_averages` [4], `level_polarity` [5] |
| REQ-540 | `pokey.rs` `pokey_step_response` [1], `speaker_step_response` [2], `dc_decays` [3] |
| REQ-541 | `pokey.rs` `nonlinear_dac_sum` [1], `nonlinear_curve` [2], `nonlinear_step_gain` [3] (new), `nonlinear_off_at_power_on` [4] |

New tests:

- `offset_c_write_has_no_effect`: writes `$FF` to `$D20C` and `$D2FC`, and compares all reads and test accessor values with the values before the write.
- `register_map_matches_table`: writes a different value to each write offset, reads it back with `test_write_regs`, and checks each read offset against the REQ-500 table.
- `power_on_write_registers_zero`: checks `test_write_regs()` after `Pokey::new` for NTSC and PAL.
- `reset_equals_new`: runs the REQ-501 script, calls `reset`, and compares all reads and test accessor values with a new instance.
- `init_mode_holds_keyboard_scan`: holds the A key down for 300 x 114 cycles with `SKCTL` = `$00` and checks `IRQST`, `SKSTAT` and `KBCODE`.
- `init_mode_holds_serial_input`: sends a frame with `SKCTL` = `$20` and checks that `SERIN`, `IRQST` bit 5 and `SKSTAT` bit 1 do not change.
- `fast_clock_bits_select_timer`: checks the fire spacing of timers 1 and 3 with `AUDCTL` = `$40` and `$20`.
- `square_wave_period_4088`: checks the period of the channel 1 digital output for NTSC and PAL.
- `stimer_reloads_all_timers`: sets random counter values, writes `STIMER`, and checks the next fire cycles of all four timers.
- `distortion_poly17`: runs channel 3 for 131071 fires in 17-bit mode and counts the ones.
- `distortion_poly5_masks_noise`: runs `AUDC3` = `$2F`, `$0F` and `$4F` from the same start cycles and compares the fires that change the output.
- `output_zero_adds_nothing`: checks the mix level 7 of REQ-510 criterion 4 for 1000 cycles.
- `mix_level_maximum_60`: sets `AUDC1-4` = `$1F` and checks the mix level.
- `high_pass_period_4032`: checks that the channel 1 digital output repeats every 4032 cycles.
- `high_pass_channel_2`: checks the changes of the channel 2 digital output against the fire log of timers 2 and 4.
- `noise_delay_ch1_ch3`: compares the bits that channels 1 and 3 take on the same fires.
- `noise_follows_random_bit_0`: reads `RANDOM` on each cycle and compares bit 0 with the bit that channel 1 takes one cycle later.
- `break_irq_on_second_press`: presses BREAK, releases it, presses it again, and checks `IRQST` bit 7 after each press.
- `ascii_never_sets_control`: converts all 128 ASCII characters and checks that no value has bit 7 set.
- `serial_input_without_irq`: receives a frame with `IRQEN` = `$00` and checks `SERIN` and `IRQST`.
- `async_output_clock_stops`: checks `serial_clock_out` in modes `011` and `101` with and without a received frame.
- `bidirectional_clock_drive`: checks `serial_clock_in_drive` in all 8 modes.
- `async_first_timer4_fire`: checks the first timer 4 fire after a start edge and the spacing of the next fires.
- `stimer_keeps_tone_ff`: writes `STIMER` in two-tone mode and checks that `serial_out` does not change.
- `two_tone_byte_55`: sends `$55` in two-tone mode and checks the change spacing in each bit.
- `restarted_scan_keeps_latched_input`: restarts a scan after the input latched and checks `POT0` and `ALLPOT` bit 0.
- `nonlinear_step_gain`: compares the sample step with the option on and off.

The harness test `basic_sound_middle_c` types `SOUND 0,121,10,8` in BASIC. BASIC sets
`AUDF1` = 121 and `AUDC1` = `$A8`, so the tone is 1789773 / (122 x 28 x 2) = 262.0 Hz on
NTSC. The test accepts 261.5-262.5 Hz. The test `os_sio_receives_frame` needs the SIO bus
model of REQ-600 and later.

## Open issues

1. The HRM does not state if the low timer of a linked pair sets its own `IRQST` bit when it
   fires. This design latches it like an unlinked timer. Check with Altirra or real hardware.
2. The HRM does not state which level of the 5-bit noise lets an audio clock through. This
   design uses 1. A wrong choice changes only the phase of the masked sounds.
3. The HRM states that the 5-bit counter gives a constant 1 to the audio circuits in
   initialization mode, but also that all counters start zeroed. This design forces the
   5-bit audio bit to 1 during initialization mode and starts the pattern from zero.
4. The HRM gives no exact delay from the falling start edge to the start of timers 3 and 4 in
   asynchronous mode. This design uses the `STIMER` delay. REQ-526 has a tolerance of +/- 8
   cycles for this reason.
5. The keyboard scan pulse has the same cycle as the pot counter pulse (W + 81 + 114 k). The
   HRM states only that the 15 kHz clock drives both.
6. In keyboard state 2, the HRM does not state the transition when the key is down on a
   compare match. This design goes back to state 3 with no IRQ.
7. HRM 5.7 says that the serial output complete "interrupt" is set even if `IRQEN` bit 3 is 0.
   HRM 14.4 implies that the IRQ line needs `IRQEN` bit 3. This design uses HRM 14.4.
8. Phantom keys (HRM 5.8 Key conflicts) are not emulated. The 65XE matrix layout would be
   necessary for them.
9. The transitional values of a live pot counter read (HRM 5.9 Reading live counters) are not
   emulated. `POTn` reads return the stable count.
10. The half-cycle high-pass delay and the second-stage clamping (HRM E.5) are not emulated.
11. The 17-bit model clears completely 17 cycles after the entry to initialization mode. The
    HRM says "another nine cycles" after the first nine. No software is known to depend on
    the difference.
12. The 65XE keyboard is assumed to be the same as the 130XE keyboard of HRM Table 12.
13. The console speaker level (0.25) and the output gain are project decisions. Adjust them
    after a listening test against recordings from a real 65XE.
14. HRM 5.3 Reload timing says that with `AUDF1` = `AUDF2` = 0 in linked mode, "the second
    reload for both timers is at T+9". The linked period of N + 7 and Figure 8 give a reload
    on T+10 (last `AUDF` write on T+9). This design uses T+10. REQ-506 criterion 7 checks only
    the fire cycles, which do not depend on this choice.
15. Resolved by OI-029: a byte sent while force break is on is not reported by
    `take_serial_byte` (call contract item 7).
16. `Pokey::new` takes a sample rate of 8000 to 192000 Hz (REQ-538). The behavior for a rate
    outside this range (panic, clamp or error) is not decided.
