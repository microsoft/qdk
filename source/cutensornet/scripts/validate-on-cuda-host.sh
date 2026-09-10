#!/usr/bin/env bash
#
# Validate this checkout of the cuTensorNet crate on a CUDA-capable x86_64 host.
#
# Most of the crate can be checked anywhere, but three things cannot:
#
#   1. `mod library` is gated to linux/x86_64, so its resolver tests silently
#      vanish on any other host -- they do not fail, they simply never run.
#   2. `tests/availability.rs` resolves every required symbol against the real
#      libcutensornet.so.2, which is the only mechanical proof that the symbol
#      names in the manifest exist.
#   3. With --archive, the bindings can be regenerated and compared against the
#      committed file, proving the checked-in bindings are reproducible.
#
# Usage:
#     scripts/validate-on-cuda-host.sh [options]
#
#     --archive <path>   also regenerate the bindings from the cuQuantum archive
#                        and diff them against the committed src/bindings/v2_13.rs
#     --qualification    also run the slow A100 numerical qualification suite
#                        (off by default: it validates simulation behaviour, not
#                        the FFI surface, and needs a real GPU)
#     --skip-hardware    skip everything that needs the native libraries
#
# Run it from anywhere; it validates the checkout it lives in and touches no
# other directory. Deliberately no bundle handling, no cloning and no pinned
# commit hashes: those belong to whatever transfer workflow got the code here,
# and baking them in makes the script stale the moment a commit is added.
#
# Note there are no expected test *counts* either. An earlier version of this
# script hard-coded them and reported a false failure when the real total was
# correct, because tests move between platforms as modules are re-included.
# What actually matters is that the x86_64-only modules ran at all, so that is
# what is asserted.

set -uo pipefail

CRATE_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly CRATE_ROOT
readonly PACKAGE="qdk_cutensornet"
readonly CUTENSORNET_SO="/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2"
readonly CUDART_SO="/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12"

archive=""
skip_hardware=0
qualification=0
failed=0

usage() {
    # Print the header comment block, so the usage text and the file's own
    # documentation cannot drift apart. Stops at the first non-comment line
    # rather than a hard-coded range, which goes stale whenever the block grows.
    awk 'NR >= 3 { if (/^#/) { sub(/^# ?/, ""); print } else { exit } }' "${BASH_SOURCE[0]}"
}

fail() {
    printf 'FAIL: %s\n' "$*"
    failed=1
}

step() {
    printf '\n=== %s ===\n' "$*"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --archive)
            [[ $# -ge 2 ]] || { usage >&2; exit 2; }
            archive="$2"
            shift 2
            ;;
        --skip-hardware)
            skip_hardware=1
            shift
            ;;
        --qualification)
            qualification=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            printf 'unknown argument: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

cd -- "$CRATE_ROOT" || exit 1
test_log="$(mktemp)"
trap 'rm -f -- "$test_log"' EXIT

step "0. host"
printf 'crate  = %s\n' "$CRATE_ROOT"
printf 'arch   = %s\n' "$(uname -m)"
printf 'kernel = %s\n' "$(uname -sr)"
if [[ "$(uname -m)" != "x86_64" || "$(uname -s)" != "Linux" ]]; then
    fail "not linux/x86_64; \`mod library\` is cfg'd out and this run proves nothing"
    printf '\nRefusing to continue: a green result here would be misleading.\n'
    exit 1
fi

step "1. toolchain"
for tool in cargo rustc rustfmt python3; do
    if command -v "$tool" >/dev/null; then
        printf '%-8s = %s\n' "$tool" "$("$tool" --version 2>&1 | head -1)"
    else
        fail "$tool is not available"
    fi
done

step "2. generated loader is current and unedited"
if cargo run -q -p "$PACKAGE" --bin generate-loader -- --check; then
    printf 'OK: src/library/symbols{,/*}.rs match the manifest\n'
else
    fail "generated loader is stale or hand-edited (run: cargo run -p $PACKAGE --bin generate-loader)"
fi

step "3. cargo fmt"
if cargo fmt -p "$PACKAGE" -- --check; then
    printf 'OK: formatted\n'
else
    fail "cargo fmt --check reported diffs"
fi

step "4. cargo clippy (native x86_64, all targets)"
if cargo clippy -p "$PACKAGE" --all-targets -- -D warnings; then
    printf 'OK: clippy clean\n'
else
    fail "clippy reported problems"
fi

step "5. cargo test"
cargo test -p "$PACKAGE" 2>&1 | tee "$test_log" | grep -E 'Running|Doc-tests|test result|^test .* FAILED'
if [[ "${PIPESTATUS[0]}" -ne 0 ]]; then
    fail "cargo test failed"
fi

step "5b. the x86_64-only resolver tests actually ran"
# `mod library` is cfg-gated, so on the wrong host these vanish rather than
# fail. Asserting they appear is the check; their count is not the point.
if grep -qE '^test library::tests::' "$test_log"; then
    printf 'OK: `mod library` compiled and its tests ran:\n'
    grep -E '^test library::(tests|simulation::(tests|session::tests))::' "$test_log" | sed 's/^/  /'
else
    fail "no library::tests::* ran -- \`mod library\` was compiled out"
fi

step "6. native libraries"
# The default paths are only where they happen to live on the reference host.
# If they are elsewhere, point the crate's own overrides at them rather than
# declaring the host unusable.
locate_library() {
    local expected="$1" soname="$2" override="$3" candidate

    if [[ -f "$expected" ]]; then
        printf '  found   %s\n' "$expected"
        return 0
    fi

    printf '  MISSING %s\n' "$expected"
    # ldconfig's cache is authoritative for the dynamic linker and answers
    # instantly; a bounded find over the usual install roots is the fallback.
    # Do not search from / -- on hosts with network or WSL mounts that takes
    # minutes and can hang the whole run.
    candidate="$(ldconfig -p 2>/dev/null | awk -v s="$soname" '$1 == s { print $NF; exit }')"
    if [[ -z "$candidate" ]]; then
        candidate="$(find /usr/lib /usr/local /opt -name "$soname*" -not -type d 2>/dev/null | head -1)"
    fi
    if [[ -z "$candidate" ]]; then
        return 1
    fi
    export "$override=$candidate"
    printf '  -> %s=%s\n' "$override" "$candidate"
}

have_cutensornet=1
locate_library "$CUTENSORNET_SO" libcutensornet.so.2 QDK_CUTENSORNET_LIBRARY || have_cutensornet=0
locate_library "$CUDART_SO" libcudart.so.12 QDK_CUDART_LIBRARY || true

step "7. hardware tests"
# Two very different ignored suites live in this crate:
#
#   tests/availability.rs  - resolves every required symbol against the real
#                            library and reads the version triple. Cheap,
#                            deterministic, no GPU work. This is the FFI guard.
#   replay.rs (7 tests)    - A100 numerical qualification runs. Expensive, need
#                            a real GPU, and some are steered by QDK_CUTENSORNET_*
#                            env vars. They validate simulation behaviour, not
#                            the symbol surface.
#
# Default to the first, since that is what a manifest or loader change can
# break. The second is opt-in via --qualification.
if [[ "$skip_hardware" -eq 1 ]]; then
    printf 'SKIPPED (--skip-hardware)\n'
elif [[ "$have_cutensornet" -eq 0 ]]; then
    fail "cuTensorNet not found; re-run with --skip-hardware to accept that gap knowingly"
else
    printf -- '-- symbol resolution against the installed library --\n'
    if cargo test -p "$PACKAGE" --test availability -- --ignored --nocapture; then
        printf 'OK: every required symbol resolved\n'
    else
        fail "symbol resolution against the real library failed"
    fi

    if [[ "$qualification" -eq 1 ]]; then
        printf -- '-- A100 numerical qualification (slow) --\n'
        if cargo test -p "$PACKAGE" --lib -- --ignored --nocapture; then
            printf 'OK: qualification suite passed\n'
        else
            fail "A100 qualification suite failed"
        fi
    else
        printf -- '-- A100 numerical qualification: SKIPPED (pass --qualification) --\n'
    fi
fi

step "8. bindings are reproducible"
if [[ -z "$archive" ]]; then
    printf 'SKIPPED (pass --archive <cuquantum archive> to reproduce the bindings)\n'
elif [[ ! -f "$archive" ]]; then
    fail "archive not found: $archive"
else
    regenerated="$(mktemp --suffix=.rs)"
    if scripts/generate-bindings.sh "$archive" "$regenerated"; then
        if cmp --silent -- "$regenerated" src/bindings/v2_13.rs; then
            printf 'OK: regenerated bindings are byte-identical to the committed file\n'
        else
            fail "regenerated bindings differ from src/bindings/v2_13.rs"
            diff -u src/bindings/v2_13.rs "$regenerated" | head -40
        fi
    else
        fail "generate-bindings.sh failed"
    fi
    rm -f -- "$regenerated"
fi

step "verdict"
if [[ "$failed" -eq 0 ]]; then
    printf 'ALL CHECKS PASSED for %s\n' "$(git -C "$CRATE_ROOT" rev-parse --short HEAD 2>/dev/null || echo 'unknown revision')"
else
    printf 'ONE OR MORE CHECKS FAILED -- see the FAIL lines above\n'
fi
exit "$failed"
