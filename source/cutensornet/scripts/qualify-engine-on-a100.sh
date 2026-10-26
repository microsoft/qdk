#!/usr/bin/env bash
# The A100 qualification of qdk_cutensornet's MPS engine (CuTensorNetEngine):
# the ignored tests of tests/qualification, run on the real libraries and an
# A100, with their rows collected and summarized.
#
# What it runs (tests/qualification/main.rs has the details):
#   conformance, conformance_without_capture, conformance_with_restart
#                 tensornet::conformance::check on the engine (which
#                 restarts every 10 computes on a new cuTensorNet handle),
#                 without MPS capture (and so without restarts), and
#                 restarting its native state from the computed MPS at every
#                 opportunity, on the same cuTensorNet handle and on a new
#                 one at each restart; the only tests that assert
#   environment   the versions and libraries found
#   empty_compute, read_cost, fork_cost, free_after_compute,
#   shape_and_tolerance, impossible_condition, rare_condition,
#   projection_memory
#                 measurements; read_cost runs with capture=on and =off and
#                 extrapolates both to H2's 1,404 reads over 48,000 gates;
#                 projection_memory records the device memory along H2's
#                 pattern, step after step, with variants of the measurement,
#                 and the retained weight <psi|psi> the truncations left
#   history_growth
#                 H2's pattern for 300 steps on seven engine variants
#                 (no_restart, the baseline; default, the engine, which
#                 restarts every 10 computes on a new cuTensorNet handle; the
#                 baseline with the minimum workspace; restart every 10 and
#                 every 50 computes on the same handle; the baseline with two
#                 computes per step, and with probes): the workspace sizes,
#                 device memory, allocations, computes per handle and the time
#                 per kind of call, every 10 steps, and their slopes; the
#                 probes, what a new state started at the computed MPS asks
#                 for on the engine's handle and on a new one
#
# Usage, on the A100 VM:
#   qualify-engine-on-a100.sh [--check | --quick] [--only TESTS] COMMIT [REPO]
#     COMMIT   the commit to qualify, on origin's BRANCH: the script fetches
#              BRANCH from origin first (the remote-tracking ref only)
#     REPO     a qdk checkout (default: the current directory)
#     --check  only the checks (host, tools, commit, GPU, libraries, disk);
#              fetches, builds and changes nothing: a commit not yet
#              fetched is checked against origin's BRANCH with ls-remote
#     --quick  smaller sweeps (QDK_ENGINE_QUALIFICATION_QUICK=1), to check the
#              setup end to end
#     --only TESTS
#              only these tests, comma-separated names from the list above
#              (for example --only projection_memory); the summary then has
#              their sections only
# Run --check first, then --quick, then with no option (each with the same
# --only, if any).
#
# It leaves REPO's checkout untouched: it adds a detached worktree at COMMIT
# next to it (../qdk-engine-a100) and builds and runs there. Results go to
# ~/engine-a100-<date>/ and a .tar.gz of it, also when a step fails: send the
# .tar.gz back (or at least summary.txt and rows.csv).
#
# Requirements, all checked first: Linux x86-64; an A100; cuTensorNet 2.13.0
# (21300) built for CUDA 12.9; CUDA runtime 12.9 (12090); a driver for CUDA
# 12.9 or newer; rustc 1.88 or newer (CI uses 1.95); python3 3.8 or newer;
# 10 GB free next to REPO.
#
# Optional environment:
#   QDK_ENGINE_BRANCH       the branch to fetch (default
#                           domingom/tensor-network-engine)
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY   library paths, as the crate
#                                                 reads them
#   QDK_ALLOW_OTHER_GPU=1   run on a GPU that is not an A100 (numbers not
#                           comparable)
# Keep the GPU otherwise idle while it runs.

set -euo pipefail

readonly CUTENSORNET_VERSION=21300 # source/cutensornet/src/version.rs
readonly CUDART_VERSION=12090
readonly MIN_DRIVER_CUDA=12090
readonly MIN_RUSTC=1.88
readonly CI_RUSTC=1.95
readonly MIN_FREE_GB=10
readonly PACKAGE=qdk_cutensornet
readonly FEATURE=qualification
readonly TEST=qualification
readonly CRATE=source/cutensornet
readonly BRANCH="${QDK_ENGINE_BRANCH:-domingom/tensor-network-engine}"
readonly CUTENSORNET_PINNED=/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2
readonly CUDART_PINNED=/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12
readonly TESTS="environment conformance conformance_without_capture conformance_with_restart empty_compute \
read_cost fork_cost free_after_compute shape_and_tolerance impossible_condition rare_condition \
projection_memory history_growth"

usage() { printf 'usage: %s [--check | --quick] [--only TESTS] COMMIT [REPO]\n' "$0" >&2; exit 2; }

mode=run
only=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --check | --quick)
            [[ "$mode" == run ]] || { printf '%s: use one of --check and --quick\n' "$0" >&2; exit 2; }
            mode="${1#--}"
            shift
            ;;
        --only)
            [[ $# -ge 2 && -n "$2" && -z "$only" ]] || { printf '%s: --only takes one list of tests\n' "$0" >&2; exit 2; }
            only="$2"
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
# The tests to run: every one, or --only's, in its order.
selected="$TESTS"
if [[ -n "$only" ]]; then
    selected=""
    IFS=, read -ra names <<<"$only"
    for name in "${names[@]}"; do
        [[ " $TESTS " == *" $name "* ]] || { printf '%s: --only: no test %s; the tests are: %s\n' "$0" "$name" "$TESTS" >&2; exit 2; }
        [[ " $selected " == *" $name "* ]] || selected="${selected:+$selected }$name"
    done
fi
readonly selected
ref="$1"
[[ -d "${2:-.}" ]] || { printf 'FAIL: %s is not a directory\n' "${2:-.}" >&2; exit 1; }
repo="$(cd "${2:-.}" && pwd)"
work="$(dirname "$repo")/qdk-engine-a100"
out=""

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
    problem "not x86-64 Linux with an A100: this is $(uname -s) $(uname -m), where the engine is compiled out"
fi
for tool in git cargo rustc python3 nvidia-smi ldconfig ldd tar df awk sort; do
    have "$tool" || problem "$tool not found"
done
if have python3; then
    if python3 -c 'import sys; sys.exit(sys.version_info < (3, 8))'; then
        ok "$(python3 --version 2>&1)"
    else
        problem "python3 3.8 or newer needed ($(python3 --version 2>&1))"
    fi
fi
rustc_version="" cargo_version=""
if have rustc && have cargo; then
    # In REPO, so that a rustup override or toolchain file there applies.
    rustc_version="$(cd "$repo" && rustc --version | awk '{ print $2 }')"
    cargo_version="$(cd "$repo" && cargo --version | awk '{ print $2 }')"
    if ! version_ge "${rustc_version%%-*}" "$MIN_RUSTC"; then
        problem "rustc $MIN_RUSTC or newer needed (found $rustc_version)"
    elif ! version_ge "${rustc_version%%-*}" "$CI_RUSTC"; then
        warn "rustc $rustc_version is older than CI's $CI_RUSTC; if the build fails, try $CI_RUSTC"
    else
        ok "rustc $rustc_version, cargo $cargo_version"
    fi
fi

printf -- '-- checkout and commit\n'
sha=""
is_repo=0
have git && git -C "$repo" rev-parse --git-dir >/dev/null 2>&1 && is_repo=1
if [[ "$is_repo" == 1 && "$mode" != check ]] && ! git -C "$repo" fetch --quiet origin "$BRANCH"; then
    warn "git -C $repo fetch origin $BRANCH failed; looking for $ref among the commits already there"
fi
if [[ "$is_repo" == 0 ]]; then
    problem "$repo is not a git checkout"
elif ! sha="$(git -C "$repo" rev-parse --verify --quiet "$ref^{commit}")"; then
    sha=""
    if [[ "$mode" == check ]] && tip="$(git -C "$repo" ls-remote origin "refs/heads/$BRANCH" | awk '{ print $1 }')" \
        && [[ -n "$tip" && "$tip" == "$ref"* ]]; then
        ok "commit $ref is origin's $BRANCH; the run will fetch it (its contents are checked then)"
    else
        problem "commit $ref not in $repo, nor origin's $BRANCH tip (is $BRANCH pushed? QDK_ENGINE_BRANCH?)"
    fi
else
    ok "commit $sha"
    for path in Cargo.toml Cargo.lock "$CRATE/Cargo.toml" "$CRATE/src/version.rs" \
        "$CRATE/tests/qualification/main.rs"; do
        git -C "$repo" cat-file -e "$sha:$path" 2>/dev/null || problem "the commit has no $path"
    done
    manifest="$(git -C "$repo" show "$sha:$CRATE/Cargo.toml" 2>/dev/null || true)"
    grep -q "^name = \"$PACKAGE\"" <<<"$manifest" || problem "$CRATE is not the package $PACKAGE"
    grep -q "^$FEATURE = " <<<"$manifest" || problem "$PACKAGE has no feature $FEATURE at the commit"
    policy="$(git -C "$repo" show "$sha:$CRATE/src/version.rs" 2>/dev/null || true)"
    grep -q "cutensornet_runtime: ${CUTENSORNET_VERSION:0:2}_${CUTENSORNET_VERSION:2}," <<<"$policy" \
        || problem "the commit's version.rs does not pin cuTensorNet $CUTENSORNET_VERSION"
    grep -q "cuda_runtime: ${CUDART_VERSION:0:2}_${CUDART_VERSION:2}," <<<"$policy" \
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

printf -- '-- disk\n'
free_kb="$(df -Pk "$(dirname "$repo")" | awk 'NR == 2 { print $4 }')"
if [[ "$free_kb" =~ ^[0-9]+$ && "$free_kb" -ge $((MIN_FREE_GB * 1024 * 1024)) ]]; then
    ok "$((free_kb / 1024 / 1024)) GB free next to $repo"
else
    problem "${MIN_FREE_GB} GB free needed next to $repo (found $((${free_kb:-0} / 1024 / 1024)) GB)"
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
    gpu_line="$(nvidia-smi -i "$index" --query-gpu=name,memory.used,utilization.gpu,mig.mode.current \
        --format=csv,noheader,nounits 2>/dev/null || true)"
    gpu_name="$(cut -d, -f1 <<<"$gpu_line" | xargs)"
    gpu_used="$(cut -d, -f2 <<<"$gpu_line" | xargs)"
    gpu_util="$(cut -d, -f3 <<<"$gpu_line" | xargs)"
    gpu_mig="$(cut -d, -f4 <<<"$gpu_line" | xargs)"
    if [[ -z "$gpu_line" ]]; then
        problem "nvidia-smi cannot query GPU $index"
    elif [[ "$gpu_name" == *A100* ]]; then
        ok "GPU $index: $gpu_name"
    elif [[ "${QDK_ALLOW_OTHER_GPU:-0}" == 1 ]]; then
        warn "GPU $index is $gpu_name, not an A100: the numbers are not comparable"
    else
        problem "not x86-64 Linux with an A100: GPU $index is $gpu_name (QDK_ALLOW_OTHER_GPU=1 to run anyway)"
    fi
    [[ "$gpu_mig" == Enabled ]] && warn "MIG is enabled: the run gets a slice of the GPU"
    busy="$(nvidia-smi -i "$index" --query-compute-apps=pid,process_name --format=csv,noheader 2>/dev/null || true)"
    [[ -z "$busy" ]] || warn "other processes use the GPU, timings will be noisy: $(tr '\n' ';' <<<"$busy")"
    [[ "$gpu_used" =~ ^[0-9]+$ && "$gpu_used" -gt 1024 ]] && warn "$gpu_used MiB of GPU memory already in use"
    [[ "$gpu_util" =~ ^[0-9]+$ && "$gpu_util" -gt 10 ]] && warn "the GPU is $gpu_util% busy"
else
    problem "not x86-64 Linux with an A100: nvidia-smi cannot reach a GPU"
fi

printf -- '-- libraries\n'
# As the crate's discovery: the variable if set, else the pinned path, else ldconfig's answer.
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
locate_library "$CUTENSORNET_PINNED" libcutensornet.so.2 QDK_CUTENSORNET_LIBRARY
locate_library "$CUDART_PINNED" libcudart.so.12 QDK_CUDART_LIBRARY
probe=""
if [[ -n "${QDK_CUTENSORNET_LIBRARY:-}" && -n "${QDK_CUDART_LIBRARY:-}" ]] && have python3; then
    # The versions the crate checks at run time, read before building.
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
        [[ "$(value cutensornet)" == "$CUTENSORNET_VERSION" ]] && ok "cuTensorNet $(value cutensornet)" \
            || problem "cuTensorNet is $(value cutensornet); the crate accepts $CUTENSORNET_VERSION only"
        [[ "$(value cudart)" == "$CUDART_VERSION" ]] && ok "CUDA runtime $(value cudart)" \
            || problem "the CUDA runtime is $(value cudart); the crate accepts $CUDART_VERSION only"
        [[ "$(value cutensornet_cudart)" == "$CUDART_VERSION" ]] \
            && ok "cuTensorNet built for CUDA $(value cutensornet_cudart)" \
            || problem "cuTensorNet was built for CUDA $(value cutensornet_cudart); the crate accepts $CUDART_VERSION only"
        driver="$(value driver)"
        if [[ "$driver" =~ ^[0-9]+$ && "$driver" -ge "$MIN_DRIVER_CUDA" ]]; then
            ok "the driver supports CUDA $driver"
        else
            problem "the driver supports CUDA $driver; $MIN_DRIVER_CUDA or newer needed"
        fi
        devices="$(value devices)"
        [[ "$devices" =~ ^[1-9][0-9]*$ ]] && ok "the CUDA runtime sees $devices GPU(s)" \
            || problem "the CUDA runtime sees no GPU ($devices)"
    else
        problem "loading the libraries failed: $(tail -1 <<<"$probe")"
    fi
fi

if [[ "${#problems[@]}" -gt 0 ]]; then
    printf '\n%d problem(s):\n' "${#problems[@]}" >&2
    printf '  - %s\n' "${problems[@]}" >&2
    fail "fix the above, then run again (--check checks only)"
fi
if [[ "$mode" == check ]]; then
    printf '\nAll checks passed. Next: %s --quick%s %s %s\n' "$0" "${only:+ --only $only}" "$ref" "$repo"
    exit 0
fi

out="$HOME/engine-a100-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out"
nvidia-smi >"$out/nvidia-smi.txt" 2>&1 || true
nvidia-smi --query-gpu=index,name,driver_version,memory.total,memory.used,clocks.sm,clocks.max.sm,temperature.gpu,persistence_mode \
    --format=csv >"$out/gpu.txt" 2>&1 || true
if [[ "$mode" == quick ]]; then
    export QDK_ENGINE_QUALIFICATION_QUICK=1
else
    unset QDK_ENGINE_QUALIFICATION_QUICK
fi
{
    printf 'mode=%s\ntests=%s\ndate=%s\nhost=%s\nrepo=%s\ncommit=%s\nworktree=%s\n' \
        "$mode" "$selected" "$(date -Is)" "$(hostname)" "$repo" "$sha" "$work"
    printf 'script_sha256=%s\n' "$(sha256sum "$0" | awk '{ print $1 }')"
    printf 'rustc=%s\ncargo=%s\npython=%s\n' "$rustc_version" "$cargo_version" "$(python3 --version 2>&1)"
    printf 'QDK_CUTENSORNET_LIBRARY=%s -> %s\n' "$QDK_CUTENSORNET_LIBRARY" "$(readlink -f "$QDK_CUTENSORNET_LIBRARY")"
    printf 'QDK_CUDART_LIBRARY=%s -> %s\n' "$QDK_CUDART_LIBRARY" "$(readlink -f "$QDK_CUDART_LIBRARY")"
    printf 'QDK_ENGINE_QUALIFICATION_QUICK=%s\n' "${QDK_ENGINE_QUALIFICATION_QUICK:-}"
    printf 'CUDA_VISIBLE_DEVICES=%s\n' "${CUDA_VISIBLE_DEVICES:-}"
    printf '%s\n' "$probe"
} >"$out/manifest.txt"

step "2. worktree $work at $sha"
[[ -d "$work" ]] || git -C "$repo" worktree add --detach "$work" "$sha" >"$out/worktree.txt" 2>&1 \
    || fail "git worktree add failed; see worktree.txt"
[[ -z "$(git -C "$work" status --porcelain)" ]] || fail "the worktree $work has changes"
ok "the worktree is $sha, unchanged"

step "3. dependencies"
if (cd "$work" && cargo metadata --locked --offline --format-version 1 >/dev/null 2>&1); then
    ok "every crate in Cargo.lock is cached"
else
    echo "  fetching crates (Cargo.lock, --locked)"
    (cd "$work" && cargo fetch --locked) >"$out/fetch.txt" 2>&1 \
        || { tail -20 "$out/fetch.txt"; fail "cargo fetch --locked failed; see fetch.txt"; }
fi

cargo_test=(cargo test --locked --release -p "$PACKAGE" --features "$FEATURE" --test "$TEST")
step "4. build (release)"
if ! (cd "$work" && "${cargo_test[@]}" --no-run) >"$out/build.txt" 2>&1; then
    tail -30 "$out/build.txt"
    fail "the build failed; see build.txt"
fi
tail -3 "$out/build.txt"

step "5. run"
harness=(--ignored --nocapture --test-threads=1)
if [[ -n "$only" ]]; then
    read -ra names <<<"$selected"
    harness+=(--exact "${names[@]}")
fi
printf '  %s -- %s\n' "${cargo_test[*]}" "${harness[*]}" | tee "$out/command.txt"
set +e
(cd "$work" && "${cargo_test[@]}" -- "${harness[@]}") 2>&1 | tee "$out/log.txt"
status="${PIPESTATUS[0]}"
set -e
# Every row starts a line (report.rs); the "test <name> ... " prefix has its own.
pattern="$(tr ' ' '|' <<<"$TESTS")"
grep -E "^($pattern),|^# " "$out/log.txt" >"$out/rows.csv" || true

step "6. summary"
summarized=1
python3 - "$out/log.txt" "$out/rows.csv" "$CUTENSORNET_VERSION" "$CUDART_VERSION" $selected <<'EOF' \
    | tee "$out/summary.txt" || summarized=0
import csv, re, sys
from collections import defaultdict
log, rows_path, tn_version, rt_version, *tests = sys.argv[1:]
text = open(log, errors="replace").read()
rows = [r for r in csv.reader(l for l in open(rows_path) if not l.startswith("#")) if len(r) >= 3]
notes = [l.rstrip("\n") for l in open(rows_path) if l.startswith("# ") and not l.rstrip().endswith("case,item,value,unit")]
bad = []

print("=== tests")
for test in tests:
    # With --nocapture the test's rows come between "test <name> ... " and
    # its result, which then has a line of its own.
    m = re.search(rf"^test {test} \.\.\. (.*?)(?=^test |^failures:|\Z)", text, re.M | re.S)
    verdicts = [l.strip() for l in m.group(1).splitlines() if l.strip() in ("ok", "FAILED", "ignored")] if m else []
    verdict = verdicts[-1] if verdicts else ("unknown" if m else "missing")
    if verdict != "ok":
        bad.append(f"test {test}: {verdict}")
    print(f"  {test:32} {verdict}")

conformance = [case for case in ("conformance", "conformance_without_capture", "conformance_with_restart")
               if case in tests]
if conformance:
    print("\n=== conformance (passed / failed per case)")
# conformance_with_restart runs a second engine, which moves the state to a
# new cuTensorNet handle at each restart; its items are prefixed handle=fresh;.
engines = [(case, prefix, label) for case in conformance
           for prefix, label in ([("", "restart=1  "), ("handle=fresh;", "restart=1 fresh")]
                                 if case == "conformance_with_restart" else
                                 [("", {"conformance": "capture=on ", "conformance_without_capture": "capture=off"}[case])])]
for case, prefix, label in engines:
    head = prefix + "case="
    cells = [(r[1][len(head):-len(";passed")], r[2]) for r in rows
             if r[0] == case and r[1].startswith(head) and r[1].endswith(";passed")]
    if not cells:
        bad.append(f"{case}: no {prefix}case rows")
        print(f"  {label}  no rows")
        continue
    # Without capture, every compute truncates the whole history again, so
    # the retained weight is reported but not held (main.rs).
    def verdict(name, value):
        if value == "true":
            return "passed"
        return "not-held" if case == "conformance_without_capture" and name == "retained weight" else "FAILED"
    print(f"  {label}  " + "  ".join(f"{name}={verdict(name, value)}" for name, value in cells))
    for note in notes:
        if note.startswith(f"# {case} {head}"):
            print(f"    {note[2:]}")

if "environment" in tests:
    print("\n=== environment")
    env = {r[1]: r[2] for r in rows if r[0] == "environment"}
    for key, value in env.items():
        print(f"  {key} = {value}")
    if env.get("cutensornet_version") != tn_version:
        bad.append(f"cuTensorNet at run time: {env.get('cutensornet_version')}, expected {tn_version}")
    if env.get("cuda_runtime_version") != rt_version:
        bad.append(f"CUDA runtime at run time: {env.get('cuda_runtime_version')}, expected {rt_version}")

if "read_cost" in tests:
    print("\n=== read_cost: median read after H2's gates per read, in ms, as operators accumulate")
    reads = defaultdict(dict)
    p_one = defaultdict(dict)
    projected = defaultdict(dict)
    mib = defaultdict(dict)
    extrapolated = []
    for r in rows:
        if r[0] != "read_cost":
            continue
        fields = dict(f.split("=", 1) for f in r[1].split(";") if "=" in f)
        metric = r[1].rsplit(";", 1)[-1]
        if "extrapolation" in fields:
            extrapolated.append((fields.get("chi"), fields.get("capture"), metric, r[2], r[3] if len(r) > 3 else ""))
        elif metric.startswith("read_after_") and metric.endswith("_operators"):
            reads[(int(fields["chi"]), int(fields["operators"]))][fields["capture"]] = r[2]
        elif metric == "p_one_site_0" and "operators" in fields:
            p_one[(int(fields["chi"]), int(fields["operators"]))][fields["capture"]] = r[2]
        elif metric == "projected_one" and "operators" in fields:
            projected[(int(fields["chi"]), int(fields["operators"]))][fields["capture"]] = r[2]
        elif metric == "device_mib" and "operators" in fields:
            mib[(int(fields["chi"]), int(fields["operators"]))][fields["capture"]] = r[2]
    print(f"  {'chi':>4} {'operators':>9} {'capture=on':>11} {'capture=off':>12}")
    for (chi, operators), by in sorted(reads.items()):
        print(f"  {chi:>4} {operators:>9} {by.get('on', '-'):>11} {by.get('off', '-'):>12}")
    print("\n  P(site 0 = 1) with and without capture, which truncate in different steps, while")
    print("  both projected site 0 on the same values (after that, 'diverged'), and the device")
    print("  memory in use, in MiB, at each checkpoint")
    print(f"  {'chi':>4} {'operators':>9} {'|p_on - p_off|':>15} {'MiB on':>8} {'MiB off':>8}")
    diverged = set()
    for key in sorted(set(p_one) | set(mib)):
        try:
            difference = f"{abs(float(p_one[key]['on']) - float(p_one[key]['off'])):.3e}"
        except (KeyError, ValueError):
            difference = "-"
        if key[0] in diverged:
            difference = "diverged"
        if projected[key].get("on") != projected[key].get("off"):
            diverged.add(key[0])
        print(f"  {key[0]:>4} {key[1]:>9} {difference:>15} {mib[key].get('on', '-'):>8} {mib[key].get('off', '-'):>8}")
    print("\n  EXTRAPOLATION to H2 (1,404 reads, 48,000 gates) from the fit above, not a measurement:")
    for chi, capture, metric, value, unit in extrapolated:
        print(f"  chi={chi} capture={capture:3} {metric:14} {value} {unit}")

if "fork_cost" in tests:
    print("\n=== fork_cost: a parent read every so often (captured with capture=on) against its fork")
    print(f"  {'case':22} {'|p - fork|':>11} {'|stepped - once|':>17} {'bond':>5} {'fork bond':>9}")
    forks = defaultdict(dict)
    for r in rows:
        if r[0] == "fork_cost" and ";stepped_" in r[1]:
            case, metric = r[1].rsplit(";", 1)
            forks[case][metric] = r[2]
    for case, by in forks.items():
        print(f"  {case:22} {by.get('stepped_fork_difference', '-'):>11} "
              f"{by.get('stepped_difference_from_plain', '-'):>17} {by.get('stepped_max_bond', '-'):>5} "
              f"{by.get('stepped_fork_max_bond', '-'):>9}")

if "projection_memory" in tests:
    print("\n=== projection_memory: the device memory along H2's pattern (gates, a read, the ending), in MiB")
    runs = {}
    for r in rows:
        if r[0] != "projection_memory":
            continue
        fields = dict(f.split("=", 1) for f in r[1].split(";") if "=" in f)
        metric = r[1].rsplit(";", 1)[-1]
        if not all(k in fields for k in ("variant", "width", "chi")):
            continue
        run = runs.setdefault((fields["variant"], fields["width"], fields["chi"]),
                              {"after": {}, "before": {}, "weight": {}})
        if "extrapolation" in fields:
            run["h2_" + metric] = r[2]
        elif "step" in fields and metric == "device_mib":
            run["after"][int(fields["step"])] = r[2]
        elif "step" in fields and metric == "device_mib_before_ending":
            run["before"][int(fields["step"])] = r[2]
        elif "step" in fields and metric == "retained_weight":
            run["weight"][int(fields["step"])] = r[2]
        elif "step" not in fields:
            run[metric] = r[2]
    def at_ending(run):
        # The mean growth across the ending alone (its registration), at the marks.
        deltas = [float(run["after"][s]) - float(run["before"][s]) for s in run["before"]
                  if s in run["after"] and run["after"][s].isdigit() and run["before"][s].isdigit()]
        return f"{sum(deltas) / len(deltas):.1f}" if deltas else "-"
    print(f"  {'variant':24} {'width':>5} {'chi':>3} {'before':>7} {'step 0':>7} {'last':>7} {'MiB/step':>8} "
          f"{'ending':>6} {'closed':>7} {'ms/step':>8} {'run':>5}")
    for (variant, width, chi), run in runs.items():
        last = run["after"][max(run["after"])] if run["after"] else "-"
        print(f"  {variant:24} {width:>5} {chi:>3} {run.get('device_mib_before_prepare', '-'):>7} "
              f"{run['after'].get(0, '-'):>7} {last:>7} {run.get('slope', '-'):>8} {at_ending(run):>6} "
              f"{run.get('device_mib_after_close', '-'):>7} {run.get('ms_per_step', '-'):>8} {run.get('run', 'ok'):>5}")
    print("\n  MiB at each mark (step:MiB), after the step's ending")
    for (variant, width, chi), run in runs.items():
        marks = " ".join(f"{s}:{v}" for s, v in sorted(run["after"].items()))
        print(f"  {variant} width={width} chi={chi}: {marks}")
    print("\n  retained weight <psi|psi> at each mark (step:weight), after the step's reads, and the")
    print("  least-squares slope of its log10 per step (1 = nothing truncated; it never increases)")
    for (variant, width, chi), run in runs.items():
        marks = " ".join(f"{s}:{v}" for s, v in sorted(run["weight"].items()))
        print(f"  {variant} width={width} chi={chi}: log10/step={run.get('log10_retained_weight_per_step', '-')} "
              f"{marks}")
    print("\n  EXTRAPOLATION to H2's 1,404 measurements from the slopes and the mean step, an estimate,")
    print("  not a measurement (log10 w: log10 of the weight left after 1,404 steps at the measured rate,")
    print("  for this random brickwork of H2's size: likely pessimistic, not H2's own circuit):")
    for (variant, width, chi), run in runs.items():
        print(f"  {variant:24} width={width:>2} chi={chi:>2} {run.get('h2_mib_per_shot', '-'):>8} MiB/shot "
              f"{run.get('h2_seconds_per_shot', '-'):>8} s/shot "
              f"{run.get('h2_log10_retained_weight', '-'):>8} log10 w")

if "history_growth" in tests:
    print("\n=== history_growth: H2's pattern on engine variants, every 10 steps (per-step means since the last mark)")
    hist = {}
    for r in rows:
        if r[0] != "history_growth":
            continue
        fields = dict(f.split("=", 1) for f in r[1].split(";") if "=" in f)
        metric = r[1].rsplit(";", 1)[-1]
        if "variant" not in fields:
            continue
        run = hist.setdefault(fields["variant"], {"marks": defaultdict(dict), "slopes": {}, "other": {},
                                                  "probes": defaultdict(dict)})
        if r[1].split(";")[1:2] == ["slope"]:
            run["slopes"][metric] = r[2]
        elif "extrapolation" in fields:
            run["other"]["h2_" + metric] = r[2]
        elif "probe" in fields:
            run["probes"][int(fields["step"])][(fields["probe"], fields["operators"], metric)] = r[2]
        elif "step" in fields:
            run["marks"][int(fields["step"])][metric] = r[2]
        else:
            run["other"][metric] = r[2]
    def mib(value):
        try:
            return f"{int(value) / 1048576:.0f}"
        except ValueError:
            return value
    kinds = ["apply", "prepare", "workspace_query", "allocate", "compute", "capture", "free", "reads",
             "restart", "unprofiled"]
    short = ["apply", "prep", "wsq", "alloc", "comp", "capt", "free", "reads", "rstrt", "other"]
    for variant, run in hist.items():
        print(f"\n  variant={variant}  (ms per step by kind of call; workspace in MiB)")
        print(f"  {'step':>5} {'ms':>8} " + " ".join(f"{s:>7}" for s in short)
              + f" {'min_ws':>7} {'rec_ws':>7} {'dev_MiB':>7} {'live':>6} {'ops':>5} {'rst':>4} {'fh':>4}"
              + f" {'comp':>5} {'hcomp':>6}")
        for step, m in sorted(run["marks"].items()):
            if step == 0:
                continue
            print(f"  {step:>5} {m.get('ms_per_step', '-'):>8} "
                  + " ".join(f"{m.get(k + '_ms', '-'):>7}" for k in kinds)
                  + f" {mib(m.get('minimum_workspace_bytes', '-')):>7} {mib(m.get('recommended_workspace_bytes', '-')):>7}"
                  + f" {m.get('device_mib', '-'):>7} {m.get('live_allocations', '-'):>6}"
                  + f" {m.get('operator_tensors', '-'):>5} {m.get('restarts', '-'):>4}"
                  + f" {m.get('fresh_handles', '-'):>4} {m.get('computes', '-'):>5} {m.get('handle_computes', '-'):>6}")
        if run["probes"]:
            # A new state started at the last computed MPS, prepared once on
            # the engine's handle or on a new one, with no operator or with
            # the step's gates; the state's own sizes beside them.
            probes = [(h, o) for h in ("engine", "fresh") for o in ("none", "batch")]
            print(f"\n  variant={variant} probes: recommended (minimum) workspace in MiB of a new state at"
                  " the computed MPS, and the computes its handle had done")
            print(f"  {'step':>5} {'state':>13} " + " ".join(f"{h + '/' + o:>21}" for h, o in probes))
            for step, p in sorted(run["probes"].items()):
                m = run["marks"].get(step, {})
                cell = lambda h, o: (f"{mib(p.get((h, o, 'recommended_workspace_bytes'), '-'))}"
                                     f" ({mib(p.get((h, o, 'minimum_workspace_bytes'), '-'))})"
                                     f" h{p.get((h, o, 'handle_computes'), '-')}"
                                     if (h, o, "workspace") not in p else "error")
                print(f"  {step:>5} {mib(m.get('recommended_workspace_bytes', '-')):>6}"
                      f" ({mib(m.get('minimum_workspace_bytes', '-')):>4}) "
                      + " ".join(f"{cell(h, o):>21}" for h, o in probes))
    print("\n  slopes per step (least squares over the marks) and peaks; MiB/comp is the recommended")
    print("  workspace's slope over the computes per step (from the last mark)")
    print(f"  {'variant':24} {'dev MiB':>8} {'rec MiB':>8} {'min MiB':>8} {'ms':>8} {'comp ms':>8} "
          f"{'prep ms':>8} {'other ms':>8} {'peak rec':>9} {'ms/step':>8} {'closed':>7} {'run':>5} {'MiB/comp':>9}")
    def per_compute(run):
        try:
            step = max(s for s, m in run["marks"].items() if "computes" in m)
            per_step = int(run["marks"][step]["computes"]) / step
            return f"{float(run['slopes']['recommended_workspace']) / per_step:.4f}"
        except (KeyError, ValueError, ZeroDivisionError):
            return "-"
    for variant, run in hist.items():
        sl, o = run["slopes"], run["other"]
        print(f"  {variant:24} {sl.get('device_mib', '-'):>8} {sl.get('recommended_workspace', '-'):>8} "
              f"{sl.get('minimum_workspace', '-'):>8} {sl.get('ms_per_step', '-'):>8} {sl.get('compute', '-'):>8} "
              f"{sl.get('prepare', '-'):>8} {sl.get('unprofiled', '-'):>8} "
              f"{o.get('peak_recommended_workspace_mib', '-'):>9} {o.get('ms_per_step', '-'):>8} "
              f"{o.get('device_mib_after_close', '-'):>7} {o.get('run', 'ok'):>5} {per_compute(run):>9}")
    for variant, run in hist.items():
        probed = {k: v for k, v in run["slopes"].items() if k.startswith("probe_")}
        if probed:
            print(f"  {variant} probes, recommended workspace MiB/step: "
                  + "  ".join(f"{k[len('probe_'):]}={v}" for k, v in probed.items()))
    base = hist.get("no_restart", {}).get("marks", {})
    print("\n  P(One) at the marks: the largest difference from variant=no_restart (a restart, a second compute or a")
    print("  probe should change none)")
    for variant, run in hist.items():
        if variant == "no_restart":
            continue
        diffs = [abs(float(m["p_one"]) - float(base[s]["p_one"])) for s, m in run["marks"].items()
                 if "p_one" in m and "p_one" in base.get(s, {})]
        print(f"  {variant:24} {max(diffs):.3e} over {len(diffs)} marks" if diffs else f"  {variant:24} -")
    print("\n  EXTRAPOLATION to H2's 1,404 measurements of the fitted lines, an estimate, not a measurement:")
    for variant, run in hist.items():
        o = run["other"]
        print(f"  {variant:24} {o.get('h2_seconds_per_shot', '-'):>8} s/shot "
              f"{o.get('h2_recommended_workspace_mib', '-'):>8} MiB recommended workspace at the end")

print("\n=== measurements (every row but projection_memory's and history_growth's; rows.csv has them as CSV)")
errors = {}
for r in rows:
    if r[1] == "errors":
        errors[r[0]] = r[2]
    elif r[0] not in ("environment", "conformance", "conformance_without_capture", "conformance_with_restart",
                      "read_cost", "projection_memory", "history_growth"):
        print(f"  {r[0]},{r[1]},{r[2]},{r[3] if len(r) > 3 else ''}")
for note in notes:
    if not note.startswith("# conformance"):
        print(f"  {note}")

print("\n=== engine errors per case (each is an error row; the sweep went on)")
for test in tests:
    print(f"  {test:32} {errors.get(test, 'no errors row')}")
    if test not in errors:
        bad.append(f"{test}: no errors row (it did not finish)")

if bad:
    print("\nINCOMPLETE OR FAILED:")
    print("\n".join("  - " + b for b in bad))
    sys.exit(1)
print("\ncomplete: every test passed and every case finished")
EOF
[[ "$status" -eq 0 ]] || fail "cargo test failed (status $status); see summary.txt and log.txt"
[[ "$summarized" == 1 ]] || fail "the results are incomplete; see summary.txt"
printf '\nDone.\n'
