# GTIA design

## Scope

This document tells how the emulator satisfies the GTIA, palette and joystick requirements
REQ-400 to REQ-441. The source is the Altirra Hardware Reference Manual (HRM), chapters 6
and 7.1, and appendix D for the palette.

| Part | File | Requirements |
| --- | --- | --- |
| Registers, display, P/M graphics, collisions, priority, GTIA modes, I/O | `crates/atari-core/src/gtia.rs` | REQ-400 to REQ-408, REQ-413 to REQ-438, REQ-441 |
| Palette and optional display filters | `crates/atari-core/src/palette.rs` | REQ-409 to REQ-412 |
| Joystick wiring | `crates/atari-core/src/machine.rs` | REQ-439, REQ-440 |

The paddles (HRM 7.2) use POKEY and are not in this document.

## Interfaces

### Constants and types

```rust
/// Frame width in half color clocks (HPOS $20-$DF).
pub const FRAME_WIDTH: usize = 384;
/// Frame height in scanlines (scanlines 8-247).
pub const FRAME_HEIGHT: usize = 240;
/// The scanline that is row 0 of the frame buffer.
pub const FIRST_VISIBLE_LINE: u32 = 8;
/// The HPOS that is column pair 0 of the frame buffer.
pub const FIRST_FRAME_HPOS: u32 = 0x20;

/// One 3-bit code on the ANx bus from ANTIC to GTIA (HRM Table 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnCode(pub u8);

impl AnCode {
    pub const BACKGROUND: Self = Self(0b000);
    pub const VSYNC: Self = Self(0b001);
    pub const HBLANK_LORES: Self = Self(0b010);
    pub const HBLANK_HIRES: Self = Self(0b011);
    /// Low resolution: PF0-PF3. High resolution: pixel pair %00-%11 (AN1 = left pixel).
    pub const fn playfield(bits: u8) -> Self { Self(0b100 | (bits & 0b11)) }
}

/// The target of a P/M DMA byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmDma {
    /// The missile byte (ANTIC cycle 0).
    Missiles,
    /// The byte for player 0-3 (ANTIC cycles 2-5).
    Player(u8),
}

/// What ANTIC gives GTIA in one machine cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GtiaCycle {
    /// The scanline, 0 to 261 (NTSC) or 0 to 311 (PAL).
    pub line: u32,
    /// The machine cycle in the scanline, 0 to 113.
    pub cycle: u32,
    /// The ANx codes for HPOS (2 * cycle - 4) mod 228 and (2 * cycle - 3) mod 228.
    pub an: [AnCode; 2],
    /// A P/M DMA byte that ANTIC fetched in this cycle, if any.
    pub pm_dma: Option<(PmDma, u8)>,
    /// The data bus value in this cycle (for REQ-441).
    pub bus: u8,
}

/// The state of the three console keys. `true` = pressed.
pub struct ConsoleKeys { pub start: bool, pub select: bool, pub option: bool }
```

### Gtia methods

```rust
impl Gtia {
    /// Creates GTIA in the power-on state (REQ-402). `standard` sets PAL (REQ-403).
    pub fn new(standard: VideoStandard) -> Self;
    /// Power-on reset (REQ-402). The machine does not call it for the RESET key.
    pub fn reset(&mut self);
    /// CPU read. `reg` is `addr & 0x1F` (REQ-400). No side effects (REQ-401).
    pub fn read(&self, reg: u8) -> u8;
    /// CPU write. `reg` is `addr & 0x1F`. Applies in the cycle of the last `tick`.
    pub fn write(&mut self, reg: u8, value: u8);
    /// One machine cycle of display from ANTIC.
    pub fn tick(&mut self, input: &GtiaCycle);
    /// The last complete frame (REQ-405).
    pub fn frame(&self) -> &[u8];
    /// Trigger input `index` 0-3. `true` = active (the register reads 0).
    pub fn set_trigger(&mut self, index: usize, active: bool);
    /// Console key inputs (REQ-437).
    pub fn set_console_keys(&mut self, keys: ConsoleKeys);
    /// The speaker line: `CONSOL` bit 3 of the last write (REQ-438).
    pub fn speaker(&self) -> bool;
}
```

### Palette functions

```rust
/// Palette parameters. `Default` gives the values of REQ-409.
pub struct PaletteParams { pub hue_step_deg: f64, pub saturation: f64 }

/// The 256 RGB values (`0x00RRGGBB`) for a standard, computed once.
pub fn table(standard: VideoStandard) -> &'static [u32; 256];
/// One color (a lookup in `table`).
pub fn rgb(standard: VideoStandard, color: u8) -> u32;
/// The palette for other parameters (for tests and a later settings screen).
pub fn build(standard: VideoStandard, params: &PaletteParams) -> [u32; 256];

/// Optional display filters (Could). They work on RGB rows, not on the frame buffer.
pub fn artifact_ntsc_row(row: &[u8], out: &mut [u32]);
pub fn pal_blend_row(prev: &[u8], row: &[u8], out: &mut [u32]);
```

### Call contract

1. `Antic::tick` calls `Gtia::tick` exactly once for each machine cycle, on all scanlines,
   also in vertical blank. ANTIC calls it before the CPU bus access of that cycle.
2. The machine calls `Gtia::read` and `Gtia::write` after `Antic::tick` for the same cycle.
   The draft `System::begin_cpu_cycle` already has this order. GTIA uses the `cycle` of the
   last `tick` as the cycle of the access.
3. In `tick`, GTIA first stores the P/M DMA byte, then makes the two color clocks.
4. GTIA copies the back buffer to the front buffer at the first `tick` of scanline 248.
   This is the same cycle in which ANTIC sets `CycleOutput::frame_done`.
5. The machine calls `set_trigger(3, !rd5)` at power-on and after each access that can
   change the cartridge RD5 line (REQ-435).
6. After each `CONSOL` write, the machine gives `speaker()` to the audio mixer (REQ-438).
7. `Machine::set_joystick(port, state)` sets `PORTA` input bits through the PIA and calls
   `set_trigger(port, state.fire)` (REQ-439, REQ-440).

### What GTIA needs from ANTIC

| Item | Contract |
| --- | --- |
| Call rate | One `tick` per machine cycle, all 114 cycles of all scanlines. |
| HPOS alignment | `an[0]` is HPOS `(2c - 4) mod 228` (always even). `an[1]` is the next HPOS. |
| Active region | Code `%000` or `%1xx` for each color clock between the horizontal blanks of scanlines 8-247. |
| Playfield position | Narrow `$40-$BF`, normal `$30-$CF`, wide `$20-$DF` with `$20-$2B` sent as `%000` (HRM 6.2). |
| Horizontal blank | Code `%010` or `%011` for the hires state of the next scanline (HRM 6.8, 6.9). |
| Vertical blank | Horizontal blank codes for the full scanline. `%001` on the vertical sync scanlines. |
| P/M DMA | `pm_dma` = `Missiles` on cycle 0 and `Player(0..=3)` on cycles 2-5, only when `DMACTL` enables the fetch, on scanlines 8-247 (HRM 4.13). |
| Bus value | `bus` = the data bus value of the cycle (only for REQ-441). |
| Scanline number | The same scanline count as ANTIC uses for `VCOUNT`. GTIA uses bit 0 for `VDELAY`. |

## State

All values are 0 at power-on (REQ-402), except where the table gives a value.

| Field | Type | Contents |
| --- | --- | --- |
| `standard` | `VideoStandard` | NTSC or PAL. Fixed. |
| `hposp`, `hposm` | `[u8; 4]` each | Positions. |
| `sizep` | `[u8; 4]` | Player sizes, bits 0-1. |
| `sizem` | `u8` | Missile sizes. |
| `grafp`, `grafm` | `[u8; 4]`, `u8` | Graphics latches. |
| `colpm`, `colpf` | `[u8; 4]` each | Color registers, bit 0 cleared. |
| `colbk` | `u8` | Background color, bit 0 cleared. |
| `prior` | `u8` | Priority control in effect. |
| `vdelay`, `gractl` | `u8` each | Vertical delay, graphics control. |
| `consol_out` | `u8` | Last `CONSOL` write, bits 0-3. |
| `pending` | fixed ring of 16 `PendingWrite` | Writes that are not in effect yet. |
| `shift` | `[u8; 8]` | Shift registers: players 0-3, then missiles 0-3. |
| `shift_state` | `[u8; 8]` | 2-bit shift states. |
| `collisions` | `[u8; 16]` | `M0PF-P3PL`, bits 0-3. |
| `hires` | `bool` | The hires flip-flop. |
| `gtia_pixel` | `u8` | The last 4-bit GTIA mode pixel (for the mode 10 delay). |
| `trig_input`, `trig_latch` | `[bool; 4]` each | Trigger inputs and latches. `trig_input[2]` is always `false`. |
| `console` | `ConsoleKeys` | Key inputs. |
| `clock` | `u64` | Absolute color clock count, for `pending`. |
| `line`, `cycle` | `u32` each | The values of the last `tick`. |
| `back`, `front` | `Vec<u8>` of 92160 each | Frame buffers. |
| `priority` | `Box<[u16; 64 * 512]>` | Priority table (see "Priority"). Built in `new`. |

`PendingWrite` holds the absolute color clock at which the write takes effect, the
register and the value. `PRIOR` makes two entries: bits 0-5 and bits 6-7.

## Behavior

### Registers (REQ-400 to REQ-404)

- `read` decodes `reg & 0x1F` with the read column of HRM Table 13. Collision registers
  return their 4 bits. `TRIGn` returns `u8::from(!(trig_input[n] || trig_latch[n]))`.
  `PAL` returns `$0F` (NTSC) or `$01` (PAL). `$15-$1E` return `$0F`.
- `CONSOL` read: `0x0F & !consol_out & !keys`, where `keys` has bit 0 for START, bit 1 for
  SELECT and bit 2 for OPTION when pressed (REQ-437).
- `write` puts `HPOSxx`, `SIZEx`, `GRAFx`, color registers and `PRIOR` into `pending`
  (see "Timing"). `VDELAY`, `GRACTL`, `HITCLR` and `CONSOL` take effect at once.
- Color register writes store `value & 0xFE` (REQ-404).
- A `GRACTL` write with bit 2 = 1 sets `trig_latch[n] |= trig_input[n]`. A write with
  bit 2 = 0 clears all `trig_latch` (REQ-436). `set_trigger` also sets the latch when the
  input is active and `GRACTL` bit 2 = 1.

### Color clock pipeline

`tick` makes two color clocks: HPOS `h0 = (2c - 4) mod 228` and `h0 + 1`. For each color
clock, GTIA does these steps in this order:

1. Apply each `pending` write whose time is at or before this color clock.
2. Run the P/M shifters (see "Players and missiles"). This gives 8 object bits.
3. Decode the ANx code into playfield inputs (see the next sections).
4. Set collision bits, if the color clock is in the collision area (REQ-422).
5. Look up the priority table and make the color byte.
6. Write the two frame buffer bytes, if the scanline is 8-247 and HPOS is `$20-$DF`.

GTIA modes need both codes of a pixel pair. The pair is always `an[0]` (even HPOS) and
`an[1]` (odd HPOS) of the same `tick`, so GTIA can form the pixel before step 3 of the
even color clock.

### Playfield decode (REQ-407, REQ-408)

- A horizontal blank code sets `hires` to `code == %011`. The color clock is blank.
- Code `%001` is blank.
- Low resolution: `%000` gives no playfield input (background). `%1nn` gives input `PFnn`.
- High resolution (`hires` and `PRIOR` bits 6-7 = `%00`): `%1xx` gives input `PF2`. The
  collision logic gets `PF2` only if AN1 or AN0 is 1. After the priority step, each half
  with bit 1 gets `(color & 0xF0) | (COLPF1 & 0x0E)`. AN1 is the left half.

### Frame buffer (REQ-405, REQ-406)

- Column `2 * (h - 0x20)` gets the left half, the next column gets the right half.
- HPOS `$20`, `$21`, `$DE` and `$DF` get `$00`.
- Blank codes in HPOS `$22-$DD` on scanlines 8-247 also give `$00`.

### Players and missiles (REQ-413 to REQ-419, REQ-441)

- DMA: a `Missiles` byte goes to `grafm` if `GRACTL` bit 0 = 1. A `Player(n)` byte goes to
  `grafp[n]` if `GRACTL` bit 1 = 1. If the `VDELAY` bit of an object is 1 and `line` is
  even, the object keeps its old value. GTIA handles a DMA byte as a write on the cycle of
  the `tick`, with the delay of `GRAFx` (3 color clocks).
- REQ-441: if `GRACTL` bit 1 = 1 and ANTIC gives a `Missiles` byte but no player bytes, GTIA
  loads `grafp[n]` from `bus` on cycles 2-5.
- Per color clock and per object: if `h == position`, then `shift |= latch` and
  `state = 0`. A missile latch is its 2 bits in bits 7-6. The object bit is bit 7 of
  `shift`. Then `state = (state + 1) & size`, and if `state == 0`, `shift <<= 1`.
- The shifters run on all color clocks of all scanlines (REQ-418).

### Collisions (REQ-420 to REQ-423)

- The collision area is HPOS `$22-$DD` on scanlines 8-247.
- `PnPF |= pf_collision_bits`, `MnPF |= pf_collision_bits`, for each active object.
- `PnPL |= players & !(1 << n)`. `MnPL |= players`.
- `pf_collision_bits`: low resolution codes `PF0-PF3`; hires `PF2` if a pixel bit is 1;
  mode 10 codes `%x100-%x111`; none in modes 9 and 11.
- The fifth player and multicolor bits do not change the collision logic.
- `HITCLR` clears `collisions` at once.

### Priority (REQ-424 to REQ-427)

GTIA builds a table in `new`. The index is `PRIOR` bits 0-5 (64 values) and 9 input bits:
`P0-P3`, `PF0-PF3` and `M5` (any missile while `PRIOR` bit 4 = 1). Each entry is a 9-bit
mask: `COLPM0-3`, `COLPF0-3`, `COLBK`. The builder uses the equations of HRM 6.7:

- `P0-P3` = player n OR (missile n AND `PRIOR` bit 4 = 0).
- `PF3` input = playfield `PF3` OR `M5`.
- `SF0-SF2` also need `/M5` (project decision, see REQ-426).
- The color is the OR of the registers in the mask, or `$00` for an empty mask.

The same builder function is the reference for the unit tests against HRM Table 16.

### GTIA modes (REQ-428 to REQ-433)

- When `PRIOR` bits 6-7 are not `%00`, `hires` is false. A change of bits 6-7 from `%00`
  to another value clears `hires` until the next `%011` code (REQ-432).
- Pixel `p = ((an[0] & 3) << 2) | (an[1] & 3)`.
- Mode 9: no playfield input. If `SB` is in the mask, the color is `COLBK | p`. If the
  fifth player is selected, the color is `COLPF3 | p`.
- Mode 11: no playfield input. If `SB` is in the mask, the color is `COLBK | (p << 4)`.
  If the fifth player is selected, it is `COLPF3 | (p << 4)`. For `p = 0`, the
  luminance bits are cleared.
- Mode 10: the even color clock uses `gtia_pixel` (the last pixel) and the odd color
  clock uses `p`. Then `gtia_pixel = p`. `p` in 0-3 sets the input `Pp`. `p` in 4-7 or
  12-15 sets `PF(p & 3)`. `p` in 8-11 sets nothing. If `an[1]` is `%000`, `p` is 8
  (REQ-433).

### Palette (REQ-409 to REQ-412)

`build` computes for each byte `c`: `H = c >> 4`, `L = c & 0x0F`, `Y = L / 15`. For
`H = 0`, `U = V = 0`. Otherwise the angle is `180° - step * (H - 1)` (NTSC) or the HRM
Table 14 ideal UV angle (PAL), and `U = S cos(angle)`, `V = S sin(angle)`. Then
`R = Y + 1.140 V`, `G = Y - 0.395 U - 0.581 V`, `B = Y + 2.032 U`. Each channel is
clamped to 0-1 and converted with `(255 * x).round()`. The defaults are `step = 24°` and
`S = 0.233`. `table` keeps the two default tables in a `std::sync::OnceLock`.

The filters of REQ-411 and REQ-412 are off by default. The frontend calls them on the RGB
output. They do not change the frame buffer.

## Timing

- Machine cycle `c` of a scanline covers GTIA color clocks `2c - 4` and `2c - 3`
  (mod 228). The base time of a CPU write in cycle `c` is color clock `2c - 2`.
- A write takes effect at `base + d` (HRM 6.10, REQ-434):

| Register | d | Write on cycle 65 shows at |
| --- | --- | --- |
| `COLPM0-3`, `COLPF0-3`, `COLBK` | 1 | `$81` |
| `PRIOR` bits 0-5 | 2 | `$82` |
| `GRAFP0-3`, `GRAFM`, P/M DMA | 3 | `$83` |
| `PRIOR` bits 6-7 | 4 | `$84` |
| `HPOSP0-3`, `HPOSM0-3`, `SIZEP0-3`, `SIZEM` | 5 | `$85` |
| `VDELAY`, `GRACTL`, `HITCLR`, `CONSOL` | 0 | at once |

- A write from cycle `c` takes effect at color clock `2c - 1` or later. GTIA makes that
  color clock in `tick(c + 1)`, so `pending` always has the write in time.
- The P/M DMA bytes come in cycles 0 and 2-5, in horizontal blank, so they are in the
  latches before the visible region.
- The front buffer changes at the first `tick` of scanline 248.

## Test plan

Unit tests are in `crates/atari-core/src/gtia.rs` and `crates/atari-core/src/palette.rs`
(`#[cfg(test)] mod tests`). A GTIA unit test makes a `Gtia`, writes registers, and calls
`tick` with synthetic `GtiaCycle` values. System tests are in `crates/harness/tests/`. They
run a small 6502 program on the full machine and check the frame buffer or memory.

| Requirement | Test |
| --- | --- |
| REQ-400 | `crates/harness/tests/gtia.rs` `registers_mirror_at_d020_d0ff` |
| REQ-401 | `gtia.rs` `reads_use_read_registers`, `writes_use_write_registers`, `unused_read_bits_are_zero`, `unassigned_reads_return_0f`, `reads_have_no_side_effects` |
| REQ-402 | `gtia.rs` `power_on_state_is_zero`, `power_on_trigger_latches_clear`; `crates/harness/tests/gtia.rs` `reset_key_keeps_gtia_state` |
| REQ-403 | `gtia.rs` `pal_register_ntsc_0f_pal_01` |
| REQ-404 | `gtia.rs` `color_registers_ignore_bit_0`, `color_change_applies_to_later_lines` |
| REQ-405 | `gtia.rs` `frame_buffer_size_and_mapping`, `frame_swaps_at_line_248` |
| REQ-406 | `gtia.rs` `blank_columns_are_zero`, `blank_columns_ignore_playfield_codes`, `players_show_from_hpos_22` |
| REQ-407 | `gtia.rs` `lores_codes_map_to_color_registers`; `crates/harness/tests/gtia.rs` `mode_e_colors` |
| REQ-408 | `gtia.rs` `hires_pixel_order`, `hires_luminance_over_players`, `hires_priority_sees_pf2`, `hblank_code_sets_hires_state`; `crates/harness/tests/gtia.rs` `mode_f_hires_colors` |
| REQ-409 | `palette.rs` `ntsc_grays`, `ntsc_reference_colors`, `ntsc_round_trip_yuv`, `ntsc_build_matches_table` |
| REQ-410 | `palette.rs` `pal_grays`, `pal_reference_colors`, `pal_round_trip_angles`, `pal_hue_1_equals_hue_15` |
| REQ-411 | `palette.rs` `artifact_alternating_rows_are_colored`, `artifact_luma_is_average`, `artifact_solid_row_unchanged`; `src/frontend.rs` `display_filters_off_by_default` |
| REQ-412 | `palette.rs` `pal_blend_formula`, `pal_blend_reference_rows`; `src/frontend.rs` `display_filters_off_by_default` |
| REQ-413 | `gtia.rs` `player_bits_msb_first`, `missile_bits_and_colors`, `all_object_colors`, `latch_repeats_without_dma` |
| REQ-414 | `gtia.rs` `gractl_gates_dma`, `gractl_off_keeps_latch`, `cpu_writes_ignore_gractl`; `crates/harness/tests/gtia.rs` `pm_dma_one_line_resolution` |
| REQ-415 | `gtia.rs` `vdelay_bit_map`, `vdelay_accepts_odd_lines_only`, `vdelay_ignores_cpu_writes` |
| REQ-416 | `gtia.rs` `player_starts_at_hpos`, `player_clipped_at_left_and_right` |
| REQ-417 | `gtia.rs` `player_sizes`, `missile_sizes`, `size_bits_2_7_ignored` |
| REQ-418 | `gtia.rs` `retrigger_shows_second_copy`, `move_left_suppresses_image`, `move_during_image_keeps_image`, `reload_ors_shift_register`, `image_shifts_out_in_hblank` |
| REQ-419 | `gtia.rs` `size_change_table_15`, `size_10_lockup_persists`, `size_change_after_image` |
| REQ-420 | `gtia.rs` `pm_playfield_collisions`, `hires_collisions_use_pf2`, `gtia_mode_playfield_collisions`, `collisions_ignore_colors_and_priority` |
| REQ-421 | `gtia.rs` `player_player_collisions`, `missile_player_collisions`, `no_missile_missile_collisions`, `no_self_collision`, `same_color_players_collide` |
| REQ-422 | `gtia.rs` `collisions_only_in_visible_area` |
| REQ-423 | `gtia.rs` `hitclr_clears_all`, `collisions_latch_until_hitclr` |
| REQ-424 | `gtia.rs` `priority_equations_all_inputs`, `standard_priority_modes` |
| REQ-425 | `gtia.rs` `priority_mode_0_mixes`, `priority_table_16_without_p5` |
| REQ-426 | `gtia.rs` `fifth_player_uses_colpf3`, `fifth_player_keeps_collisions`, `fifth_player_keeps_position_and_size`, `priority_table_16_p5_rows` |
| REQ-427 | `gtia.rs` `multicolor_players_or_colors`, `multicolor_missiles`, `multicolor_keeps_collisions` |
| REQ-428 | `gtia.rs` `gtia_pixels_pair_even_hpos`, `gtia_mode_ignores_an2`, `gtia_mode_ignores_hires`; `crates/harness/tests/gtia.rs` `gtia_mode_pixels_from_antic` |
| REQ-429 | `gtia.rs` `mode_9_luminance`, `mode_9_players_on_top`, `mode_9_fifth_player`, `mode_9_no_playfield_collisions` |
| REQ-430 | `gtia.rs` `mode_10_register_map`, `mode_10_delay_and_border`, `mode_10_wide_right_edge`, `mode_10_collisions` |
| REQ-431 | `gtia.rs` `mode_11_hues`, `mode_11_players_on_top`, `mode_11_fifth_player`, `mode_11_no_playfield_collisions` |
| REQ-432 | `gtia.rs` `pseudo_mode_e_after_mid_line_prior` |
| REQ-433 | `gtia.rs` `mode_10_bak_mutes_pixel` |
| REQ-434 | `gtia.rs` `write_delays_table_18`, `write_delay_base_other_cycle`; `crates/harness/tests/gtia.rs` `mid_line_colbk_change` |
| REQ-435 | `gtia.rs` `trigger_reads`; `crates/harness/tests/gtia.rs` `trig2_always_inactive`, `trig3_follows_cartridge` |
| REQ-436 | `gtia.rs` `trigger_latch_holds_until_cleared`, `trigger_latch_all_triggers` |
| REQ-437 | `gtia.rs` `consol_keys_and_outputs` |
| REQ-438 | `gtia.rs` `speaker_follows_consol_bit_3`; `crates/harness/tests/gtia.rs` `key_click_makes_audio` |
| REQ-439 | `crates/harness/tests/joystick.rs` `joystick_directions_on_porta` |
| REQ-440 | `crates/harness/tests/joystick.rs` `joystick_buttons_on_trig0_trig1` |
| REQ-441 | `gtia.rs` `players_load_bus_without_player_dma` |

The tests that this list adds or changes do these checks:

- `crates/harness/tests/gtia.rs` `registers_mirror_at_d020_d0ff`: a 6502 program writes `$34` to `$D0FA`, and the test checks the background color. The test compares `Machine::peek` of each address `$D020-$D0FF` with `$D000-$D01F`, with collisions set, the port 1 button pressed and released, and each console key combination.
- `writes_use_write_registers`: does the write map of REQ-401 criterion 2, one write for each address `$D000-$D01B`.
- `power_on_trigger_latches_clear`: after `new`, writes `$04` to `GRACTL` and reads `$01` from `TRIG0-TRIG3`.
- `crates/harness/tests/gtia.rs` `reset_key_keeps_gtia_state` (was `reset_key_keeps_collisions`): sets a collision and `COLBK` = `$46`, calls `press_reset`, and checks the collision register and the background color.
- `color_change_applies_to_later_lines`: changes `COLPF0` in horizontal blank of scanline 100 and checks rows 0-91 and 92-239.
- `blank_columns_ignore_playfield_codes`: sends `%000` and `%100` at HPOS `$20-$21` and `$DE-$DF` and checks that columns 0-3 and 380-383 are `$00`.
- `hires_pixel_order`: sends codes `%100-%111` on a hires line and checks the two columns of each.
- `hires_priority_sees_pf2`: `PRIOR` = `$04`, player 0 on code `%100`, checks `COLPF2` in both columns.
- `crates/harness/tests/gtia.rs` `mode_f_hires_colors`: an ANTIC mode F line with first byte `$AA` and checks columns 32-39.
- `ntsc_round_trip_yuv`, `pal_round_trip_angles`: convert each unclamped RGB value back with the HRM D.2 inverse matrix and check Y, the chroma angle and the magnitude.
- `ntsc_build_matches_table`: compares `build(NTSC, &PaletteParams::default())` with `table(NTSC)`.
- `artifact_alternating_rows_are_colored`, `artifact_luma_is_average`, `artifact_solid_row_unchanged`: call `artifact_ntsc_row` on the rows of REQ-411 and check the mean colors.
- `pal_blend_formula`, `pal_blend_reference_rows`: call `pal_blend_row` and compare with the formula and the RGB values of REQ-412.
- `src/frontend.rs` `display_filters_off_by_default`: converts a frame with the default frontend settings and compares each pixel with `palette::rgb`.
- `all_object_colors`: players 1-3 and missiles 2-3 at different positions show `COLPM1-3`.
- `gractl_off_keeps_latch`: writes `$00` to `GRACTL` in cycle 110 of scanline 99 and checks rows 92-239.
- `vdelay_bit_map`: sets each `VDELAY` bit alone and checks that only the related object skips the even-scanline DMA byte.
- `size_bits_2_7_ignored`: compares the rows for `SIZEP0` = `$FC` and `$00`, and `$FD` and `$01`.
- `move_during_image_keeps_image`, `image_shifts_out_in_hblank`: REQ-418 criteria 3 and 5.
- `size_change_after_image`: changes `SIZEP0` after the image and compares the row with a row with no write.
- `collisions_ignore_colors_and_priority`: all color registers `$00`, `PRIOR` = `$04`, player 0 on `PF0`, checks `P0PF` = `$01`.
- `no_self_collision`, `same_color_players_collide`: REQ-421 criteria 4 and 5.
- `priority_equations_all_inputs`: compares the priority table with an evaluation of the HRM 6.7 equations that the test code does itself, for all `PRIOR` values with bit 4 = 0 and all inputs.
- `fifth_player_keeps_position_and_size`: missiles 0 and 3 at different positions and sizes with `PRIOR` = `$10`.
- `multicolor_missiles`, `multicolor_keeps_collisions`: REQ-427 criteria 3-6.
- `gtia_mode_ignores_an2`: code pairs (`%000`, `%101`) and (`%100`, `%101`) give the same pixel.
- `crates/harness/tests/gtia.rs` `gtia_mode_pixels_from_antic`: ANTIC mode F and mode E lines with `PRIOR` = `$40` (REQ-428 criteria 1 and 2).
- `mode_9_no_playfield_collisions`, `mode_11_players_on_top`, `mode_11_no_playfield_collisions`: REQ-429 criterion 6, REQ-431 criteria 3 and 5.
- `mode_10_wide_right_edge`: a wide mode F line with `PRIOR` = `$80`, checks HPOS `$2C-$DD`.
- `write_delay_base_other_cycle`: a `COLBK` write on cycle 30 changes the color from HPOS `$3B`.
- `crates/harness/tests/gtia.rs` `trig2_always_inactive`: reads `TRIG2` after boot, with both joystick buttons pressed, and with `GRACTL` = `$04`.
- `trigger_latch_all_triggers`: latches triggers 0, 1 and 3, and latches an input that is active when `GRACTL` bit 2 is set.
- `players_load_bus_without_player_dma` (was a system test): feeds `GtiaCycle` values with a missile DMA byte and bus values on cycles 2-5, and checks the player images.

## Open issues

1. Fifth player priority. The HRM gives no equations for the fifth player. The model in
   "Priority" disagrees with three cells of HRM Table 16 and with the HRM text for
   `PRIOR` = `$18` (REQ-426). A test on real hardware or the Altirra source can settle it.
2. Palette values. `step = 24°` and `S = 0.233` are choices (REQ-409). The palette does not
   model the close luminance levels 7 and 8, the blacker-than-black level 0, NTSC gamma,
   NTSC to sRGB color correction or a tint control (HRM D.2).
3. PAL palette. Real PAL machines often use a delay of 18-19° per step, with different
   colors on even and odd lines (HRM D.4). The emulator uses the ideal angles only. The PAL
   saturation is not in the HRM.
4. Hires bug in vertical blank (HRM 6.2). GTIA takes the line and cycle from ANTIC. It does
   not model its own horizontal counter, so it cannot show shifted horizontal sync. Codes
   that ANTIC sends on scanlines 248-7 are not in the frame buffer.
5. With all P/M DMA off in ANTIC and `GRACTL` on, GTIA takes a display list fetch as the
   missile byte and loads players at cycles 3-7 (HRM 6.5). This needs a halt signal from
   ANTIC and is not designed.
6. Write delays with a range in the HRM (`PRIOR` bit 4: 2-3, bits 6-7: 3-5) use one fixed
   value. The transition artifacts of GTIA mode changes (HRM 6.10) are not modeled.
7. The read value of `$D015-$D01E` on a PAL GTIA, the `CONSOL` bit 3 read value, the way the
   trigger latch is cleared and the delay of `HITCLR` are project decisions. The HRM does
   not state them.
8. HRM inconsistencies: HRM 6.5 gives `HPOSP0-3` at `[D400-D403]`. HRM 14.3 and Table 13 give
   `$D000-$D003`. The emulator uses `$D000`. HRM 14.3 gives the wide playfield as `$22-$DD`.
   HRM 4.3 and 6.2 give `$2C-$DD` for the visible playfield. ANTIC sets the clip.
9. The HPOS alignment `2c - 4` in `GtiaCycle` must agree with the ANTIC design
   (`docs/design/antic.md`).
10. The frontend has no setting to turn on the display filters of REQ-411 and REQ-412
    (`docs/design/architecture.md`). The test `display_filters_off_by_default` needs the setting.
11. REQ-412 does not give the output of `pal_blend_row` for row 0, which has no previous row.
12. The system tests in `crates/harness/tests/gtia.rs` run small 6502 programs, some with exact
    cycles (`mid_line_colbk_change`, `registers_mirror_at_d020_d0ff`). `test-harness.md` has no
    helper to load and start such a program. The design must add one, or the tests must use an
    OS vector (for example the deferred vertical blank vector) or a display list interrupt.
13. REQ-411 names PAL artifacting (HRM D.5) in its Source, but the design has only
    `artifact_ntsc_row`.
