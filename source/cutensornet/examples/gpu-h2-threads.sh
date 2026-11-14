#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# Development only (not for review): do threads in one process, each with
# its own CudaMpsEngine (its own cuTensorNet handle and CUDA stream), match
# or beat separate processes under the CUDA MPS daemon (NVIDIA's
# Multi-Process Service) on H2? The qualification (gpu-h2-qualify.sh) ran
# 22 processes under MPS: 60.6 shots/h in its first wave, of which shot 0
# ran 60 s alone first (about 63.5 shots/h without that head start).
#
# Each configuration runs one wave: as many shots as workers (shots 0 to
# N - 1, seed 1), all started at once, with the qualification's flags and
# per-shot checks (TVD <= 1e-8, the peaks 01010 and 10110, largest bond
# 24, 1414 draws). Configurations, in this order (CONFIGS), so that each
# isolates one cause:
#
#   id                workers      allocation      sync      MPS daemon
#   T-as-is           22 threads   cudaMalloc      spin      no
#   T-async           22 threads   stream-ordered  spin      no
#   T-async-block     22 threads   stream-ordered  blocking  no
#   T-async-block-44  44 threads   stream-ordered  blocking  no
#   P-async           22 processes stream-ordered  spin      yes
#   P-as-is           22 processes cudaMalloc      spin      yes  (only when
#                     listed: the qualification's baseline, rerun without
#                     the head start)
#
# An id is T (threads in one process: h2_dev --threads N) or P (N
# processes, h2_dev --shots 1 each), then as-is, async
# (--stream-ordered-allocation) or async-block (and --blocking-sync), then
# an optional -N for N workers (default WORKERS). The MPS daemon runs for
# the P configurations alone, started before and stopped after each; none
# runs during a T configuration.
#
# Per configuration, in $OUT/<id>/: the logs (threads.log, or
# shot-<i>.log per process); gpu.csv, the GPU's memory.used and
# utilisation every 5 s; times-before.txt and times-after.txt, bash's
# `times` (the children's user and system CPU seconds, threads included);
# when /usr/bin/time exists, time-<i>.txt per process (`-v`: context
# switches, largest resident set); h2-summary.txt, h2_summary.py with
# --first-wave N --expect-shots N, so the throughput is the
# qualification's first-wave rate: N / (last end - first start). One line
# in summary.txt per configuration:
#   verdict      pass when every process exited 0, N SHOT lines, none
#                failed, and every shot's checks hold; the G-test is
#                logged, not judged (one wave of 22 shots is a small
#                sample, and a 5% chance failure would hide the timing);
#   shots_per_hour, vs_60.6, vs_63.5  the first wave's rate, and its ratio
#                to the qualification's 60.6 and to 63.5 (above);
#   cpu_user_seconds, cpu_system_seconds, cpu_cores_busy  the children's
#                CPU time, and (user + system) / wall: about the workers'
#                count while they spin, far fewer when they block. Under
#                MPS the MPS server's own CPU time is not counted;
#   voluntary_switches, involuntary_switches, max_rss_kib  from
#                /usr/bin/time, summed (the largest set: the largest);
#   gpu_memory_peak_mib, gpu_utilisation_mean  from gpu.csv, the peak less
#                the reading before the start;
#   blocking_sync  h2_dev's BLOCKING_SYNC line (the call's status), for
#                the -block configurations;
#   records_vs_<first>  shots whose records equal the first
#                configuration's, of those both ran.
# Then a table of all configurations, and the RESULT line.
#
# Time limit: LIMIT_PER_WORKER (default 245 s, one shot at
# CuTensorNetEngine's qualified 14.7 shots/h) times the workers: 90 min at
# 22, 3 h at 44. A configuration that reaches it is slower than
# CuTensorNetEngine, which is answer enough; it is stopped, its finished
# shots kept, and its verdict is fail.
#
# ESTIMATED wall time, from the qualification's shot times (396-1,250 s
# each under sharing, 60.6 shots/h at 22 processes): the build 1-3 min on
# the VM's warm target dir; T-as-is 45-90 min (12-30 shots/h predicted,
# stopped at 90); T-async, T-async-block and P-async about 20-25 min each;
# T-async-block-44 about 40-50 min. In all about 2.5-3.5 h; at most 9 h
# if every configuration reaches its limit.
#
# Run from anywhere, on the branch's checkout:
#
#   bash source/cutensornet/examples/gpu-h2-threads.sh
#
# Send back $OUT.tar.gz (its path is printed at the end).
#
# Environment (all optional):
#   OUT               the output directory (default
#                     $HOME/gpu-h2-threads-<date>)
#   CONFIGS           the configurations, in order (default "T-as-is
#                     T-async T-async-block T-async-block-44 P-async")
#   WORKERS           the workers of an id without -N (default 22, the
#                     qualification's P)
#   LIMIT_PER_WORKER  seconds per worker of a configuration's limit
#                     (default 245)
#   H2                H2's bytecode JSON (default
#                     /data/domingom/h2.bytecode.json)
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY, QDK_CUSOLVER_LIBRARY
#                     the libraries, when neither at the pinned paths below
#                     nor in /data/domingom/tn-venv
#   CARGO_FLAGS       extra flags for cargo (for instance --offline)

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
OUT=${OUT:-$HOME/gpu-h2-threads-$(date +%Y%m%d-%H%M)}
CONFIGS=${CONFIGS:-T-as-is T-async T-async-block T-async-block-44 P-async}
WORKERS=${WORKERS:-22}
LIMIT_PER_WORKER=${LIMIT_PER_WORKER:-245}
H2=${H2:-/data/domingom/h2.bytecode.json}
H2_SHA256=a8a2b39cdb9df504f3142c6d1cff34097438d0da0add5049979878a45189ec5f
TABLE=$REPO/source/simulators/examples/h2-readout-distribution.txt
SUMMARIZE=$REPO/source/simulators/examples/h2_summary.py
TN_VENV=/data/domingom/tn-venv
CARGO_FLAGS=${CARGO_FLAGS:-}
BIN=${CARGO_TARGET_DIR:-$REPO/target}/release/examples
SUMMARY=$OUT/summary.txt
BUILD_BOX=3600
GPU_INTERVAL=5
# The qualification's first wave (gpu-h2-qualify.sh, 2026-11-13), and the
# same without shot 0's 60 s head start.
BASELINE=60.6
BASELINE_NO_HEAD_START=63.5
# Step 4's TVD bar on the GPU, and gpu-h2-qualify.sh's flags.
TVD=1e-8
FLAGS=(--engine cuda-mps --bytecode "$H2" --readout "$TABLE" --seed 1 --chi 128
  --order resumer --variant seq3-mpo --svd-algo gesvd)
export CUDA_MPS_PIPE_DIRECTORY=$HOME/mps/pipe CUDA_MPS_LOG_DIRECTORY=$HOME/mps/log
ID_PATTERN='^([TP])-(as-is|async|async-block)(-([1-9][0-9]*))?$'
# A positive integer $2 for setting $1, or the script stops.
positive() {
  if [[ ! $2 =~ ^[1-9][0-9]*$ ]]; then
    echo "$1=$2: a positive integer, please" >&2
    exit 2
  fi
}
positive WORKERS "$WORKERS"
positive LIMIT_PER_WORKER "$LIMIT_PER_WORKER"
[ -n "${CONFIGS// /}" ] || {
  echo "CONFIGS is empty" >&2
  exit 2
}
for id in $CONFIGS; do
  if [[ ! $id =~ $ID_PATTERN ]]; then
    echo "CONFIGS=$CONFIGS: $id is not T or P, then as-is, async or async-block, then an optional -N" >&2
    exit 2
  fi
done

# --- The libraries (as gpu-h2-qualify.sh) -----------------------------------
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
# One line in summary.txt: the part, its exit code, its seconds and what it
# found.
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

# The GPU's total memory used in MiB (the first GPU nvidia-smi lists), or
# nothing when unreadable.
gpu_memory_used() {
  local used
  used=$(nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits 2>/dev/null | head -1)
  [[ ${used:-} =~ ^[0-9]+$ ]] && echo "$used"
}

# The children's CPU seconds in bash's `times` file $1: "user system".
# `times` writes the shell's line, then its children's (XmY.ZZZs each).
children_seconds() {
  awk 'NR == 2 {
    for (i = 1; i <= 2; i++) {
      t = $i; sub(/s$/, "", t); split(t, part, "m"); s[i] = part[1] * 60 + part[2]
    }
    printf "%.1f %.1f\n", s[1], s[2]
  }' "$1"
}

# Stops every MPS daemon of this user, on the default pipe directory and
# ours, and waits up to 15 s for its processes to end; logs to mps.log and
# sets MPS_LEFT to the processes still there.
quit_any_mps() {
  local waited=0
  {
    echo "== $(date '+%F %T') quit"
    echo quit | env -u CUDA_MPS_PIPE_DIRECTORY -u CUDA_MPS_LOG_DIRECTORY nvidia-cuda-mps-control
    echo quit | nvidia-cuda-mps-control
  } >> "$OUT/mps.log" 2>&1
  MPS_LEFT=$(pgrep -u "$UID" -c nvidia-cuda-mps)
  while [ "${MPS_LEFT:-0}" -gt 0 ] && [ "$waited" -lt 15 ]; do
    sleep 1
    waited=$((waited + 1))
    MPS_LEFT=$(pgrep -u "$UID" -c nvidia-cuda-mps)
  done
  MPS_LEFT=${MPS_LEFT:-0}
  pgrep -a -u "$UID" nvidia-cuda-mps >> "$OUT/mps.log" 2>&1
  MPS_STARTED=no
}

# Starts the MPS daemon on our pipe directory; sets STATUS.
MPS_STARTED=no
start_mps() {
  mkdir -p "$CUDA_MPS_PIPE_DIRECTORY" "$CUDA_MPS_LOG_DIRECTORY"
  # Set before the start: a start that reports a failure may still leave
  # a daemon behind, and quitting one that is not there is harmless.
  MPS_STARTED=yes
  echo "== $(date '+%F %T') start" >> "$OUT/mps.log"
  nvidia-cuda-mps-control -d >> "$OUT/mps.log" 2>&1
  STATUS=$?
}

# The workers of the configuration running, and its GPU monitor.
WORKER_PIDS=()
MONITOR_PID=
# Stops the workers still running (TERM, which timeout passes on; KILL
# after 30 s); the number stopped in STOPPED.
stop_workers() {
  local pid waited=0 running
  STOPPED=0
  for pid in "${WORKER_PIDS[@]}"; do
    kill -0 "$pid" 2>/dev/null && STOPPED=$((STOPPED + 1))
    kill -TERM "$pid" 2>/dev/null
  done
  running=$STOPPED
  while [ "$running" -gt 0 ] && [ "$waited" -lt 30 ]; do
    sleep 1
    waited=$((waited + 1))
    running=0
    for pid in "${WORKER_PIDS[@]}"; do
      kill -0 "$pid" 2>/dev/null && running=$((running + 1))
    done
  done
  for pid in "${WORKER_PIDS[@]}"; do
    kill -KILL "$pid" 2>/dev/null
  done
  WORKER_PIDS=()
}
stop_monitor() {
  if [ -n "$MONITOR_PID" ]; then
    kill "$MONITOR_PID" 2>/dev/null
    wait "$MONITOR_PID" 2>/dev/null
    MONITOR_PID=
  fi
}

# Starts worker $1 of configuration directory $2 in the background: its
# output in $3, its /usr/bin/time report (when there is one) in
# $2/time-$1.txt, then h2_dev's arguments. `timeout` puts the worker in its
# own process group, so a Ctrl-C reaches the script alone, whose exit
# stops the workers.
start_worker() {
  local index=$1 dir=$2 log=$3
  shift 3
  local timer=()
  [ -x /usr/bin/time ] && timer=(/usr/bin/time -v -o "$dir/time-$index.txt")
  timeout -k 60 "$LIMIT" "${timer[@]}" "$BIN/h2_dev" "${FLAGS[@]}" "$@" \
    < /dev/null > "$log" 2>&1 &
  WORKER_PIDS+=($!)
  echo "$(date '+%F %T') start worker=$index pid=$! $*" >> "$dir/schedule.log"
}

# The "shot records" pairs of the SHOT lines in the logs of directory $1,
# sorted as `join` needs.
records_of() {
  cat "$1"/threads.log "$1"/shot-*.log 2>/dev/null |
    awk '/^SHOT / {
      shot = ""; records = ""
      for (i = 2; i <= NF; i++) {
        split($i, kv, "=")
        if (kv[1] == "shot") shot = kv[2]
        if (kv[1] == "records") records = kv[2]
      }
      print shot, records
    }' | LC_ALL=C sort
}

# Every exit path: stops the workers, the GPU monitor and the MPS daemon,
# packs the outputs and prints the table and the verdict line.
VERDICT=incomplete
TABLE_LINES=()
finish() {
  local rc=$?
  trap - EXIT INT TERM HUP
  if [ "${#WORKER_PIDS[@]}" -gt 0 ]; then
    stop_workers
    summary stop - "$SECONDS" "workers_stopped=$STOPPED"
  fi
  stop_monitor
  if [ "$MPS_STARTED" = yes ]; then
    quit_any_mps
    summary mps-quit - 0 "mps_processes_left=$MPS_LEFT (mps.log)"
  fi
  {
    echo "== Table"
    printf '%-18s %-8s %-6s %10s %8s %8s %9s %9s %9s\n' config workers verdict shots/h vs_60.6 vs_63.5 cpu_cores gpu_mib records
    for line in "${TABLE_LINES[@]}"; do
      printf '%s\n' "$line"
    done
  } | tee -a "$SUMMARY"
  tar czf "$OUT.tar.gz" --exclude='*.tar.gz' -C "$(dirname "$OUT")" "$(basename "$OUT")"
  echo "Send back: $OUT.tar.gz"
  echo "== Summary ($SECONDS s)"
  cat "$SUMMARY"
  echo "RESULT verdict=$VERDICT configs=\"$CONFIGS\" ${RATES:-} baseline=$BASELINE (first wave, shot 0's head start included; $BASELINE_NO_HEAD_START without) wall_seconds=$SECONDS out=$OUT.tar.gz"
  exit "$rc"
}
trap finish EXIT
trap 'VERDICT=interrupted; exit 130' INT
trap 'VERDICT=interrupted; exit 143' TERM
trap 'VERDICT=interrupted; exit 129' HUP

{
  date
  echo "repo=$REPO out=$OUT configs=$CONFIGS workers=$WORKERS limit_per_worker=$LIMIT_PER_WORKER"
  echo "flags=${FLAGS[*]}"
  git -C "$REPO" log --oneline -1
  echo "nproc=$(nproc)"
  nvidia-smi
  rustc --version
  cargo --version
  python3 --version
  sha256sum "$H2"
  echo "usr_bin_time=$([ -x /usr/bin/time ] && echo yes || echo no)"
  env | grep -E '^(QDK_|CUDA|CARGO)'
} > "$OUT/environment.txt" 2>&1

# --- h2-bytecode: the one every earlier run used -------------------------------
if [ ! -f "$H2" ] || [ "$(sha256sum "$H2" | awk '{print $1}')" != "$H2_SHA256" ]; then
  summary h2-bytecode - 0 "verdict=fail path=$H2 sha256_expected=$H2_SHA256 (nothing else runs)"
  VERDICT=FAIL
  exit 1
fi
summary h2-bytecode - 0 "verdict=pass path=$H2"

# --- build (release), before any shot, so a compile error fails fast ------------
run_part "$BUILD_BOX" "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" \
  --release $CARGO_FLAGS -p qdk_simulators --example h2_dev
summary build "$STATUS" "$SECONDS_TAKEN" "verdict=$([ "$STATUS" = 0 ] && echo pass || echo fail) h2_dev=$BIN/h2_dev (build.log)"
if [ "$STATUS" != 0 ]; then
  VERDICT=FAIL
  exit 1
fi

# --- no MPS daemon left from an earlier run ---------------------------------------
quit_any_mps
summary mps-clear - 0 "verdict=$([ "$MPS_LEFT" = 0 ] && echo pass || echo fail) mps_processes_left=$MPS_LEFT (mps.log)"
if [ "$MPS_LEFT" != 0 ]; then
  # A T configuration would run as an MPS client without knowing it.
  VERDICT=FAIL
  exit 1
fi

# --- The configurations ------------------------------------------------------------
FAILED_CONFIGS=
FIRST_CONFIG=
RATES=
for id in $CONFIGS; do
  [[ $id =~ $ID_PATTERN ]]
  kind=${BASH_REMATCH[1]}
  mode=${BASH_REMATCH[2]}
  n=${BASH_REMATCH[4]:-$WORKERS}
  extra=()
  allocation=cudaMalloc
  sync=spin
  case "$mode" in
  async)
    extra=(--stream-ordered-allocation)
    allocation=stream-ordered
    ;;
  async-block)
    extra=(--stream-ordered-allocation --blocking-sync)
    allocation=stream-ordered
    sync=blocking
    ;;
  esac
  LIMIT=$((LIMIT_PER_WORKER * n))
  dir=$OUT/$id
  mkdir -p "$dir"
  mps=no
  if [ "$kind" = P ]; then
    mps=yes
    start_mps
    if [ "$STATUS" != 0 ]; then
      summary "$id" "$STATUS" 0 "verdict=fail the MPS daemon did not start (mps.log)"
      FAILED_CONFIGS=$FAILED_CONFIGS,$id
      quit_any_mps
      continue
    fi
  fi

  gpu_before=$(gpu_memory_used)
  nvidia-smi --query-gpu=timestamp,memory.used,utilization.gpu --format=csv,noheader,nounits \
    -l "$GPU_INTERVAL" > "$dir/gpu.csv" 2>&1 &
  MONITOR_PID=$!
  # A builtin with a redirection runs in this shell, so it reads this
  # shell's children (a command substitution would fork and read zeros).
  times > "$dir/times-before.txt"
  start=$SECONDS
  WORKER_PIDS=()
  if [ "$kind" = T ]; then
    start_worker 0 "$dir" "$dir/threads.log" --first-shot 0 --shots "$n" --threads "$n" "${extra[@]}"
  else
    for ((i = 0; i < n; i++)); do
      start_worker "$i" "$dir" "$dir/shot-$i.log" --first-shot "$i" --shots 1 "${extra[@]}"
    done
  fi
  exits=
  timed_out=no
  for pid in "${WORKER_PIDS[@]}"; do
    wait "$pid"
    status=$?
    exits=$exits,$status
    [ "$status" = 124 ] && timed_out=yes
  done
  WORKER_PIDS=()
  wall=$((SECONDS - start))
  times > "$dir/times-after.txt"
  stop_monitor
  if [ "$kind" = P ]; then
    quit_any_mps
    summary "$id-mps-quit" - 0 "verdict=$([ "$MPS_LEFT" = 0 ] && echo pass || echo fail) mps_processes_left=$MPS_LEFT (mps.log)"
  fi

  # CPU: the children's seconds, after less before.
  read -r user_before system_before < <(children_seconds "$dir/times-before.txt")
  read -r user_after system_after < <(children_seconds "$dir/times-after.txt")
  read -r cpu_user cpu_system cores < <(awk -v ub="$user_before" -v sb="$system_before" \
    -v ua="$user_after" -v sa="$system_after" -v wall="$wall" \
    'BEGIN {u = ua - ub; s = sa - sb; printf "%.1f %.1f %.2f\n", u, s, (wall > 0 ? (u + s) / wall : 0)}')
  switches=$(cat "$dir"/time-*.txt 2>/dev/null | awk -F': ' '
    /Voluntary context switches/ {v += $2; seen = 1}
    /Involuntary context switches/ {iv += $2}
    /Maximum resident set size/ {if ($2 + 0 > rss) rss = $2 + 0}
    END {if (seen) printf "voluntary_switches=%d involuntary_switches=%d max_rss_kib=%d", v, iv, rss
         else printf "voluntary_switches=- involuntary_switches=- max_rss_kib=-"}')
  # GPU: the largest memory.used less the reading before; the mean
  # utilisation.
  read -r gpu_peak gpu_utilisation < <(awk -F', *' -v before="${gpu_before:-}" '
    $2 ~ /^[0-9]+$/ {if ($2 + 0 > m) m = $2 + 0; u += $3; k++}
    END {printf "%s %s\n", (before == "" || k == 0) ? "-" : m - before, k ? sprintf("%.0f%%", u / k) : "-"}' "$dir/gpu.csv")
  blocking=$(cat "$dir"/threads.log "$dir"/shot-*.log 2>/dev/null | grep -m1 -o '^BLOCKING_SYNC .*')
  blocking=${blocking#BLOCKING_SYNC }

  logs=("$dir"/threads.log "$dir"/shot-*.log)
  existing=()
  for log in "${logs[@]}"; do
    [ -f "$log" ] && existing+=("$log")
  done
  python3 "$SUMMARIZE" --tvd "$TVD" --table "$TABLE" --first-wave "$n" --expect-shots "$n" \
    "${existing[@]}" > "$dir/h2-summary.txt" 2>&1
  counted=$(grep -m1 '^shots: ' "$dir/h2-summary.txt")
  shots=$(printf '%s\n' "$counted" | sed -nE 's/^shots: ([0-9]+),.*/\1/p')
  failed=$(printf '%s\n' "$counted" | sed -nE 's/^shots: [0-9]+, failed: ([0-9]+).*/\1/p')
  per_shot=$(grep -m1 '^per shot ' "$dir/h2-summary.txt" | sed -E 's/.*: //')
  throughput=$(grep -m1 '^throughput: ' "$dir/h2-summary.txt")
  rate=$(printf '%s\n' "$throughput" | sed -nE 's/.*first wave [0-9]+ shots, ([0-9.]+) shots\/h.*/\1/p')
  whole=$(printf '%s\n' "$throughput" | sed -nE 's/.*whole run ([0-9.]+) shots\/h.*/\1/p')
  seconds=$(grep -m1 '^seconds per shot: ' "$dir/h2-summary.txt" | sed -E 's/^seconds per shot: //; s/ to /-/')
  g_test=$(grep -m1 '^G = ' "$dir/h2-summary.txt" | sed -E 's/ \(20000 draws\)//')
  read -r ratio ratio_no_head < <(awk -v r="${rate:-}" -v a="$BASELINE" -v b="$BASELINE_NO_HEAD_START" \
    'BEGIN {if (r == "") print "- -"; else printf "%.2f %.2f\n", r / a, r / b}')

  verdict=pass
  [[ ${exits#,} =~ ^0(,0)*$ ]] || verdict=fail
  [ "${shots:-}" = "$n" ] && [ "${failed:-}" = 0 ] && [ "$per_shot" = "all hold" ] || verdict=fail
  [ "$verdict" = pass ] || FAILED_CONFIGS=$FAILED_CONFIGS,$id

  records=-
  if [ -z "$FIRST_CONFIG" ]; then
    FIRST_CONFIG=$id
    records_note="records_reference=yes"
  else
    records=$(LC_ALL=C join <(records_of "$OUT/$FIRST_CONFIG") <(records_of "$dir") |
      awk '{n++; if ($2 == $3) same++} END {printf "%d/%d", same, n}')
    records_note="records_vs_$FIRST_CONFIG=$records"
  fi

  summary "$id" "${exits#,}" "$wall" \
    "verdict=$verdict kind=$([ "$kind" = T ] && echo threads || echo processes) workers=$n allocation=$allocation sync=$sync mps=$mps limit=$LIMIT timed_out=$timed_out shots=${shots:--} failed=${failed:--} per_shot=\"${per_shot:--}\" shots_per_hour=${rate:--} vs_$BASELINE=$ratio vs_$BASELINE_NO_HEAD_START=$ratio_no_head whole_run_shots_per_hour=${whole:--} seconds_per_shot=${seconds:--} cpu_user_seconds=$cpu_user cpu_system_seconds=$cpu_system cpu_cores_busy=$cores $switches gpu_memory_peak_mib=$gpu_peak gpu_utilisation_mean=$gpu_utilisation${blocking:+ blocking_sync=\"$blocking\"} $records_note g_test=\"${g_test:--}\" ($id/)"
  TABLE_LINES+=("$(printf '%-18s %-8s %-6s %10s %8s %8s %9s %9s %9s' "$id" "$n" "$verdict" "${rate:--}" "$ratio" "$ratio_no_head" "$cores" "$gpu_peak" "$records")")
  RATES="$RATES $id=${rate:--}"
done
RATES=${RATES# }

if [ -n "$FAILED_CONFIGS" ]; then
  VERDICT="FAIL(${FAILED_CONFIGS#,})"
  exit 1
fi
VERDICT=PASS
exit 0
