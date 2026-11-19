//! Tests of the CPU halt by ANTIC DMA in the full machine (`docs/design/cpu.md`, "Test
//! plan").

use super::{BusAccess, dma_program};
use crate::cpu::{Bus, Tick};

/// A flat 64 KB bus that logs each access: the reference CPU runs on it without halts.
struct FlatBus {
    /// The memory.
    mem: Vec<u8>,
    /// The access of the last tick.
    last: Option<BusAccess>,
}

impl Bus for FlatBus {
    fn read(&mut self, addr: u16) -> u8 {
        let value = self.mem[usize::from(addr)];
        self.last = Some(BusAccess {
            addr,
            value,
            write: false,
        });
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.mem[usize::from(addr)] = value;
        self.last = Some(BusAccess {
            addr,
            value,
            write: true,
        });
    }
}

/// What the machine did in one cycle.
struct Cycle {
    /// ANTIC halted the CPU.
    halt: bool,
    /// The CPU bus access.
    access: Option<BusAccess>,
}

/// The number of cycles from the access `first` to the next access that matches `next`.
fn gaps(
    log: &[Cycle],
    first: impl Fn(BusAccess) -> bool,
    next: impl Fn(BusAccess) -> bool,
) -> Vec<usize> {
    let mut gaps = Vec::new();
    for (start, cycle) in log.iter().enumerate() {
        if cycle.access.is_some_and(&first)
            && let Some(offset) = log[start + 1..]
                .iter()
                .position(|later| later.access.is_some_and(&next))
        {
            gaps.push(offset + 1);
        }
    }
    gaps
}

/// Verifies REQ-125: with ANTIC DMA (mode 2 lines and refresh), a loop of `LDA $1000` and
/// `INC $0600` makes no access and no CPU state change in a halted cycle, and continues in
/// the next free cycle: the accesses in the free cycles are the accesses of a CPU without
/// halts, in the same order, with the same registers after each access. Halts fall between
/// the opcode fetch of `LDA $1000` and its read of `$1000` (criterion 1), and between the
/// read of `$0600` by `INC $0600` and its write of the old value (criterion 2).
#[test]
fn antic_dma_delays_cpu_accesses() {
    let mut m = dma_program(&[
        0xAD, 0x00, 0x10, // $C20F: LDA $1000
        0xEE, 0x00, 0x06, // $C212: INC $0600
        0x4C, 0x0F, 0xC2, // $C215: JMP $C20F
    ]);
    m.run_frame();
    m.run_frame();
    let mut reference = m.cpu;
    let mut flat = FlatBus {
        mem: (0..=0xFFFF_u16).map(|addr| m.peek(addr)).collect(),
        last: None,
    };
    let mut log = Vec::new();
    for c in 0..29_868 {
        let before = m.cpu;
        m.run_cycle();
        assert!(m.last_out.rdy, "cycle {c}: RDY is high");
        let cycle = Cycle {
            halt: m.last_out.halt,
            access: m.last_access,
        };
        if cycle.halt {
            assert_eq!(cycle.access, None, "cycle {c}: no access in a halted cycle");
            assert!(m.cpu == before, "cycle {c}: no CPU state change");
        } else {
            assert_eq!(reference.tick(&mut flat, true), Tick::Ran, "reference tick");
            assert_eq!(cycle.access, flat.last, "cycle {c}: the next access");
            assert_eq!(
                m.cpu.registers(),
                reference.registers(),
                "cycle {c}: registers"
            );
        }
        log.push(cycle);
    }
    let halted = log.iter().filter(|cycle| cycle.halt).count();
    assert!(halted > 192 * 40, "{halted} halted cycles");

    let lda = gaps(
        &log,
        |a| a.addr == 0xC20F && !a.write,
        |a| a.addr == 0x1000 && !a.write,
    );
    assert!(lda.iter().any(|&gap| gap > 3), "a halt inside LDA $1000");
    assert!(lda.contains(&3), "LDA $1000 without a halt");
    let inc = gaps(
        &log,
        |a| a.addr == 0x0600 && !a.write,
        |a| a.addr == 0x0600 && a.write,
    );
    assert!(
        inc.iter().any(|&gap| gap > 1),
        "a halt between the read and the write of INC $0600"
    );
    assert!(inc.contains(&1), "INC $0600 without a halt");
}
