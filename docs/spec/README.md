# Specification

This directory holds the requirements for the Atari 65XE emulator. Each requirement is in
its own file: `requirements/REQ-NNN.md`.

## Sources

The main hardware source is the Altirra Hardware Reference Manual (HRM) by Avery Lee,
2026-01-02 edition. It is in git: `reference/altirra-hardware-reference.pdf`. Each hardware
requirement cites a section of this manual, for example "HRM 4.9 WSYNC".

The other references are second sources (OI-041). Each reference has a registry entry and a
section index in [`references/`](references/) (REQ-046). The HRM and the Atari 8-Bit FAQ are
in git ([`reference/`](../../reference/README.md)). The other references are pinned by their
URL and their SHA-256: `make reference` downloads them to `reference/pinned/`.

The HRM is correct when two references do not agree. A requirement also cites each other
reference that is relevant to it. Its `Notes` section states each difference from the HRM.

## Non-goals

These items are out of scope. A requirement must not depend on them.

| Non-goal | Since | Note |
| --- | --- | --- |
| A Linux frontend | 2026-11-19 | Temporary. The core and the test harness still build and run on Linux, because CI uses Linux runners for them. |
| A Windows frontend, and Windows CI | 2026-11-19 | No gate checks Windows. |
| The light pen (`PENH`, `PENV` input) | 2026-11-19 | OI-016. REQ-342 is withdrawn. |
| The fifth player (`PRIOR` bit 4) | 2026-11-19 | OI-019. REQ-426 is withdrawn. The emulator ignores `PRIOR` bit 4. |
| Tape writing (cassette record mode, saving a CAS file) | 2026-11-20 | OI-049. The cassette deck only plays (SLICE-08). |
| Hardware outside the 65XE | Project start | See REQ-001: no extended memory, 65C02, 65C816, PBI devices or memory upgrades. |

## Rules

1. Write one testable statement in each requirement. Use "shall".
2. Write in Simplified Technical English. The rules are in the project output style,
   [`.claude/output-styles/simplified-technical-english.md`](../../.claude/output-styles/simplified-technical-english.md).
3. Do not specify the implementation. Put implementation decisions in `docs/design/`.
4. Give each requirement at least one acceptance criterion that a test can check.
5. Do not change the ID of a requirement. If a requirement is removed, set its status to
   `withdrawn` and keep the file.

## ID ranges

| Range of `REQ-NNN` | Area | `area` value | Main source |
| --- | --- | --- | --- |
| 001 to 019 | Product and frontend | `product`, `frontend` | This project |
| 020 to 039 | Test harness | `test-harness` | This project |
| 040 to 079 | Quality | `quality` | This project |
| 080 to 099 | Product and frontend (continued, since 2026-11-19) | `product`, `frontend` | This project |
| 100 to 199 | CPU (6502C) | `cpu` | HRM chapter 3 |
| 200 to 299 | System: clocks, memory map, bank switching, reset, PIA | `system` | HRM chapter 2 |
| 300 to 399 | ANTIC | `antic` | HRM chapter 4 |
| 400 to 499 | GTIA, color palette, joystick and paddle inputs | `gtia` | HRM chapters 6, 7.1, 7.2 |
| 500 to 599 | POKEY: audio, timers, keyboard, interrupts, serial port | `pokey` | HRM chapter 5 |
| 600 to 699 | Media: cartridges, executables, SIO bus, disk drives | `media` | HRM chapters 8.1, 9.1, 10 |

## Priority

| Priority | Meaning |
| --- | --- |
| Must | Necessary to boot AltirraOS and BASIC, and to run ordinary software. |
| Should | Necessary for accurate timing or for less common software. |
| Could | Rare behavior. Implement after all Must and Should requirements. |

## Template

```markdown
---
id: REQ-NNN
title: <short title>
status: draft            # draft | accepted | implemented | withdrawn (REQ-057)
priority: must           # must | should | could
area: <`area` value from the ID range table>
source: HRM <section number> <section title>; <ID> <key> <title>   # or: Project decision
verification: test       # test | inspection | demonstration
depends_on: [REQ-NNN, REQ-NNN]                 # [] if none
---
# REQ-NNN: <short title>

## Requirement

The emulator shall <one testable statement>.

## Rationale

<Why this is necessary. Name the software that needs it, if known.>

## Acceptance criteria

1. <A condition that a test can check, with exact values.>

## Notes

<Exceptions, hardware variants and open questions. Delete this section if empty.>
```

The metadata is YAML frontmatter (REQ-043). `title` is the text of the first heading after
`REQ-NNN: `. `verification` is one of: `test` (an automated test), `inspection` (a review of
the code or the documents) or `demonstration` (a person runs the emulator and looks at the
result).

`source` is `Project decision`, or a list of citations separated by `; ` (REQ-046). Each
citation starts with the ID of a registry entry in [`references/`](references/), for example
`HRM`. When the section index of the reference has sections, the second word is a key of
the index (the section number), and the citation can continue with the section title and
other text, for example `HRM 3.3 Cycle timing (Clock speed, Dead memory cycles)`. Use `,`, not
`;`, inside a citation. When the index has no sections, the citation is only the ID. Each
citation names its reference: write `HRM 4.9 WSYNC; HRM 14.6 ANTIC registers`, not
`HRM 4.9 WSYNC; 14.6 ANTIC registers`. When a source cites the HRM and other references, the
first citation is an `HRM` citation. A source with citations cannot also say "project
decision": write the project decision in the `Notes` section.

## Index

The index is in [`requirements/INDEX.md`](requirements/INDEX.md). The index of the open
items is in [`open-items/README.md`](open-items/README.md). Both are generated: run
`cargo run -p atari-harness --bin spec-index` after a change.

## Gates

`make spec-check` runs the specification gates of
[`spec-gates.md`](../design/spec-gates.md) (REQ-043 to REQ-055). The gates read
[`vague-words.txt`](vague-words.txt), the registry entries in [`references/`](references/)
and their section indices, for example [`references/HRM.sections.txt`](references/HRM.sections.txt).

The violations that the gates found when they were added are in
[`gate-baseline.txt`](gate-baseline.txt), one per line. The gates ignore these violations.
A new violation fails. Remove a line when its violation is fixed: a line that matches no
violation also fails.
