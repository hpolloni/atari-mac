# Design

This directory holds the design documents. A design document tells how the emulator
satisfies the requirements in `docs/spec/`. Each design document lists the requirements that
it covers.

| Document | Area | Range of `REQ-NNN` |
| --- | --- | --- |
| [architecture.md](architecture.md) | Crates, modules, data flow, scheduling | 001 to 099 |
| [test-harness.md](test-harness.md) | Test harness and test data | 001 to 099 |
| [cpu.md](cpu.md) | CPU | 100 to 199 |
| [system.md](system.md) | Memory map, bank switching, reset, PIA | 200 to 299 |
| [antic.md](antic.md) | ANTIC | 300 to 399 |
| [gtia.md](gtia.md) | GTIA and palette | 400 to 499 |
| [pokey.md](pokey.md) | POKEY | 500 to 599 |
| [media.md](media.md) | Cartridges, executables, SIO, disk drives | 600 to 699 |
| [spec-gates.md](spec-gates.md) | Specification gates and traceability | REQ-027, REQ-043 to REQ-055 |

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
