#!/usr/bin/env python3
"""Host-only CLI selector/stop guards using injected commands, not GPU evidence."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("validate-on-cuda-host.sh")
PREFIX = "simulation::contraction::execution::qualification::"
FIXED = [
    PREFIX + "native::" + name
    for name in ("a_asymmetric_diagnostic", "b_case_a_2x2", "c_case_a_4x4")
]
REUSABLE = [
    PREFIX + "reusable::native::" + name
    for name in ("a_supplied_plan_candidate_reuse", "b_supplied_plan_joint_operators")
]

COMMAND = r"""
import json
import os
from pathlib import Path
import sys

name = Path(sys.argv[0]).name
args = sys.argv[1:]
if name == "uname":
    print({"-s": "Linux", "-m": "x86_64"}.get(args[0], "Linux host-only-fixture"))
elif name == "ldconfig":
    print("libcutensornet.so.2 (libc6,x86-64) => /host-only-fixture/libcutensornet.so.2")
    print("libcudart.so.12 (libc6,x86-64) => /host-only-fixture/libcudart.so.12")
elif name == "cargo":
    with open(os.environ["VALIDATOR_COMMAND_LOG"], "a") as log:
        log.write(json.dumps(args) + "\n")
    if args[0] == "test":
        selector = args[args.index("--lib") + 1] if "--lib" in args else "library::tests::fixture"
        mode = os.environ["VALIDATOR_TEST_MODE"]
        if mode == "availability-failure" and "--test" in args:
            print("injected availability command failure")
            sys.exit(1)
        if "::qualification::" in selector:
            if mode == "failure":
                print(f"test {selector} ... FAILED")
                print("test result: FAILED. 0 passed; 1 failed; 0 ignored;")
                sys.exit(1)
            if mode == "zero-tests":
                print("test result: ok. 0 passed; 0 failed; 0 ignored;")
                sys.exit(0)
            if mode == "wrong-selector":
                selector = "unrelated::test"
        print(f"test {selector} ... ok")
        print("test result: ok. 1 passed; 0 failed; 0 ignored;")
    else:
        print("host-only command fixture")
else:
    print("host-only version fixture")
"""


class ValidatorTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="qdk-validator-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.log = self.root / "commands.jsonl"
        for name in ("uname", "cargo", "rustc", "rustfmt", "python3", "ldconfig"):
            path = self.root / name
            path.write_text(f"#!{sys.executable}\n" + COMMAND)
            path.chmod(0o700)
        self.env = dict(os.environ)
        self.env.update({
            "PATH": str(self.root) + os.pathsep + self.env.get("PATH", ""),
            "VALIDATOR_COMMAND_LOG": str(self.log),
            "VALIDATOR_TEST_MODE": "success",
        })

    def run_validator(self, *args, mode="success"):
        self.env["VALIDATOR_TEST_MODE"] = mode
        return subprocess.run(
            ["bash", str(SCRIPT), *args],
            env=self.env, capture_output=True, text=True, check=False, timeout=30,
        )

    def numerical_selectors(self):
        commands = [json.loads(line) for line in self.log.read_text().splitlines()]
        selected = []
        for args in commands:
            if args[0] == "test" and "--lib" in args:
                selector = args[args.index("--lib") + 1]
                if "::qualification::" in selector:
                    self.assertEqual(
                        args[args.index("--"):],
                        ["--", "--exact", "--ignored", "--nocapture", "--test-threads=1"],
                    )
                    selected.append(selector)
        return selected

    def test_new_selector_runs_only_the_two_requested_cases(self):
        result = self.run_validator("--reusable-input-qualification")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.numerical_selectors(), REUSABLE)

    def test_existing_fixed_selector_is_unchanged(self):
        result = self.run_validator("--contraction-qualification")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.numerical_selectors(), FIXED)

    def test_reusable_cases_are_not_part_of_default_run(self):
        result = self.run_validator()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.numerical_selectors(), [])

    def test_native_failure_zero_tests_and_wrong_selector_stop_the_sequence(self):
        for mode in ("failure", "zero-tests", "wrong-selector"):
            with self.subTest(mode=mode):
                if self.log.exists():
                    self.log.unlink()
                result = self.run_validator("--reusable-input-qualification", mode=mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.numerical_selectors(), REUSABLE[:1])

    def test_earlier_failure_prevents_reusable_case_execution(self):
        result = self.run_validator("--reusable-input-qualification", mode="availability-failure")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.numerical_selectors(), [])
        self.assertIn("SKIPPED because an earlier check failed", result.stdout)

    def test_skip_hardware_combination_is_rejected_before_commands(self):
        result = self.run_validator("--skip-hardware", "--reusable-input-qualification")
        self.assertEqual(result.returncode, 2)
        self.assertIn("cannot be combined", result.stderr)
        self.assertFalse(self.log.exists())


if __name__ == "__main__":
    unittest.main()
