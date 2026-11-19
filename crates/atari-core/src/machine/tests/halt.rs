//! Planned tests of the CPU halt by ANTIC DMA in the full machine (`docs/design/cpu.md`,
//! "Test plan").

use crate::planned::planned;

/// Verifies REQ-125: with ANTIC DMA in cycles 20 to 29 of a line, `LDA $1000` and
/// `INC $0600` make their bus accesses in the cycles of criteria 1 and 2: no access and no
/// CPU state change in a DMA cycle, and the CPU continues in the next free cycle.
#[test]
#[ignore = "planned: SLICE-02"]
fn antic_dma_delays_cpu_accesses() {
    planned();
}
