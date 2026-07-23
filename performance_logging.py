#!/usr/bin/env python3
"""Performance logging helpers for build subprocess execution.

Public API:
- PerfLogConfig: runtime configuration for instrumentation.
- SubprocessRunKwargs: supported process-launch kwargs.
- ExecutionPolicy: caller-owned execution behavior.
- run_with_logging: instrumented subprocess execution entrypoint.
"""

# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import ctypes
from dataclasses import dataclass, field
from enum import Enum
import os
import platform
import shlex
import subprocess
import sys
import textwrap
import time
from typing import List, Mapping, Optional, Protocol, Sequence, TypedDict

import psutil


class OffenderPolicy(str, Enum):
    """Supported offender-selection policy identifiers."""

    HYBRID = "hybrid"
    TOP_N = "top_n"


@dataclass(frozen=True)
class HybridOffenderPolicyConfig:
    """Configuration for hybrid offender selection."""

    kind: OffenderPolicy = OffenderPolicy.HYBRID
    min_entries: int = 5
    min_tree_share: float = 0.05


@dataclass(frozen=True)
class TopNOffenderPolicyConfig:
    """Configuration for top-N offender selection."""

    kind: OffenderPolicy = OffenderPolicy.TOP_N
    top_n: int = 5


OffenderPolicyConfig = HybridOffenderPolicyConfig | TopNOffenderPolicyConfig


@dataclass(frozen=True)
class LogRenderConfig:
    """Rendering settings for structured perf-log output."""

    prefix: str = "build.py: [perf-log]"
    indent_unit: int = 2
    wrap_width: int = 140
    cmdline_limit: int = 140

@dataclass(frozen=True)
class PerfLogConfig:
    """Configuration for performance-logging execution.

    repo_root: repository path used for sanitizing absolute paths in logs.
    sampling_interval_sec: polling interval for process-tree RSS sampling.
    offender_policy: offender-selection configuration union.
    cmdline_allowlist: process names allowed to emit command lines (empty disables cmdline logging).
    log_render_config: output rendering configuration.
    log_context_label: human-readable label used only for emitted log text.
    """

    repo_root: Optional[str] = None
    sampling_interval_sec: float = 0.1
    offender_policy: OffenderPolicyConfig = field(default_factory=HybridOffenderPolicyConfig)
    cmdline_allowlist: Sequence[str] = ()
    log_render_config: LogRenderConfig = field(default_factory=LogRenderConfig)
    log_context_label: str = "instrumented command"


class SubprocessRunKwargs(TypedDict):
    """Supported process-launch kwargs for perf logging.

    These are intentionally restricted to keep parity with current build usage
    and to make logging-path behavior explicit.
    """

    cwd: str
    env: Optional[Mapping[str, str]]
    text: bool


@dataclass(frozen=True)
class ExecutionPolicy:
    """Execution behavior owned by the caller.

    run_kwargs are passed directly to process launch.
    check controls whether non-zero return codes are promoted to exceptions.
    """

    run_kwargs: SubprocessRunKwargs
    check: bool = True


def run_with_logging(
    args: Sequence[str],
    execution_policy: ExecutionPolicy,
    config: PerfLogConfig,
) -> subprocess.CompletedProcess[str]:
    """Run a command with process-tree memory sampling and perf-log output.

    This is the module's public entrypoint for instrumented subprocess
    execution. The caller owns execution policy; this function applies
    best-effort diagnostics and peak memory sampling around that policy.
    """

    # The caller owns runtime behavior policy; we only validate input shape
    # and apply logging/sampling around that policy.
    _validate_run_kwargs(execution_policy.run_kwargs)
    logger = PerfLogger.from_config(config)
    reporter = PerfReportWriter(logger)
    log_context_label = config.log_context_label

    sanitized_args = _sanitize_command_args(args, config.repo_root)
    reporter.log_build_context(
        f"{log_context_label} pre-build",
        "pre-run",
        sanitized_args,
    )
    reporter.log_system_memory_snapshot_phase(
        "pre-run",
        _system_memory_snapshot_for_system(platform.system()),
    )
    start = time.time()

    sampling_logger = logger.section("peak-memory-sampling")
    sampling_logger.line("source=psutil process-tree sampling")
    sampling_logger.line(f"offender-policy={config.offender_policy.kind.value}")

    offender_policy = _offender_selection_policy_from_config(config)

    try:
        return_code, peak_report = _run_command_with_psutil_logging(
            args,
            execution_policy.run_kwargs,
            config.sampling_interval_sec,
            offender_policy,
            config.cmdline_allowlist,
        )
        reporter.log_peak_report(peak_report)
        if execution_policy.check:
            _raise_for_nonzero_return(return_code, args)
    except Exception:
        elapsed = time.time() - start
        logger.line(f"{log_context_label} failed after {elapsed:.3f}s")
        reporter.log_build_context(
            f"{log_context_label} failure snapshot",
            "failure",
            sanitized_args,
        )
        reporter.log_system_memory_snapshot_phase(
            "failure",
            _system_memory_snapshot_for_system(platform.system()),
        )
        raise

    elapsed = time.time() - start
    logger.line(f"{log_context_label} completed in {elapsed:.3f}s")
    reporter.log_build_context(
        f"{log_context_label} post-build",
        "post-run",
        sanitized_args,
        include_environment=False,
    )
    reporter.log_system_memory_snapshot_phase(
        "post-run",
        _system_memory_snapshot_for_system(platform.system()),
    )
    return subprocess.CompletedProcess(args=args, returncode=return_code)


@dataclass(frozen=True)
class ProcessRssEntry:
    """RSS information for a single process at one sample instant."""

    pid: int
    name: str
    rss: int
    cmdline: str
    role: str = ""
    ppid: int = 0
    exe: str = ""


@dataclass(frozen=True)
class ProcessTreeSample:
    """One point-in-time memory sample for a process tree."""

    root_rss: int
    tree_rss: int
    largest_process: Optional[ProcessRssEntry]
    rss_entries: List[ProcessRssEntry]


@dataclass
class PeakProcessTree:
    """Aggregate peak metrics derived from multiple ProcessTreeSample values."""

    peak_root_rss: int = 0
    peak_tree_rss: int = 0
    peak_largest_process: Optional[ProcessRssEntry] = None
    peak_tree_top_offenders: List[ProcessRssEntry] = field(default_factory=list)


@dataclass(frozen=True)
class SystemMemorySnapshot:
    """System memory snapshot payload prepared outside reporting."""

    lines: List[str]


class OffenderSelectionPolicy(Protocol):
    """Select relevant offenders from a single process-tree sample."""

    def select_offenders(self, sample_data: ProcessTreeSample) -> List[ProcessRssEntry]:
        """Return offenders for the sample ordered by descending relevance."""
        ...


@dataclass(frozen=True)
class TopNOffenderSelectionPolicy:
    """Select the top N processes by RSS."""

    limit: int

    def select_offenders(self, sample_data: ProcessTreeSample) -> List[ProcessRssEntry]:
        if self.limit <= 0:
            return []
        return sorted(sample_data.rss_entries, key=lambda entry: entry.rss, reverse=True)[: self.limit]


@dataclass(frozen=True)
class HybridOffenderSelectionPolicy:
    """Threshold-first selection with top-RSS fallback up to a minimum count."""

    min_entries: int
    min_tree_share: float

    def select_offenders(self, sample_data: ProcessTreeSample) -> List[ProcessRssEntry]:
        ordered = sorted(sample_data.rss_entries, key=lambda entry: entry.rss, reverse=True)
        if not ordered:
            return []

        threshold_bytes_from_share = int(sample_data.tree_rss * self.min_tree_share)
        selected = [entry for entry in ordered if entry.rss >= threshold_bytes_from_share]

        if ordered[0] not in selected:
            selected.insert(0, ordered[0])

        min_target = max(self.min_entries, 0)
        if len(selected) < min_target:
            selected_pids = {entry.pid for entry in selected}
            for entry in ordered:
                if entry.pid in selected_pids:
                    continue
                selected.append(entry)
                selected_pids.add(entry.pid)
                if len(selected) >= min_target:
                    break

        return selected


def _offender_selection_policy_from_config(config: PerfLogConfig) -> OffenderSelectionPolicy:
    offender_policy = config.offender_policy

    if isinstance(offender_policy, TopNOffenderPolicyConfig):
        return TopNOffenderSelectionPolicy(limit=offender_policy.top_n)
    if isinstance(offender_policy, HybridOffenderPolicyConfig):
        return HybridOffenderSelectionPolicy(
            min_entries=offender_policy.min_entries,
            min_tree_share=offender_policy.min_tree_share,
        )

    raise TypeError(f"unsupported offender policy config: {type(offender_policy).__name__}")


@dataclass(frozen=True)
class PerfLogger:
    """Generic structured logger for perf-log output rendering.

    Responsibility boundary:
    - Owns output mechanics only (prefixing, indentation, wrapping, emission).
    - Does not decide *what* build/perf information to log.

    Domain-specific reporting is owned by PerfReportWriter, which composes this
    logger to render concrete build/perf sections.
    """

    prefix: str
    indent_unit: int
    wrap_width: int
    cmdline_limit: int
    indent_level: int = 0

    @classmethod
    def from_config(cls, config: PerfLogConfig) -> "PerfLogger":
        return cls(
            prefix=config.log_render_config.prefix,
            indent_unit=config.log_render_config.indent_unit,
            wrap_width=config.log_render_config.wrap_width,
            cmdline_limit=config.log_render_config.cmdline_limit,
        )

    def line(self, message: str) -> None:
        indent = " " * (self.indent_unit * self.indent_level)
        print(f"{self.prefix} {indent}{message}", flush=True)

    def section(self, title: str) -> "PerfLogger":
        self.line(f"{title}:")
        return self.indented()

    def indented(self, levels: int = 1) -> "PerfLogger":
        return PerfLogger(
            prefix=self.prefix,
            indent_unit=self.indent_unit,
            indent_level=self.indent_level + levels,
            wrap_width=self.wrap_width,
            cmdline_limit=self.cmdline_limit,
        )

    def wrapped_field(self, key: str, value: str) -> None:
        prefix = f"{key}="
        available = max(40, self.wrap_width - len(prefix))
        wrapped_lines = textwrap.wrap(
            value,
            width=available,
            break_long_words=False,
            break_on_hyphens=False,
        )

        if not wrapped_lines:
            self.line(prefix)
            return

        self.line(prefix + wrapped_lines[0])
        continuation = self.indented()
        for line in wrapped_lines[1:]:
            continuation.line(line)

@dataclass(frozen=True)
class PerfReportWriter:
    """Domain-specific reporter that composes PerfLogger.

    Responsibility boundary:
    - Owns *what* performance/build sections are emitted.
    - Delegates *how* lines are rendered to PerfLogger.
    """

    logger: PerfLogger

    def log_build_context(
        self,
        label: str,
        phase_label: str,
        args: Sequence[str],
        include_environment: bool = True,
    ) -> None:
        self.logger.line(label)
        if include_environment:
            environment_logger = self.logger.section(f"environment ({phase_label})")
            environment_logger.line(f"platform={platform.platform()}")
            environment_logger.line(f"python={sys.version.split()[0]}")
            environment_logger.wrapped_field("command", " ".join(args))

    def log_system_memory_snapshot_phase(
        self,
        phase_label: str,
        system_memory_snapshot: "SystemMemorySnapshot",
    ) -> None:
        snapshot_logger = self.logger.section(f"os-memory-snapshot ({phase_label})")
        for line in system_memory_snapshot.lines:
            snapshot_logger.line(line)

    def log_peak_report(self, peak_report: "PeakProcessTree") -> None:
        summary_logger = self.logger.section("peak-memory-summary")
        summary_logger.line("indicator=psutil")
        summary_logger.line(f"peak-root-rss={_format_bytes(peak_report.peak_root_rss)}")
        if peak_report.peak_largest_process is not None:
            largest_process = peak_report.peak_largest_process
            summary_logger.line(
                f"max-single-process-rss-across-run={_format_bytes(largest_process.rss)}"
                + f" pid={largest_process.pid}"
                + f" ppid={largest_process.ppid}"
                + f" name={largest_process.name}"
            )
            process_logger = summary_logger.indented()
            if largest_process.exe:
                process_logger.wrapped_field("exe", largest_process.exe)
            if largest_process.cmdline:
                process_logger.wrapped_field("cmdline", largest_process.cmdline)
        summary_logger.line(
            f"peak-process-tree-rss={_format_bytes(peak_report.peak_tree_rss)}",
        )

        if peak_report.peak_tree_top_offenders:
            offenders_logger = summary_logger.section("process-tree-rss-offenders-at-tree-peak")
            for offender in peak_report.peak_tree_top_offenders:
                offenders_logger.line(
                    f"pid={offender.pid}"
                    + f" ppid={offender.ppid}"
                    + f" name={offender.name}"
                    + f" role={offender.role}"
                    + f" rss={_format_bytes(offender.rss)}"
                )
                offender_details_logger = offenders_logger.indented()
                if offender.exe:
                    offender_details_logger.wrapped_field("exe", offender.exe)
                if offender.cmdline:
                    offender_details_logger.wrapped_field("cmdline", offender.cmdline)


class SystemMemorySnapshotProvider(Protocol):
    """Abstraction for best-effort memory snapshot diagnostics per platform."""

    def snapshot_lines(self) -> List[str]:
        """Return human-readable memory snapshot lines.

        Implementations should be best effort and avoid raising for common
        environmental failures.
        """

        ...


class LinuxMemorySnapshotProvider:
    """Linux memory snapshot provider using /proc/meminfo."""

    _MEMINFO_KEYS_IN_ORDER = (
        "MemTotal",
        "MemFree",
        "MemAvailable",
        "SwapTotal",
        "SwapFree",
    )

    def snapshot_lines(self) -> List[str]:
        lines_by_key: dict[str, str] = {}
        try:
            with open("/proc/meminfo", encoding="utf-8") as file:
                for line in file:
                    key = line.split(":", 1)[0]
                    if key in self._MEMINFO_KEYS_IN_ORDER:
                        lines_by_key[key] = line.strip()
        except Exception as exc:  # best effort diagnostics
            return [f"meminfo unavailable ({type(exc).__name__}): {exc}"]

        return [
            lines_by_key[key]
            for key in self._MEMINFO_KEYS_IN_ORDER
            if key in lines_by_key
        ]


class DarwinMemorySnapshotProvider:
    """Darwin memory snapshot provider using sysctl/vm_stat utilities."""

    _MEMORY_PRESSURE_MAX_LINES = 8
    _VM_STAT_MAX_LINES = 8

    @staticmethod
    def _capture_output(command: Sequence[str]) -> str:
        return subprocess.run(
            command,
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()

    def snapshot_lines(self) -> List[str]:
        lines = []

        try:
            total = self._capture_output(["sysctl", "-n", "hw.memsize"])
            lines.append(f"hw.memsize={total}")
        except Exception as exc:
            lines.append(f"hw.memsize unavailable ({type(exc).__name__}): {exc}")

        try:
            swap = self._capture_output(["sysctl", "vm.swapusage"])
            lines.append(swap)
        except Exception as exc:
            lines.append(f"vm.swapusage unavailable ({type(exc).__name__}): {exc}")

        try:
            pressure = self._capture_output(["memory_pressure", "-Q"])
            lines.extend(pressure.splitlines()[:self._MEMORY_PRESSURE_MAX_LINES])
        except Exception as exc:
            lines.append(f"memory_pressure unavailable ({type(exc).__name__}): {exc}")

        try:
            vm_stat = self._capture_output(["vm_stat"])
            lines.extend(vm_stat.splitlines()[:self._VM_STAT_MAX_LINES])
        except Exception as exc:
            lines.append(f"vm_stat unavailable ({type(exc).__name__}): {exc}")

        return lines


class WindowsMemorySnapshotProvider:
    """Windows memory snapshot provider using GlobalMemoryStatusEx."""

    def snapshot_lines(self) -> List[str]:
        class MEMORYSTATUSEX(ctypes.Structure):
            _fields_ = [
                ("dwLength", ctypes.c_ulong),
                ("dwMemoryLoad", ctypes.c_ulong),
                ("ullTotalPhys", ctypes.c_ulonglong),
                ("ullAvailPhys", ctypes.c_ulonglong),
                ("ullTotalPageFile", ctypes.c_ulonglong),
                ("ullAvailPageFile", ctypes.c_ulonglong),
                ("ullTotalVirtual", ctypes.c_ulonglong),
                ("ullAvailVirtual", ctypes.c_ulonglong),
                ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
            ]

        try:
            status = MEMORYSTATUSEX()
            status.dwLength = ctypes.sizeof(MEMORYSTATUSEX)
            windll = getattr(ctypes, "windll", None)
            if windll is None:
                return ["GlobalMemoryStatusEx unavailable"]

            if windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
                return [
                    "memory_load={memory_load}% total_phys={total_phys} "
                    "avail_phys={avail_phys} total_pagefile={total_pagefile} "
                    "avail_pagefile={avail_pagefile}".format(
                        memory_load=status.dwMemoryLoad,
                        total_phys=status.ullTotalPhys,
                        avail_phys=status.ullAvailPhys,
                        total_pagefile=status.ullTotalPageFile,
                        avail_pagefile=status.ullAvailPageFile,
                    )
                ]
            return ["GlobalMemoryStatusEx failed"]
        except Exception as exc:
            return [f"windows memory unavailable ({type(exc).__name__}): {exc}"]


class EmptyMemorySnapshotProvider:
    """Fallback provider for unsupported platforms."""

    def snapshot_lines(self) -> List[str]:
        return []


def _system_memory_snapshot_for_system(system: str) -> SystemMemorySnapshot:
    provider = _system_memory_snapshot_provider_for_system(system)
    return SystemMemorySnapshot(lines=provider.snapshot_lines())


def _system_memory_snapshot_provider_for_system(system: str) -> SystemMemorySnapshotProvider:
    providers = {
        "Linux": LinuxMemorySnapshotProvider,
        "Darwin": DarwinMemorySnapshotProvider,
        "Windows": WindowsMemorySnapshotProvider,
    }
    provider_type = providers.get(system, EmptyMemorySnapshotProvider)
    return provider_type()


def _sanitize_path(path: str, repo_root: Optional[str]) -> str:
    if repo_root is not None:
        root = os.path.abspath(repo_root)
        if path == root:
            return "<repo>"
        if path.startswith(root + os.sep):
            return "<repo>" + path[len(root) :]

    home_dir = os.path.expanduser("~")
    if path == home_dir:
        return "~"
    if path.startswith(home_dir + os.sep):
        return "~" + path[len(home_dir) :]

    return path


def _sanitize_command_args(args: Sequence[object], repo_root: Optional[str]) -> List[str]:
    # Logging sanitizer entrypoint: currently normalizes absolute paths
    # (repo/home), and is the intentional place to add future token-level
    # sanitization rules if needed.
    sanitized = []
    for token in args:
        token = str(token)
        if os.path.isabs(token):
            token = _sanitize_path(token, repo_root)
        sanitized.append(token)
    return sanitized


def _format_bytes(value: int) -> str:
    return f"{value} bytes ({value / (1024 * 1024):.1f} MiB)"


def _join_cmdline(cmdline: Sequence[str]) -> str:
    if not cmdline:
        return ""

    return shlex.join([str(token) for token in cmdline])


def _is_cmdline_allowed(process_name: str, cmdline_allowlist: Sequence[str]) -> bool:
    if not cmdline_allowlist:
        return False
    normalized_name = process_name.lower()
    allowed = {name.lower() for name in cmdline_allowlist}
    return normalized_name in allowed


def _redact_cmdline_args(cmdline: Sequence[str]) -> List[str]:
    redacted: List[str] = []
    skip_next = False
    sensitive_flag_prefixes = (
        "--password",
        "--token",
        "--secret",
        "--apikey",
        "--api-key",
        "--access-token",
        "--pat",
    )

    def _looks_sensitive(value: str) -> bool:
        lowered = value.lower()
        return (
            "password" in lowered
            or "token" in lowered
            or "secret" in lowered
            or "apikey" in lowered
            or "api-key" in lowered
            or lowered.startswith(("ghp_", "github_pat_", "sk-", "xox"))
        )

    for token in [str(token) for token in cmdline]:
        lowered = token.lower()

        if skip_next:
            redacted.append("<redacted>")
            skip_next = False
            continue

        if any(lowered.startswith(prefix + "=") for prefix in sensitive_flag_prefixes):
            key = token.split("=", 1)[0]
            redacted.append(f"{key}=<redacted>")
            continue

        if any(lowered == prefix for prefix in sensitive_flag_prefixes):
            redacted.append(token)
            skip_next = True
            continue

        if _looks_sensitive(token):
            redacted.append("<redacted>")
            continue

        redacted.append(token)

    return redacted


def _derive_process_role(name: str, cmdline: Sequence[str]) -> str:
    """Derive a compact and low-risk process role from command arguments."""

    if not cmdline:
        return name

    executable = os.path.basename(cmdline[0]) if cmdline[0] else name

    def _normalize_role_token(token: str) -> str:
        if not token:
            return token
        if os.sep in token:
            return os.path.basename(token)
        return token

    subcommand = ""

    sensitive_flag_prefixes = (
        "--password",
        "--token",
        "--secret",
        "--apikey",
        "--api-key",
        "--access-token",
        "--pat",
    )

    def _is_sensitive_token(token: str) -> bool:
        lowered = token.lower()
        if any(prefix in lowered for prefix in ("password", "token", "secret", "apikey", "api-key")):
            return True
        if lowered.startswith(("ghp_", "github_pat_", "sk-", "xox")):
            return True
        return False

    index = 1
    skip_next = False
    while index < len(cmdline):
        token = cmdline[index]
        lowered = token.lower()

        if skip_next:
            skip_next = False
            index += 1
            continue

        if not token:
            index += 1
            continue

        if any(lowered.startswith(prefix) for prefix in sensitive_flag_prefixes):
            # Handle both --flag value and --flag=value forms.
            if "=" not in lowered:
                skip_next = True
            index += 1
            continue

        if token in ("-m", "-c") and (index + 1) < len(cmdline):
            subcommand = _normalize_role_token(cmdline[index + 1])
            break

        if token.startswith("-"):
            index += 1
            continue

        if _is_sensitive_token(token):
            index += 1
            continue

        subcommand = _normalize_role_token(token)
        break

    if subcommand:
        return f"{executable} {subcommand}"
    return executable


def _build_process_tree(root_process: psutil.Process) -> List[psutil.Process]:
    process_list = [root_process]
    try:
        process_list.extend(root_process.children(recursive=True))
    except (psutil.AccessDenied, psutil.NoSuchProcess, psutil.ZombieProcess):
        pass
    return process_list


def _sample_process_tree(
    root_process: psutil.Process,
    cmdline_allowlist: Sequence[str],
) -> ProcessTreeSample:
    rss_entries = []
    root_rss = 0
    process_list = _build_process_tree(root_process)

    for current_process in process_list:
        try:
            rss = current_process.memory_info().rss
            name = current_process.name()
            raw_cmdline = current_process.cmdline()
            cmdline = ""
            if _is_cmdline_allowed(name, cmdline_allowlist):
                cmdline = _join_cmdline(_redact_cmdline_args(raw_cmdline))
            role = _derive_process_role(name, raw_cmdline)
            ppid = current_process.ppid()
            exe = current_process.exe()
        except (psutil.AccessDenied, psutil.NoSuchProcess, psutil.ZombieProcess):
            continue

        if current_process.pid == root_process.pid:
            root_rss = rss

        rss_entries.append(
            ProcessRssEntry(
                pid=current_process.pid,
                name=name,
                rss=rss,
                cmdline=cmdline,
                role=role,
                ppid=ppid,
                exe=exe,
            )
        )

    tree_rss = sum(entry.rss for entry in rss_entries)
    top_offenders = sorted(rss_entries, key=lambda entry: entry.rss, reverse=True)
    largest_process = top_offenders[0] if top_offenders else None
    return ProcessTreeSample(
        root_rss=root_rss,
        tree_rss=tree_rss,
        largest_process=largest_process,
        rss_entries=rss_entries,
    )


def _update_peak_process_tree(
    peak_data: PeakProcessTree,
    sample_data: ProcessTreeSample,
    offender_selection_policy: OffenderSelectionPolicy,
) -> PeakProcessTree:
    peak_data.peak_root_rss = max(peak_data.peak_root_rss, sample_data.root_rss)

    if sample_data.largest_process is not None and (
        peak_data.peak_largest_process is None
        or sample_data.largest_process.rss >= peak_data.peak_largest_process.rss
    ):
        peak_data.peak_largest_process = sample_data.largest_process

    if sample_data.tree_rss >= peak_data.peak_tree_rss:
        peak_data.peak_tree_rss = sample_data.tree_rss
        peak_data.peak_tree_top_offenders = offender_selection_policy.select_offenders(sample_data)

    return peak_data


def _monitor_peak_process_tree(
    process: subprocess.Popen[str],
    sampling_interval_sec: float,
    offender_selection_policy: OffenderSelectionPolicy,
    cmdline_allowlist: Sequence[str] = (),
) -> PeakProcessTree:
    root_process = psutil.Process(process.pid)
    peak_data = PeakProcessTree()

    while True:
        try:
            sample_data = _sample_process_tree(
                root_process,
                cmdline_allowlist=cmdline_allowlist,
            )
        except (psutil.AccessDenied, psutil.NoSuchProcess, psutil.ZombieProcess):
            sample_data = ProcessTreeSample(
                root_rss=0,
                tree_rss=0,
                largest_process=None,
                rss_entries=[],
            )

        peak_data = _update_peak_process_tree(
            peak_data,
            sample_data,
            offender_selection_policy,
        )

        if process.poll() is not None:
            break

        time.sleep(sampling_interval_sec)

    return peak_data


def _run_command_with_psutil_logging(
    args: Sequence[str],
    run_kwargs: SubprocessRunKwargs,
    sampling_interval_sec: float,
    offender_selection_policy: OffenderSelectionPolicy,
    cmdline_allowlist: Sequence[str],
) -> tuple[int, PeakProcessTree]:
    process = subprocess.Popen(  # pylint: disable=consider-using-with
        args,
        **run_kwargs,
    )

    peak_report = _monitor_peak_process_tree(
        process,
        sampling_interval_sec=sampling_interval_sec,
        offender_selection_policy=offender_selection_policy,
        cmdline_allowlist=cmdline_allowlist,
    )
    return process.wait(), peak_report


def _raise_for_nonzero_return(return_code: int, args: Sequence[str]) -> None:
    """Raise subprocess.CalledProcessError for non-zero exits.

    We raise explicitly in the perf-logging path to preserve behavioral parity
    with `subprocess.run(..., check=True)`: callers should observe the same
    failure contract regardless of whether instrumentation is enabled.
    """

    if return_code != 0:
        raise subprocess.CalledProcessError(return_code, args)


def _validate_run_kwargs(run_kwargs: SubprocessRunKwargs) -> None:
    """Enforce an explicit subprocess kwargs contract for perf logging.

    This strict allow-list is intentional: it keeps normal subprocess execution
    and instrumented execution behavior aligned, and forces new kwargs support
    to be an explicit reviewed change instead of a silent drift.
    """

    allowed_keys = {"cwd", "env", "text"}
    extra_keys = set(run_kwargs) - allowed_keys
    if extra_keys:
        extras = ", ".join(sorted(extra_keys))
        raise ValueError(f"unsupported subprocess kwargs for perf logging: {extras}")
