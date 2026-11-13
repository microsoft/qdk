#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# Development only (not for review): CudaMpsEngine on polyene's relevant
# sizes, the whole 32-step QPE prefix at N = 16 and N = 20, after checking
# the engine (its kit, then polyene N = 8 against the exact state and one
# H2 shot) and running the diagnostic of CuTensorNetEngine's three-site
# MPOs (a reproducer for NVIDIA, not a fix). cuda-mps runs with its
# defaults: QR centre moves on, GESVD; GESVDP is checked beside it.
# The diagnostic alone: PARTS="build diagnostic" (Run, below).
#
# Polyene (the PPP model, N carbon sites, 2N + 1 qubits) is a development
# case and a candidate relevant case, pending the chemistry team; H2 is a
# development case.
#
# Each part has its own log and one line per run in summary.txt; a run
# that fails or times out does not stop the next. The session keeps within
# BUDGET_SECONDS (5 h by default): each run's limit is the smaller of its
# own and what is left, and a run with under a minute left is skipped, its
# summary line saying so. The parts in the order they run, with their
# limits and ESTIMATED durations (from gpu-qr.sh's A100 run of 2026-11-13:
# build 44 s cached, kit 199 s, N = 8 302 s, H2 299 s, N = 16 chi 128
# 224 s to step 11 at bond 68, 30.5 s per step there):
#
#   build        release builds of polyene_qpe_prefix, h2_dev and the two
#                GPU test binaries. Limit 1 h; 1-3 min on the VM's warm
#                target dir, 20-30 min on a fresh one.
#   kit          CudaMpsEngine's GPU tests (tests/cuda_mps_on_the_gpu.rs),
#                GESVDP's among them (GESVDP_PERTURBATION lines). Pass:
#                every test passes, no SELF_CHECK passed=false. Limit
#                15 min; about 4 min.
#   diagnostic   CuTensorNetEngine's three-site operators as one MPO
#                (tests/cutensornet_mpo_on_the_gpu.rs) against the CPU
#                engine, thirteen cases (adjacent, far, unordered,
#                two-site-padded, sequence, then the cases that separate
#                what sequence mixes: two-mpos-no-read,
#                two-mpos-read-between, mpo-then-narrow-after-read,
#                mpo-after-read, mpo-after-restart, mpo-then-restart,
#                mpo-across-projection, projection-then-mpo); one summary
#                line per case with the largest probability difference, as
#                given, with the operator transposed, and with every MPO
#                applied again at each later compute (max_diff_reapplied,
#                "-" where that applies none again), which of them the
#                engine reads as (reading=reference, reapplied, transposed
#                or unexplained), and the cuTensorNet version. A failure
#                here is the expected evidence, not a stop. Limit 10 min;
#                1-2 min.
#   correctness  for each combination of COMBOS (qr-on-gesvd, the
#                defaults, and qr-on-gesvdp): polyene N = 8 chi 128 in the
#                identity order against ppp8.exact.reference (pass:
#                verdict=AgreesWithExact) and one H2 shot in the resumer's
#                order (pass: records=01010, draws=1414, tvd <= 1e-9; run
#                only if H2's sha256 is a8a2b39c...). Limit 15 min each;
#                about 5 min each, 20 min in all.
#   relevant     polyene N = 16 and N = 20 at chi 128, interleave-centre,
#                the whole prefix (32 controlled steps), with the defaults.
#                Per run: steps-<run>.txt (p, cumulative and step seconds,
#                largest bond, 1 - W), and a summary line with the mean and
#                last step's seconds against the CPU's, the largest bond,
#                the final 1 - W, what the prefix gives a chemist (the
#                ancilla's P(One), the first round's output; c_32; <H>;
#                N_alpha, N_beta), and the checks against the CPU
#                references (c_p at p = 1, 2, 4, ..., 32 against the
#                reference's, and the harness's check of every reading).
#                Limits 75 min (N = 16) and 105 min (N = 20); ESTIMATED
#                25-45 min and 40-70 min (the N = 16 run reached step 11
#                in 224 s; the later steps, at bonds up to 128, are
#                guessed at 40-80 s each, and N = 20 at 1.6 times N = 16,
#                its share of gates).
#
# Expected in all: about 1.5-2.5 h on the warm target dir; at most the
# budget.
#
# No exact reference exists at N = 16 or 20; what checks them:
#   - N = 16: ppp16.cpu-ic-seq3-chi128.reference, the CPU engine at the
#     same chi and order (1 - W 1.26e-8, bond 128); the harness's verdict
#     (SameTruncation or WithinTruncationErrors) and c_p's largest gap.
#   - N = 20: ppp20.cpu-ic-seq3-chi256.reference, the CPU engine at chi 256,
#     whose bond never passed 184; on the CPU, chi 128 moved c_32 by 1.8e-8
#     from it (chi 64 by 6.7e-6), so chi 128 is enough and the GPU's c_p
#     should agree within 1e-4. The harness prints NotChecked (the chi
#     differs) but still the largest gap of every reading.
#   - both: 1 - W, the discarded weight, after every step.
# The prefix gives no energy estimate: its output is one round's ancilla
# P(One) and the phases c_p; the energy comes from the full program's shots
# (polyene_shots_dev), not run here.
#
# Run from anywhere, on the branch's checkout:
#
#   bash source/cutensornet/examples/gpu-polyene-full.sh
#
# The diagnostic alone (with the build it needs), a few minutes:
#
#   PARTS="build diagnostic" bash source/cutensornet/examples/gpu-polyene-full.sh
#
# Environment (all optional):
#   OUT            the output directory (default $HOME/gpu-polyene-full-<date>)
#   PARTS          the parts to run (default "build kit diagnostic correctness
#                  relevant")
#   COMBOS         correctness's combinations (default "qr-on-gesvd
#                  qr-on-gesvdp")
#   RELEVANT_CASES relevant's N/chi cases (default "16/128 20/128")
#   BUDGET_SECONDS the session's budget (default 18000, 5 h)
#   LIMIT_PERCENT  the runs' own limits in percent (default 100)
#   H2             H2's bytecode JSON (default /data/domingom/h2.bytecode.json)
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY, QDK_CUSOLVER_LIBRARY
#                  the libraries, when neither at the pinned paths below nor
#                  in /data/domingom/tn-venv
#   CARGO_FLAGS    extra flags for cargo build (for instance --offline)

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
OUT=${OUT:-$HOME/gpu-polyene-full-$(date +%Y%m%d-%H%M)}
PARTS=${PARTS:-build kit diagnostic correctness relevant}
COMBOS=${COMBOS:-qr-on-gesvd qr-on-gesvdp}
RELEVANT_CASES=${RELEVANT_CASES:-16/128 20/128}
BUDGET_SECONDS=${BUDGET_SECONDS:-18000}
LIMIT_PERCENT=${LIMIT_PERCENT:-100}
BUILD_BOX=3600
KIT_BOX=900
DIAGNOSTIC_BOX=600
N8_BOX=900
H2_BOX=900
N16_BOX=4500
N20_BOX=6300
H2=${H2:-/data/domingom/h2.bytecode.json}
H2_SHA256=a8a2b39cdb9df504f3142c6d1cff34097438d0da0add5049979878a45189ec5f
TABLE=$REPO/source/simulators/examples/h2-readout-distribution.txt
INPUTS=$REPO/source/cutensornet/examples/polyene_qpe_prefix
TN_VENV=/data/domingom/tn-venv
CARGO_FLAGS=${CARGO_FLAGS:-}
BIN=${CARGO_TARGET_DIR:-$REPO/target}/release/examples
SUMMARY=$OUT/summary.txt
# Kept for the tar at the end.
TAR_MARGIN=300
# c_p's largest gap from the CPU reference for a pass: well above what chi
# changes on the CPU (|dc_32| 1.8e-8 from chi 128 to 256 at N = 20, 4.0e-7
# at N = 16), well below the signal (|c_32| about 0.28 at N = 16).
CP_TOLERANCE=1e-4

# --- The CPU's numbers, one core, for the ratios -----------------------------
# The 32-step prefix in interleave-centre, seq3/mpo, on the CPU engine
# (the polyene-n20 runs of 2026-11-13, one core each): the prefix's
# seconds (the mean step's times 32) and the last step's.
cpu_prefix() {
  case "$1/$2" in
  16/128) echo "2057.6 140.8" ;;
  20/128) echo "2561 236.3" ;;
  *) echo "- -" ;;
  esac
}
# The reference of N $1 at chi $2 in interleave-centre, seq3/mpo.
reference_of() {
  case "$1" in
  16) echo ppp16.cpu-ic-seq3-chi128.reference ;;
  20) echo ppp20.cpu-ic-seq3-chi256.reference ;;
  esac
}
# The limit of N $1's relevant run.
relevant_box() {
  case "$1" in
  16) echo "$N16_BOX" ;;
  *) echo "$N20_BOX" ;;
  esac
}

# --- The libraries -----------------------------------------------------------
# The pinned paths of the acceptance run, then the tn-venv's wheels;
# cuSOLVER sits beside the CUDA Runtime.
CUDA_LIB=/usr/local/cuda-12.9/targets/x86_64-linux/lib
# The first file under the tn-venv named $1, or nothing.
in_tn_venv() {
  [ -d "$TN_VENV" ] && find "$TN_VENV" -name "$1" 2>/dev/null | sort | head -1
}
if [ -z "${QDK_CUTENSORNET_LIBRARY:-}" ]; then
  QDK_CUTENSORNET_LIBRARY=/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2
  [ -e "$QDK_CUTENSORNET_LIBRARY" ] || QDK_CUTENSORNET_LIBRARY=$(in_tn_venv 'libcutensornet.so.2*')
fi
if [ -z "${QDK_CUDART_LIBRARY:-}" ]; then
  QDK_CUDART_LIBRARY=$CUDA_LIB/libcudart.so.12
  [ -e "$QDK_CUDART_LIBRARY" ] || QDK_CUDART_LIBRARY=$(in_tn_venv 'libcudart.so.12*')
fi
export QDK_CUTENSORNET_LIBRARY QDK_CUDART_LIBRARY
if [ -z "${QDK_CUSOLVER_LIBRARY:-}" ]; then
  for candidate in "$CUDA_LIB/libcusolver.so.11" \
    "$(ldconfig -p 2>/dev/null | awk '/libcusolver\.so\.11 /{print $NF; exit}')" \
    "$(in_tn_venv 'libcusolver.so.11*')"; do
    if [ -n "$candidate" ] && [ -e "$candidate" ]; then
      export QDK_CUSOLVER_LIBRARY=$candidate
      break
    fi
  done
fi

mkdir -p "$OUT"
: > "$SUMMARY"

# --- Helpers -----------------------------------------------------------------
# One line in summary.txt: the run, its exit code (124: timed out; 2: a
# reading disagrees with the reference; skipped: not run), its seconds and
# what it found.
summary() {
  printf 'part=%s exit=%s seconds=%s %s\n' "$1" "$2" "$3" "$4" | tee -a "$SUMMARY"
}

# The limit of a run that wants $1 seconds (scaled by LIMIT_PERCENT, at
# least a minute): the smaller of that and what is left of the budget; 0
# when under a minute is left.
box() {
  local want=$(($1 * LIMIT_PERCENT / 100)) left=$((BUDGET_SECONDS - TAR_MARGIN - SECONDS))
  [ "$want" -lt 60 ] && want=60
  [ "$left" -lt "$want" ] && want=$left
  [ "$want" -lt 60 ] && want=0
  echo "$want"
}

# Runs "$@" under a time limit of $1 seconds (0: skipped, the budget is
# spent), its output appended to $2; sets STATUS and SECONDS_TAKEN.
run_part() {
  local limit=$1 log=$2
  shift 2
  if [ "$limit" -le 0 ]; then
    echo "== $(date '+%F %T') skipped, the budget is spent: $*" | tee -a "$log"
    STATUS=skipped
    SECONDS_TAKEN=0
    return
  fi
  local start=$SECONDS
  echo "== $(date '+%F %T') limit=${limit}s $*" | tee -a "$log"
  timeout -k 60 "$limit" "$@" >> "$log" 2>&1
  STATUS=$?
  SECONDS_TAKEN=$((SECONDS - start))
  echo "== $(date '+%F %T') exit=$STATUS seconds=$SECONDS_TAKEN" | tee -a "$log"
}

# The value of key=value field $1 in line $2.
field() {
  printf '%s\n' "$2" | tr ' ' '\n' | awk -F= -v key="$1" '$1 == key {print $2; exit}'
}

# The lines of a test's log $2 that hold marker $1, each from the marker
# on. Not '^MARKER': with --nocapture libtest prints "test <name> ... "
# without a newline, so a test's first printed line follows it.
marked() {
  grep -o "$1 .*" "$2"
}

# Whether part $1 is in PARTS.
wanted() {
  case " $PARTS " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

# $1 / $2 to two decimals, or "-".
ratio() {
  awk -v a="$1" -v b="$2" 'BEGIN {if (a != "-" && a != "" && b != "-" && b + 0 > 0) printf "%.2f", a / b; else printf "-"}'
}

{
  date
  echo "repo=$REPO out=$OUT parts=$PARTS combos=$COMBOS relevant_cases=$RELEVANT_CASES"
  echo "budget_seconds=$BUDGET_SECONDS limit_percent=$LIMIT_PERCENT"
  git -C "$REPO" log --oneline -1
  nvidia-smi
  rustc --version
  cargo --version
  sha256sum "$H2"
  sha256sum "$INPUTS"/ppp*
  env | grep -E '^(QDK_|CUDA|CARGO)'
} > "$OUT/environment.txt" 2>&1

# H2's bytecode must be the one every earlier run used.
H2_OK=no
if [ -f "$H2" ] && [ "$(sha256sum "$H2" | awk '{print $1}')" = "$H2_SHA256" ]; then
  H2_OK=yes
fi
summary h2-bytecode - 0 "verdict=$([ "$H2_OK" = yes ] && echo pass || echo fail) step=h2-bytecode path=$H2 sha256_expected=$H2_SHA256 (on a fail, the H2 runs are skipped)"

# The MPS daemon off: every run is one process alone on the GPU.
pgrep -a nvidia-cuda-mps || true
echo quit | nvidia-cuda-mps-control 2>/dev/null || true

# --- a. build (release) ------------------------------------------------------
if wanted build; then
  log=$OUT/build.log
  run_part "$(box "$BUILD_BOX")" "$log" cargo build --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_cutensornet --example polyene_qpe_prefix
  build_cutensornet=$STATUS
  run_part "$(box "$BUILD_BOX")" "$log" cargo build --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_simulators --example h2_dev
  build_simulators=$STATUS
  run_part "$(box "$BUILD_BOX")" "$log" cargo test --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_cutensornet --test cuda_mps_on_the_gpu \
    --test cutensornet_mpo_on_the_gpu --no-run
  build_tests=$STATUS
  summary build "$build_cutensornet,$build_simulators,$build_tests" "$SECONDS" \
    "step=build polyene_qpe_prefix=$build_cutensornet h2_dev=$build_simulators gpu_tests=$build_tests"
fi

# --- a. kit: CudaMpsEngine's self-checks, kit and agreement tests -------------
if wanted kit; then
  log=$OUT/kit.log
  run_part "$(box "$KIT_BOX")" "$log" cargo test --manifest-path "$REPO/Cargo.toml" --release \
    $CARGO_FLAGS -p qdk_cutensornet --test cuda_mps_on_the_gpu -- --ignored --nocapture --test-threads=1
  checks=$(marked SELF_CHECK "$log" | grep -c .)
  failed=$(marked SELF_CHECK "$log" | grep -c '^SELF_CHECK .* passed=false')
  failed_names=$(marked SELF_CHECK "$log" | grep '^SELF_CHECK .* passed=false' | awk '{printf "%s%s", sep, $2; sep = ","}')
  # From libtest's closing list: with --nocapture a test's printed lines
  # may split its "test ... FAILED" line.
  failed_tests=$(awk '/^failures:$/ {n++; next} n == 2 && /^    [a-z_]/ {printf "%s%s", sep, $1; sep = ","}' "$log")
  result=$(grep '^test result: ' "$log" | tail -1)
  tests_passed=$(printf '%s\n' "$result" | sed -nE 's/.* ([0-9]+) passed.*/\1/p')
  tests_failed=$(printf '%s\n' "$result" | sed -nE 's/.* ([0-9]+) failed.*/\1/p')
  perturbation=$(marked GESVDP_PERTURBATION "$log" |
    awk '{for (i = 1; i <= NF; i++) if ($i ~ /^largest=/) {split($i, kv, "="); if (kv[2] + 0 > best) best = kv[2] + 0}} END {printf "%.3e", best + 0}')
  verdict=fail
  if [ "$STATUS" = 0 ] && [ "$checks" -gt 0 ] && [ "$failed" = 0 ]; then
    verdict=pass
  fi
  summary kit "$STATUS" "$SECONDS_TAKEN" \
    "verdict=$verdict step=kit engine=cuda-mps tests_passed=${tests_passed:--} tests_failed=${tests_failed:--}${failed_tests:+ ($failed_tests)} self_checks=$checks self_checks_failed=$failed${failed_names:+ ($failed_names)} gesvdp_largest_perturbation=$perturbation (see kit.log)"
fi

# --- b. diagnostic: CuTensorNetEngine's three-site MPOs ----------------------
if wanted diagnostic; then
  log=$OUT/diagnostic.log
  run_part "$(box "$DIAGNOSTIC_BOX")" "$log" cargo test --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_cutensornet --test cutensornet_mpo_on_the_gpu -- --ignored \
    --nocapture --test-threads=1
  lines=$(marked MPO_DIAGNOSTIC "$log" | grep -c .)
  if [ "$lines" = 0 ]; then
    summary diagnostic "$STATUS" "$SECONDS_TAKEN" \
      "verdict=error step=diagnostic engine=cutensornet no MPO_DIAGNOSTIC line, error=\"$(grep -E 'panicked|error' "$log" | head -2 | tr '\n' ' ' | cut -c1-300)\""
  fi
  marked MPO_DIAGNOSTIC "$log" | while read -r line; do
    # Not $(field case …): bash before 5.2 reads that `case` as the keyword
    # and looks for its esac.
    case_=${line#* case=}
    case_=${case_%% *}
    difference=$(field max_diff "$line")
    verdict=$(awk -v d="$difference" 'BEGIN {print (d != "" && d + 0 <= 1e-9) ? "agrees" : "differs"}')
    summary "diagnostic-$case_" "$STATUS" "$SECONDS_TAKEN" \
      "verdict=$verdict step=diagnostic engine=cutensornet ${line#MPO_DIAGNOSTIC }"
  done
fi

# --- The runs ----------------------------------------------------------------
# The flags of combination $1 (qr-on-gesvdp: --svd-algo gesvdp); QR moves
# are the engine's default.
combo_flags() {
  local qr=${1%-*} algo=${1##*-}
  case "$qr" in
  qr-on) echo "--svd-algo $algo" ;;
  *) echo "--no-qr-moves --svd-algo $algo" ;;
  esac
}

# One polyene prefix run on cuda-mps, seq3-mpo: name $1, flags $2 (word
# split), order $3, N $4, chi $5, limit $6, reference $7 (empty: none).
# Sets P_VERDICT (the harness's verdict, done, timed-out, error,
# skipped), P_STEPS, P_SECONDS (to the last step reached), P_LAST (the
# last step's seconds), P_DETAIL, and P_LOG.
polyene_run() {
  local name=$1 flags=$2 order=$3 n=$4 chi=$5 limit=$6 reference=$7
  P_LOG=$OUT/$name.log
  # shellcheck disable=SC2086
  run_part "$limit" "$P_LOG" "$BIN/polyene_qpe_prefix" --engine cuda-mps --n "$n" --chi "$chi" \
    --order "$order" --variant seq3-mpo $flags ${reference:+--reference "$reference"}
  local done_line steps_lines step check counters error key
  done_line=$(grep '^POLYENE done ' "$P_LOG" | tail -1)
  steps_lines=$(grep '^POLYENE step ' "$P_LOG" | tail -2)
  step=$(printf '%s\n' "$steps_lines" | tail -1)
  check=$(grep '^POLYENE check ' "$P_LOG" | tail -1)
  counters=$(grep '^POLYENE cuda_mps_counters ' "$P_LOG" | tail -1)
  error=$(grep -E '^POLYENE error |panicked' "$P_LOG" | tail -1 | cut -c1-200)
  if [ "$STATUS" = skipped ]; then
    P_VERDICT=skipped
  elif [ -n "$done_line" ]; then
    P_VERDICT=$(field verdict "$done_line")
    [ "$P_VERDICT" = NotChecked ] && P_VERDICT=done
  elif [ "$STATUS" = 124 ] || [ "$STATUS" = 137 ]; then
    P_VERDICT=timed-out
  else
    P_VERDICT=error
  fi
  P_STEPS=$(field p "$step")
  P_STEPS=${P_STEPS:-0}
  P_SECONDS=$(field seconds "$step")
  P_SECONDS=${P_SECONDS:--}
  P_LAST=-
  if [ "$(printf '%s\n' "$steps_lines" | grep -c .)" = 2 ]; then
    P_LAST=$(awk -v a="$(field seconds "$(printf '%s\n' "$steps_lines" | head -1)")" \
      -v b="$P_SECONDS" 'BEGIN {printf "%.1f", b - a}')
  elif [ "$P_STEPS" = 1 ]; then
    P_LAST=$P_SECONDS
  fi
  P_DETAIL="one_minus_w=$(field one_minus_w "$step") largest_bond=$(field largest_bond "$step")"
  if [ -n "$counters" ]; then
    for key in qr_moves qr_move_seconds gate_splits gate_split_seconds mpo_operators \
      mpo_seconds largest_svd_perturbation; do
      P_DETAIL="$P_DETAIL $key=$(field "$key" "$counters")"
    done
  fi
  [ -n "$check" ] && P_DETAIL="$P_DETAIL largest_gap=$(field largest "$check")"
  [ -n "$error" ] && [ "$P_VERDICT" = error ] && P_DETAIL="$P_DETAIL error=\"$error\""
}

# One H2 shot on cuda-mps, seq3-mpo, the resumer's order: name $1,
# combination $2, limit $3. Sets H_VERDICT (pass, fail, timed-out,
# skipped), H_SECONDS and H_DETAIL.
h2_run() {
  local name=$1 combo=$2 limit=$3
  local log=$OUT/$name.log
  H_SECONDS=-
  if [ "$H2_OK" != yes ]; then
    STATUS=skipped
    SECONDS_TAKEN=0
    H_VERDICT=skipped
    H_DETAIL="H2's bytecode is missing or not the expected one"
    return
  fi
  # shellcheck disable=SC2046
  run_part "$limit" "$log" "$BIN/h2_dev" --engine cuda-mps --bytecode "$H2" --readout "$TABLE" \
    --seed 1 --chi 128 --first-shot 0 --shots 1 --order resumer --variant seq3-mpo \
    $(combo_flags "$combo")
  local shot
  shot=$(grep -m1 '^SHOT ' "$log" || true)
  if [ "$STATUS" = skipped ]; then
    H_VERDICT=skipped
    H_DETAIL="the budget is spent"
  elif [ -n "$shot" ]; then
    local records draws tvd
    records=$(field records "$shot")
    draws=$(field draws "$shot")
    tvd=$(field tvd "$shot")
    H_SECONDS=$(field seconds "$shot")
    H_VERDICT=fail
    if [ "$records" = 01010 ] && [ "$draws" = 1414 ] &&
      awk -v t="$tvd" 'BEGIN {exit !(t != "" && t + 0 <= 1e-9)}'; then
      H_VERDICT=pass
    fi
    H_DETAIL="records=$records draws=$draws tvd=$tvd"
    local key
    for key in qr_moves qr_move_seconds gate_splits gate_split_seconds mpo_operators \
      mpo_seconds largest_svd_perturbation; do
      H_DETAIL="$H_DETAIL $key=$(field "$key" "$shot")"
    done
  elif [ "$STATUS" = 124 ] || [ "$STATUS" = 137 ]; then
    H_VERDICT=timed-out
    H_DETAIL="no SHOT line before the limit"
  else
    H_VERDICT=fail
    H_DETAIL="no SHOT line, error=\"$(grep -E -A1 'panicked|error' "$log" | tail -2 | tr '\n' ' ' | cut -c1-300)\""
  fi
}

# --- c. correctness: polyene N = 8 against the exact state, and one H2 shot ---
if wanted correctness; then
  for combo in $COMBOS; do
    name=correctness-n8-cuda-mps-$combo
    polyene_run "$name" "$(combo_flags "$combo")" identity 8 128 "$(box "$N8_BOX")" \
      ppp8.exact.reference
    verdict=$([ "$P_VERDICT" = AgreesWithExact ] && echo pass || echo "fail-or-unverified ($P_VERDICT)")
    summary "$name" "$STATUS" "$SECONDS_TAKEN" \
      "verdict=$verdict step=correctness-n8 engine=cuda-mps combo=$combo variant=seq3-mpo order=identity n=8 chi=128 steps_done=$P_STEPS gpu_seconds=$P_SECONDS $P_DETAIL"

    name=correctness-h2-cuda-mps-$combo
    h2_run "$name" "$combo" "$(box "$H2_BOX")"
    summary "$name" "$STATUS" "$SECONDS_TAKEN" \
      "verdict=$H_VERDICT step=correctness-h2 engine=cuda-mps combo=$combo variant=seq3-mpo order=resumer shot_seconds=$H_SECONDS $H_DETAIL"
  done
fi

# --- d. relevant: polyene N = 16 and N = 20, the whole prefix -----------------
# c_p's largest gap between log $1 and reference file $2's "# cp" lines:
# "<gap> p=<p> compared=<count>".
cp_gap() {
  awk '
    FNR == NR {
      if ($1 == "#" && $2 == "cp") {
        split($3, p, "="); split($4, re, "="); split($5, im, "=")
        ref_re[p[2]] = re[2]; ref_im[p[2]] = im[2]
      }
      next
    }
    $1 == "POLYENE" && $2 == "cp" {
      delete v
      for (i = 3; i <= NF; i++) { split($i, kv, "="); v[kv[1]] = kv[2] }
      if (v["p"] in ref_re) {
        d = sqrt((v["re"] - ref_re[v["p"]]) ^ 2 + (v["im"] - ref_im[v["p"]]) ^ 2)
        n++
        if (d >= best) { best = d; at = v["p"] }
      }
    }
    END { if (n) printf "%.3e p=%s compared=%d", best, at, n; else printf "- p=- compared=0" }
  ' "$2" "$1"
}

# Writes log $1's steps to file $2 (p, cumulative and step seconds, largest
# bond, 1 - W) and prints "<mean step seconds> <largest bond>".
steps_table() {
  grep '^POLYENE step ' "$1" | awk -v out="$2" '
    BEGIN { print "p seconds step_seconds largest_bond one_minus_w" > out }
    {
      delete v
      for (i = 3; i <= NF; i++) { split($i, kv, "="); v[kv[1]] = kv[2] }
      step = v["seconds"] - last; last = v["seconds"]; n++
      if (v["largest_bond"] + 0 > bond) bond = v["largest_bond"] + 0
      printf "%s %s %.1f %s %s\n", v["p"], v["seconds"], step, v["largest_bond"], v["one_minus_w"] > out
    }
    END { if (n) printf "%.1f %d", last / n, bond; else printf "- -" }'
}

if wanted relevant; then
  for case_ in $RELEVANT_CASES; do
    n=${case_%/*}
    chi=${case_#*/}
    reference=$(reference_of "$n")
    name=relevant-n$n-chi$chi-cuda-mps
    polyene_run "$name" "" interleave-centre "$n" "$chi" "$(box "$(relevant_box "$n")")" \
      "$reference"
    read -r mean bond <<< "$(steps_table "$P_LOG" "$OUT/steps-$name.txt")"
    read -r cpu_seconds cpu_last <<< "$(cpu_prefix "$n" "$chi")"
    end=$(grep '^POLYENE end ' "$P_LOG" | tail -1)
    c32=$(grep '^POLYENE cp p=32 ' "$P_LOG" | tail -1)
    gap=-
    cp_verdict=not-compared
    if [ -n "$reference" ]; then
      gap=$(cp_gap "$P_LOG" "$INPUTS/$reference")
      cp_verdict=$(awk -v g="${gap%% *}" -v t="$CP_TOLERANCE" \
        'BEGIN {print (g != "-" && g + 0 <= t + 0) ? "pass" : "fail"}')
    fi
    line="verdict=$P_VERDICT cp_verdict=$cp_verdict step=relevant engine=cuda-mps combo=defaults"
    line="$line variant=seq3-mpo order=interleave-centre n=$n chi=$chi steps_done=$P_STEPS"
    line="$line gpu_seconds=$P_SECONDS cpu_prefix_seconds=$cpu_seconds"
    line="$line gpu_over_cpu=$([ "$P_STEPS" = 32 ] && ratio "$P_SECONDS" "$cpu_seconds" || echo -)"
    line="$line mean_step_seconds=$mean last_step_seconds=$P_LAST cpu_last_step_seconds=$cpu_last"
    line="$line largest_bond_reached=$bond $P_DETAIL"
    line="$line ancilla_one=$(field ancilla_one "$end") c32_re=$(field re "$c32") c32_im=$(field im "$c32")"
    line="$line energy=$(field energy "$end") n_alpha=$(field n_alpha "$end") n_beta=$(field n_beta "$end")"
    line="$line reference=$reference cp_largest_gap=$gap cp_tolerance=$CP_TOLERANCE"
    line="$line (steps-$name.txt)"
    summary "$name" "$STATUS" "$SECONDS_TAKEN" "$line"
  done
fi

# --- What to send back ---------------------------------------------------------
tar czf "$OUT.tar.gz" --exclude='*.tar.gz' -C "$(dirname "$OUT")" "$(basename "$OUT")"
echo "Send back: $OUT.tar.gz"
echo "== Summary ($SECONDS s)"
cat "$SUMMARY"
