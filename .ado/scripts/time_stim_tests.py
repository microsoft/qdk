# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Installs the built qdk wheel and times the stim differential tests over several runs.

Arguments not recognized by this script are passed through to pytest.
"""

import argparse
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TEST_DIR = ROOT / "source" / "qdk_package" / "tests-integration"


def pip_install(*args: str) -> None:
    subprocess.run([sys.executable, "-m", "pip", "install", *args], check=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--wheels-dir", type=Path, default=ROOT / "target" / "wheels")
    parser.add_argument(
        "--no-install",
        dest="install",
        action="store_false",
        help="Use the qdk and test dependencies that are already installed",
    )
    args, pytest_args = parser.parse_known_args()
    if args.runs < 1:
        parser.error("--runs must be at least 1")

    if args.install:
        pip_install(
            "-r",
            str(TEST_DIR / "test_requirements.txt"),
            "--only-binary",
            "qirrunner",
            "--only-binary",
            "pyqir",
        )
        # Matches build.py: the wheel's dependencies come from test_requirements.txt.
        pip_install(
            "qdk",
            "--force-reinstall",
            "--no-deps",
            "--no-index",
            f"--find-links={args.wheels_dir}",
        )

    durations = []
    for run in range(1, args.runs + 1):
        print(f"=== Stim tests: run {run}/{args.runs} ===", flush=True)
        start = time.perf_counter()
        subprocess.run(
            [sys.executable, "-m", "pytest", "stim_compiler", *pytest_args],
            check=True,
            cwd=TEST_DIR,
        )
        durations.append(time.perf_counter() - start)
        print(f"Run {run} took {durations[-1]:.2f}s", flush=True)

    print("=== Stim tests: timing summary ===")
    for run, duration in enumerate(durations, start=1):
        print(f"Run {run}: {duration:.2f}s")
    print(f"Average: {statistics.mean(durations):.2f}s")
    if len(durations) > 1:
        print(f"Std dev: {statistics.stdev(durations):.2f}s")
    print(f"Min:     {min(durations):.2f}s")
    print(f"Max:     {max(durations):.2f}s")


if __name__ == "__main__":
    main()
