# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Build/run the compact examples and assert the actual output/status matrix."""

import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from harness.common import KIT, load_config, read_json  # noqa: E402
from harness.runner import create_run  # noqa: E402


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--config", type=Path, default=KIT / "tests/smoke.json")
    cli.add_argument("--mode", choices=("release", "debug"), default="release")
    args = cli.parse_args()
    run = create_run(load_config(args.config), KIT / "examples/cases.json", args.mode)
    manifest = read_json(run / "manifest.json")
    if manifest["status"] != "complete":
        raise AssertionError(f"Incomplete smoke run: {manifest}")
    cases = {case["id"]: case for case in read_json(run / "cases.json")}
    rows = [json.loads(line) for line in (run / "results.jsonl").read_text().splitlines()]
    for row in rows:
        case = cases[row["case"]]
        stochastic = case["oracle"]["kind"] == "observe"
        for comparison in row["comparison"]:
            expected = (
                "simulator_observed" if comparison["profile"] == "Unrestricted"
                else "inconclusive" if stochastic
                else "validated_unchanged"
            )
            if comparison["verdict"] != expected:
                raise AssertionError(f"Unexpected smoke outcome: {row['case']} {comparison}")
        for role, result in row["results"].items():
            values = result["simulator"]["values"]
            if len(values) != case["shots"]:
                raise AssertionError(f"Wrong simulator shot count: {case['id']} {role}")
            for profile, record in result["profiles"].items():
                runner = record["runner"]
                if len(runner["values"]) != case["shots"] or sum(runner["counts"].values()) != case["shots"]:
                    raise AssertionError(f"Lost runner shots: {case['id']} {role} {profile}")
    print(f"Smoke assertions passed: {len(rows)} cases; {run}")


if __name__ == "__main__":
    main()
