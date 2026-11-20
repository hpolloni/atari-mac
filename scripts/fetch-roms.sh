#!/usr/bin/env bash
# Downloads the ROMs, the test data and the pinned references. Every source is pinned to a
# commit or an archive snapshot, and every file is verified with a checksum (REQ-020). Files
# that are present and valid are not downloaded again.
#
# Usage:
#   scripts/fetch-roms.sh                 ROMs and the Klaus Dormann 6502 test (small)
#   scripts/fetch-roms.sh --single-step   also the SingleStepTests 6502 suite (about 1.1 GB)
#   scripts/fetch-roms.sh --references    only the references with `storage: pin` of
#                                         docs/spec/references/ (`make reference`)
#
# Output (git ignores these directories):
#   roms/altirraos-xl.rom, roms/altirra-basic.rom          see roms/README.md
#   test-data/klaus/6502_functional_test.bin                GPL-3.0, test use only
#   test-data/single-step/6502/<opcode>.json                MIT
#   reference/pinned/<file>                                 the `file` of each registry entry
# The HRM and the FAQ are in git (reference/README.md), so no download is necessary for them.
set -euo pipefail

cd "$(dirname "$0")/.."

readonly ATARI800_COMMIT=98cc2dc749867c9c7317a0271ba062a6c797f886
readonly KLAUS_COMMIT=7954e2dbb49c469ea286070bf46cdd71aeb29e4b
readonly SINGLE_STEP_COMMIT=2f6980a2d95757486c7bee24355c360e40e2a224

single_step=false
references=false
for arg in "$@"; do
    case "$arg" in
        --single-step) single_step=true ;;
        --references) references=true ;;
        -h | --help)
            sed -n '2,17p' "$0"
            exit 0
            ;;
        *)
            echo "unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

# sha256sum is on Linux and in Git Bash on Windows; shasum is on macOS.
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# verify_sha256 <file> <expected sha256>
verify_sha256() {
    [[ -f "$1" && "$(sha256_of "$1")" == "$2" ]]
}

# fetch <url> <dest> <sha256>
fetch() {
    local url=$1 dest=$2 sha=$3
    if verify_sha256 "$dest" "$sha"; then
        echo "ok      $dest"
        return
    fi
    mkdir -p "$(dirname "$dest")"
    curl --fail --silent --show-error --location --retry 3 --output "$dest.part" "$url"
    if ! verify_sha256 "$dest.part" "$sha"; then
        echo "checksum mismatch: $url" >&2
        rm -f "$dest.part"
        exit 1
    fi
    mv "$dest.part" "$dest"
    echo "fetched $dest"
}

# fetch_c_array <atari800 source name> <dest> <sha256 of the extracted ROM>
# The atari800 project stores the Altirra ROMs as C byte arrays. This extracts the bytes.
fetch_c_array() {
    local name=$1 dest=$2 sha=$3
    if verify_sha256 "$dest" "$sha"; then
        echo "ok      $dest"
        return
    fi
    mkdir -p "$(dirname "$dest")"
    curl --fail --silent --show-error --location --retry 3 \
        "https://raw.githubusercontent.com/atari800/atari800/$ATARI800_COMMIT/src/roms/$name.c" |
        sed -n '/^{/,/^};/p' | grep -oE '0x[0-9a-fA-F]{2}' | sed 's/^0x//' | xxd -r -p \
        >"$dest.part"
    if ! verify_sha256 "$dest.part" "$sha"; then
        echo "checksum mismatch after extraction: $name.c" >&2
        rm -f "$dest.part"
        exit 1
    fi
    mv "$dest.part" "$dest"
    echo "fetched $dest"
}

# frontmatter_value <file> <key>: the value of a top-level key of the YAML frontmatter of a
# registry entry, without the quotes.
frontmatter_value() {
    awk -v key="$2" '
        NR == 1 && $0 != "---" { exit }
        NR > 1 && $0 == "---" { exit }
        NR > 1 && index($0, key ": ") == 1 {
            value = substr($0, length(key) + 3)
            gsub(/^"|"$/, "", value)
            print value
            exit
        }' "$1"
}

# fetch_references: downloads each registry entry with `storage: pin` from its `url` to its
# `file`, and verifies the file with its `sha256` (REQ-020 criterion 5, REQ-046).
fetch_references() {
    local entry url file sha count=0
    shopt -s nullglob
    for entry in docs/spec/references/*.md; do
        [[ "$(basename "$entry")" == README.md ]] && continue
        [[ "$(frontmatter_value "$entry" storage)" == pin ]] || continue
        url=$(frontmatter_value "$entry" url)
        file=$(frontmatter_value "$entry" file)
        sha=$(frontmatter_value "$entry" sha256)
        if [[ -z "$url" || -z "$sha" || "$file" != reference/pinned/?* ]]; then
            echo "bad registry entry (url, sha256, or file in reference/pinned/): $entry" >&2
            exit 1
        fi
        fetch "$url" "$file" "$sha"
        count=$((count + 1))
    done
    echo "references: $count pinned files"
}

if [[ "$references" == true ]]; then
    fetch_references
    exit 0
fi

fetch_c_array altirraos_xl roms/altirraos-xl.rom \
    9de5a313fe3946f04fe236a8d3ceacb471fbed4ec5fc5db009732e1169946ccf
fetch_c_array altirra_basic roms/altirra-basic.rom \
    19fd64377895eb5d4414319855322dd3f860c21fdc1df9adc0e98ab72c540913

fetch "https://raw.githubusercontent.com/Klaus2m5/6502_65C02_functional_tests/$KLAUS_COMMIT/bin_files/6502_functional_test.bin" \
    test-data/klaus/6502_functional_test.bin \
    fa12bfc761e6f9057e4cc01a665a7b800ff01ae91f598af1e39a1201d01953fd

if [[ "$single_step" == true ]]; then
    # scripts/single-step-6502.sha1 holds the git blob SHA-1 of each file at the pinned commit.
    dir=test-data/single-step/6502
    mkdir -p "$dir"
    missing=()
    while read -r blob file; do
        if [[ ! -f "$dir/$file" || "$(git hash-object "$dir/$file")" != "$blob" ]]; then
            missing+=("$file")
        fi
    done <scripts/single-step-6502.sha1
    echo "single-step: ${#missing[@]} of 256 files to download"
    if ((${#missing[@]} > 0)); then
        printf '%s\n' "${missing[@]}" | xargs -P 8 -I{} curl --fail --silent --show-error \
            --location --retry 3 --output "$dir/{}" \
            "https://raw.githubusercontent.com/SingleStepTests/65x02/$SINGLE_STEP_COMMIT/6502/v1/{}"
        while read -r blob file; do
            if [[ "$(git hash-object "$dir/$file")" != "$blob" ]]; then
                echo "checksum mismatch: $dir/$file" >&2
                rm -f "$dir/$file"
                exit 1
            fi
        done <scripts/single-step-6502.sha1
    fi
    echo "ok      $dir (256 files)"
fi
