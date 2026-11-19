//! Planned tests of the keyboard scan (REQ-521).
//!
//! Each test is a stub until its package writes it (`docs/design/pokey.md`, "Test plan").

use crate::planned::planned;

/// Verifies REQ-521: Criterion 3: Two instances run criterion 1. One instance also reads `KBCODE`
/// on each cycle. The reads of `IRQST` and `SKSTAT` (with `peek`) of the two instances are the same
/// on each cycle.
#[test]
#[ignore = "planned: SLICE-06"]
fn kbcode_read_no_side_effect() {
    planned();
}
