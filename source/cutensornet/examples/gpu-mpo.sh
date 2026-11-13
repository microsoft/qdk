#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# Development only (not for review): do the GPU engines get the CPU's gains
# (the interleave-centre order, seq3-mpo, Merge), and where, in N and chi,
# does the GPU beat one CPU core? Both GPU engines run: the State API
# engine (cutensornet, CuTensorNetEngine) and the CUDA MPS engine
# (cuda-mps, CudaMpsEngine), each in two variants: base (the gates one by
# one, far gates routed with SWAPs) and seq3-mpo (gates fused in order into
# operators on up to three sites; cuda-mps applies a far one as an MPO
# across its span, cutensornet passes it whole to the State API).
#
# Polyene (the PPP model, N carbon sites, 2N + 1 qubits) is a development
# case and a candidate relevant case, pending the chemistry team; H2 is a
# development case.
#
# Each part has its own log and one line per run in summary.txt; a run that
# fails or times out does not stop the next. The whole session keeps within
# BUDGET_SECONDS (6 h): each run's limit is the smaller of its own and what
# is left, and a run with under a minute left is skipped, its summary line
# saying so. The parts, in order, with ESTIMATED durations (not measured:
# from the A100's GateSplit at b = 64, 128, 256: 12.3, 26.1, 64.7 ms, and
# the CPU's runs):
#
#   build        release builds of polyene_qpe_prefix, gate_split_bench,
#                h2_dev, polyene_shots_dev and the kit's test binary; about
#                20-30 min on a fresh target dir.
#   kit          CudaMpsEngine's GPU tests (tests/cuda_mps_on_the_gpu.rs):
#                the GateSplit self-checks, the conformance kit, agreement
#                with the CPU engine to 1e-10 on random 7-site circuits, and
#                the MPO path (with_dev_mpo_far) against the CPU's. Pass:
#                every test passes, no SELF_CHECK passed=false. 1-5 min.
#   svd-bench    gate_split_bench: every SVD algorithm, both gate-split
#                algorithms, bonds 8 to 512. Under 5 min (the reduced-only
#                bench took 12 s on 2026-11-12).
#   correctness  before any timing, for each engine x variant: polyene
#                N = 8 chi 128 in the identity order against
#                ppp8.exact.reference (pass: verdict=AgreesWithExact; the
#                interleave-centre order disagrees with the exact state at
#                chi 128 on the CPU too, so it is not the check), and one H2
#                shot (pass: records=01010, draws=1414, tvd <= 1e-9). A
#                variant that fails either is skipped in the parts after;
#                one that timed out runs, marked unverified. Estimates:
#                polyene 5-45 min each (State API base 27.9 s/step: 15 min),
#                H2 3-20 min each (State API default: about 18 min). Boxed
#                at 20-30 min each; 1.5-2.5 h in all.
#   timing       the polyene prefix in interleave-centre at N = 16, chi 64,
#                128, 256 and N = 20, chi 128, 256, each engine x variant,
#                TIMING_STEPS (32) steps or the run's box: 8-12 min for
#                seq3-mpo, 3 min for base (seq3-mpo first, so the deadline
#                cuts base). Per run: the steps reached, the GPU's seconds
#                to that step, the CPU's at the same step (one core, same
#                order and variant) and their ratio. About 2.2 h at most.
#   h2           one line per engine x variant: the H2 shot's seconds (from
#                the correctness part, or run here if that part did not
#                run) against the CPU's 84.8 s (default) and 12.6 s
#                (seq3-mpo).
#   full         the full polyene program, N = 16, S = 64, Merge + seq3-mpo
#                in interleave-centre, chi 64 and 128, on the faster engine
#                at timing's N = 16 chi 64 seq3-mpo; boxed at 25 min each
#                (one CPU core: 1,000 s at chi 64). It prints nothing until
#                all 64 shots end, so a timed-out run reports no shots.
#
# Run from anywhere, on the branch's checkout:
#
#   bash source/cutensornet/examples/gpu-mpo.sh
#
# Environment (all optional):
#   OUT           the output directory (default $HOME/gpu-mpo-<date>)
#   PARTS         the parts to run (default: all, in the order above)
#   ENGINES       the GPU engines (default "cutensornet cuda-mps")
#   VARIANTS      the variants (default "base seq3-mpo")
#   BUDGET_SECONDS the session's budget (default 21600, 6 h)
#   LIMIT_PERCENT the runs' own limits in percent (default 100)
#   TIMING_STEPS  the timing part's steps (default 32)
#   FULL_SHOTS    the full program's shots (default 64)
#   H2            H2's bytecode JSON (default /data/domingom/h2.bytecode.json)
#   PPP16, PPP16_PARAMS the full program's bytecode and parameters (default
#                 /data/domingom/ppp16_m6.bytecode.json and
#                 /data/domingom/ppp16.params.json; copy them there first)
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY, QDK_CUSOLVER_LIBRARY
#                 the libraries, when neither at the pinned paths below nor
#                 in /data/domingom/tn-venv
#   CARGO_FLAGS   extra flags for cargo build (for instance --offline)

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
OUT=${OUT:-$HOME/gpu-mpo-$(date +%Y%m%d-%H%M)}
PARTS=${PARTS:-build kit svd-bench correctness timing h2 full}
ENGINES=${ENGINES:-cutensornet cuda-mps}
VARIANTS=${VARIANTS:-base seq3-mpo}
BUDGET_SECONDS=${BUDGET_SECONDS:-21600}
LIMIT_PERCENT=${LIMIT_PERCENT:-100}
TIMING_STEPS=${TIMING_STEPS:-32}
FULL_SHOTS=${FULL_SHOTS:-64}
H2=${H2:-/data/domingom/h2.bytecode.json}
PPP16=${PPP16:-/data/domingom/ppp16_m6.bytecode.json}
PPP16_PARAMS=${PPP16_PARAMS:-/data/domingom/ppp16.params.json}
TABLE=$REPO/source/simulators/examples/h2-readout-distribution.txt
TN_VENV=/data/domingom/tn-venv
CARGO_FLAGS=${CARGO_FLAGS:-}
BIN=${CARGO_TARGET_DIR:-$REPO/target}/release/examples
SUMMARY=$OUT/summary.txt
# Kept for the tar at the end.
TAR_MARGIN=300

# --- The CPU's numbers, one core, for the ratios -----------------------------
# Cumulative seconds after each controlled step p = 1, 2, ... of the
# polyene prefix (polyene_swaps_dev or polyene_qpe_prefix, release, one
# core of the development machine). Empty: not measured yet.
#
# Interleave-centre, seq3-mpo; K's runs (polyene-swaps, logs
# i16-seq3-mpo-ic-32 and i16-seq3-mpo-ic-128-32, 2026-11-12):
CPU_N16_CHI64_SEQ3_IC=${CPU_N16_CHI64_SEQ3_IC:-"0.5 1.6 3.7 6.4 9.7 13.7 19.4 27.2 38.1 51.8 68.0 87.2 108.5 131.6 155.0 178.4 202.1 225.9 249.6 273.8 298.1 322.5 347.2 371.7 396.4 421.2 446.4 473.0 499.9 525.5 550.9 576.3"}
CPU_N16_CHI128_SEQ3_IC=${CPU_N16_CHI128_SEQ3_IC:-"0.4 1.6 3.7 6.3 9.5 13.3 18.6 25.8 35.9 48.7 64.3 82.3 105.8 136.8 176.2 228.3 291.3 365.0 453.3 561.2 689.5 833.7 1011.9 1173.8 1347.0 1511.6 1685.7 1858.7 2023.3 2186.5 2350.2 2515.7"}
# SLOTS for the CPU agent polyene-n20's numbers (the coordinator fills them
# in, in the same format):
CPU_N16_CHI256_SEQ3_IC=${CPU_N16_CHI256_SEQ3_IC:-""}
CPU_N20_CHI128_SEQ3_IC=${CPU_N20_CHI128_SEQ3_IC:-""}
CPU_N20_CHI256_SEQ3_IC=${CPU_N20_CHI256_SEQ3_IC:-""}
# Interleave-centre, base; K's run i16-base-ic-4 (4 steps only):
CPU_N16_CHI64_BASE_IC=${CPU_N16_CHI64_BASE_IC:-"1.4 5.2 11.5 20.1"}
CPU_N16_CHI128_BASE_IC=${CPU_N16_CHI128_BASE_IC:-""}
CPU_N16_CHI256_BASE_IC=${CPU_N16_CHI256_BASE_IC:-""}
CPU_N20_CHI128_BASE_IC=${CPU_N20_CHI128_BASE_IC:-""}
CPU_N20_CHI256_BASE_IC=${CPU_N20_CHI256_BASE_IC:-""}
# Identity order, N = 8, chi 128 (the correctness case); K's runs
# f8-base-32 and f8-seq3-mpo-32:
CPU_N8_CHI128_BASE_ID="1.6 6.3 13.5 23.3 36.0 51.7 69.5 89.3 110.1 132.0 155.7 180.2 204.5 229.6 259.0 290.1 320.5 349.8 377.7 405.5 434.2 462.1 490.3 518.3 546.3 574.4 602.5 630.3 658.1 685.9 713.4 741.7"
CPU_N8_CHI128_SEQ3_ID="0.5 1.9 4.2 7.3 11.4 16.1 21.8 28.1 35.0 42.6 51.1 60.0 69.1 78.9 89.0 99.3 109.5 120.1 131.0 142.3 154.1 165.8 177.8 189.5 201.3 213.2 225.1 238.5 252.5 266.7 281.2 295.5"
# The State API engine on the A100, base, identity order, s/step (the
# user's runs): N = 8 chi 128 27.9; N = 16 chi 64, 128, 256: 176.8, 283.1,
# 377.2 (overhead-bound: it grows slowly with chi).
STATE_API_N8_CHI128_ID=27.9
STATE_API_N16_ID_CHI64=176.8
STATE_API_N16_ID_CHI128=283.1
STATE_API_N16_ID_CHI256=377.2
# One H2 shot on one CPU core (L, polyene-combined): default 84.8 s,
# seq3-mpo 12.6 s; on the A100's State API engine (2026-11-12): engine
# routing 1,102 s, tuned routing 610 s.
CPU_H2_BASE=84.8
CPU_H2_SEQ3=12.6
# The full program, N = 16, S = 64, Merge + seq3-mpo in interleave-centre,
# one CPU core (L): chi 64 1,000.4 s; chi 128 is a SLOT.
CPU_FULL_N16_CHI64=${CPU_FULL_N16_CHI64:-1000.4}
CPU_FULL_N16_CHI128=${CPU_FULL_N16_CHI128:-""}

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
  echo "repo=$REPO out=$OUT parts=$PARTS engines=$ENGINES variants=$VARIANTS"
  echo "budget_seconds=$BUDGET_SECONDS limit_percent=$LIMIT_PERCENT timing_steps=$TIMING_STEPS full_shots=$FULL_SHOTS"
  git -C "$REPO" log --oneline -1
  nvidia-smi
  rustc --version
  cargo --version
  sha256sum "$H2" "$PPP16" "$PPP16_PARAMS"
  sha256sum "$REPO"/source/cutensornet/examples/polyene_qpe_prefix/ppp*
  env | grep -E '^(QDK_|CUDA|CARGO|CPU_)'
} > "$OUT/environment.txt" 2>&1
echo "H2's sha256 must be a8a2b39cdb9df504f3142c6d1cff34097438d0da0add5049979878a45189ec5f,"
echo "PPP16's 47bffc18c82abf68e4329427bd58596ffd7412791bc66747ba8cb116a1a5c2a3,"
echo "PPP16_PARAMS' b1b0f081409e1714a433e2559a87aeb38586fcd14ff6e24ee42b0e0705cc386e:"
for file in "$H2" "$PPP16" "$PPP16_PARAMS"; do
  grep -E "^[0-9a-f]{64}  $file\$" "$OUT/environment.txt" || echo "  (not found: $file)"
done

# The MPS daemon off: every run is one process alone on the GPU.
pgrep -a nvidia-cuda-mps || true
echo quit | nvidia-cuda-mps-control 2>/dev/null || true

# --- build (release) ---------------------------------------------------------
if wanted build; then
  run_part "$(box 3600)" "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_cutensornet --example polyene_qpe_prefix --example gate_split_bench
  build_cutensornet=$STATUS
  run_part "$(box 3600)" "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_simulators --example h2_dev --example polyene_shots_dev
  build_simulators=$STATUS
  run_part "$(box 3600)" "$OUT/build.log" cargo test --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_cutensornet --test cuda_mps_on_the_gpu --no-run
  build_kit=$STATUS
  summary build "$build_cutensornet,$build_simulators,$build_kit" "$SECONDS" \
    "polyene_qpe_prefix+gate_split_bench=$build_cutensornet h2_dev+polyene_shots_dev=$build_simulators kit=$build_kit"
fi

# --- kit: CudaMpsEngine's self-checks, conformance kit and MPO path -----------
if wanted kit; then
  log=$OUT/kit.log
  run_part "$(box 900)" "$log" cargo test --manifest-path "$REPO/Cargo.toml" --release \
    $CARGO_FLAGS -p qdk_cutensornet --test cuda_mps_on_the_gpu -- --ignored --nocapture --test-threads=1
  checks=$(grep -c '^SELF_CHECK ' "$log")
  failed=$(grep -c '^SELF_CHECK .* passed=false' "$log")
  failed_names=$(grep '^SELF_CHECK .* passed=false' "$log" | awk '{printf "%s%s", sep, $2; sep = ","}')
  failed_tests=$(grep -E '^test .* FAILED$' "$log" | awk '{printf "%s%s", sep, $2; sep = ","}')
  result=$(grep '^test result: ' "$log" | tail -1)
  tests_passed=$(printf '%s\n' "$result" | sed -nE 's/.* ([0-9]+) passed.*/\1/p')
  tests_failed=$(printf '%s\n' "$result" | sed -nE 's/.* ([0-9]+) failed.*/\1/p')
  verdict=fail
  if [ "$STATUS" = 0 ] && [ "$checks" -gt 0 ] && [ "$failed" = 0 ]; then
    verdict=pass
  fi
  summary kit "$STATUS" "$SECONDS_TAKEN" \
    "verdict=$verdict engine=cuda-mps tests_passed=${tests_passed:--} tests_failed=${tests_failed:--}${failed_tests:+ ($failed_tests)} self_checks=$checks self_checks_failed=$failed${failed_names:+ ($failed_names)} (see kit.log)"
fi

# --- svd-bench: gate_split_bench's SVD algorithms -----------------------------
if wanted svd-bench; then
  log=$OUT/svd-bench.log
  run_part "$(box 900)" "$log" "$BIN/gate_split_bench" --engine cutensornet \
    --bonds 8,16,24,64,128,256,512 --gate-split-algos reduced,direct
  best=$(grep '^BEST ' "$log" | awk '{printf "%s%s", sep, $0; sep = " | "}')
  summary svd-bench "$STATUS" "$SECONDS_TAKEN" \
    "gatesplit_ok=$(grep -c '^GATESPLIT status=ok' "$log") disagrees=$(grep -c 'status=disagrees' "$log") refused=$(grep -c 'status=refused' "$log") best: ${best:-none} (see svd-bench.log)"
fi

# --- The runs ----------------------------------------------------------------
# The variant's flags for polyene_qpe_prefix, h2_dev and polyene_shots_dev.
variant_flags() {
  case "$1" in
  seq3-mpo) echo "--variant seq3-mpo" ;;
  *) echo "" ;;
  esac
}

# One polyene prefix run: name $1, engine $2, variant $3, order $4, N $5,
# chi $6, limit $7, steps $8 (empty: the program's 32), reference $9 (empty:
# none). Sets P_VERDICT (AgreesWithExact, Disagrees, done, timed-out,
# error, skipped), P_STEPS, P_SECONDS (to the last step reached), P_LAST
# (the last step's seconds) and P_DETAIL.
polyene_run() {
  local name=$1 engine=$2 variant=$3 order=$4 n=$5 chi=$6 limit=$7 steps=$8 reference=$9
  local log=$OUT/$name.log
  # shellcheck disable=SC2046
  run_part "$limit" "$log" "$BIN/polyene_qpe_prefix" --engine "$engine" --n "$n" --chi "$chi" \
    --order "$order" $(variant_flags "$variant") ${steps:+--steps "$steps"} \
    ${reference:+--reference "$reference"}
  local done_line steps_lines step check counters error
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
    P_DETAIL="$P_DETAIL splits=$(field splits "$counters") centre_moves=$(field centre_moves "$counters")"
    P_DETAIL="$P_DETAIL gate_split_seconds=$(field gate_split_seconds "$counters")"
    P_DETAIL="$P_DETAIL mpo_operators=$(field mpo_operators "$counters") mpo_seconds=$(field mpo_seconds "$counters")"
    P_DETAIL="$P_DETAIL mpo_host_seconds=$(field mpo_host_seconds "$counters")"
  fi
  [ -n "$check" ] && P_DETAIL="$P_DETAIL largest_gap=$(field largest "$check")"
  [ -n "$error" ] && [ "$P_VERDICT" = error ] && P_DETAIL="$P_DETAIL error=\"$error\""
}

# One H2 shot: name $1, engine $2, variant $3, limit $4. Sets H_VERDICT
# (pass, fail, timed-out, skipped), H_SECONDS and H_DETAIL.
h2_run() {
  local name=$1 engine=$2 variant=$3 limit=$4
  local log=$OUT/$name.log
  # shellcheck disable=SC2046
  run_part "$limit" "$log" "$BIN/h2_dev" --engine "$engine" --bytecode "$H2" --readout "$TABLE" \
    --seed 1 --chi 128 --first-shot 0 --shots 1 $(variant_flags "$variant")
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
  elif [ "$STATUS" = 124 ] || [ "$STATUS" = 137 ]; then
    H_VERDICT=timed-out
    H_DETAIL="no SHOT line before the limit"
  else
    H_VERDICT=fail
    H_DETAIL="no SHOT line, error=\"$(grep -E 'panicked|error' "$log" | tail -1 | cut -c1-200)\""
  fi
}

# Each engine x variant's correctness: pass, fail or unverified (unset:
# not checked).
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
# The H2 shots' seconds, kept for the h2 part.
declare -A H2_SECONDS H2_VERDICT
if wanted correctness; then
  for engine in $ENGINES; do
    for variant in $VARIANTS; do
      key=$engine/$variant
      case "$engine/$variant" in
      cuda-mps/base) want=1800 ;;
      *) want=1200 ;;
      esac
      polyene_run "correctness-n8-$engine-$variant" "$engine" "$variant" identity 8 128 \
        "$(box $want)" "" ppp8.exact.reference
      case "$P_VERDICT" in
      AgreesWithExact) mark "$key" pass ;;
      timed-out | skipped) mark "$key" unverified ;;
      *) mark "$key" fail ;;
      esac
      if [ "$variant" = seq3-mpo ]; then cpu=$CPU_N8_CHI128_SEQ3_ID; else cpu=$CPU_N8_CHI128_BASE_ID; fi
      cpu_at=$(at_step "$cpu" "$P_STEPS")
      line="verdict=$([ "$P_VERDICT" = AgreesWithExact ] && echo pass || echo "fail-or-unverified ($P_VERDICT)")"
      line="$line step=correctness-n8 engine=$engine variant=$variant order=identity n=8 chi=128"
      line="$line steps_done=$P_STEPS gpu_seconds=$P_SECONDS cpu_seconds_same_step=$cpu_at"
      line="$line gpu_over_cpu=$(ratio "$P_SECONDS" "$cpu_at")"
      line="$line state_api_base_s_per_step=$STATE_API_N8_CHI128_ID $P_DETAIL"
      summary "correctness-n8-$engine-$variant" "$STATUS" "$SECONDS_TAKEN" "$line"
    done
  done
  for engine in $ENGINES; do
    for variant in $VARIANTS; do
      key=$engine/$variant
      case "$engine/$variant" in
      cutensornet/base) want=1500 ;;
      cuda-mps/base) want=1200 ;;
      *) want=900 ;;
      esac
      h2_run "correctness-h2-$engine-$variant" "$engine" "$variant" "$(box $want)"
      H2_SECONDS[$key]=$H_SECONDS
      H2_VERDICT[$key]=$H_VERDICT
      case "$H_VERDICT" in
      pass) mark "$key" pass ;;
      timed-out | skipped) mark "$key" unverified ;;
      *) mark "$key" fail ;;
      esac
      summary "correctness-h2-$engine-$variant" "$STATUS" "$SECONDS_TAKEN" \
        "verdict=$H_VERDICT step=correctness-h2 engine=$engine variant=$variant shot_seconds=$H_SECONDS $H_DETAIL"
    done
  done
  for engine in $ENGINES; do
    for variant in $VARIANTS; do
      key=$engine/$variant
      summary "correctness-$engine-$variant" - - \
        "verdict=${CORRECT[$key]} (polyene N = 8 identity against the exact state, and one H2 shot; fail: skipped below; unverified: runs below, marked)"
    done
  done
fi

# Whether engine/variant $1 may be timed; prints why not otherwise.
may_time() {
  case "${CORRECT[$1]:-not-checked}" in
  fail)
    echo "skipped: $1 failed the correctness part"
    return 1
    ;;
  esac
  return 0
}

# --- timing: the polyene prefix in interleave-centre --------------------------
# The CPU timeline at N $1, chi $2, variant $3 (interleave-centre).
cpu_timeline() {
  case "$1/$2/$3" in
  16/64/seq3-mpo) echo "$CPU_N16_CHI64_SEQ3_IC" ;;
  16/128/seq3-mpo) echo "$CPU_N16_CHI128_SEQ3_IC" ;;
  16/256/seq3-mpo) echo "$CPU_N16_CHI256_SEQ3_IC" ;;
  20/128/seq3-mpo) echo "$CPU_N20_CHI128_SEQ3_IC" ;;
  20/256/seq3-mpo) echo "$CPU_N20_CHI256_SEQ3_IC" ;;
  16/64/base) echo "$CPU_N16_CHI64_BASE_IC" ;;
  16/128/base) echo "$CPU_N16_CHI128_BASE_IC" ;;
  16/256/base) echo "$CPU_N16_CHI256_BASE_IC" ;;
  20/128/base) echo "$CPU_N20_CHI128_BASE_IC" ;;
  20/256/base) echo "$CPU_N20_CHI256_BASE_IC" ;;
  esac
}

# The State API's s/step at N $1 chi $2, base, identity order, or "-".
state_api_identity() {
  case "$1/$2" in
  16/64) echo "$STATE_API_N16_ID_CHI64" ;;
  16/128) echo "$STATE_API_N16_ID_CHI128" ;;
  16/256) echo "$STATE_API_N16_ID_CHI256" ;;
  *) echo - ;;
  esac
}

# Timing at N = 16 chi 64 seq3-mpo, per engine: "steps seconds".
declare -A FASTEST_INPUT
if wanted timing; then
  for variant in seq3-mpo base; do
    case " $VARIANTS " in *" $variant "*) ;; *) continue ;; esac
    for case_ in "16 64" "16 128" "16 256" "20 128" "20 256"; do
      read -r n chi <<< "$case_"
      for engine in $ENGINES; do
        key=$engine/$variant
        name=timing-n$n-chi$chi-$engine-$variant
        if ! why=$(may_time "$key"); then
          summary "$name" skipped 0 "verdict=$why"
          continue
        fi
        if [ "$variant" = seq3-mpo ]; then
          case "$n/$chi" in
          16/64) want=480 ;;
          16/256 | 20/256) want=720 ;;
          *) want=600 ;;
          esac
        else
          want=180
        fi
        polyene_run "$name" "$engine" "$variant" interleave-centre "$n" "$chi" "$(box $want)" \
          "$TIMING_STEPS" ""
        cpu_at=$(at_step "$(cpu_timeline "$n" "$chi" "$variant")" "$P_STEPS")
        line="verdict=$P_VERDICT correctness=${CORRECT[$key]:-not-checked}"
        line="$line engine=$engine variant=$variant order=interleave-centre n=$n chi=$chi"
        line="$line steps_done=$P_STEPS gpu_seconds=$P_SECONDS last_step_seconds=$P_LAST"
        line="$line cpu_seconds_same_step=$cpu_at gpu_over_cpu=$(ratio "$P_SECONDS" "$cpu_at")"
        if [ "$engine" = cutensornet ] && [ "$variant" = base ]; then
          line="$line state_api_identity_base_s_per_step=$(state_api_identity "$n" "$chi")"
        fi
        summary "$name" "$STATUS" "$SECONDS_TAKEN" "$line $P_DETAIL"
        if [ "$n/$chi/$variant" = 16/64/seq3-mpo ] && [ "$P_STEPS" -gt 0 ]; then
          FASTEST_INPUT[$engine]="$P_STEPS $P_SECONDS"
        fi
      done
    done
  done
fi

# --- h2: one shot per engine x variant, against the CPU -----------------------
if wanted h2; then
  for engine in $ENGINES; do
    for variant in $VARIANTS; do
      key=$engine/$variant
      name=h2-$engine-$variant
      if [ -n "${H2_VERDICT[$key]:-}" ]; then
        verdict=${H2_VERDICT[$key]}
        seconds=${H2_SECONDS[$key]}
        exit_=from-correctness
        taken=0
      elif ! why=$(may_time "$key"); then
        summary "$name" skipped 0 "verdict=$why"
        continue
      else
        h2_run "$name" "$engine" "$variant" "$(box 1500)"
        verdict=$H_VERDICT
        seconds=$H_SECONDS
        exit_=$STATUS
        taken=$SECONDS_TAKEN
      fi
      if [ "$variant" = seq3-mpo ]; then cpu=$CPU_H2_SEQ3; else cpu=$CPU_H2_BASE; fi
      summary "$name" "$exit_" "$taken" \
        "verdict=$verdict engine=$engine variant=$variant shot_seconds=$seconds cpu_seconds=$cpu gpu_over_cpu=$(ratio "$seconds" "$cpu") state_api_2026_11_12=engine-routing:1102,tuned:610"
    done
  done
fi

# --- full: N = 16, S = 64, Merge + seq3-mpo, on the faster engine -------------
if wanted full; then
  # The faster engine at N = 16 chi 64 seq3-mpo: more steps reached, then
  # fewer seconds; cutensornet when timing did not run.
  full_engine=
  best_steps=0
  best_seconds=
  for engine in $ENGINES; do
    [ "${CORRECT[$engine/seq3-mpo]:-not-checked}" = fail ] && continue
    read -r steps seconds <<< "${FASTEST_INPUT[$engine]:-0 -}"
    if [ -z "$full_engine" ] || [ "$steps" -gt "$best_steps" ] ||
      { [ "$steps" = "$best_steps" ] && awk -v a="$seconds" -v b="$best_seconds" \
        'BEGIN {exit !(a != "-" && (b == "-" || a + 0 < b + 0))}'; }; then
      full_engine=$engine
      best_steps=$steps
      best_seconds=$seconds
    fi
  done
  if [ -z "$full_engine" ]; then
    summary full skipped 0 "verdict=skipped: no engine passed the correctness part with seq3-mpo"
  elif [ ! -e "$PPP16" ] || [ ! -e "$PPP16_PARAMS" ]; then
    summary full skipped 0 "verdict=skipped: $PPP16 or $PPP16_PARAMS is missing (copy them to the VM)"
  else
    for chi in 64 128; do
      name=full-n16-chi$chi-$full_engine
      log=$OUT/$name.log
      run_part "$(box 1500)" "$log" "$BIN/polyene_shots_dev" --bytecode "$PPP16" \
        --params "$PPP16_PARAMS" --engine "$full_engine" --chi "$chi" --shots "$FULL_SHOTS" \
        --seed 1 --reset merge --variant seq3-mpo --order interleave-centre
      result=$(grep '^SUMMARY ' "$log" | tail -1)
      if [ "$chi" = 64 ]; then cpu=$CPU_FULL_N16_CHI64; else cpu=$CPU_FULL_N16_CHI128; fi
      if [ -n "$result" ]; then
        seconds=$(field seconds "$result")
        summary "$name" "$STATUS" "$SECONDS_TAKEN" \
          "verdict=done engine=$full_engine chi=$chi shots=$FULL_SHOTS seconds=$seconds cpu_seconds=${cpu:--} gpu_over_cpu=$(ratio "$seconds" "${cpu:--}") controlled_steps=$(field controlled_steps "$result") least_weight=$(field least_weight "$result") largest_bond=$(field largest_bond "$result") mode_k=$(field mode_k "$result") hist=$(field hist "$result") (CPU chi 64: 156 steps, 1 - W 2e-6, mode_k 12)"
      else
        verdict=error
        { [ "$STATUS" = 124 ] || [ "$STATUS" = 137 ]; } && verdict=timed-out
        [ "$STATUS" = skipped ] && verdict=skipped
        summary "$name" "$STATUS" "$SECONDS_TAKEN" \
          "verdict=$verdict engine=$full_engine chi=$chi step=full no SUMMARY line, shots printed: $(grep -c '^SHOT ' "$log") error=\"$(grep -E 'FAILED|panicked' "$log" | tail -1 | cut -c1-200)\""
      fi
    done
  fi
fi

# --- What to send back ---------------------------------------------------------
tar czf "$OUT.tar.gz" --exclude='*.tar.gz' -C "$(dirname "$OUT")" "$(basename "$OUT")"
echo "Send back: $OUT.tar.gz"
echo "== Summary ($SECONDS s)"
cat "$SUMMARY"
