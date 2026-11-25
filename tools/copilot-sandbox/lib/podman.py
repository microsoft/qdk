"""Selecting, checking and preparing the Podman engine that runs the sandbox."""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path, PureWindowsPath
import re
import sys

from .common import LauncherError, run, sanitize_diagnostic

MACHINE = "qdk-sandbox"
# Podman machines (Windows, macOS) need 6: 5.x mishandles cgroups in WSL.
MINIMUM_VERSION: tuple[int, int] = (4, 9)
MACHINE_MINIMUM_VERSION: tuple[int, int] = (6, 0)
INTERFACE_LABEL = "qdk.copilot-sandbox.interface"
INTERFACE_VERSION = "2"
# Docker's default seccomp profile (moby/profiles, Apache-2.0). Podman's own default lets a container
# create user namespaces, a common first step in kernel exploits, and open vsock sockets to the hypervisor.
SECCOMP_PROFILE = Path(__file__).resolve().parents[1] / "seccomp.json"
# Windows and macOS run containers in a dedicated Podman machine; Linux runs them directly.
CONNECTION: str | None = MACHINE if sys.platform in ("win32", "darwin") else None
SELINUX_ENFORCE = Path("/sys/fs/selinux/enforce")


@dataclass(frozen=True)
class Engine:
    architecture: str
    network: str
    # IDs in rootless Podman's own user namespace: you (0) plus your subordinate IDs.
    id_count: int


def podman(*args: str | os.PathLike[str]) -> list[str]:
    if CONNECTION is None:
        return ["podman", *[os.fspath(arg) for arg in args]]
    return ["podman", "--connection", CONNECTION, *[os.fspath(arg) for arg in args]]


def vm_path(path: str | os.PathLike[str]) -> str:
    """Returns the path at which Podman's Linux VM sees a host path."""
    if sys.platform != "win32":
        return os.fspath(path)
    windows = PureWindowsPath(path)
    if not re.fullmatch(r"[A-Za-z]:", windows.drive):
        raise LauncherError(f"Sandbox paths must be on a local drive: {path}")
    return "/mnt/" + windows.drive[0].lower() + "/" + "/".join(windows.parts[1:])


def _minimum_version() -> tuple[int, int]:
    return MACHINE_MINIMUM_VERSION if CONNECTION == MACHINE else MINIMUM_VERSION


def _required_version() -> str:
    major, minor = _minimum_version()
    return f"{major}.{minor}"


def _machine_hint() -> str:
    if CONNECTION != MACHINE:
        return ""
    return (
        f"\nRun 'sandbox.py setup-machine' once, or 'podman machine start {MACHINE}' "
        "if it already exists."
    )


def check_podman_version() -> dict:
    """Checks that Podman is reachable and new enough, and returns its 'podman info'."""
    result = run(podman("info", "--format", "json"), check=False)
    if result.returncode:
        raise LauncherError(
            f"Podman is unavailable; Podman >= {_required_version()} is required."
            + _machine_hint()
        )
    try:
        info = json.loads(result.stdout)
        match = re.match(r"(\d+)\.(\d+)", info["version"]["Version"])
        version: tuple[int, int] = (int(match[1]), int(match[2])) if match else (0, 0)
    except (KeyError, TypeError, ValueError) as error:
        raise LauncherError("Cannot read Podman host information.") from error
    if version < _minimum_version():
        raise LauncherError(f"Podman >= {_required_version()} is required.")
    return info


def check_podman() -> Engine:
    print("Checking Podman host configuration...", flush=True)
    info = check_podman_version()
    try:
        host = info["host"]
        rootless = host["security"]["rootless"]
        seccomp = host["security"]["seccompEnabled"]
        selinux = host["security"]["selinuxEnabled"]
        operating_system = host["os"]
        cgroups = host["cgroupVersion"]
        architecture = host["arch"]
        network = host.get("rootlessNetworkCmd") or "slirp4netns"
        id_count = min(
            sum(entry["size"] for entry in host["idMappings"][kind])
            for kind in ("uidmap", "gidmap")
        )
    except (KeyError, TypeError, ValueError) as error:
        raise LauncherError("Cannot read Podman host information.") from error
    if operating_system != "linux" or cgroups != "v2":
        raise LauncherError("Podman must run Linux containers with cgroup v2.")
    if not rootless:
        raise LauncherError("The sandbox requires rootless Podman." + _machine_hint())
    if not seccomp:
        raise LauncherError("Podman reports that seccomp is unavailable.")
    if network not in ("pasta", "slirp4netns"):
        raise LauncherError(f"Unsupported rootless network: {network}")
    # The macOS machine enforces SELinux too, but labels its shared folders so containers can read them.
    if CONNECTION is None and selinux:
        _check_selinux_mode()
    if CONNECTION == MACHINE and sys.platform == "win32":
        _check_windows_interop()
    if id_count <= 1000:
        raise LauncherError(
            "Rootless Podman needs a subordinate ID range (/etc/subuid, /etc/subgid) of at least 1000 IDs."
        )
    return Engine(architecture, network, id_count)


def _check_selinux_mode() -> None:
    try:
        mode = SELINUX_ENFORCE.read_text().strip()
    except OSError:
        mode = ""
    if mode == "1":
        raise LauncherError(
            "SELinux is enforcing on this host, which the sandbox doesn't support yet: "
            "it stops the containers from reading your repository's .git and the "
            "instance's exchange folders."
        )
    if mode != "0":
        print(
            f"Warning: Podman reports SELinux is enabled, but {SELINUX_ENFORCE} doesn't show its mode. "
            "If SELinux is enforcing, the sandbox can't read your repository or exchange folders."
        )


_CHECK_WINDOWS_INTEROP = r"""
import configparser
from pathlib import Path

configuration = configparser.ConfigParser(interpolation=None)
with Path("/etc/wsl.conf").open() as stream:
    configuration.read_file(stream)
configured_off = all(
    configuration.get("interop", option, fallback="").strip() == "false"
    for option in ("enabled", "appendWindowsPath")
)
print("interop-configured-off" if configured_off else "interop-unverified")
"""


def _check_windows_interop() -> None:
    # WSL reads wsl.conf at distro startup. Later edits can make this check pass
    # while interop is still enabled, exposing Windows execution after a container escape.
    print("Checking Windows interop configuration...", flush=True)
    result = run(
        [
            "wsl",
            "--distribution",
            f"podman-{MACHINE}",
            "--exec",
            "python3",
            "-I",
            "-c",
            _CHECK_WINDOWS_INTEROP,
        ],
        check=False,
    )
    if result.returncode:
        raise LauncherError(
            f"Cannot read /etc/wsl.conf in WSL distribution podman-{MACHINE} "
            f"(exit {result.returncode}).\n"
            + sanitize_diagnostic(result.stderr or result.stdout)
        )
    if result.stdout.strip() != "interop-configured-off":
        raise LauncherError(
            f"Cannot confirm Windows interop is configured off for {MACHINE}. "
            "Both [interop] settings in /etc/wsl.conf must be false. "
            "Run 'sandbox.py setup-machine'."
        )
    print("Windows interop is configured off.", flush=True)


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
            # Podman's 2 GiB default is too little to build the QDK image; WSL sizes its own memory.
            command += ["--memory", "8192"]
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
