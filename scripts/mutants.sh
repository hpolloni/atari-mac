#!/usr/bin/env bash
# Mutation testing of the changed lines (REQ-060, docs/design/code-gates.md "Mutation testing").
#
# Usage: scripts/mutants.sh BASE [CARGO-MUTANTS ARGUMENTS...]
#
# The diff is from the commit BASE to the working tree (`git diff BASE`, so a new file counts
# after `git add`). It goes to target/mutants/mutants.diff, and `cargo mutants --in-diff` makes
# the mutants of its added and modified lines only. The other arguments go to cargo-mutants:
# the packages, and in CI the shard (`--shard K/N --sharding round-robin`). The settings are
# in .cargo/mutants.toml.
#
# The mutants are not made in this checkout. The script makes a throwaway git worktree in
# target/mutants-wt: HEAD (detached), the uncommitted changes of this tree applied on top, and
# the ROMs and the test-data/ link of `scripts/worktree-setup.sh --no-fast-forward`. In that
# worktree, `cargo mutants --in-place` changes one source file at a time. A hard kill can only
# leave a mutation in the throwaway worktree, and the next run removes it. The build directory
# (target/mutants/build) and the results (target/mutants/mutants.out) stay in this tree.
#
# The gate fails when a mutant is `missed`. A `caught`, `timeout` or `unviable` mutant passes
# (REQ-060 criterion 2). A diff with no added or modified line of Rust code passes without a
# mutant run (criterion 5).
set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "usage: $0 BASE [PACKAGE ARGUMENTS...]" >&2
  exit 1
fi
base=$1
shift

top=$(git rev-parse --show-toplevel)
cd "$top"
out=$top/target/mutants
wt=$top/target/mutants-wt

mkdir -p "$out"
git diff "$base" >"$out/mutants.diff"
if ! git diff --numstat "$base" -- '*.rs' | awk '$1 > 0 { found = 1 } END { exit !found }'; then
  echo "mutants: no line of Rust code is added or modified since $base: pass"
  exit 0
fi

remove_worktree() {
  if [ -e "$wt" ]; then
    git worktree remove --force "$wt" 2>/dev/null || rm -rf "$wt"
  fi
  git worktree prune
}

# A worktree that an earlier run left behind (for example after a hard kill).
remove_worktree
trap remove_worktree EXIT

git worktree add --quiet --detach "$wt" HEAD
git diff HEAD --binary >"$out/uncommitted.diff"
if [ -s "$out/uncommitted.diff" ]; then
  git -C "$wt" apply --binary "$out/uncommitted.diff"
fi
(cd "$wt" && scripts/worktree-setup.sh --no-fast-forward)

# Each mutant changes one source file and builds again. An incremental build compiles only the
# changed parts of the crate: in CI (where rust-cache sets CARGO_INCREMENTAL=0) a build of a
# mutant took about 31 s without it.
status=0
(cd "$wt" && CARGO_INCREMENTAL=1 CARGO_TARGET_DIR=$out/build cargo mutants --in-place \
  --in-diff "$out/mutants.diff" --output "$out" "$@") || status=$?

# cargo-mutants exits with 3 when a mutant timed out, also when another mutant is missed.
if [ "$status" -eq 3 ]; then
  if [ -s "$out/mutants.out/missed.txt" ]; then
    exit 2
  fi
  echo "mutants: no mutant is missed (a timeout counts as caught): pass"
  exit 0
fi
exit "$status"
