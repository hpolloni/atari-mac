//! Save states of the machine (REQ-018, `docs/design/architecture.md`, "Save states").
//!
//! The format: the signature, the format version (`u16`), the configuration hash (`u64`),
//! the state of each part of the machine in the order of [`Machine`], and the FNV-1a hash
//! (`u64`) of all bytes before it. All numbers are little-endian.

use super::{BusAccess, Machine, Memory};
use crate::state::{FNV_START, Reader, State, StateError, Writer, fnv1a, impl_state};

/// The first bytes of each save state.
const SIGNATURE: [u8; 8] = *b"A65XEST\x1A";

/// The format version. A change of the format increments it.
const VERSION: u16 = 3;

/// The length of the RAM.
const RAM_SIZE: usize = 0x1_0000;

impl_state!(BusAccess { addr, value, write });

impl_state!(Machine {
    cpu,
    mem,
    antic,
    gtia,
    pokey,
    pia,
    sio,
    cb2,
    ca2,
    standard,
    data_bus,
    cycles,
    frames,
    last_out,
    last_access,
} skip {
    // Settings of the frontend: a load keeps the settings of the running machine.
    fast_sio,
    fast_tape,
    // Inputs of the frontend: GTIA has the console keys that the machine sees, and the
    // OPTION hold of `start_xex` and the START hold of `start_tape` are key presses of the
    // frontend start.
    console_keys,
    xex_option_until,
    tape_start_until,
    // The next tape mark of the event log: `load_state` calculates it again.
    tape_mark_at,
    tape_mark,
    // The debugger state of the hooked run (`docs/design/debugger.md`, "State").
    sequence_start
});

impl State for Memory {
    // The ROMs are not saved: they come from the configuration, and the configuration
    // hash checks them. The cartridge checks its own ROM.
    #[expect(
        clippy::unneeded_field_pattern,
        reason = "the pattern names each field, so a new field must be listed"
    )]
    fn save(&self, w: &mut Writer) {
        let Self {
            ram,
            os: _,
            basic: _,
            cart,
            portb,
        } = self;
        ram.save(w);
        if let Some(cart) = cart {
            cart.save(w);
        }
        portb.save(w);
    }

    #[expect(
        clippy::unneeded_field_pattern,
        reason = "the pattern names each field, so a new field must be listed"
    )]
    fn load(&mut self, r: &mut Reader<'_>) -> Result<(), StateError> {
        let Self {
            ram,
            os: _,
            basic: _,
            cart,
            portb,
        } = self;
        ram.load(r)?;
        if ram.len() != RAM_SIZE {
            return Err(StateError::Invalid);
        }
        if let Some(cart) = cart {
            cart.load(r)?;
        }
        portb.load(r)
    }
}

/// The hash of the configuration that the state needs: the video standard, the audio
/// sample rate, the OS and BASIC ROMs, and whether a cartridge is inserted.
fn config_hash(machine: &Machine) -> u64 {
    let mut w = Writer::default();
    machine.standard.save(&mut w);
    machine.pokey.sample_rate().save(&mut w);
    machine.mem.basic.is_some().save(&mut w);
    machine.mem.cart.is_some().save(&mut w);
    let hash = fnv1a(FNV_START, &w.into_bytes());
    let hash = fnv1a(hash, &machine.mem.os);
    fnv1a(hash, machine.mem.basic.as_deref().unwrap_or_default())
}

/// `Machine::save_state`.
pub(super) fn save(machine: &Machine) -> Vec<u8> {
    let mut w = Writer::default();
    w.put(&SIGNATURE);
    VERSION.save(&mut w);
    config_hash(machine).save(&mut w);
    machine.save(&mut w);
    let mut bytes = w.into_bytes();
    let sum = fnv1a(FNV_START, &bytes);
    bytes.extend_from_slice(&sum.to_le_bytes());
    bytes
}

/// `Machine::load_state`.
pub(super) fn load(machine: &mut Machine, bytes: &[u8]) -> Result<(), StateError> {
    if !bytes.starts_with(&SIGNATURE) {
        return Err(StateError::Signature);
    }
    let mut r = Reader::new(bytes.get(SIGNATURE.len()..).unwrap_or_default());
    let mut version = 0_u16;
    version.load(&mut r)?;
    if version != VERSION {
        return Err(StateError::Version(version));
    }
    let (body, sum) = bytes.split_last_chunk::<8>().ok_or(StateError::Truncated)?;
    if body.len() < SIGNATURE.len() + 2 + 8 {
        return Err(StateError::Truncated);
    }
    if fnv1a(FNV_START, body) != u64::from_le_bytes(*sum) {
        return Err(StateError::Checksum);
    }
    let mut r = Reader::new(body.get(SIGNATURE.len() + 2..).unwrap_or_default());
    let mut hash = 0_u64;
    hash.load(&mut r)?;
    if hash != config_hash(machine) {
        return Err(StateError::Config);
    }
    let mut next = machine.clone();
    next.load(&mut r)?;
    if !r.is_empty() {
        return Err(StateError::TrailingBytes);
    }
    *machine = next;
    Ok(())
}
