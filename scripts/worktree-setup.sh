#!/usr/bin/env bash
# Sets up a new git worktree of this repository (`make worktree-setup`):
#   1. fast-forwards the current branch to `main` when the branch is behind `main` and has
#      no commits of its own (a branch that has diverged is not changed),
#   2. copies `roms/*.rom` from the main checkout (no other files of `roms/`),
#   3. makes the symlink `test-data` to the `test-data/` of the main checkout.
# The main checkout is the first entry of `git worktree list --porcelain`. In the main
# checkout, the script does nothing. Each step prints one line. The ROMs and the symlink are
# ignored by git (`.gitignore`) and are never committed.
set -euo pipefail

readonly MAIN_BRANCH=main

top=$(git rev-parse --show-toplevel)
cd "$top"
here=$(pwd -P)

main_checkout=$(git worktree list --porcelain | sed -n '1s/^worktree //p')
if [[ -z "$main_checkout" || ! -d "$main_checkout" ]]; then
    echo "worktree-setup: cannot find the main checkout in 'git worktree list'" >&2
    exit 1
fi
main_checkout=$(cd "$main_checkout" && pwd -P)

if [[ "$here" == "$main_checkout" ]]; then
    echo "worktree-setup: $here is the main checkout, nothing to do"
    exit 0
fi

# 1. Fast-forward to main.
if ! git rev-parse --verify --quiet "$MAIN_BRANCH^{commit}" >/dev/null; then
    echo "fast-forward: skipped, there is no branch '$MAIN_BRANCH'"
elif [[ "$(git rev-parse HEAD)" == "$(git rev-parse "$MAIN_BRANCH")" ]]; then
    echo "fast-forward: skipped, already at $MAIN_BRANCH ($(git rev-parse --short HEAD))"
elif git merge-base --is-ancestor HEAD "$MAIN_BRANCH"; then
    old=$(git rev-parse --short HEAD)
    if git merge --ff-only --quiet "$MAIN_BRANCH" >/dev/null; then
        echo "fast-forward: done, $old -> $(git rev-parse --short HEAD) ($MAIN_BRANCH)"
    else
        echo "fast-forward: FAILED, 'git merge --ff-only $MAIN_BRANCH' did not complete" \
            "(local changes?); the branch is not changed" >&2
        exit 1
    fi
elif git merge-base --is-ancestor "$MAIN_BRANCH" HEAD; then
    echo "fast-forward: skipped, the branch contains $MAIN_BRANCH and has its own commits"
else
    echo "fast-forward: skipped, the branch has diverged from $MAIN_BRANCH" \
        "(it has its own commits); not changed, merge or rebase by hand"
fi

# 2. Copy the ROMs.
mkdir -p roms
found=false
for src in "$main_checkout"/roms/*.rom; do
    [[ -f "$src" ]] || continue
    found=true
    name=$(basename "$src")
    dst="roms/$name"
    if [[ -f "$dst" ]] && cmp -s "$src" "$dst"; then
        echo "roms: skipped $name, already there and identical"
    elif [[ -e "$dst" ]]; then
        cp "$src" "$dst"
        echo "roms: replaced $name (it was different from the main checkout)"
    else
        cp "$src" "$dst"
        echo "roms: copied $name"
    fi
done
if [[ "$found" == false ]]; then
    echo "roms: skipped, the main checkout has no roms/*.rom (run 'make roms' there)"
fi

# 3. Link the test data.
target="$main_checkout/test-data"
if [[ -L test-data ]]; then
    current=$(readlink test-data)
    if [[ "$current" == "$target" ]]; then
        echo "test-data: skipped, the link already points to $target"
    else
        echo "test-data: skipped, test-data is a link to $current, not to $target;" \
            "not changed"
    fi
elif [[ -e test-data ]]; then
    echo "test-data: skipped, test-data exists and is not a link; not changed"
elif [[ ! -d "$target" ]]; then
    echo "test-data: skipped, the main checkout has no test-data/ (run 'make roms' there)"
else
    ln -s "$target" test-data
    echo "test-data: linked test-data -> $target"
fi
