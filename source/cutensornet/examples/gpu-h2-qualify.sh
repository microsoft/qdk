#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# Development only (not for review): qualifies CudaMpsEngine on H2 as
# CuTensorNetEngine was qualified (A100-RUN.md's step 4): 50 shots, one
# shot per process, the processes run in parallel on one GPU under the
# CUDA MPS daemon (NVIDIA's Multi-Process Service, which lets several
# processes share the GPU at once; unrelated to the engine's matrix
# product states). H2 is a development case.
#
# Why parallel processes, and how many. An H2 shot keeps the GPU waiting
# on small kernels and syncs (bond 24 at most): the engine is bound by
# latency, not by the GPU's throughput, so processes running side by side
# fill the GPU's idle time. 16 processes gave CuTensorNetEngine 5-5.7 times
# the throughput of one. So the script runs as many as the machine
# affords, P (PROCS overrides it), the smallest of:
#   - the CPU budget, nproc less 2: each process busy-waits on a core
#     while it waits for the GPU, and the 2 are left for the system;
#   - the GPU-memory budget, floor(0.8 x free GPU memory / per-process
#     peak). The peak is calibrated: shot 0 starts alone, and the GPU's
#     total memory.used is sampled every 5 s for CALIBRATION_SECONDS; the
#     peak is the largest reading less the reading before the start (the
#     total, because under MPS the per-process query may show only the MPS
#     server). A peak at or below 0 falls back to 800 MiB, step 4's
#     figure (about 13 GB for 16 processes);
#   - the MPS client limit, 48 per GPU;
#   - the number of shots.
# The part=procs line logs each budget and which one bound P. Shot 0 keeps
# running after the calibration and counts as one of the shots; the others
# start as slots free up, with at most P running at once.
#
# The parts, each with one line in summary.txt, in the order they run,
# with ESTIMATED durations (CudaMpsEngine's H2 shot took 301 s alone on
# the A100 of 2026-11-13):
#
#   h2-bytecode  H2's bytecode is the one every earlier run used (sha256);
#                on a fail nothing else runs.
#   build        the release build of h2_dev. Limit 1 h; 1-3 min on the
#                VM's warm target dir, 20-30 min on a fresh one.
#   mps-start    the MPS daemon, its pipe and log directories under
#                $HOME/mps.
#   calibration  shot 0 alone for CALIBRATION_SECONDS (default 60 s), the
#                GPU memory sampled to calibration.csv.
#   procs        P, as above.
#   shots        shots 0 to SHOTS - 1 (default 50), shot i writing
#                shots/shot-i.log, at most P at once, each under SHOT_LIMIT
#                (default 3 h); the GPU's memory and utilisation every 60 s
#                in gpu-memory.csv. At 5 times one process's throughput,
#                50 shots take about 50 min.
#   mps-quit     the MPS daemon stopped. It is stopped on every exit path,
#                Ctrl-C and failures included, after the running shots are.
#   acceptance   h2_summary.py on the shots' logs (h2-summary.txt), with
#                step 4's criteria: SHOTS shots and none failed; per shot
#                draws 1414, largest bond 24, TVD <= 1e-8, peaks 01010 and
#                10110; the G-test of the records against the readout
#                table with p > 0.05; and the first wave's throughput (the
#                first P shots to start) against CuTensorNetEngine's
#                qualified 14.7 shots/h, whose floor, 13.23 shots/h (14.7
#                less 10%), it must reach.
#
# Expected in all: about 55 min on the warm target dir. The last line
# printed gives the verdict, P, the shots per hour and the wall time.
#
# Run from anywhere, on the branch's checkout:
#
#   bash source/cutensornet/examples/gpu-h2-qualify.sh
#
# Send back $OUT.tar.gz (its path is printed at the end).
#
# Environment (all optional):
#   OUT            the output directory (default $HOME/gpu-h2-qualify-<date>)
#   SHOTS          the shots (default 50)
#   PROCS          the processes at once, instead of the computed P
#   CALIBRATION_SECONDS  shot 0's time alone (default 60)
#   SHOT_LIMIT     each shot's time limit in seconds (default 10800, 3 h)
#   H2             H2's bytecode JSON (default /data/domingom/h2.bytecode.json)
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY, QDK_CUSOLVER_LIBRARY
#                  the libraries, when neither at the pinned paths below nor
#                  in /data/domingom/tn-venv
#   CARGO_FLAGS    extra flags for cargo build (for instance --offline)

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
OUT=${OUT:-$HOME/gpu-h2-qualify-$(date +%Y%m%d-%H%M)}
SHOTS=${SHOTS:-50}
PROCS=${PROCS:-}
CALIBRATION_SECONDS=${CALIBRATION_SECONDS:-60}
SHOT_LIMIT=${SHOT_LIMIT:-10800}
H2=${H2:-/data/domingom/h2.bytecode.json}
H2_SHA256=a8a2b39cdb9df504f3142c6d1cff34097438d0da0add5049979878a45189ec5f
TABLE=$REPO/source/simulators/examples/h2-readout-distribution.txt
SUMMARIZE=$REPO/source/simulators/examples/h2_summary.py
TN_VENV=/data/domingom/tn-venv
CARGO_FLAGS=${CARGO_FLAGS:-}
BIN=${CARGO_TARGET_DIR:-$REPO/target}/release/examples
SUMMARY=$OUT/summary.txt
BUILD_BOX=3600
CALIBRATION_INTERVAL=5
# The budgets of P (header).
CPU_RESERVE=2
GPU_MEMORY_PERCENT=80
FALLBACK_PEAK_MIB=800
MPS_CLIENT_LIMIT=48
# Used only when nvidia-smi gives no free memory: step 4's count.
UNREAD_MEMORY_PROCS=16
# Step 4's TVD bar on the GPU (user, 2026-11-11).
TVD=1e-8
# The run 2 combination of gpu-qr.sh (2026-11-13): QR centre moves at the
# engine's default (on), GESVD.
FLAGS=(--engine cuda-mps --bytecode "$H2" --readout "$TABLE" --seed 1 --chi 128
  --order resumer --variant seq3-mpo --svd-algo gesvd)
export CUDA_MPS_PIPE_DIRECTORY=$HOME/mps/pipe CUDA_MPS_LOG_DIRECTORY=$HOME/mps/log
# A positive integer $2 for setting $1, or the script stops.
positive() {
  if [[ ! $2 =~ ^[1-9][0-9]*$ ]]; then
    echo "$1=$2: a positive integer, please" >&2
    exit 2
  fi
}
positive SHOTS "$SHOTS"
positive CALIBRATION_SECONDS "$CALIBRATION_SECONDS"
positive SHOT_LIMIT "$SHOT_LIMIT"
[ -z "$PROCS" ] || positive PROCS "$PROCS"

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

mkdir -p "$OUT/shots"
: > "$SUMMARY"

# --- Helpers -----------------------------------------------------------------
# One line in summary.txt: the part, its exit code, its seconds and what it
# found.
summary() {
  printf 'part=%s exit=%s seconds=%s %s\n' "$1" "$2" "$3" "$4" | tee -a "$SUMMARY"
}

# Runs "$@" under a time limit of $1 seconds, its output appended to $2;
# sets STATUS and SECONDS_TAKEN. In the foreground, so that a Ctrl-C stops
# it and the script at once.
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

# Sets USED and FREE to the GPU's total memory used and free in MiB (the
# first GPU nvidia-smi lists), each empty when unreadable.
read_gpu_memory() {
  local line
  line=$(nvidia-smi --query-gpu=memory.used,memory.free --format=csv,noheader,nounits 2>/dev/null | head -1)
  IFS=', ' read -r USED FREE <<< "$line"
  [[ ${USED:-} =~ ^[0-9]+$ ]] || USED=
  [[ ${FREE:-} =~ ^[0-9]+$ ]] || FREE=
}

# Starts shot $1 in the background, its output in shots/shot-$1.log.
# `timeout` puts the shot in its own process group, so a Ctrl-C reaches
# the script alone, whose exit stops the shots.
SHOT_PIDS=()
start_shot() {
  timeout -k 60 "$SHOT_LIMIT" "$BIN/h2_dev" "${FLAGS[@]}" --first-shot "$1" --shots 1 \
    < /dev/null > "$OUT/shots/shot-$1.log" 2>&1 &
  SHOT_PIDS[$1]=$!
  echo "$(date '+%F %T') start shot=$1 pid=$!" >> "$OUT/schedule.log"
}

# Sets RUNNING to the number of shots still running.
count_running() {
  local pid
  RUNNING=0
  for pid in "${SHOT_PIDS[@]}"; do
    if kill -0 "$pid" 2>/dev/null; then
      RUNNING=$((RUNNING + 1))
    fi
  done
}

# Stops the shots still running (TERM, which timeout passes on; KILL after
# 30 s) and logs how many there were.
stop_shots() {
  count_running
  [ "$RUNNING" = 0 ] && return
  local stopped=$RUNNING pid waited=0
  for pid in "${SHOT_PIDS[@]}"; do
    kill -TERM "$pid" 2>/dev/null
  done
  while [ "$RUNNING" -gt 0 ] && [ "$waited" -lt 30 ]; do
    sleep 1
    waited=$((waited + 1))
    count_running
  done
  for pid in "${SHOT_PIDS[@]}"; do
    kill -KILL "$pid" 2>/dev/null
  done
  summary stop - "$SECONDS" "shots_stopped=$stopped still_running_after_30s=$RUNNING"
}

# Stops the MPS daemon if this script started it.
MPS_STARTED=no
quit_mps() {
  [ "$MPS_STARTED" = yes ] || return
  local start=$SECONDS status left waited=0
  echo quit | nvidia-cuda-mps-control >> "$OUT/mps.log" 2>&1
  status=$?
  MPS_STARTED=no
  # The daemon and its server, this user's, as pgrep names them (comm, 15
  # characters: nvidia-cuda-mps).
  left=$(pgrep -u "$UID" -c nvidia-cuda-mps)
  while [ "${left:-0}" -gt 0 ] && [ "$waited" -lt 15 ]; do
    sleep 1
    waited=$((waited + 1))
    left=$(pgrep -u "$UID" -c nvidia-cuda-mps)
  done
  pgrep -a -u "$UID" nvidia-cuda-mps >> "$OUT/mps.log" 2>&1
  summary mps-quit "$status" "$((SECONDS - start))" \
    "verdict=$([ "${left:-0}" = 0 ] && echo pass || echo fail) mps_processes_left=${left:-0} (mps.log)"
}

# Every exit path: stops the shots, the GPU monitor and the MPS daemon,
# packs the outputs and prints the verdict line.
VERDICT=incomplete
PROCS_USED=-
SHOTS_PER_HOUR=-
WHOLE_RUN_PER_HOUR=-
MONITOR_PID=
finish() {
  local rc=$?
  trap - EXIT INT TERM HUP
  stop_shots
  if [ -n "$MONITOR_PID" ]; then
    kill "$MONITOR_PID" 2>/dev/null
    wait "$MONITOR_PID" 2>/dev/null
  fi
  quit_mps
  tar czf "$OUT.tar.gz" --exclude='*.tar.gz' -C "$(dirname "$OUT")" "$(basename "$OUT")"
  echo "Send back: $OUT.tar.gz"
  echo "== Summary ($SECONDS s)"
  cat "$SUMMARY"
  echo "RESULT verdict=$VERDICT engine=cuda-mps procs=$PROCS_USED shots_per_hour=$SHOTS_PER_HOUR (first wave; whole run $WHOLE_RUN_PER_HOUR; CuTensorNetEngine's floor 13.23) wall_seconds=$SECONDS out=$OUT.tar.gz"
  exit "$rc"
}
trap finish EXIT
trap 'VERDICT=interrupted; exit 130' INT
trap 'VERDICT=interrupted; exit 143' TERM
trap 'VERDICT=interrupted; exit 129' HUP

{
  date
  echo "repo=$REPO out=$OUT shots=$SHOTS procs=${PROCS:-auto} calibration_seconds=$CALIBRATION_SECONDS shot_limit=$SHOT_LIMIT"
  echo "flags=${FLAGS[*]}"
  git -C "$REPO" log --oneline -1
  echo "nproc=$(nproc)"
  nvidia-smi
  rustc --version
  cargo --version
  python3 --version
  sha256sum "$H2"
  env | grep -E '^(QDK_|CUDA|CARGO)'
} > "$OUT/environment.txt" 2>&1

# --- h2-bytecode: the one every earlier run used -------------------------------
if [ ! -f "$H2" ] || [ "$(sha256sum "$H2" | awk '{print $1}')" != "$H2_SHA256" ]; then
  summary h2-bytecode - 0 "verdict=fail path=$H2 sha256_expected=$H2_SHA256 (nothing else runs)"
  VERDICT=FAIL
  exit 1
fi
summary h2-bytecode - 0 "verdict=pass path=$H2"

# --- build (release) -----------------------------------------------------------
run_part "$BUILD_BOX" "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" \
  --release $CARGO_FLAGS -p qdk_simulators --example h2_dev
summary build "$STATUS" "$SECONDS_TAKEN" "verdict=$([ "$STATUS" = 0 ] && echo pass || echo fail) h2_dev=$BIN/h2_dev (build.log)"
if [ "$STATUS" != 0 ]; then
  VERDICT=FAIL
  exit 1
fi

# --- mps-start -----------------------------------------------------------------
# Any daemon left running, on the default pipe directory or ours, stops
# first.
{
  pgrep -a nvidia-cuda-mps
  echo quit | env -u CUDA_MPS_PIPE_DIRECTORY -u CUDA_MPS_LOG_DIRECTORY nvidia-cuda-mps-control
  echo quit | nvidia-cuda-mps-control
} >> "$OUT/mps.log" 2>&1
mkdir -p "$CUDA_MPS_PIPE_DIRECTORY" "$CUDA_MPS_LOG_DIRECTORY"
# Set before the start: a start that reports a failure may still leave a
# daemon behind, and quitting one that is not there is harmless.
MPS_STARTED=yes
nvidia-cuda-mps-control -d >> "$OUT/mps.log" 2>&1
STATUS=$?
summary mps-start "$STATUS" 0 \
  "verdict=$([ "$STATUS" = 0 ] && echo pass || echo fail) pipe=$CUDA_MPS_PIPE_DIRECTORY log=$CUDA_MPS_LOG_DIRECTORY"
if [ "$STATUS" != 0 ]; then
  VERDICT=FAIL
  exit 1
fi

nvidia-smi --query-gpu=timestamp,memory.used,memory.total,utilization.gpu --format=csv -l 60 \
  > "$OUT/gpu-memory.csv" 2>&1 &
MONITOR_PID=$!

# --- calibration: shot 0 alone ---------------------------------------------------
start=$SECONDS
read_gpu_memory
BASELINE=$USED
FREE_BEFORE=$FREE
echo "seconds,memory.used [MiB],memory.free [MiB]" > "$OUT/calibration.csv"
echo "0,$USED,$FREE" >> "$OUT/calibration.csv"
largest=$USED
start_shot 0
elapsed=0
while [ "$elapsed" -lt "$CALIBRATION_SECONDS" ]; do
  sleep "$CALIBRATION_INTERVAL"
  elapsed=$((elapsed + CALIBRATION_INTERVAL))
  read_gpu_memory
  echo "$elapsed,$USED,$FREE" >> "$OUT/calibration.csv"
  if [ -n "$USED" ] && { [ -z "$largest" ] || [ "$USED" -gt "$largest" ]; }; then
    largest=$USED
  fi
  kill -0 "${SHOT_PIDS[0]}" 2>/dev/null || break
done
PEAK=-
if [ -n "$BASELINE" ] && [ -n "$largest" ]; then
  PEAK=$((largest - BASELINE))
fi
PEAK_SOURCE=calibrated
if [ "$PEAK" = - ] || [ "$PEAK" -le 0 ]; then
  PEAK_SOURCE="fallback (calibrated $PEAK)"
  PEAK=$FALLBACK_PEAK_MIB
fi
# The MPS server's pid: shot 0 runs under MPS.
MPS_SERVER=$(echo get_server_list | nvidia-cuda-mps-control 2>/dev/null | tr '\n' ',' | sed 's/,$//')
count_running
shot0_line=$(grep -c '^SHOT ' "$OUT/shots/shot-0.log")
calibration_verdict=pass
if [ "$RUNNING" = 0 ] && [ "$shot0_line" = 0 ]; then
  calibration_verdict=fail
fi
summary calibration - "$((SECONDS - start))" \
  "verdict=$calibration_verdict sampled_seconds=$elapsed baseline_mib=${BASELINE:--} largest_mib=${largest:--} free_before_mib=${FREE_BEFORE:--} peak_mib=$PEAK peak_source=$PEAK_SOURCE mps_server=${MPS_SERVER:-none} shot0_running=$([ "$RUNNING" = 1 ] && echo yes || echo no) (calibration.csv)"
if [ "$calibration_verdict" = fail ]; then
  # Shot 0 ended without a SHOT line: the engine fails alone, so the other
  # shots would too.
  VERDICT=FAIL
  exit 1
fi

# --- procs: P ----------------------------------------------------------------------
NPROC=$(nproc)
CPU_BUDGET=$((NPROC - CPU_RESERVE))
if [ -n "$FREE_BEFORE" ]; then
  GPU_BUDGET=$((FREE_BEFORE * GPU_MEMORY_PERCENT / 100 / PEAK))
  GPU_NOTE="${GPU_MEMORY_PERCENT}% of free ${FREE_BEFORE} MiB / peak ${PEAK} MiB"
else
  GPU_BUDGET=$UNREAD_MEMORY_PROCS
  GPU_NOTE="free memory unread, step 4's count"
fi
P=$CPU_BUDGET
for budget in "$GPU_BUDGET" "$MPS_CLIENT_LIMIT" "$SHOTS"; do
  [ "$budget" -lt "$P" ] && P=$budget
done
BOUND=
[ "$CPU_BUDGET" = "$P" ] && BOUND=$BOUND,cpu
[ "$GPU_BUDGET" = "$P" ] && BOUND=$BOUND,gpu-memory
[ "$MPS_CLIENT_LIMIT" = "$P" ] && BOUND=$BOUND,mps-clients
[ "$SHOTS" = "$P" ] && BOUND=$BOUND,shots
BOUND=${BOUND#,}
if [ "$P" -lt 1 ]; then
  P=1
  BOUND="$BOUND (raised to 1)"
fi
if [ -n "$PROCS" ]; then
  BOUND="PROCS (computed $P, bound by $BOUND)"
  P=$PROCS
fi
PROCS_USED=$P
summary procs - 0 \
  "procs=$P bound=$BOUND cpu_budget=$CPU_BUDGET (nproc $NPROC less $CPU_RESERVE) gpu_memory_budget=$GPU_BUDGET ($GPU_NOTE, $PEAK_SOURCE) mps_client_limit=$MPS_CLIENT_LIMIT shots=$SHOTS"

# --- shots: the rest, at most P at once -----------------------------------------
start=$SECONDS
for ((shot = 1; shot < SHOTS; shot++)); do
  count_running
  while [ "$RUNNING" -ge "$P" ]; do
    sleep 2
    count_running
  done
  start_shot "$shot"
done
nonzero=
timed_out=
for ((shot = 0; shot < SHOTS; shot++)); do
  wait "${SHOT_PIDS[$shot]}"
  status=$?
  echo "== exit=$status" >> "$OUT/shots/shot-$shot.log"
  [ "$status" = 0 ] || nonzero=$nonzero,$shot
  [ "$status" = 124 ] && timed_out=$timed_out,$shot
done
nonzero=${nonzero#,}
timed_out=${timed_out#,}
summary shots - "$((SECONDS - start))" \
  "launched=$SHOTS procs=$P nonzero_exits=${nonzero:-none} timed_out=${timed_out:-none} (shots/, schedule.log, gpu-memory.csv)"

kill "$MONITOR_PID" 2>/dev/null
wait "$MONITOR_PID" 2>/dev/null
MONITOR_PID=
quit_mps
peak_used=$(awk -F', *' 'NR > 1 {v = $2 + 0; if (v > m) m = v} END {print m + 0}' "$OUT/gpu-memory.csv")

# --- acceptance ------------------------------------------------------------------
start=$SECONDS
python3 "$SUMMARIZE" --tvd "$TVD" --table "$TABLE" --first-wave "$P" --expect-shots "$SHOTS" \
  "$OUT"/shots/shot-*.log > "$OUT/h2-summary.txt" 2>&1
STATUS=$?
cat "$OUT/h2-summary.txt"
throughput=$(grep '^throughput: ' "$OUT/h2-summary.txt")
SHOTS_PER_HOUR=$(printf '%s\n' "$throughput" | sed -nE 's/.*first wave [0-9]+ shots, ([0-9.]+) shots\/h.*/\1/p')
WHOLE_RUN_PER_HOUR=$(printf '%s\n' "$throughput" | sed -nE 's/.*whole run ([0-9.]+) shots\/h.*/\1/p')
SHOTS_PER_HOUR=${SHOTS_PER_HOUR:--}
WHOLE_RUN_PER_HOUR=${WHOLE_RUN_PER_HOUR:--}
VERDICT=$(grep -o 'ACCEPTANCE: [A-Z]*' "$OUT/h2-summary.txt" | awk '{print $2}')
VERDICT=${VERDICT:-FAIL}
g_test=$(grep '^G = ' "$OUT/h2-summary.txt" | sed -E 's/ \(20000 draws\)//')
summary acceptance "$STATUS" "$((SECONDS - start))" \
  "verdict=$VERDICT procs=$P first_wave_shots_per_hour=$SHOTS_PER_HOUR whole_run_shots_per_hour=$WHOLE_RUN_PER_HOUR cutensornet_qualified=14.7 cutensornet_floor=13.23 gpu_memory_peak_mib=$peak_used g_test=\"$g_test\" (h2-summary.txt)"
[ "$VERDICT" = PASS ]
