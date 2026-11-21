//! The CPU hook of the machine for the trace and the monitor (REQ-086). The test names
//! follow the test plan in `docs/design/debugger.md`.

/// The tests and their helpers. The module has `cfg(test)`, so that the helpers can use
/// `unwrap` as the tests do.
#[cfg(test)]
mod hook {
    use atari_core::cpu::Boundary;
    use atari_core::debug::{
        BusAccess, Control, CpuHook, InstructionStart, Position, RunEnd, Sequence, SequenceKind,
    };
    use atari_core::{Machine, MachineConfig, VideoStandard};
    use atari_harness::system::{BootOptions, TEST_SAMPLE_RATE, boot, boot_program, test_os_rom};

    /// Machine cycles in a scan line.
    const LINE_CYCLES: u64 = 114;
    /// Scan lines in an NTSC frame.
    const NTSC_LINES: u64 = 262;

    /// One call of the hook.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Event {
        /// A call of `instruction`.
        Instruction(InstructionStart),
        /// A call of `sequence`.
        Sequence(Sequence),
    }

    /// A hook that records each call.
    #[derive(Debug, Default)]
    struct Recorder {
        /// The `instruction` and `sequence` calls, in order.
        events: Vec<Event>,
        /// The `access` calls.
        accesses: Vec<BusAccess>,
    }

    impl CpuHook for Recorder {
        fn instruction(&mut self, start: &InstructionStart) -> Control {
            self.events.push(Event::Instruction(*start));
            Control::Continue
        }

        fn sequence(&mut self, sequence: &Sequence) {
            self.events.push(Event::Sequence(*sequence));
        }

        fn access(&mut self, access: BusAccess) {
            self.accesses.push(access);
        }
    }

    /// Runs `frames` frames with `run_frame_with` and a [`Recorder`].
    fn hooked(machine: &mut Machine, frames: u64) -> Recorder {
        let mut recorder = Recorder::default();
        for _ in 0..frames {
            assert_eq!(machine.run_frame_with(&mut recorder), RunEnd::FrameDone);
        }
        recorder
    }

    /// The events of `frames` frames from a cycle-by-cycle run with `run_cycle`, for a machine
    /// that starts at power-on (so that the beam position follows from the cycle count). An
    /// instruction starts in the cycle after which `Cpu::at_opcode_fetch` is no longer true.
    /// A sequence starts in the cycle after which the boundary of an interrupt or a reset is
    /// gone, and it ends at the next instruction start.
    fn reference(machine: &mut Machine, frames: u64) -> Vec<Event> {
        let end = machine.frame_count() + frames;
        let mut events = Vec::new();
        let mut pending: Option<(Position, bool)> = None;
        while machine.frame_count() < end {
            let cycle = machine.cycle_count();
            let at = Position {
                frame: machine.frame_count(),
                scanline: u32::try_from((cycle / LINE_CYCLES) % NTSC_LINES).unwrap(),
                hpos: u32::try_from(cycle % LINE_CYCLES).unwrap(),
                cycle,
            };
            let fetch = machine.cpu().at_opcode_fetch();
            let boundary = machine.cpu().boundary();
            let regs = *machine.cpu().registers();
            let byte = |offset: u16| {
                machine
                    .debug_peek(regs.pc.wrapping_add(offset))
                    .unwrap_or(0xFF)
            };
            let bytes = [byte(0), byte(1), byte(2)];
            machine.run_cycle();
            if fetch && !machine.cpu().at_opcode_fetch() {
                if let Some((start, reset)) = pending.take() {
                    events.push(Event::Sequence(Sequence {
                        at: start,
                        kind: sequence_kind(machine, reset),
                        handler: regs.pc,
                    }));
                }
                events.push(Event::Instruction(InstructionStart { at, regs, bytes }));
            } else if matches!(boundary, Some(Boundary::Interrupt | Boundary::Reset))
                && machine.cpu().boundary().is_none()
            {
                pending = Some((at, boundary == Some(Boundary::Reset)));
            }
        }
        events
    }

    /// The kind of the sequence that ended: `Reset` for a reset, else from `Cpu::vector`.
    fn sequence_kind(machine: &Machine, reset: bool) -> SequenceKind {
        if reset {
            SequenceKind::Reset
        } else if machine.cpu().vector() == 0xFFFA {
            SequenceKind::Nmi
        } else {
            SequenceKind::Irq
        }
    }

    /// Verifies REQ-086: a program machine with ANTIC DMA and a `WSYNC` write
    /// runs one frame with `Machine::run_frame_with`. Each opcode fetch cycle that a cycle-by-cycle
    /// run finds (the cycle after which `Cpu::at_opcode_fetch` is false) has exactly one
    /// `instruction` call, with that position and the registers before the instruction (criteria 1
    /// and 4).
    #[test]
    fn hook_reports_each_instruction_start() {
        let mut program = vec![
            0xA9, 0x22, 0x8D, 0x00, 0xD4, // LDA #$22, STA DMACTL
            0xA9, 0x00, 0x8D, 0x02, 0xD4, // LDA #$00, STA DLISTL
            0xA9, 0xC1, 0x8D, 0x03, 0xD4, // LDA #$C1, STA DLISTH
            0x8D, 0x0A, 0xD4, // $C00F: STA WSYNC
            0xE6, 0x80, // INC $80
            0xB1, 0x80, // LDA ($80),Y
            0x4C, 0x0F, 0xC0, // JMP $C00F
        ];
        // The display list at $C100: 24 blank lines, 21 lines of mode 2 from $3000, JVB.
        program.resize(0x100, 0xEA);
        program.extend([0x70, 0x70, 0x70, 0x42, 0x00, 0x30]);
        program.extend([0x02; 20]);
        program.extend([0x41, 0x00, 0xC1]);
        let mut m = boot_program(
            &program,
            BootOptions {
                basic: false,
                standard: VideoStandard::Ntsc,
            },
        )
        .unwrap();
        let mut copy = m.clone();
        let recorder = hooked(&mut m, 2);
        let expected = reference(&mut copy, 2);
        assert!(m == copy, "the hooked run changed the machine");
        assert_eq!(recorder.events, expected, "hook calls");

        let starts: Vec<InstructionStart> = recorder
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Instruction(start) => Some(*start),
                Event::Sequence(_) => None,
            })
            .collect();
        assert!(starts.len() > 1000, "{} instructions", starts.len());
        // The `WSYNC` stall and the DMA make gaps larger than the instruction cycles.
        let index = starts
            .iter()
            .position(|start| start.regs.pc == 0xC00F)
            .unwrap();
        let (store, next) = (starts[index], starts[index + 1]);
        assert_eq!(next.regs.pc, 0xC012, "INC after STA WSYNC");
        assert!(
            next.at.cycle - store.at.cycle > 4,
            "the WSYNC stall is in the gap: {store:?}, {next:?}"
        );
        assert_eq!(starts[0].disassembly().to_string(), "LDA #$22");
        assert_eq!(starts[0].bytes, [0xA9, 0x22, 0x8D], "the bytes at PC");
    }

    /// The test OS ROM of [`test_os_rom`] with `program`, and the IRQ vector at `$C020`.
    fn irq_rom(program: &[u8]) -> Vec<u8> {
        let mut rom = test_os_rom(program);
        rom[0x3FFE] = 0x20;
        rom[0x3FFF] = 0xC0;
        rom
    }

    /// Verifies REQ-086: the reset sequence after power-on, a VBI NMI and a POKEY
    /// timer IRQ each give one `sequence` call with the kind, the position of the first cycle of
    /// the sequence and the handler address, before the `instruction` call of the first handler
    /// instruction (criterion 2).
    #[test]
    fn hook_reports_interrupt_sequences() {
        let mut program = vec![
            0xA9, 0x40, 0x8D, 0x0E, 0xD4, // LDA #$40, STA NMIEN (VBI)
            0xA9, 0x03, 0x8D, 0x0F, 0xD2, // LDA #$03, STA SKCTL
            0xA9, 0xFF, 0x8D, 0x00, 0xD2, // LDA #$FF, STA AUDF1
            0xA9, 0x01, 0x8D, 0x0E, 0xD2, // LDA #$01, STA IRQEN (timer 1)
            0x8D, 0x09, 0xD2, // STA STIMER
            0x58, // CLI
            0x4C, 0x18, 0xC0, // $C018: JMP $C018
        ];
        // The IRQ handler at $C020 acknowledges timer 1 and enables it again.
        program.resize(0x20, 0xEA);
        program.extend([
            0x48, // PHA
            0xA9, 0x00, 0x8D, 0x0E, 0xD2, // LDA #$00, STA IRQEN
            0xA9, 0x01, 0x8D, 0x0E, 0xD2, // LDA #$01, STA IRQEN
            0x68, // PLA
            0x40, // RTI
        ]);
        let mut m = Machine::new(MachineConfig {
            os_rom: irq_rom(&program),
            basic_rom: None,
            cartridge: None,
            disk: None,
            fast_sio: false,
            fast_tape: false,
            tape: None,
            standard: VideoStandard::Ntsc,
            audio_sample_rate: TEST_SAMPLE_RATE,
        })
        .unwrap();
        let mut copy = m.clone();
        let recorder = hooked(&mut m, 2);
        let expected = reference(&mut copy, 2);
        assert_eq!(recorder.events, expected, "hook calls");

        let events = &recorder.events;
        assert_eq!(
            events[0],
            Event::Sequence(Sequence {
                at: Position::default(),
                kind: SequenceKind::Reset,
                handler: 0xC000,
            }),
            "the reset sequence after power-on"
        );
        let mut kinds = [0_usize; 3];
        for (index, event) in events.iter().enumerate() {
            let Event::Sequence(sequence) = event else {
                continue;
            };
            let (slot, handler) = match sequence.kind {
                SequenceKind::Reset => (0, 0xC000),
                SequenceKind::Nmi => (1, 0xFFF0),
                SequenceKind::Irq => (2, 0xC020),
            };
            kinds[slot] += 1;
            assert_eq!(sequence.handler, handler, "handler of {sequence:?}");
            if sequence.kind == SequenceKind::Nmi {
                assert_eq!(sequence.at.scanline, 248, "the VBI NMI at line 248");
            }
            match events.get(index + 1) {
                Some(Event::Instruction(start)) => {
                    assert_eq!(start.regs.pc, handler, "the first handler instruction");
                    assert!(
                        start.at.cycle >= sequence.at.cycle + 7,
                        "the sequence has 7 cycles"
                    );
                }
                other => panic!("{sequence:?} is followed by {other:?}"),
            }
        }
        assert_eq!(kinds[0], 1, "one reset sequence");
        assert!(kinds[1] >= 1, "a VBI NMI");
        assert!(kinds[2] >= 1, "a POKEY timer IRQ");
    }

    /// Verifies REQ-086: 3 frames of the `AltirraOS` boot with `run_frame` and with
    /// `run_frame_with` and a hook that records each call give equal machines (criterion 5).
    #[test]
    fn hooked_run_matches_plain_run() {
        let mut plain = boot(BootOptions::default()).unwrap();
        let mut traced = plain.clone();
        for _ in 0..3 {
            plain.run_frame();
        }
        let recorder = hooked(&mut traced, 3);
        assert!(plain == traced, "the hooked run changed the machine");
        assert!(recorder.events.len() > 1000, "instruction calls");
        assert!(recorder.accesses.len() > 1000, "access calls");
    }
}
