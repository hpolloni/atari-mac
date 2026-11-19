# Design

This directory holds the design documents. A design document tells how the emulator
satisfies the requirements in `docs/spec/`. Each design document lists the requirements that
it covers.

| Document | Area | Requirements |
| --- | --- | --- |
| [architecture.md](architecture.md) | Crates, modules, data flow, scheduling | REQ-001 to REQ-099 |
| [test-harness.md](test-harness.md) | Test harness and test data | REQ-001 to REQ-099 |
| [cpu.md](cpu.md) | CPU | REQ-100 to REQ-199 |
| [system.md](system.md) | Memory map, bank switching, reset, PIA | REQ-200 to REQ-299 |
| [antic.md](antic.md) | ANTIC | REQ-300 to REQ-399 |
| [gtia.md](gtia.md) | GTIA and palette | REQ-400 to REQ-499 |
| [pokey.md](pokey.md) | POKEY | REQ-500 to REQ-599 |
| [media.md](media.md) | Cartridges, executables, SIO, disk drives | REQ-600 to REQ-699 |

## Template

```markdown
# <Area> design

## Scope

<What this document covers. List the requirement IDs.>

## Interfaces

<The public types and functions, and how other modules call them. Give the call order and
the timing contract (for example "called once per machine cycle").>

## State

<The data that the module keeps, with sizes and power-on values.>

## Behavior

<How the module works. Use one subsection per topic. Cite HRM sections.>

## Timing

<Cycle-level behavior: when each action occurs, relative to the scanline and cycle counters.>

## Test plan

| Requirement | Test |
| --- | --- |
| REQ-NNN | `crates/<crate>/tests/<file>.rs` `<test name>`, or a unit test name |

## Open issues

<Decisions that are not made yet.>
```
