//! 6520 PIA: joystick ports (port A), the 65XE memory control (port B), and the SIO control
//! lines CA1, CA2, CB1 and CB2.
//!
//! Design: `docs/design/system.md`, "pia.rs". Requirements: REQ-218 to REQ-231. Hardware:
//! HRM 2.5 and HRM 14.5.

/// `PACTL`/`PBCTL` bit 0: the CA1/CB1 interrupt is enabled.
const CTL_C1_ENABLE: u8 = 0x01;
/// `PACTL`/`PBCTL` bit 1: CA1/CB1 detects a positive edge (0 = negative edge).
const CTL_C1_POSITIVE: u8 = 0x02;
/// `PACTL`/`PBCTL` bit 2: 1 = the data register, 0 = the data direction register.
const CTL_DATA: u8 = 0x04;
/// `PACTL`/`PBCTL` bit 3: the CA2/CB2 interrupt is enabled (in input mode).
const CTL_C2_ENABLE: u8 = 0x08;
/// `PACTL`/`PBCTL` bit 4: CA2/CB2 detects a positive edge (in input mode).
const CTL_C2_POSITIVE: u8 = 0x10;
/// `PACTL`/`PBCTL` bit 5: CA2/CB2 is an output.
const CTL_C2_OUTPUT: u8 = 0x20;
/// The writable control bits 0-5 (REQ-224).
const CTL_WRITABLE: u8 = 0x3F;

/// The CA2/CB2 mode in control bits 5-3 (HRM 14.5).
const MODE_MASK: u8 = 0x38;
/// `100`: low after a data access until the next active C1 transition.
const MODE_HANDSHAKE: u8 = 0x20;
/// `101`: low for one cycle after a data access.
const MODE_PULSE: u8 = 0x28;
/// `110`: output low.
const MODE_LOW: u8 = 0x30;
/// `111`: output high.
const MODE_HIGH: u8 = 0x38;

/// Port B bits that are pulled up as inputs: 0, 1 and 7 (HRM 2.6).
const PORT_B_PULL_UPS: u8 = 0x83;
/// Port B bits that float as inputs on the 65XE: 2-6 (HRM 2.5, REQ-223).
const PORT_B_FLOATING: u8 = 0x7C;

/// Machine cycles for which a floating port B bit reads 1 after it was output 1 (REQ-223).
/// 300 ms on NTSC, 303 ms on PAL (project decision, inside the HRM range of 100-500 ms).
pub const FLOAT_DECAY_CYCLES: u32 = 536_932;

/// `tick` calls until a one-cycle pulse ends: the `tick` of the access cycle and the next.
const PULSE_TICKS: u8 = 2;

/// One edge-detecting control input (CA1, CB1, or CA2/CB2 in input mode).
///
/// Returns `true` if the change from `old` to `new` is an edge in the selected direction.
const fn active_edge(old: bool, new: bool, positive: bool) -> bool {
    if positive { !old && new } else { old && !new }
}

/// The PIA chip.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    missing_copy_implementations,
    reason = "a chip state is not copied by accident; the design gives `&self` methods"
)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag for each interrupt latch and control line of the chip"
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
    /// CA1 interrupt status (`PACTL` bit 7).
    irqa1: bool,
    /// CA2 interrupt status (`PACTL` bit 6).
    irqa2: bool,
    /// CB1 interrupt status (`PBCTL` bit 7).
    irqb1: bool,
    /// CB2 interrupt status (`PBCTL` bit 6).
    irqb2: bool,
    /// External port A pins. Not reset.
    port_a_in: u8,
    /// CA1 level (SIO proceed). Not reset.
    ca1: bool,
    /// CB1 level (SIO interrupt). Not reset.
    cb1: bool,
    /// External CA2 level in input mode. Not reset.
    ca2_in: bool,
    /// External CB2 level in input mode. Not reset.
    cb2_in: bool,
    /// CA2 is low because of a pulse mode (`100` or `101`).
    ca2_low: bool,
    /// CB2 is low because of a pulse mode (`100` or `101`).
    cb2_low: bool,
    /// `tick` calls until the CA2 one-cycle pulse ends (0 = no pulse).
    ca2_pulse: u8,
    /// `tick` calls until the CB2 one-cycle pulse ends (0 = no pulse).
    cb2_pulse: u8,
    /// The CB2 output went from `110` to `111` (REQ-231).
    cb2_rose: bool,
    /// Port B bits 2-6 that still read 1 as floating inputs (REQ-223).
    float_high: u8,
    /// Cycles until each floating bit reads 0. Index = bit number.
    float_timer: [u32; 8],
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
            irqa1: false,
            irqa2: false,
            irqb1: false,
            irqb2: false,
            port_a_in: 0xFF,
            ca1: true,
            cb1: true,
            ca2_in: true,
            cb2_in: true,
            ca2_low: false,
            cb2_low: false,
            ca2_pulse: 0,
            cb2_pulse: 0,
            cb2_rose: false,
            float_high: 0,
            float_timer: [0; 8],
        }
    }

    /// Clears all registers to `$00`, the interrupt status included (REQ-230, HRM 2.5). The
    /// external line levels do not change.
    pub const fn reset(&mut self) {
        *self = Self {
            port_a_in: self.port_a_in,
            ca1: self.ca1,
            cb1: self.cb1,
            ca2_in: self.ca2_in,
            cb2_in: self.cb2_in,
            ..Self::new()
        };
    }

    /// CPU read. `reg` is the low address byte; the PIA uses `reg & 3` (REQ-218).
    ///
    /// A data register read clears the interrupt status of its port (REQ-228). A port A data
    /// read starts the CA2 pulse of the modes `100` and `101` (REQ-227).
    pub const fn read(&mut self, reg: u8) -> u8 {
        let value = self.peek(reg);
        match reg & 3 {
            0 if self.pactl & CTL_DATA != 0 => {
                self.irqa1 = false;
                self.irqa2 = false;
                match self.pactl & MODE_MASK {
                    MODE_HANDSHAKE => {
                        self.ca2_low = true;
                        self.ca2_pulse = 0;
                    }
                    MODE_PULSE => {
                        self.ca2_low = true;
                        self.ca2_pulse = PULSE_TICKS;
                    }
                    _ => {}
                }
            }
            1 if self.pbctl & CTL_DATA != 0 => {
                self.irqb1 = false;
                self.irqb2 = false;
            }
            _ => {}
        }
        value
    }

    /// Read without side effects, for debuggers.
    #[must_use]
    pub const fn peek(&self, reg: u8) -> u8 {
        match reg & 3 {
            0 if self.pactl & CTL_DATA != 0 => self.port_a_value(),
            0 => self.ddra,
            1 if self.pbctl & CTL_DATA != 0 => self.port_b_value(),
            1 => self.ddrb,
            2 => Self::status_bits(self.irqa1, self.irqa2) | self.pactl,
            _ => Self::status_bits(self.irqb1, self.irqb2) | self.pbctl,
        }
    }

    /// CPU write. `reg` is the low address byte.
    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 3 {
            0 if self.pactl & CTL_DATA != 0 => self.ora = value,
            0 => self.ddra = value,
            1 if self.pbctl & CTL_DATA != 0 => {
                self.orb = value;
                match self.pbctl & MODE_MASK {
                    MODE_HANDSHAKE => {
                        self.cb2_low = true;
                        self.cb2_pulse = 0;
                    }
                    MODE_PULSE => {
                        self.cb2_low = true;
                        self.cb2_pulse = PULSE_TICKS;
                    }
                    _ => {}
                }
            }
            1 => self.write_ddrb(value),
            2 => self.write_pactl(value),
            _ => self.write_pbctl(value),
        }
    }

    /// Advances one machine cycle: the one-cycle pulses end and the floating bits decay
    /// (REQ-223, REQ-227).
    pub fn tick(&mut self) {
        if self.ca2_pulse > 0 {
            self.ca2_pulse -= 1;
            if self.ca2_pulse == 0 {
                self.ca2_low = false;
            }
        }
        if self.cb2_pulse > 0 {
            self.cb2_pulse -= 1;
            if self.cb2_pulse == 0 {
                self.cb2_low = false;
            }
        }
        if self.float_high != 0 {
            self.decay_floating_bits();
        }
    }

    /// One cycle of the floating bit delay (REQ-223).
    fn decay_floating_bits(&mut self) {
        let mut still_high = 0;
        for (bit, timer) in self.float_timer.iter_mut().enumerate() {
            let mask = 1_u8 << bit;
            if self.float_high & mask != 0 {
                *timer = timer.saturating_sub(1);
                if *timer != 0 {
                    still_high |= mask;
                }
            }
        }
        self.float_high = still_high;
    }

    /// External port A pin levels. Bit = 0: pulled low (pressed joystick direction).
    pub const fn set_port_a_input(&mut self, pins: u8) {
        self.port_a_in = pins;
    }

    /// The external port A pins, as last set.
    #[must_use]
    pub const fn port_a_input(&self) -> u8 {
        self.port_a_in
    }

    /// Level of the SIO proceed line (CA1). `true` = high (REQ-225).
    pub const fn set_ca1(&mut self, high: bool) {
        if active_edge(self.ca1, high, self.pactl & CTL_C1_POSITIVE != 0) {
            self.irqa1 = true;
            if self.pactl & MODE_MASK == MODE_HANDSHAKE {
                self.ca2_low = false;
            }
        }
        self.ca1 = high;
    }

    /// Level of the SIO interrupt line (CB1). `true` = high (REQ-225).
    pub const fn set_cb1(&mut self, high: bool) {
        if active_edge(self.cb1, high, self.pbctl & CTL_C1_POSITIVE != 0) {
            self.irqb1 = true;
            if self.pbctl & MODE_MASK == MODE_HANDSHAKE {
                self.cb2_low = false;
            }
        }
        self.cb1 = high;
    }

    /// External level on CA2 in input mode. `true` = high, the default (REQ-226).
    pub const fn set_ca2_input(&mut self, high: bool) {
        if self.pactl & CTL_C2_OUTPUT == 0
            && active_edge(self.ca2_in, high, self.pactl & CTL_C2_POSITIVE != 0)
        {
            self.irqa2 = true;
        }
        self.ca2_in = high;
    }

    /// External level on CB2 in input mode. `true` = high, the default (REQ-226).
    pub const fn set_cb2_input(&mut self, high: bool) {
        if self.pbctl & CTL_C2_OUTPUT == 0
            && active_edge(self.cb2_in, high, self.pbctl & CTL_C2_POSITIVE != 0)
        {
            self.irqb2 = true;
        }
        self.cb2_in = high;
    }

    /// Level of the SIO motor line (CA2). `true` = high (REQ-227).
    #[must_use]
    pub const fn ca2(&self) -> bool {
        Self::c2_level(self.pactl, self.ca2_in, self.ca2_low)
    }

    /// Level of the SIO command line (CB2). `true` = high (REQ-227).
    #[must_use]
    pub const fn cb2(&self) -> bool {
        Self::c2_level(self.pbctl, self.cb2_in, self.cb2_low)
    }

    /// Port B pin levels for the memory map (REQ-211): the output register for output bits,
    /// and 1 for input bits 0, 1 and 7. Input bits 2-6 are 0; the memory map does not use
    /// them.
    #[must_use]
    pub const fn port_b_pins(&self) -> u8 {
        (self.orb & self.ddrb) | (!self.ddrb & PORT_B_PULL_UPS)
    }

    /// `true` while the PIA asserts IRQ (REQ-229).
    #[must_use]
    pub const fn irq(&self) -> bool {
        (self.irqa1 && self.pactl & CTL_C1_ENABLE != 0)
            || (self.irqa2 && self.pactl & CTL_C2_OUTPUT == 0 && self.pactl & CTL_C2_ENABLE != 0)
            || (self.irqb1 && self.pbctl & CTL_C1_ENABLE != 0)
            || (self.irqb2 && self.pbctl & CTL_C2_OUTPUT == 0 && self.pbctl & CTL_C2_ENABLE != 0)
    }

    /// The port A data register value: an output bit reads as the AND of the output and the
    /// pin (HRM 2.5, REQ-220). Port A has internal pull-ups (REQ-221).
    const fn port_a_value(&self) -> u8 {
        (self.ora | !self.ddra) & self.port_a_in
    }

    /// The port B data register value: an output bit reads the output register, input bits
    /// 0, 1 and 7 read 1, and input bits 2-6 read the floating value (REQ-222, REQ-223).
    const fn port_b_value(&self) -> u8 {
        (self.orb & self.ddrb) | (!self.ddrb & (PORT_B_PULL_UPS | self.float_high))
    }

    /// Bits 7 and 6 of a control register read (REQ-224).
    const fn status_bits(c1: bool, c2: bool) -> u8 {
        let mut bits = 0;
        if c1 {
            bits |= 0x80;
        }
        if c2 {
            bits |= 0x40;
        }
        bits
    }

    /// The CA2 or CB2 level for control bits `ctl`, the external level `input` and the pulse
    /// state `low` (REQ-226, REQ-227).
    const fn c2_level(ctl: u8, input: bool, low: bool) -> bool {
        match ctl & MODE_MASK {
            MODE_HANDSHAKE | MODE_PULSE => !low,
            MODE_LOW => false,
            MODE_HIGH => true,
            _ => input,
        }
    }

    /// A `DDRB` write. A bit 2-6 that changes from output 1 to input floats high (REQ-223).
    fn write_ddrb(&mut self, value: u8) {
        let to_input = self.ddrb & !value & PORT_B_FLOATING;
        self.ddrb = value;
        // An output bit never floats.
        self.float_high &= !value;
        for (bit, timer) in self.float_timer.iter_mut().enumerate() {
            let mask = 1_u8 << bit;
            if to_input & mask != 0 {
                if self.orb & mask != 0 {
                    self.float_high |= mask;
                    *timer = FLOAT_DECAY_CYCLES;
                } else {
                    self.float_high &= !mask;
                }
            }
        }
    }

    /// A `PACTL` write with the mode change rules (REQ-224, REQ-228, REQ-231).
    const fn write_pactl(&mut self, value: u8) {
        let old_mode = self.pactl & MODE_MASK;
        self.pactl = value & CTL_WRITABLE;
        let new_mode = self.pactl & MODE_MASK;
        if self.pactl & CTL_C2_OUTPUT != 0 {
            self.irqa2 = false;
        }
        // CA2 from output low to a positive edge input mode (`010` or `011`).
        if old_mode == MODE_LOW && new_mode & !CTL_C2_ENABLE == CTL_C2_POSITIVE {
            self.irqa2 = true;
        }
        if old_mode != new_mode {
            self.ca2_low = false;
            self.ca2_pulse = 0;
        }
    }

    /// A `PBCTL` write with the mode change rules (REQ-224, REQ-228, REQ-231).
    const fn write_pbctl(&mut self, value: u8) {
        let old_mode = self.pbctl & MODE_MASK;
        self.pbctl = value & CTL_WRITABLE;
        let new_mode = self.pbctl & MODE_MASK;
        if self.pbctl & CTL_C2_OUTPUT != 0 {
            self.irqb2 = false;
        }
        if old_mode == MODE_LOW && new_mode == MODE_HIGH {
            self.cb2_rose = true;
        } else if new_mode == MODE_LOW {
            self.cb2_rose = false;
        } else if self.cb2_rose && new_mode & CTL_C2_OUTPUT == 0 {
            // CB2 from output high, after a low-to-high change, to an input mode.
            self.irqb2 = true;
            self.cb2_rose = false;
        }
        if old_mode != new_mode {
            self.cb2_low = false;
            self.cb2_pulse = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FLOAT_DECAY_CYCLES, Pia};

    /// `PACTL` register offset.
    const PACTL: u8 = 2;
    /// `PBCTL` register offset.
    const PBCTL: u8 = 3;
    /// `PORTA` register offset.
    const PORTA: u8 = 0;
    /// `PORTB` register offset.
    const PORTB: u8 = 1;

    /// A PIA with `DDRB` = `ddrb`, the port B output register = `orb`, and `PBCTL` = `pbctl`.
    fn port_b(ddrb: u8, orb: u8, pbctl: u8) -> Pia {
        let mut pia = Pia::new();
        pia.write(PBCTL, 0x00);
        pia.write(PORTB, ddrb);
        pia.write(PBCTL, 0x04);
        pia.write(PORTB, orb);
        pia.write(PBCTL, pbctl);
        pia
    }

    /// Runs `n` machine cycles.
    fn ticks(pia: &mut Pia, n: u32) {
        for _ in 0..n {
            pia.tick();
        }
    }

    /// Verifies REQ-218: the PIA decodes only the low two address bits (criteria 1 and 3).
    #[test]
    fn registers_mirrored_every_four() {
        let mut pia = Pia::new();
        pia.write(0xFE, 0x3C);
        assert_eq!(pia.read(0x02), 0x3C, "1: PACTL written at $D3FE");
        pia.write(0xFF, 0x34);
        assert_eq!(pia.read(0x03), 0x34, "3: PBCTL written at $D3FF");
        assert_eq!(pia.peek(0x07), 0x34, "PBCTL read at a mirror");
    }

    /// Verifies REQ-218: a read at a mirror address has the side effects of the register
    /// (criterion 2).
    #[test]
    fn mirror_read_side_effects() {
        let mut pia = port_b(0xFF, 0xFD, 0x3C);
        pia.set_cb1(false);
        assert_eq!(pia.peek(PBCTL), 0xBC, "PBCTL bit 7 set");
        assert_eq!(pia.read(0x05), 0xFD, "PORTB at $D305");
        assert_eq!(pia.peek(PBCTL), 0x3C, "PBCTL bit 7 cleared");
    }

    /// Verifies REQ-219: `PACTL` bit 2 = 0 selects `DDRA` (criterion 1).
    #[test]
    fn ddr_selected_by_bit_2_clear() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x30);
        pia.write(PORTA, 0xF0);
        assert_eq!(pia.read(PORTA), 0xF0, "DDRA");
    }

    /// Verifies REQ-219: `PACTL` bit 2 = 1 selects the data register (criterion 2).
    #[test]
    fn data_register_selected_by_bit_2_set() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x30);
        pia.write(PORTA, 0xF0);
        pia.write(PACTL, 0x34);
        pia.write(PORTA, 0x55);
        assert_eq!(pia.read(PORTA), 0x5F, "inputs high, outputs from ORA");
        pia.write(PACTL, 0x30);
        assert_eq!(pia.read(PORTA), 0xF0, "DDRA did not change");
    }

    /// Verifies REQ-219: each `DDRA` bit selects the direction of its bit (criterion 3).
    #[test]
    fn ddr_bit_selects_direction() {
        let mut pia = Pia::new();
        pia.write(PORTA, 0x01);
        pia.write(PACTL, 0x34);
        pia.write(PORTA, 0x00);
        assert_eq!(pia.read(PORTA), 0xFE, "bit 0 is an output");
        pia.write(PACTL, 0x30);
        pia.write(PORTA, 0x00);
        pia.write(PACTL, 0x34);
        assert_eq!(pia.read(PORTA), 0xFF, "all bits are inputs");
    }

    /// Verifies REQ-219: the output register keeps its value while `DDRA` is selected
    /// (criterion 4).
    #[test]
    fn output_register_kept_while_ddr_selected() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x34);
        pia.write(PORTA, 0x55);
        pia.write(PACTL, 0x30);
        pia.write(PORTA, 0xFF);
        pia.write(PACTL, 0x34);
        assert_eq!(pia.read(PORTA), 0x55, "ORA kept its value");
    }

    /// Verifies REQ-219: `PBCTL` bit 2 selects `DDRB` or the port B data register
    /// (criterion 5).
    #[test]
    fn port_b_ddr_select() {
        let mut pia = Pia::new();
        pia.write(PBCTL, 0x38);
        pia.write(PORTB, 0xFF);
        assert_eq!(pia.read(PORTB), 0xFF, "DDRB");
        pia.write(PBCTL, 0x3C);
        pia.write(PORTB, 0xFD);
        assert_eq!(pia.read(PORTB), 0xFD, "port B data");
    }

    /// Verifies REQ-220: an output bit reads as the AND of the output and the pin
    /// (criteria 1 and 2).
    #[test]
    fn port_a_output_reads_and_of_pins() {
        let mut pia = Pia::new();
        pia.write(PORTA, 0xF0); // DDRA
        pia.write(PACTL, 0x3C);
        pia.write(PORTA, 0x00);
        assert_eq!(pia.read(PORTA), 0x0F, "1: outputs low");
        pia.write(PORTA, 0xF0);
        pia.set_port_a_input(0xEF);
        assert_eq!(pia.read(PORTA), 0xEF, "2: pin 4 pulled low");
    }

    /// Verifies REQ-220: an input bit reads the pin (criterion 3).
    #[test]
    fn port_a_input_reads_pins() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x3C);
        pia.set_port_a_input(0xFB);
        assert_eq!(pia.read(PORTA), 0xFB, "pin 2 pulled low");
        assert_eq!(pia.port_a_input(), 0xFB, "pins kept");
    }

    /// Verifies REQ-221: port A inputs read 1 with no input (criterion 1).
    #[test]
    fn port_a_pulled_up() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x3C);
        assert_eq!(pia.read(PORTA), 0xFF, "no joystick pressed");
    }

    /// Verifies REQ-221: port A bits do not float after a change from output 0 to input
    /// (criterion 3).
    #[test]
    fn port_a_inputs_do_not_float() {
        let mut pia = Pia::new();
        pia.write(PORTA, 0xFF); // DDRA
        pia.write(PACTL, 0x3C);
        pia.write(PORTA, 0x00);
        pia.write(PACTL, 0x38);
        pia.write(PORTA, 0x00); // DDRA = $00
        pia.write(PACTL, 0x3C);
        ticks(&mut pia, 10);
        assert_eq!(pia.read(PORTA), 0xFF, "10 cycles after the write");
        ticks(&mut pia, 1_000_000 - 10);
        assert_eq!(pia.read(PORTA), 0xFF, "1,000,000 cycles after the write");
    }

    /// Verifies REQ-222: an output bit reads the output register (criterion 1).
    #[test]
    fn port_b_output_reads_latch() {
        let mut pia = port_b(0xFF, 0xFD, 0x3C);
        assert_eq!(pia.read(PORTB), 0xFD, "PORTB");
        assert_eq!(pia.port_b_pins(), 0xFD, "pins");
    }

    /// Verifies REQ-222: after a reset, port B reads `$83` (criterion 2), and the memory map
    /// sees bits 0, 1 and 7 high (REQ-211).
    #[test]
    fn port_b_inputs_after_reset() {
        let mut pia = port_b(0xFF, 0xFF, 0x3C);
        pia.reset();
        pia.write(PBCTL, 0x04);
        assert_eq!(pia.read(PORTB), 0x83, "bits 0, 1 and 7 high, bits 2-6 low");
        assert_eq!(
            pia.port_b_pins() & 0x83,
            0x83,
            "pull-ups for the memory map"
        );
    }

    /// Verifies REQ-222: the pull-ups do not change output bits (criterion 3).
    #[test]
    fn port_b_output_zero_not_pulled_up() {
        let mut pia = port_b(0xFF, 0x00, 0x3C);
        assert_eq!(pia.read(PORTB), 0x00, "PORTB");
        assert_eq!(pia.port_b_pins(), 0x00, "pins");
    }

    /// Verifies REQ-223: a bit 2-6 that was output 1 reads 1 as an input, then 0 after the
    /// delay (criteria 1, 2, 3 and 5).
    #[test]
    fn floating_bit_reads_one_then_zero() {
        for bit in 2..=6 {
            let mask = 1_u8 << bit;
            let mut pia = port_b(mask, mask, 0x38);
            pia.write(PORTB, 0x00); // DDRB = $00
            pia.write(PBCTL, 0x3C);
            ticks(&mut pia, 10);
            assert_eq!(pia.read(PORTB) & mask, mask, "bit {bit} after 10 cycles");
            ticks(&mut pia, 178_977 - 10);
            assert_eq!(pia.read(PORTB) & mask, mask, "bit {bit} after 100 ms");
            ticks(&mut pia, 894_887 - 178_977);
            assert_eq!(pia.read(PORTB) & mask, 0, "bit {bit} after 500 ms");
        }
        let mut pia = port_b(0x04, 0x04, 0x38);
        pia.write(PORTB, 0x00);
        ticks(&mut pia, FLOAT_DECAY_CYCLES - 1);
        assert_eq!(pia.peek(PBCTL), 0x38, "PBCTL");
        pia.write(PBCTL, 0x3C);
        assert_eq!(
            pia.read(PORTB),
            0x87,
            "one cycle before the end of the delay"
        );
        pia.tick();
        assert_eq!(pia.read(PORTB), 0x83, "at the end of the delay");
        // A bit that is an output again does not float.
        let mut again = port_b(0x04, 0x04, 0x38);
        again.write(PORTB, 0x00);
        again.write(PORTB, 0x04);
        again.write(PBCTL, 0x3C);
        again.write(PORTB, 0x00);
        assert_eq!(again.read(PORTB), 0x83, "output 0 after floating");
    }

    /// Verifies REQ-223: a bit 2-6 that was output 0 reads 0 at once as an input (criteria 4
    /// and 5).
    #[test]
    fn floating_bit_from_zero_reads_zero() {
        for bit in 2..=6 {
            let mask = 1_u8 << bit;
            let mut pia = port_b(mask, 0x00, 0x38);
            pia.write(PORTB, 0x00);
            pia.write(PBCTL, 0x3C);
            ticks(&mut pia, 10);
            assert_eq!(pia.read(PORTB) & mask, 0, "bit {bit} after 10 cycles");
        }
    }

    /// Verifies REQ-224: bits 6 and 7 of a control write are ignored (criteria 1 and 5).
    #[test]
    fn control_write_ignores_bits_6_7() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0xFF);
        assert_eq!(pia.read(PACTL), 0x3F, "1: PACTL");
        pia.write(PACTL, 0xC0);
        assert_eq!(pia.read(PACTL), 0x00, "5: PACTL");
    }

    /// Verifies REQ-224: a control read shows the interrupt status in bits 7 and 6
    /// (criteria 2 to 4).
    #[test]
    fn control_read_shows_status() {
        let mut pia = Pia::new();
        pia.set_cb1(false);
        pia.write(PBCTL, 0x3C);
        assert_eq!(pia.read(PBCTL), 0xBC, "2: PBCTL");
        pia.write(PACTL, 0x3C);
        pia.set_ca1(false);
        assert_eq!(pia.read(PACTL), 0xBC, "3: PACTL after CA1");
        let mut ca2 = Pia::new();
        ca2.write(PACTL, 0x04);
        ca2.set_ca2_input(false);
        assert_eq!(ca2.read(PACTL), 0x44, "4: PACTL after CA2");
    }

    /// Verifies REQ-225: a CA1 high-to-low transition sets bit 7 (criterion 1).
    #[test]
    fn ca1_negative_edge() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x3C);
        pia.set_ca1(false);
        assert_eq!(pia.peek(PACTL) & 0x80, 0x80, "bit 7");
        pia.set_ca1(false);
        assert_eq!(pia.peek(PACTL) & 0x80, 0x80, "no change, bit kept");
    }

    /// Verifies REQ-225: a CA1 low-to-high transition does not set bit 7 in the negative
    /// mode (criterion 2).
    #[test]
    fn ca1_positive_edge_ignored_in_negative_mode() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x3C);
        pia.set_ca1(false);
        let _read = pia.read(PORTA);
        assert_eq!(pia.peek(PACTL) & 0x80, 0, "cleared by the read");
        pia.set_ca1(true);
        assert_eq!(pia.peek(PACTL) & 0x80, 0, "bit 7 stays 0");
    }

    /// Verifies REQ-225: a CB1 low-to-high transition sets bit 7 in the positive mode
    /// (criterion 3).
    #[test]
    fn cb1_positive_edge() {
        let mut pia = Pia::new();
        pia.write(PBCTL, 0x3E);
        pia.set_cb1(false);
        assert_eq!(pia.peek(PBCTL) & 0x80, 0, "negative edge ignored");
        pia.set_cb1(true);
        assert_eq!(pia.peek(PBCTL) & 0x80, 0x80, "bit 7");
    }

    /// Verifies REQ-225: the status is set also when the interrupt is disabled, and bit 0
    /// enables the IRQ (criterion 4).
    #[test]
    fn ca1_status_without_enable() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x3C);
        pia.set_ca1(false);
        assert_eq!(pia.peek(PACTL), 0xBC, "status set");
        assert!(!pia.irq(), "no IRQ with bit 0 = 0");
        let mut cb = Pia::new();
        cb.write(PBCTL, 0x3E);
        cb.set_cb1(false);
        cb.set_cb1(true);
        assert!(!cb.irq(), "no IRQ for CB1 with bit 0 = 0");
        let mut enabled = Pia::new();
        enabled.write(PACTL, 0x3D);
        enabled.set_ca1(false);
        assert!(enabled.irq(), "IRQ with bit 0 = 1");
    }

    /// Verifies REQ-226: a CA2 negative edge sets bit 6 without IRQ when bit 3 = 0
    /// (criterion 1).
    #[test]
    fn ca2_input_negative_edge_no_irq() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x04);
        pia.set_ca2_input(false);
        assert_eq!(pia.peek(PACTL), 0x44, "bit 6");
        assert!(!pia.irq(), "no IRQ");
    }

    /// Verifies REQ-226: in the positive mode, only a low-to-high CA2 transition sets bit 6,
    /// and bit 3 enables the IRQ (criterion 2).
    #[test]
    fn ca2_input_positive_edge_irq() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x1C);
        pia.set_ca2_input(false);
        assert_eq!(pia.peek(PACTL) & 0x40, 0, "negative edge ignored");
        pia.set_ca2_input(true);
        assert_eq!(pia.peek(PACTL) & 0x40, 0x40, "bit 6");
        assert!(pia.irq(), "IRQ");
        // In an output mode, the CA2 input does not set bit 6.
        let mut output = Pia::new();
        output.write(PACTL, 0x3C);
        output.set_ca2_input(false);
        assert_eq!(output.peek(PACTL) & 0x40, 0, "output mode");
    }

    /// Verifies REQ-226: a CB2 negative edge sets bit 6 and the IRQ (criterion 3).
    #[test]
    fn cb2_input_negative_edge_irq() {
        let mut pia = Pia::new();
        pia.write(PBCTL, 0x0C);
        pia.set_cb2_input(false);
        assert_eq!(pia.peek(PBCTL), 0x4C, "bit 6");
        assert!(pia.irq(), "IRQ");
        let mut output = Pia::new();
        output.write(PBCTL, 0x3C);
        output.set_cb2_input(false);
        assert_eq!(output.peek(PBCTL) & 0x40, 0, "output mode");
    }

    /// Verifies REQ-226: CA2 and CB2 are high in input mode with no device (criterion 4).
    #[test]
    fn ca2_cb2_input_idle_high() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x04);
        pia.write(PBCTL, 0x04);
        assert!(pia.ca2(), "CA2 high");
        assert!(pia.cb2(), "CB2 high");
        pia.set_cb2_input(false);
        assert!(!pia.cb2(), "CB2 follows the external level");
    }

    /// Verifies REQ-227: `110` makes the command line low, `111` high (criterion 1).
    #[test]
    fn cb2_command_line_levels() {
        let mut pia = Pia::new();
        pia.write(PBCTL, 0x34);
        assert!(!pia.cb2(), "low");
        pia.write(PBCTL, 0x3C);
        assert!(pia.cb2(), "high");
    }

    /// Verifies REQ-227: `110` makes the motor line low, `111` high (criterion 2).
    #[test]
    fn ca2_motor_line_levels() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x34);
        assert!(!pia.ca2(), "low");
        pia.write(PACTL, 0x3C);
        assert!(pia.ca2(), "high");
    }

    /// Verifies REQ-227: in mode `101`, a port A data read makes CA2 low for one cycle
    /// (criterion 3).
    #[test]
    fn ca2_pulse_one_cycle_on_read() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x2C);
        assert!(pia.ca2(), "high before the read");
        let _read = pia.read(PORTA); // cycle c
        pia.tick();
        assert!(!pia.ca2(), "low at the end of cycle c");
        pia.tick();
        assert!(pia.ca2(), "high at the end of cycle c+1");
        // A DDRA read does not start a pulse.
        pia.write(PACTL, 0x28);
        let _read = pia.read(PORTA);
        assert!(pia.ca2(), "no pulse on a DDRA read");
    }

    /// Verifies REQ-227: in mode `100`, CA2 stays low until a CA1 transition (criterion 4).
    #[test]
    fn ca2_low_until_ca1() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x24);
        let _read = pia.read(PORTA);
        ticks(&mut pia, 1000);
        assert!(!pia.ca2(), "low");
        pia.set_ca1(true);
        assert!(!pia.ca2(), "no CA1 transition");
        pia.set_ca1(false);
        assert!(pia.ca2(), "high after the CA1 transition");
    }

    /// Verifies REQ-227: CB2 pulses after a port B data write in the modes `101` and `100`
    /// (criterion 5).
    #[test]
    fn cb2_pulse_modes_on_write() {
        let mut pia = Pia::new();
        pia.write(PBCTL, 0x2C);
        pia.write(PORTB, 0x00); // cycle c
        pia.tick();
        assert!(!pia.cb2(), "low at the end of cycle c");
        pia.tick();
        assert!(pia.cb2(), "high at the end of cycle c+1");
        pia.write(PBCTL, 0x24);
        pia.write(PORTB, 0x00);
        ticks(&mut pia, 1000);
        assert!(!pia.cb2(), "low until CB1");
        pia.set_cb1(false);
        assert!(pia.cb2(), "high after the CB1 transition");
        pia.write(PORTB, 0x00);
        pia.write(PBCTL, 0x3C);
        pia.write(PBCTL, 0x24);
        assert!(pia.cb2(), "a mode change ends the low level");
    }

    /// Verifies REQ-228: a data register read clears both status bits (criterion 1).
    #[test]
    fn data_read_clears_status() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x04);
        pia.set_ca1(false);
        pia.set_ca2_input(false);
        assert_eq!(pia.peek(PACTL), 0xC4, "both bits set");
        let _read = pia.read(PORTA);
        assert_eq!(pia.read(PACTL), 0x04, "both bits cleared");
        let mut b = Pia::new();
        b.write(PBCTL, 0x04);
        b.set_cb1(false);
        b.set_cb2_input(false);
        assert_eq!(b.peek(PORTB), 0x83, "peek of PORTB");
        assert_eq!(b.peek(PBCTL), 0xC4, "peek has no side effects");
        let _read = b.read(PORTB);
        assert_eq!(b.read(PBCTL), 0x04, "port B bits cleared");
    }

    /// Verifies REQ-228: a DDR read does not clear the status (criterion 2).
    #[test]
    fn ddr_read_keeps_status() {
        let mut pia = Pia::new();
        pia.set_ca1(false);
        pia.set_ca2_input(false);
        let _read = pia.read(PORTA);
        assert_eq!(pia.read(PACTL), 0xC0, "bits kept");
        pia.set_cb1(false);
        let _read = pia.read(PORTB);
        assert_eq!(pia.read(PBCTL), 0x80, "port B bit kept");
    }

    /// Verifies REQ-228: a disable does not clear the status (criterion 3).
    #[test]
    fn disable_keeps_status() {
        let mut pia = Pia::new();
        pia.write(PBCTL, 0x0D);
        pia.set_cb1(false);
        pia.set_cb2_input(false);
        assert!(pia.irq(), "IRQ while enabled");
        pia.write(PBCTL, 0x04);
        assert_eq!(pia.read(PBCTL), 0xC4, "bits kept");
        assert!(!pia.irq(), "no IRQ while disabled");
    }

    /// Verifies REQ-228: an output mode clears bit 6 and keeps bit 7 (criterion 4).
    #[test]
    fn output_mode_clears_bit_6() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x04);
        pia.set_ca1(false);
        pia.set_ca2_input(false);
        pia.write(PACTL, 0x34);
        assert_eq!(pia.read(PACTL), 0xB4, "PACTL");
        let mut b = Pia::new();
        b.write(PBCTL, 0x04);
        b.set_cb2_input(false);
        b.write(PBCTL, 0x3C);
        assert_eq!(b.read(PBCTL), 0x3C, "PBCTL");
    }

    /// Verifies REQ-229: a CA1 transition with bit 0 = 1 asserts IRQ (criterion 1).
    #[test]
    fn irq_from_ca1() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x3D);
        assert!(!pia.irq(), "no IRQ before");
        pia.set_ca1(false);
        assert_eq!(pia.peek(PACTL) & 0x80, 0x80, "bit 7");
        assert!(pia.irq(), "IRQ");
    }

    /// Verifies REQ-229: a CB2 transition in mode `001` asserts IRQ (criterion 2).
    #[test]
    fn irq_from_cb2() {
        let mut pia = Pia::new();
        pia.write(PBCTL, 0x0C);
        pia.set_cb2_input(false);
        assert_eq!(pia.peek(PBCTL) & 0x40, 0x40, "bit 6");
        assert!(pia.irq(), "IRQ");
    }

    /// Verifies REQ-229: the IRQ ends after a data register read (criterion 3).
    #[test]
    fn irq_released_after_read() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x3D);
        pia.set_ca1(false);
        assert!(pia.irq(), "IRQ");
        let _read = pia.read(PORTA);
        pia.tick();
        assert!(!pia.irq(), "no IRQ after the read");
        let mut b = Pia::new();
        b.write(PBCTL, 0x0D);
        b.set_cb1(false);
        assert!(b.irq(), "CB1 IRQ");
        let _read = b.read(PORTB);
        assert!(!b.irq(), "no CB1 IRQ after the read");
    }

    /// Verifies REQ-229: no IRQ while both interrupts are disabled (criterion 4).
    #[test]
    fn no_irq_when_disabled() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x3C);
        pia.set_ca1(false);
        pia.write(PBCTL, 0x04);
        pia.set_cb2_input(false);
        assert_eq!(pia.peek(PACTL), 0xBC, "PACTL bit 7");
        assert_eq!(pia.peek(PBCTL), 0x44, "PBCTL bit 6");
        assert!(!pia.irq(), "no IRQ");
    }

    /// Verifies REQ-230: a reset clears the control registers and the status (criterion 1).
    #[test]
    fn reset_clears_all_registers() {
        let mut pia = Pia::new();
        pia.write(PORTA, 0xFF);
        pia.write(PORTB, 0xFF);
        pia.write(PACTL, 0x3D);
        pia.write(PBCTL, 0x3C);
        pia.write(PORTA, 0xFF);
        pia.write(PORTB, 0xFF);
        pia.set_ca1(false);
        assert_eq!(pia.peek(PACTL), 0xBD, "PACTL before");
        pia.tick();
        pia.reset();
        assert_eq!(pia.read(PACTL), 0x00, "PACTL");
        assert_eq!(pia.read(PBCTL), 0x00, "PBCTL");
        assert_eq!(pia.port_a_input(), 0xFF, "external pins kept");
    }

    /// Verifies REQ-230: after a reset, the port registers select the DDR (criterion 2).
    #[test]
    fn reset_selects_ddr() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x3C);
        pia.reset();
        assert_eq!(pia.read(PORTA), 0x00, "DDRA");
        pia.write(PORTA, 0xFF);
        pia.write(PACTL, 0x04);
        assert_eq!(pia.read(PORTA), 0x00, "the output register is $00");
    }

    /// Verifies REQ-230: a reset clears the output registers (criterion 3).
    #[test]
    fn reset_clears_output_registers() {
        let mut pia = port_b(0xFF, 0xFF, 0x3C);
        pia.reset();
        pia.write(PORTB, 0xFF); // DDRB
        assert_eq!(pia.port_b_pins() & 0x01, 0x00, "OS ROM off: ORB is $00");
        assert_eq!(pia, {
            let mut new = Pia::new();
            new.write(PORTB, 0xFF);
            new
        });
    }

    /// Verifies REQ-230: after a reset, the PIA does not assert IRQ (criterion 4).
    #[test]
    fn reset_no_irq() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x3D);
        pia.set_ca1(false);
        assert!(pia.irq(), "IRQ before");
        pia.reset();
        assert!(!pia.irq(), "no IRQ");
        assert!(pia.ca2() && pia.cb2(), "CA2 and CB2 high");
        pia.set_ca1(true);
        assert_eq!(pia, Pia::default(), "reset state = power-on state");
    }

    /// Verifies REQ-231: CA2 from output low to `010` sets bit 6 without IRQ (criterion 1).
    #[test]
    fn spurious_ca2_positive_edge_mode() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x34);
        pia.write(PACTL, 0x14);
        assert_eq!(pia.peek(PACTL), 0x54, "bit 6");
        assert!(!pia.irq(), "no IRQ");
        let mut negative = Pia::new();
        negative.write(PACTL, 0x34);
        negative.write(PACTL, 0x04);
        assert_eq!(
            negative.peek(PACTL),
            0x04,
            "no bit 6 for a negative edge mode"
        );
    }

    /// Verifies REQ-231: CA2 from output low to `011` sets bit 6 and the IRQ (criterion 2).
    #[test]
    fn spurious_ca2_with_irq() {
        let mut pia = Pia::new();
        pia.write(PACTL, 0x34);
        pia.write(PACTL, 0x1C);
        assert_eq!(pia.peek(PACTL), 0x5C, "bit 6");
        assert!(pia.irq(), "IRQ");
    }

    /// Verifies REQ-231: CB2 from `110` to `111` and then to an input mode sets bit 6
    /// (criterion 3).
    #[test]
    fn spurious_cb2_after_rise() {
        let mut pia = Pia::new();
        pia.write(PBCTL, 0x34);
        pia.write(PBCTL, 0x3C);
        pia.write(PBCTL, 0x08);
        assert_eq!(pia.peek(PBCTL), 0x48, "bit 6");
        assert!(pia.irq(), "IRQ");
        pia.write(PBCTL, 0x04);
        let _read = pia.read(PORTB);
        pia.write(PBCTL, 0x00);
        assert_eq!(pia.peek(PBCTL), 0x00, "only once");
        let mut low_again = Pia::new();
        low_again.write(PBCTL, 0x34);
        low_again.write(PBCTL, 0x3C);
        low_again.write(PBCTL, 0x34);
        low_again.write(PBCTL, 0x00);
        assert_eq!(
            low_again.peek(PBCTL),
            0x00,
            "a change to 110 clears the rise"
        );
    }

    /// Verifies REQ-231: CB2 from `111` with no earlier `110` does not set bit 6
    /// (criterion 4).
    #[test]
    fn no_spurious_cb2_without_low() {
        let mut pia = Pia::new();
        pia.write(PBCTL, 0x3C);
        pia.write(PBCTL, 0x00);
        assert_eq!(pia.peek(PBCTL), 0x00, "bit 6 stays 0");
    }
}
