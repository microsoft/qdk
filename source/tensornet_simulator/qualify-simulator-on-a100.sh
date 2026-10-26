#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# The A100 qualification of tensornet_simulator: M1's stack (MpsStack, the
# batch strategy and PerShot(TensorNetShots)) over qdk_cutensornet's
# CuTensorNetEngine, on M0's qualification cases (B1-B4, with M0's pinned
# values), against ExactReference (B0, B2), and through the walk shot by shot;
# and M1's acceptance test, H2 phase estimation on the MPS through the protocol.
# The tests are src/tests/qualification.rs, all #[ignore]d; this script checks
# the machine, then runs them in release mode, one at a time.
#
# Usage, on the A100 VM:
#   ./qualify-simulator-on-a100.sh [--check | --quick] [--h2 FILE [--shots N] [--fusion F] [--svd A] [--gauge G] [--stop-after M] [--readout-distribution TABLE] [--order FILE]] COMMIT [REPO]
#     COMMIT   the commit to qualify (it must have src/tests/qualification.rs)
#     REPO     the qdk checkout that has COMMIT (default: the current directory)
#     --check  only the checks (host, tools, commit, GPU, libraries, disk);
#              builds and changes nothing
#     --quick  a smoke run: Bell, B1 and the per-shot cases only (unfused and
#              in pairs)
#     --h2 FILE  run only H2 (h2_qpe_on_the_a100) on the AdaptiveProgram.as_dict()
#              JSON in FILE, each shot in its own session; with --check, it
#              also checks FILE. Without --h2, the full and quick runs skip H2.
#     --shots N  H2's shots (default 1; at 50 or more it asserts the two peaks).
#              A shot is expected to take minutes, so 50 shots take hours.
#     --fusion F  H2's gate fusion (QDK_H2_FUSION): pairs (the default, each
#              region's gates in blocks of one or two qubits, GateFusion::Pairs)
#              or unfused (one engine operator per gate).
#     --svd A  H2's SVD algorithm (QDK_H2_SVD), cuTensorNet's for each bond:
#              gesvd (the default, M0's), gesvdj (Jacobi), gesvdp (polar) or
#              gesvdr (randomized: approximate, not yet checked on H2).
#     --gauge G  H2's MPS gauge (QDK_H2_GAUGE): simple (the default, M0's,
#              simple update) or free (no gauge).
#     --stop-after M  stop each H2 shot after M measurements
#              (QDK_H2_STOP_AFTER), to time the first part of a shot: the
#              shot has no row and the peaks are not checked. Each shot
#              prints its time, operators and device memory every 10
#              measurements; with --stop-after, the summary keeps those lines.
#     --readout-distribution TABLE  M1's acceptance on the exact readout
#              distribution (QDK_H2_READOUT=distribution): each shot reads
#              the exact joint distribution of its five readout measurements
#              and every mid-circuit P(One); the test compares the first with
#              TABLE (QDK_H2_READOUT_TABLE: the sparse simulator's .readout
#              file, record order, chemistry order, probability) and asserts
#              the two peaks lead it, the TVD and every |P(One) - 1/2| within
#              sqrt(1 - w) + 1e-9 (w the retained weight). One shot suffices.
#              The trace of P(One) goes to h2-trace.txt in the results.
#     --order FILE  H2's qubit order (QDK_H2_ORDER): FILE gives the site of each
#              qubit on the MPS chain, qubit 0 first, whitespace-separated (#
#              starts a comment); without it, qubit q is on site q. The order
#              changes the bonds the state needs and the time, not the values.
#              Each shot prints the mean span of its two-qubit operators and its
#              largest bond.
# Run --check first, then --quick, then with no option; then --h2 with
# --shots 1 --readout-distribution TABLE (the acceptance). To measure fusion, run --h2 FILE
# --stop-after 100 with --fusion unfused and with --fusion pairs; to measure an
# order, run --h2 FILE --stop-after 300 with and without --order ORDER. To measure
# the SVD algorithm and the gauge, run --h2 FILE --stop-after 300 with each --svd
# (and --gauge free with the fastest), then the fastest with --shots 1
# --readout-distribution TABLE.
#
# It leaves REPO's checkout untouched: it adds a detached worktree at COMMIT
# next to it (../qdk-simulator-qualification) and builds there. Results go to
# $HOME/simulator-qualification-<date>/ (for H2,
# simulator-qualification-h2-<fusion>-<svd>-<gauge>-[order-<ORDER's name>-][readout-][m<M>-]<date>/) and a .tar.gz of it, also when a step
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
#   QDK_H2_SEED (0), QDK_H2_CHI (32)  with --h2: the run's seed (shot j, from
#       1, runs in its own session opened on shot j with it, as qdk_simulators'
#       h2_qpe test does) and the bond cap chi
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
readonly QUICK_TESTS=(bell_on_the_a100 b1_widths per_shot_bell_pairs_on_the_a100
    per_shot_feedback_and_resets_on_the_a100 per_shot_phase_estimation_on_the_a100
    per_shot_feedback_and_resets_in_pairs_on_the_a100 per_shot_phase_estimation_in_pairs_on_the_a100)
readonly H2_TEST=h2_qpe_on_the_a100
readonly CUTENSORNET_PINNED=/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2
readonly CUDART_PINNED=/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12

usage() { printf 'usage: %s [--check | --quick] [--h2 FILE [--shots N] [--fusion F] [--svd A] [--gauge G] [--stop-after M] [--readout-distribution TABLE] [--order FILE]] COMMIT [REPO]\n' "$0" >&2; exit 2; }
mode=run
h2=""
shots=""
fusion=""
svd=""
gauge=""
stop_after=""
readout_table=""
order=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --check | --quick)
            [[ "$mode" == run ]] || { printf '%s: use one of --check and --quick\n' "$0" >&2; exit 2; }
            mode="${1#--}"
            shift
            ;;
        --h2)
            [[ $# -ge 2 ]] || usage
            h2="$2"
            shift 2
            ;;
        --shots)
            [[ $# -ge 2 && "$2" =~ ^[1-9][0-9]*$ ]] || { printf '%s: --shots needs a positive integer\n' "$0" >&2; exit 2; }
            shots="$2"
            shift 2
            ;;
        --fusion)
            [[ $# -ge 2 && "$2" =~ ^(unfused|pairs)$ ]] || { printf '%s: --fusion is unfused or pairs\n' "$0" >&2; exit 2; }
            fusion="$2"
            shift 2
            ;;
        --svd)
            [[ $# -ge 2 && "$2" =~ ^(gesvd|gesvdj|gesvdp|gesvdr)$ ]] || { printf '%s: --svd is gesvd, gesvdj, gesvdp or gesvdr\n' "$0" >&2; exit 2; }
            svd="$2"
            shift 2
            ;;
        --gauge)
            [[ $# -ge 2 && "$2" =~ ^(simple|free)$ ]] || { printf '%s: --gauge is simple or free\n' "$0" >&2; exit 2; }
            gauge="$2"
            shift 2
            ;;
        --stop-after)
            [[ $# -ge 2 && "$2" =~ ^[1-9][0-9]*$ ]] || { printf '%s: --stop-after needs a positive integer\n' "$0" >&2; exit 2; }
            stop_after="$2"
            shift 2
            ;;
        --readout-distribution)
            [[ $# -ge 2 ]] || usage
            readout_table="$2"
            shift 2
            ;;
        --order)
            [[ $# -ge 2 ]] || usage
            order="$2"
            shift 2
            ;;
        -h | --help) sed -n '2,/^set -euo/p' "$0" | sed '$d'; exit 0 ;;
        -*) printf 'unknown option %s\n' "$1" >&2; exit 2 ;;
        *) break ;;
    esac
done
[[ $# -ge 1 && $# -le 2 ]] || usage
# Before anything else: a shell that has not loaded rustup's environment
# has no cargo.
command -v cargo >/dev/null 2>&1 \
    || { printf 'FAIL: cargo is not on PATH; load rustup'"'"'s environment first: . "$HOME/.cargo/env"\n' >&2; exit 1; }
[[ -z "$shots$fusion$svd$gauge$stop_after$readout_table$order" || -n "$h2" ]] \
    || { printf '%s: --shots, --fusion, --svd, --gauge, --stop-after, --readout-distribution and --order go with --h2\n' "$0" >&2; exit 2; }
[[ -z "$h2" || "$mode" != quick ]] || { printf '%s: use one of --quick and --h2\n' "$0" >&2; exit 2; }
if [[ -n "$h2" ]]; then
    [[ -f "$h2" ]] || { printf 'FAIL: --h2 %s is not a file\n' "$h2" >&2; exit 1; }
    # Absolute, because the test runs in the worktree.
    h2="$(cd "$(dirname "$h2")" && pwd)/$(basename "$h2")"
    shots="${shots:-1}"
    fusion="${fusion:-pairs}"
    svd="${svd:-gesvd}"
    gauge="${gauge:-simple}"
fi
if [[ -n "$readout_table" ]]; then
    [[ -f "$readout_table" ]] || { printf 'FAIL: --readout-distribution %s is not a file\n' "$readout_table" >&2; exit 1; }
    readout_table="$(cd "$(dirname "$readout_table")" && pwd)/$(basename "$readout_table")"
fi
if [[ -n "$order" ]]; then
    [[ -f "$order" ]] || { printf 'FAIL: --order %s is not a file\n' "$order" >&2; exit 1; }
    order="$(cd "$(dirname "$order")" && pwd)/$(basename "$order")"
    order_name="$(basename "$order")"
    order_name="${order_name%.*}"
fi
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

if [[ -n "$h2" ]]; then
    printf -- '-- H2 bytecode\n'
    h2_shape=""
    if ! have python3; then
        problem "python3 is needed to check $h2"
    elif h2_shape="$(python3 - "$h2" 2>&1 <<'EOF'
import json, sys
program = json.load(open(sys.argv[1]))
missing = [k for k in ("num_qubits", "num_results", "instructions") if k not in program]
if missing:
    sys.exit(f"no {', '.join(missing)}")
print(f"{program['num_qubits']} qubits, {program['num_results']} results, "
      f"{len(program['instructions'])} instructions")
EOF
    )"; then
        ok "$h2: $h2_shape; $shots shot(s), fusion $fusion, svd $svd, gauge $gauge${order:+, order $order}${stop_after:+, stopping after $stop_after measurements}${readout_table:+, readout distribution against $readout_table}"
    else
        problem "$h2 is not an AdaptiveProgram.as_dict() JSON: $(tail -1 <<<"$h2_shape")"
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
out="$HOME/simulator-qualification-${h2:+h2-$fusion-$svd-$gauge-}${order:+order-$order_name-}${readout_table:+readout-}${stop_after:+m$stop_after-}$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out"
{
    printf 'commit=%s\nmode=%s\nhost=%s\ndate=%s\n' "$sha" "$mode" "$(uname -a)" "$(date -Is)"
    [[ -z "$h2" ]] || printf 'h2=%s\nh2_sha256=%s\nQDK_H2_SHOTS=%s\nQDK_H2_SEED=%s\nQDK_H2_CHI=%s\nQDK_H2_FUSION=%s\nQDK_H2_SVD=%s\nQDK_H2_GAUGE=%s\nQDK_H2_STOP_AFTER=%s\n' \
        "$h2" "$(sha256sum "$h2" | awk '{ print $1 }')" "$shots" "${QDK_H2_SEED:-0}" "${QDK_H2_CHI:-32}" \
        "$fusion" "$svd" "$gauge" "$stop_after"
    [[ -z "$readout_table" ]] || printf 'QDK_H2_READOUT=distribution\nQDK_H2_READOUT_TABLE=%s\nreadout_table_sha256=%s\n' \
        "$readout_table" "$(sha256sum "$readout_table" | awk '{ print $1 }')"
    [[ -z "$order" ]] || printf 'QDK_H2_ORDER=%s\norder_sha256=%s\n' \
        "$order" "$(sha256sum "$order" | awk '{ print $1 }')"
    printf 'QDK_CUTENSORNET_LIBRARY=%s\nQDK_CUDART_LIBRARY=%s\n' "$QDK_CUTENSORNET_LIBRARY" "$QDK_CUDART_LIBRARY"
    printf '%s\n' "$probe"
    (cd "$work" && rustc --version && cargo --version)
    nvidia-smi
} >"$out/environment.txt" 2>&1 || true

step "3. build and run"
filters=("$FILTER" --skip "$H2_TEST")
if [[ -n "$h2" ]]; then
    filters=("$FILTER$H2_TEST" --exact)
    export QDK_H2_BYTECODE="$h2" QDK_H2_SHOTS="$shots" QDK_H2_FUSION="$fusion" QDK_H2_SVD="$svd" QDK_H2_GAUGE="$gauge"
    if [[ -n "$stop_after" ]]; then export QDK_H2_STOP_AFTER="$stop_after"; else unset QDK_H2_STOP_AFTER; fi
    if [[ -n "$order" ]]; then export QDK_H2_ORDER="$order"; else unset QDK_H2_ORDER; fi
    if [[ -n "$readout_table" ]]; then
        export QDK_H2_READOUT=distribution QDK_H2_READOUT_TABLE="$readout_table" QDK_H2_TRACE="$out/h2-trace.txt"
    else
        unset QDK_H2_READOUT QDK_H2_READOUT_TABLE QDK_H2_TRACE
    fi
elif [[ "$mode" == quick ]]; then
    filters=("${QUICK_TESTS[@]/#/$FILTER}")
fi
command=(cargo test -p "$PACKAGE" --release -- --ignored --nocapture --test-threads=1 "${filters[@]}")
printf '%s\n' "(cd $work && ${h2:+QDK_H2_BYTECODE=$h2 QDK_H2_SHOTS=$shots QDK_H2_FUSION=$fusion QDK_H2_SVD=$svd QDK_H2_GAUGE=$gauge }${order:+QDK_H2_ORDER=$order }${stop_after:+QDK_H2_STOP_AFTER=$stop_after }${readout_table:+QDK_H2_READOUT=distribution QDK_H2_READOUT_TABLE=$readout_table QDK_H2_TRACE=$out/h2-trace.txt }${command[*]})" \
    | tee "$out/command.txt"
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
    grep -E '^phase estimation' "$out/run.log" || true
    # H2's results; its progress lines only when the shots stop early, where
    # they are the measurement.
    if [[ -n "$stop_after" ]]; then
        grep -E '^h2 ' "$out/run.log" || true
    else
        grep -E '^h2 ' "$out/run.log" | grep -vE '^h2 shot [0-9]+: [0-9]+ measurements, [0-9.]+ s \(' || true
    fi
    grep -E '^test result:' "$out/run.log" || true
} | tee "$out/summary.txt"
[[ "$status" -eq 0 ]] || fail "cargo test exited with $status (see $out/run.log)"
printf '\nAll the qualification tests passed.\n'
