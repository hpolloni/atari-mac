//! 6520 PIA: joystick ports (port A) and the 65XE memory control (port B).
//!
//! Design: `docs/design/system.md`, "pia.rs". Requirements: REQ-218 to REQ-231.

#![expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    reason = "WP-0 stubs: delete this when the work package replaces the stubs"
)]

/// `PACTL`/`PBCTL` bit 2: 1 = the data register, 0 = the data direction register.
const CTL_DATA: u8 = 0x04;

/// The PIA chip.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    missing_copy_implementations,
    reason = "a chip state is not copied by accident; the design gives `&self` methods"
)]
pub struct Pia {
    /// Port A data direction (1 = output).
    ddra: u8,
    /// Port B data direction (1 = output).
    ddrb: u8,
    /// Port A output register.
    ora: u8,
    /// Port B output register.
    orb: u8,
    /// Port A control bits 0-5.
    pactl: u8,
    /// Port B control bits 0-5.
    pbctl: u8,
    /// External port A pins. Not reset.
    port_a_in: u8,
}

impl Default for Pia {
    fn default() -> Self {
        Self::new()
    }
}

impl Pia {
    /// The PIA after power-on: the same state as after `reset`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ddra: 0,
            ddrb: 0,
            ora: 0,
            orb: 0,
            pactl: 0,
            pbctl: 0,
            port_a_in: 0xFF,
        }
    }

    /// Clears all registers to `$00` (REQ-230).
    pub const fn reset(&mut self) {
        self.ddra = 0;
        self.ddrb = 0;
        self.ora = 0;
        self.orb = 0;
        self.pactl = 0;
        self.pbctl = 0;
    }

    /// CPU read. `reg` is the low address byte; the PIA uses `reg & 3` (REQ-218).
    pub const fn read(&mut self, reg: u8) -> u8 {
        // WP-5: interrupt status and CA2/CB2 pulses are not in SLICE-01.
        self.peek(reg)
    }

    /// Read without side effects, for debuggers.
    #[must_use]
    pub const fn peek(&self, reg: u8) -> u8 {
        match reg & 3 {
            0 if self.pactl & CTL_DATA != 0 => {
                // Port A: an output bit reads as the AND of output and pin (HRM 2.5).
                (self.port_a_in & !self.ddra) | (self.ora & self.ddra & self.port_a_in)
            }
            0 => self.ddra,
            // Port B: an output bit reads back the output state (HRM 2.5).
            1 if self.pbctl & CTL_DATA != 0 => (self.orb & self.ddrb) | !self.ddrb,
            1 => self.ddrb,
            2 => self.pactl,
            _ => self.pbctl,
        }
    }

    /// CPU write. `reg` is the low address byte.
    pub const fn write(&mut self, reg: u8, value: u8) {
        match reg & 3 {
            0 if self.pactl & CTL_DATA != 0 => self.ora = value,
            0 => self.ddra = value,
            1 if self.pbctl & CTL_DATA != 0 => self.orb = value,
            1 => self.ddrb = value,
            2 => self.pactl = value & 0x3F,
            _ => self.pbctl = value & 0x3F,
        }
    }

    /// Advances one machine cycle.
    pub const fn tick(&mut self) {}

    /// External port A pin levels. Bit = 0: pulled low (pressed joystick direction).
    pub const fn set_port_a_input(&mut self, pins: u8) {
        self.port_a_in = pins;
    }

    /// The external port A pins, as last set.
    #[must_use]
    pub const fn port_a_input(&self) -> u8 {
        self.port_a_in
    }

    /// Level of the SIO command line (CB2). `true` = high.
    #[must_use]
    pub const fn cb2(&self) -> bool {
        true
    }

    /// Port B pin levels for the memory map (REQ-211). An input bit is pulled up.
    #[must_use]
    pub const fn port_b_pins(&self) -> u8 {
        (self.orb & self.ddrb) | !self.ddrb
    }

    /// `true` while the PIA asserts IRQ (REQ-229).
    #[must_use]
    pub const fn irq(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::Pia;

    /// Verifies REQ-218: the PIA decodes only the low two address bits.
    #[test]
    fn registers_mirror_every_four_bytes() {
        let mut pia = Pia::new();
        pia.write(0xFE, 0x3C);
        assert_eq!(pia.read(0x02), 0x3C, "PACTL written at a mirror");
        assert_eq!(pia.peek(0x06), 0x3C, "PACTL read at a mirror");
    }

    /// Verifies REQ-219 and REQ-224: `PACTL`/`PBCTL` bit 2 selects DDR or data register.
    #[test]
    fn control_bit2_selects_register() {
        let mut pia = Pia::new();
        pia.write(1, 0x0F); // PBCTL bit 2 = 0: DDRB
        assert_eq!(pia.read(1), 0x0F, "DDRB");
        pia.write(3, 0xFC); // bits 6-7 are not written
        assert_eq!(pia.read(3), 0x3C, "PBCTL bits 0-5");
        pia.write(1, 0x05); // ORB
        assert_eq!(pia.read(1), 0xF5, "inputs read 1, outputs read ORB");
        pia.write(2, 0x00);
        pia.write(0, 0x0F); // DDRA
        assert_eq!(pia.read(0), 0x0F, "DDRA");
        pia.write(2, 0x04);
        pia.write(0, 0x0A); // ORA
        assert_eq!(pia.read(0), 0xFA, "port A: inputs and output AND pins");
    }

    /// Verifies REQ-220 and REQ-221: port A reads the pins, and an output bit reads as the
    /// AND of the output and the pin.
    #[test]
    fn port_a_reads_pins() {
        let mut pia = Pia::new();
        pia.write(2, 0x04);
        pia.set_port_a_input(0xF0);
        assert_eq!(pia.read(0), 0xF0, "all inputs");
        assert_eq!(pia.port_a_input(), 0xF0, "pins kept");
    }

    /// Verifies REQ-211 and REQ-222: the port B pins are the output bits, and input bits are
    /// pulled up.
    #[test]
    fn port_b_pins_pull_up_inputs() {
        let mut pia = Pia::new();
        assert_eq!(pia.port_b_pins(), 0xFF, "all inputs after reset");
        pia.write(1, 0x03); // DDRB: bits 0-1 output
        pia.write(3, 0x04);
        pia.write(1, 0x00); // ORB = 0
        assert_eq!(pia.port_b_pins(), 0xFC, "bits 0-1 low");
    }

    /// Verifies REQ-229 and REQ-230: reset clears all registers, and the PIA asserts no IRQ.
    #[test]
    fn reset_clears_registers() {
        let mut pia = Pia::new();
        pia.write(3, 0x3C);
        pia.write(1, 0x12);
        pia.tick();
        pia.reset();
        assert_eq!(pia.read(3), 0x00, "PBCTL");
        assert_eq!(pia.read(1), 0x00, "DDRB");
        assert!(!pia.irq(), "no IRQ");
        assert!(pia.cb2(), "CB2 high");
        assert_eq!(pia, Pia::default(), "reset state = power-on state");
    }

    /// Verifies REQ-219: the acceptance criteria 1 to 5 (DDR and data register select).
    #[test]
    fn ddr_select_criteria() {
        let mut pia = Pia::new();
        pia.write(2, 0x30);
        pia.write(0, 0xF0);
        assert_eq!(pia.read(0), 0xF0, "1: DDRA");
        pia.write(2, 0x34);
        pia.write(0, 0x55);
        assert_eq!(pia.read(0), 0x5F, "2: inputs high, outputs from ORA");
        pia.write(2, 0x30);
        assert_eq!(pia.read(0), 0xF0, "2: DDRA kept");
        pia.write(0, 0x01);
        pia.write(2, 0x34);
        pia.write(0, 0x00);
        assert_eq!(pia.read(0), 0xFE, "3: bit 0 is an output");
        pia.write(2, 0x30);
        pia.write(0, 0x00);
        pia.write(2, 0x34);
        assert_eq!(pia.read(0), 0xFF, "3: all inputs");
        pia.write(0, 0x55);
        pia.write(2, 0x30);
        pia.write(0, 0xFF);
        pia.write(2, 0x34);
        assert_eq!(pia.read(0), 0x55, "4: ORA kept its value");
        pia.write(3, 0x38);
        pia.write(1, 0xFF);
        assert_eq!(pia.read(1), 0xFF, "5: DDRB");
        pia.write(3, 0x3C);
        pia.write(1, 0xFD);
        assert_eq!(pia.read(1), 0xFD, "5: ORB");
    }

    /// Verifies REQ-220: the acceptance criteria 1 to 3 (port A output bits read as the AND
    /// of the output register and the pin).
    #[test]
    fn port_a_read_criteria() {
        let mut pia = Pia::new();
        pia.write(0, 0xF0); // DDRA
        pia.write(2, 0x3C);
        pia.write(0, 0x00);
        assert_eq!(pia.read(0), 0x0F, "1: outputs low");
        pia.write(0, 0xF0);
        pia.set_port_a_input(0xEF);
        assert_eq!(pia.read(0), 0xEF, "2: pin 4 pulled low");
        pia.write(2, 0x38);
        pia.write(0, 0x00); // DDRA
        pia.write(2, 0x3C);
        pia.set_port_a_input(0xFB);
        assert_eq!(pia.read(0), 0xFB, "3: pin 2 pulled low");
    }

    /// Verifies REQ-224: the acceptance criteria 1 and 5 (bits 6 and 7 of a write are
    /// ignored).
    #[test]
    fn control_write_ignores_bits_6_and_7() {
        let mut pia = Pia::new();
        pia.write(2, 0xFF);
        assert_eq!(pia.read(2), 0x3F, "1: PACTL");
        pia.write(2, 0xC0);
        assert_eq!(pia.read(2), 0x00, "5: PACTL");
    }
}
