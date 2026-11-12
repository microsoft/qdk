#!/usr/bin/env python3
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Development branch only (not for review): aggregates the SHOT lines that
`h2_dev` prints, from any number of logs, and checks H2's acceptance.

    python3 h2_summary.py --table h2-readout-distribution.txt LOG...

Prints the shots and failures; per shot the draws, largest bond, lost
weight, TVD and peaks; the histogram of the records (chemistry order); the
G statistic of the records against the table with a Monte Carlo p-value
(20,000 draws); and the throughput: over the run (shots / (last end - first
start)) and over the first wave (the shots that started within a minute of
the first, divided by the time until the last of them ended). Exits 1 when
a check fails.
"""

import argparse
import collections
import math
import random
import sys

BAR = 14.7 * 0.9  # shots per hour: the qualified run's, less 10%


def parse(paths):
    shots, failures = [], []
    for path in paths:
        with open(path) as log:
            for line in log:
                if line.startswith("SHOT "):
                    shots.append(dict(field.split("=", 1) for field in line.split()[1:]))
                elif line.startswith("FAILED "):
                    failures.append(line.strip())
    return shots, failures


def table(path):
    rows = {}
    with open(path) as file:
        for line in file:
            if line.startswith("#") or not line.strip():
                continue
            record_order, _chemistry, probability = line.split()
            rows[record_order] = float(probability)
    return rows


def g_statistic(counts, probabilities, n):
    return 2 * sum(c * math.log(c / (n * probabilities[k])) for k, c in counts.items() if c)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--table", required=True)
    parser.add_argument("--bond", type=int, default=24)
    parser.add_argument("--draws", type=int, default=1414)
    parser.add_argument("--peaks", default="01010,10110")
    parser.add_argument("--tvd", type=float, default=1e-9)
    parser.add_argument("logs", nargs="+")
    args = parser.parse_args()
    shots, failures = parse(args.logs)
    probabilities = table(args.table)
    ok = True

    print(f"shots: {len(shots)}, failed: {len(failures)}")
    for failure in failures:
        print("  " + failure)
    ok &= not failures and bool(shots)

    peaks = set(args.peaks.split(","))
    bad = []
    for shot in sorted(shots, key=lambda s: int(s["shot"])):
        problems = []
        if int(shot["draws"]) != args.draws:
            problems.append(f"draws {shot['draws']}")
        if int(shot["largest_bond"]) != args.bond:
            problems.append(f"largest bond {shot['largest_bond']}")
        if shot.get("tvd", "none") == "none" or float(shot["tvd"]) > args.tvd:
            problems.append(f"tvd {shot.get('tvd')}")
        if set(shot.get("peaks", "").split(",")) != peaks:
            problems.append(f"peaks {shot.get('peaks')}")
        if problems:
            bad.append(f"shot {shot['shot']}: " + ", ".join(problems))
    print(f"per shot (draws {args.draws}, largest bond {args.bond}, TVD <= {args.tvd:g}, peaks {args.peaks}):",
          "all hold" if not bad else f"{len(bad)} shots fail")
    for line in bad:
        print("  " + line)
    ok &= not bad
    if shots:
        tvds = [float(s["tvd"]) for s in shots if s.get("tvd", "none") != "none"]
        lost = [float(s["lost_weight"]) for s in shots]
        if tvds:
            print(f"TVD: {min(tvds):.2e} to {max(tvds):.2e}")
        print(f"1 - retained weight: at most {max(lost):.2e}")
        seconds = [float(s["seconds"]) for s in shots]
        print(f"seconds per shot: {min(seconds):.0f} to {max(seconds):.0f}")

    n = len(shots)
    counts = collections.Counter(s["records"] for s in shots)
    print("histogram (chemistry order):")
    for record, count in counts.most_common():
        print(f"  {record[::-1]} {count}  (expected {n * probabilities.get(record, 0):.1f})")
    if n:
        observed = g_statistic(counts, probabilities, n)
        keys = list(probabilities)
        weights = [probabilities[k] for k in keys]
        generator = random.Random(1)
        draws = 20000
        above = sum(
            g_statistic(collections.Counter(generator.choices(keys, weights, k=n)), probabilities, n) >= observed
            for _ in range(draws)
        )
        p = above / draws
        print(f"G = {observed:.2f}, Monte Carlo p = {p:.3f} ({draws} draws): {'pass' if p > 0.05 else 'FAIL'} (p > 0.05)")
        ok &= p > 0.05

        starts = [float(s["start_unix"]) for s in shots]
        ends = [float(s["end_unix"]) for s in shots]
        overall = n / (max(ends) - min(starts)) * 3600
        first = min(starts)
        wave = [s for s in shots if float(s["start_unix"]) < first + 60]
        wave_rate = len(wave) / (max(float(s["end_unix"]) for s in wave) - first) * 3600
        print(f"throughput: first wave {len(wave)} shots, {wave_rate:.1f} shots/h; "
              f"whole run {overall:.1f} shots/h; bar {BAR:.2f} shots/h (14.7 less 10%): "
              f"{'pass' if wave_rate >= BAR else 'FAIL'}")
    print("ACCEPTANCE:", "PASS" if ok else "FAIL")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
