# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Fast self-tests; no compiler build, installed qdk, or network is required."""

import ast
import json
import io
import os
from pathlib import Path
import sys
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from harness.analysis import (  # noqa: E402
    callable_stages, compare_runs, hierarchy_violations, match_issues, normalize_signature,
    parse_selection, select_cases, summarize_run,
)
from harness.builds import build_revision, refresh_timestamps, verify_build  # noqa: E402
from harness.cases import load_cases, snapshot_project  # noqa: E402
from harness.common import (  # noqa: E402
    KIT, ReviewError, isolated, load_config, ordered_roles, read_json, select_roles, tree_digest, validate,
    write_json,
)
from harness.diffscan import scan_diff_text  # noqa: E402
from harness import evidence  # noqa: E402
from harness.issue_drafts import draft_issues, load_definitions, markdown as issue_markdown, render  # noqa: E402
from harness.tables import cell as table_cell, format_value, run_table  # noqa: E402
from harness.timing import aggregate, ratios  # noqa: E402
from harness.variants import run_variants, variant_lines  # noqa: E402
from harness.variants_worker import load_variants  # noqa: E402
from harness.discovery import discover_samples, infer_entry, snapshot_issues  # noqa: E402
from harness.execution import (  # noqa: E402
    enclosing_function, exec_script, last_exception, parse_panics, reproduction_status,
)
from harness.extraction import convert_dump, extract_tests, insert_at_function_start, instrument  # noqa: E402
from harness.investigate import (  # noqa: E402
    adapter_variants, ddmin, qsharp_constants, render_adapter, replace_constant,
)
from harness.manifests import export_run_cases, subset_manifests  # noqa: E402
from harness.records import encode, parse_output, validate_value  # noqa: E402
from harness.report import generate_report, source_repro  # noqa: E402
from harness.runner import compare, create_run, endpoint, prune_qir, run_case  # noqa: E402
from harness.statecheck import check as statecheck  # noqa: E402
from harness import variants_worker, worker  # noqa: E402

sys.path.insert(0, str(KIT / "generators"))
import callable_chains  # noqa: E402
import dynamic_recursion  # noqa: E402


ONE = {"type": "result", "value": 1}
ZERO = {"type": "result", "value": 0}
CASE = {
    "id": "test",
    "source": "operation Main() : Result { Zero }",
    "entry": "Main()",
    "profiles": ["Base"],
    "shots": 2,
    "seed": 1,
    "oracle": {"kind": "deterministic"},
}


def good(value=ONE):
    return {
        "simulator": {"status": "ok", "values": [value, value]},
        "profiles": {
            "Base": {
                "compile": {"status": "ok"},
                "runner": {"status": "ok", "values": [value, value]},
            }
        },
    }


def frame(*records):
    return "START\nMETADATA\tentry_point\n" + "\n".join(records) + "\nEND\t0\n"


class ConfigurationTests(unittest.TestCase):
    def test_bundled_smoke_config_resolves_repository(self):
        with patch("harness.common.resolve_ref", side_effect=lambda repo, ref: ref):
            config = load_config(KIT / "tests" / "smoke.json")
        self.assertEqual(Path(config["repository"]), KIT.parents[1])

    def test_config_outside_checkout_uses_sandbox_defaults(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "review.json"
            write_json(path, {"reference": "old", "candidate": "new"})
            with patch("harness.common.git", return_value=""), patch(
                "harness.common.resolve_ref", side_effect=lambda repo, ref: ref + "-sha"
            ), patch.dict(os.environ, {"REVIEW_KIT_CACHE": ""}):
                config = load_config(path)
            self.assertEqual(config["refs"], {"reference": "old-sha", "candidate": "new-sha"})
            self.assertEqual(config["repository"], "/work/qdk")
            self.assertEqual(config["venv"], "/opt/qdk-venv")
            self.assertEqual(config["work_dir"], str(Path(tempfile.gettempdir()) / "compiler-review-kit/review"))
            self.assertEqual(config["build_dir"], "/work/compiler-review-kit/build")
            self.assertEqual(config["cache_dir"], "/work/cache/compiler-review-kit")
            self.assertEqual(config["report"], "/exchange/from_copilot/review.md")
            self.assertEqual(config["profiles"], ["Base", "Adaptive_RIF", "Adaptive"])
            self.assertEqual(config["debug_validation"], "none")

    def test_explicit_debug_and_adaptive_ri_remain_supported(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "review.json"
            for mode in ("targeted", "all"):
                with self.subTest(mode=mode):
                    write_json(path, {"reference": "old", "candidate": "new",
                                      "profiles": ["Adaptive_RI"], "debug_validation": mode})
                    with patch("harness.common.git", return_value=""), patch(
                        "harness.common.resolve_ref", side_effect=lambda repo, ref: ref
                    ):
                        config = load_config(path)
                    self.assertEqual(config["profiles"], ["Adaptive_RI"])
                    self.assertEqual(config["debug_validation"], mode)

    def test_path_overrides_remain_config_relative(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "config" / "review.json"
            overrides = {"repository": "../repo", "venv": "../python", "work_dir": "scratch",
                         "build_dir": "../builds", "cache_dir": "../evidence", "report": "out/report.md"}
            write_json(path, {"reference": "old", "candidate": "new", **overrides})
            with patch("harness.common.git", return_value=""), patch(
                "harness.common.resolve_ref", side_effect=lambda repo, ref: ref
            ), patch.dict(os.environ, {"REVIEW_KIT_CACHE": "/other-cache"}):
                config = load_config(path)
            for field, value in overrides.items():
                with self.subTest(field=field):
                    self.assertEqual(config[field], str((path.parent / value).resolve()))

    def test_named_reviews_isolate_scratch_and_reports(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "review.json"
            configs = []
            with patch("harness.common.git", return_value=""), patch(
                "harness.common.resolve_ref", side_effect=lambda repo, ref: ref
            ):
                for name in ("first", "second"):
                    write_json(path, {"reference": "old", "candidate": "new", "name": name})
                    configs.append(load_config(path))
            self.assertNotEqual(configs[0]["work_dir"], configs[1]["work_dir"])
            self.assertEqual(configs[0]["report"], "/exchange/from_copilot/first.md")
            self.assertEqual(configs[1]["report"], "/exchange/from_copilot/second.md")
            self.assertEqual(configs[0]["build_dir"], configs[1]["build_dir"])

    def test_derived_name_cannot_escape_scratch_root(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "review.json"
            write_json(path, {"reference": "old", "candidate": "new", "report": ".."})
            with patch("harness.common.resolve_ref", side_effect=lambda repo, ref: ref):
                with self.assertRaisesRegex(ReviewError, "set name explicitly"):
                    load_config(path)

    def test_unknown_field(self):
        with self.assertRaisesRegex(ReviewError, "unknown"):
            validate({"reference": "a", "candidate": "b", "profiels": []}, read_json(KIT / "review.schema.json"))

    def test_bool_is_not_a_number(self):
        with self.assertRaisesRegex(ReviewError, "integer"):
            validate({"reference": "a", "candidate": "b", "limits": {"shots": True}}, read_json(KIT / "review.schema.json"))

    def test_duplicate_json_keys(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bad.json"
            path.write_text('{"candidate": "a", "candidate": "b"}')
            with self.assertRaisesRegex(ReviewError, "Duplicate"):
                read_json(path)

    def test_cases_require_explicit_oracle(self):
        with self.assertRaisesRegex(ReviewError, "oracle"):
            validate([{"id": "x", "source": "", "entry": "Main()"}], read_json(KIT / "cases.schema.json"))

    def test_duplicate_case_ids(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "cases.json"
            write_json(path, [CASE, CASE])
            with self.assertRaisesRegex(ReviewError, "duplicate"):
                load_cases(path, {"input": {}, "profiles": ["Base"], "limits": {"shots": 2, "seed": 1}}, directory)

    def test_exclusions_are_recorded(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "cases.json"
            write_json(path, [CASE])
            config = {"input": {"exclusions": [{"match": "test", "reason": "budget"}]}}
            cases, skipped = load_cases(path, config, directory)
            self.assertEqual(cases, [])
            self.assertEqual(skipped, [{"id": "test", "reason": "budget"}])


class RecordTests(unittest.TestCase):
    def test_nested_values_and_shot_multiplicity(self):
        text = frame(
            "OUTPUT\tTUPLE\t5\troot",
            "OUTPUT\tINT\t42\ti",
            "OUTPUT\tBOOL\ttrue\tb",
            "OUTPUT\tDOUBLE\t-0.0\td",
            "OUTPUT\tARRAY\t1\ta",
            "OUTPUT\tRESULT\t1\tr",
            "OUTPUT\tTUPLE\t0\tu",
        )
        parsed = parse_output(text + text, 2)
        expected = encode((42, True, -0.0, [1], None))
        expected["items"][3]["items"][0] = ONE
        self.assertEqual(parsed, [expected, expected])
        self.assertNotEqual(encode(0.0), encode(-0.0))

    def test_array_and_tuple_remain_distinct(self):
        self.assertNotEqual(encode([1]), encode((1,)))

    def test_nan_and_infinity(self):
        self.assertEqual(parse_output(frame("OUTPUT\tDOUBLE\tNaN"), 1), [encode(float("nan"))])
        self.assertEqual(parse_output(frame("OUTPUT\tDOUBLE\tinf"), 1), [encode(float("inf"))])

    def test_invalid_protocols(self):
        invalid = [
            "",
            "START\nEND\t0\n",
            "START\nSTART\n",
            frame("OUTPUT\tARRAY\t2", "OUTPUT\tINT\t1"),
            frame("OUTPUT\tARRAY\t-1"),
            frame("OUTPUT\tINT\t1", "OUTPUT\tINT\t2"),
            frame("OUTPUT\tBOOL\t1"),
            frame("OUTPUT\tRESULT\t2"),
            frame("OUTPUT\tOTHER\t1"),
            frame("OUTPUT\tINT\t1").replace("END\t0", "END\t1"),
            frame("OUTPUT\tINT\t1") + "trailing garbage\n",
        ]
        for text in invalid:
            with self.subTest(text=text), self.assertRaises(ReviewError):
                parse_output(text, 1)

    def test_wrong_shot_count(self):
        with self.assertRaisesRegex(ReviewError, "expected 2"):
            parse_output(frame("OUTPUT\tINT\t1"), 2)

    def test_expected_type_validation(self):
        for value in (ONE, encode((1, False, -0.0)), encode(float("nan"))):
            validate_value(value)
        for value in ({"type": "result", "value": True}, {"type": "double", "value": "-0.0"},
                      {"type": "tuple", "items": []}, {"type": "unit", "value": None}):
            with self.subTest(value=value), self.assertRaises(ReviewError):
                validate_value(value)
        with self.assertRaises(ReviewError):
            validate_value({"type": "double", "value": "0x1p+999999999999"})


class ClassificationTests(unittest.TestCase):
    def test_wrong_result_regression(self):
        candidate = good()
        candidate["profiles"]["Base"]["runner"]["values"] = [ZERO, ZERO]
        self.assertEqual(compare(CASE, good(), candidate)[0]["verdict"], "regression_candidate")

    def test_validated_fix(self):
        reference = good()
        reference["profiles"]["Base"]["compile"] = {"status": "panic", "message": "panic"}
        self.assertEqual(compare(CASE, reference, good())[0]["verdict"], "fix_candidate")

    def test_acceptance_is_not_a_fix(self):
        case = CASE | {"oracle": {"kind": "compile_only"}}
        reference = good()
        reference["profiles"]["Base"]["compile"] = {"status": "diagnostic"}
        self.assertEqual(compare(case, reference, good())[0]["verdict"], "newly_accepted_unvalidated")

    def test_stochastic_observations_not_miscompile(self):
        case = CASE | {"oracle": {"kind": "observe"}}
        candidate = good(ZERO)
        self.assertEqual(compare(case, good(), candidate)[0]["verdict"], "inconclusive")

    def test_changed_simulator_without_expected_is_ambiguous(self):
        self.assertEqual(compare(CASE, good(), good(ZERO))[0]["verdict"], "inconclusive")

    def test_unit_is_not_an_oracle(self):
        unit = {"type": "unit"}
        self.assertEqual(endpoint(CASE, good(unit), "Base"), "unit_unvalidated")

    def test_runner_failure_not_success(self):
        candidate = good()
        candidate["profiles"]["Base"]["runner"] = {"status": "runner_error", "returncode": 1}
        self.assertEqual(endpoint(CASE, candidate, "Base"), "execution_failure")

    def test_parser_failure_is_infrastructure(self):
        candidate = good()
        candidate["profiles"]["Base"]["runner"] = {"status": "parser_error"}
        self.assertEqual(endpoint(CASE, candidate, "Base"), "parser_error")
        self.assertEqual(compare(CASE, good(), candidate)[0]["verdict"], "inconclusive")

    def test_explicit_expected_detects_shared_wrong_answer(self):
        case = CASE | {"oracle": {"kind": "deterministic", "expected": ONE}}
        self.assertEqual(endpoint(case, good(ZERO), "Base"), "wrong_result")

    def test_simulator_panic_is_not_hidden_by_successful_qir(self):
        candidate = good()
        candidate["simulator"] = {"status": "panic", "message": "native panic"}
        self.assertEqual(compare(CASE, good(), candidate)[-1]["verdict"], "simulator_regression_candidate")

    def test_other_profiles_continue_after_panic(self):
        case = CASE | {"profiles": ["Base", "Adaptive"]}
        config = {"keep_qir": False}
        build = {"sha": "sha", "mode": "release"}
        calls = []

        def fake_stage(config, build, case, name, profile=None, qir_path=None):
            calls.append((name, profile))
            if name == "simulate":
                return {"status": "timeout"}
            if name == "compile":
                return {"status": "panic"} if profile == "Base" else {"status": "ok", "qir": "qir"}
            return {"status": "ok", "values": [ONE, ONE]}

        with tempfile.TemporaryDirectory() as directory, patch("harness.runner.stage", side_effect=fake_stage):
            result = run_case(config, build, case, directory)
        self.assertEqual(result["profiles"]["Base"]["compile"]["status"], "panic")
        self.assertEqual(result["profiles"]["Adaptive"]["runner"]["status"], "ok")
        self.assertIn(("compile", "Adaptive"), calls)

    def test_historical_failure_qir_is_kept(self):
        with tempfile.TemporaryDirectory() as directory:
            results = {"reference": good(), "candidate": good(), "historical_reference": good()}
            results["historical_reference"]["profiles"]["Base"]["runner"]["values"] = [ZERO, ZERO]
            for name, result in results.items():
                path = Path(directory) / f"{name}.ll"
                path.write_text("raw")
                result["profiles"]["Base"]["compile"]["qir_path"] = str(path)
            prune_qir({"keep_qir": False}, CASE, results)
            self.assertTrue((Path(directory) / "historical_reference.ll").exists())


class ProcessTests(unittest.TestCase):
    def call(self, code, timeout=3):
        with tempfile.TemporaryDirectory() as directory:
            return isolated(
                [sys.executable, "-c", code], payload={}, env=os.environ.copy(),
                cwd=directory, timeout=timeout,
            )

    def test_timeout(self):
        self.assertEqual(self.call("import time; time.sleep(20)", timeout=0.2)["status"], "timeout")

    def test_signal(self):
        self.assertEqual(self.call("import os,signal; os.kill(os.getpid(), signal.SIGKILL)")["status"], "crash")

    def test_missing_json(self):
        self.assertEqual(self.call("print('not json')")["status"], "infrastructure_error")

    def test_nonzero_with_success_shaped_stdout(self):
        result = self.call('import sys; print(\'{"status":"ok"}\'); sys.exit(1)')
        self.assertEqual(result["status"], "infrastructure_error")

    def test_stderr_preserved(self):
        result = self.call('import sys; print("stage",file=sys.stderr); print(\'{"status":"panic"}\')')
        self.assertEqual(result["status"], "panic")
        self.assertIn("stage", result["stderr"])


class WorkerBoundaryTests(unittest.TestCase):
    def call(self, error):
        output = io.StringIO()
        with patch("harness.worker.execute", side_effect=error), patch(
            "sys.stdin", io.StringIO("{}")
        ), patch("sys.stdout", output):
            worker.main()
        return json.loads(output.getvalue())

    def test_panic_exception_is_a_base_exception(self):
        class PanicException(BaseException):
            pass

        self.assertEqual(self.call(PanicException("native panic"))["status"], "panic")

    def test_python_error_is_infrastructure(self):
        result = self.call(ValueError("bad adapter"))
        self.assertEqual(result["status"], "infrastructure_error")
        self.assertIn("bad adapter", result["traceback"])

    def test_interrupt_is_not_swallowed(self):
        with self.assertRaises(KeyboardInterrupt):
            self.call(KeyboardInterrupt())


class HelperTests(unittest.TestCase):
    def test_package_boundaries_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            copied = snapshot_project(KIT / "examples/project", Path(directory), {})
            manifest = read_json(copied / "qsharp.json")
            dependency = Path(manifest["dependencies"]["Dependency"]["path"])
            self.assertNotEqual(copied, dependency)
            self.assertTrue((dependency / "src/MeasureOne.qs").exists())
            self.assertFalse((copied / "src/MeasureOne.qs").exists())

    def test_ddmin_preserves_predicate(self):
        self.assertEqual(ddmin(["a", "bug", "b", "c"], lambda parts: "bug" in parts), ["bug"])

    def test_inference_is_conservative(self):
        self.assertEqual(infer_entry("namespace A { @EntryPoint() operation Go() : Unit {} }"), "A.Go()")
        self.assertIsNone(infer_entry("operation Main(x : Int) : Unit {}"))
        self.assertIsNone(infer_entry("namespace A {} namespace B { operation Main() : Unit {} }"))

    def test_issue_selection_includes_closed_and_explicit_older_issues(self):
        config = {
            "repository": "/unused",
            "refs": {"reference": "old", "candidate": "new"},
            "input": {"issues": {"author": "amcasey", "min_number": 3461, "include": [3440]}},
        }
        requests = []

        def api(endpoint, paginated=False):
            requests.append(endpoint)
            if "?creator=" in endpoint:
                return [{"number": 3500}, {"number": 3400}]
            if "/comments?" in endpoint:
                return []
            number = int(endpoint.rsplit("/", 1)[1])
            return {"number": number, "html_url": "url", "title": "title", "state": "closed", "body": "body", "updated_at": "date"}

        with tempfile.TemporaryDirectory() as directory, patch("harness.discovery._api", side_effect=api), patch(
            "harness.discovery.git", return_value="Fix #3600"
        ):
            path = Path(directory) / "issues.json"
            snapshot_issues(config, path)
            snapshot = read_json(path)
        self.assertEqual([i["number"] for i in snapshot["issues"]], [3440, 3500, 3600])
        self.assertTrue(any("state=all" in r for r in requests))

    def test_changed_package_cache_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "qdk").mkdir()
            (root / "qdk/file.py").write_text("before")
            build = {"overlay": directory, "tools": {}, "package_digest": tree_digest(root / "qdk")}
            (root / "qdk/file.py").write_text("after")
            with self.assertRaisesRegex(ReviewError, "package changed"):
                verify_build(build, {})

    def test_inline_repro_is_valid_python(self):
        compile(source_repro(CASE), "<repro>", "exec")

    def test_report_handles_incomplete_run_and_protects_existing_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_json(root / "manifest.json", {
                "config": {"refs": {"reference": "old", "candidate": "new"}},
                "mode": "release", "status": "failed", "skipped": [],
            })
            write_json(root / "cases.json", [CASE])
            (root / "results.jsonl").write_text("")
            output = root / "report.md"
            generate_report(root, output)
            self.assertIn("run status: `failed`", output.read_text())
            with self.assertRaisesRegex(ReviewError, "Report exists"):
                generate_report(root, output)


def comparison_row(case_id, statuses, message="", verdicts=None):
    """A results.jsonl row; statuses maps profile -> (reference, candidate)."""
    rows, profiles = [], {}
    for profile, (old, new) in statuses.items():
        verdict = (verdicts or {}).get(profile) or (
            "regression_candidate" if old == "validated" and new != "validated"
            else "fix_candidate" if new == "validated" and old != "validated"
            else "validated_unchanged" if old == new == "validated"
            else "pre_existing_or_changed_failure"
        )
        rows.append({"profile": profile, "reference": old, "candidate": new, "verdict": verdict})
        compiled = {"status": "ok", "seconds": 0.5} if new == "validated" else {"status": "panic", "message": message, "seconds": 0.5}
        profiles[profile] = {"compile": compiled}
    result = {"profiles": profiles}
    return {"case": case_id, "results": {"reference": result, "candidate": result}, "comparison": rows}


def write_run(root, rows, refs=None, cases_path=None):
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    write_json(root / "manifest.json", {
        "config": {"refs": refs or {"reference": "a" * 40, "candidate": "b" * 40}},
        "status": "complete", "cases_path": str(cases_path or root / "cases.json"),
    })
    write_json(root / "cases.json", [CASE | {"id": row["case"]} for row in rows])
    (root / "results.jsonl").write_text("".join(json.dumps(row) + "\n" for row in rows))
    return root


class AnalysisTests(unittest.TestCase):
    def test_signature_normalization_groups_ids(self):
        a = normalize_signature("PostArgPromote: Expr 67 passes Expr 70\nsecond line")
        b = normalize_signature("\n  PostArgPromote: Expr 1   passes Expr 2")
        self.assertEqual(a, b)
        self.assertEqual(a, "PostArgPromote: Expr N passes Expr N")

    def test_profile_hierarchy(self):
        statuses = {"Base": "rejected", "Adaptive_RI": "validated", "Adaptive_RIF": "validated", "Adaptive": "rejected"}
        self.assertEqual(
            hierarchy_violations(statuses),
            [("Adaptive_RI", "Adaptive", "rejected"), ("Adaptive_RIF", "Adaptive", "rejected")],
        )
        self.assertEqual(hierarchy_violations({"Adaptive_RIF": "validated", "Adaptive": "validated"}), [])

    def test_summary_tables_signatures_and_new_hierarchy_violations(self):
        with tempfile.TemporaryDirectory() as directory:
            run = write_run(directory, [
                comparison_row("one", {"Adaptive_RIF": ("validated", "validated"), "Adaptive": ("validated", "panic")},
                               "Local 4 is not bound"),
                comparison_row("two", {"Adaptive_RIF": ("panic", "panic"), "Adaptive": ("panic", "panic")},
                               "Local 9 is not bound"),
            ])
            summary = summarize_run(run)
        self.assertEqual(summary["verdicts"]["regression_candidate"], 1)
        self.assertEqual(summary["flagged"][0]["signature"], "Local N is not bound")
        groups = {(g["profile"], g["signature"]): g for g in summary["candidate_failures"]}
        self.assertEqual(groups[("Adaptive", "Local N is not bound")]["count"], 2)
        self.assertEqual(groups[("Adaptive", "Local N is not bound")]["reference"], {"validated": 1, "panic": 1})
        self.assertEqual(summary["hierarchy"]["candidate"], 1)
        self.assertEqual(summary["hierarchy"]["new_at_candidate"][0]["case"], "one")

    def test_run_comparison_selection_and_export(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "cases" / "cases.json"
            (root / "cases" / "adapters").mkdir(parents=True)
            (root / "cases" / "adapters" / "x.py").write_text("def prepare(ctx):\n    return 'Main()', ()\n")
            write_json(manifest, [CASE | {"id": "fixed"}, CASE | {"id": "lost"},
                                  {"id": "py", "adapter": "adapters/x.py", "oracle": {"kind": "compile_only"}}])
            old = write_run(root / "old", [
                comparison_row("fixed", {"Base": ("panic", "panic")}),
                comparison_row("lost", {"Base": ("panic", "validated")}),
            ], cases_path=manifest)
            new = write_run(root / "new", [
                comparison_row("fixed", {"Base": ("panic", "validated")}),
                comparison_row("lost", {"Base": ("panic", "panic")}),
            ], cases_path=manifest)
            result = compare_runs(old, new)
            directions = {c["examples"][0]: c["direction"] for c in result["changes"]}
            self.assertEqual(directions, {"fixed": "better", "lost": "worse"})
            self.assertEqual(result["reference_changes"], [])
            chosen = select_cases(old, new, "a=validated,b!=validated")
            self.assertEqual(chosen, ["lost"])
            output = root / "out" / "lost.json"
            export_run_cases(new, chosen, output)
            self.assertEqual([c["id"] for c in read_json(output)], ["lost"])
        with self.assertRaises(ReviewError):
            parse_selection("a==validated")

    def test_callable_stage_classifier(self):
        source = """
            struct Box { F : (Qubit => Unit) }
            function Make(op : (Qubit => Unit)) : (Qubit => Unit) { q => op(q) }
            operation Main() : Unit {
                let f = if true { H } else { X };
                ApplyToEach(Make(f), []);
            }
        """
        self.assertTrue({"factory", "closure", "record", "selection", "library"} <= callable_stages(source))
        self.assertEqual(callable_stages("operation Main() : Unit {}"), set())

    def test_issue_matching_uses_stable_prefix(self):
        snapshot = {"issues": [
            {"number": 1, "state": "open", "title": "[Panic] something",
             "body": "PostArgPromote/PostAll call invariant violation: Expr 3 passes Expr 5"},
            {"number": 2, "state": "closed", "title": "unrelated", "body": "no panic here"},
        ]}
        result = match_issues(snapshot, ["PostArgPromote/PostAll call invariant violation: Expr 67 passes Expr 70"])
        self.assertEqual([i["number"] for i in next(iter(result.values()))["issues"]], [1])


class ManifestTests(unittest.TestCase):
    def test_subset_rebases_relative_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a" / "adapters").mkdir(parents=True)
            (root / "a" / "adapters" / "x.py").write_text("")
            write_json(root / "a" / "cases.json", [
                {"id": "py-1", "adapter": "adapters/x.py", "oracle": {"kind": "compile_only"}}, CASE,
            ])
            write_json(root / "b.json", [CASE | {"id": "other"}])
            output = root / "c" / "d" / "subset.json"
            count = subset_manifests([root / "a" / "cases.json", root / "b.json"], output, ids=["other"], patterns=["py-*"])
            cases = read_json(output)
            self.assertEqual(count, 2)
            self.assertEqual((output.parent / cases[0]["adapter"]).resolve(), (root / "a/adapters/x.py").resolve())
            with self.assertRaisesRegex(ReviewError, "not found"):
                subset_manifests([root / "b.json"], output, ids=["missing"])
            # The same ID with the same content (provenance aside) is merged; different content is an error.
            write_json(root / "b2.json", [CASE | {"id": "other", "origin": "elsewhere"}])
            self.assertEqual(subset_manifests([root / "b.json", root / "b2.json"], output, patterns=["*"]), 1)
            write_json(root / "b3.json", [CASE | {"id": "other", "entry": "Other()"}])
            with self.assertRaisesRegex(ReviewError, "different content"):
                subset_manifests([root / "b.json", root / "b3.json"], output, patterns=["*"])


class ExecutionTests(unittest.TestCase):
    STDERR = (
        "thread '<unnamed>' (1234) panicked at source/compiler/qsc_x/src/a.rs:12:9:\n"
        "first message\nnote: run with RUST_BACKTRACE\n"
        "thread '<unnamed>' (1234) panicked at source/compiler/qsc_x/src/a.rs:12:9:\n"
        "first message\n"
        "Traceback (most recent call last):\n  File \"x.py\"\npyo3_runtime.PanicException: first message\n"
    )

    def test_panic_parsing_deduplicates(self):
        panics = parse_panics(self.STDERR)
        self.assertEqual(panics, [{"path": "source/compiler/qsc_x/src/a.rs", "line": 12, "column": 9,
                                   "message": "first message"}])
        self.assertEqual(last_exception(self.STDERR), "pyo3_runtime.PanicException: first message")

    def test_enclosing_function(self):
        text = "impl A {\n    pub(crate) fn outer(\n        x: u8,\n    ) {\n        let y = 1;\n    }\n}\nfn other() {}\n"
        self.assertEqual(enclosing_function(text, 5), "outer")
        self.assertEqual(enclosing_function(text, 8), "other")
        self.assertIsNone(enclosing_function(text, 1))

    def test_reproduction_verdict_precedes_exit_code(self):
        scenarios = [
            ("REPRODUCED\n", 0, "reproduces"),
            ("REPRODUCED: Adaptive differs\n", 1, "reproduces"),
            ("NOT REPRODUCED\n", 1, "not reproduced"),
            ("NOT REPRODUCED\n", 0, "not reproduced"),
            ("REPRODUCED\nNOT REPRODUCED\n", 1, "not reproduced"),
            ("NOT REPRODUCED\nREPRODUCED\n", 0, "reproduces"),
            ("", 0, "not reproduced"),
            ("Traceback: failure\n", 1, "exit 1"),
            ("", -9, "exit -9"),
            ("", "timeout", "exit timeout"),
            ("example: NOT REPRODUCED\n", 1, "exit 1"),
        ]
        for stdout, exit_code, expected in scenarios:
            with self.subTest(stdout=stdout, exit_code=exit_code):
                self.assertEqual(reproduction_status(stdout, exit_code), expected)

    def test_exec_and_recheck_preserve_verdict_outside_stdout_tail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "repro.py"
            script.write_text("")
            stdout = "REPRODUCED: original trigger\n" + "detail\n" * 500
            proc = subprocess.CompletedProcess([], 0, stdout=stdout, stderr="")
            build = {"overlay": str(root), "worktree": str(root)}
            with patch("harness.execution.preflight", return_value={}), patch(
                "harness.execution.build_revision", return_value=build
            ), patch("harness.execution.subprocess.run", return_value=proc):
                rows = exec_script({"venv": str(root)}, script, [("candidate", "a" * 40)],
                                   "release", root / "exec", 5)
            self.assertNotIn("REPRODUCED", rows[0]["stdout_tail"])
            self.assertEqual(rows[0]["reproduction"], "reproduces")
            with patch("harness.evidence.exec_script", return_value=rows):
                table = evidence.recheck_scripts({}, [script], [], "release", root / "exec")
            output = io.StringIO()
            with patch("sys.stdout", output):
                evidence.print_recheck(table, ["candidate"])
            self.assertIn("reproduces", output.getvalue())


class ExtractionTests(unittest.TestCase):
    def test_hook_insertion(self):
        text = "fn helper<T: Into<u8>>(source: &str, expected: (u8, u8)) -> Result<(), String> where T: Copy {\n    body();\n}\n"
        patched = insert_at_function_start(text, "helper", "hook();")
        self.assertIn("{\n    hook();\n    body();", patched)
        with self.assertRaisesRegex(ReviewError, "exactly one"):
            insert_at_function_start(text + text, "helper", "hook();")
        with self.assertRaisesRegex(ReviewError, "exactly one"):
            insert_at_function_start(text, "missing", "hook();")

    def test_instrument_edits_owned_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "src/utils.rs").write_text("fn check(source: &str) {\n}\n")
            spec = {"helper_file": "src/utils.rs", "helper_code": "\n// helper\n",
                    "hooks": [{"file": "src/utils.rs", "function": "check", "code": "dump(source);"}]}
            self.assertEqual(instrument(root, spec), ["src/utils.rs"])
            self.assertEqual((root / "src/utils.rs").read_text(), "fn check(source: &str) {\n    dump(source);\n}\n\n// helper\n")

    def test_dump_conversion(self):
        main = "@EntryPoint() operation Main() : Int { 1 }"
        lines = [
            json.dumps({"kind": "semantic", "test": "a::b", "source": main, "expected": {"type": "int", "value": 1}}),
            json.dumps({"kind": "semantic", "test": "a::c", "source": main, "expected": {"type": "int", "value": 1}}),
            json.dumps({"kind": "semantic", "test": "a::d", "source": "operation Other() : Unit {}",
                        "expected": {"type": "unit"}}),
            json.dumps({"kind": "semantic", "test": "a::e", "source": "x", "expected": None}),
            json.dumps({"kind": "qir_int", "test": "a::f", "source": main.replace("1 }", "2 }"),
                        "expected": {"type": "tuple", "items": [{"type": "double_bits", "value": 4607182418800017408}]}}),
            json.dumps({"kind": "semantic", "test": "a::g", "source": main.replace("1 }", "3 }"), "expected": {"type": "int", "value": 3}}),
            json.dumps({"kind": "semantic", "test": "a::h", "source": main.replace("1 }", "3 }"), "expected": {"type": "int", "value": 4}}),
        ]
        cases, stats = convert_dump(lines, "c" * 40, "crate")
        self.assertEqual(stats, {"records": 7, "unsupported_value": 1, "no_entry": 1, "conflict": 1})
        self.assertEqual(len(cases), 2)
        doubles = [c for c in cases if c["id"].startswith("t-qir_int-")]
        self.assertEqual(doubles[0]["oracle"]["expected"], {"type": "tuple", "items": [encode(1.0)]})
        validate(cases, read_json(KIT / "cases.schema.json"))


class AdapterReductionTests(unittest.TestCase):
    ADAPTER = (
        'SRC = """\noperation A() : Unit {}\noperation B() : Unit {}\n"""\n\n'
        "def prepare(ctx):\n    ctx.eval(SRC)\n    c = ctx.code\n"
        "    f = c.Wrap(c.Pick([c.A(), c.B()]))\n    return (c.Run, (f,))\n"
    )

    def test_variants_unwrap_calls_and_drop_elements(self):
        variants = list(adapter_variants(render_adapter(__import__("ast").parse(self.ADAPTER))))
        expressions = {line.strip() for v in variants for line in v.splitlines() if line.strip().startswith("f =")}
        self.assertIn("f = c.Pick([c.A(), c.B()])", expressions)
        self.assertIn("f = c.Wrap([c.A(), c.B()])", expressions)
        self.assertIn("f = c.Wrap(c.Pick([c.B()]))", expressions)
        for variant in variants:
            compile(variant, "<variant>", "exec")

    def test_qsharp_constants_round_trip(self):
        self.assertEqual(qsharp_constants(self.ADAPTER), ["SRC"])
        reduced = replace_constant(self.ADAPTER, "SRC", "operation A() : Unit {}\n")
        self.assertIn('SRC = """operation A() : Unit {}\n"""', reduced)
        self.assertNotIn("operation B", reduced)


class GeneratorTests(unittest.TestCase):
    def test_generated_cases_are_valid_and_deterministic(self):
        with tempfile.TemporaryDirectory() as first, tempfile.TemporaryDirectory() as second:
            cases = callable_chains.generate(first, 12, 8, 6, seed=3, profiles=["Adaptive_RIF"])
            callable_chains.generate(second, 12, 8, 6, seed=3, profiles=["Adaptive_RIF"])
            self.assertEqual((Path(first) / "cases.json").read_text(), (Path(second) / "cases.json").read_text())
            validate(cases, read_json(KIT / "cases.schema.json"))
            loaded, _ = load_cases(Path(first) / "cases.json", {"input": {}, "profiles": ["Base"],
                                                                 "limits": {"shots": 2, "seed": 1}}, first)
            self.assertEqual(len(loaded), 26)
            for case in cases:
                validate_value(case["oracle"]["expected"])
                if "adapter" in case:
                    compile((Path(first) / case["adapter"]).read_text(), case["adapter"], "exec")

    def test_model_inverse_and_composition(self):
        tag = callable_chains.M.tag(5, 1, "c")
        identity = callable_chains.M.ident()
        self.assertEqual(tag.then(tag.adj()).perm, identity.perm)
        self.assertEqual(tag.then(tag.adj()).phase, 0)
        self.assertNotEqual(tag.adj().phase, tag.phase)


class StatecheckTests(unittest.TestCase):
    @staticmethod
    def qir(*calls):
        body = "\n".join(f"  call void @__quantum__qis__{c}" for c in calls)
        return f"define void @main() #0 {{\n{body}\n  ret void\n}}\nattributes #0 = {{ \"required_num_qubits\"=\"2\" }}\n"

    Q1 = "%Qubit* inttoptr (i64 1 to %Qubit*)"

    def test_unmeasured_qubit_left_flipped_is_reported(self):
        q1 = self.Q1
        result = statecheck(self.qir("x__body(%Qubit* null)", f"h__body({q1})", f"s__body({q1})", f"s__body({q1})",
                                     f"h__body({q1})", "mresetz__body(%Qubit* null, %Result* null)"))
        self.assertEqual(result["p1"], [1.0, 1.0])
        self.assertEqual(result["measured"], [0])
        self.assertEqual(result["dirty_unmeasured"], [1])

    def test_adjoint_uncomputes(self):
        q1 = self.Q1
        result = statecheck(self.qir(f"h__body({q1})", f"s__adj({q1})", f"s__body({q1})", f"h__body({q1})"))
        self.assertEqual(result["dirty_unmeasured"], [])

    def test_branching_qir_is_rejected(self):
        with self.assertRaisesRegex(ReviewError, "straight-line"):
            statecheck("br i1 %0, label %a, label %b\n\"required_num_qubits\"=\"1\"")


class ExtraRecordTests(unittest.TestCase):
    def test_strings_are_typed_simulator_values(self):
        self.assertEqual(encode("<Range> 2..1"), {"type": "string", "value": "<Range> 2..1"})
        validate_value({"type": "string", "value": "Foo"})
        with self.assertRaises(ReviewError):
            validate_value({"type": "string", "value": 1})


class BuildFreshnessTests(unittest.TestCase):
    def test_refresh_timestamps_skips_git_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text("")
            (root / ".git").write_text("gitdir: elsewhere")
            os.utime(root / "src/lib.rs", (1, 1))
            os.utime(root / ".git", (1, 1))
            refresh_timestamps(root)
            self.assertGreater((root / "src/lib.rs").stat().st_mtime, 1)
            self.assertEqual((root / ".git").stat().st_mtime, 1)

    def test_native_build_and_reuse_stay_outside_scratch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            builds, scratch, sha = root / "build-store", root / "scratch", "a" * 40
            worktree = builds / "worktrees" / sha
            package = worktree / "source/qdk_package/qdk"
            package.mkdir(parents=True)
            (package / "__init__.py").write_text("")
            (worktree / "library").mkdir()
            config = {"build_dir": str(builds), "work_dir": str(scratch),
                      "repository": str(root), "venv": "/opt/qdk-venv"}

            def cargo(args, *, cwd, env, stdout, stderr):
                self.assertEqual(Path(cwd), worktree)
                self.assertEqual(Path(env["CARGO_TARGET_DIR"]), builds / "cargo-target")
                native = Path(env["CARGO_TARGET_DIR"]) / "release/libqdk.so"
                native.parent.mkdir(parents=True)
                native.write_bytes(b"native fixture")
                return subprocess.CompletedProcess(args, 0)

            def git(repo, *args):
                if args == ("rev-parse", "HEAD"):
                    return sha
                self.assertEqual(args, ("status", "--porcelain"))
                return ""

            with patch("harness.builds.git", side_effect=git), patch(
                "harness.builds.subprocess.run", side_effect=cargo
            ) as compile_build, patch("sys.stdout", io.StringIO()):
                build = build_revision(config, sha, "release", {})
                self.assertEqual(build_revision(config, sha, "release", {}), build)
            self.assertEqual(compile_build.call_count, 1)
            self.assertEqual(Path(build["overlay"]), builds / "builds" / f"{sha}-release")
            self.assertTrue((Path(build["overlay"]) / "qdk/_native.abi3.so").is_file())
            self.assertTrue((builds / "build.lock").is_file())
            self.assertFalse(scratch.exists())

    def test_discovery_uses_the_build_lock(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            builds, scratch = root / "build-store", root / "scratch"
            checkout = root / "checkout"
            (checkout / "samples").mkdir(parents=True)
            (checkout / "samples/Main.qs").write_text("operation Main() : Result { Zero }")
            config = {"build_dir": str(builds), "work_dir": str(scratch), "refs": {"candidate": "a" * 40}}

            def checkout_revision(config, sha):
                self.assertTrue((builds / "build.lock").is_file())
                self.assertFalse((scratch / "build.lock").exists())
                return checkout

            with patch("harness.discovery.checkout_revision", side_effect=checkout_revision):
                found, pending = discover_samples(config, scratch / "samples.json")
            self.assertEqual((found, pending), (1, 0))

    def test_extraction_separates_native_target_from_dump_and_log(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            builds, scratch, sha = root / "build-store", root / "scratch", "a" * 40
            worktree = builds / "extract" / sha
            worktree.mkdir(parents=True)
            config = {"build_dir": str(builds), "work_dir": str(scratch),
                      "repository": str(root), "venv": "/opt/qdk-venv"}
            record = {"kind": "semantic", "test": "test", "source": "operation Main() : Int { 1 }",
                      "expected": {"type": "int", "value": 1}}

            def cargo(args, *, cwd, env, stdout, stderr):
                self.assertEqual(Path(cwd), worktree)
                self.assertEqual(Path(env["CARGO_TARGET_DIR"]), builds / "extract-target")
                dump = Path(env["REVIEW_DUMP"])
                self.assertEqual(dump.parent, scratch / "extract")
                dump.write_text(json.dumps(record) + "\n")
                return subprocess.CompletedProcess(args, 0)

            with patch("harness.extraction.git", side_effect=lambda repo, *args: sha if args == (
                "rev-parse", "HEAD"
            ) else ""), patch("harness.extraction.instrument", return_value=["test.rs"]), patch(
                "harness.extraction.subprocess.run", side_effect=cargo
            ), patch("sys.stdout", io.StringIO()):
                count, _, status = extract_tests(config, sha, "qsc_fir_transforms", scratch / "cases.json")
            self.assertEqual((count, status), (1, 0))
            self.assertTrue((scratch / "extract" / f"{sha[:12]}-qsc_fir_transforms.log").is_file())


class RunOrderingTests(unittest.TestCase):
    def test_results_are_rewritten_in_manifest_order(self):
        import time

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cases = [CASE | {"id": f"c{i}"} for i in range(4)]
            write_json(root / "cases.json", cases)
            config = {"work_dir": str(root / "work"), "refs": {"reference": "r", "candidate": "c"},
                      "input": {}, "profiles": ["Base"], "limits": {"jobs": 4, "shots": 2, "seed": 1},
                      "keep_qir": False}

            def slow_first(config, build, case, directory):
                time.sleep(0.3 if case["id"] == "c0" else 0)
                return good()

            with patch("harness.runner.preflight", return_value={}), patch(
                "harness.runner.build_revision", return_value={"sha": "x", "mode": "release"}
            ), patch("harness.runner.run_case", side_effect=slow_first), patch("sys.stdout", io.StringIO()) as out:
                run = create_run(config, root / "cases.json", "release")
            order = [json.loads(line)["case"] for line in (run / "results.jsonl").read_text().splitlines()]
            self.assertEqual(order, ["c0", "c1", "c2", "c3"])
            self.assertIn("[4/4] c0", out.getvalue())


class RolesAndCacheTests(unittest.TestCase):
    def test_layers_previous_and_cache_defaults(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory) / "repo"
            repo.mkdir()
            path = repo / "review.json"
            write_json(path, {"reference": "old", "candidate": "new", "previously_reviewed": "prev",
                              "layers": ["l-one", "l-two"], "report": "out/ian20261023.md"})
            with patch("harness.common.git", return_value=str(repo)), patch(
                "harness.common.resolve_ref", side_effect=lambda repo, ref: ref + "-sha"
            ), patch.dict(os.environ, {}, clear=False):
                os.environ.pop("REVIEW_KIT_CACHE", None)
                config = load_config(path)
            self.assertEqual(config["refs"]["layer1"], "l-one-sha")
            self.assertEqual(config["layers"][1], {"role": "layer2", "ref": "l-two", "sha": "l-two-sha"})
            self.assertEqual(ordered_roles(config["refs"]),
                             ["previously_reviewed", "reference", "layer1", "layer2", "candidate"])
            self.assertEqual(select_roles(config, []), ["reference", "candidate"])
            self.assertEqual(select_roles(config, ["layer2"]), ["reference", "layer2", "candidate"])
            with self.assertRaisesRegex(ReviewError, "no layer9"):
                select_roles(config, ["layer9"])
            self.assertEqual(config["name"], "ian20261023")
            self.assertEqual(config["cache_dir"], "/work/cache/compiler-review-kit")
            self.assertEqual(config["cache_budget_mb"], 50)


def result(status="ok", seconds=0.5, digest="q", values=None, message=""):
    compiled = {"status": status, "seconds": seconds, "message": message}
    if status == "ok":
        compiled["qir_digest"] = digest
    record = {"compile": compiled}
    if status == "ok":
        record["runner"] = {"status": "ok", "values": values or [ONE, ONE]}
    return {"sha": "x", "simulator": {"status": "ok", "values": [ONE, ONE]}, "profiles": {"Adaptive": record}}


class NewSummaryTests(unittest.TestCase):
    def test_silent_lost_layers_outliers_identity_and_pipeline_gaps(self):
        candidate = "b" * 40
        rows = [
            {"case": "silent", "results": {"reference": result("panic", message="boom 7"),
                                           "candidate": result(values=[ZERO, ZERO])},
             "comparison": [{"profile": "Adaptive", "reference": "panic", "candidate": "wrong_result",
                             "verdict": "pre_existing_or_changed_failure"}],
             "previous": [{"profile": "Adaptive", "reference": "validated", "candidate": "wrong_result",
                           "verdict": "regression_candidate"}],
             "layers": {"layer1": {"Adaptive": "validated"}}},
            {"case": "hazard", "results": {"reference": result(seconds=0.5, digest="a"),
                                           "candidate": result(seconds=2.0, digest="a")},
             "comparison": [{"profile": "Adaptive", "reference": "validated", "candidate": "validated",
                             "verdict": "validated_unchanged"}],
             "layers": {"layer1": {"Adaptive": "wrong_result"}}},
            {"case": "gap", "results": {"reference": result("panic"), "candidate": result("panic", message="x")},
             "comparison": [{"profile": "Adaptive", "reference": "panic", "candidate": "panic",
                             "verdict": "pre_existing_or_changed_failure"}]},
        ]
        with tempfile.TemporaryDirectory() as directory:
            run = write_run(directory, rows, refs={"reference": "a" * 40, "candidate": candidate})
            cases = read_json(Path(run) / "cases.json")
            cases[2]["origin"] = f"{candidate[:12]} qsc_fir_transforms test mod::my_test (extracted)"
            write_json(Path(run) / "cases.json", cases)
            summary = summarize_run(run)
        self.assertEqual([item["case"] for item in summary["silenced"]], ["silent"])
        self.assertEqual(summary["silenced"][0]["signature"], "boom N")
        self.assertEqual([item["case"] for item in summary["lost_fixes"]], ["silent"])
        self.assertEqual(summary["layer_hazards"]["layer1"][0]["examples"], ["hazard"])
        self.assertEqual(summary["compile_outliers"][0]["case"], "hazard")
        self.assertEqual(summary["compile_outliers"][0]["ratio"], 4.0)
        self.assertEqual(summary["qir_identity"], {"identical": 1})  # only "hazard" compiled at both endpoints
        self.assertEqual(summary["pipeline_gaps"][0]["test"], "mod::my_test")

    def test_hierarchy_groups_count_each_program_once(self):
        statuses = {"Base": ("validated", "validated"), "Adaptive_RI": ("validated", "validated"),
                    "Adaptive_RIF": ("validated", "validated"), "Adaptive": ("rejected", "rejected")}
        with tempfile.TemporaryDirectory() as directory:
            run = write_run(directory, [comparison_row("one", statuses, "Qdk.Qsc.X")])
            groups = summarize_run(run)["hierarchy"]["candidate_groups"]
        self.assertEqual(len(groups), 1)
        self.assertEqual((groups[0]["lower"], groups[0]["count"]), ("Adaptive_RIF", 1))

    def test_compare_marks_silent_worsening(self):
        with tempfile.TemporaryDirectory() as directory:
            old = write_run(Path(directory) / "a", [comparison_row("c", {"Base": ("panic", "panic")})])
            row = comparison_row("c", {"Base": ("panic", "panic")})
            row["comparison"][0]["candidate"] = "wrong_result"
            new = write_run(Path(directory) / "b", [row])
            self.assertEqual(compare_runs(old, new)["changes"][0]["direction"], "worse (silent)")


class TableTests(unittest.TestCase):
    def test_values_and_cells(self):
        value = {"type": "tuple", "items": [ONE, {"type": "array", "items": [ZERO]}, {"type": "bool", "value": True},
                                            {"type": "double", "value": (0.5).hex()}]}
        self.assertEqual(format_value(value), "(1, [0], true, 0.5)")
        record = {"compile": {"status": "ok"}, "runner": {"status": "ok", "values": [ZERO, ONE]}}
        self.assertEqual(table_cell(record, ONE), "**0x1/1x1**")
        panic = {"compile": {"status": "panic", "message": "first line\nsecond"}}
        self.assertEqual(table_cell(panic, ONE), "PANIC: first line")
        rejected = {"compile": {"status": "diagnostic", "message": "Qdk.Qsc.CapabilitiesCk.UseOfDynamicQubit\n x"}}
        self.assertEqual(table_cell(rejected, ONE), "DIAGNOSTIC: Qdk.Qsc.CapabilitiesCk.UseOfDynamicQubit")

    def test_run_table_orders_roles_and_filters(self):
        rows = [{"case": "same", "results": {"candidate": result(), "reference": result()}, "comparison": []},
                {"case": "differs", "results": {"candidate": result(values=[ZERO]), "reference": result()},
                 "comparison": []}]
        with tempfile.TemporaryDirectory() as directory:
            run = write_run(directory, rows)
            cases = read_json(Path(run) / "cases.json")
            for case in cases:
                case["oracle"] = {"kind": "deterministic", "expected": ONE}
            write_json(Path(run) / "cases.json", cases)
            lines = run_table(run, profiles=["Adaptive"], only_diff=True)
        self.assertEqual(lines[0], "## differs  expected=1")
        self.assertTrue(lines[1].lstrip().startswith("reference"))
        self.assertIn("Adaptive=**0**", lines[2])


class TimingTests(unittest.TestCase):
    def test_medians_ratios_and_failures(self):
        def sweep(compile_seconds, status="ok"):
            row = {"status": status, "context": 0.1, "load": 0.2, "compile": compile_seconds}
            return {"results": {"c": {"Adaptive": row}, "d": {"Adaptive": {"status": "panic", "message": "m"}}}}
        sweeps = {"ref": [sweep(1.0), sweep(3.0), sweep(2.0)], "cand": [sweep(4.0), sweep(4.0), sweep(5.0)]}
        rows = aggregate(sweeps, ["ref", "cand"])
        by_case = {row["case"]: row for row in rows}
        self.assertEqual(by_case["c"]["revisions"]["ref"]["compile"], 2.0)
        self.assertEqual(by_case["d"]["revisions"]["cand"]["status"], "panic")
        self.assertEqual(ratios(rows, "ref", "cand"), [
            {"case": "c", "profile": "Adaptive", "baseline": 2.0, "value": 4.0, "ratio": 2.0}])


class VariantTests(unittest.TestCase):
    def test_worker_profile_defaults_and_explicit_selection(self):
        defaults = ["Base", "Adaptive_RIF", "Adaptive"]
        scenarios = [(None, None, defaults), (None, ["Adaptive"], ["Adaptive"]),
                     (["Adaptive_RI"], ["Adaptive"], ["Adaptive_RI"])]
        for requested, module_profiles, expected in scenarios:
            with self.subTest(requested=requested, module_profiles=module_profiles):
                module = SimpleNamespace()
                if module_profiles is not None:
                    module.PROFILES = module_profiles
                job = {"variants": "variants.py", "profiles": requested, "shots": 2, "seed": 1, "runner": "runner"}
                output = io.StringIO()
                with patch.dict(sys.modules, {"qdk": SimpleNamespace(__file__="qdk/__init__.py")}), patch(
                    "harness.variants_worker.load_variants", return_value=(module, [{"name": "v"}])
                ), patch("harness.variants_worker.run_variant", return_value={"name": "v"}) as run, patch(
                    "sys.stdin", io.StringIO(json.dumps(job))
                ), patch("sys.stdout", output):
                    variants_worker.main()
                self.assertEqual(json.loads(output.getvalue())["profiles"], expected)
                self.assertEqual(run.call_args.args[3], expected)

    def test_explicit_config_profiles_reach_variant_worker(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "variants.py"
            path.write_text("VARIANTS = [{'name': 'v'}]\n")
            config = {"work_dir": str(root), "venv": "/opt/qdk-venv",
                      "profiles": ["Adaptive_RI"], "input": {"profiles": ["Adaptive_RI"]},
                      "limits": {"shots": 2, "seed": 1, "compile_seconds": 10}}
            build = {"overlay": str(root), "worktree": str(root)}
            for requested, expected in ((None, ["Adaptive_RI"]), (["Base"], ["Base"])):
                with self.subTest(requested=requested), patch("harness.variants.preflight", return_value={}), patch(
                    "harness.variants.build_revision", return_value=build
                ), patch("harness.variants.isolated", return_value={
                    "status": "ok", "profiles": expected, "variants": [],
                }) as execute:
                    run_variants(config, path, [("candidate", "a" * 40)], requested, "release")
                self.assertEqual(execute.call_args.kwargs["payload"]["profiles"], expected)

    def test_matrix_marks_wrong_and_differences(self):
        def column(label, value):
            return {"label": label, "sha": label * 8, "panics": [], "variants": [
                {"name": "v", "expected": "1", "simulator": {"status": "ok", "values": ["1"]},
                 "profiles": {"Adaptive": {"status": "ok", "values": [value]}}}]}
        lines = variant_lines([column("a", "1"), column("b", "0")])
        self.assertIn("Adaptive=1", lines[1])
        self.assertIn("Adaptive=**0**", lines[2])
        self.assertIn("differs between revisions: 1 (v)", lines[-1])

    def test_variant_files_need_unique_names(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "v.py"
            path.write_text("VARIANTS = [{'name': 'a'}, {'name': 'a'}]\n")
            with self.assertRaisesRegex(ReviewError, "unique"):
                load_variants(path)
            self.assertEqual(len(load_variants(KIT / "examples" / "variants.py")[1]), 4)


class IssueDraftTests(unittest.TestCase):
    def test_repro_profiles_follow_defaults_and_explicit_selection(self):
        _, issues = load_definitions(KIT / "examples" / "issues.py")
        for profiles, expected in ((None, ["Base", "Adaptive_RIF", "Adaptive"]),
                                   (["Adaptive_RI"], ["Adaptive_RI"])):
            with self.subTest(profiles=profiles):
                tree = ast.parse(render(issues[0], profiles))
                value = next(node.value for node in tree.body if isinstance(node, ast.Assign)
                             and any(isinstance(target, ast.Name) and target.id == "PROFILES"
                                     for target in node.targets))
                self.assertEqual(ast.literal_eval(value), expected)

    def test_render_and_markdown_from_executions(self):
        _, issues = load_definitions(KIT / "examples" / "issues.py")
        issue = issues[0]
        script = render(issue)
        self.assertIn("Exits 0 when the bug no longer reproduces.", script)
        self.assertIn("Reproduces when Adaptive return something other than 1.", script)
        self.assertNotRegex(script, r"__[A-Z_]+__")
        results = {
            "main": {"exit": 0, "sha": "a" * 40, "rows": {"simulator": ["1"], "Adaptive": ["0"]},
                     "panics": []},
            "cand": {"exit": 0, "sha": "b" * 40, "rows": {"simulator": ["1"], "Adaptive": ["panics: oops: detail"]},
                     "panics": [{"path": "source/compiler/qsc_x/src/a.rs", "line": 3, "function": "f"}]},
        }
        issue = dict(issue, kind="Panic at {panic:cand}.")
        text = issue_markdown(issue, ["main", "cand"], {"main": "`main`", "cand": "PR"}, results, script)
        self.assertIn("| Adaptive | **`0`** | **panics: `oops`** |", text)
        self.assertIn("[`qsc_x/src/a.rs:3`](https://github.com/microsoft/qdk/blob/" + "b" * 40, text)
        self.assertIn("cand: panics: oops: detail", text)

    def test_definitions_are_validated(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "issues.py"
            path.write_text("ISSUES = [{'slug': 'x'}]\n")
            with self.assertRaisesRegex(ReviewError, "lacks"):
                load_definitions(path)

    def test_rendered_repro_exits_zero_only_when_not_reproduced(self):
        _, issues = load_definitions(KIT / "examples" / "issues.py")
        tree = ast.parse(render(issues[0]))
        main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "main")
        scenarios = [("1", ["Adaptive"], 0, "NOT REPRODUCED"), ("0", ["Adaptive"], 1, "REPRODUCED:"),
                     ("1", ["Adaptive_RI"], 2, "INCONCLUSIVE:")]
        for value, affected, expected_exit, verdict in scenarios:
            with self.subTest(value=value), patch("sys.stdout", io.StringIO()) as output:
                namespace = {
                    "outputs": lambda: {"Adaptive": {value}},
                    "EXPECTED": "1", "AFFECTED": affected, "json": json,
                }
                exec(compile(ast.Module(body=[main], type_ignores=[]), "<repro>", "exec"), namespace)
                self.assertEqual(namespace["main"](), expected_exit)
                self.assertTrue(output.getvalue().splitlines()[-1].startswith(verdict))

    def test_draft_status_uses_verdict_instead_of_exit_code(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / "repro.log"
            log.write_text('JSON: {"Adaptive": ["0"]}\n')
            rows = [
                {"label": label, "sha": "a" * 40, "exit": exit_code, "panics": [],
                 "log": str(log), "reproduction": status}
                for label, exit_code, status in (
                    ("legacy", 0, "reproduces"), ("current", 1, "reproduces"),
                    ("error", 1, "exit 1"), ("absent", 0, "not reproduced"),
                )
            ]
            config = {"work_dir": str(root / "work"),
                      "profiles": ["Base", "Adaptive_RIF", "Adaptive"],
                      "refs": {row["label"]: row["sha"] for row in rows}}
            with patch("harness.issue_drafts.exec_script", return_value=rows):
                _, status = draft_issues(config, KIT / "examples" / "issues.py", root / "issues.md",
                                         revisions=list(config["refs"]))
            self.assertEqual(status["loop_result_slot"],
                             {row["label"]: row["reproduction"] for row in rows})


class EvidenceTests(unittest.TestCase):
    def test_bundle_save_prune_and_recheck_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "m" / "adapters").mkdir(parents=True)
            (root / "m" / "adapters" / "a.py").write_text("def prepare(ctx):\n    return 'Main()', ()\n")
            (root / "proj" / "src").mkdir(parents=True)
            (root / "proj" / "qsharp.json").write_text("{}")
            write_json(root / "m" / "cases.json", [
                CASE | {"id": "one", "project_root": "../proj"}, CASE | {"id": "two", "project_root": "../proj"},
                {"id": "py", "adapter": "adapters/a.py", "oracle": {"kind": "compile_only"}},
            ])
            (root / "repro.py").write_text('"""T\\n\\nExits 0 while the bug reproduces: X."""\n')
            cache = root / "cache"
            (cache / "old").mkdir(parents=True)
            (cache / "old" / "big").write_bytes(b"x" * 3 * 1024 * 1024)
            os.utime(cache / "old", (1, 1))
            config = {"cache_dir": str(cache), "name": "now", "cache_budget_mb": 2, "refs": {"reference": "r"},
                      "report": str(root / "report.md"), "input": {}, "work_dir": str(root / "work")}
            saved, size, total, removed = evidence.save_evidence(config, findings=[root / "m" / "cases.json"],
                                                                 files=[root / "repro.py"])
            bundled = read_json(saved / "findings" / "m-cases" / "cases.json")
            self.assertEqual({c.get("project_root") for c in bundled[:2]}, {"projects/001-proj"})
            self.assertEqual(bundled[2]["adapter"], "adapters/py.py")
            self.assertEqual(removed, ["old"])
            manifests, scripts, source = evidence.recheck_inputs(config)
            self.assertEqual(len(manifests), 1)
            self.assertEqual([p.name for p in scripts], ["repro.py"])
            merged = root / "merged.json"
            self.assertEqual(evidence.merged_manifest(manifests * 2, merged), 3)
            self.assertTrue(Path(read_json(merged)[2]["adapter"]).is_absolute())

    def test_report_fallback_extracts_marked_scripts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = root / "report.md"
            report.write_text('# R\n\n```python\n"""Bug one\n\nExits 0 while the bug reproduces: A."""\n```\n\n'
                              '```python\n"""Bug two\n\nExits 0 when the bug no longer reproduces."""\n```\n\n'
                              '```python\nprint("not a repro")\n```\n')
            config = {"cache_dir": str(root / "cache"), "name": "gone", "work_dir": str(root / "work")}
            manifests, scripts, source = evidence.recheck_inputs(config, report=report)
            self.assertEqual((manifests, [p.name for p in scripts]), ([], ["01_bug_one.py", "02_bug_two.py"]))
            self.assertIn("embedded", source)
            with self.assertRaisesRegex(ReviewError, "No evidence"):
                evidence.recheck_inputs(config)


class DiffScanTests(unittest.TestCase):
    DIFF = """diff --git a/source/compiler/x/src/lib.rs b/source/compiler/x/src/lib.rs
--- a/source/compiler/x/src/lib.rs
+++ b/source/compiler/x/src/lib.rs
@@ -10,2 +10,6 @@ impl X {
-    #[diagnostic(code("Qdk.Qsc.X.Moved"))]
-    #[diagnostic(code("Qdk.Qsc.X.Gone"))]
+    #[error("cannot do the thing")]
+    #[diagnostic(code("Qdk.Qsc.X.New"))]
+    #[diagnostic(code("Qdk.Qsc.X.Moved"))]
+        let v = map.get(k).expect("value should exist");
--- a/source/compiler/x/src/tests.rs
+++ b/source/compiler/x/src/tests.rs
@@ -1,0 +1,1 @@
+    panic!("in a test");
"""

    def test_added_removed_and_panic_sites(self):
        found = scan_diff_text(self.DIFF)
        self.assertEqual([(c["code"], c["line"]) for c in found["codes_added"]], [("Qdk.Qsc.X.New", 11)])
        self.assertEqual([c["code"] for c in found["codes_removed"]], ["Qdk.Qsc.X.Gone"])
        self.assertEqual(found["errors_added"][0]["message"], "cannot do the thing")
        self.assertEqual([(p["kind"], p["message"], p["line"]) for p in found["panics_added"]],
                         [(".expect", "value should exist", 13)])


class NewGeneratorTests(unittest.TestCase):
    def test_chain_families_and_typed_binding(self):
        with tempfile.TemporaryDirectory() as directory:
            cases = callable_chains.generate(directory, 120, 40, 60, seed=11, profiles=["Adaptive"])
            text = json.dumps(cases) + "".join(p.read_text() for p in (Path(directory) / "adapters").glob("*.py"))
        for marker in ("MkAll", "ArrBox", "MkRep", "PyCapBox", "PyMkRep", "c.PyMkBox(f)"):
            self.assertIn(marker, text)
        self.assertIn("let f : (Qubit[] => Unit is Adj + Ctl) = ", text)
        layout = [c["source"] for c in cases if c["id"].startswith("gl-")]
        self.assertFalse(any("operation Rep" in source and "(Int, Qubit[]) =>" in source.split("operation Rep")[1][:60]
                             for source in layout))
        self.assertEqual(callable_chains.power(callable_chains.M.tag(1, 1), 2).perm,
                         callable_chains.M.ident().perm)

    def test_dynamic_recursion_cases(self):
        with tempfile.TemporaryDirectory() as first, tempfile.TemporaryDirectory() as second:
            cases = dynamic_recursion.generate(first, 30, 15, 8, seed=4)
            dynamic_recursion.generate(second, 30, 15, 8, seed=4)
            self.assertEqual((Path(first) / "cases.json").read_text(), (Path(second) / "cases.json").read_text())
            explicit = dynamic_recursion.generate(Path(first) / "explicit", 1, 1, 1, seed=4, profiles=["Adaptive_RI"])
        validate(cases, read_json(KIT / "cases.schema.json"))
        for case in cases:
            validate_value(case["oracle"]["expected"])
        classical = [c for c in cases if c["id"].startswith("dc-")]
        self.assertTrue(classical and all("Base" not in c["profiles"] for c in classical))
        self.assertTrue(all("Adaptive_RI" not in c["profiles"] for c in cases))
        self.assertTrue(all(c["profiles"] == ["Adaptive_RI"] for c in explicit))
        self.assertTrue(any("Hop(" in c["source"] for c in cases))


if __name__ == "__main__":
    unittest.main()
