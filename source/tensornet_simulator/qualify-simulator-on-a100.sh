#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# The A100 qualification of tensornet_simulator: M1's stack (MpsStack, the
# batch strategy and PerShot(TensorNetShots)) over qdk_cutensornet's
# CuTensorNetEngine, on M0's qualification cases (B1-B4, with M0's pinned
# values), against ExactReference (B0, B2), and through the walk shot by shot.
# The tests are src/tests/qualification.rs, all #[ignore]d; this script checks
# the machine, then runs them in release mode, one at a time.
#
# Usage, on the A100 VM:
#   ./qualify-simulator-on-a100.sh [--check | --quick] COMMIT [REPO]
#     COMMIT   the commit to qualify (it must have src/tests/qualification.rs)
#     REPO     the qdk checkout that has COMMIT (default: the current directory)
#     --check  only the checks (host, tools, commit, GPU, libraries, disk);
#              builds and changes nothing
#     --quick  a smoke run: Bell, B1 and the per-shot cases only
# Run --check first, then --quick, then with no option.
#
# It leaves REPO's checkout untouched: it adds a detached worktree at COMMIT
# next to it (../qdk-simulator-qualification) and builds there. Results go to
# $HOME/simulator-qualification-<date>/ and a .tar.gz of it, also when a step
# fails: run.log (the whole cargo output), summary.txt (each test passed or
# failed, then the measured values), environment.txt. Send back the .tar.gz.
#
# Requirements, all checked first (qdk_cutensornet's version.rs accepts these
# exact versions only): Linux x86-64; an A100; cuTensorNet 2.13.0 (21300) for
# CUDA 12; CUDA runtime 12.9 (12090), and cuTensorNet built against 12.9; a
# driver for CUDA 12.9 or newer; rustc 1.85 or newer (edition 2024); python3
# (to read the library versions); 15 GB free next to REPO.
#
# Optional environment:
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY  library paths, as
#       qdk_cutensornet::discover reads them (else the pinned paths, else
#       ldconfig's answer)
#   Q_ALLOW_OTHER_GPU=1  run on a GPU that is not an A100
# Keep the GPU otherwise idle while it runs.
set -euo pipefail

readonly CUTENSORNET_VERSION=21300 # source/cutensornet/src/version.rs
readonly CUDART_VERSION=12090      # the same, for the runtime and cuTensorNet's own
readonly MIN_DRIVER_CUDA=12090
readonly MIN_RUSTC=1.85
readonly MIN_FREE_GB=15
readonly PACKAGE=tensornet_simulator
readonly QUALIFICATION=source/tensornet_simulator/src/tests/qualification.rs
readonly FILTER=tests::qualification::
readonly QUICK_TESTS=(bell_on_the_a100 b1_widths per_shot_bell_pairs_on_the_a100 per_shot_feedback_and_resets_on_the_a100)
readonly CUTENSORNET_PINNED=/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2
readonly CUDART_PINNED=/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12

usage() { printf 'usage: %s [--check | --quick] COMMIT [REPO]\n' "$0" >&2; exit 2; }
mode=run
while [[ $# -gt 0 ]]; do
    case "$1" in
        --check | --quick)
            [[ "$mode" == run ]] || { printf '%s: use one of --check and --quick\n' "$0" >&2; exit 2; }
            mode="${1#--}"
            shift
            ;;
        -h | --help) sed -n '2,/^set -euo/p' "$0" | sed '$d'; exit 0 ;;
        -*) printf 'unknown option %s\n' "$1" >&2; exit 2 ;;
        *) break ;;
    esac
done
[[ $# -ge 1 && $# -le 2 ]] || usage
ref="$1"
[[ -d "${2:-.}" ]] || { printf 'FAIL: %s is not a directory\n' "${2:-.}" >&2; exit 1; }
repo="$(cd "${2:-.}" && pwd)"
work="$(dirname "$repo")/qdk-simulator-qualification"
out=""
probe=""

problems=()
problem() { problems+=("$1"); printf '  FAIL  %s\n' "$1"; }
warn() { printf '  WARN  %s\n' "$*"; }
ok() { printf '  ok    %s\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }
version_ge() { printf '%s\n%s\n' "$2" "$1" | sort -V -C; } # $1 >= $2
step() { printf '\n== %s (%s)\n' "$1" "$(date +%H:%M:%S)"; }
fail() {
    printf '\nFAIL: %s\n' "$1" >&2
    exit 1
}
finish() {
    local status=$?
    if [[ -n "$out" && -d "$out" ]]; then
        tar -czf "$out.tar.gz" -C "$(dirname "$out")" "$(basename "$out")"
        printf '\nResults: %s\nTarball: %s.tar.gz\n' "$out" "$out"
        [[ "$status" -eq 0 ]] || printf 'It failed; send the tarball anyway: it has the logs.\n'
    fi
    if [[ -d "$work" && "$mode" != check ]]; then
        printf 'The worktree stays at %s; remove it later with:\n  git -C %s worktree remove --force %s\n' \
            "$work" "$repo" "$work"
    fi
}
trap finish EXIT

step "1. checks"
printf -- '-- host and tools\n'
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then
    ok "Linux x86-64"
else
    problem "needs x86-64 Linux with an A100 (this is $(uname -s) $(uname -m)); the cuTensorNet engine is compiled out elsewhere"
fi
for tool in git cargo rustc python3 nvidia-smi ldconfig ldd tar df awk sort; do
    have "$tool" || problem "$tool not found"
done
if have rustc && have cargo; then
    # In REPO, so that a rustup override or toolchain file there applies.
    rustc_version="$(cd "$repo" && rustc --version | awk '{ print $2 }')"
    if version_ge "${rustc_version%%-*}" "$MIN_RUSTC"; then
        ok "rustc $rustc_version"
    else
        problem "rustc $MIN_RUSTC or newer needed for edition 2024 (found $rustc_version)"
    fi
fi

printf -- '-- checkout and commit\n'
sha=""
if ! git -C "$repo" rev-parse --git-dir >/dev/null 2>&1; then
    problem "$repo is not a git checkout"
elif ! sha="$(git -C "$repo" rev-parse --verify --quiet "$ref^{commit}")"; then
    sha=""
    problem "commit $ref not in $repo (fetch it first)"
else
    ok "commit $sha"
    for path in Cargo.toml Cargo.lock source/tensornet_simulator/Cargo.toml "$QUALIFICATION" \
        source/cutensornet/src/version.rs; do
        git -C "$repo" cat-file -e "$sha:$path" 2>/dev/null || problem "the commit has no $path"
    done
    policy="$(git -C "$repo" show "$sha:source/cutensornet/src/version.rs" 2>/dev/null || true)"
    grep -q "\"$CUTENSORNET_VERSION\"" <<<"$policy" \
        || problem "the commit's version.rs does not pin cuTensorNet $CUTENSORNET_VERSION"
    grep -q "\"$CUDART_VERSION\"" <<<"$policy" \
        || problem "the commit's version.rs does not pin the CUDA runtime $CUDART_VERSION"
fi
if [[ -e "$work" && -n "$sha" ]]; then
    if [[ "$(git -C "$work" rev-parse HEAD 2>/dev/null)" != "$sha" ]]; then
        problem "$work exists and is not a worktree at $sha; move it away"
    elif [[ -n "$(git -C "$work" status --porcelain)" ]]; then
        problem "$work has changes; move it away"
    else
        ok "reusing the worktree $work"
    fi
fi

printf -- '-- GPU\n'
if have nvidia-smi && nvidia-smi -L >/dev/null 2>&1; then
    index=0
    if [[ -n "${CUDA_VISIBLE_DEVICES:-}" ]]; then
        if [[ "${CUDA_VISIBLE_DEVICES%%,*}" =~ ^[0-9]+$ ]]; then
            index="${CUDA_VISIBLE_DEVICES%%,*}"
        else
            warn "CUDA_VISIBLE_DEVICES=$CUDA_VISIBLE_DEVICES: checking GPU 0, which may not be the one used"
        fi
    fi
    gpu_line="$(nvidia-smi -i "$index" --query-gpu=name,memory.used,mig.mode.current \
        --format=csv,noheader,nounits 2>/dev/null || true)"
    gpu_name="$(cut -d, -f1 <<<"$gpu_line" | xargs)"
    gpu_used="$(cut -d, -f2 <<<"$gpu_line" | xargs)"
    gpu_mig="$(cut -d, -f3 <<<"$gpu_line" | xargs)"
    if [[ -z "$gpu_line" ]]; then
        problem "nvidia-smi cannot query GPU $index"
    elif [[ "$gpu_name" == *A100* ]]; then
        ok "GPU $index: $gpu_name"
    elif [[ "${Q_ALLOW_OTHER_GPU:-0}" == 1 ]]; then
        warn "GPU $index is $gpu_name, not an A100"
    else
        problem "GPU $index is $gpu_name, not an A100 (Q_ALLOW_OTHER_GPU=1 to run anyway)"
    fi
    [[ "$gpu_mig" == Enabled ]] && warn "MIG is enabled: the run gets a slice of the GPU"
    [[ "$gpu_used" =~ ^[0-9]+$ && "$gpu_used" -gt 1024 ]] && warn "$gpu_used MiB of GPU memory already in use"
else
    problem "nvidia-smi cannot reach a GPU (is the driver loaded?)"
fi

printf -- '-- libraries\n'
# The variable if set, else the pinned path, else ldconfig's answer.
locate_library() {
    local pinned="$1" soname="$2" variable="$3" found
    found="${!variable:-}"
    if [[ -n "$found" ]]; then
        [[ -f "$found" ]] || { problem "$variable=$found does not exist"; return; }
    elif [[ -f "$pinned" ]]; then
        found="$pinned"
    else
        found="$(ldconfig -p 2>/dev/null | awk -v s="$soname" '$1 == s { print $NF; exit }')"
        [[ -n "$found" ]] || { problem "$soname not found (set $variable)"; return; }
    fi
    export "$variable=$found"
    ok "$variable=$found -> $(readlink -f "$found")"
    local missing
    missing="$(ldd "$found" 2>/dev/null | awk '/not found/ { print $1 }' | tr '\n' ' ')"
    [[ -z "$missing" ]] || problem "$soname's dependencies not found: $missing(LD_LIBRARY_PATH?)"
}
if have ldconfig && have ldd; then
    locate_library "$CUTENSORNET_PINNED" libcutensornet.so.2 QDK_CUTENSORNET_LIBRARY
    locate_library "$CUDART_PINNED" libcudart.so.12 QDK_CUDART_LIBRARY
fi
if [[ -n "${QDK_CUTENSORNET_LIBRARY:-}" && -n "${QDK_CUDART_LIBRARY:-}" ]] && have python3; then
    # The versions qdk_cutensornet::discover reads at run time, read before building.
    if probe="$(python3 - "$QDK_CUDART_LIBRARY" "$QDK_CUTENSORNET_LIBRARY" 2>&1 <<'EOF'
import ctypes, sys
cudart = ctypes.CDLL(sys.argv[1], mode=ctypes.RTLD_GLOBAL)
tn = ctypes.CDLL(sys.argv[2])
def cuda(name):
    value = ctypes.c_int(0)
    status = getattr(cudart, name)(ctypes.byref(value))
    return value.value if status == 0 else f"error{status}"
tn.cutensornetGetVersion.restype = ctypes.c_size_t
tn.cutensornetGetCudartVersion.restype = ctypes.c_size_t
print(f"cudart={cuda('cudaRuntimeGetVersion')}")
print(f"driver={cuda('cudaDriverGetVersion')}")
print(f"devices={cuda('cudaGetDeviceCount')}")
print(f"cutensornet={tn.cutensornetGetVersion()}")
print(f"cutensornet_cudart={tn.cutensornetGetCudartVersion()}")
EOF
    )"; then
        value() { awk -F= -v k="$1" '$1 == k { print $2 }' <<<"$probe"; }
        if [[ "$(value cutensornet)" == "$CUTENSORNET_VERSION" ]]; then
            ok "cuTensorNet $(value cutensornet)"
        else
            problem "cuTensorNet is $(value cutensornet); qdk_cutensornet accepts $CUTENSORNET_VERSION only"
        fi
        if [[ "$(value cudart)" == "$CUDART_VERSION" ]]; then
            ok "CUDA runtime $(value cudart)"
        else
            problem "the CUDA runtime is $(value cudart); qdk_cutensornet accepts $CUDART_VERSION only"
        fi
        if [[ "$(value cutensornet_cudart)" == "$CUDART_VERSION" ]]; then
            ok "cuTensorNet built for CUDA $(value cutensornet_cudart)"
        else
            problem "cuTensorNet was built for CUDA $(value cutensornet_cudart); $CUDART_VERSION needed"
        fi
        driver="$(value driver)"
        if [[ "$driver" =~ ^[0-9]+$ && "$driver" -ge "$MIN_DRIVER_CUDA" ]]; then
            ok "the driver supports CUDA $driver"
        else
            problem "the driver supports CUDA $driver; $MIN_DRIVER_CUDA or newer needed"
        fi
        devices="$(value devices)"
        if [[ "$devices" =~ ^[1-9][0-9]*$ ]]; then
            ok "the CUDA runtime sees $devices GPU(s)"
        else
            problem "the CUDA runtime sees no GPU ($devices)"
        fi
    else
        problem "loading the libraries failed: $(tail -1 <<<"$probe")"
    fi
fi

printf -- '-- disk and memory\n'
free_gb="$(df -Pk "$(dirname "$repo")" | awk 'NR == 2 { print int($4 / 1048576) }')"
if [[ "$free_gb" -ge "$MIN_FREE_GB" ]]; then
    ok "$free_gb GB free in $(dirname "$repo")"
else
    problem "$free_gb GB free in $(dirname "$repo"); the release build needs about $MIN_FREE_GB"
fi
mem_gb="$(awk '/^MemAvailable:/ { print int($2 / 1048576) }' /proc/meminfo 2>/dev/null || echo 0)"
if [[ "$mem_gb" -ge 8 ]]; then ok "$mem_gb GB of memory available"; else warn "only $mem_gb GB of memory available"; fi

if [[ "${#problems[@]}" -gt 0 ]]; then
    printf '\n%d problem(s):\n' "${#problems[@]}" >&2
    printf '  - %s\n' "${problems[@]}" >&2
    fail "fix the above, then run again (--check checks only)"
fi
if [[ "$mode" == check ]]; then
    printf '\nAll checks passed; nothing was built or changed.\n'
    exit 0
fi

step "2. worktree"
if [[ ! -e "$work" ]]; then
    git -C "$repo" worktree add --detach "$work" "$sha"
fi
out="$HOME/simulator-qualification-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out"
{
    printf 'commit=%s\nmode=%s\nhost=%s\ndate=%s\n' "$sha" "$mode" "$(uname -a)" "$(date -Is)"
    printf 'QDK_CUTENSORNET_LIBRARY=%s\nQDK_CUDART_LIBRARY=%s\n' "$QDK_CUTENSORNET_LIBRARY" "$QDK_CUDART_LIBRARY"
    printf '%s\n' "$probe"
    (cd "$work" && rustc --version && cargo --version)
    nvidia-smi
} >"$out/environment.txt" 2>&1 || true

step "3. build and run"
filters=("$FILTER")
if [[ "$mode" == quick ]]; then
    filters=("${QUICK_TESTS[@]/#/$FILTER}")
fi
command=(cargo test -p "$PACKAGE" --release -- --ignored --nocapture --test-threads=1 "${filters[@]}")
printf '%s\n' "(cd $work && ${command[*]})" | tee "$out/command.txt"
set +e
(cd "$work" && "${command[@]}") 2>&1 | tee "$out/run.log"
status="${PIPESTATUS[0]}"
set -e

step "4. summary"
{
    printf 'commit %s, mode %s, cargo exit status %s\n\n' "$sha" "$mode" "$status"
    # With --nocapture a test's own output follows "test NAME ... " on the
    # same line, and its status comes later on a line of its own.
    awk '
        function status(word) { return word == "ok" ? "passed" : word == "FAILED" ? "failed" : "" }
        /^test [^ ]+ \.\.\. / {
            name = $2
            rest = substr($0, index($0, " ... ") + 5)
            if (status(rest) != "") { print status(rest) "\t" name; name = "" }
            next
        }
        name != "" && status($0) != "" { print status($0) "\t" name; name = "" }
    ' "$out/run.log"
    printf '\n'
    grep -E '^(cutensornet_version|cutensornet_cuda_runtime_version|cudart_version|cuda_driver_version)=' \
        "$out/run.log" | awk '!seen[$0]++' || true
    grep -E '^(trotter|b1) ' "$out/run.log" || true
    grep -E '^test result:' "$out/run.log" || true
} | tee "$out/summary.txt"
[[ "$status" -eq 0 ]] || fail "cargo test exited with $status (see $out/run.log)"
printf '\nAll the qualification tests passed.\n'
