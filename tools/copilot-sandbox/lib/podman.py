"""Selecting, checking and preparing the Podman engine that runs the sandbox."""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path, PureWindowsPath
import re
import sys

from .common import LauncherError, run

MACHINE = "qdk-sandbox"
MINIMUM_VERSION = (4, 9)
INTERFACE_LABEL = "qdk.copilot-sandbox.interface"
INTERFACE_VERSION = "2"
# Docker's default seccomp profile (moby/profiles, Apache-2.0). Podman's own default lets a container
# create user namespaces, a common first step in kernel exploits, and open vsock sockets to the hypervisor.
SECCOMP_PROFILE = Path(__file__).resolve().parents[1] / "seccomp.json"
# Windows and macOS run containers in a dedicated Podman machine; Linux runs them directly.
CONNECTION: str | None = MACHINE if sys.platform in ("win32", "darwin") else None


@dataclass(frozen=True)
class Engine:
    architecture: str
    network: str


def podman(*args: str | os.PathLike[str]) -> list[str]:
    if CONNECTION is None:
        return ["podman", *map(os.fspath, args)]
    return ["podman", "--connection", CONNECTION, *map(os.fspath, args)]


def vm_path(path: str | os.PathLike[str]) -> str:
    """Returns the path at which Podman's Linux VM sees a host path."""
    if sys.platform != "win32":
        return os.fspath(path)
    windows = PureWindowsPath(path)
    if not re.fullmatch(r"[A-Za-z]:", windows.drive):
        raise LauncherError(f"Sandbox paths must be on a local drive: {path}")
    return "/mnt/" + windows.drive[0].lower() + "/" + "/".join(windows.parts[1:])


def _machine_hint() -> str:
    if CONNECTION != MACHINE:
        return ""
    return (
        f"\nRun 'sandbox.py setup-machine' once, or 'podman machine start {MACHINE}' "
        "if it already exists."
    )


def check_podman() -> Engine:
    result = run(podman("info", "--format", "json"), check=False)
    if result.returncode:
        raise LauncherError(
            "Podman is unavailable; Podman >= 4.9 is required." + _machine_hint()
        )
    try:
        info = json.loads(result.stdout)
        host = info["host"]
        match = re.match(r"(\d+)\.(\d+)", info["version"]["Version"])
        version = (int(match[1]), int(match[2])) if match else (0, 0)
        rootless = host["security"]["rootless"]
        seccomp = host["security"]["seccompEnabled"]
        operating_system = host["os"]
        cgroups = host["cgroupVersion"]
        architecture = host["arch"]
        network = host.get("rootlessNetworkCmd") or "slirp4netns"
    except (KeyError, TypeError, ValueError) as error:
        raise LauncherError("Cannot read Podman host information.") from error
    if version < MINIMUM_VERSION:
        raise LauncherError("Podman >= 4.9 is required.")
    if operating_system != "linux" or cgroups != "v2":
        raise LauncherError("Podman must run Linux containers with cgroup v2.")
    if not rootless:
        raise LauncherError("The sandbox requires rootless Podman." + _machine_hint())
    if not seccomp:
        raise LauncherError("Podman reports that seccomp is unavailable.")
    if network not in ("pasta", "slirp4netns"):
        raise LauncherError(f"Unsupported rootless network: {network}")
    if CONNECTION == MACHINE and sys.platform == "win32":
        _check_windows_interop()
    return Engine(architecture, network)


def _check_windows_interop() -> None:
    # WSL lets Linux processes start Windows programs; a container escape must not inherit that.
    result = run(
        [
            "podman",
            "machine",
            "ssh",
            MACHINE,
            "if /mnt/c/Windows/System32/whoami.exe >/dev/null 2>&1; "
            "then echo interop-enabled; else echo interop-disabled; fi",
        ],
        check=False,
    )
    if result.stdout.strip().splitlines()[-1:] != ["interop-disabled"]:
        raise LauncherError(
            f"The {MACHINE} Podman machine can start Windows programs. "
            "Run 'sandbox.py setup-machine' to turn that off."
        )


_DISABLE_INTEROP = r"""
set -eu
file=/etc/wsl.conf
touch "$file"
awk '/^[[:space:]]*\[/ { skip = ($0 ~ /^[[:space:]]*\[interop\][[:space:]]*$/) } !skip' "$file" > "$file.new"
printf '\n[interop]\nenabled=false\nappendWindowsPath=false\n' >> "$file.new"
mv "$file.new" "$file"
"""


def setup_machine(shared: list[Path]) -> None:
    if CONNECTION != MACHINE:
        raise LauncherError(
            "setup-machine is for Windows and macOS; on Linux, rootless Podman runs the sandbox directly."
        )
    if run(["podman", "machine", "inspect", MACHINE], check=False).returncode == 0:
        print(f"The {MACHINE} Podman machine already exists.")
    else:
        command = ["podman", "machine", "init", "--rootful=false"]
        if sys.platform == "darwin":
            # Only these folders are visible inside the VM, which limits what a container escape reaches.
            for path in dict.fromkeys(shared):
                if "," in str(path) or ":" in str(path):
                    raise LauncherError(
                        f"Cannot share a path containing ':' or ',': {path}"
                    )
                command += ["--volume", f"{path}:{path}"]
        run([*command, MACHINE], interactive=True)
    if (
        json.loads(run(["podman", "machine", "inspect", MACHINE]).stdout)[0].get(
            "State"
        )
        != "running"
    ):
        run(["podman", "machine", "start", MACHINE], interactive=True)
    run(
        [
            "podman",
            "machine",
            "ssh",
            MACHINE,
            'mkdir -p "$HOME/.config/containers" && : > "$HOME/.config/containers/mounts.conf"',
        ]
    )
    if sys.platform == "win32":
        run(
            [
                "podman",
                "machine",
                "ssh",
                "--username",
                "root",
                MACHINE,
                _DISABLE_INTEROP,
            ]
        )
        # WSL rereads wsl.conf only when the distribution restarts.
        run(["podman", "machine", "stop", MACHINE], interactive=True)
        run(["podman", "machine", "start", MACHINE], interactive=True)
    check_podman()
    print(f"The {MACHINE} Podman machine is ready.")


def machine_shares() -> list[str]:
    if CONNECTION != MACHINE or sys.platform != "darwin":
        return []
    result = run(["podman", "machine", "inspect", MACHINE])
    try:
        return [
            mount["Source"]
            for mount in json.loads(result.stdout)[0].get("Mounts") or []
        ]
    except (ValueError, KeyError, IndexError, TypeError) as error:
        raise LauncherError(
            "Cannot read the Podman machine's shared folders."
        ) from error


def host_filesystems() -> set[str]:
    """Returns the device numbers of filesystems mounted where Podman runs (the VM, on Windows and macOS)."""
    if CONNECTION == MACHINE:
        mountinfo = run(
            ["podman", "machine", "ssh", MACHINE, "cat /proc/self/mountinfo"]
        ).stdout
    else:
        mountinfo = Path("/proc/self/mountinfo").read_text()
    return {line.split()[2] for line in mountinfo.splitlines() if line.strip()}


def normalize_image_id(value: str) -> str:
    value = value.strip().removeprefix("sha256:")
    if not re.fullmatch(r"[0-9a-f]{64}", value):
        raise ValueError("Invalid image ID.")
    return "sha256:" + value
