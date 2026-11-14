#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# Development only (not for review): MPS state-preparation circuits with
# the structure of qdk-chemistry's `matrix_product_state` state preparation
# (polyene PPP ground states from DMRG, truncated to a bond χ; one isometry
# per orbital on a bond register, then the CZ sign layer) run on
# CudaMpsEngine with `mps_prep_check`, each checked against readings
# computed from its target MPS: ⟨H⟩, every qubit's P(One), and the
# Hamiltonian's Pauli strings. One line per run in summary.txt.
#
# The cases are not in the repository (tens of MB of circuits): copy the
# tarball made by the study's generator to the VM first, then
#
#   tar xzf mps-prep-cases.tar.gz -C $HOME
#   bash source/cutensornet/examples/gpu-mps-prep.sh
#
# Run it alone on the GPU: it refuses to start while another process uses
# the GPU, and stops any MPS daemon.
#
# Environment (all optional):
#   MPS_PREP_OUT the output directory (default
#                /data/domingom/runs/gpu-mps-prep-<date>); a shell's OUT
#                is ignored
#   CASES        the folder holding the case folders (default
#                $HOME/mps-prep-cases)
#   RUNS         space-separated case:placement pairs, in order (default
#                below: the χ ladder at N = 8, 16 and 20 with the register
#                following the active orbital, the cases named -ladder with
#                the CZ sign layer rewritten as a parity ladder (the same
#                unitary), two static placements for comparison, then
#                N = 20 χ = 64 with the CZ layer as emitted)
#   RUN_SECONDS  the harness's own time limit per run (default 5400); a
#                run past it prints what it reached and exits with 3
#   MPS_PREP_CHI_CAP
#                the engine's bond cap (default 1024); a split that needs
#                a larger bond truncates to it, which the run's 1 - W
#                reports
#   CARGO_FLAGS  extra flags for cargo build (for instance --offline)
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY
#
# It takes about 2 h with the default runs (measured: 1 h 48 min on an
# A100, about 2 ms per gate split); each run is also killed at
# RUN_SECONDS + 15 min.

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
OUT=${MPS_PREP_OUT:-/data/domingom/runs/gpu-mps-prep-$(date +%Y%m%d-%H%M)}
CASES=${CASES:-$HOME/mps-prep-cases}
RUNS=${RUNS:-"n8-chi64:following n16-chi16:following n16-chi32:following \
n16-chi64-ladder:following n20-chi32:following n20-chi64-ladder:following \
n16-chi16:interleaved n12-chi4:blocked n20-chi64:following"}
RUN_SECONDS=${RUN_SECONDS:-5400}
CHI_CAP=${MPS_PREP_CHI_CAP:-1024}
CARGO_FLAGS=${CARGO_FLAGS:-}
TN_VENV=/data/domingom/tn-venv
BIN=${CARGO_TARGET_DIR:-$REPO/target}/release/examples
SUMMARY=$OUT/summary.txt

# --- The GPU must be free ------------------------------------------------------
busy=$(nvidia-smi --query-compute-apps=pid,process_name --format=csv,noheader 2>/dev/null |
  grep -v nvidia-cuda-mps-server)
if [ -n "$busy" ] || pgrep -f 'h2_dev|gpu-h2-threads|gpu-h2-qualify|gate_split_bench' > /dev/null; then
  echo "The GPU is in use; run this once it is free:" >&2
  printf '%s\n' "$busy" >&2
  pgrep -a -f 'h2_dev|gpu-h2-threads|gpu-h2-qualify|gate_split_bench' >&2
  exit 2
fi

if [ ! -d "$CASES" ]; then
  echo "No case folder $CASES: copy mps-prep-cases.tar.gz and run tar xzf mps-prep-cases.tar.gz -C \$HOME" >&2
  exit 2
fi

# --- The libraries (as gpu-split-scaling.sh) -------------------------------------
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

mkdir -p "$OUT"
: > "$SUMMARY"

# --- Helpers -------------------------------------------------------------------
summary() {
  printf 'part=%s exit=%s seconds=%s %s\n' "$1" "$2" "$3" "$4" | tee -a "$SUMMARY"
}

# Runs "$@" under a time limit of $1 seconds, its output appended to $2;
# sets STATUS and SECONDS_TAKEN.
run_part() {
  local limit=$1 log=$2
  shift 2
  local start=$SECONDS
  echo "== $(date '+%F %T') limit=${limit}s $*" | tee -a "$log"
  timeout --foreground -k 60 "$limit" "$@" >> "$log" 2>&1
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
  echo "repo=$REPO out=$OUT cases=$CASES run_seconds=$RUN_SECONDS chi_cap=$CHI_CAP"
  echo "runs=$RUNS"
  git -C "$REPO" log --oneline -1
  nvidia-smi
  free -g
  rustc --version
  cargo --version
  env | grep -E '^(QDK_|CUDA|CARGO|MPS_PREP_)'
  ls -la "$CASES"
} > "$OUT/environment.txt" 2>&1

# Every MPS daemon off: the runs are alone on the GPU.
{
  echo quit | env -u CUDA_MPS_PIPE_DIRECTORY -u CUDA_MPS_LOG_DIRECTORY nvidia-cuda-mps-control
  CUDA_MPS_PIPE_DIRECTORY=$HOME/mps/pipe CUDA_MPS_LOG_DIRECTORY=$HOME/mps/log \
    nvidia-cuda-mps-control <<< quit
  sleep 2
  pgrep -a nvidia-cuda-mps
} > "$OUT/mps.log" 2>&1

# --- Build -----------------------------------------------------------------------
run_part 3600 "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" --release \
  $CARGO_FLAGS -p qdk_cutensornet --example mps_prep_check
summary build "$STATUS" "$SECONDS_TAKEN" "-"
[ "$STATUS" = 0 ] || exit 1

nvidia-smi --query-gpu=timestamp,memory.used,memory.total,utilization.gpu \
  --format=csv -l 5 > "$OUT/gpu-memory.csv" 2>&1 &
SAMPLER=$!
trap 'kill "$SAMPLER" 2>/dev/null' EXIT

# --- The runs ----------------------------------------------------------------------
for run in $RUNS; do
  case_name=${run%%:*}
  placement=${run##*:}
  log=$OUT/$case_name-$placement.log
  if [ ! -d "$CASES/$case_name" ]; then
    summary "$run" - - "missing=$CASES/$case_name"
    continue
  fi
  run_part $((RUN_SECONDS + 900)) "$log" "$BIN/mps_prep_check" --engine cuda-mps \
    --case "$CASES/$case_name" --placement "$placement" --chi-cap "$CHI_CAP" \
    --stop-after-seconds "$RUN_SECONDS"
  prepared=$(grep '^MPSPREP prepared' "$log" | tail -1)
  check=$(grep '^MPSPREP check' "$log" | tail -1)
  last=$(grep '^MPSPREP mark' "$log" | tail -1)
  summary "$run" "$STATUS" "$SECONDS_TAKEN" \
    "verdict=$(field verdict "$check") gate_seconds=$(field seconds "$prepared") largest_bond=$(field largest_bond "$prepared") one_minus_w=$(field one_minus_w "$prepared") energy_gap=$(field energy_gap "$check") largest_gap=$(field largest "$check") last_mark_seconds=$(field seconds "$last") $(printf '%s\n' "$last" | grep -o 'splits=[0-9]*') $(printf '%s\n' "$last" | grep -o 'gate_split_seconds=[0-9.]*')"
done

kill "$SAMPLER" 2>/dev/null
peak=$(awk -F', ' 'NR > 1 {sub(/ MiB/, "", $2); if ($2 + 0 > max) max = $2 + 0} END {print max + 0}' \
  "$OUT/gpu-memory.csv")
summary memory - - "gpu_memory_peak_mib=$peak"

# --- What to send back -------------------------------------------------------------
tar czf "$OUT.tar.gz" --exclude='*.tar.gz' -C "$(dirname "$OUT")" "$(basename "$OUT")"
echo "Send back: $OUT.tar.gz"
cat "$SUMMARY"
