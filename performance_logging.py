#!/usr/bin/env python3

# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import ctypes
import functools
import os
import platform
import subprocess
import sys
import time

# Keep logs interleaved with subprocess output.
print = functools.partial(print, flush=True)

PERF_LOG_PREFIX = "build.py: [perf-log]"


def performance_logging_enabled():
    # Keep performance logging policy centralized here for future opt-out logic.
    return True


def _log_header(label):
    print(f"{PERF_LOG_PREFIX} {label}")


def _log_system_memory_snapshot():
    # Best-effort, dependency-free snapshots for CI-safe diagnostics.
    # OS-native tooling differs by platform, so peak-process memory parity is currently
    # strongest on Linux/macOS (time wrappers) and limited on Windows.
    # Future iterations can add optional cross-platform process-tree sampling (e.g., psutil).
    system = platform.system()
    if system == "Linux":
        try:
            interesting = {
                "MemTotal",
                "MemFree",
                "MemAvailable",
                "SwapTotal",
                "SwapFree",
            }
            with open("/proc/meminfo", encoding="utf-8") as file:
                for line in file:
                    if line.split(":", 1)[0] in interesting:
                        print(f"{PERF_LOG_PREFIX} {line.strip()}")
        except Exception as exc:  # best effort diagnostics
            print(
                f"{PERF_LOG_PREFIX} meminfo unavailable"
                + f" ({type(exc).__name__}): {exc}"
            )
    elif system == "Darwin":
        try:
            total = subprocess.run(
                ["sysctl", "-n", "hw.memsize"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout.strip()
            print(f"{PERF_LOG_PREFIX} hw.memsize={total}")
        except Exception as exc:
            print(
                f"{PERF_LOG_PREFIX} hw.memsize unavailable"
                + f" ({type(exc).__name__}): {exc}"
            )
        try:
            swap = subprocess.run(
                ["sysctl", "vm.swapusage"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout.strip()
            print(f"{PERF_LOG_PREFIX} {swap}")
        except Exception as exc:
            print(
                f"{PERF_LOG_PREFIX} vm.swapusage unavailable"
                + f" ({type(exc).__name__}): {exc}"
            )
        try:
            pressure = subprocess.run(
                ["memory_pressure", "-Q"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout.strip()
            for line in pressure.splitlines()[:8]:
                print(f"{PERF_LOG_PREFIX} {line}")
        except Exception as exc:
            print(
                f"{PERF_LOG_PREFIX} memory_pressure unavailable"
                + f" ({type(exc).__name__}): {exc}"
            )
        try:
            vm_stat = subprocess.run(
                ["vm_stat"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout.strip()
            for line in vm_stat.splitlines()[:8]:
                print(f"{PERF_LOG_PREFIX} {line}")
        except Exception as exc:
            print(
                f"{PERF_LOG_PREFIX} vm_stat unavailable"
                + f" ({type(exc).__name__}): {exc}"
            )
    elif system == "Windows":
        try:
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

            status = MEMORYSTATUSEX()
            status.dwLength = ctypes.sizeof(MEMORYSTATUSEX)
            if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
                print(
                    f"{PERF_LOG_PREFIX} "
                    + f"memory_load={status.dwMemoryLoad}% "
                    + f"total_phys={status.ullTotalPhys} "
                    + f"avail_phys={status.ullAvailPhys} "
                    + f"total_pagefile={status.ullTotalPageFile} "
                    + f"avail_pagefile={status.ullAvailPageFile}"
                )
            else:
                print(f"{PERF_LOG_PREFIX} GlobalMemoryStatusEx failed")
        except Exception as exc:
            print(
                f"{PERF_LOG_PREFIX} windows memory unavailable"
                + f" ({type(exc).__name__}): {exc}"
            )


def _log_build_context(label, args):
    _log_header(label)
    print(f"{PERF_LOG_PREFIX} platform={platform.platform()}")
    print(f"{PERF_LOG_PREFIX} python={sys.version.split()[0]}")
    print(f"{PERF_LOG_PREFIX} command={' '.join(args)}")
    _log_system_memory_snapshot()


def _sanitize_path(path, repo_root):
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


def _sanitize_command_args(args, repo_root):
    sanitized = []
    for token in args:
        token = str(token)
        if os.path.isabs(token):
            token = _sanitize_path(token, repo_root)
        sanitized.append(token)
    return sanitized


def _time_wrapper_for_platform():
    if platform.system() == "Linux" and os.path.exists("/usr/bin/time"):
        return ["/usr/bin/time", "-v"]
    if platform.system() == "Darwin" and os.path.exists("/usr/bin/time"):
        return ["/usr/bin/time", "-l"]
    return None


def _stream_timed_command(args, cwd, env, wrapper, sanitized_args):
    cmd = [*wrapper, *args]
    process = subprocess.Popen(  # pylint: disable=consider-using-with
        cmd,
        cwd=cwd,
        env=env,
        text=True,
        stdout=None,
        stderr=subprocess.PIPE,
    )

    assert process.stderr is not None
    in_timing_report = False
    sanitized_timed_cmd = ' '.join(sanitized_args)

    for line in process.stderr:
        stripped = line.rstrip("\n")
        if stripped.lstrip().startswith("Command being timed:"):
            in_timing_report = True
            stripped = f'   Command being timed: "{sanitized_timed_cmd}"'

        if in_timing_report:
            print(f"{PERF_LOG_PREFIX} {stripped}")
            if stripped.lstrip().startswith("Exit status:"):
                in_timing_report = False
        else:
            print(stripped)

    return process.wait()


def run_native_wheel_build_with_logging(args, cwd, env=None, repo_root=None):
    sanitized_args = _sanitize_command_args(args, repo_root)
    _log_build_context("qdk native wheel pre-build", sanitized_args)
    start = time.time()

    wrapper = _time_wrapper_for_platform()
    if wrapper is not None:
        print(
            f"{PERF_LOG_PREFIX} peak indicator source={' '.join(wrapper)}"
            + " (check command output for max RSS)"
        )
    elif platform.system() == "Windows":
        print(
            f"{PERF_LOG_PREFIX} peak process RSS not available via built-in wrapper"
            + " on Windows"
        )

    try:
        if wrapper is None:
            subprocess.run(args, check=True, text=True, cwd=cwd, env=env)
        else:
            return_code = _stream_timed_command(
                args,
                cwd=cwd,
                env=env,
                wrapper=wrapper,
                sanitized_args=sanitized_args,
            )
            if return_code != 0:
                raise subprocess.CalledProcessError(return_code, args)
    except Exception:
        elapsed = time.time() - start
        print(f"{PERF_LOG_PREFIX} qdk native wheel failed after {elapsed:.3f}s")
        _log_build_context("qdk native wheel failure snapshot", sanitized_args)
        raise

    elapsed = time.time() - start
    print(f"{PERF_LOG_PREFIX} qdk native wheel completed in {elapsed:.3f}s")
    _log_build_context("qdk native wheel post-build", sanitized_args)
