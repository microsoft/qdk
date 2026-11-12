#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
#
# Development only (not for review): polyene's QPE prefix on the A100's MPS
# engine (CuTensorNetEngine), for the GPU's qualification. Polyene is a
# development case for large bonds and a candidate relevant case, pending
# the chemistry team. The harness is the example polyene_qpe_prefix (its
# inputs and its doc are in examples/polyene_qpe_prefix/). Each part has
# its own log and one line in summary.txt; a part that fails or times out
# does not stop the next, and a part stopped by its limit still logs every
# step and c_p it reached.
#
# The three questions, cheapest and most decisive first:
#
#   n8-chi128   correctness: N = 8 at chi 128 (exact on the CPU engine)
#               against the exact readings; pass: verdict=AgreesWithExact.
#   n12-chi64   correctness and speed: N = 12 at chi 64 against the CPU
#               engine's run at chi 64; its time against the CPU's clean
#               baseline (CPU_N12_SECONDS).
#   n16-chi64   correctness and speed: N = 16 at chi 64 against the CPU
#               engine's run at chi 64; its time against the CPU's 9,441 s.
#   n16-chi128, n16-chi192, n16-chi256
#               accuracy against chi: c_p for p = 1, 2, 4, ..., 32 and
#               1 - W at each step; cp-convergence.txt says, per p, how
#               far c_p moved between the two largest chi that reached p.
#
# Run from anywhere, on the branch's checkout, after a100-batch.sh:
#
#   bash source/cutensornet/examples/polyene-gpu.sh
#
# Environment (all optional):
#   OUT        the output directory (default $HOME/polyene-gpu-<date>)
#   ENGINE     cutensornet (default) or cpu: cpu dry-runs the plumbing
#   PARTS      the parts to run (default: all, in the order above), for
#              instance "n8-chi128 n16-chi64"
#   STEPS      fewer controlled steps than the program's 32, for a quick
#              dry run (the references are then not checked)
#   LIMIT_PERCENT the parts' time limits in percent (default 100), for a
#              dry run that stops each part early
#   QDK_CUTENSORNET_LIBRARY, QDK_CUDART_LIBRARY, QDK_CUSOLVER_LIBRARY
#              the libraries, when not at the pinned paths below
#   CARGO_FLAGS extra flags for cargo build (for instance --offline)
#
# The parts' time limits (timeout) keep the session within about 3 h.

set -u

REPO=$(readlink -f "$(dirname "$(readlink -f "$0")")/../../..")
ENGINE=${ENGINE:-cutensornet}
OUT=${OUT:-$HOME/polyene-gpu-$(date +%Y%m%d-%H%M)}
PARTS=${PARTS:-n8-chi128 n12-chi64 n16-chi64 n16-chi128 n16-chi192 n16-chi256}
STEPS=${STEPS:-}
LIMIT_PERCENT=${LIMIT_PERCENT:-100}
CARGO_FLAGS=${CARGO_FLAGS:-}
BIN=${CARGO_TARGET_DIR:-$REPO/target}/release/examples
SUMMARY=$OUT/summary.txt
# The CPU engine's prefix seconds at chi 64, on one core: N = 16 loaded
# (the polyene quality study), N = 12 alone on its core (this program on
# the CPU engine, the clean baseline).
CPU_N16_SECONDS=9441
CPU_N12_SECONDS=${CPU_N12_SECONDS:-2850}

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

{
  date
  echo "repo=$REPO engine=$ENGINE out=$OUT parts=$PARTS steps=${STEPS:-32} limit_percent=$LIMIT_PERCENT"
  git -C "$REPO" log --oneline -1
  nvidia-smi
  rustc --version
  cargo --version
  sha256sum "$REPO"/source/cutensornet/examples/polyene_qpe_prefix/ppp*
  env | grep -E '^(QDK_|CUDA|CARGO)'
} > "$OUT/environment.txt" 2>&1

# The MPS daemon off: every part runs one process alone on the GPU.
if [ "$ENGINE" = cutensornet ]; then
  pgrep -a nvidia-cuda-mps || true
  echo quit | nvidia-cuda-mps-control 2>/dev/null || true
fi

# --- Build (release) ---------------------------------------------------------
run_part 1200 "$OUT/build.log" cargo build --manifest-path "$REPO/Cargo.toml" --release \
  $CARGO_FLAGS -p qdk_cutensornet --example polyene_qpe_prefix
summary build "$STATUS" "$SECONDS_TAKEN" "polyene_qpe_prefix=$STATUS"

# --- The parts -------------------------------------------------------------------
# One part: N ($2) at chi ($3) within $4 seconds, checked against the
# reference file $5 (in examples/polyene_qpe_prefix/).
polyene_part() {
  local part=$1 n=$2 chi=$3 limit=$(($4 * LIMIT_PERCENT / 100)) reference=$5
  case " $PARTS " in *" $part "*) ;; *) return ;; esac
  local log=$OUT/$part.log
  run_part "$limit" "$log" "$BIN/polyene_qpe_prefix" --engine "$ENGINE" --n "$n" --chi "$chi" \
    ${reference:+--reference "$reference"} ${STEPS:+--steps "$STEPS"}
  local done_line step cp check
  done_line=$(grep '^POLYENE done ' "$log" | tail -1)
  step=$(grep '^POLYENE step ' "$log" | tail -1)
  cp=$(grep '^POLYENE cp ' "$log" | tail -1)
  check=$(grep '^POLYENE check ' "$log" | tail -1)
  local verdict=none
  if [ -n "$done_line" ]; then
    verdict=$(field verdict "$done_line")
  elif [ "$STATUS" = 124 ] || [ "$STATUS" = 137 ]; then
    verdict=timed-out
  else
    verdict=error
  fi
  local p seconds
  p=$(field p "$step")
  seconds=$(field seconds "$step")
  local line="verdict=$verdict steps_done=${p:-0} seconds_at_last_step=${seconds:--}"
  line="$line seconds_per_step=$(awk -v s="${seconds:-0}" -v p="${p:-0}" 'BEGIN {if (p > 0) printf "%.1f", s / p; else printf "-"}')"
  if [ -n "$step" ]; then
    line="$line one_minus_w=$(field one_minus_w "$step") largest_bond=$(field largest_bond "$step")"
  fi
  if [ -n "$cp" ]; then
    line="$line c_p$(field p "$cp")=($(field re "$cp"),$(field im "$cp"))"
  fi
  if [ -n "$done_line" ]; then
    line="$line prefix_seconds=$(field prefix_seconds "$done_line")"
  fi
  if [ -n "$check" ]; then
    line="$line largest_gap=$(field largest "$check")"
  fi
  echo "$line"
  summary "$part" "$STATUS" "$SECONDS_TAKEN" "$line"
}

# The time of a finished prefix against the CPU engine's, as part=... ratio.
speed() {
  local part=$1 cpu_seconds=$2 done_line
  [ -f "$OUT/$part.log" ] || return
  done_line=$(grep '^POLYENE done ' "$OUT/$part.log" | tail -1)
  if [ -n "$done_line" ] && [ -z "$STEPS" ] && [ "$cpu_seconds" != 0 ]; then
    local prefix
    prefix=$(field prefix_seconds "$done_line")
    summary "speed-$part" - - "$(awk -v g="$prefix" -v c="$cpu_seconds" \
      'BEGIN {printf "prefix_seconds=%s cpu_seconds=%s cpu_over_this=%.2f", g, c, c / g}')"
  fi
}

polyene_part n8-chi128 8 128 900 ppp8.exact.reference
polyene_part n12-chi64 12 64 900 ppp12.cpu-chi64.reference
speed n12-chi64 "$CPU_N12_SECONDS"
polyene_part n16-chi64 16 64 2400 ppp16.cpu-chi64.reference
speed n16-chi64 "$CPU_N16_SECONDS"
polyene_part n16-chi128 16 128 2400 ""
polyene_part n16-chi192 16 192 1500 ""
polyene_part n16-chi256 16 256 1500 ""

# --- c_p against chi --------------------------------------------------------------
# Per p, c_p at each chi that reached it, and how far it moved between the
# two largest; converged when that is below 1e-3.
{
  echo "# N = 16: c_p = <K|U^p|K> per chi; moved = |c_p(largest chi) - c_p(next)|"
  for chi in 64 128 192 256; do
    log=$OUT/n16-chi$chi.log
    [ -f "$log" ] && grep '^POLYENE cp ' "$log" | sed "s/^/chi=$chi /"
  done | awk '
    {
      split($1, c, "="); chi = c[2] + 0
      for (i = 2; i <= NF; i++) { split($i, kv, "="); f[kv[1]] = kv[2] }
      p = f["p"] + 0; re[p, chi] = f["re"]; im[p, chi] = f["im"]; w[p, chi] = f["one_minus_w"]
      # The logs come in increasing chi.
      count[p]++; next_top[p] = top[p]; top[p] = chi
    }
    END {
      for (p = 1; p <= 64; p++) {
        if (!(p in count)) continue
        line = sprintf("p=%d", p)
        for (chi = 64; chi <= 256; chi += 64)
          if ((p, chi) in re) line = line sprintf(" chi%d=%s%si(1-W=%s)", chi, re[p, chi], (im[p, chi] < 0 ? "" : "+") im[p, chi], w[p, chi])
        if (count[p] >= 2) {
          a = top[p]; b = next_top[p]
          moved = sqrt((re[p, a] - re[p, b]) ^ 2 + (im[p, a] - im[p, b]) ^ 2)
          line = line sprintf(" moved(chi%d,chi%d)=%.3e converged=%s", b, a, moved, moved < 1e-3 ? "yes" : "no")
        } else line = line " moved=- (one chi reached this p)"
        print line
      }
    }'
} > "$OUT/cp-convergence.txt"
cat "$OUT/cp-convergence.txt"

# --- What to send back ------------------------------------------------------------
tar czf "$OUT.tar.gz" --exclude='*.tar.gz' -C "$(dirname "$OUT")" "$(basename "$OUT")"
echo "Send back: $OUT.tar.gz"
cat "$SUMMARY"
