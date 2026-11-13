#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# Development only (not for review): the CUDA MPS engine (CudaMpsEngine,
# qdk_cpu_mps's algorithm on cuTensorNet's tensor API) on the A100, against
# the State API engine (CuTensorNetEngine) and one CPU core. Each part has
# its own log and one line in summary.txt; a part that fails or times out
# does not stop the next, and a part stopped by its limit still logs every
# step it reached, with the engine's counters after each step.
#
# The parts, with ESTIMATED durations (not measured: the calls per step
# were counted on a host stand-in of the tensor calls, since removed:
# splits plus centre moves; the time
# per call from stage 0's GateSplit at b = 24, 64, 128 and 256: 4.4, 12.3,
# 26.1 and 64.7 ms):
#
#   build       release builds: polyene_qpe_prefix, h2_dev and the kit's
#               test binary; about 20 min on a fresh target dir.
#   kit         the GPU tests (tests/cuda_mps_on_the_gpu.rs): the GateSplit
#               self-checks (F2: the first value kept; F8: the U/V layout
#               at a reduced extent; the discarded weight; ties,
#               informational), the conformance kit on the real library,
#               and agreement with CpuMpsEngine to 1e-10 on random 7-site
#               circuits (distant gates, a chi 3 cap, projections, draws'
#               conditionals); pass: every test passes and no SELF_CHECK
#               line says passed=false. Estimate: 1-3 min.
#   n8-chi128   polyene N = 8 at chi 128 against the exact readings; pass:
#               verdict=AgreesWithExact. Time per step against 27.9 s (State
#               API) and 25.3 s (the CPU's 808 s over 32 steps). Estimate:
#               6.8k calls per step at 5-12 ms, 35-80 s per step, 20-45 min.
#   n12-chi64   N = 12 at chi 64 against the CPU engine's readings at chi
#               64; time per step against 83 s (State API) and 89 s (CPU).
#               Estimate: 16k calls per step at 9-12 ms, 150-200 s per
#               step: the limit stops it after about 10 steps.
#   n16-chi64   N = 16 at chi 64; time per step (CPU: about 295 s, loaded).
#               Estimate: 30k calls per step, 300-360 s per step.
#   n16-chi128  N = 16 at chi 128; estimate 350-770 s per step.
#   n16-chi256  N = 16 at chi 256; estimate 350-1,900 s per step, so the
#               limit may stop it before its first step: its counters then
#               are not printed, and its log's last line is the trial's.
#   h2          one H2 shot, as a100-batch.sh's part 1a (tuned routing,
#               restore at batch end); pass: records=01010, draws=1414,
#               tvd <= 1e-9; its time against 610 s (the State API engine).
#               Estimate: about 165k splits at 2-4 ms plus centre moves,
#               7-15 min.
#
# Run from anywhere, on the branch's checkout:
#
#   bash source/cutensornet/examples/cuda-mps-dev.sh
#
# Environment (all optional):
#   OUT        the output directory (default $HOME/cuda-mps-dev-<date>)
#   PARTS      the parts to run (default: all, in the order above), for
#              instance "kit n8-chi128"
#   STEPS      fewer controlled steps than the polyene program's 32, for a
#              quick dry run (the references are then not checked)
#   LIMIT_PERCENT the parts' time limits in percent (default 100)
#   H2         H2's bytecode JSON (default /data/domingom/h2.bytecode.json)
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY, QDK_CUSOLVER_LIBRARY
#              the libraries, when neither at the pinned paths below nor in
#              /data/domingom/tn-venv
#   CARGO_FLAGS extra flags for cargo build (for instance --offline)
#
# The parts' time limits (timeout) keep the session within about 4 h.

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
ENGINE=cuda-mps
OUT=${OUT:-$HOME/cuda-mps-dev-$(date +%Y%m%d-%H%M)}
PARTS=${PARTS:-kit n8-chi128 n12-chi64 n16-chi64 n16-chi128 n16-chi256 h2}
STEPS=${STEPS:-}
LIMIT_PERCENT=${LIMIT_PERCENT:-100}
H2=${H2:-/data/domingom/h2.bytecode.json}
TABLE=$REPO/source/simulators/examples/h2-readout-distribution.txt
TN_VENV=/data/domingom/tn-venv
CARGO_FLAGS=${CARGO_FLAGS:-}
BIN=${CARGO_TARGET_DIR:-$REPO/target}/release/examples
SUMMARY=$OUT/summary.txt

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

# One line in summary.txt: the part, its exit code (124: timed out; 2: a
# reading disagrees with the reference), its seconds and its verdict.
summary() {
  printf 'part=%s exit=%s seconds=%s %s\n' "$1" "$2" "$3" "$4" | tee -a "$SUMMARY"
}

# Runs "$@" under a time limit of $1 seconds, its output in $2; sets STATUS
# and SECONDS_TAKEN.
run_part() {
  local limit=$1 log=$2
  shift 2
  local start=$SECONDS
  echo "== $(date '+%F %T') $*" | tee -a "$log"
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

{
  date
  echo "repo=$REPO engine=$ENGINE out=$OUT parts=$PARTS steps=${STEPS:-32} limit_percent=$LIMIT_PERCENT"
  git -C "$REPO" log --oneline -1
  nvidia-smi
  rustc --version
  cargo --version
  sha256sum "$H2"
  sha256sum "$REPO"/source/cutensornet/examples/polyene_qpe_prefix/ppp*
  env | grep -E '^(QDK_|CUDA|CARGO)'
} > "$OUT/environment.txt" 2>&1
echo "H2's sha256 must be a8a2b39cdb9df504f3142c6d1cff34097438d0da0add5049979878a45189ec5f:"
grep -E "^[0-9a-f]{64}  $H2\$" "$OUT/environment.txt" || echo "  (H2 not found: $H2)"

# The MPS daemon off: every part runs one process alone on the GPU.
pgrep -a nvidia-cuda-mps || true
echo quit | nvidia-cuda-mps-control 2>/dev/null || true

# --- Build (release) ---------------------------------------------------------
run_part 3600 "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" --release \
  $CARGO_FLAGS -p qdk_cutensornet --example polyene_qpe_prefix
build_polyene=$STATUS
run_part 3600 "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" --release \
  $CARGO_FLAGS -p qdk_simulators --example h2_dev
build_h2=$STATUS
run_part 3600 "$OUT/build.log" cargo test --manifest-path "$REPO/Cargo.toml" --release \
  $CARGO_FLAGS -p qdk_cutensornet --test cuda_mps_on_the_gpu --no-run
build_kit=$STATUS
summary build "$build_polyene,$build_h2,$build_kit" "-" \
  "polyene_qpe_prefix=$build_polyene h2_dev=$build_h2 kit=$build_kit"

# --- kit: the self-checks, the conformance kit and agreement with the CPU -----------
if wanted kit; then
  log=$OUT/kit.log
  run_part $((900 * LIMIT_PERCENT / 100)) "$log" cargo test --manifest-path "$REPO/Cargo.toml" \
    --release $CARGO_FLAGS -p qdk_cutensornet --test cuda_mps_on_the_gpu -- --ignored --nocapture --test-threads=1
  checks=$(grep -c '^SELF_CHECK ' "$log")
  failed=$(grep -c '^SELF_CHECK .* passed=false' "$log")
  failed_names=$(grep '^SELF_CHECK .* passed=false' "$log" | awk '{printf "%s%s", sep, $2; sep = ","}')
  ties=$(grep '^SELF_CHECK .*-tie-' "$log" | awk '{printf "%s%s:%s", sep, $2, $4; sep = ","}')
  result=$(grep '^test result: ' "$log" | tail -1)
  tests_passed=$(printf '%s\n' "$result" | sed -nE 's/.* ([0-9]+) passed.*/\1/p')
  tests_failed=$(printf '%s\n' "$result" | sed -nE 's/.* ([0-9]+) failed.*/\1/p')
  verdict=fail
  if [ "$STATUS" = 0 ] && [ "$checks" -gt 0 ] && [ "$failed" = 0 ]; then
    verdict=pass
  fi
  summary kit "$STATUS" "$SECONDS_TAKEN" \
    "verdict=$verdict tests_passed=${tests_passed:--} tests_failed=${tests_failed:--} self_checks=$checks self_checks_failed=$failed${failed_names:+ ($failed_names)} ties=${ties:-none} (see kit.log)"
fi

# --- Polyene ---------------------------------------------------------------------
# One part: N ($2) at chi ($3) within $4 seconds, checked against the
# reference file $5 (in examples/polyene_qpe_prefix/, none if empty); the
# time per step against the State API engine's $6 s and the CPU's $7 s.
polyene_part() {
  local part=$1 n=$2 chi=$3 limit=$(($4 * LIMIT_PERCENT / 100)) reference=$5
  local state_api=$6 cpu=$7
  wanted "$part" || return
  local log=$OUT/$part.log
  run_part "$limit" "$log" "$BIN/polyene_qpe_prefix" --engine "$ENGINE" --n "$n" --chi "$chi" \
    ${reference:+--reference "$reference"} ${STEPS:+--steps "$STEPS"}
  local done_line step check counters
  done_line=$(grep '^POLYENE done ' "$log" | tail -1)
  step=$(grep '^POLYENE step ' "$log" | tail -1)
  check=$(grep '^POLYENE check ' "$log" | tail -1)
  counters=$(grep '^POLYENE cuda_mps_counters ' "$log" | tail -1)
  local verdict=none
  if [ -n "$done_line" ]; then
    verdict=$(field verdict "$done_line")
  elif [ "$STATUS" = 124 ] || [ "$STATUS" = 137 ]; then
    verdict=timed-out
  else
    verdict=error
  fi
  local p seconds per_step
  p=$(field p "$step")
  seconds=$(field seconds "$step")
  per_step=$(awk -v s="${seconds:-0}" -v p="${p:-0}" 'BEGIN {if (p > 0) printf "%.1f", s / p; else printf "-"}')
  local line="verdict=$verdict steps_done=${p:-0} seconds_per_step=$per_step"
  if [ "$per_step" != - ]; then
    line="$line state_api_over_this=$(awk -v g="$per_step" -v c="$state_api" \
      'BEGIN {if (c > 0) printf "%.2f", c / g; else printf "-"}')"
    line="$line cpu_over_this=$(awk -v g="$per_step" -v c="$cpu" \
      'BEGIN {if (c > 0) printf "%.2f", c / g; else printf "-"}')"
    line="$line one_minus_w=$(field one_minus_w "$step") largest_bond=$(field largest_bond "$step")"
  fi
  if [ -n "$counters" ]; then
    line="$line splits=$(field splits "$counters") centre_moves=$(field centre_moves "$counters")"
    line="$line gate_split_seconds=$(field gate_split_seconds "$counters")"
  fi
  if [ -n "$check" ]; then
    line="$line largest_gap=$(field largest "$check")"
  fi
  summary "$part" "$STATUS" "$SECONDS_TAKEN" "$line"
}

# The seconds per step of the State API engine and of one CPU core: N = 8
# and N = 12 from the A100 runs, N = 16's CPU from the polyene quality
# study (9,441 s over 32 steps, loaded); "-" where none was measured.
polyene_part n8-chi128 8 128 3000 ppp8.exact.reference 27.9 25.3
polyene_part n12-chi64 12 64 1800 ppp12.cpu-chi64.reference 83 89
polyene_part n16-chi64 16 64 1200 "" - 295
polyene_part n16-chi128 16 128 1800 "" - -
polyene_part n16-chi256 16 256 1800 "" - -

# --- h2: one H2 shot, as a100-batch.sh's part 1a ---------------------------------
if wanted h2; then
  log=$OUT/h2.log
  run_part $((3000 * LIMIT_PERCENT / 100)) "$log" "$BIN/h2_dev" --engine "$ENGINE" \
    --bytecode "$H2" --readout "$TABLE" --seed 1 --chi 128 --first-shot 0 --shots 1 \
    --routing tuned --restore batch
  shot=$(grep -m1 '^SHOT ' "$log" || true)
  if [ -n "$shot" ]; then
    records=$(field records "$shot")
    draws=$(field draws "$shot")
    tvd=$(field tvd "$shot")
    seconds=$(field seconds "$shot")
    verdict=fail
    if [ "$records" = 01010 ] && [ "$draws" = 1414 ] &&
      awk -v t="$tvd" 'BEGIN {exit !(t != "" && t + 0 <= 1e-9)}'; then
      verdict=pass
    fi
    ratio=$(awk -v s="$seconds" 'BEGIN {printf "%.3f", s / 610}')
    summary h2 "$STATUS" "$SECONDS_TAKEN" \
      "verdict=$verdict records=$records draws=$draws tvd=$tvd shot_seconds=$seconds this_over_state_api_610s=$ratio"
  else
    summary h2 "$STATUS" "$SECONDS_TAKEN" "verdict=fail no SHOT line (see h2.log)"
  fi
fi

# --- What to send back ------------------------------------------------------------
tar czf "$OUT.tar.gz" --exclude='*.tar.gz' -C "$(dirname "$OUT")" "$(basename "$OUT")"
echo "Send back: $OUT.tar.gz"
echo "== Summary"
cat "$SUMMARY"
