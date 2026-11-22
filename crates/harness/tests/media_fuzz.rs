//! The media fuzz tests: the parsers of the media and the machine never panic on random
//! input (REQ-061). The test names follow the test plan in `docs/design/code-gates.md`.

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

/// Verifies REQ-061: `Cartridge::from_raw` and `Cartridge::from_car` each get at least 1000
/// random inputs of 0 to 4096 bytes and at least 1000 inputs that start with a valid header
/// of the format and continue with random bytes (criteria 1, 2). Each call returns and no
/// call panics, and for each input that is accepted a machine that uses the result runs 10
/// frames and does not panic (criterion 3).
#[test]
#[ignore = "planned: SLICE-12"]
fn cartridge_parsers_never_panic() {
    planned();
}

/// Verifies REQ-061: `Disk::from_atr` gets at least 1000 random inputs of 0 to 4096 bytes and
/// at least 1000 inputs that start with a valid ATR header and continue with random bytes
/// (criteria 1, 2). Each call returns and no call panics, and for each input that is accepted
/// a machine that uses the disk runs 10 frames and does not panic (criterion 3).
#[test]
#[ignore = "planned: SLICE-12"]
fn atr_parser_never_panics() {
    planned();
}

/// Verifies REQ-061: `Tape::from_cas` gets at least 1000 random inputs of 0 to 4096 bytes and
/// at least 1000 inputs that start with a valid CAS header and continue with random bytes
/// (criteria 1, 2). Each call returns and no call panics, and for each input that is accepted
/// a machine that uses the tape runs 10 frames and does not panic (criterion 3).
#[test]
#[ignore = "planned: SLICE-12"]
fn cas_parser_never_panics() {
    planned();
}

/// Verifies REQ-061: `Xex::parse` gets at least 1000 random inputs of 0 to 4096 bytes and at
/// least 1000 inputs that start with a valid XEX header and continue with random bytes
/// (criteria 1, 2). Each call returns and no call panics, and for each input that is accepted
/// a machine that loads the program runs 10 frames and does not panic (criterion 3).
#[test]
#[ignore = "planned: SLICE-12"]
fn xex_parser_never_panics() {
    planned();
}

/// Verifies REQ-061: `Machine::new` gets at least 1000 random ROM byte sets in
/// `MachineConfig` of 0 to 4096 bytes and at least 1000 that start with valid ROM content and
/// continue with random bytes (criteria 1, 2). Each call returns and no call panics, and each
/// machine that is made runs 10 frames and does not panic (criterion 3).
#[test]
#[ignore = "planned: SLICE-12"]
fn rom_check_never_panics() {
    planned();
}

/// Verifies REQ-061: `Machine::load_state` gets at least 1000 random inputs of 0 to 4096
/// bytes and at least 1000 inputs that start with a valid save state header and continue
/// with random bytes (criteria 1, 2). Each call returns and no call panics, and each machine
/// that accepts a state runs 10 frames and does not panic (criterion 3).
#[test]
#[ignore = "planned: SLICE-12"]
fn load_state_never_panics() {
    planned();
}

/// Verifies REQ-061: the random generator has a fixed seed, and the same seed gives the same
/// inputs on each run (criterion 4).
#[test]
#[ignore = "planned: SLICE-12"]
fn fuzz_inputs_are_deterministic() {
    planned();
}
