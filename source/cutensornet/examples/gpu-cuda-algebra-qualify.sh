#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# Development only (not for review): the A100 qualification of
# qdk_cutensornet::CudaMpsEngine, canonical_mps's engine on CudaAlgebra,
# with the engine's own fusion (windows of up to three sites, far ones as
# MPOs), before its push. Every GPU result is checked against the CPU
# engine (canonical_mps::CpuMpsEngine, the same fused engine) on the same
# input, computed beforehand and shipped in INPUTS, and every time is set
# beside the dev line's fused CudaMpsEngine on the same A100 (its runs of
# 2026-11-12 to 2026-11-15), with run 1's (the engine gate by gate, on the
# same A100, 2026-11-16) beside it.
#
# The parts, in the order they run (PARTS selects among them), each with
# one line in summary.txt and its own log, with ESTIMATED durations from
# the dev line's A100 runs:
#
#   inputs      always: INPUTS' files against INPUTS/SHA256SUMS; on a
#               fail nothing else runs.
#   build       the release build of the drivers (h2_replay,
#               polyene_qpe_prefix, mps_prep_check) and of the GPU tests'
#               binary. Limit 1 h; 10-25 min on a fresh target dir.
#   gpu-tests   tests/cuda_algebra_on_the_gpu.rs, its #[ignore] tests
#               (a split and a move against the reference algebra, their
#               isometries, bonds and cutoffs; the engine against the
#               reference and the CPU). Pass: every one of them passes.
#               Limit 30 min; a few minutes.
#   sanitizer   NVIDIA's compute-sanitizer, memcheck with --leak-check full,
#               then synccheck, on two programs:
#               - SANITIZER_TESTS of the same binary (a split, a move, and
#                 the engine on a short circuit);
#               - the fused engine on a short polyene prefix
#                 (polyene_qpe_prefix SANITIZER_POLYENE: N = 8, chi 16, two
#                 steps; 1424 splits, 278 MPOs, 110 leftward sweeps, spans
#                 up to 4 chi = 64 on the CPU), so that the leftward
#                 sweep's shapes (Us QR moves at span bonds up to 4 chi,
#                 which the dev line never ran) run under the sanitizer;
#                 its log is compared with the CPU's (INPUTS/sanitizer):
#                 c_p, the energy and the work counts, leftward sweeps
#                 included.
#               Pass: each ERROR SUMMARY counts 0 errors, the tests pass,
#               and the prefix agrees with the CPU's with leftward_sweeps
#               above 0. Limit 1 h per run; 5-40 min each (the sanitizer
#               slows CUDA calls 10-100 times). When missing, its line
#               says verdict=missing with the install command.
#   h2          recorded H2 shots replayed on the GPU (h2_replay), each
#               dump against the CPU engine's dump of the same recording:
#               every projection probability, the retained weight and the
#               readout distribution within H2_TOLERANCE, the work
#               counts (splits, MPOs, centre moves, leftward sweeps)
#               equal, since they follow from the gates alone. Its steps:
#                 h2-solo      shot 0 alone, without the MPS daemon: one
#                              process's time (run 1: 1147 s, 313,350
#                              splits; the dev line's h2_dev shot, a
#                              different fusion of the full program: 301 s,
#                              83,322 splits; this replay: 57,643 splits on
#                              the CPU). About 3-5 min.
#                 mps-start    the CUDA MPS daemon (NVIDIA's Multi-Process
#                              Service, which lets processes share the
#                              GPU at once), its pipe and log under
#                              $HOME/mps.
#                 calibration  shot 0 alone for CALIBRATION_SECONDS, the
#                              GPU memory sampled: one process's peak.
#                 procs        P, the processes at once: the smallest of
#                              nproc less 2 (each waits on a core),
#                              80% of free GPU memory / the peak, 48 (the
#                              MPS client limit) and SHOTS; PROCS
#                              overrides it.
#                 shots        SHOTS shots (default 50), shot i replaying
#                              recording i mod R (R recordings in INPUTS,
#                              6 today), at most P at once.
#                 mps-quit     the daemon stopped (on every exit path).
#                 h2-compare   the dumps against the CPU's; the first
#                              wave's rate (the first P shots to start)
#                              against the dev line's 60.6 shots/h (slow
#                              below it), run 1's 13.2 shots/h beside it.
#               About 1 h in all at the dev line's rate (run 1: 3 h 47 min).
#               The fused engine sweeps an MPO's span from its end nearer
#               the next site, so it sweeps some leftward (Us QR moves,
#               spans up to 4 chi; on the CPU, shot 0: 3,393 of 15,006
#               MPOs, the largest span bond 144); each dump's leftward
#               count shows they ran, and must equal the CPU's.
#   h2-lowsweep shot 0 alone again, with P6_LOW_SWEEP_ONLY=1: every
#               span swept from its low end, as the dev line did (a
#               measurement switch of this dev branch, off by default),
#               against the CPU's dump made with the same switch
#               (INPUTS/h2/cpu-lowsweep; leftward 0), its time beside
#               h2-solo's (versus_ratio: low-sweep / default). On the CPU
#               at chi 128 the default was 5% faster (16.4 s against
#               17.2 s, 19% more centre moves when sweeping low). About
#               3-5 min.
#   mps-prep    #540's state preparations (mps_prep_check), MPS_CASES
#               (default n16-chi32, n20-chi64, n12-chi128), placement
#               "following": the driver's check (verdict=Agrees), the
#               energy against the CPU engine's within the check's
#               tolerance, and each phase (MPSPREP span) of at least 5 s
#               on the dev line within 10% of the dev line's time (slower:
#               verdict=slow). Dev line: 256 s, 1919 s and 4548 s, about
#               1 h 52 min; each limited to 3 times its dev time.
#   polyene     the polyene QPE prefix at N = 16 and N = 20, chi 128, the
#               gates sent one by one (the driver's variant base) and fused
#               by the engine: every c_p read and the final energy within
#               1e-4 of the CPU engine's, the driver's verdict against its
#               reference the CPU's, the work counts equal to the CPU's;
#               the prefix's time against the dev line's fused run
#               (seq3-mpo: 1409 s and 2010 s), slow when more than 10%
#               longer, its work counts beside the dev line's (N = 16:
#               91,386 splits, 9,776 MPOs, 104,110 QR moves), run 1's
#               time beside it (N = 16: 5042 s). About 57 min; each
#               limited to 4 times the dev line's time.
#
# Expected in all: about 4 h (build 20 min, gpu-tests 5, sanitizer 50,
# h2 60, h2-lowsweep 5, mps-prep 112, polyene 57), less if #540's
# preparations gain from the fusion. BUDGET_SECONDS (default 32400, 9 h) bounds the whole run: a
# part that would start past it is skipped (verdict=skipped-budget), and
# each run's limit is cut to what is left.
#
# The last line printed gives the verdict: PASS when every part passed;
# FAIL (exit 1) when one failed; SLOW (exit 4) when none failed but a
# #540 phase, H2's rate or a polyene prefix missed the dev line's time;
# INCOMPLETE (exit 3) when a part could not run (a missing tool, the
# budget).
#
# Before it starts, the script checks that the GPU is free: no compute
# process in nvidia-smi and no gpu-polyene-full script running (FORCE=1
# skips the check).
#
# Layout: the tarball unpacks into one directory holding repo/ (this
# source) and inputs/ (the cases and references). Run from anywhere:
#
#   nohup bash <dir>/repo/source/cutensornet/examples/gpu-cuda-algebra-qualify.sh > <dir>/run.log 2>&1 &
#
# DRY_RUN=1 prints each part's commands without running them, nor the
# GPU and input checks. Send back $OUT.tar.gz (its path is printed at the
# end).
#
# Environment (all optional):
#   OUT              the output directory (default <dir>/out-<date>)
#   INPUTS           the inputs (default <dir>/inputs)
#   PARTS            among build gpu-tests sanitizer h2 h2-lowsweep
#                    mps-prep polyene (default all, in that order)
#   BUDGET_SECONDS   the whole run's budget (default 32400)
#   DRY_RUN          1: print the commands only
#   FORCE            1: skip the free-GPU check
#   SANITIZER_TESTS  the tests the sanitizer runs (above)
#   SANITIZER_POLYENE  0: the sanitizer skips the polyene prefix
#   COMPUTE_SANITIZER  compute-sanitizer's path, instead of the search
#   SHOTS            H2's shots under MPS (default 50)
#   PROCS            the processes at once, instead of the computed P
#   CALIBRATION_SECONDS  shot 0's time alone under MPS (default 60)
#   SHOT_LIMIT       each H2 shot's time limit in seconds (default 10800)
#   H2_CHI           H2's bond cap (default 64; the CPU references ship
#                    for 64 and 128)
#   H2_TOLERANCE     H2's largest gap to the CPU (default 1e-8)
#   MPS_CASES        the #540 cases (above)
#   POLYENE_SIZES    the polyene sizes (default "16 20")
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY, QDK_CUSOLVER_LIBRARY
#                    the libraries, when neither at the pinned paths below
#                    nor in /data/domingom/tn-venv
#   CARGO_TARGET_DIR the build's target dir (default <dir>/repo/target)
#   CARGO_FLAGS      extra flags for cargo (for instance --offline)

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
TOP=$(dirname "$REPO")
OUT=${OUT:-$TOP/out-$(date +%Y%m%d-%H%M)}
INPUTS=${INPUTS:-$TOP/inputs}
ALL_PARTS="build gpu-tests sanitizer h2 h2-lowsweep mps-prep polyene"
PARTS=${PARTS:-$ALL_PARTS}
BUDGET_SECONDS=${BUDGET_SECONDS:-32400}
DRY_RUN=${DRY_RUN:-0}
FORCE=${FORCE:-0}
SANITIZER_TESTS=${SANITIZER_TESTS:-a_split_gives_the_reference_algebras_truncated_svd a_move_gives_the_reference_algebras_product_and_bond the_engine_on_the_gpu_agrees_with_the_reference}
SANITIZER_POLYENE=${SANITIZER_POLYENE:-1}
# The fused prefix the sanitizer runs, and its CPU log in INPUTS/sanitizer.
SANITIZER_PREFIX=(--n 8 --chi 16 --order interleave-centre --variant base --steps 2)
SANITIZER_PREFIX_CPU=cpu-n8-chi16-steps2.log
SHOTS=${SHOTS:-50}
PROCS=${PROCS:-}
CALIBRATION_SECONDS=${CALIBRATION_SECONDS:-60}
SHOT_LIMIT=${SHOT_LIMIT:-10800}
H2_CHI=${H2_CHI:-64}
H2_TOLERANCE=${H2_TOLERANCE:-1e-8}
MPS_CASES=${MPS_CASES:-n16-chi32 n20-chi64 n12-chi128}
POLYENE_SIZES=${POLYENE_SIZES:-16 20}
TN_VENV=/data/domingom/tn-venv
CARGO_FLAGS=${CARGO_FLAGS:-}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$REPO/target}
BIN=$CARGO_TARGET_DIR/release/examples
COMPARE=$REPO/source/cutensornet/examples/cuda_algebra_compare.py
GPU_TESTS=$REPO/source/cutensornet/tests/cuda_algebra_on_the_gpu.rs
SUMMARY=$OUT/summary.txt
BUILD_BOX=3600
TESTS_BOX=1800
SANITIZER_BOX=3600
# compute-sanitizer's package in NVIDIA's CUDA 12.9 apt repository.
SANITIZER_INSTALL="sudo apt-get install -y cuda-sanitizer-12-9"
# The acceptance's site order for H2 (qubit q on site ORDER[q]), the one
# its CPU dumps used.
H2_ORDER=26,25,27,24,21,6,8,4,3,18,17,15,16,1,5,28,23,22,20,19,10,2,0,11,12,14,13,9,7
# The dev line's CudaMpsEngine H2 throughput, its first wave of 22
# processes under MPS (2026-11-13); its h2_dev shot alone and that shot's
# work (seq3-mpo, χ 128, 2026-11-13): informational, since h2_dev fuses
# the full program differently from the engine's replay.
H2_DEV_PER_HOUR=60.6
H2_DEV_SOLO_SECONDS=301
H2_DEV_WORK=(--dev-splits 83322 --dev-mpos 17939 --dev-moves 93090)
# Run 1's H2 (the engine gate by gate, 2026-11-16): its whole run's rate
# under MPS and shot 0 alone.
H2_RUN1_PER_HOUR=13.2
H2_RUN1_SOLO_SECONDS=1147
CALIBRATION_INTERVAL=5
CPU_RESERVE=2
GPU_MEMORY_PERCENT=80
FALLBACK_PEAK_MIB=800
MPS_CLIENT_LIMIT=48
# The dev line's A100 seconds for each #540 case (following), its limit
# three times that.
declare -A MPS_DEV_SECONDS=([n16-chi32]=256 [n20-chi64]=1919 [n12-chi128]=4548)
# The polyene's reference per size: the driver's own check.
declare -A POLYENE_REFERENCE=([16]=ppp16.cpu-ic-seq3-chi128.reference [20]=ppp20.cpu-ic-seq3-chi256.reference)
# The dev line's fused (seq3-mpo) polyene prefix seconds on the A100, the
# limit four times that; run 1's (gate by gate, N = 16 only).
declare -A POLYENE_DEV_SECONDS=([16]=1409 [20]=2010)
declare -A POLYENE_RUN1_SECONDS=([16]=5041.5 [20]=-)
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
positive BUDGET_SECONDS "$BUDGET_SECONDS"
positive H2_CHI "$H2_CHI"
[ -z "$PROCS" ] || positive PROCS "$PROCS"
for part in $PARTS; do
  case " $ALL_PARTS " in
  *" $part "*) ;;
  *)
    echo "PARTS=$PARTS: $part is not among $ALL_PARTS" >&2
    exit 2
    ;;
  esac
done
for case_name in $MPS_CASES; do
  if [ -z "${MPS_DEV_SECONDS[$case_name]:-}" ]; then
    echo "MPS_CASES=$MPS_CASES: $case_name is not among ${!MPS_DEV_SECONDS[*]}" >&2
    exit 2
  fi
done
for n in $POLYENE_SIZES; do
  if [ -z "${POLYENE_REFERENCE[$n]:-}" ]; then
    echo "POLYENE_SIZES=$POLYENE_SIZES: $n is not among ${!POLYENE_REFERENCE[*]}" >&2
    exit 2
  fi
done

# --- The libraries -----------------------------------------------------------
# The pinned paths of the dev line's runs, then the tn-venv's wheels;
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

mkdir -p "$OUT/h2/solo" "$OUT/h2/shots" "$OUT/h2/lowsweep" "$OUT/mps-prep" "$OUT/polyene"
: > "$SUMMARY"

# --- Helpers -----------------------------------------------------------------
dry() { [ "$DRY_RUN" = 1 ]; }

# One line in summary.txt: the part, its exit code, its seconds and what it
# found.
summary() {
  printf 'part=%s exit=%s seconds=%s %s\n' "$1" "$2" "$3" "$4" | tee -a "$SUMMARY"
}

# The seconds left in the budget.
left() {
  echo $((BUDGET_SECONDS - SECONDS))
}

# The smaller of $1 and the seconds left in the budget.
capped() {
  local rest
  rest=$(left)
  if [ "$1" -lt "$rest" ]; then echo "$1"; else echo "$rest"; fi
}

# Whether part or run $1 may start: false, with its summary line, when the
# budget is spent.
within_budget() {
  if [ "$(left)" -lt 60 ]; then
    summary "$1" - 0 "verdict=skipped-budget budget_seconds=$BUDGET_SECONDS elapsed=$SECONDS"
    MISSING_PARTS=$MISSING_PARTS,$1
    return 1
  fi
}

# Runs "$@" under a time limit of $1 seconds, its output appended to $2;
# sets STATUS and SECONDS_TAKEN. In the foreground, so that a Ctrl-C stops
# it and the script at once. Under DRY_RUN, prints the command only.
run_part() {
  local limit=$1 log=$2
  shift 2
  if dry; then
    echo "DRY limit=${limit}s log=$log: $*"
    STATUS=0
    SECONDS_TAKEN=0
    return
  fi
  local start=$SECONDS
  echo "== $(date '+%F %T') limit=${limit}s $*" | tee -a "$log"
  timeout --foreground -k 60 "$limit" "$@" >> "$log" 2>&1
  STATUS=$?
  SECONDS_TAKEN=$((SECONDS - start))
  echo "== $(date '+%F %T') exit=$STATUS seconds=$SECONDS_TAKEN" | tee -a "$log"
}

# Runs the comparison "$@" (cuda_algebra_compare.py's arguments); sets
# COMPARED to its line and VERDICT_OF to its verdict.
compare() {
  if dry; then
    echo "DRY python3 $COMPARE $*"
    COMPARED="(dry run)"
    VERDICT_OF=pass
    return
  fi
  COMPARED=$(python3 "$COMPARE" "$@" 2>&1 | tail -1)
  VERDICT_OF=$(printf '%s\n' "$COMPARED" | grep -o 'verdict=[a-z-]*$' | cut -d= -f2)
  [ -n "$VERDICT_OF" ] || VERDICT_OF=fail
}

# Records verdict $2 of part $1 among the failed or slow parts.
record() {
  case "$2" in
  pass) ;;
  slow) SLOW_PARTS=$SLOW_PARTS,$1 ;;
  missing) MISSING_PARTS=$MISSING_PARTS,$1 ;;
  *) FAILED_PARTS=$FAILED_PARTS,$1 ;;
  esac
}

# Whether part $1 is in PARTS.
wanted() {
  case " $PARTS " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

FAILED_PARTS=
SLOW_PARTS=
MISSING_PARTS=

# Sets SANITIZER to compute-sanitizer's path (COMPUTE_SANITIZER, PATH,
# then the CUDA toolkits' directories), empty when none is found.
find_sanitizer() {
  SANITIZER=${COMPUTE_SANITIZER:-}
  [ -n "$SANITIZER" ] && return
  SANITIZER=$(command -v compute-sanitizer)
  [ -n "$SANITIZER" ] && return
  local candidate
  for candidate in /usr/local/cuda*/bin/compute-sanitizer \
    /usr/local/cuda*/compute-sanitizer/compute-sanitizer; do
    if [ -x "$candidate" ]; then
      SANITIZER=$candidate
      return
    fi
  done
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

# The recordings, sorted by their shot number.
RECORDINGS=()
while IFS= read -r recording; do
  RECORDINGS+=("$recording")
done < <(find "$INPUTS/h2" -name 'h2-seed1-shot*.replay' 2>/dev/null | sort -V)

# h2_replay's command for shot $1 (recording $1 mod R).
shot_command() {
  echo "$BIN/h2_replay" --engine cuda-mps --chi "$H2_CHI" --order "$H2_ORDER" \
    "${RECORDINGS[$(($1 % ${#RECORDINGS[@]}))]}"
}

# Starts shot $1 in the background, its output in h2/shots/shot-$1.log.
# `timeout` puts the shot in its own process group, so a Ctrl-C reaches
# the script alone, whose exit stops the shots.
SHOT_PIDS=()
start_shot() {
  local recording=${RECORDINGS[$(($1 % ${#RECORDINGS[@]}))]}
  timeout -k 60 "$(capped "$SHOT_LIMIT")" "$BIN/h2_replay" --engine cuda-mps --chi "$H2_CHI" \
    --order "$H2_ORDER" "$recording" < /dev/null > "$OUT/h2/shots/shot-$1.log" 2>&1 &
  SHOT_PIDS[$1]=$!
  echo "$(date '+%F %T') start shot=$1 epoch=$(date +%s.%N) pid=$! recording=$(basename "$recording")" \
    >> "$OUT/h2/schedule.log"
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
  [ "${#SHOT_PIDS[@]}" = 0 ] && return
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
  local start=$SECONDS status left_processes waited=0
  echo quit | nvidia-cuda-mps-control >> "$OUT/h2/mps.log" 2>&1
  status=$?
  MPS_STARTED=no
  # The daemon and its server, this user's, as pgrep names them (comm, 15
  # characters: nvidia-cuda-mps).
  left_processes=$(pgrep -u "$UID" -c nvidia-cuda-mps)
  while [ "${left_processes:-0}" -gt 0 ] && [ "$waited" -lt 15 ]; do
    sleep 1
    waited=$((waited + 1))
    left_processes=$(pgrep -u "$UID" -c nvidia-cuda-mps)
  done
  pgrep -a -u "$UID" nvidia-cuda-mps >> "$OUT/h2/mps.log" 2>&1
  summary mps-quit "$status" "$((SECONDS - start))" \
    "verdict=$([ "${left_processes:-0}" = 0 ] && echo pass || echo fail) mps_processes_left=${left_processes:-0} (h2/mps.log)"
}

# The verdict of the parts run so far.
parts_verdict() {
  if [ -n "$FAILED_PARTS" ]; then
    echo FAIL
  elif [ -n "$SLOW_PARTS" ]; then
    echo SLOW
  elif [ -n "$MISSING_PARTS" ]; then
    echo INCOMPLETE
  else
    echo PASS
  fi
}

# Every exit path: stops the shots, the GPU monitor and the MPS daemon,
# packs the outputs and prints the verdict line.
VERDICT=incomplete
MONITOR_PID=
finish() {
  local rc=$?
  trap - EXIT INT TERM HUP
  stop_shots
  if [ -n "$MONITOR_PID" ]; then
    kill "$MONITOR_PID" 2>/dev/null
    wait "$MONITOR_PID" 2>/dev/null
    local peak
    peak=$(awk -F', ' 'NR > 1 {sub(/ MiB/, "", $2); if ($2 + 0 > max) max = $2 + 0} END {print max + 0}' \
      "$OUT/gpu-memory.csv")
    summary memory - - "gpu_memory_peak_mib=$peak (gpu-memory.csv)"
  fi
  quit_mps
  tar czf "$OUT.tar.gz" -C "$(dirname "$OUT")" "$(basename "$OUT")"
  echo "Send back: $OUT.tar.gz"
  echo "== Summary ($SECONDS s)"
  cat "$SUMMARY"
  echo "RESULT verdict=$VERDICT failed=${FAILED_PARTS#,} slow=${SLOW_PARTS#,} missing=${MISSING_PARTS#,} wall_seconds=$SECONDS out=$OUT.tar.gz"
  exit "$rc"
}
trap finish EXIT
trap 'VERDICT=interrupted; exit 130' INT
trap 'VERDICT=interrupted; exit 143' TERM
trap 'VERDICT=interrupted; exit 129' HUP

# --- The GPU is free ---------------------------------------------------------
if ! dry && [ "$FORCE" != 1 ]; then
  others=$(pgrep -af gpu-polyene-full)
  apps=$(nvidia-smi --query-compute-apps=pid,process_name,used_memory --format=csv,noheader 2>&1)
  if [ -n "$others" ] || [ -n "$apps" ]; then
    summary gpu-free - 0 "verdict=fail the GPU is busy: gpu-polyene-full=[${others//$'\n'/; }] compute_apps=[${apps//$'\n'/; }] (wait for it to end, or FORCE=1)"
    VERDICT=FAIL
    exit 2
  fi
fi

{
  date
  echo "repo=$REPO inputs=$INPUTS out=$OUT parts=$PARTS budget_seconds=$BUDGET_SECONDS"
  echo "shots=$SHOTS procs=${PROCS:-auto} h2_chi=$H2_CHI h2_tolerance=$H2_TOLERANCE mps_cases=$MPS_CASES polyene_sizes=$POLYENE_SIZES"
  echo "commit=$(cat "$INPUTS/COMMIT" 2>/dev/null)"
  echo "nproc=$(nproc)"
  nvidia-smi
  rustc --version
  cargo --version
  python3 --version
  env | grep -E '^(QDK_|CUDA|CARGO)'
} > "$OUT/environment.txt" 2>&1

# --- inputs: the shipped files are intact ------------------------------------
if dry; then
  echo "DRY sha256sum --quiet -c SHA256SUMS (in $INPUTS)"
else
  start=$SECONDS
  (builtin cd "$INPUTS" && sha256sum --quiet -c SHA256SUMS) > "$OUT/inputs.log" 2>&1
  STATUS=$?
  summary inputs "$STATUS" "$((SECONDS - start))" \
    "verdict=$([ "$STATUS" = 0 ] && echo pass || echo fail) files=$(wc -l < "$INPUTS/SHA256SUMS") recordings=${#RECORDINGS[@]} (inputs.log)"
  if [ "$STATUS" != 0 ]; then
    VERDICT=FAIL
    exit 1
  fi
fi

# --- build -------------------------------------------------------------------
TEST_BIN=
if wanted build && within_budget build; then
  log=$OUT/build.log
  # shellcheck disable=SC2086
  run_part "$(capped "$BUILD_BOX")" "$log" cargo build --manifest-path "$REPO/Cargo.toml" --release \
    $CARGO_FLAGS -p qdk_cutensornet --example h2_replay --example polyene_qpe_prefix \
    --example mps_prep_check
  examples_status=$STATUS
  examples_seconds=$SECONDS_TAKEN
  # shellcheck disable=SC2086
  run_part "$(capped "$BUILD_BOX")" "$log" cargo test --manifest-path "$REPO/Cargo.toml" --release \
    $CARGO_FLAGS -p qdk_cutensornet --test cuda_algebra_on_the_gpu --no-run \
    --message-format=json-render-diagnostics
  tests_status=$STATUS
  if dry; then
    TEST_BIN="$CARGO_TARGET_DIR/release/deps/cuda_algebra_on_the_gpu-<hash>"
  else
    TEST_BIN=$(grep -o '"executable":"[^"]*/cuda_algebra_on_the_gpu-[0-9a-f]*"' "$log" | tail -1 |
      sed -e 's/^"executable":"//' -e 's/"$//')
  fi
  verdict=fail
  [ "$examples_status" = 0 ] && [ "$tests_status" = 0 ] && [ -n "$TEST_BIN" ] && verdict=pass
  summary build "$examples_status,$tests_status" "$((examples_seconds + SECONDS_TAKEN))" \
    "verdict=$verdict examples=$BIN test_binary=${TEST_BIN:--} (build.log)"
  record build "$verdict"
  if [ "$verdict" != pass ]; then
    VERDICT=FAIL
    exit 1
  fi
elif ! wanted build; then
  # The binaries of an earlier build in the same target dir.
  TEST_BIN=$(find "$CARGO_TARGET_DIR/release/deps" -maxdepth 1 -type f -executable \
    -name 'cuda_algebra_on_the_gpu-*' -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)
fi

if ! dry; then
  nvidia-smi --query-gpu=timestamp,memory.used,memory.total,utilization.gpu --format=csv -l 5 \
    > "$OUT/gpu-memory.csv" 2>&1 &
  MONITOR_PID=$!
fi

# --- gpu-tests ---------------------------------------------------------------
if wanted gpu-tests && within_budget gpu-tests; then
  log=$OUT/gpu-tests.log
  expected=$(grep -c '^#\[ignore' "$GPU_TESTS")
  if [ -z "$TEST_BIN" ]; then
    summary gpu-tests - 0 "verdict=fail no test binary (run the build part)"
    record gpu-tests fail
  else
    run_part "$(capped "$TESTS_BOX")" "$log" "$TEST_BIN" --ignored --nocapture --test-threads=1
    result=$(grep -o 'test result: .*' "$log" 2>/dev/null | tail -1)
    passed=$(printf '%s\n' "$result" | sed -nE 's/.* ([0-9]+) passed.*/\1/p')
    failed=$(printf '%s\n' "$result" | sed -nE 's/.* ([0-9]+) failed.*/\1/p')
    failed_tests=$(awk '/^failures:$/ {n++; next} n == 2 && /^    [a-z_]/ {printf "%s%s", sep, $1; sep = ","}' "$log" 2>/dev/null)
    verdict=fail
    if dry || { [ "$STATUS" = 0 ] && [ "${passed:-0}" = "$expected" ]; }; then
      verdict=pass
    fi
    summary gpu-tests "$STATUS" "$SECONDS_TAKEN" \
      "verdict=$verdict passed=${passed:--} expected=$expected failed=${failed:--}${failed_tests:+ ($failed_tests)} (gpu-tests.log)"
    record gpu-tests "$verdict"
  fi
fi

# --- sanitizer: memcheck and synccheck on a few GPU tests --------------------
if wanted sanitizer && within_budget sanitizer; then
  find_sanitizer
  dry && [ -z "$SANITIZER" ] && SANITIZER=compute-sanitizer
  if [ -z "$SANITIZER" ]; then
    summary sanitizer - 0 \
      "verdict=missing searched=COMPUTE_SANITIZER,PATH,/usr/local/cuda*/bin,/usr/local/cuda*/compute-sanitizer install=\"$SANITIZER_INSTALL\" (then run PARTS=sanitizer again)"
    record sanitizer missing
  elif [ -z "$TEST_BIN" ]; then
    summary sanitizer - 0 "verdict=fail no test binary (run the build part)"
    record sanitizer fail
  else
    dry || "$SANITIZER" --version > "$OUT/sanitizer-version.txt" 2>&1
    start=$SECONDS
    verdict=pass
    statuses=
    details=
    for tool in memcheck synccheck; do
      log=$OUT/sanitizer-$tool.log
      options=(--tool "$tool")
      [ "$tool" = memcheck ] && options+=(--leak-check full)
      # shellcheck disable=SC2086
      run_part "$(capped "$SANITIZER_BOX")" "$log" "$SANITIZER" "${options[@]}" "$TEST_BIN" \
        --ignored --exact $SANITIZER_TESTS --nocapture --test-threads=1
      errors=$(grep -o 'ERROR SUMMARY: [0-9]* error' "$log" 2>/dev/null | tail -1 | awk '{print $3}')
      result=$(grep -o 'test result: [a-zA-Z]*' "$log" 2>/dev/null | tail -1 | awk '{print $3}')
      if ! dry && { [ "$STATUS" != 0 ] || [ "${errors:-}" != 0 ]; }; then
        verdict=fail
      fi
      statuses=$statuses,$STATUS
      details="$details ${tool}_errors=${errors:--} ${tool}_tests=${result:--}"
      [ "$SANITIZER_POLYENE" = 0 ] && continue
      # The fused prefix: its leftward sweeps under the sanitizer.
      log=$OUT/sanitizer-$tool-polyene.log
      run_part "$(capped "$SANITIZER_BOX")" "$log" "$SANITIZER" "${options[@]}" \
        "$BIN/polyene_qpe_prefix" --engine cuda-mps "${SANITIZER_PREFIX[@]}"
      errors=$(grep -o 'ERROR SUMMARY: [0-9]* error' "$log" 2>/dev/null | tail -1 | awk '{print $3}')
      compare polyene --gpu "$log" --cpu "$INPUTS/sanitizer/$SANITIZER_PREFIX_CPU" \
        --table "$OUT/sanitizer-$tool-polyene.md"
      leftward=$(printf '%s\n' "$COMPARED" | grep -o 'leftward=[0-9-]*' | head -n 1 | cut -d= -f2)
      if ! dry && { [ "$STATUS" != 0 ] || [ "${errors:-}" != 0 ] || [ "$VERDICT_OF" != pass ] ||
        [ "${leftward:-0}" = - ] || [ "${leftward:-0}" -le 0 ]; }; then
        verdict=fail
      fi
      statuses=$statuses,$STATUS
      details="$details ${tool}_polyene_errors=${errors:--} ${tool}_polyene=$VERDICT_OF ${tool}_polyene_leftward=${leftward:--}"
    done
    summary sanitizer "${statuses#,}" "$((SECONDS - start))" \
      "verdict=$verdict$details tests=\"$SANITIZER_TESTS\" polyene=\"${SANITIZER_PREFIX[*]}\" sanitizer=$SANITIZER (sanitizer-memcheck.log, sanitizer-synccheck.log, sanitizer-*-polyene.log)"
    record sanitizer "$verdict"
  fi
fi

# --- h2: recorded shots, alone and under MPS ---------------------------------
if wanted h2 && within_budget h2; then
  if [ "${#RECORDINGS[@]}" = 0 ]; then
    summary h2 - 0 "verdict=fail no recordings in $INPUTS/h2"
    record h2 fail
  else
    # h2-solo: shot 0 alone, without MPS.
    log=$OUT/h2/solo/shot-0.log
    # shellcheck disable=SC2046
    run_part "$(capped "$SHOT_LIMIT")" "$log" $(shot_command 0)
    compare h2 --shots "$OUT/h2/solo" --schedule /dev/null --cpu "$INPUTS/h2/cpu" --chi "$H2_CHI" \
      --tolerance "$H2_TOLERANCE" --expect 1 --first-wave 1 --bar 0 \
      --dev-solo-seconds "$H2_DEV_SOLO_SECONDS" --run1-solo-seconds "$H2_RUN1_SOLO_SECONDS" \
      "${H2_DEV_WORK[@]}" --table "$OUT/h2/solo.md"
    [ "$STATUS" = 0 ] || VERDICT_OF=fail
    summary h2-solo "$STATUS" "$SECONDS_TAKEN" "$COMPARED (h2/solo/, h2/solo.md)"
    record h2-solo "$VERDICT_OF"
    H2_SOLO_REPLAY_SECONDS=$(printf '%s\n' "$COMPARED" | grep -o 'replay_seconds_min=[0-9.]*' | cut -d= -f2)

    # mps-start: any daemon left running, on the default pipe directory or
    # ours, stops first.
    if dry; then
      echo "DRY echo quit | nvidia-cuda-mps-control; nvidia-cuda-mps-control -d (pipe $CUDA_MPS_PIPE_DIRECTORY)"
      STATUS=0
    else
      {
        pgrep -a nvidia-cuda-mps
        echo quit | env -u CUDA_MPS_PIPE_DIRECTORY -u CUDA_MPS_LOG_DIRECTORY nvidia-cuda-mps-control
        echo quit | nvidia-cuda-mps-control
      } >> "$OUT/h2/mps.log" 2>&1
      mkdir -p "$CUDA_MPS_PIPE_DIRECTORY" "$CUDA_MPS_LOG_DIRECTORY"
      # Set before the start: a start that reports a failure may still
      # leave a daemon behind, and quitting one that is not there is
      # harmless.
      MPS_STARTED=yes
      nvidia-cuda-mps-control -d >> "$OUT/h2/mps.log" 2>&1
      STATUS=$?
    fi
    summary mps-start "$STATUS" 0 \
      "verdict=$([ "$STATUS" = 0 ] && echo pass || echo fail) pipe=$CUDA_MPS_PIPE_DIRECTORY log=$CUDA_MPS_LOG_DIRECTORY"
    if [ "$STATUS" != 0 ]; then
      record mps-start fail
    elif dry; then
      echo "DRY calibration: shot 0 alone for ${CALIBRATION_SECONDS}s, then shots 1 to $((SHOTS - 1)) at most P at once:"
      echo "DRY shot i: timeout -k 60 $SHOT_LIMIT $(shot_command 1) (recording i mod ${#RECORDINGS[@]})"
      compare h2 --shots "$OUT/h2/shots" --schedule "$OUT/h2/schedule.log" --cpu "$INPUTS/h2/cpu" \
        --chi "$H2_CHI" --tolerance "$H2_TOLERANCE" --expect "$SHOTS" --first-wave P \
        --bar "$H2_DEV_PER_HOUR" --run1-per-hour "$H2_RUN1_PER_HOUR" "${H2_DEV_WORK[@]}" \
        --table "$OUT/h2/shots.md"
    else
      # calibration: shot 0 alone under MPS.
      start=$SECONDS
      read_gpu_memory
      BASELINE=$USED
      FREE_BEFORE=$FREE
      echo "seconds,memory.used [MiB],memory.free [MiB]" > "$OUT/h2/calibration.csv"
      echo "0,$USED,$FREE" >> "$OUT/h2/calibration.csv"
      largest=$USED
      start_shot 0
      elapsed=0
      while [ "$elapsed" -lt "$CALIBRATION_SECONDS" ]; do
        sleep "$CALIBRATION_INTERVAL"
        elapsed=$((elapsed + CALIBRATION_INTERVAL))
        read_gpu_memory
        echo "$elapsed,$USED,$FREE" >> "$OUT/h2/calibration.csv"
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
      summary calibration - "$((SECONDS - start))" \
        "sampled_seconds=$elapsed baseline_mib=${BASELINE:--} largest_mib=${largest:--} free_before_mib=${FREE_BEFORE:--} peak_mib=$PEAK peak_source=$PEAK_SOURCE (h2/calibration.csv)"

      # procs: P.
      NPROC=$(nproc)
      CPU_BUDGET=$((NPROC - CPU_RESERVE))
      if [ -n "$FREE_BEFORE" ]; then
        GPU_BUDGET=$((FREE_BEFORE * GPU_MEMORY_PERCENT / 100 / PEAK))
      else
        GPU_BUDGET=$CPU_BUDGET
      fi
      P=$CPU_BUDGET
      for budget in "$GPU_BUDGET" "$MPS_CLIENT_LIMIT" "$SHOTS"; do
        [ "$budget" -lt "$P" ] && P=$budget
      done
      [ "$P" -lt 1 ] && P=1
      [ -n "$PROCS" ] && P=$PROCS
      summary procs - 0 \
        "procs=$P cpu_budget=$CPU_BUDGET (nproc $NPROC less $CPU_RESERVE) gpu_memory_budget=$GPU_BUDGET mps_client_limit=$MPS_CLIENT_LIMIT shots=$SHOTS procs_override=${PROCS:-none}"

      # shots: the rest, at most P at once.
      start=$SECONDS
      for ((shot = 1; shot < SHOTS; shot++)); do
        count_running
        while [ "$RUNNING" -ge "$P" ]; do
          sleep 2
          count_running
        done
        [ "$(left)" -lt 60 ] && break
        start_shot "$shot"
      done
      launched=${#SHOT_PIDS[@]}
      nonzero=
      for ((shot = 0; shot < launched; shot++)); do
        wait "${SHOT_PIDS[$shot]}"
        status=$?
        # In the schedule, not the shot's log: the comparison takes each
        # log's last write as the shot's end.
        echo "$(date '+%F %T') exit shot=$shot status=$status" >> "$OUT/h2/schedule.log"
        [ "$status" = 0 ] || nonzero=$nonzero,$shot
      done
      SHOT_PIDS=()
      summary shots - "$((SECONDS - start))" \
        "launched=$launched procs=$P nonzero_exits=${nonzero#,} (h2/shots/, h2/schedule.log)"
      quit_mps
      compare h2 --shots "$OUT/h2/shots" --schedule "$OUT/h2/schedule.log" --cpu "$INPUTS/h2/cpu" \
        --chi "$H2_CHI" --tolerance "$H2_TOLERANCE" --expect "$SHOTS" --first-wave "$P" \
        --bar "$H2_DEV_PER_HOUR" --run1-per-hour "$H2_RUN1_PER_HOUR" "${H2_DEV_WORK[@]}" \
        --table "$OUT/h2/shots.md"
      [ -z "$nonzero" ] || VERDICT_OF=fail
      summary h2-compare - 0 "$COMPARED (h2/shots.md)"
      record h2 "$VERDICT_OF"
    fi
  fi
fi

# --- h2-lowsweep: shot 0 alone, every span swept from its low end ------------
if wanted h2-lowsweep && within_budget h2-lowsweep; then
  if [ "${#RECORDINGS[@]}" = 0 ]; then
    summary h2-lowsweep - 0 "verdict=fail no recordings in $INPUTS/h2"
    record h2-lowsweep fail
  else
    log=$OUT/h2/lowsweep/shot-0.log
    # shellcheck disable=SC2046
    run_part "$(capped "$SHOT_LIMIT")" "$log" env P6_LOW_SWEEP_ONLY=1 $(shot_command 0)
    compare h2 --shots "$OUT/h2/lowsweep" --schedule /dev/null --cpu "$INPUTS/h2/cpu-lowsweep" \
      --chi "$H2_CHI" --tolerance "$H2_TOLERANCE" --expect 1 --first-wave 1 --bar 0 \
      --versus-seconds "${H2_SOLO_REPLAY_SECONDS:--}" --table "$OUT/h2/lowsweep.md"
    [ "$STATUS" = 0 ] || VERDICT_OF=fail
    summary h2-lowsweep "$STATUS" "$SECONDS_TAKEN" "$COMPARED (h2/lowsweep/, h2/lowsweep.md)"
    record h2-lowsweep "$VERDICT_OF"
  fi
fi

# --- mps-prep: #540's preparations -------------------------------------------
if wanted mps-prep; then
  for case_name in $MPS_CASES; do
    within_budget "mps-prep:$case_name" || continue
    log=$OUT/mps-prep/$case_name-following.log
    run_part "$(capped $((3 * ${MPS_DEV_SECONDS[$case_name]})))" "$log" "$BIN/mps_prep_check" \
      --engine cuda-mps --case "$INPUTS/mps-prep/cases/$case_name" --placement following
    compare mps-prep --gpu "$log" --cpu "$INPUTS/mps-prep/cpu/run-$case_name-following.log" \
      --dev "$INPUTS/mps-prep/dev-gpu/$case_name-following.log" \
      --table "$OUT/mps-prep/$case_name-phases.md"
    [ "$STATUS" = 0 ] || VERDICT_OF=fail
    summary "mps-prep:$case_name" "$STATUS" "$SECONDS_TAKEN" \
      "$COMPARED (mps-prep/$case_name-following.log, mps-prep/$case_name-phases.md)"
    record "mps-prep:$case_name" "$VERDICT_OF"
  done
fi

# --- polyene: N = 16 and 20, fused by the engine -----------------------------
if wanted polyene; then
  for n in $POLYENE_SIZES; do
    within_budget "polyene:n$n" || continue
    log=$OUT/polyene/n$n-chi128-cuda-mps.log
    run_part "$(capped $((4 * ${POLYENE_DEV_SECONDS[$n]})))" "$log" "$BIN/polyene_qpe_prefix" \
      --engine cuda-mps --n "$n" --chi 128 --order interleave-centre --variant base \
      --reference "${POLYENE_REFERENCE[$n]}"
    compare polyene --gpu "$log" --cpu "$INPUTS/polyene/cpu-n$n-chi128.log" \
      --dev "$INPUTS/polyene/dev-gpu-n$n-chi128-cuda-mps-seq3-mpo.log" \
      --run1-seconds "${POLYENE_RUN1_SECONDS[$n]}" --table "$OUT/polyene/n$n-steps.md"
    [ "$STATUS" = 0 ] || VERDICT_OF=fail
    summary "polyene:n$n" "$STATUS" "$SECONDS_TAKEN" \
      "$COMPARED (polyene/n$n-chi128-cuda-mps.log, polyene/n$n-steps.md)"
    record "polyene:n$n" "$VERDICT_OF"
  done
fi

VERDICT=$(parts_verdict)
case "$VERDICT" in
PASS) exit 0 ;;
FAIL) exit 1 ;;
SLOW) exit 4 ;;
*) exit 3 ;;
esac
