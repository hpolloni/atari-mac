//! The priority logic of HRM 6.7 (REQ-424 to REQ-427).
//!
//! The table index is `PRIOR` bits 0-5 (bits 14-9) and 9 input bits: `P0-P3` (bits 0-3,
//! with the missiles when `PRIOR` bit 4 = 0), `PF0-PF3` (bits 4-7) and `M5` (bit 8, any
//! missile when `PRIOR` bit 4 = 1). Each entry is a mask of the color registers: `COLPM0-3`
//! (bits 0-3), `COLPF0-3` (bits 4-7) and `COLBK` (bit 8).

/// The mask bit of `COLBK`.
pub(super) const BAK: u16 = 1 << 8;
/// The mask bit of `COLPF3`.
pub(super) const PF3: u16 = 1 << 7;
/// The number of table entries.
const ENTRIES: usize = 64 * 512;

/// Bit `n` of `value`.
const fn bit(value: u16, n: u32) -> bool {
    (value >> n) & 1 == 1
}

/// The color register mask for `PRIOR` bits 0-5 and the 9 input bits (HRM 6.7 equations).
/// `SF0-SF2` also need `/M5` (project decision, REQ-426).
#[expect(
    clippy::similar_names,
    clippy::nonminimal_bool,
    reason = "the signal names and the equations of HRM 6.7, kept in the HRM form"
)]
pub(super) const fn mask(prior: u8, inputs: u16) -> u16 {
    let prior = prior as u16;
    let [pri0, pri1, pri2, pri3, multi] = [
        bit(prior, 0),
        bit(prior, 1),
        bit(prior, 2),
        bit(prior, 3),
        bit(prior, 5),
    ];
    let [p0, p1, p2, p3] = [
        bit(inputs, 0),
        bit(inputs, 1),
        bit(inputs, 2),
        bit(inputs, 3),
    ];
    let m5 = bit(inputs, 8);
    let [pf0, pf1, pf2] = [bit(inputs, 4), bit(inputs, 5), bit(inputs, 6)];
    let pf3 = bit(inputs, 7) || m5;
    let (pri01, pri12, pri23, pri03) = (pri0 || pri1, pri1 || pri2, pri2 || pri3, pri0 || pri3);
    let (p01, p23, pf01, pf23) = (p0 || p1, p2 || p3, pf0 || pf1, pf2 || pf3);
    let sp0 = p0 && !(pf01 && pri23) && !(pri2 && pf23);
    let sp1 = p1 && !(pf01 && pri23) && !(pri2 && pf23) && (!p0 || multi);
    let sp2 = p2 && !p01 && !(pf23 && pri12) && !(pf01 && !pri0);
    let sp3 = p3 && !p01 && !(pf23 && pri12) && !(pf01 && !pri0) && (!p2 || multi);
    let sf3 = pf3 && !(p23 && pri03) && !(p01 && !pri2);
    let sf01 = !(p23 && pri0) && !(p01 && pri01) && !sf3 && !m5;
    let sf0 = pf0 && sf01;
    let sf1 = pf1 && sf01;
    let sf2 = pf2 && !(p23 && pri03) && !(p01 && !pri2) && !sf3 && !m5;
    let sb = !p01 && !p23 && !pf01 && !pf23;
    let outputs = [sp0, sp1, sp2, sp3, sf0, sf1, sf2, sf3, sb];
    let mut out = 0;
    let mut i = 0;
    while i < outputs.len() {
        if outputs[i] {
            out |= 1 << i;
        }
        i += 1;
    }
    out
}

/// The table for all `PRIOR` bits 0-5 and inputs.
pub(super) fn table() -> Box<[u16]> {
    (0..ENTRIES)
        .map(|index| {
            let prior = u8::try_from(index >> 9).unwrap_or(0);
            let inputs = u16::try_from(index & 0x1FF).unwrap_or(0);
            mask(prior, inputs)
        })
        .collect()
}

/// The table index.
pub(super) fn index(prior: u8, inputs: u16) -> usize {
    (usize::from(prior & 0x3F) << 9) | usize::from(inputs & 0x1FF)
}
