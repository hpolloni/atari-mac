# atari-mac

## Development

The toolchain is pinned in `rust-toolchain.toml`. `rustup` installs it automatically.

One-time setup:

```sh
make tools   # installs cargo-deny, cargo-hack, cargo-llvm-cov, cargo-machete, taplo, typos
make hooks   # enables the pre-commit hook in .githooks/
make roms    # downloads the ROMs and the small test data (scripts/fetch-roms.sh)
make test-data  # also downloads the 6502 single-step suite (about 1.1 GB)
```

Run every gate locally (the same gates as CI):

```sh
make check
```

### Gates

| Gate | Tool | Command |
| --- | --- | --- |
| Rust formatting | rustfmt | `cargo fmt --all --check` |
| TOML formatting and lint | taplo | `taplo fmt --check`, `taplo lint` |
| Spelling | typos | `typos` |
| Lints (rustc + clippy `all`, `pedantic`, `nursery`, `cargo`, selected `restriction`) | clippy | `cargo clippy --all-targets --all-features` |
| Unit, integration and doc tests | cargo test | `cargo test --all-targets`, `cargo test --doc` |
| Docs build without warnings | rustdoc | `cargo doc --no-deps --document-private-items` |
| Advisories, licenses, duplicate deps, sources | cargo-deny | `cargo deny check` |
| Unused dependencies | cargo-machete | `cargo machete` |
| MSRV (`rust-version` in Cargo.toml) | cargo-hack | `cargo hack check --rust-version` |
| Every feature combination | cargo-hack | `cargo hack check --feature-powerset` |
| Line coverage of at least 80% | cargo-llvm-cov | `cargo llvm-cov --fail-under-lines 80` |
| Workflow lint and security (CI only) | actionlint, zizmor, shellcheck | — |

All warnings are errors (`RUSTFLAGS=-D warnings`). `unsafe` code is forbidden.
To silence a lint, use `#[expect(lint, reason = "...")]`; `#[allow]` is rejected.
