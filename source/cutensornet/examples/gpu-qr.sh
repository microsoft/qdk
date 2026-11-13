#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# Development only (not for review): do CudaMpsEngine's QR centre moves
# (with_dev_qr_moves: one TensorQR and one contraction per exact centre
# move, in place of a GateSplit) and the Jacobi SVD (gesvdj, in place of
# cuSOLVER's GESVD) keep its results and make it faster? cuda-mps only,
# seq3-mpo (gates fused in order into operators on up to three sites, a
# far one applied as an MPO), each case in the order gpu-mpo.sh used.
#
# Polyene (the PPP model, N carbon sites, 2N + 1 qubits) is a development
# case and a candidate relevant case, pending the chemistry team; H2 is a
# development case.
#
# A combination is the QR moves off or on, and an SVD algorithm:
# qr-off-gesvd (the engine as gpu-mpo.sh ran it), qr-on-gesvd,
# qr-off-gesvdj and qr-on-gesvdj. Each part has its own log and one line
# per run in summary.txt, naming the step, the engine and the combination;
# a run that fails or times out does not stop the next. The whole session
# keeps within BUDGET_SECONDS (3 h by default): each run's limit is the
# smaller of its own and what is left, and a run with under a minute left
# is skipped, its summary line saying so. The parts in the order they run,
# with ESTIMATED durations (not measured: the A100 runs the QR moves for
# the first time; from F's bench, TensorQR 0.8-4.9 ms against GESVD's
# 26-196 ms at bonds 128-512, and gpu-mpo.sh's boxes):
#
#   build        release builds of polyene_qpe_prefix, h2_dev and the kit's
#                test binary; about 20-30 min on a fresh target dir.
#   kit          CudaMpsEngine's GPU tests (tests/cuda_mps_on_the_gpu.rs):
#                the self-checks (GateSplit, and now each QR move against
#                its gated pair), the conformance kit, agreement with the
#                CPU engine, the MPO path, the QR moves and the Jacobi
#                SVD against the CPU engine. Pass: every test passes, no
#                SELF_CHECK passed=false. 1-5 min.
#   correctness  for each combination: polyene N = 8 chi 128 in the
#                identity order against ppp8.exact.reference (pass:
#                verdict=AgreesWithExact), and one H2 shot in the resumer's
#                order (pass: records=01010, draws=1414, tvd <= 1e-9). A
#                combination that fails either is skipped in timing, and
#                the summary says so; one that timed out is timed, marked
#                unverified. Boxed at N8_BOX (600 s) and H2_BOX (600 s);
#                about 80 min at most for the four.
#   timing       the H2 shot's seconds (from correctness) against the CPU's
#                12.6 s, with the engine's counters from its SHOT line, then
#                the polyene prefix in interleave-centre at
#                TIMING_CASES (N/chi; N = 16, chi 64, 128, 256), each
#                combination that passed, TIMING_STEPS (32) steps or the
#                run's box (TIMING_BOX, 240 s): the steps reached, the
#                GPU's seconds to that step, the CPU's at the same step,
#                and the counters (qr_moves and their seconds, GateSplits
#                and their seconds, the MPO operators and their device and
#                host seconds). About 50 min at most.
#
# Run from anywhere, on the branch's checkout:
#
#   bash source/cutensornet/examples/gpu-qr.sh
#
# Environment (all optional):
#   OUT           the output directory (default $HOME/gpu-qr-<date>)
#   PARTS         the parts to run (default "build kit correctness timing")
#   COMBOS        the combinations (default "qr-off-gesvd qr-on-gesvd
#                 qr-off-gesvdj qr-on-gesvdj")
#   BUDGET_SECONDS the session's budget (default 10800, 3 h)
#   LIMIT_PERCENT the runs' own limits in percent (default 100)
#   N8_BOX, H2_BOX, TIMING_BOX each run's own limit in seconds (default 600,
#                 600, 240)
#   TIMING_CASES  the timing part's N/chi cases (default "16/64 16/128
#                 16/256")
#   TIMING_STEPS  the timing part's steps (default 32)
#   H2            H2's bytecode JSON (default /data/domingom/h2.bytecode.json)
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY, QDK_CUSOLVER_LIBRARY
#                 the libraries, when neither at the pinned paths below nor
#                 in /data/domingom/tn-venv
#   CARGO_FLAGS   extra flags for cargo build (for instance --offline)

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
OUT=${OUT:-$HOME/gpu-qr-$(date +%Y%m%d-%H%M)}
PARTS=${PARTS:-build kit correctness timing}
COMBOS=${COMBOS:-qr-off-gesvd qr-on-gesvd qr-off-gesvdj qr-on-gesvdj}
BUDGET_SECONDS=${BUDGET_SECONDS:-10800}
LIMIT_PERCENT=${LIMIT_PERCENT:-100}
N8_BOX=${N8_BOX:-600}
H2_BOX=${H2_BOX:-600}
TIMING_BOX=${TIMING_BOX:-240}
TIMING_CASES=${TIMING_CASES:-16/64 16/128 16/256}
TIMING_STEPS=${TIMING_STEPS:-32}
H2=${H2:-/data/domingom/h2.bytecode.json}
TABLE=$REPO/source/simulators/examples/h2-readout-distribution.txt
TN_VENV=/data/domingom/tn-venv
CARGO_FLAGS=${CARGO_FLAGS:-}
BIN=${CARGO_TARGET_DIR:-$REPO/target}/release/examples
SUMMARY=$OUT/summary.txt
# Kept for the tar at the end.
TAR_MARGIN=300

# --- The CPU's numbers, one core, for the ratios -----------------------------
# Cumulative seconds after each controlled step p = 1, 2, ... of the
# polyene prefix, seq3-mpo (gpu-mpo.sh's, from K's runs). Empty: not
# measured yet.
CPU_N16_CHI64_SEQ3_IC=${CPU_N16_CHI64_SEQ3_IC:-"0.5 1.6 3.7 6.4 9.7 13.7 19.4 27.2 38.1 51.8 68.0 87.2 108.5 131.6 155.0 178.4 202.1 225.9 249.6 273.8 298.1 322.5 347.2 371.7 396.4 421.2 446.4 473.0 499.9 525.5 550.9 576.3"}
CPU_N16_CHI128_SEQ3_IC=${CPU_N16_CHI128_SEQ3_IC:-"0.4 1.6 3.7 6.3 9.5 13.3 18.6 25.8 35.9 48.7 64.3 82.3 105.8 136.8 176.2 228.3 291.3 365.0 453.3 561.2 689.5 833.7 1011.9 1173.8 1347.0 1511.6 1685.7 1858.7 2023.3 2186.5 2350.2 2515.7"}
CPU_N16_CHI256_SEQ3_IC=${CPU_N16_CHI256_SEQ3_IC:-""}
CPU_N8_CHI128_SEQ3_ID="0.5 1.9 4.2 7.3 11.4 16.1 21.8 28.1 35.0 42.6 51.1 60.0 69.1 78.9 89.0 99.3 109.5 120.1 131.0 142.3 154.1 165.8 177.8 189.5 201.3 213.2 225.1 238.5 252.5 266.7 281.2 295.5"
# One H2 shot on one CPU core, seq3-mpo, the resumer's order (L).
CPU_H2_SEQ3=12.6

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

# Whether part $1 is in PARTS.
wanted() {
  case " $PARTS " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

# The $2-th value of the timeline $1, or "-".
at_step() {
  printf '%s\n' "$1" | awk -v p="$2" '{if (p >= 1 && p <= NF) print $p; else print "-"}'
}

# $1 / $2 to two decimals, or "-".
ratio() {
  awk -v a="$1" -v b="$2" 'BEGIN {if (a != "-" && a != "" && b != "-" && b + 0 > 0) printf "%.2f", a / b; else printf "-"}'
}

{
  date
  echo "repo=$REPO out=$OUT parts=$PARTS combos=$COMBOS"
  echo "budget_seconds=$BUDGET_SECONDS limit_percent=$LIMIT_PERCENT n8_box=$N8_BOX h2_box=$H2_BOX timing_box=$TIMING_BOX timing_cases=$TIMING_CASES timing_steps=$TIMING_STEPS"
  git -C "$REPO" log --oneline -1
  nvidia-smi
  rustc --version
  cargo --version
  sha256sum "$H2"
  sha256sum "$REPO"/source/cutensornet/examples/polyene_qpe_prefix/ppp*
  env | grep -E '^(QDK_|CUDA|CARGO|CPU_)'
} > "$OUT/environment.txt" 2>&1
echo "H2's sha256 must be a8a2b39cdb9df504f3142c6d1cff34097438d0da0add5049979878a45189ec5f:"
grep -E "^[0-9a-f]{64}  $H2\$" "$OUT/environment.txt" || echo "  (not found: $H2)"

# The MPS daemon off: every run is one process alone on the GPU.
pgrep -a nvidia-cuda-mps || true
echo quit | nvidia-cuda-mps-control 2>/dev/null || true

# --- build (release) ---------------------------------------------------------
if wanted build; then
  run_part "$(box 3600)" "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_cutensornet --example polyene_qpe_prefix
  build_cutensornet=$STATUS
  run_part "$(box 3600)" "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_simulators --example h2_dev
  build_simulators=$STATUS
  run_part "$(box 3600)" "$OUT/build.log" cargo test --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_cutensornet --test cuda_mps_on_the_gpu --no-run
  build_kit=$STATUS
  summary build "$build_cutensornet,$build_simulators,$build_kit" "$SECONDS" \
    "step=build polyene_qpe_prefix=$build_cutensornet h2_dev=$build_simulators kit=$build_kit"
fi

# --- kit: CudaMpsEngine's self-checks (QR moves included) and kit ------------
if wanted kit; then
  log=$OUT/kit.log
  run_part "$(box 900)" "$log" cargo test --manifest-path "$REPO/Cargo.toml" --release \
    $CARGO_FLAGS -p qdk_cutensornet --test cuda_mps_on_the_gpu -- --ignored --nocapture --test-threads=1
  checks=$(grep -c '^SELF_CHECK ' "$log")
  qr_checks=$(grep -c '^SELF_CHECK qr-move-' "$log")
  failed=$(grep -c '^SELF_CHECK .* passed=false' "$log")
  failed_names=$(grep '^SELF_CHECK .* passed=false' "$log" | awk '{printf "%s%s", sep, $2; sep = ","}')
  failed_tests=$(grep -E '^test .* FAILED$' "$log" | awk '{printf "%s%s", sep, $2; sep = ","}')
  result=$(grep '^test result: ' "$log" | tail -1)
  tests_passed=$(printf '%s\n' "$result" | sed -nE 's/.* ([0-9]+) passed.*/\1/p')
  tests_failed=$(printf '%s\n' "$result" | sed -nE 's/.* ([0-9]+) failed.*/\1/p')
  verdict=fail
  if [ "$STATUS" = 0 ] && [ "$checks" -gt 0 ] && [ "$qr_checks" -gt 0 ] && [ "$failed" = 0 ]; then
    verdict=pass
  fi
  summary kit "$STATUS" "$SECONDS_TAKEN" \
    "verdict=$verdict step=kit engine=cuda-mps tests_passed=${tests_passed:--} tests_failed=${tests_failed:--}${failed_tests:+ ($failed_tests)} self_checks=$checks qr_move_checks=$qr_checks self_checks_failed=$failed${failed_names:+ ($failed_names)} (see kit.log)"
fi

# --- The runs ----------------------------------------------------------------
# The flags of combination $1 (qr-on-gesvdj: --qr-moves --svd-algo gesvdj).
combo_flags() {
  local qr=${1%-*} algo=${1##*-}
  case "$qr" in
  qr-on) echo "--qr-moves --svd-algo $algo" ;;
  *) echo "--svd-algo $algo" ;;
  esac
}

# One polyene prefix run on cuda-mps, seq3-mpo: name $1, combination $2,
# order $3, N $4, chi $5, limit $6, steps $7 (empty: the program's 32),
# reference $8 (empty: none). Sets P_VERDICT (AgreesWithExact, Disagrees,
# done, timed-out, error, skipped), P_STEPS, P_SECONDS (to the last step
# reached), P_LAST (the last step's seconds) and P_DETAIL.
polyene_run() {
  local name=$1 combo=$2 order=$3 n=$4 chi=$5 limit=$6 steps=$7 reference=$8
  local log=$OUT/$name.log
  # shellcheck disable=SC2046
  run_part "$limit" "$log" "$BIN/polyene_qpe_prefix" --engine cuda-mps --n "$n" --chi "$chi" \
    --order "$order" --variant seq3-mpo $(combo_flags "$combo") ${steps:+--steps "$steps"} \
    ${reference:+--reference "$reference"}
  local done_line steps_lines step check counters error key
  done_line=$(grep '^POLYENE done ' "$log" | tail -1)
  steps_lines=$(grep '^POLYENE step ' "$log" | tail -2)
  step=$(printf '%s\n' "$steps_lines" | tail -1)
  check=$(grep '^POLYENE check ' "$log" | tail -1)
  counters=$(grep '^POLYENE cuda_mps_counters ' "$log" | tail -1)
  error=$(grep -E '^POLYENE error |panicked' "$log" | tail -1 | cut -c1-200)
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
    for key in qr_moves qr_move_seconds gate_splits gate_split_seconds splits centre_moves \
      mpo_operators mpo_seconds mpo_host_seconds; do
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
  # shellcheck disable=SC2046
  run_part "$limit" "$log" "$BIN/h2_dev" --engine cuda-mps --bytecode "$H2" --readout "$TABLE" \
    --seed 1 --chi 128 --first-shot 0 --shots 1 --order resumer --variant seq3-mpo \
    $(combo_flags "$combo")
  local shot
  shot=$(grep -m1 '^SHOT ' "$log" || true)
  H_SECONDS=-
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
    for key in qr_moves qr_move_seconds gate_splits gate_split_seconds centre_moves \
      mpo_operators mpo_seconds mpo_host_seconds; do
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

# Each combination's correctness: pass, fail or unverified (unset: not
# checked).
declare -A CORRECT
# Folds verdict $2 (pass, fail or unverified) into $1's.
mark() {
  case "${CORRECT[$1]:-pass}/$2" in
  fail/* | */fail) CORRECT[$1]=fail ;;
  unverified/* | */unverified) CORRECT[$1]=unverified ;;
  *) CORRECT[$1]=pass ;;
  esac
}

# --- correctness: polyene N = 8 against the exact state, and one H2 shot ------
# The H2 shots' seconds and verdicts, kept for timing.
declare -A H2_SECONDS H2_VERDICT H2_DETAIL
if wanted correctness; then
  for combo in $COMBOS; do
    name=correctness-n8-cuda-mps-$combo
    polyene_run "$name" "$combo" identity 8 128 "$(box "$N8_BOX")" "" ppp8.exact.reference
    case "$P_VERDICT" in
    AgreesWithExact) mark "$combo" pass ;;
    timed-out | skipped) mark "$combo" unverified ;;
    *) mark "$combo" fail ;;
    esac
    cpu_at=$(at_step "$CPU_N8_CHI128_SEQ3_ID" "$P_STEPS")
    line="verdict=$([ "$P_VERDICT" = AgreesWithExact ] && echo pass || echo "fail-or-unverified ($P_VERDICT)")"
    line="$line step=correctness-n8 engine=cuda-mps combo=$combo variant=seq3-mpo order=identity n=8 chi=128"
    line="$line steps_done=$P_STEPS gpu_seconds=$P_SECONDS cpu_seconds_same_step=$cpu_at"
    line="$line gpu_over_cpu=$(ratio "$P_SECONDS" "$cpu_at") $P_DETAIL"
    summary "$name" "$STATUS" "$SECONDS_TAKEN" "$line"

    name=correctness-h2-cuda-mps-$combo
    h2_run "$name" "$combo" "$(box "$H2_BOX")"
    H2_SECONDS[$combo]=$H_SECONDS
    H2_VERDICT[$combo]=$H_VERDICT
    H2_DETAIL[$combo]=$H_DETAIL
    case "$H_VERDICT" in
    pass) mark "$combo" pass ;;
    timed-out | skipped) mark "$combo" unverified ;;
    *) mark "$combo" fail ;;
    esac
    summary "$name" "$STATUS" "$SECONDS_TAKEN" \
      "verdict=$H_VERDICT step=correctness-h2 engine=cuda-mps combo=$combo variant=seq3-mpo order=resumer shot_seconds=$H_SECONDS $H_DETAIL"
  done
  for combo in $COMBOS; do
    summary "correctness-cuda-mps-$combo" - - \
      "verdict=${CORRECT[$combo]} step=correctness engine=cuda-mps combo=$combo (polyene N = 8 identity against the exact state, and one H2 shot; fail: skipped in timing; unverified: timed, marked)"
  done
fi

# Whether combination $1 may be timed; prints why not otherwise.
may_time() {
  case "${CORRECT[$1]:-not-checked}" in
  fail)
    echo "skipped: engine cuda-mps combination $1 failed the correctness part"
    return 1
    ;;
  esac
  return 0
}

# --- timing: H2's shot, then the polyene prefix in interleave-centre ---------
# The CPU timeline at N $1, chi $2 (interleave-centre, seq3-mpo).
cpu_timeline() {
  case "$1/$2" in
  16/64) echo "$CPU_N16_CHI64_SEQ3_IC" ;;
  16/128) echo "$CPU_N16_CHI128_SEQ3_IC" ;;
  16/256) echo "$CPU_N16_CHI256_SEQ3_IC" ;;
  esac
}

if wanted timing; then
  for combo in $COMBOS; do
    name=timing-h2-cuda-mps-$combo
    if ! why=$(may_time "$combo"); then
      summary "$name" skipped 0 "verdict=$why step=timing-h2"
      continue
    fi
    if [ -n "${H2_VERDICT[$combo]:-}" ]; then
      verdict=${H2_VERDICT[$combo]}
      seconds=${H2_SECONDS[$combo]}
      detail=${H2_DETAIL[$combo]}
      exit_=from-correctness
      taken=0
    else
      h2_run "$name" "$combo" "$(box "$H2_BOX")"
      verdict=$H_VERDICT
      seconds=$H_SECONDS
      detail=$H_DETAIL
      exit_=$STATUS
      taken=$SECONDS_TAKEN
    fi
    summary "$name" "$exit_" "$taken" \
      "verdict=$verdict step=timing-h2 engine=cuda-mps combo=$combo variant=seq3-mpo order=resumer shot_seconds=$seconds cpu_seconds=$CPU_H2_SEQ3 gpu_over_cpu=$(ratio "$seconds" "$CPU_H2_SEQ3") $detail"
  done
  for case_ in $TIMING_CASES; do
    n=${case_%/*}
    chi=${case_#*/}
    for combo in $COMBOS; do
      name=timing-n$n-chi$chi-cuda-mps-$combo
      if ! why=$(may_time "$combo"); then
        summary "$name" skipped 0 "verdict=$why step=timing-polyene"
        continue
      fi
      polyene_run "$name" "$combo" interleave-centre "$n" "$chi" "$(box "$TIMING_BOX")" \
        "$TIMING_STEPS" ""
      cpu_at=$(at_step "$(cpu_timeline "$n" "$chi")" "$P_STEPS")
      line="verdict=$P_VERDICT step=timing-polyene engine=cuda-mps combo=$combo"
      line="$line correctness=${CORRECT[$combo]:-not-checked} variant=seq3-mpo"
      line="$line order=interleave-centre n=$n chi=$chi steps_done=$P_STEPS"
      line="$line gpu_seconds=$P_SECONDS last_step_seconds=$P_LAST"
      line="$line cpu_seconds_same_step=$cpu_at gpu_over_cpu=$(ratio "$P_SECONDS" "$cpu_at")"
      summary "$name" "$STATUS" "$SECONDS_TAKEN" "$line $P_DETAIL"
    done
  done
fi

# --- What to send back ---------------------------------------------------------
tar czf "$OUT.tar.gz" --exclude='*.tar.gz' -C "$(dirname "$OUT")" "$(basename "$OUT")"
echo "Send back: $OUT.tar.gz"
echo "== Summary ($SECONDS s)"
cat "$SUMMARY"
