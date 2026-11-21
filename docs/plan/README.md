---
product_complete: false
---
# Plan

This directory holds the delivery plan. The work is done in vertical slices. A vertical slice
is a small, complete path through all layers (CPU, chips, machine, frontend, test harness)
that a person can run and that the tests check.

Each slice has its own file: `SLICE-NN.md`. A slice lists the requirements that it
implements, the work packages, and the conditions that tell when it is done.

| Slice | Goal | Status |
| --- | --- | --- |
| [SLICE-01](SLICE-01.md) | Boot to BASIC `READY` in a window, and type `PRINT 2+2` | Done |
| [SLICE-02](SLICE-02.md) | Graphics modes, fine scrolling and player/missile graphics, with golden frame tests | Done |
| [SLICE-03](SLICE-03.md) | Sound: POKEY audio and the console speaker, at real-time speed | Done |
| [SLICE-04](SLICE-04.md) | Cartridges, executable files, joysticks and paddles | Done |
| [SLICE-05](SLICE-05.md) | Disk drive D1: with ATR images (needs SLICE-04) | Planned |
| [SLICE-06](SLICE-06.md) | Exact interrupt and PIA timing, power-on state, save states and product qualities | Planned |
| [SLICE-07](SLICE-07.md) | Run a BASIC program from a text file with `--bas` | Done |
| [SLICE-08](SLICE-08.md) | Cassette tape: boot a CAS tape image with `--cas` (needs SLICE-05) | Done |

Each requirement that is not withdrawn is in exactly one work package (REQ-050). SLICE-02 to SLICE-05 are independent of each other, except that SLICE-05 needs SLICE-04, and SLICE-08 needs SLICE-05 and the CA2 output of SLICE-06.
