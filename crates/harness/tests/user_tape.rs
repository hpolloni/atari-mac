//! A check of the CAS loader with a real tape image of the user (not a requirement test).
//!
//! The file `roms/Ninja Commando.cas` is not in git (`.gitignore` has `/roms/*.cas`), so CI
//! has no such file. Without it, the test prints a message and passes.

use std::fs;

use atari_core::media::{Tape, TapeBlock};
use atari_harness::data::repo_root;

/// `roms/Ninja Commando.cas` loads with 368 data blocks, all of 600 baud, and the first
/// gap is 30247 ms.
#[test]
fn ninja_commando_cas_loads() {
    let path = repo_root().join("roms").join("Ninja Commando.cas");
    let Ok(file) = fs::read(&path) else {
        println!("skipped: {} is not there", path.display());
        return;
    };
    let tape = Tape::from_cas(&file).unwrap_or_else(|error| panic!("{error}"));
    let blocks = tape.blocks();
    assert!(
        blocks
            .iter()
            .all(|block| matches!(block, TapeBlock::Data { baud: 600, .. })),
        "each block is a data block of 600 baud"
    );
    assert_eq!(blocks.len(), 368, "data blocks");
    assert!(
        matches!(blocks.first(), Some(TapeBlock::Data { gap_ms: 30_247, .. })),
        "the first gap: {:?}",
        blocks.first().map(|block| match block {
            TapeBlock::Data { gap_ms, .. } | TapeBlock::Signal { gap_ms, .. } => *gap_ms,
        })
    );
}
