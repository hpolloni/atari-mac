# Open items

An open item is a question that the specification cannot answer without a decision. Each
item has its own file: `OI-NNN.md`. When a decision is made, write it in the `Decision`
section, set the status to `Decided`, and change the affected requirements.

| Item | Title | Area | Status |
| --- | --- | --- | --- |
| [OI-001](OI-001.md) | ANTIC-to-GTIA cycle interface | ANTIC, GTIA | Decided |
| [OI-002](OI-002.md) | Side effects of ANTIC DMA reads in $D500-$D5FF | System, media | Open |
| [OI-003](OI-003.md) | Cartridge interface signatures | System, media | Open |
| [OI-004](OI-004.md) | Duplicate cartridge requirements | System, media | Open |
| [OI-005](OI-005.md) | Chip state that a test can compare | System, GTIA, POKEY, ANTIC | Open |
| [OI-006](OI-006.md) | Values derived from HRM charts that are images | ANTIC, GTIA | Open |
| [OI-007](OI-007.md) | IRQ after PLP followed by SEI | CPU | Open |
| [OI-008](OI-008.md) | REQ-210 covers $8000-$9FFF | System | Open |
| [OI-009](OI-009.md) | POKEY reset at power-on | System, POKEY | Open |
| [OI-010](OI-010.md) | Cycle of the CA2/CB2 pulse | System | Open |
| [OI-011](OI-011.md) | Reads of unmapped I/O | System | Open |
| [OI-012](OI-012.md) | PAL vertical sync lines | ANTIC | Open |
| [OI-013](OI-013.md) | VSCROL end deadline | ANTIC | Open |
| [OI-014](OI-014.md) | Line buffer when playfield DMA is off | ANTIC | Open |
| [OI-015](OI-015.md) | Extent of the hi-res bug at vertical blank | ANTIC | Open |
| [OI-016](OI-016.md) | PENH and PENV addresses | ANTIC | Open |
| [OI-017](OI-017.md) | NMI start cycle contract with the CPU | ANTIC, CPU | Open |
| [OI-018](OI-018.md) | Playfield DMA on cycle 105 | ANTIC | Open |
| [OI-019](OI-019.md) | Fifth player priority | GTIA | Open |
| [OI-020](OI-020.md) | Artifacting scope | GTIA, frontend | Open |
| [OI-021](OI-021.md) | PAL chroma blending of row 0 | GTIA | Open |
| [OI-022](OI-022.md) | Frontend setting for display filters | GTIA, frontend | Open |
| [OI-023](OI-023.md) | Palette parameters | GTIA | Open |
| [OI-024](OI-024.md) | HRM errors found during the spec work | All | Open |
| [OI-025](OI-025.md) | Serial output complete interrupt and IRQEN | POKEY | Open |
| [OI-026](OI-026.md) | POKEY project decisions to confirm | POKEY | Open |
| [OI-027](OI-027.md) | Audio sample rate out of range | POKEY | Open |
| [OI-028](OI-028.md) | Linked timer reload cycle | POKEY | Open |
| [OI-029](OI-029.md) | Force break and the SIO bus model | POKEY, media | Open |
| [OI-030](OI-030.md) | 65XE keyboard layout | POKEY | Open |
| [OI-031](OI-031.md) | POKEY choices where the HRM conflicts with itself | POKEY | Open |
| [OI-032](OI-032.md) | CAR types 41 and 42 | Media | Open |
| [OI-033](OI-033.md) | ED Percom block and Format $21 | Media | Open |
| [OI-034](OI-034.md) | Write-protect status bit | Media | Open |
| [OI-035](OI-035.md) | Bank-switched cartridge types without a source | Media | Open |
| [OI-036](OI-036.md) | File format details marked to verify | Media | Open |
| [OI-037](OI-037.md) | SIO timing statements that conflict | Media | Open |
