//! Emulation core for the Atari 65XE.
//!
//! The crate has no dependencies and no I/O. A frontend creates a [`Machine`], calls
//! [`Machine::run_frame`] once per video frame, shows [`Machine::frame`], plays the audio
//! from [`Machine::drain_audio`] and sends input with the `Machine` input methods.
//!
//! # Specification
//!
//! The hardware specification is the Altirra Hardware Reference Manual by Avery Lee
//! (`reference/altirra-hardware-reference.pdf`, in git; `docs/spec/references/HRM.md`).
//! Code that implements a hardware behavior cites the manual section in its documentation.
//!
//! # Module ownership
//!
//! Each chip lives in its own module and talks to the rest of the system only through the
//! public interface that is documented in that module:
//!
//! | Module | Contents |
//! | --- | --- |
//! | [`cpu`] | MOS 6502 (NMOS, with undocumented opcodes), one bus access per cycle |
//! | [`antic`] | ANTIC: display list, DMA, cycle stealing, NMI, `WSYNC`, `VCOUNT` |
//! | [`gtia`] | GTIA: playfield colors, player/missile graphics, collisions, triggers, `CONSOL` |
//! | [`palette`] | GTIA color byte to RGB |
//! | [`pokey`] | POKEY: audio, timers, keyboard, IRQ, random numbers |
//! | [`keyboard`] | Atari keyboard codes (`KBCODE`) |
//! | [`pia`] | 6520 PIA: joystick ports and the 65XE memory control (`PORTB`) |
//! | [`media`] | Cartridge, executable, tape and disk image formats, the SIO bus and drive D1: |
//! | [`screen`] | The text of the OS text screen in memory (REQ-024) |
//! | [`machine`] | Memory map, bus scheduling, interrupts: ties all chips together |
//! | [`timing`] | Clock and frame timing constants |

pub mod antic;
pub mod cpu;
pub mod gtia;
pub mod keyboard;
pub mod machine;
pub mod media;
pub mod palette;
pub mod pia;
#[cfg(test)]
mod planned;
pub mod pokey;
pub mod screen;
pub mod timing;

pub use machine::{ConfigError, JoystickState, Machine, MachineConfig};
pub use timing::VideoStandard;
