import subprocess
import unittest
from typing import cast
from unittest.mock import mock_open, patch

import performance_logging as perf

# Repo-level build-system tests for build orchestration and perf logging behavior.


class TestPerformanceLoggingHelpers(unittest.TestCase):
    def test_sanitize_path_repo_root_and_home(self) -> None:
        self.assertEqual(perf._sanitize_path("/repo", "/repo"), "<repo>")
        self.assertEqual(perf._sanitize_path("/repo/src/file.py", "/repo"), "<repo>/src/file.py")
        self.assertEqual(perf._sanitize_path("/tmp/file.py", "/repo"), "/tmp/file.py")

        with patch("pathlib.Path.home", return_value=__import__("pathlib").Path("/home/test")):
            self.assertEqual(perf._sanitize_path("/home/test", None), "~")
            self.assertEqual(perf._sanitize_path("/home/test/work/file.py", None), "~/work/file.py")

    def test_derive_process_role_prefers_subcommand(self) -> None:
        role = perf._derive_process_role(
            "cargo",
            ["/usr/bin/cargo", "rustc", "--release"],
        )
        self.assertEqual(role, "cargo rustc")

    def test_derive_process_role_falls_back_to_executable(self) -> None:
        role = perf._derive_process_role(
            "python",
            ["/opt/venv/bin/python", "-m", "build"],
        )
        self.assertEqual(role, "python build")

    def test_derive_process_role_skips_sensitive_flags(self) -> None:
        role = perf._derive_process_role(
            "tool",
            ["/usr/bin/tool", "--token", "supersecret", "run"],
        )
        self.assertEqual(role, "tool run")

    def test_derive_process_role_skips_sensitive_value_tokens(self) -> None:
        role = perf._derive_process_role(
            "tool",
            ["/usr/bin/tool", "sk-live-123", "run"],
        )
        self.assertEqual(role, "tool run")

    def test_is_cmdline_allowed_is_case_insensitive(self) -> None:
        self.assertTrue(perf._is_cmdline_allowed("Cargo", ["cargo", "maturin"]))
        self.assertFalse(perf._is_cmdline_allowed("python", ["cargo", "maturin"]))

    def test_redact_cmdline_args_redacts_sensitive_values(self) -> None:
        redacted = perf._redact_cmdline_args(
            ["tool", "--token", "abc", "--secret=xyz", "run", "ghp_foobar"]
        )
        self.assertEqual(
            redacted,
            ["tool", "--token", "<redacted>", "--secret=<redacted>", "run", "<redacted>"],
        )


class TestPerfLoggerFormatting(unittest.TestCase):
    def test_section_indentation_uses_config_indent_unit(self) -> None:
        logger = perf.PerfLogger.from_config(
            perf.PerfLogConfig(
                log_render_config=perf.LogRenderConfig(
                    prefix="pref",
                    indent_unit=3,
                    wrap_width=140,
                    cmdline_limit=140,
                ),
            )
        )

        with patch("performance_logging.print") as emit_mock:
            section_logger = logger.section("header")
            section_logger.line("child")

        self.assertEqual(
            [call.args[0] for call in emit_mock.call_args_list],
            [
                "pref header:",
                "pref    child",
            ],
        )

    def test_wrapped_field_continuation_is_indented(self) -> None:
        logger = perf.PerfLogger.from_config(
            perf.PerfLogConfig(
                log_render_config=perf.LogRenderConfig(
                    prefix="pref",
                    indent_unit=2,
                    wrap_width=40,
                    cmdline_limit=140,
                ),
            )
        )

        with patch("performance_logging.print") as emit_mock:
            logger.wrapped_field(
                "command",
                "python -m build --wheel --config-setting=build-args=--compatibility",
            )

        self.assertGreaterEqual(len(emit_mock.call_args_list), 2)
        first_line = emit_mock.call_args_list[0].args[0]
        second_line = emit_mock.call_args_list[1].args[0]
        self.assertTrue(first_line.startswith("pref command="))
        self.assertTrue(second_line.startswith("pref   "))


class TestPeakUpdateLogic(unittest.TestCase):
    def test_update_peak_process_tree_respects_maxima(self) -> None:
        peak = perf.PeakProcessTree()
        sample = perf.ProcessTreeSample(
            root_rss=100,
            tree_rss=300,
            largest_process=perf.ProcessRssEntry(
                pid=2,
                name="child",
                rss=180,
                cmdline="python child.py",
            ),
            rss_entries=[
                perf.ProcessRssEntry(pid=1, name="root", rss=100, cmdline="root"),
                perf.ProcessRssEntry(pid=2, name="child", rss=180, cmdline="child"),
                perf.ProcessRssEntry(pid=3, name="worker", rss=20, cmdline="worker"),
            ],
        )

        updated = perf._update_peak_process_tree(
            peak_data=peak,
            sample_data=sample,
            offender_selection_policy=perf.TopNOffenderSelectionPolicy(limit=2),
        )

        self.assertEqual(updated.peak_root_rss, 100)
        self.assertEqual(updated.peak_tree_rss, 300)
        self.assertIsNotNone(updated.peak_largest_process)
        assert updated.peak_largest_process is not None
        self.assertEqual(updated.peak_largest_process.pid, 2)
        self.assertEqual(len(updated.peak_tree_top_offenders), 2)
        self.assertEqual(updated.peak_tree_top_offenders[0].rss, 180)


class TestOffenderPolicies(unittest.TestCase):
    def test_hybrid_policy_filters_small_entries_by_tree_share(self) -> None:
        sample = perf.ProcessTreeSample(
            root_rss=10,
            tree_rss=100,
            largest_process=None,
            rss_entries=[
                perf.ProcessRssEntry(pid=1, name="a", rss=60, cmdline=""),
                perf.ProcessRssEntry(pid=2, name="b", rss=30, cmdline=""),
                perf.ProcessRssEntry(pid=3, name="c", rss=10, cmdline=""),
            ],
        )
        policy = perf.HybridOffenderSelectionPolicy(
            min_entries=0,
            min_tree_share=0.25,
        )

        selected = policy.select_offenders(sample)
        self.assertEqual([entry.pid for entry in selected], [1, 2])

    def test_offender_policy_factory_rejects_unsupported_config_type(self) -> None:
        config = perf.PerfLogConfig(offender_policy=cast(perf.OffenderPolicyConfig, "unknown"))
        with self.assertRaises(TypeError):
            perf._offender_selection_policy_from_config(config)

    def test_offender_policy_factory_builds_top_n_policy(self) -> None:
        config = perf.PerfLogConfig(
            offender_policy=perf.TopNOffenderPolicyConfig(top_n=2),
        )
        policy = perf._offender_selection_policy_from_config(config)
        self.assertIsInstance(policy, perf.TopNOffenderSelectionPolicy)
        top_n_policy = cast(perf.TopNOffenderSelectionPolicy, policy)
        self.assertEqual(top_n_policy.limit, 2)

    def test_hybrid_policy_falls_back_to_min_entries(self) -> None:
        sample = perf.ProcessTreeSample(
            root_rss=10,
            tree_rss=100,
            largest_process=None,
            rss_entries=[
                perf.ProcessRssEntry(pid=1, name="a", rss=60, cmdline=""),
                perf.ProcessRssEntry(pid=2, name="b", rss=25, cmdline=""),
                perf.ProcessRssEntry(pid=3, name="c", rss=15, cmdline=""),
            ],
        )
        policy = perf.HybridOffenderSelectionPolicy(
            min_entries=3,
            min_tree_share=0.50,
        )

        selected = policy.select_offenders(sample)
        self.assertEqual([entry.pid for entry in selected], [1, 2, 3])


class TestProviderFactory(unittest.TestCase):
    def test_memory_snapshot_provider_factory(self) -> None:
        self.assertIsInstance(
            perf._system_memory_snapshot_provider_for_system("Linux"),
            perf.LinuxMemorySnapshotProvider,
        )
        self.assertIsInstance(
            perf._system_memory_snapshot_provider_for_system("Darwin"),
            perf.DarwinMemorySnapshotProvider,
        )
        self.assertIsInstance(
            perf._system_memory_snapshot_provider_for_system("Windows"),
            perf.WindowsMemorySnapshotProvider,
        )
        self.assertIsInstance(
            perf._system_memory_snapshot_provider_for_system("UnknownOS"),
            perf.EmptyMemorySnapshotProvider,
        )

    def test_system_memory_snapshot_for_system_wraps_provider_output(self) -> None:
        with patch(
            "performance_logging._system_memory_snapshot_provider_for_system"
        ) as provider_factory:
            provider = provider_factory.return_value
            provider.snapshot_lines.return_value = ["line-a", "line-b"]

            snapshot = perf._system_memory_snapshot_for_system("Linux")

        provider_factory.assert_called_once_with("Linux")
        self.assertEqual(snapshot.lines, ["line-a", "line-b"])


class TestLinuxMemorySnapshotProvider(unittest.TestCase):
    def test_snapshot_lines_uses_deterministic_key_order(self) -> None:
        meminfo = "\n".join(
            [
                "SwapFree: 1 kB",
                "MemAvailable: 2 kB",
                "MemTotal: 3 kB",
                "SwapTotal: 4 kB",
                "MemFree: 5 kB",
                "Cached: 6 kB",
            ]
        )

        with patch("builtins.open", mock_open(read_data=meminfo)):
            lines = perf.LinuxMemorySnapshotProvider().snapshot_lines()

        self.assertEqual(
            lines,
            [
                "MemTotal: 3 kB",
                "MemFree: 5 kB",
                "MemAvailable: 2 kB",
                "SwapTotal: 4 kB",
                "SwapFree: 1 kB",
            ],
        )


class TestRunPathPlumbing(unittest.TestCase):
    def test_run_with_logging_passes_subprocess_parameters(self) -> None:
        defaults = perf.PerfLogConfig(offender_policy=perf.HybridOffenderPolicyConfig())
        hybrid_defaults = cast(perf.HybridOffenderPolicyConfig, defaults.offender_policy)
        with patch.object(perf.PerfReportWriter, "log_build_context"), patch.object(
            perf.PerfReportWriter,
            "log_system_memory_snapshot_phase",
        ), patch.object(
            perf,
            "_run_command_with_psutil_logging",
            return_value=(0, perf.PeakProcessTree()),
        ) as run_mock, patch.object(perf.PerfReportWriter, "log_peak_report"):
            perf.run_with_logging(
                args=["python", "-m", "build"],
                execution_policy=perf.ExecutionPolicy(
                    run_kwargs={"cwd": "/tmp", "env": {"A": "B"}, "text": True},
                    check=True,
                ),
                config=perf.PerfLogConfig(repo_root=None),
            )

        run_mock.assert_called_once_with(
            ["python", "-m", "build"],
            {"cwd": "/tmp", "env": {"A": "B"}, "text": True},
            defaults.sampling_interval_sec,
            perf.HybridOffenderSelectionPolicy(
                min_entries=hybrid_defaults.min_entries,
                min_tree_share=hybrid_defaults.min_tree_share,
            ),
            (),
        )

    def test_run_with_logging_passes_cmdline_allowlist_opt_in(self) -> None:
        defaults = perf.PerfLogConfig(offender_policy=perf.HybridOffenderPolicyConfig())
        hybrid_defaults = cast(perf.HybridOffenderPolicyConfig, defaults.offender_policy)
        with patch.object(perf.PerfReportWriter, "log_build_context"), patch.object(
            perf.PerfReportWriter,
            "log_system_memory_snapshot_phase",
        ), patch.object(
            perf,
            "_run_command_with_psutil_logging",
            return_value=(0, perf.PeakProcessTree()),
        ) as run_mock, patch.object(perf.PerfReportWriter, "log_peak_report"):
            perf.run_with_logging(
                args=["python", "-m", "build"],
                execution_policy=perf.ExecutionPolicy(
                    run_kwargs={"cwd": "/tmp", "env": {"A": "B"}, "text": True},
                    check=True,
                ),
                config=perf.PerfLogConfig(
                    repo_root=None,
                    cmdline_allowlist=("cargo", "maturin"),
                ),
            )

        run_mock.assert_called_once_with(
            ["python", "-m", "build"],
            {"cwd": "/tmp", "env": {"A": "B"}, "text": True},
            defaults.sampling_interval_sec,
            perf.HybridOffenderSelectionPolicy(
                min_entries=hybrid_defaults.min_entries,
                min_tree_share=hybrid_defaults.min_tree_share,
            ),
            ("cargo", "maturin"),
        )

    def test_run_with_logging_respects_check_flag(self) -> None:
        with patch.object(perf.PerfReportWriter, "log_build_context"), patch.object(
            perf.PerfReportWriter,
            "log_system_memory_snapshot_phase",
        ), patch.object(
            perf,
            "_run_command_with_psutil_logging",
            return_value=(2, perf.PeakProcessTree()),
        ), patch.object(perf.PerfReportWriter, "log_peak_report"):
            result = perf.run_with_logging(
                args=["python", "-m", "build"],
                execution_policy=perf.ExecutionPolicy(
                    run_kwargs={"cwd": "/tmp", "env": None, "text": True},
                    check=False,
                ),
                config=perf.PerfLogConfig(repo_root=None),
            )
        self.assertIsInstance(result, subprocess.CompletedProcess)
        self.assertEqual(result.args, ["python", "-m", "build"])
        self.assertEqual(result.returncode, 2)

    def test_run_with_logging_raises_when_check_true_and_nonzero(self) -> None:
        with patch.object(perf.PerfReportWriter, "log_build_context"), patch.object(
            perf.PerfReportWriter,
            "log_system_memory_snapshot_phase",
        ), patch.object(
            perf,
            "_run_command_with_psutil_logging",
            return_value=(2, perf.PeakProcessTree()),
        ), patch.object(perf.PerfReportWriter, "log_peak_report"):
            with self.assertRaises(subprocess.CalledProcessError):
                perf.run_with_logging(
                    args=["python", "-m", "build"],
                    execution_policy=perf.ExecutionPolicy(
                        run_kwargs={"cwd": "/tmp", "env": None, "text": True},
                        check=True,
                    ),
                    config=perf.PerfLogConfig(repo_root=None),
                )

    def test_run_with_logging_rejects_unsupported_run_kwargs(self) -> None:
        invalid_kwargs = cast(
            perf.SubprocessRunKwargs,
            {
                "cwd": "/tmp",
                "env": None,
                "text": True,
                "timeout": 5,
            },
        )
        with self.assertRaises(ValueError):
            perf.run_with_logging(
                args=["python", "-m", "build"],
                execution_policy=perf.ExecutionPolicy(
                    run_kwargs=invalid_kwargs,
                    check=False,
                ),
                config=perf.PerfLogConfig(repo_root=None),
            )

    def test_raise_for_nonzero_return_raises(self) -> None:
        with self.assertRaises(subprocess.CalledProcessError):
            perf._raise_for_nonzero_return(2, ["python", "-m", "build"])


if __name__ == "__main__":
    unittest.main()
