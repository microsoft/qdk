#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# Development only (not for review): what one cuTensorNet gate split costs
# on the A100 as the bond grows to 12 872, the largest bond the 4×4
# Hubbard QPE's state needs (C(16, 8) + 2). The Hubbard study extrapolated
# 6–12 min per split from 3–6 ms at bond 256 by (χ/256)³; this measures
# it. Two parts, each with its own log and one line in summary.txt:
#
#   checked    gate_split_bench at bonds 256, 512 and 1024, each split
#              checked against faer's SVD on the host before it is timed:
#              the curve's small end, and proof that the bench is right.
#   unchecked  the same gate split at bonds 2048, 4096, 8192 and 12 872,
#              too large for the host's reference: the first run, then
#              REPS timed runs, the library's extents and discarded weight,
#              and the workspace's size. A bond whose memory does not fit
#              prints status=refused and the next one still runs.
#
# Each part times GESVD (the engines' algorithm) and GESVDR (randomized,
# keeping exactly b of the 2b values), with the REDUCED gate split.
# nvidia-smi samples the GPU's memory every 5 s into gpu-memory.csv.
#
# Run it alone on the GPU: it refuses to start while another process
# uses the GPU (the threads run, for instance), and stops any MPS daemon.
#
#   bash source/cutensornet/examples/gpu-split-scaling.sh
#
# Environment (all optional):
#   OUT             the output directory (default $HOME/gpu-split-scaling-<date>)
#   CHECKED_BONDS   default 256,512,1024
#   UNCHECKED_BONDS default 2048,4096,8192,12872
#   SVDS            default gesvd,gesvdr
#   REPS            timed runs per unchecked split, after the first (default 2)
#   CARGO_FLAGS     extra flags for cargo build (for instance --offline)
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY
#
# It takes 1–2 h; the unchecked part stops after 3 h.

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
OUT=${OUT:-$HOME/gpu-split-scaling-$(date +%Y%m%d-%H%M)}
CHECKED_BONDS=${CHECKED_BONDS-256,512,1024}
UNCHECKED_BONDS=${UNCHECKED_BONDS-2048,4096,8192,12872}
SVDS=${SVDS:-gesvd,gesvdr}
REPS=${REPS:-2}
CARGO_FLAGS=${CARGO_FLAGS:-}
TN_VENV=/data/domingom/tn-venv
BIN=${CARGO_TARGET_DIR:-$REPO/target}/release/examples
SUMMARY=$OUT/summary.txt
# The 4×4 Hubbard step's splits (the study's CPU run): about 1200 at the
# largest bond (the low end) and 5500 in all (the high end).
SPLITS_AT_PEAK=1200
SPLITS_PER_STEP=5500

# --- The GPU must be free ------------------------------------------------------
busy=$(nvidia-smi --query-compute-apps=pid,process_name --format=csv,noheader 2>/dev/null |
  grep -v nvidia-cuda-mps-server)
if [ -n "$busy" ] || pgrep -f 'h2_dev|gpu-h2-threads|gpu-h2-qualify' > /dev/null; then
  echo "The GPU is in use; run this once it is free:" >&2
  printf '%s\n' "$busy" >&2
  pgrep -a -f 'h2_dev|gpu-h2-threads|gpu-h2-qualify' >&2
  exit 2
fi

# --- The libraries (as gpu-h2-threads.sh) ----------------------------------------
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
# The batched cuSOLVER part of the bench is not wanted here.
unset QDK_CUSOLVER_LIBRARY

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
  echo "repo=$REPO out=$OUT checked=$CHECKED_BONDS unchecked=$UNCHECKED_BONDS svds=$SVDS reps=$REPS"
  git -C "$REPO" log --oneline -1
  nvidia-smi
  free -g
  rustc --version
  cargo --version
  env | grep -E '^(QDK_|CUDA|CARGO)'
} > "$OUT/environment.txt" 2>&1

# Every MPS daemon off: the bench runs alone on the GPU.
{
  echo quit | env -u CUDA_MPS_PIPE_DIRECTORY -u CUDA_MPS_LOG_DIRECTORY nvidia-cuda-mps-control
  CUDA_MPS_PIPE_DIRECTORY=$HOME/mps/pipe CUDA_MPS_LOG_DIRECTORY=$HOME/mps/log \
    nvidia-cuda-mps-control <<< quit
  sleep 2
  pgrep -a nvidia-cuda-mps
} > "$OUT/mps.log" 2>&1

# --- Build -----------------------------------------------------------------------
run_part 3600 "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" --release \
  $CARGO_FLAGS -p qdk_cutensornet --example gate_split_bench
summary build "$STATUS" "$SECONDS_TAKEN" "-"
[ "$STATUS" = 0 ] || exit 1

nvidia-smi --query-gpu=timestamp,memory.used,memory.total,utilization.gpu \
  --format=csv -l 5 > "$OUT/gpu-memory.csv" 2>&1 &
SAMPLER=$!
trap 'kill "$SAMPLER" 2>/dev/null' EXIT

# --- Part 1: checked bonds ----------------------------------------------------------
if [ -n "$CHECKED_BONDS" ]; then
  log=$OUT/checked.log
  run_part 1800 "$log" "$BIN/gate_split_bench" --engine cutensornet \
    --bonds "$CHECKED_BONDS" --svd-algos "$SVDS" --gate-split-algos reduced
  summary checked "$STATUS" "$SECONDS_TAKEN" \
    "ok=$(grep -c '^GATESPLIT status=ok' "$log") disagrees=$(grep -c 'status=disagrees' "$log") refused=$(grep -c 'status=refused' "$log")"
  grep '^GATESPLIT status=ok' "$log" | while read -r line; do
    printf '  b=%s svd=%s reduced=%s ms_median=%s\n' "$(field b "$line")" \
      "$(field svd "$line")" "$(field reduced "$line")" "$(field ms_median "$line")"
  done | tee -a "$SUMMARY"
fi

# --- Part 2: unchecked bonds ----------------------------------------------------------
if [ -n "$UNCHECKED_BONDS" ]; then
  log=$OUT/unchecked.log
  run_part 10800 "$log" "$BIN/gate_split_bench" --engine cutensornet --bonds "" \
    --unchecked-bonds "$UNCHECKED_BONDS" --svd-algos "$SVDS" --gate-split-algos reduced \
    --reps "$REPS"
  summary unchecked "$STATUS" "$SECONDS_TAKEN" \
    "ok=$(grep -c '^UNCHECKED status=ok' "$log") suspect=$(grep -c '^UNCHECKED status=suspect' "$log") refused=$(grep -c '^UNCHECKED status=refused' "$log")"
  grep '^UNCHECKED status=' "$log" | while read -r line; do
    status=$(field status "$line")
    median=$(field s_median "$line")
    projection=""
    if [ -n "$median" ]; then
      projection=$(awk -v s="$median" -v low="$SPLITS_AT_PEAK" -v high="$SPLITS_PER_STEP" \
        'BEGIN {printf "step_hours_if_all_at_this_bond=%.1f-%.1f", s * low / 3600, s * high / 3600}')
    fi
    printf '  status=%s b=%s svd=%s reduced=%s s_median=%s first_run_s=%s workspace_device_gb=%s %s\n' \
      "$status" "$(field b "$line")" "$(field svd "$line")" "$(field reduced "$line")" \
      "${median:--}" "$(field first_run_s "$line")" "$(field workspace_device_gb "$line")" \
      "$projection"
  done | tee -a "$SUMMARY"
  grep '^UNCHECKED status=refused' "$log" | sed 's/^/  /' | tee -a "$SUMMARY"
fi

kill "$SAMPLER" 2>/dev/null
peak=$(awk -F', ' 'NR > 1 {sub(/ MiB/, "", $2); if ($2 + 0 > max) max = $2 + 0} END {print max + 0}' \
  "$OUT/gpu-memory.csv")
summary memory - - "gpu_memory_peak_mib=$peak"

# --- What to send back -------------------------------------------------------------
tar czf "$OUT.tar.gz" --exclude='*.tar.gz' -C "$(dirname "$OUT")" "$(basename "$OUT")"
echo "Send back: $OUT.tar.gz"
cat "$SUMMARY"
