# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Offline tests of stack inference and the untrusted artifact boundary."""

import argparse
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import MagicMock, patch
from urllib.parse import parse_qs, urlsplit


SPEC = importlib.util.spec_from_file_location(
    "workflow", Path(__file__).resolve().parents[1] / "workflow.py"
)
workflow = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(workflow)
COMMON_SPEC = importlib.util.spec_from_file_location(
    "kit_common", Path(__file__).resolve().parents[2] / "compiler-review-kit/harness/common.py"
)
kit_common = importlib.util.module_from_spec(COMMON_SPEC)
COMMON_SPEC.loader.exec_module(kit_common)


def git(repo, *args):
    return subprocess.check_output(
        ["git", "-C", str(repo), *args], text=True, stderr=subprocess.PIPE
    ).strip()


def pull(number, head, base, branch="main", head_branch=None):
    return {
        "number": number,
        "html_url": f"https://github.com/microsoft/qdk/pull/{number}",
        "head": {
            "sha": head,
            "ref": head_branch or f"feature-{number}",
            "repo": {"full_name": "microsoft/qdk"},
        },
        "base": {
            "sha": base,
            "ref": branch,
            "repo": {"full_name": "microsoft/qdk"},
        },
    }


class StackTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.repo = Path(self.temporary.name)
        git(self.repo, "init", "-q")
        git(self.repo, "config", "user.name", "Workflow test")
        git(self.repo, "config", "user.email", "test@example.invalid")
        self.commits = []
        for index in range(4):
            (self.repo / "source.txt").write_text(str(index))
            git(self.repo, "add", "source.txt")
            git(self.repo, "commit", "-qm", f"change {index}")
            self.commits.append(git(self.repo, "rev-parse", "HEAD"))

    def discover(self, pulls, requested, associations=None):
        associations = associations or {}

        def listing(path):
            if path.startswith("commits/"):
                return associations.get(path.split("/")[1], [])
            branch = parse_qs(urlsplit(path).query)["head"][0].split(":", 1)[1]
            return [pr for pr in pulls.values() if pr["head"]["ref"] == branch]

        with patch.object(workflow, "fetch", side_effect=lambda repo, sha: sha):
            return workflow.discover_stack(
                self.repo, requested,
                read=lambda path: pulls[int(path.split("/")[1])], listing=listing,
            )

    def test_base_branch_stack_includes_every_pr_tip_and_merge_base(self):
        a, b, c, d = self.commits
        pulls = {
            1: pull(1, b, a),
            2: pull(2, c, b, branch="feature-1"),
            3: pull(3, d, c, branch="feature-2"),
        }
        scope = self.discover(pulls, 3)
        self.assertEqual((scope["reference"], scope["candidate"]), (a, d))
        self.assertEqual(scope["layers"], [b, c])
        self.assertEqual([pr["number"] for pr in scope["pull_requests"]], [1, 2, 3])

    def test_stack_targeting_main_uses_associated_pr_tips_not_each_commit(self):
        a, b, c, d = self.commits
        lower = pull(1, c, a)
        top = pull(2, d, a)
        scope = self.discover(
            {1: lower, 2: top}, 2,
            associations={b: [lower, top], c: [lower, top], d: [top]},
        )
        self.assertEqual(scope["layers"], [c])
        self.assertEqual(scope["reference"], a)

    def test_single_pr_has_no_intermediate_layers(self):
        a, _, _, d = self.commits
        scope = self.discover({1: pull(1, d, a)}, 1)
        self.assertEqual(scope["layers"], [])

    def test_moved_base_snapshot_is_rejected(self):
        a, b, c, d = self.commits
        with self.assertRaisesRegex(workflow.WorkflowError, "base moved"):
            self.discover({1: pull(1, c, a), 2: pull(2, d, b, branch="feature-1")}, 2)

    def test_ambiguous_tip_is_rejected(self):
        a, b, _, d = self.commits
        one, two = pull(1, b, a), pull(2, b, a)
        with self.assertRaisesRegex(workflow.WorkflowError, "Multiple PRs share tip"):
            self.discover(
                {1: one, 2: two, 3: pull(3, d, a)}, 3,
                associations={b: [one, two]},
            )

    def test_non_linear_lower_tip_is_rejected(self):
        a, b, _, d = self.commits
        git(self.repo, "checkout", "--detach", a)
        (self.repo / "other.txt").write_text("divergent")
        git(self.repo, "add", "other.txt")
        git(self.repo, "commit", "-qm", "divergent")
        divergent = git(self.repo, "rev-parse", "HEAD")
        with self.assertRaisesRegex(workflow.WorkflowError, "not in the pinned stack"):
            self.discover(
                {
                    1: pull(1, divergent, a),
                    2: pull(2, d, divergent, branch="feature-1"),
                },
                2,
            )

    def test_release_pair_preserves_endpoints_instead_of_merge_base(self):
        a, _, _, d = self.commits
        git(self.repo, "checkout", "--detach", a)
        (self.repo / "release.txt").write_text("different release branch")
        git(self.repo, "add", "release.txt")
        git(self.repo, "commit", "-qm", "release branch")
        reference = git(self.repo, "rev-parse", "HEAD")
        args = argparse.Namespace(
            pr="", reference="v1", candidate="v2", repository=str(self.repo),
            output=str(self.repo / "scope.json"),
        )
        with (
            patch.object(workflow, "github", side_effect=[{"sha": reference}, {"sha": d}]),
            patch.object(workflow, "fetch", side_effect=lambda repo, sha: sha),
            patch.dict(os.environ, {"GITHUB_SHA": a}, clear=True),
        ):
            workflow.resolve(args)
        scope = json.loads(Path(args.output).read_text())
        self.assertEqual((scope["reference"], scope["candidate"]), (reference, d))


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def archive(self, name, kind=tarfile.REGTYPE):
        archive = self.root / "dependencies.tar.gz"
        with tarfile.open(archive, "w:gz") as bundle:
            entry = tarfile.TarInfo(name)
            entry.type = kind
            if kind == tarfile.SYMTYPE:
                entry.linkname = "/etc/passwd"
            entry.size = 0
            bundle.addfile(entry, io.BytesIO())
        return archive

    def test_dependency_bundle_rejects_traversal_links_and_special_files(self):
        for name, kind in [
            ("../escape", tarfile.REGTYPE),
            ("/wheels/escape", tarfile.REGTYPE),
            ("wheels/link", tarfile.SYMTYPE),
            ("cargo/fifo", tarfile.FIFOTYPE),
            ("controller.py", tarfile.REGTYPE),
        ]:
            with self.subTest(name=name), self.assertRaises(workflow.WorkflowError):
                workflow.extract_dependencies(self.archive(name, kind), self.root / "out")

    def test_dependency_bundle_extracts_regular_dependency_files(self):
        workflow.extract_dependencies(
            self.archive("cargo/registry/cache/crate"), self.root / "out"
        )
        self.assertTrue((self.root / "out/cargo/registry/cache/crate").is_file())

    def collect(self):
        args = argparse.Namespace(
            report=str(self.root / "report"),
            logs=str(self.root / "logs"),
            output=str(self.root / "out"),
        )
        with patch.object(workflow, "stop_worker"):
            workflow.collect(args)
        return Path(args.output)

    def test_artifacts_redact_token_and_preserve_explicit_incomplete_status(self):
        token = "this-is-the-real-inference-token"
        (self.root / "report").mkdir()
        (self.root / "logs").mkdir()
        (self.root / "logs/session.jsonl").write_text(token)
        with patch.dict(os.environ, {"COPILOT_GITHUB_TOKEN": token}, clear=True):
            output = self.collect()
        self.assertEqual((output / "diagnostics/session.jsonl").read_text(), "[REDACTED]")
        self.assertIn("Incomplete", (output / "report/report.md").read_text())

    def test_artifact_symlink_is_rejected_instead_of_followed(self):
        (self.root / "report").mkdir()
        (self.root / "logs").mkdir()
        (self.root / "report/secret").symlink_to("/etc/passwd")
        with self.assertRaisesRegex(workflow.WorkflowError, "Unsafe artifact"):
            self.collect()

    def test_input_rejects_non_qdk_urls_and_shell_syntax(self):
        for value in ["https://github.com/other/repo/pull/1", "1; echo bad", "0"]:
            with self.subTest(value=value), self.assertRaises(workflow.WorkflowError):
                workflow.pr_number(value)


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def test_single_pr_and_stack_configs_satisfy_toolkit_schema(self):
        schema = json.loads(
            (Path(__file__).resolve().parents[2] / "compiler-review-kit/review.schema.json")
            .read_text()
        )
        for layers in ([], ["b" * 40]):
            scope = {"reference": "a" * 40, "candidate": "c" * 40, "layers": layers}
            with (
                self.subTest(layers=layers),
                patch.object(workflow, "ROOT", self.root),
                patch.object(workflow, "extract_dependencies"),
                patch.object(workflow, "materialize"),
                patch.object(workflow.os, "chmod"),
                patch.object(workflow, "write_json") as write,
                patch.dict(os.environ, {"GITHUB_RUN_ID": "1"}, clear=True),
            ):
                workflow.configure(argparse.Namespace(scope=json.dumps(scope), archive="unused"))
            config = next(
                call.args[1] for call in write.call_args_list
                if Path(call.args[0]).name == "review.json"
            )
            kit_common.validate(config, schema)

    def test_token_stays_out_of_command_line_and_runner_environment_is_not_inherited(self):
        token = "this-is-the-real-inference-token"
        process = MagicMock()
        process.pid = 123456
        process.wait.return_value = 0
        args = argparse.Namespace(logs=str(self.root), model="approved-model")
        with (
            patch.dict(os.environ, {
                "COPILOT_GITHUB_TOKEN": token, "AZURE_SECRET": "not-for-the-worker",
            }, clear=True),
            patch.object(workflow, "command", return_value="GitHub Copilot CLI 1.0.97."),
            patch.object(workflow.subprocess, "Popen", return_value=process) as launch,
            patch.object(workflow.os, "killpg", side_effect=ProcessLookupError),
            patch.object(workflow, "stop_worker"),
        ):
            workflow.run_review(args)
        self.assertNotIn(token, " ".join(launch.call_args.args[0]))
        self.assertNotIn("AZURE_SECRET", launch.call_args.kwargs["env"])
        self.assertEqual(launch.call_args.kwargs["env"]["COPILOT_GITHUB_TOKEN"], token)
        self.assertEqual(
            json.loads((self.root / "execution.json").read_text())["status"], "agent_exited"
        )

    def test_startup_failure_still_records_failure_metadata(self):
        args = argparse.Namespace(logs=str(self.root), model="")
        with (
            patch.dict(os.environ, {"COPILOT_GITHUB_TOKEN": "unused"}, clear=True),
            patch.object(workflow, "command", return_value="GitHub Copilot CLI 1.0.97."),
            self.assertRaisesRegex(workflow.WorkflowError, "COMPILER_REVIEW_MODEL"),
        ):
            workflow.run_review(args)
        self.assertEqual(
            json.loads((self.root / "execution.json").read_text())["status"], "startup_failed"
        )

    def test_timeout_records_incomplete_status_and_stops_worker(self):
        args = argparse.Namespace(logs=str(self.root), model="approved-model")
        process = MagicMock()
        process.pid = 123456
        process.wait.side_effect = [subprocess.TimeoutExpired("copilot", 1), 0]
        with (
            patch.dict(os.environ, {"COPILOT_GITHUB_TOKEN": "unused"}, clear=True),
            patch.object(workflow, "command", return_value="GitHub Copilot CLI 1.0.97."),
            patch.object(workflow.subprocess, "Popen", return_value=process),
            patch.object(workflow.os, "killpg", side_effect=ProcessLookupError),
            patch.object(workflow, "stop_worker") as stop,
            self.assertRaisesRegex(workflow.WorkflowError, "timed_out"),
        ):
            workflow.run_review(args)
        stop.assert_called_once()
        execution = json.loads((self.root / "execution.json").read_text())
        self.assertEqual((execution["status"], execution["exit_code"]), ("timed_out", 124))


if __name__ == "__main__":
    unittest.main()
