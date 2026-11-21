//! The debugger reads of the machine (`docs/design/debugger.md`, "Machine").
//!
//! The reads have no side effects (REQ-087 criterion 5, REQ-090 criterion 5).

use super::Machine;
use crate::disasm::{Disassembly, disassemble};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the design puts the debugger functions of the machine in machine/debug.rs"
)]
impl Machine {
    /// A byte as the CPU sees it, without side effects (REQ-087 criterion 5, REQ-090
    /// criterion 5). `None` where the chip has no value to show without a side effect: the
    /// cartridge control area `$D500` to `$D5FF`, where a read can switch the cartridge bank.
    #[must_use]
    pub fn debug_peek(&self, addr: u16) -> Option<u8> {
        match addr {
            0xD500..=0xD5FF => None,
            _ => Some(self.peek(addr)),
        }
    }

    /// The disassembly at `addr`, with the bytes of [`Machine::debug_peek`] (`$FF` for
    /// `None`). The bytes after `addr` wrap at `$FFFF`.
    #[must_use]
    pub fn disassemble_at(&self, addr: u16) -> Disassembly {
        let byte = |offset: u16| self.debug_peek(addr.wrapping_add(offset)).unwrap_or(0xFF);
        disassemble(addr, [byte(0), byte(1), byte(2)])
    }
}
