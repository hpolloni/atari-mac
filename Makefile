# Local quality gates. `make check` runs the same gates as CI.
# Install the extra tools once with `make tools`, and the git hook with `make hooks`.

SHELL := /bin/bash
.SHELLFLAGS := -euo pipefail -c
.DEFAULT_GOAL := check

export RUSTFLAGS := -D warnings
export RUSTDOCFLAGS := -D warnings
export CARGO_TERM_COLOR := always

COVERAGE_MIN := 80

.PHONY: check pre-commit fmt fmt-check spec-check clippy test doc deny typos toml-check \
        machete msrv features coverage roms test-data tools hooks clean

## Full gate set (same as CI, except the OS matrix).
check: fmt-check toml-check typos spec-check clippy test doc deny machete msrv features coverage

## Fast subset used by the git pre-commit hook.
pre-commit: fmt-check clippy test

fmt:
	cargo fmt --all
	taplo fmt

fmt-check:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace --all-targets --all-features --locked

## ROMs and the small test data (fast). `make test-data` adds the 1.1 GB single-step suite.
roms:
	scripts/fetch-roms.sh

test-data:
	scripts/fetch-roms.sh --single-step

test: roms
	cargo test --workspace --all-targets --all-features --locked
	cargo test --workspace --doc --all-features --locked

doc:
	cargo doc --workspace --no-deps --document-private-items --all-features --locked

deny:
	cargo deny --all-features --locked check

typos:
	typos

spec-check:
	scripts/req-index.sh --check

toml-check:
	taplo fmt --check
	taplo lint

machete:
	cargo machete

msrv:
	cargo hack check --rust-version --workspace --all-targets --locked

features:
	cargo hack check --feature-powerset --workspace --all-targets --locked

coverage: roms
	cargo llvm-cov --workspace --all-features --locked --fail-under-lines $(COVERAGE_MIN)

tools:
	cargo install --locked cargo-deny cargo-hack cargo-llvm-cov cargo-machete taplo-cli typos-cli

hooks:
	git config core.hooksPath .githooks
	@echo "pre-commit hook installed (.githooks/pre-commit)"

clean:
	cargo clean
