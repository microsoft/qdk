# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Host-only driver protocol tests. These artifacts are NOT native GPU evidence."""

import argparse
import copy
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest


SCRIPT = Path(__file__).with_name("contraction-experiments.py")
SPEC = importlib.util.spec_from_file_location("experiments", SCRIPT)
driver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(driver)


def command(scenario):
    return [sys.executable, str(Path(__file__).resolve()), "--child", scenario]


def config(source="chronological"):
    return next(
        value for value in driver.configurations([1], [0], [17], 5)
        if value["plan_source"] == source
    )


def sample(_pid):
    return {"gpu_process_bytes": 4096, "host_peak_rss_bytes": 8192}


def successful_events(configuration):
    optimizer = configuration["plan_source"] == "optimizer"
    phases = ["optimize" if optimizer else "construct_control", "export", "prepare_host_call"]
    events = [
        {
            "event": "environment", "pid": os.getpid(),
            "versions": {
                "cutensornet": 21300, "cutensornet_cuda_runtime": 12090,
                "cuda_runtime": 12090, "cuda_driver": 13000,
            },
            "libraries": {"cutensornet": sys.executable, "cuda_runtime": sys.executable},
            "device_scratch_ceiling": 32 * 1024**3, "host_scratch_ceiling": None,
            "optimizer_workspace_constraint": 32 * 1024**3 if optimizer else None,
            "limit": 1e-8, "precision": "CUDA_C_64F/COMPUTE_64F", "device": 0,
            "threads": 1, "slicing": False, "cache": False, "autotuning": False,
        },
        {
            "event": "plan", "path": [[0, 1]] * 447, "slicing": [], "num_slices": 1,
            "intermediate_modes": [[0]] * 447,
        },
        {
            "event": "estimates", "source": "HOST TEST ONLY",
            "flops": 123 if optimizer else None,
            "largest_intermediate_elements": 65536 if optimizer else None,
        },
        {
            "event": "memory", "selected_input_bytes": None, "selected_input_count": None,
            "resident_input_bytes": 352, "resident_input_count": 6,
            "output_bytes": 65536 * 16, "device_scratch_minimum": 256,
            "device_scratch_recommended": 512, "device_scratch_allocated": 256,
            "host_scratch_minimum": 0, "host_scratch_recommended": 0,
            "host_scratch_allocated": 0, "device_cache_recommended": 0,
            "host_cache_recommended": 0, "owned_device_bytes": 352 + 65536 * 16 + 256,
        },
    ]
    events.extend({"event": "timing", "phase": phase, "seconds": 0.01} for phase in phases)
    for iteration in range(configuration["repeats"] + 1):
        events.extend([
            {"event": "timing", "phase": f"contract_readback_{iteration}", "seconds": iteration + 1},
            {
                "event": "comparison", "iteration": iteration, "amplitude_error": 1e-9,
                "probability_tv": 1e-10, "squared_norm_error": 0.0,
                "bitwise_equal_to_first": True,
            },
        ])
    final_memory = next(event for event in events if event["event"] == "memory").copy()
    final_memory.update(selected_input_bytes=352, selected_input_count=6)
    events.append(final_memory)
    events.extend(
        {"event": "cleanup", "owner": owner, "error": None}
        for owner in ("source_topology", "source_session", "execution", "fresh_session")
    )
    events.append({"event": "result", "status": "passed", "error": None, "workspace_rejection": None})
    return events


def child(scenario):
    if "--list" in sys.argv:
        selector = sys.argv[sys.argv.index("--exact") + 1]
        print("0 tests" if scenario == "zero_list" else f"{selector}: test")
        return 0
    directory = Path(os.environ["QDK_CONTRACTION_EVIDENCE_DIR"])
    if scenario in ("qualification", "zero_run"):
        selector = sys.argv[sys.argv.index("--exact") + 1]
        if scenario == "zero_run":
            print("test result: ok. 0 passed; 0 failed; 0 ignored;")
            return 0
        name, count = next(
            (name, count) for suffix, name, count in [
                ("a_asymmetric_diagnostic", "diagnostic", 8),
                ("b_case_a_2x2", "case_a_2x2", 16),
                ("c_case_a_4x4", "case_a_4x4", 65536),
            ] if selector.endswith(suffix)
        )
        for iteration in range(2):
            (directory / f"{name}-{iteration}.complex64le").write_bytes(bytes(count * 16))
        print("test result: ok. 1 passed; 0 failed; 0 ignored;")
        return 0
    configuration = json.loads((directory / "config.json").read_text())
    (directory / "host-test-only.txt").write_text("Injected driver protocol, not native evidence.\n")
    (directory / "child.pid").write_text(str(os.getpid()))
    if scenario == "sequence":
        scenario = ["workspace_limit", "allocation_failed", "passed"][int(directory.name[-3:])]
    events = successful_events(configuration)
    if scenario in ("workspace_limit", "allocation_failed"):
        events = [
            {"event": "cleanup", "owner": "fresh_session", "error": None},
            {
                "event": "result", "status": scenario, "error": "injected resource rejection",
                "workspace_rejection": {"required": 32 * 1024**3 + 1, "maximum": 32 * 1024**3},
            },
        ]
    elif scenario == "numerical":
        events[-1].update(status="failed", error="numerical mismatch")
    elif scenario == "threshold":
        next(e for e in events if e["event"] == "comparison")["amplitude_error"] = 1e-8 + 1e-15
    elif scenario == "boundary":
        for event in events:
            if event["event"] == "comparison":
                event.update(amplitude_error=1e-8, probability_tv=1e-8, squared_norm_error=1e-8)
    elif scenario == "cleanup":
        next(e for e in events if e["event"] == "cleanup")["error"] = "injected close failure"
    elif scenario == "missing_comparison":
        events = [e for e in events if not (e["event"] == "comparison" and e["iteration"] == 5)]
    elif scenario == "missing_timing":
        events = [e for e in events if not (e["event"] == "timing" and e["phase"] == "prepare_host_call")]
    elif scenario == "duplicate_timing":
        events.insert(-1, next(e for e in events if e["event"] == "timing"))
    elif scenario == "duplicate_result":
        events.append(events[-1])
    elif scenario == "memory":
        next(e for e in events if e["event"] == "memory")["owned_device_bytes"] = 0
    elif scenario == "slicing":
        next(e for e in events if e["event"] == "plan")["num_slices"] = 2
    elif scenario == "fabricated_estimate":
        next(e for e in events if e["event"] == "estimates")["flops"] = 0
    elif scenario == "malformed_shape":
        events = [[]]
    elif scenario == "nan":
        next(e for e in events if e["event"] == "timing")["seconds"] = float("nan")
    elif scenario in ("timeout", "partial_timeout", "fatal_timeout", "cleanup_timeout", "descendant"):
        events = []
        if scenario == "fatal_timeout":
            events = [{"event": "operation_error", "status": "failed", "error": "metadata changed"}]
        if scenario == "cleanup_timeout":
            events = [{"event": "cleanup", "owner": "execution", "error": "close failed"}]
        if scenario == "descendant":
            process = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), "--sleeper"])
            (directory / "descendant.pid").write_text(str(process.pid))
    with (directory / "events.jsonl").open("x") as stream:
        for event in events:
            stream.write(json.dumps(event) + "\n")
        if scenario in ("partial", "partial_timeout"):
            stream.write('{"event":')
        if scenario == "malformed":
            stream.write("not json\n")
    if scenario == "missing_journal":
        (directory / "events.jsonl").unlink()
    if scenario not in ("missing_readback", "workspace_limit", "allocation_failed"):
        (directory / "case_a_4x4-0.complex64le").write_bytes(
            bytes(8 if scenario == "readback_shape" else 65536 * 16)
        )
    if scenario.endswith("timeout") or scenario == "descendant":
        if scenario != "descendant":
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
        time.sleep(60)
    time.sleep(0.05)
    return 1 if scenario in ("workspace_limit", "allocation_failed", "numerical", "wrong_exit") else 0


class DriverTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="qdk-host-protocol-only-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def trial(self, scenario, configuration=None, sampler=sample, seconds=5):
        directory = self.root / scenario
        result = driver.run_trial(
            command(scenario), directory, configuration or config(), seconds, sampler
        )
        self.assertEqual(result, json.loads((directory / "summary.json").read_text()))
        return result

    def test_default_grid(self):
        args = driver.argument_parser().parse_args([
            "--output", str(self.root), "--expected-head", "0" * 40,
        ])
        configs = list(driver.configurations(
            args.hyper_samples, args.reconfigurations, args.seeds, args.repeats
        ))
        self.assertEqual(len(configs), 9)
        self.assertEqual(configs[0], config())
        self.assertEqual(
            {
                (c["hyper_samples"], c["reconfiguration_iterations"],
                 c["disable_rank_simplification"], c["seed"])
                for c in configs[1:]
            },
            {
                (samples, reconfiguration, disabled, 17)
                for samples in (1, 64)
                for reconfiguration in (0, 500)
                for disabled in (True, False)
            },
        )
        self.assertTrue(all(c["repeats"] == 5 for c in configs))
        self.assertEqual(args.trial_seconds, 300)
        self.assertEqual(args.campaign_seconds, 6 * 3600)
        self.assertEqual(driver.integers("0,64,500"), [0, 64, 500])
        for text in ("-1", "1,1", "2147483648"):
            with self.assertRaises(argparse.ArgumentTypeError):
                driver.integers(text)

    def test_explicit_full_grid_remains_available(self):
        args = driver.argument_parser().parse_args([
            "--output", str(self.root), "--expected-head", "0" * 40,
            "--hyper-samples", "1,8,64", "--reconfigurations", "0,64,500",
            "--seeds", "17,29,43",
        ])
        configs = list(driver.configurations(
            args.hyper_samples, args.reconfigurations, args.seeds, args.repeats
        ))
        self.assertEqual(len(configs), 55)
        self.assertEqual(configs[0], config())
        self.assertEqual(len({json.dumps(c, sort_keys=True) for c in configs}), 55)

    def test_success_timings_policy_hashes_and_observed_memory(self):
        for source in ("chronological", "optimizer"):
            with self.subTest(source=source):
                result = driver.run_trial(
                    command("passed"), self.root / source, config(source), 5, sample
                )
                self.assertEqual(result["status"], "passed")
                self.assertEqual(result["timings_seconds"]["contract_readback_0"], 1)
                self.assertEqual(result["repeated_readback_median_seconds"], 4)
                self.assertEqual(result["repeated_readback_min_seconds"], 2)
                self.assertEqual(result["repeated_readback_max_seconds"], 6)
                self.assertEqual(len(result["comparisons"]), 6)
                self.assertEqual(result["observed_gpu_process_peak_bytes"], 4096)
                self.assertEqual(result["observed_host_peak_rss_bytes"], 8192)
                self.assertEqual(len(result["readbacks_sha256"]), 1)
                self.assertEqual(len(result["native_libraries_sha256"]), 2)
                self.assertEqual(result["environment"]["host_scratch_ceiling"], None)
                self.assertEqual(result["memory"]["device_scratch_allocated"], 256)
                self.assertEqual(result["memory"]["selected_input_bytes"], 352)
                self.assertEqual(result["memory"]["selected_input_count"], 6)
                self.assertEqual(result["memory"]["resident_input_bytes"], 352)
                self.assertEqual(result["memory"]["resident_input_count"], 6)
                self.assertNotIn("import", result["timings_seconds"])

    def test_resource_snapshots_follow_registration_and_complete_selection(self):
        for source in ("chronological", "optimizer"):
            configuration = config(source)
            events = successful_events(configuration)
            snapshots = [event for event in events if event["event"] == "memory"]
            self.assertIsNone(snapshots[0]["selected_input_bytes"])
            self.assertIsNone(snapshots[0]["selected_input_count"])
            self.assertEqual(snapshots[0]["resident_input_count"], 6)
            result = driver.summarize(events, configuration, 0, False)
            self.assertEqual(result["status"], "passed", result)
            self.assertEqual(result["memory"], snapshots[-1])

    def test_missing_or_inconsistent_resource_snapshots_are_fatal(self):
        for corruption in (
            "missing_initial", "missing_final", "duplicate", "early_selection",
            "unknown_final_selection", "wrong_selected_count", "wrong_selected_bytes",
            "noninteger_selected_count", "noninteger_selected_bytes",
            "wrong_resident_count", "wrong_resident_bytes", "unknown_requirement",
            "changed_requirement",
        ):
            with self.subTest(corruption=corruption):
                events = successful_events(config())
                initial, final = [event for event in events if event["event"] == "memory"]
                if corruption == "missing_initial":
                    events.remove(initial)
                elif corruption == "missing_final":
                    events.remove(final)
                elif corruption == "duplicate":
                    events.insert(-1, final.copy())
                elif corruption == "early_selection":
                    initial.update(selected_input_bytes=352, selected_input_count=6)
                elif corruption == "unknown_final_selection":
                    final.update(selected_input_bytes=None, selected_input_count=None)
                elif corruption == "wrong_selected_count":
                    final["selected_input_count"] = 448
                elif corruption == "wrong_selected_bytes":
                    final["selected_input_bytes"] = 0
                elif corruption == "noninteger_selected_count":
                    final["selected_input_count"] = 6.0
                elif corruption == "noninteger_selected_bytes":
                    final["selected_input_bytes"] = 352.0
                elif corruption == "wrong_resident_count":
                    final["resident_input_count"] = 0
                elif corruption == "wrong_resident_bytes":
                    final["resident_input_bytes"] = 0
                elif corruption == "unknown_requirement":
                    initial["device_scratch_minimum"] = None
                elif corruption == "changed_requirement":
                    final["device_cache_recommended"] = 1
                result = driver.summarize(events, config(), 0, False)
                self.assertEqual(result["status"], "failed", result)

    def test_inclusive_numerical_thresholds(self):
        self.assertEqual(self.trial("boundary")["status"], "passed")
        self.assertEqual(self.trial("threshold")["status"], "failed")

    def test_malformed_incomplete_or_inconsistent_evidence_is_fatal(self):
        for scenario in (
            "numerical", "cleanup", "missing_comparison", "missing_timing", "duplicate_timing",
            "duplicate_result", "memory", "slicing", "fabricated_estimate", "malformed_shape",
            "nan", "partial", "malformed", "missing_journal", "missing_readback",
            "readback_shape", "wrong_exit",
        ):
            with self.subTest(scenario=scenario):
                self.assertEqual(self.trial(scenario)["status"], "failed")

    def test_resource_rejections_continue_and_keep_measurements_absent(self):
        outcome = driver.campaign(command("sequence"), self.root, [config()] * 3, 5, 30, sample)
        self.assertEqual(outcome["reason"], "completed")
        self.assertEqual(outcome["trials_completed"], 3)
        self.assertEqual(outcome["ranked_successful_trials"], ["trial-002"])
        rows = [
            json.loads(line) for line in (self.root / "results.jsonl").read_text().splitlines()
        ]
        self.assertEqual([row["status"] for row in rows], ["workspace_limit", "allocation_failed", "passed"])
        self.assertIsNone(rows[0]["memory"])
        self.assertNotIn("repeated_readback_median_seconds", rows[0])
        self.assertEqual(rows[0]["workspace_rejection"]["required"], 32 * 1024**3 + 1)
        csv = (self.root / "results.csv").read_text()
        self.assertIn("prepare_host_call_seconds", csv)
        self.assertNotIn("import_seconds", csv)

    def test_fatal_trial_stops_campaign(self):
        outcome = driver.campaign(command("cleanup"), self.root, [config()] * 2, 5, 30, sample)
        self.assertEqual(outcome["reason"], "trial_failure")
        self.assertEqual(outcome["trials_completed"], 1)
        self.assertFalse((self.root / "trial-001").exists())
        self.assertEqual(outcome, json.loads((self.root / "campaign.json").read_text()))

    def test_timeout_retains_partial_evidence_but_never_masks_fatal_event(self):
        for scenario, expected in [
            ("timeout", "timeout"), ("partial_timeout", "timeout"),
            ("fatal_timeout", "failed"), ("cleanup_timeout", "failed"),
        ]:
            with self.subTest(scenario=scenario):
                result = self.trial(scenario, seconds=0.3)
                self.assertEqual(result["status"], expected)
                self.assertNotEqual(result["returncode"], 0)
                self.assertFalse(Path(f"/proc/{result['pid']}").exists())

    def test_timeout_continues_but_campaign_deadline_stops(self):
        output = self.root / "timeouts"
        output.mkdir()
        outcome = driver.campaign(command("timeout"), output, [config()] * 2, 0.2, 20, sample)
        self.assertEqual(outcome["reason"], "completed")
        self.assertEqual(outcome["trials_completed"], 2)
        output = self.root / "deadline"
        output.mkdir()
        outcome = driver.campaign(command("timeout"), output, [config()] * 2, 30, 0.2, sample)
        self.assertEqual(outcome["reason"], "campaign_deadline")
        self.assertEqual(outcome["trials_completed"], 1)
        self.assertFalse((output / "trial-001").exists())

    def test_termination_kills_descendants_even_when_leader_exits(self):
        result = self.trial("descendant", seconds=0.4)
        self.assertEqual(result["status"], "timeout")
        pid = int((self.root / "descendant/descendant.pid").read_text())
        status = Path(f"/proc/{pid}/status")
        deadline = time.monotonic() + 2
        while status.exists():
            if "State:\tZ" in status.read_text():
                break
            self.assertLess(time.monotonic(), deadline, "descendant survived SIGKILL")
            time.sleep(0.01)

    def test_sampler_failure_and_interruption_are_persisted_and_stop(self):
        for name, error, expected in [
            ("sample_failure", OSError("injected sampling failure"), "trial_failure"),
            ("interruption", KeyboardInterrupt("injected interrupt"), "interrupted"),
        ]:
            with self.subTest(name=name):
                output = self.root / name
                output.mkdir()

                def failing_sampler(_pid):
                    raise error

                outcome = driver.campaign(
                    command("timeout"), output, [config()] * 2, 5, 30, failing_sampler
                )
                self.assertEqual(outcome["reason"], expected)
                summary = json.loads((output / "trial-000/summary.json").read_text())
                self.assertIn(type(error).__name__, summary["driver_error"])
                self.assertFalse(Path(f"/proc/{summary['pid']}").exists())
                self.assertEqual(outcome["trials_completed"], 1)

    def test_invalid_sampler_data_is_not_a_success_shaped_memory_default(self):
        for name, observation in [
            ("missing", {}),
            ("negative", {"gpu_process_bytes": -1, "host_peak_rss_bytes": 0}),
            ("nonfinite", {"gpu_process_bytes": float("nan"), "host_peak_rss_bytes": 0}),
        ]:
            with self.subTest(name=name):
                result = driver.run_trial(
                    command("timeout"), self.root / name, config(), 5,
                    lambda _pid: observation,
                )
                self.assertEqual(result["status"], "failed")
                self.assertIn("driver_error", result)

    def test_signal_interrupt_persists_trial_and_campaign_and_reaps_child(self):
        process = subprocess.Popen(
            [sys.executable, str(Path(__file__).resolve()), "--campaign-child", str(self.root)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        try:
            pidfile = self.root / "trial-000/child.pid"
            deadline = time.monotonic() + 5
            while not pidfile.exists() and process.poll() is None and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(pidfile.exists(), "injected trial did not start")
            process.send_signal(signal.SIGTERM)
            stdout, stderr = process.communicate(timeout=10)
            self.assertEqual(process.returncode, 1, (stdout, stderr))
            outcome = json.loads((self.root / "campaign.json").read_text())
            self.assertEqual(outcome["reason"], "interrupted")
            summary = json.loads((self.root / "trial-000/summary.json").read_text())
            self.assertEqual(summary["status"], "interrupted")
            self.assertFalse(Path(f"/proc/{summary['pid']}").exists())
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate()

    def test_spawn_failure_and_existing_trial_have_explicit_evidence(self):
        summary = driver.run_trial(
            [str(self.root / "missing-executable")], self.root / "spawn", config(), 1, sample
        )
        self.assertEqual(summary["status"], "failed")
        self.assertIsNone(summary["pid"])
        (self.root / "trial-000").mkdir()
        marker = self.root / "trial-000/untouched"
        marker.write_text("preserve")
        outcome = driver.campaign(command("passed"), self.root, [config()], 5, 30, sample)
        self.assertEqual(outcome["reason"], "driver_failure")
        self.assertEqual(marker.read_text(), "preserve")
        with self.assertRaises(FileExistsError):
            driver.campaign(command("passed"), self.root, [config()], 5, 30, sample)

    def test_existing_trial_and_json_are_never_overwritten(self):
        self.trial("passed")
        summary = self.root / "passed/summary.json"
        original = summary.read_bytes()
        with self.assertRaises(FileExistsError):
            self.trial("passed")
        with self.assertRaises(FileExistsError):
            driver.write_json(summary, {"replacement": True})
        self.assertEqual(summary.read_bytes(), original)

    def test_selector_and_qualification_guard_against_zero_test_success(self):
        with self.assertRaises(ValueError):
            driver.check_selector(command("zero_list"), driver.TEST)
        for scenario in ("qualification", "zero_run"):
            output = self.root / scenario
            output.mkdir()
            if scenario == "zero_run":
                with self.assertRaises(ValueError):
                    driver.preflight(command(scenario), output)
                self.assertFalse((output / "b_case_a_2x2").exists())
            else:
                driver.preflight(command(scenario), output)
                self.assertEqual(len(list(output.glob("*/*.complex64le"))), 6)

    def test_source_fixture_and_binary_identity_changes_are_rejected(self):
        def git(*args):
            subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True)

        git("init", "-q")
        fixture = self.root / "samples/python_interop/ising2d_tensor_network_demo/fixtures/case_a_4x4/input"
        fixture.parent.mkdir(parents=True)
        fixture.write_text("host provenance fixture")
        git("add", ".")
        git("-c", "user.name=Host Test", "-c", "user.email=host-test@example.invalid", "commit", "-qm", "fixture")
        before = driver.source_snapshot(self.root)
        driver.verify_unchanged(before, driver.source_snapshot(self.root), "hash", "hash")
        fixture.write_text("changed")
        with self.assertRaises(ValueError):
            driver.verify_unchanged(before, driver.source_snapshot(self.root), "hash", "hash")
        with self.assertRaises(ValueError):
            driver.verify_unchanged(before, before, "hash", "changed")
        after = copy.deepcopy(before)
        after["head"] = "other"
        with self.assertRaises(ValueError):
            driver.verify_unchanged(before, after, "hash", "hash")
        after = copy.deepcopy(before)
        after["branch_refs"] += "refs/heads/changed other\n"
        with self.assertRaises(ValueError):
            driver.verify_unchanged(before, after, "hash", "hash")
        dirty = copy.deepcopy(before)
        dirty["source_status"] = " M input"
        with self.assertRaises(ValueError):
            driver.verify_unchanged(dirty, dirty, "hash", "hash")


if __name__ == "__main__":
    if "--child" in sys.argv:
        sys.exit(child(sys.argv[sys.argv.index("--child") + 1]))
    if "--sleeper" in sys.argv:
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        time.sleep(60)
    elif "--campaign-child" in sys.argv:
        signal.signal(signal.SIGTERM, driver.stop_on_signal)
        output = Path(sys.argv[sys.argv.index("--campaign-child") + 1])
        outcome = driver.campaign(command("timeout"), output, [config()] * 2, 30, 60, sample)
        sys.exit(0 if outcome["reason"] == "completed" else 1)
    else:
        unittest.main()
