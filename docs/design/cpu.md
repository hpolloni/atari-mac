# CPU design

## Scope

This document tells how the emulator satisfies the CPU requirements REQ-100 to REQ-128. The
CPU is the NMOS 6502 core of the 6502C in the Atari 65XE (HRM chapter 3). The 65C02 and the
65C816 are out of scope (HRM 3.6 to 3.8).

The CPU is in `crates/atari-core/src/cpu.rs`. It has no dependency on the other chips. It
sees the system only through the `Bus` trait and three input lines: NMI, IRQ and RDY. The
machine (see [system.md](system.md)) decides in which machine cycles the CPU runs.

| Requirement | Topic |
| --- | --- |
| REQ-100 to REQ-106 | Instructions, bus cycles, page crossing, RMW, wraps, branches |
| REQ-107 to REQ-109 | Flags and decimal mode |
| REQ-110 | Klaus Dormann functional test |
| REQ-111 to REQ-113 | Undocumented and KIL opcodes |
| REQ-114 to REQ-123 | BRK, IRQ, NMI and the interrupt timing effects |
| REQ-124, REQ-128 | Reset and power-on |
| REQ-125 to REQ-127 | Halt by ANTIC DMA, stop by RDY |

## Interfaces

### Bus trait

```rust
/// The CPU's view of the system. Each call is the bus access of one CPU cycle.
pub trait Bus {
    /// Reads one byte, with the side effects of a real read.
    fn read(&mut self, addr: u16) -> u8;
    /// Writes one byte.
    fn write(&mut self, addr: u16, value: u8);
}
```

Contract:

1. The CPU calls `read` or `write` exactly once in each call of `Cpu::tick` (REQ-101).
2. The bus does not advance the machine clock. The machine advances the clock and calls
   `Cpu::tick` once for each machine cycle in which the CPU has the bus (REQ-125).
3. The bus sends every access to the device at that address, also dummy accesses (REQ-203).

This contract is different from the draft skeleton, where `Bus::read` advances the chips.
See "Open issues".

### Cpu type

```rust
pub struct Cpu { /* see State */ }

/// The programmer-visible registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Registers {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub s: u8,
    /// Processor status. The CPU never changes bits 4 and 5: they are 1 after power-on and
    /// keep the value that `registers_mut` writes (the SingleStepTests data needs this).
    /// `PHP` and interrupts push them as REQ-107 states.
    pub p: u8,
    pub pc: u16,
}

/// The result of `Cpu::step`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// One instruction executed.
    Instruction,
    /// One interrupt sequence (NMI or IRQ, 7 cycles) executed.
    Interrupt,
    /// The reset sequence (7 cycles) executed.
    Reset,
    /// The CPU is locked by a KIL opcode. One cycle executed.
    Jammed,
}

/// What the CPU did in one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    /// A normal cycle.
    Ran,
    /// RDY was low and the cycle was a read: the read was repeated and the CPU did not advance.
    Stalled,
}

impl Cpu {
    /// Creates the CPU in the power-on state (REQ-128). A reset is pending, so the first
    /// 7 ticks run the reset sequence (REQ-124).
    pub const fn new() -> Self;

    /// Asserts the reset input. The next tick starts the 7-cycle reset sequence.
    /// It also ends the KIL state and clears a pending NMI edge (REQ-113, REQ-124).
    pub fn reset(&mut self);

    /// Runs one CPU cycle. It makes exactly one call to `bus.read` or `bus.write`.
    /// `rdy` is the level of the RDY input in this cycle (`false` = low, CPU stopped).
    pub fn tick(&mut self, bus: &mut impl Bus, rdy: bool) -> Tick;

    /// Sets the level of the NMI input (`true` = asserted). The CPU detects the edge.
    /// Ends machine cycle `c`: latches the NMI edge and makes the poll of cycle `c` (OI-017).
    pub fn end_cycle(&mut self, nmi_level: bool);

    /// Sets the level of the IRQ input (`true` = asserted).
    pub fn set_irq(&mut self, asserted: bool);

    /// `true` if the next tick is the first cycle of an instruction, an interrupt
    /// sequence or the reset sequence.
    pub const fn at_boundary(&self) -> bool;

    /// Runs ticks with `rdy = true` until the next boundary. For tests and debuggers.
    pub fn step(&mut self, bus: &mut impl Bus) -> Step;

    pub const fn registers(&self) -> &Registers;
    pub const fn registers_mut(&mut self) -> &mut Registers;
    pub const fn is_jammed(&self) -> bool;
}
```

### Call contract

The machine calls the CPU in this order in each machine cycle `c` (see
[system.md](system.md), "Timing"):

1. `set_irq` with the IRQ line level for cycle `c`. "The IRQ line is asserted in cycle N"
   in the requirements means that the machine passes `true` before the tick of cycle N.
2. `tick(bus, rdy)` if ANTIC does not use the bus in cycle `c`. If ANTIC uses the bus, the
   machine does not call `tick` (REQ-125).
3. `end_cycle(nmi_level)` in every cycle, also in a halted cycle. "The NMI line is asserted in
   cycle N" means that the machine passes `true` to `end_cycle` of cycle N. The NMI edge
   latch, the copy to `nmi_pending` and the interrupt poll of cycle `c` occur in
   `end_cycle`, after the bus access (OI-017, HRM 3.4 Figure 2).

`registers_mut` is permitted only at a boundary. The SingleStepTests harness uses it to load
the initial state. It then calls `tick` exactly once for each entry of the `cycles` list.

## State

| Field | Type | Power-on value | Purpose |
| --- | --- | --- | --- |
| `regs` | `Registers` | all 0, `p` = `$30` | Registers. `p` stores only bits 0-3, 6, 7 (REQ-107, REQ-128). |
| `opcode` | `u8` | 0 | Opcode of the current instruction. |
| `cycle` | `u8` | 0 | Cycle number in the current instruction or sequence, from 0. |
| `mode` | `Mode` | `Reset` | `Fetch`, `Execute`, `Interrupt`, `Reset` or `Jammed`. |
| `addr` | `u16` | 0 | Effective address under construction. |
| `base_hi` | `u8` | 0 | High byte of the base address, for page cross and `SHA`/`SHX`/`SHY`/`SHS`. |
| `data` | `u8` | 0 | Operand or RMW value. |
| `nmi_level` | `bool` | false | Last level from `end_cycle`. |
| `nmi_edge` | `bool` | false | Edge latched by `end_cycle`, not yet sampled. |
| `nmi_pending` | `bool` | false | Edge sampled in a tick, not yet serviced. |
| `irq_level` | `bool` | false | Level from `set_irq`. |
| `poll` | `bool` | false | Result of the interrupt poll of this instruction. |
| `poll_nmi` | `bool` | false | `true` if the poll found an NMI. |
| `vector` | `u16` | `$FFFC` | Vector of the current sequence. |
| `brk` | `bool` | false | The current sequence is a `BRK`. |
| `no_poll_next` | `bool` | false | Skip the interrupt check at the next boundary (REQ-123). |
| `jam_cycle` | `u8` | 0 | Cycle count in the KIL state, for the SingleStepTests pattern. |

`Mode::Reset` at power-on makes the first 7 ticks the reset sequence. After it, `S` =
`$FD` (REQ-128).

## Behavior

### Instruction decode (REQ-100 to REQ-106, REQ-111, REQ-112)

A table of 256 entries gives the operation and the addressing mode of each opcode (HRM 3.5
Table 2). The operations are in groups by bus pattern:

| Group | Examples | Bus pattern |
| --- | --- | --- |
| Read | `LDA`, `ADC`, `NOP abs,X`, `LAX` | Operand read at the effective address. |
| Write | `STA`, `SAX`, `SHA` | Write at the effective address. |
| RMW | `INC`, `ASL`, `SLO`, `ISB` | Read, write of the old value, write of the new value (REQ-104). |
| Implied | `TXA`, `CLI`, `INX` | Dummy read of `PC` in cycle 2 (HRM 3.3). |
| Stack | `PHA`, `PLP`, `JSR`, `RTS`, `RTI` | Fixed sequences. |
| Branch | `BNE` | 2, 3 or 4 cycles (REQ-106). |
| Special | `BRK`, `JMP`, `JMP (abs)`, KIL | Fixed sequences. |

The addressing mode code makes the cycles that compute `addr`. The rules from HRM 3.3:

1. `abs,X`, `abs,Y` and `(zp),Y` add the index to the low byte first. The next cycle reads
   `base_hi:low`. For a read without carry, this read is the operand read. Otherwise the CPU
   reads again at the corrected address (REQ-102).
2. Writes and RMW instructions in these modes always do the read at `base_hi:low`, then the
   access at the corrected address (REQ-103).
3. `zp,X`, `zp,Y` and `(zp,X)` wrap in page zero. The `(zp),Y` and `(zp,X)` pointer reads
   wrap from `$FF` to `$00` (REQ-105).
4. `JMP (abs)` reads the high byte from the same page as the low byte (REQ-105).
5. A taken branch adds the offset to the address of the next instruction. A page cross adds
   a cycle with a read at the uncorrected address (REQ-106).

The SingleStepTests 6502 data is the reference for the exact address of each dummy cycle.

### Flags (REQ-107)

`regs.p` keeps bits 4 and 5 at 1 on every read. `PLP` and `RTI` ignore bits 4 and 5 of the
pulled value. The push value is `p | $30` for `PHP` and `BRK`, and `(p | $20) & !$10` for
IRQ and NMI. In the BRK hijack case of REQ-122, the push value stays `p | $30`.

### Decimal mode (REQ-108, REQ-109)

`ADC` and `SBC` use one function each, with a `decimal: bool` argument from `D`. `ISB`,
`RRA` and `ARR` call the same functions. The NMOS algorithm computes the low nibble with
correction, then the high nibble. `N`, `Z` and `V` come from the result before the
correction, and `C` from the corrected high nibble (HRM 3.2). No other instruction reads `D`.
The interrupt and reset sequences do not change `D`.

### Undocumented opcodes (REQ-111, REQ-112)

The combined opcodes call the RMW operation and then the ALU operation on the result (HRM 3.5).
The unstable opcodes use the SingleStepTests formulas:

| Opcode | Result |
| --- | --- |
| `ANE` $8B | `A = (A \| $EE) & X & imm` |
| `LXA` $AB | `A = X = (A \| $EE) & imm` |
| `SHA` $93, $9F | value `A & X & (base_hi + 1)` |
| `SHX` $9E | value `X & (base_hi + 1)` |
| `SHY` $9C | value `Y & (base_hi + 1)` |
| `SHS` $9B | `S = A & X`, value `S & (base_hi + 1)` |

For `SHA`, `SHX`, `SHY` and `SHS`, a page cross replaces the high byte of the write address
with the value.

### KIL opcodes (REQ-113)

The 12 KIL opcodes set `mode = Jammed`. In each tick, the CPU then reads by the
SingleStepTests pattern: `PC+1`, `$FFFF`, `$FFFE`, `$FFFE`, and then `$FFFF` in every later
tick. `PC` stays at the opcode address + 1. The CPU ignores NMI and IRQ. Only `reset` ends
the state. `step` returns `Step::Jammed` after one tick.

### Interrupt lines and polling (REQ-115 to REQ-118, REQ-127)

The CPU uses two stages for NMI:

1. `end_cycle(true)` after `end_cycle(false)` sets `nmi_edge`. This also occurs in a halted
   cycle, so no edge is lost (REQ-127).
2. In `end_cycle` of a cycle whose tick ran (`Tick::Ran`), the CPU copies `nmi_edge` to
   `nmi_pending` and clears `nmi_edge`.

The CPU computes the interrupt request in `end_cycle` of each cycle whose tick ran:
`req = nmi_pending || (irq_level && !I)`. `I` is the value after the bus access of that
tick. A tick with `Tick::Stalled` and a halted cycle make no poll (REQ-127).

The instruction uses one poll result. For an instruction of L cycles, the result comes from
the end of cycle L-1 (the second-to-last cycle). The CPU stores it in `poll` and `poll_nmi`.
At the boundary, if `poll` is true and `no_poll_next` is false, the CPU starts an interrupt
sequence. NMI has priority over IRQ.

This rule gives the timing of the requirements:

| Requirement | Effect of the rule |
| --- | --- |
| REQ-118 | A line asserted in cycle N is seen at the end of cycle N. The earliest sequence starts in cycle N+2. |
| REQ-119 | `CLI` and `PLP` change `I` in their last cycle, after the poll. The next instruction runs first. `RTI` loads `P` in cycle 4 of 6, before the poll in cycle 5. |
| REQ-120 | `SEI` and `PLP` set `I` in their last cycle, after a poll that saw `I` = 0. The sequence pushes `P` with `I` = 1. |
| REQ-121 | A taken branch without page cross uses the poll from the end of cycle 1, not cycle 2. A taken branch with page cross uses cycle 3. A branch not taken uses cycle 1. |

### Interrupt sequence (REQ-114 to REQ-116, REQ-122, REQ-123)

The sequence has 7 cycles. Cycle numbers start at 1:

| Cycle | IRQ or NMI | `BRK` |
| --- | --- | --- |
| 1 | Read `PC` (opcode, discarded) | Read `PC` (opcode `$00`), `PC += 1` |
| 2 | Read `PC` (discarded) | Read `PC` (discarded), `PC += 1` |
| 3 | Write `PCH` to `$0100+S`, `S -= 1` | Same |
| 4 | Write `PCL` to `$0100+S`, `S -= 1` | Same |
| 5 | Write `P` push value, `S -= 1`, select vector | Same |
| 6 | Read vector low byte, set `I` | Same |
| 7 | Read vector high byte | Same |

In cycle 5, the CPU selects `$FFFA` if `nmi_pending` is true. It then clears
`nmi_pending`. Otherwise it selects `$FFFE`. This gives the hijack of REQ-122 for an NMI
line asserted in cycles S-1 to S+3 of a sequence that starts in cycle S.

An NMI edge that the CPU samples at the end of cycle 5 is discarded. This gives the lost NMI
of REQ-122 (NMI asserted in cycle S+4). An edge sampled at the end of cycle 6 or 7 stays
pending.

After cycle 7, the CPU sets `no_poll_next`. The first handler instruction then always
executes (REQ-123). The flag is cleared at the next boundary. A `BRK` as the first handler
instruction uses the vector selection of cycle 5, so a pending NMI can hijack it.

The NMI vector `$FFFA` is a project decision (REQ-116). It must be confirmed by a test with
the real OS: the OS NMI handler must run for each VBI.

### Reset sequence (REQ-124, REQ-128)

`reset` sets `mode = Reset`, `jam_cycle = 0`, `nmi_edge = false` and `nmi_pending = false`.
The 7 ticks of the sequence are:

| Cycle | Access |
| --- | --- |
| 1 | Read `PC` |
| 2 | Read `PC` |
| 3 | Read `$0100+S`, `S -= 1` |
| 4 | Read `$0100+S`, `S -= 1` |
| 5 | Read `$0100+S`, `S -= 1` |
| 6 | Read `$FFFC`, set `I` |
| 7 | Read `$FFFD` |

The sequence makes no write (project decision, REQ-124). `D` does not change.

### Halt by ANTIC (REQ-125)

The CPU has no halt input. The machine does not call `tick` in a cycle that ANTIC uses. The
CPU state does not change in that cycle, so all results are the same with and without DMA.
Read and write cycles halt in the same way.

### Stop by RDY (REQ-126, REQ-127)

If `rdy` is false and the current cycle is a read, `tick` calls `bus.read` with the address
of that cycle. It discards the value and returns `Tick::Stalled`. The CPU state does not
change and the CPU makes no poll. If `rdy` is false and the current cycle is a write, `tick`
runs the cycle normally (HRM 4.9).

## Timing

The CPU has no clock of its own. One `tick` is one machine cycle (HRM 3.3). The machine owns
the cycle counter and the scan line position.

| Event | Cycle |
| --- | --- |
| IRQ line set by `set_irq` | Before the tick of cycle N |
| NMI level passed to `end_cycle` | After the tick (or the halted cycle) N |
| Poll result for an instruction | End of the second-to-last cycle of the instruction |
| First cycle of an interrupt sequence | First boundary at or after N+2 (unhalted cycles) |
| Vector selection in a sequence | Cycle 5 of the sequence |
| First opcode fetch after reset | Cycle 8 after `reset` |

With ANTIC NMI at cycle 8 of a scan line, the earliest NMI sequence starts at cycle 10
(HRM 4.8).

## Test plan

The integration tests are in `crates/harness/tests/` (see [test-harness.md](test-harness.md)):

- `cpu_single_step.rs` reads `test-data/single-step/6502/<opcode>.json`. For each case it
  loads the initial state into a `FlatBus` with `registers_mut`, calls `tick` once for each
  entry of `cycles`, and compares the bus log and the final state. Each test in the table
  below runs the files of one group of opcodes. `single_step_decimal` runs only the cases
  with `D` = 1.
- `cpu_functional.rs` runs `test-data/klaus/6502_functional_test.bin` on a `FlatBus`.

The unit tests are in `crates/atari-core/src/cpu.rs`, module `tests`. They use a test
driver that runs numbered machine cycles. In each cycle `c`, the driver sets the NMI and IRQ
levels from a script. Then it calls `tick` with the RDY level of the script, or it does not
call `tick` if the script halts cycle `c`. A `TestBus` records each access as
`(c, address, value, read or write)`. The tests compare this log with the expected cycles.

| Requirement | Test |
| --- | --- |
| REQ-100 | `crates/harness/tests/cpu_single_step.rs` `single_step_documented_opcodes`; `cpu/tests.rs` unit tests `address_calculation_keeps_flags`, `bit_sets_n_v_z` |
| REQ-101 | `cpu_single_step.rs` `single_step_bus_cycles`; `cpu/tests.rs` unit tests `implied_dummy_read`, `one_access_per_tick` |
| REQ-102 | `cpu/tests.rs` unit tests `abs_x_read_page_cross_dummy_read`, `abs_y_and_ind_y_read_page_cross_dummy_read`, `abs_x_read_no_page_cross`, `ind_y_pointer_ff_no_extra_cycle`, `zero_page_index_no_extra_cycle` |
| REQ-103 | `cpu/tests.rs` unit tests `indexed_store_dummy_read_page_cross` (abs,X and abs,Y), `indexed_store_dummy_read_same_page`, `ind_y_store_six_cycles` (with and without page crossing) |
| REQ-104 | `cpu/tests.rs` unit test `rmw_writes_old_value_then_new_value`; `cpu_single_step.rs` `single_step_bus_cycles` |
| REQ-105 | `cpu/tests.rs` unit tests `zero_page_x_wraps`, `zero_page_y_wraps`, `ind_x_pointer_wraps`, `ind_y_pointer_wraps_at_ff`, `jmp_indirect_page_bug` |
| REQ-106 | `cpu/tests.rs` unit tests `branch_page_cross_from_next_instruction`, `branch_same_page_three_cycles`, `branch_not_taken_two_cycles`; `cpu_single_step.rs` `single_step_bus_cycles` |
| REQ-107 | `cpu/tests.rs` unit tests `php_pushes_bits_4_and_5`, `irq_pushes_b_clear`, `nmi_pushes_b_clear`, `brk_pushes_b_set` |
| REQ-108 | `cpu/tests.rs` unit tests `decimal_adc_0f_0f`, `decimal_adc_99_01_flags`, `decimal_adc_ff_01_zero`, `decimal_sbc_borrow`, `decimal_sbc_no_borrow`, `decimal_no_extra_cycle`; `cpu_single_step.rs` `single_step_decimal` |
| REQ-109 | `cpu/tests.rs` unit tests `decimal_flag_ignored_by_compare_and_increment`, `interrupt_keeps_decimal_flag`, `reset_keeps_decimal_flag`; `cpu_single_step.rs` `single_step_decimal_undocumented` (the `ISB`, `RRA` and `ARR` files) |
| REQ-110 | `crates/harness/tests/cpu_functional.rs` `klaus_dormann_functional_test` |
| REQ-111 | `cpu_single_step.rs` `single_step_undocumented_stable`; `cpu/tests.rs` unit tests `isb_uses_inc_result`, `lax_loads_a_and_x`, `sax_keeps_flags`, `nop_abs_x_page_cross`, `opcode_table_counts` |
| REQ-112 | `cpu_single_step.rs` `single_step_undocumented_unstable`; `cpu/tests.rs` unit tests `shx_and_high_byte_plus_one`, `shx_page_cross_replaces_high_byte`, `shs_sets_s` |
| REQ-113 | `cpu_single_step.rs` `single_step_kil`; `cpu/tests.rs` unit tests `kil_reads_ffff_forever`, `kil_ignores_interrupts`, `reset_ends_kil`, `opcode_table_counts`; `crates/atari-core/src/machine.rs` unit test `kil_machine_keeps_running` |
| REQ-114 | `cpu_single_step.rs` `single_step_brk`; `cpu/tests.rs` unit test `brk_pushes_and_sets_i` |
| REQ-115 | `cpu/tests.rs` unit tests `irq_sequence_seven_cycles`, `irq_masked_by_i`, `irq_removed_before_cli_is_lost`, `irq_repeats_while_asserted` |
| REQ-116 | `cpu/tests.rs` unit tests `nmi_ignores_i_and_uses_fffa`, `nmi_level_held_one_interrupt`, `nmi_two_edges_one_interrupt` |
| REQ-117 | `cpu/tests.rs` unit tests `nmi_waits_for_seven_cycle_instruction`, `nmi_waits_for_eight_cycle_rmw`, `irq_waits_for_long_instructions` |
| REQ-118 | `cpu/tests.rs` unit tests `nmi_two_cycle_recognition_delay`, `nmi_in_last_cycle_runs_next_instruction`, `irq_two_cycle_recognition_delay` |
| REQ-119 | `cpu/tests.rs` unit tests `cli_delays_irq_one_instruction`, `plp_delays_irq_one_instruction`, `rti_irq_immediate` |
| REQ-120 | `cpu/tests.rs` unit tests `cli_sei_irq_with_i_set`, `cli_plp_irq_with_i_set`, `pairs_without_irq_dispatch`, `rti_pairs_irq_after_rti` |
| REQ-121 | `cpu/tests.rs` unit tests `taken_branch_delays_nmi`, `taken_branch_early_nmi_no_delay`, `page_cross_branch_no_delay`, `not_taken_branch_no_delay`, `jump_instructions_no_delay`, `taken_branch_delays_irq` |
| REQ-122 | `cpu/tests.rs` unit tests `nmi_after_irq_handler_first_instruction` (S = N-5), `nmi_lost_when_irq_starts_four_cycles_before` (S = N-4), `nmi_hijacks_irq_sequence` (S = N-3 to N+1), `nmi_hijacks_brk_with_b_set` (`BRK`, S = N-5 to N+1) |
| REQ-123 | `cpu/tests.rs` unit tests `handler_first_instruction_always_runs`, `nmi_handler_first_instruction_before_second_nmi`, `nmi_hijacks_brk_as_first_handler_instruction` |
| REQ-124 | `cpu/tests.rs` unit tests `reset_loads_fffc_in_seven_cycles`, `reset_decrements_s_by_three_no_writes`, `reset_sets_i_keeps_d`, `reset_ends_kil`, `reset_clears_pending_nmi`, `reset_keeps_decimal_flag` |
| REQ-125 | `cpu/tests.rs` unit tests `halted_cycles_delay_accesses`, `halted_rmw_writes`, `halted_cycles_do_not_change_results`; `machine.rs` unit test `antic_dma_delays_cpu_accesses` |
| REQ-126 | `cpu/tests.rs` unit tests `rdy_low_repeats_read`, `rdy_low_after_inc_wsync`, `rdy_low_does_not_stop_write`, `rdy_low_keeps_registers` |
| REQ-127 | `cpu/tests.rs` unit tests `halted_cycles_extend_recognition_delay`, `nmi_edge_in_halted_cycle_kept`, `nmi_during_rdy_after_stopped_instruction`, `nmi_sequence_stops_on_rdy` |
| REQ-128 | `cpu/tests.rs` unit tests `power_on_registers`, `power_on_is_deterministic` |

New tests in this plan, one line each:

- `implied_dummy_read`: `TXA` at `$0200` reads `$0200` and `$0201` and makes no write.
- `one_access_per_tick`: each `tick` makes exactly one `TestBus` access, for 10,000 ticks of mixed code.
- `abs_y_and_ind_y_read_page_cross_dummy_read`: `LDA $20F0,Y` and `LDA ($40),Y` with `Y` = `$80` read `$2070`, then `$2170`.
- `zero_page_y_wraps`, `ind_x_pointer_wraps`: `LDX $F0,Y` with `Y` = `$20` reads `$0010`. `LDA ($F0,X)` with `X` = `$0F` reads the pointer at `$00FF` and `$0000`.
- `decimal_sbc_no_borrow`, `decimal_no_extra_cycle`: the values of REQ-108 criteria 5 and 7.
- `isb_uses_inc_result`, `lax_loads_a_and_x`, `sax_keeps_flags`, `nop_abs_x_page_cross`: the start states and results of REQ-111 criteria 1 to 5.
- `shx_and_high_byte_plus_one`, `shx_page_cross_replaces_high_byte`, `shs_sets_s`: the start states and results of REQ-112 criteria 2 to 4.
- `kil_machine_keeps_running` (`machine.rs`): a test OS image has a KIL opcode at the reset address. Each `run_frame` advances the cycle counter by 29,868.
- `irq_waits_for_long_instructions`: REQ-117 criteria 1 and 2 with the IRQ line and `I` = 0.
- `taken_branch_early_nmi_no_delay`, `taken_branch_delays_irq`: REQ-121 criteria 2 and 6.
- `nmi_handler_first_instruction_before_second_nmi`: a second NMI edge in cycle 7 of an NMI sequence starts the next NMI sequence after the first handler instruction (REQ-123 criterion 2).
- `halted_cycles_delay_accesses`, `halted_rmw_writes`: the cycle numbers of REQ-125 criteria 1 and 2, with halted cycles 20 to 29.
- `rdy_low_after_inc_wsync`: REQ-126 criterion 2.
- `nmi_edge_in_halted_cycle_kept`: REQ-127 criterion 2.

## Open issues

1. The draft `Bus` advances the chips inside `read` and `write`, and `Cpu::step` runs a whole
   instruction. That model cannot give the per-cycle interrupt poll (REQ-118 to REQ-122) or
   the repeated reads during RDY (REQ-126). This design replaces it with `Cpu::tick`. The
   machine skeleton must change to match.
2. HRM 3.4 says that `PLP`+`SEI` dispatches no IRQ. The poll rule of this design gives an IRQ
   after `PLP` (`I` = 0) + `SEI`, the same as for `CLI`+`SEI`. The test `pairs_without_irq_dispatch`
   will show the difference. The HRM statement needs a second source before the design
   changes.
3. The discard of an NMI edge sampled in cycle 5 of a sequence is a model of HRM Figure 2.
   The HRM gives no internal reason for it. Tests at other values of N must confirm that the
   model is correct at all scan line positions, not only for the ANTIC NMI at cycle 8.
4. The HRM does not state if RDY-stopped cycles count for the interrupt delay. This design
   does not count them (REQ-127).
5. The NMI vector `$FFFA` and the IRQ vector `$FFFE` come from the 6502 convention and the
   SingleStepTests `BRK` data. The HRM does not list the vectors.
6. Resolved: [test-harness.md](test-harness.md) now calls one `tick` for each entry of
   `cycles`, and uses the seven test names of this test plan.
