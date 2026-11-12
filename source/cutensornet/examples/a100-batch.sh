#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# Development only (not for review): one unattended GPU session that
# measures what decides the next steps of the GPU speed plan. Three parts,
# each with its own log and one line in summary.txt; a part that fails does
# not stop the next:
#
#   1a     one H2 shot on cuTensorNet with the tuned routing and restore at
#          batch end (rb); pass: records=01010, draws=1414, tvd <= 1e-9; its
#          time against the engine routing's 1,102 s.
#   tuning the tuning bench (tuning_bench): 300 draws of H2's pattern,
#          recorded here on the CPU engine and replayed on cuTensorNet, per
#          variant (T1 adjacent/distant, T2 SVD algorithms, T3a hyper-samples,
#          T3b canonical-centre read, T4 pooled buffers).
#   stage0 the gate-split bench (gate_split_bench): GateSplit, TensorQR and
#          cuSOLVER's batched SVDs by bond and batch size; then, in its own
#          log, bonds 256 and 512.
#
# Run from anywhere, on the branch's checkout:
#
#   bash source/cutensornet/examples/a100-batch.sh
#
# Environment (all optional):
#   OUT        the output directory (default $HOME/a100-batch-<date>)
#   H2         H2's bytecode JSON (default /data/domingom/h2.bytecode.json)
#   ENGINE     cutensornet (default) or cpu: cpu dry-runs the plumbing
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY, QDK_CUSOLVER_LIBRARY
#              the libraries, when not at the pinned paths below
#   CARGO_FLAGS extra flags for cargo build (for instance --offline)
#   EXTRA_BONDS the large bonds of stage 0 (default 256,512; empty: none)
#
# The parts' time limits (timeout) keep the session within about 2 h.

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
ENGINE=${ENGINE:-cutensornet}
H2=${H2:-/data/domingom/h2.bytecode.json}
TABLE=$REPO/source/simulators/examples/h2-readout-distribution.txt
OUT=${OUT:-$HOME/a100-batch-$(date +%Y%m%d-%H%M)}
EXTRA_BONDS=${EXTRA_BONDS-256,512}
CARGO_FLAGS=${CARGO_FLAGS:-}
BIN=${CARGO_TARGET_DIR:-$REPO/target}/release/examples
SUMMARY=$OUT/summary.txt

# The pinned paths of the acceptance run; cuSOLVER sits beside the CUDA
# Runtime.
CUDA_LIB=/usr/local/cuda-12.9/targets/x86_64-linux/lib
if [ "$ENGINE" = cutensornet ]; then
  export QDK_CUTENSORNET_LIBRARY=${QDK_CUTENSORNET_LIBRARY:-/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2}
  export QDK_CUDART_LIBRARY=${QDK_CUDART_LIBRARY:-$CUDA_LIB/libcudart.so.12}
  if [ -z "${QDK_CUSOLVER_LIBRARY:-}" ]; then
    for candidate in "$CUDA_LIB/libcusolver.so.11" \
      "$(ldconfig -p 2>/dev/null | awk '/libcusolver\.so\.11 /{print $NF; exit}')"; do
      if [ -n "$candidate" ] && [ -e "$candidate" ]; then
        export QDK_CUSOLVER_LIBRARY=$candidate
        break
      fi
    done
  fi
fi

mkdir -p "$OUT/traces"
: > "$SUMMARY"

# One line in summary.txt: the part, its exit code (124: timed out), its
# seconds and its verdict.
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

{
  date
  echo "repo=$REPO engine=$ENGINE out=$OUT"
  git -C "$REPO" log --oneline -1
  nvidia-smi
  rustc --version
  cargo --version
  sha256sum "$H2"
  env | grep -E '^(QDK_|CUDA|CARGO)'
} > "$OUT/environment.txt" 2>&1
echo "H2's sha256 must be a8a2b39cdb9df504f3142c6d1cff34097438d0da0add5049979878a45189ec5f:"
grep -E '^[0-9a-f]{64} ' "$OUT/environment.txt" || echo "  (H2 not found: $H2)"

# The MPS daemon off: every part runs one process alone on the GPU.
if [ "$ENGINE" = cutensornet ]; then
  pgrep -a nvidia-cuda-mps || true
  echo quit | nvidia-cuda-mps-control 2>/dev/null || true
fi

# --- Build (release) ---------------------------------------------------------
run_part 3600 "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" --release \
  $CARGO_FLAGS -p qdk_simulators --example h2_dev
build_simulators=$STATUS
run_part 3600 "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" --release \
  $CARGO_FLAGS -p qdk_cutensornet --example tuning_bench --example gate_split_bench
build_cutensornet=$STATUS
summary build "$build_simulators,$build_cutensornet" "-" "h2_dev=$build_simulators benches=$build_cutensornet"

# --- 1a: one rb shot -----------------------------------------------------------
log=$OUT/1a-rb-shot.log
run_part 3000 "$log" "$BIN/h2_dev" --engine "$ENGINE" --bytecode "$H2" --readout "$TABLE" \
  --seed 1 --chi 128 --first-shot 0 --shots 1 --routing tuned --restore batch
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
  ratio=$(awk -v s="$seconds" 'BEGIN {printf "%.3f", s / 1102}')
  summary 1a "$STATUS" "$SECONDS_TAKEN" \
    "verdict=$verdict records=$records draws=$draws tvd=$tvd shot_seconds=$seconds vs_engine_1102s=$ratio"
else
  summary 1a "$STATUS" "$SECONDS_TAKEN" "verdict=fail no SHOT line (see 1a-rb-shot.log)"
fi

# --- 2: the tuning bench -------------------------------------------------------
# The traces are recorded on the CPU engine (deterministic, about 2 min):
# adjacent operators (the tuned routing), distant ones (the engine routing).
log=$OUT/2-tuning.log
run_part 1200 "$OUT/2-record-adjacent.log" "$BIN/h2_dev" --engine cpu --bytecode "$H2" \
  --seed 1 --chi 128 --first-shot 0 --shots 1 --routing tuned --restore batch \
  --record-calls "$OUT/traces/adjacent.calls" --record-draws 300
record_adjacent=$STATUS
run_part 1200 "$OUT/2-record-distant.log" "$BIN/h2_dev" --engine cpu --bytecode "$H2" \
  --seed 1 --chi 128 --first-shot 0 --shots 1 --routing engine \
  --record-calls "$OUT/traces/distant.calls" --record-draws 300
record_distant=$STATUS
if [ "$record_adjacent" = 0 ] && [ "$record_distant" = 0 ]; then
  run_part 2700 "$log" "$BIN/tuning_bench" --engine "$ENGINE" \
    --trace-adjacent "$OUT/traces/adjacent.calls" --trace-distant "$OUT/traces/distant.calls"
  done_line=$(grep -m1 '^TUNING done' "$log" || echo "no TUNING done line")
  summary tuning "$STATUS" "$SECONDS_TAKEN" \
    "variants=$(grep -c '^VARIANT' "$log") ratios=$(grep -c '^RATIO' "$log") ${done_line#TUNING done }"
else
  summary tuning "record:$record_adjacent,$record_distant" "-" "verdict=fail the traces were not recorded"
fi

# --- 3: stage 0, the gate-split bench ------------------------------------------
log=$OUT/3-stage0.log
run_part 2400 "$log" "$BIN/gate_split_bench" --engine "$ENGINE"
summary stage0 "$STATUS" "$SECONDS_TAKEN" \
  "gatesplit_ok=$(grep -c '^GATESPLIT status=ok' "$log") disagrees=$(grep -c 'status=disagrees' "$log") refused=$(grep -c 'status=refused' "$log") $(grep -m1 '^DECISION' "$log" || echo 'no DECISION line')"
if [ -n "$EXTRA_BONDS" ]; then
  log=$OUT/3b-stage0-large.log
  run_part 1500 "$log" "$BIN/gate_split_bench" --engine "$ENGINE" --bonds "$EXTRA_BONDS"
  summary stage0-large "$STATUS" "$SECONDS_TAKEN" \
    "bonds=$EXTRA_BONDS gatesplit_ok=$(grep -c '^GATESPLIT status=ok' "$log") disagrees=$(grep -c 'status=disagrees' "$log") refused=$(grep -c 'status=refused' "$log")"
fi

# --- What to send back ------------------------------------------------------------
tar czf "$OUT.tar.gz" --exclude='*.tar.gz' -C "$(dirname "$OUT")" "$(basename "$OUT")"
echo "Send back: $OUT.tar.gz"
cat "$SUMMARY"
