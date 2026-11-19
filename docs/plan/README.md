# Plan

This directory holds the delivery plan. The work is done in vertical slices. A vertical slice
is a small, complete path through all layers (CPU, chips, machine, frontend, test harness)
that a person can run and that the tests check.

Each slice has its own file: `SLICE-NN.md`. A slice lists the requirements that it
implements, the work packages, and the conditions that tell when it is done.

| Slice | Goal | Status |
| --- | --- | --- |
| [SLICE-01](SLICE-01.md) | Boot to BASIC `READY` in a window, and type `PRINT 2+2` | Planned |
