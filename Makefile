# Local quality gates. `make check` runs the same gates as CI.
# Install the extra tools once with `make tools`, and the git hook with `make hooks`.

SHELL := /bin/bash
.SHELLFLAGS := -euo pipefail -c
.DEFAULT_GOAL := check

export RUSTFLAGS := -D warnings
export RUSTDOCFLAGS := -D warnings
export CARGO_TERM_COLOR := always

COVERAGE_MIN := 80

# The frontend (`atari-mac`) builds on macOS only (docs/design/architecture.md, "CI targets").
# On another host, the gates that build code skip it, as the Linux CI jobs do.
ifeq ($(shell uname -s),Darwin)
PACKAGES := --workspace
# cargo-mutants reads `--exclude` as a file glob, so the packages of `PACKAGES` are named.
MUTANTS_PACKAGES := --workspace --test-workspace=true
else
PACKAGES := --workspace --exclude atari-mac
MUTANTS_PACKAGES := --package atari-core --package atari-harness
endif

.PHONY: check pre-commit fmt fmt-check spec-check clippy test doc deny typos toml-check \
        machete msrv features coverage doc-test release roms test-data reference worktree-setup \
        tools hooks clean
.PHONY: mutants

## Full gate set (same as CI, except the OS matrix). `coverage` runs the tests of `test`
## (`cargo llvm-cov --all-targets`), so `check` does not run them a second time.
check: fmt-check toml-check typos spec-check clippy coverage doc-test release doc deny machete \
       msrv features
check: mutants

## Fast subset used by the git pre-commit hook.
pre-commit: fmt-check spec-check clippy test

fmt:
	cargo fmt --all
	taplo fmt

fmt-check:
	cargo fmt --all --check

clippy:
	cargo clippy $(PACKAGES) --all-targets --all-features --locked

## ROMs and the small test data (fast). `make test-data` adds the 1.1 GB single-step suite.
roms:
	scripts/fetch-roms.sh

test-data:
	scripts/fetch-roms.sh --single-step

## The pinned references of docs/spec/references/ (REQ-020). No other target downloads them.
reference:
	scripts/fetch-roms.sh --references

## In a new git worktree: fast-forward to main, copy the ROMs and link test-data/ from the
## main checkout. It does nothing in the main checkout.
worktree-setup:
	scripts/worktree-setup.sh

test: roms doc-test
	cargo test $(PACKAGES) --all-targets --all-features --locked

doc-test:
	cargo test $(PACKAGES) --doc --all-features --locked

## The release build of CI (overflow checks on, LTO).
release:
	cargo build $(PACKAGES) --release --locked

doc:
	cargo doc $(PACKAGES) --no-deps --document-private-items --all-features --locked

deny:
	cargo deny --all-features --locked check

typos:
	typos

## Specification gates (docs/design/spec-gates.md) and the code gates of REQ-058 and REQ-059
## (docs/design/code-gates.md). After a change of the specification, run
## `cargo run -p atari-harness --bin spec-index` to write the generated indexes.
spec-check:
	cargo test -p atari-harness --locked --test spec_gates --test traceability --test code_gates

toml-check:
	taplo fmt --check
	taplo lint

machete:
	cargo machete

msrv:
	cargo hack check --rust-version $(PACKAGES) --all-targets --locked

features:
	cargo hack check --feature-powerset $(PACKAGES) --all-targets --locked

coverage: roms
	cargo llvm-cov $(PACKAGES) --all-targets --all-features --locked --fail-under-lines $(COVERAGE_MIN)

tools:
	cargo install --locked cargo-deny cargo-hack cargo-llvm-cov cargo-machete taplo-cli typos-cli
	cargo install --locked cargo-mutants@27.1.0 cargo-nextest@0.9.151

## Mutation testing of the lines changed from the merge base with origin/main to the working
## tree (REQ-060, docs/design/code-gates.md). The script makes the mutants in a throwaway
## worktree (target/mutants-wt), not in this tree; the settings are in .cargo/mutants.toml and
## the results in target/mutants/mutants.out. Too slow for `pre-commit`. Another base:
## `make mutants MUTANTS_BASE=<commit>`.
MUTANTS_BASE ?= $$(git merge-base origin/main HEAD)
mutants: roms
	scripts/mutants.sh "$(MUTANTS_BASE)" $(MUTANTS_PACKAGES)

hooks:
	git config core.hooksPath .githooks
	@echo "pre-commit hook installed (.githooks/pre-commit)"

clean:
	cargo clean
