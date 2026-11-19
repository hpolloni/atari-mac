# Specification

This directory holds the requirements for the Atari 65XE emulator. Each requirement is in
its own file: `requirements/REQ-NNN.md`.

## Sources

The primary hardware source is the Altirra Hardware Reference Manual by Avery Lee (edition of
2026-01-03). Run `scripts/fetch-roms.sh` to download it to
`reference/altirra-hardware-reference.pdf`. Each hardware requirement cites a section of this
manual, for example "HRM 4.9 WSYNC".

## Non-goals

These items are out of scope. A requirement must not depend on them.

| Non-goal | Since | Note |
| --- | --- | --- |
| A Linux frontend | 2026-11-19 | Temporary. The core and the test harness still build and run on Linux, because CI uses Linux runners for them. |
| A Windows frontend, and Windows CI | 2026-11-19 | No gate checks Windows. |
| Hardware outside the 65XE | Project start | See REQ-001: no extended memory, 65C02, 65C816, PBI devices or memory upgrades. |

## Rules

1. Write one testable statement in each requirement. Use "shall".
2. Write in Simplified Technical English (see the project output style).
3. Do not specify the implementation. Put implementation decisions in `docs/design/`.
4. Give each requirement at least one acceptance criterion that a test can check.
5. Do not change the ID of a requirement. If a requirement is removed, set its status to
   `Withdrawn` and keep the file.

## ID ranges

| Range | Area | Main source |
| --- | --- | --- |
| REQ-001 to REQ-019 | Product and frontend | This project |
| REQ-020 to REQ-039 | Test harness | This project |
| REQ-040 to REQ-099 | Quality | This project |
| REQ-100 to REQ-199 | CPU (6502C) | HRM chapter 3 |
| REQ-200 to REQ-299 | System: clocks, memory map, bank switching, reset, PIA | HRM chapter 2 |
| REQ-300 to REQ-399 | ANTIC | HRM chapter 4 |
| REQ-400 to REQ-499 | GTIA, color palette, joystick and paddle inputs | HRM chapters 6, 7.1, 7.2 |
| REQ-500 to REQ-599 | POKEY: audio, timers, keyboard, interrupts, serial port | HRM chapter 5 |
| REQ-600 to REQ-699 | Media: cartridges, executables, SIO bus, disk drives | HRM chapters 8.1, 9.1, 10 |

## Priority

| Priority | Meaning |
| --- | --- |
| Must | Necessary to boot AltirraOS and BASIC, and to run ordinary software. |
| Should | Necessary for accurate timing or for less common software. |
| Could | Rare behavior. Implement after all Must and Should requirements. |

## Template

```markdown
# REQ-NNN: <short title>

| Field | Value |
| --- | --- |
| Status | Draft |
| Priority | Must |
| Area | <area from the ID range table> |
| Source | HRM <section number> <section title> |
| Verification | Test |
| Depends on | REQ-NNN, REQ-NNN (or "None") |

## Requirement

The emulator shall <one testable statement>.

## Rationale

<Why this is necessary. Name the software that needs it, if known.>

## Acceptance criteria

1. <A condition that a test can check, with exact values.>

## Notes

<Exceptions, hardware variants and open questions. Delete this section if empty.>
```

`Verification` is one of: `Test` (an automated test), `Inspection` (a review of the code or
the documents) or `Demonstration` (a person runs the emulator and looks at the result).

## Index

The index is in [`requirements/INDEX.md`](requirements/INDEX.md).
