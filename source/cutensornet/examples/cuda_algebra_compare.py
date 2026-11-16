#!/usr/bin/env python3
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Development only (not for review): the comparisons of
gpu-cuda-algebra-qualify.sh, each the GPU's output against the CPU
engine's references and the dev line's A100 times.

Each subcommand prints one line of key=value fields ending with
`verdict=pass|slow|fail`, writes its per-item table to --table, and exits
0 on pass, 1 on fail and 4 on slow (correct, but slower than the dev
line's time beyond the bar).

  h2        the H2 replays' dumps (shots/shot-<i>.log) against the CPU
            engine's dumps of the same recorded shot: every projection
            probability (p), the retained weight (w) and the readout
            distribution (r) within --tolerance, and the split and swap
            counts equal; the throughput over the run and over the first
            wave (the first --first-wave shots to start) against --bar
            (0: no bar).
  polyene   a polyene log against the CPU engine's log of the same case:
            every step's c_p and the energy within --tolerance, and the
            driver's verdict against its reference the CPU's; each step's
            time beside the dev line's (informational).
  mps-prep  a #540 log: the driver's own verdict (Agrees), the energy
            against the CPU engine's log within the check's energy
            tolerance, and each phase (MPSPREP span) against the dev
            line's, slow when one of at least --span-floor seconds on the
            dev line takes more than --slack longer.
"""

import argparse
import glob
import math
import os
import re
import sys


def fields(line):
    return dict(w.split("=", 1) for w in line.split() if "=" in w)


# --- h2 ----------------------------------------------------------------------


def read_dump(path):
    """The values of an h2_replay dump: name, p, w, r, stats, seconds."""
    dump = {"name": None, "p": [], "w": [], "r": [], "stats": {}, "seconds": None, "tvd": None}
    with open(path, errors="replace") as f:
        for line in f:
            words = line.split()
            if not words:
                continue
            head = words[0]
            if head == "shot" and len(words) == 2:
                dump["name"] = words[1]
            elif head in ("p", "w", "r") and len(words) == 2:
                try:
                    dump[head].append(float(words[1]))
                except ValueError:
                    pass
            elif head == "stats":
                dump["stats"] = dict(zip(words[1::2], words[2::2]))
            elif head == "time":
                f2 = fields(line)
                dump["seconds"] = float(f2["seconds"])
                dump["tvd"] = f2.get("tvd")
    return dump


def h2(args):
    logs = sorted(glob.glob(os.path.join(args.shots, "shot-*.log")),
                  key=lambda p: int(re.search(r"shot-(\d+)\.log$", p).group(1)))
    starts = {}
    if os.path.exists(args.schedule):
        for line in open(args.schedule):
            f = fields(line)
            if "shot" in f and "epoch" in f:
                starts[int(f["shot"])] = float(f["epoch"])
    rows, failed, largest, stats_note = [], [], 0.0, []
    ends, seconds = {}, []
    for log in logs:
        index = int(re.search(r"shot-(\d+)\.log$", log).group(1))
        gpu = read_dump(log)
        problem = ""
        gap = math.nan
        if gpu["name"] is None or gpu["seconds"] is None:
            problem = "incomplete"
        else:
            recorded = re.search(r"shot(\d+)\.replay$", gpu["name"]).group(1)
            ref = os.path.join(args.cpu, f"h2-cpu-chi{args.chi}-shot{recorded}.txt")
            cpu = read_dump(ref)
            counts = [len(gpu[k]) == len(cpu[k]) for k in ("p", "w", "r")]
            if not all(counts) or not cpu["p"]:
                problem = "counts differ" if cpu["p"] else f"no reference {ref}"
            else:
                gap = max(abs(a - b) for k in ("p", "w", "r") for a, b in zip(gpu[k], cpu[k]))
                largest = max(largest, gap)
                if gap > args.tolerance:
                    problem = f"gap {gap:.3e}"
                for key in ("splits", "swaps"):
                    if gpu["stats"].get(key) != cpu["stats"].get(key):
                        problem += f" {key} {gpu['stats'].get(key)}!={cpu['stats'].get(key)}"
                for key in ("rest", "transient"):
                    if gpu["stats"].get(key) != cpu["stats"].get(key):
                        stats_note.append(f"{index}:{key}")
            seconds.append(gpu["seconds"])
            ends[index] = os.path.getmtime(log)
        if problem:
            failed.append(index)
        rows.append(f"| {index} | {gpu['name']} | {gap:.3e} | {gpu['stats']} | "
                    f"{gpu['seconds']} | {gpu['tvd']} | {problem or 'pass'} |")
    with open(args.table, "w") as t:
        t.write("| shot | recorded | largest gap to CPU | stats | replay s | tvd | verdict |\n")
        t.write("| --- | --- | --- | --- | --- | --- | --- |\n")
        t.write("\n".join(rows) + "\n")
    timed = sorted(i for i in ends if i in starts)
    whole = wave_rate = math.nan
    if timed:
        first = min(starts[i] for i in timed)
        whole = len(timed) / (max(ends[i] for i in timed) - first) * 3600
        wave = sorted(timed, key=lambda i: starts[i])[: args.first_wave]
        wave_rate = len(wave) / (max(ends[i] for i in wave) - first) * 3600
    ok = not failed and len(logs) == args.expect
    slow = args.bar > 0 and not wave_rate >= args.bar
    verdict = "fail" if not ok else ("slow" if slow else "pass")
    print(f"shots={len(logs)} expected={args.expect} failed={','.join(map(str, failed)) or 'none'} "
          f"largest_gap={largest:.3e} tolerance={args.tolerance:g} "
          f"bond_stats_differ={','.join(stats_note) or 'none'} "
          f"replay_seconds_min={min(seconds, default=math.nan):.1f} "
          f"replay_seconds_max={max(seconds, default=math.nan):.1f} "
          f"first_wave={args.first_wave} first_wave_per_hour={wave_rate:.1f} "
          f"whole_run_per_hour={whole:.1f} dev_per_hour={args.bar} verdict={verdict}")
    return verdict


# --- polyene ------------------------------------------------------------------


def read_polyene(path):
    cp, steps, done = {}, {}, {}
    with open(path, errors="replace") as f:
        for line in f:
            if line.startswith("POLYENE cp "):
                g = fields(line)
                cp[int(g["p"])] = complex(float(g["re"]), float(g["im"]))
            elif line.startswith("POLYENE step "):
                g = fields(line)
                steps[int(g["p"])] = (float(g["seconds"]), g.get("largest_bond"), g.get("one_minus_w"))
            elif line.startswith("POLYENE done ") or line.startswith("POLYENE end "):
                done.update(fields(line))
    return cp, steps, done


def polyene(args):
    gcp, gsteps, gdone = read_polyene(args.gpu)
    ccp, _, cdone = read_polyene(args.cpu)
    _, dsteps, ddone = read_polyene(args.dev) if args.dev else ({}, {}, {})
    rows, largest, problems = [], 0.0, []
    previous = {"g": 0.0, "d": 0.0}
    worst_ratio = 0.0
    for p in sorted(set(gsteps) | set(dsteps) | set(ccp)):
        gap = abs(gcp[p] - ccp[p]) if p in gcp and p in ccp else math.nan
        if gap == gap:
            largest = max(largest, gap)
        g = gsteps.get(p, (math.nan, None, None))
        d = dsteps.get(p, (math.nan, None, None))
        g_step, d_step = g[0] - previous["g"], d[0] - previous["d"]
        previous = {"g": g[0], "d": d[0]}
        ratio = g_step / d_step if d_step > 0 else math.nan
        if ratio == ratio:
            worst_ratio = max(worst_ratio, ratio)
        rows.append(f"| {p} | {gap:.3e} | {g_step:.1f} | {d_step:.1f} | {ratio:.3f} | "
                    f"{g[1]} | {d[1]} | {g[2]} |")
    missing = sorted(set(ccp) - set(gcp))
    if missing:
        problems.append(f"missing_steps={','.join(map(str, missing))}")
    if not ccp:
        problems.append("no_cpu_steps")
    if largest > args.tolerance:
        problems.append(f"cp_gap={largest:.3e}")
    energy_gap = math.nan
    if "energy" in gdone and "energy" in cdone:
        energy_gap = abs(float(gdone["energy"]) - float(cdone["energy"]))
        if energy_gap > args.tolerance:
            problems.append(f"energy_gap={energy_gap:.3e}")
    elif ccp:
        problems.append("no_end_line")
    if gdone.get("verdict") != cdone.get("verdict"):
        problems.append(f"cpu_driver_verdict={cdone.get('verdict')}")
    with open(args.table, "w") as t:
        t.write("| p | c_p gap to CPU | GPU step s | dev step s | GPU/dev | GPU bond | dev bond | GPU 1-W |\n")
        t.write("| --- | --- | --- | --- | --- | --- | --- | --- |\n")
        t.write("\n".join(rows) + "\n")
    g_total = float(gdone.get("prefix_seconds", "nan"))
    d_total = float(ddone.get("prefix_seconds", "nan"))
    verdict = "fail" if problems else "pass"
    print(f"steps={len(gsteps)} cp_read={len(gcp)}/{len(ccp)} largest_cp_gap={largest:.3e} energy_gap={energy_gap:.3e} "
          f"tolerance={args.tolerance:g} driver_verdict={gdone.get('verdict')} "
          f"one_minus_w={gdone.get('one_minus_w', '-')} prefix_seconds={g_total:.1f} "
          f"dev_prefix_seconds={d_total:.1f} ratio={g_total / d_total:.3f} "
          f"largest_step_ratio={worst_ratio:.3f} {' '.join(problems)} verdict={verdict}")
    return verdict


# --- mps-prep -----------------------------------------------------------------


def read_prep(path):
    spans, check, done = [], {}, {}
    with open(path, errors="replace") as f:
        for line in f:
            if line.startswith("MPSPREP span "):
                m = re.match(r"MPSPREP span from=(.*) to=(.*) seconds=(\S+)", line)
                spans.append((m.group(1), m.group(2), float(m.group(3))))
            elif line.startswith("MPSPREP check "):
                check = fields(line)
            elif line.startswith("MPSPREP done "):
                done = fields(line)
    return spans, check, done


def mps_prep(args):
    gspans, gcheck, gdone = read_prep(args.gpu)
    _, ccheck, _ = read_prep(args.cpu)
    dspans, _, ddone = read_prep(args.dev)
    dev = {(a, b): s for a, b, s in dspans}
    rows, slow, problems = [], [], []
    worst = 0.0
    for a, b, s in gspans:
        d = dev.get((a, b))
        ratio = s / d if d else math.nan
        flag = ""
        if d is not None and d >= args.span_floor:
            worst = max(worst, ratio)
            if ratio > 1 + args.slack:
                flag = "slow"
                slow.append(f"{a}->{b}")
        rows.append(f"| {a} | {b} | {s:.3f} | {'-' if d is None else f'{d:.3f}'} | {ratio:.3f} | {flag} |")
    if [(a, b) for a, b, _ in gspans] != [(a, b) for a, b, _ in dspans]:
        problems.append("phases_differ_from_dev")
    if gcheck.get("verdict") != "Agrees":
        problems.append(f"check={gcheck.get('verdict')}")
    energy_gap = math.nan
    if "energy" in gcheck and "energy" in ccheck:
        energy_gap = abs(float(gcheck["energy"]) - float(ccheck["energy"]))
        if energy_gap > float(gcheck.get("energy_tolerance", "0")):
            problems.append(f"energy_gap_to_cpu={energy_gap:.3e}")
    else:
        problems.append("no_energy")
    with open(args.table, "w") as t:
        t.write("| from | to | GPU s | dev s | GPU/dev | flag |\n| --- | --- | --- | --- | --- | --- |\n")
        t.write("\n".join(rows) + "\n")
    g_total = float(gdone.get("seconds", "nan"))
    d_total = float(ddone.get("seconds", "nan"))
    verdict = "fail" if problems else ("slow" if slow else "pass")
    print(f"check={gcheck.get('verdict')} energy={gcheck.get('energy')} energy_gap_to_cpu={energy_gap:.3e} "
          f"largest_gap={gcheck.get('largest')} seconds={g_total:.1f} dev_seconds={d_total:.1f} "
          f"ratio={g_total / d_total:.3f} largest_phase_ratio={worst:.3f} "
          f"slow_phases={','.join(slow) or 'none'} {' '.join(problems)} verdict={verdict}")
    return verdict


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    a = sub.add_parser("h2")
    a.add_argument("--shots", required=True)
    a.add_argument("--schedule", required=True)
    a.add_argument("--cpu", required=True)
    a.add_argument("--chi", type=int, required=True)
    a.add_argument("--tolerance", type=float, required=True)
    a.add_argument("--expect", type=int, required=True)
    a.add_argument("--first-wave", type=int, required=True)
    a.add_argument("--bar", type=float, required=True)
    a.add_argument("--table", required=True)
    b = sub.add_parser("polyene")
    b.add_argument("--gpu", required=True)
    b.add_argument("--cpu", required=True)
    b.add_argument("--dev")
    b.add_argument("--tolerance", type=float, default=1e-4)
    b.add_argument("--table", required=True)
    c = sub.add_parser("mps-prep")
    c.add_argument("--gpu", required=True)
    c.add_argument("--cpu", required=True)
    c.add_argument("--dev", required=True)
    c.add_argument("--slack", type=float, default=0.10)
    c.add_argument("--span-floor", type=float, default=5.0)
    c.add_argument("--table", required=True)
    args = parser.parse_args()
    verdict = {"h2": h2, "polyene": polyene, "mps-prep": mps_prep}[args.command](args)
    sys.exit({"pass": 0, "fail": 1, "slow": 4}[verdict])


if __name__ == "__main__":
    main()
