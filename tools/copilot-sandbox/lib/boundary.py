"""Host-controlled checks of the actual Podman isolation boundary."""

from __future__ import annotations

from collections.abc import Iterable
import json
import posixpath
import re

from .common import LauncherError, run
from .layout import (
    AGENT_PIDS_LIMIT,
    AGENT_TMPFS,
    AGENT_USER,
    PROXY_COMMAND,
    PROXY_PIDS_LIMIT,
    PROXY_TMPFS,
    PROXY_USER,
    RELAY_COMMAND,
    Mount,
    Session,
)
from .podman import host_filesystems, normalize_image_id, podman

_AGENT_PROBE = r"""
import errno
import os
from pathlib import Path
import socket
import stat
import sys
import time
import uuid


def fail(message):
    raise SystemExit("probe: " + message)


# Confirm the kernel enforces the expected PID limit.
if Path("/sys/fs/cgroup/pids.max").read_text().strip() != sys.argv[1]:
    fail("PID limit is not enforced")

# Confirm the agent has only loopback, so the relay socket is its only way out.
if sorted(os.listdir("/sys/class/net")) != ["lo"]:
    fail("network interfaces other than loopback exist")
if Path("/proc/net/route").read_text().splitlines()[1:]:
    fail("network routes exist")
if not stat.S_ISSOCK(Path("/run/qdk-relay/proxy.sock").lstat().st_mode):
    fail("relay socket is missing")

# Confirm the relay and input mounts reject writes by attempting exclusive file creation.
for directory in ("/run/qdk-relay", "/seed.git", "/exchange/to_copilot"):
    path = Path(directory) / (".qdk-boundary-" + uuid.uuid4().hex)
    try:
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except OSError as error:
        if error.errno not in (errno.EROFS, errno.EACCES):
            raise
    else:
        os.close(descriptor)
        path.unlink()
        fail("protected mount is writable: " + directory)

# Confirm direct TCP connections to the sampled internet and metadata endpoints fail.
for address in (("1.1.1.1", 443), ("169.254.169.254", 80)):
    try:
        connection = socket.create_connection(address, timeout=1)
    except OSError:
        pass
    else:
        connection.close()
        fail("direct egress is available")

# Confirm the seccomp profile blocks hypervisor sockets and new user namespaces.
try:
    socket.socket(40, socket.SOCK_STREAM).close()  # AF_VSOCK
except OSError as error:
    if error.errno != errno.EPERM:
        fail("vsock sockets are not blocked by seccomp")
else:
    fail("vsock sockets are permitted")
child = os.fork()
if child == 0:
    try:
        os.unshare(os.CLONE_NEWUSER)
    except PermissionError:
        os._exit(0)
    os._exit(1)
if os.waitpid(child, 0)[1] != 0:
    fail("the agent can create user namespaces")


def ask_proxy(request):
    # Retry briefly while the relay and Squid start.
    for attempt in range(50):
        try:
            connection = socket.create_connection(("127.0.0.1", 3128), timeout=1)
            break
        except OSError:
            if attempt == 49:
                fail("relay is unavailable")
            time.sleep(0.1)
    with connection:
        connection.settimeout(5)
        connection.sendall(request.encode("ascii"))
        try:
            return connection.recv(4096)
        except ConnectionResetError:
            return b""
        except socket.timeout:
            # Squid refuses before connecting, so a hang means it allowed the request.
            fail("proxy did not answer " + request.split(" ", 2)[1])


# Confirm Squid itself answers through the relay, so a dead relay can't pass as a rejection below.
for attempt in range(50):
    response = ask_proxy("GET http://qdk-boundary.invalid/ HTTP/1.1\r\nHost: qdk-boundary.invalid\r\n\r\n")
    if response:
        break
    time.sleep(0.1)
head = response.split(b"\r\n\r\n", 1)[0].lower()
if not head.startswith(b"http/1.1 403 ") or b"\r\nserver: squid" not in head:
    fail("Squid did not answer through the relay")

# Confirm the proxy rejects forbidden names and addresses with a 403 or closure, including
# allowlisted probe names that resolve to loopback and to your machine.
for destination in (
    "qdk-boundary-" + uuid.uuid4().hex + ".invalid",
    "127.0.0.1",
    "169.254.169.254",
    "loopback.qdk-boundary.test",
    "host.qdk-boundary.test",
):
    response = ask_proxy("CONNECT " + destination + ":443 HTTP/1.1\r\nHost: " + destination + ":443\r\n\r\n")
    if response and b" 403 " not in response.split(b"\r\n", 1)[0]:
        fail("proxy did not reject " + destination)

# Confirm no PAT has been delivered to this unverified container.
if os.path.lexists("/tmp/qdk-sandbox"):
    fail("token directory exists before verification")

# Return kernel process restrictions for independent host-side validation.
print(Path("/proc/self/status").read_text())
"""


# Rootless Podman omits devices from inspect output, so list the device nodes inside each container.
_LIST_DEVICES = (
    "shopt -s globstar nullglob dotglob; for path in /dev/**; do "
    'if [[ ! -L $path && ( -c $path || -b $path ) ]]; then echo "${path#/dev/}"; fi; done'
)
_STANDARD_DEVICES = {
    "full",
    "null",
    "ptmx",
    "pts/ptmx",
    "random",
    "tty",
    "urandom",
    "zero",
}
# Mounts every container gets, with the filesystem each must be.
_STANDARD_MOUNTS = {
    "/dev": "tmpfs",
    "/dev/mqueue": "mqueue",
    "/dev/pts": "devpts",
    "/dev/shm": "tmpfs",
    "/proc": "proc",
    "/sys": "sysfs",
    "/sys/fs/cgroup": "cgroup2",
}
# Files Podman generates in the container's own userdata directory.
_CONTAINER_FILES = {
    "/etc/hostname": "hostname",
    "/etc/hosts": "hosts",
    "/etc/resolv.conf": "resolv.conf",
    "/run/.containerenv": ".containerenv",
}
_INIT_BINARIES = {"catatonit", "podman-init", "tini"}
# Podman creates these filesystems fresh for each container, so none may be one the host has mounted.
_CONTAINER_FILESYSTEMS = {"devpts", "mqueue", "proc", "sysfs", "tmpfs"}


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise LauncherError("Sandbox boundary verification failed: " + message)


def _as_list(value: Iterable[object] | None) -> list:
    # Podman 4 reports a single-element Entrypoint as a string.
    if value is None:
        return []
    if isinstance(value, str):
        return [value]
    return list(value)


def _verify_mounts(container: dict, role: str, expected: list[Mount]) -> None:
    # Exact set: Podman configuration must not have added mounts or loosened any.
    def key(kind: str, source: str) -> str:
        return source if kind == "volume" else posixpath.normpath(source)

    wanted = {
        mount.target: (mount.kind, key(mount.kind, mount.source), not mount.readonly)
        for mount in expected
    }
    actual = {}
    for mount in container["Mounts"]:
        destination = mount["Destination"]
        _require(destination not in actual, f"{role} has duplicate mounts")
        source = mount["Name"] if mount["Type"] == "volume" else mount["Source"]
        actual[destination] = (mount["Type"], key(mount["Type"], source), mount["RW"])
    _require(actual == wanted, f"{role} mounts differ from the expected set")


def verify_container_config(container: dict, role: str, session: Session) -> None:
    try:
        config = container["Config"]
        host = container["HostConfig"]
        state = container["State"]
        agent = role == "agent"
        # Confirm the container is running with the selected image and runtime identity.
        _require(state["Running"] and not state["Paused"], f"{role} is not running")
        _require(
            normalize_image_id(container["Image"])
            == normalize_image_id(
                session.agent_image if agent else session.proxy_image
            ),
            f"{role} image differs from the selected immutable image",
        )
        _require(
            config["User"] == (AGENT_USER if agent else PROXY_USER),
            f"{role} has an unexpected user",
        )
        _require(
            host["PidsLimit"] == (AGENT_PIDS_LIMIT if agent else PROXY_PIDS_LIMIT),
            f"{role} has an unexpected PID limit",
        )
        # Confirm no host proxy settings, which can hold credentials, were passed through.
        actual = {
            key: value
            for key, _, value in (entry.partition("=") for entry in config["Env"])
            if key.lower().endswith("_proxy")
        }
        expected = {
            key: value
            for key, value in (session.environment.items() if agent else ())
            if key.lower().endswith("_proxy")
        }
        _require(actual == expected, f"{role} has unexpected proxy settings")
        # Confirm privileged mode is disabled and no capabilities or devices are added.
        _require(not host["Privileged"], f"{role} is privileged")
        _require(not host.get("CapAdd"), f"{role} has added capabilities")
        # Confirm no-new-privileges and the bundled seccomp profile are configured.
        options = host.get("SecurityOpt") or []
        _require("no-new-privileges" in options, f"{role} permits privilege escalation")
        _require(
            session.seccomp in options,
            f"{role} does not use the bundled seccomp profile",
        )
        _require(
            not {"seccomp=unconfined", "label=disable", "apparmor=unconfined"}
            & set(options),
            f"{role} disables a security module",
        )
        # Confirm the container has its own process, user, hostname and IPC namespaces.
        for key, allowed in (
            ("PidMode", {"private"}),
            ("UsernsMode", {"private"}),
            ("UTSMode", {"private"}),
            ("IpcMode", {"private", "shareable"}),
        ):
            _require(host.get(key) in allowed, f"{role} shares a namespace ({key})")
        # Confirm no container ports are published on the host.
        _require(not host.get("PublishAllPorts"), f"{role} publishes ports")
        _require(not host.get("PortBindings"), f"{role} publishes ports")
        # Confirm the exact command, under an init process.
        _require(host.get("Init") is True, f"{role} has no init process")
        command = _as_list(config["Entrypoint"]) + _as_list(config["Cmd"])
        _require(
            command == list(RELAY_COMMAND if agent else PROXY_COMMAND),
            f"{role} runs an unexpected command",
        )
        # Confirm temporary storage is exactly the expected set and disables setuid, devices and execution.
        tmpfs = host.get("Tmpfs") or {}
        _require(
            set(tmpfs) == set(AGENT_TMPFS if agent else PROXY_TMPFS),
            f"{role} has unexpected tmpfs mounts",
        )
        for value in tmpfs.values():
            _require(
                {"nosuid", "nodev", "noexec"} <= set(value.split(",")),
                f"{role} temporary storage is not hardened",
            )
        _verify_mounts(
            container, role, session.agent_mounts() if agent else session.proxy_mounts()
        )
        # Confirm the agent has no network and the proxy has only its own rootless egress.
        networks = set(container["NetworkSettings"].get("Networks") or {})
        if agent:
            _require(host["NetworkMode"] == "none", "agent has a network")
            _require(networks <= {"none"}, "agent has network attachments")
        else:
            _require(
                host["NetworkMode"] == session.engine.network,
                "proxy network mode differs",
            )
            _require(not networks, "proxy is attached to a shared network")
            _require(host["ReadonlyRootfs"], "proxy root filesystem is writable")
    except (KeyError, TypeError, AttributeError, ValueError) as error:
        raise LauncherError(
            "Cannot verify Podman container security metadata."
        ) from error


def verify_process_status(status: str) -> None:
    try:
        fields = dict(line.split(":", 1) for line in status.splitlines() if ":" in line)
        # Confirm real, effective, saved-set, and filesystem UIDs are all nonroot.
        _require(
            all(int(value) > 0 for value in fields["Uid"].split()),
            "runtime user is root",
        )
        # Confirm the kernel applied no-new-privileges and seccomp filter mode.
        _require(
            int(fields["NoNewPrivs"]) == 1,
            "kernel privilege restriction is inactive",
        )
        _require(int(fields["Seccomp"]) == 2, "kernel seccomp filtering is inactive")
        # Confirm every capability set, including bounding and ambient, is empty.
        for field in ("CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"):
            _require(int(fields[field], 16) == 0, "runtime retains Linux capabilities")
    except (KeyError, ValueError) as error:
        raise LauncherError("Cannot verify kernel process restrictions.") from error


def _unescape(field: str) -> str:
    return re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), field)


def verify_mount_table(
    mountinfo: str,
    role: str,
    container: str,
    expected: dict[str, str | None],
    host_devices: set[str],
) -> None:
    # Inspect omits mounts that Podman configuration adds remotely, so check the kernel's view.
    # Beyond the expected mounts, accept only the shapes Podman itself creates.
    entries = []
    for line in mountinfo.splitlines():
        fields, filesystem = line.split(" - ", 1)
        fields = fields.split()
        entries.append(
            (
                _unescape(fields[4]),
                fields[2],
                _unescape(fields[3]),
                filesystem.split()[0],
                fields[5].split(","),
            )
        )
    targets = [entry[0] for entry in entries]
    _require(len(targets) == len(set(targets)), f"{role} has stacked mounts")
    devices = {target: device for target, device, *_ in entries}
    for target, device, root, filesystem, options in entries:
        prefix = "/proc" if target.startswith("/proc/") else "/sys"
        if filesystem in _CONTAINER_FILESYSTEMS and target not in _CONTAINER_FILES:
            _require(
                device not in host_devices,
                f"{role} shares a host filesystem at {target}",
            )
        if target in expected:
            allowed = expected[target] in (None, filesystem)
        elif target == "/":
            allowed = True
        elif target in _STANDARD_MOUNTS:
            allowed = filesystem == _STANDARD_MOUNTS[target] and root == "/"
        elif target in _CONTAINER_FILES:
            allowed = root.endswith(f"/{container}/userdata/{_CONTAINER_FILES[target]}")
        elif target == "/run/podman-init":
            allowed = posixpath.basename(root) in _INIT_BINARIES
        elif target.startswith("/dev/"):
            name = target.removeprefix("/dev/")
            allowed = (
                filesystem == "devtmpfs"
                and name in _STANDARD_DEVICES
                and root == "/" + name
            )
        elif target.startswith(("/proc/", "/sys/")):
            allowed = (
                # /dev/null over a masked file, an empty read-only tmpfs over a masked directory,
                # or a read-only rebind of the container's own /proc or /sys.
                (filesystem == "devtmpfs" and root == "/null")
                or (filesystem == "tmpfs" and root == "/" and "ro" in options)
                or (
                    filesystem in ("proc", "sysfs")
                    and device == devices.get(prefix)
                    and root == target.removeprefix(prefix)
                )
            )
        else:
            allowed = False
        _require(allowed, f"{role} has an unexpected mount at {target}")


def verify_devices(listing: str, role: str) -> None:
    _require(set(listing.split()) <= _STANDARD_DEVICES, f"{role} exposes host devices")


def verify_proxy_listeners(tables: str) -> None:
    # Confirm Squid listens only on IPv4 loopback port 3128, so nothing on a network can reach it.
    listeners = {
        fields[1]
        for fields in map(str.split, tables.splitlines())
        if len(fields) > 3 and fields[3] == "0A"
    }
    _require(listeners == {"0100007F:0C38"}, "proxy listens beyond 127.0.0.1:3128")


def _inspect(name: str) -> dict:
    try:
        data = json.loads(run(podman("container", "inspect", name)).stdout)
        if (
            not isinstance(data, list)
            or len(data) != 1
            or not isinstance(data[0], dict)
        ):
            raise ValueError("Expected one inspected container")
        return data[0]
    except (ValueError, TypeError) as error:
        raise LauncherError("Cannot read Podman boundary metadata.") from error


def verify_boundary(session: Session) -> str:
    # Confirm Podman's container configuration before running live probes.
    print("Checking sandbox container configurations...", flush=True)
    agent = _inspect(session.agent)
    verify_container_config(agent, "agent", session)
    proxy = _inspect(session.proxy)
    verify_container_config(proxy, "proxy", session)
    print("Reading host filesystem mounts...", flush=True)
    host_devices = host_filesystems()
    for role, container, mounts, tmpfs in (
        ("agent", agent["Id"], session.agent_mounts(), AGENT_TMPFS),
        ("proxy", proxy["Id"], session.proxy_mounts(), PROXY_TMPFS),
    ):
        print(f"Checking {role} filesystem mounts and devices...", flush=True)
        verify_mount_table(
            run(
                podman("exec", container, "/usr/bin/cat", "/proc/self/mountinfo")
            ).stdout,
            role,
            container,
            {mount.target: None for mount in mounts} | dict.fromkeys(tmpfs, "tmpfs"),
            host_devices,
        )
        verify_devices(
            run(podman("exec", container, "/bin/bash", "-c", _LIST_DEVICES)).stdout,
            role,
        )
    # Confirm kernel restrictions, loopback-only listening and no IPv6 in the proxy using fresh processes.
    print("Checking proxy process restrictions, listeners and IPv6...", flush=True)
    verify_process_status(
        run(podman("exec", session.proxy, "/usr/bin/cat", "/proc/self/status")).stdout
    )
    verify_proxy_listeners(
        run(
            podman(
                "exec",
                session.proxy,
                "/usr/bin/cat",
                "/proc/net/tcp",
                "/proc/net/tcp6",
            )
        ).stdout
    )
    _require(
        not run(
            podman("exec", session.proxy, "/usr/bin/cat", "/proc/net/if_inet6")
        ).stdout.strip(),
        "proxy has IPv6 addresses",
    )
    # Confirm the agent's restrictions using a host-supplied probe in isolated Python.
    print("Checking agent isolation and proxy access restrictions...", flush=True)
    result = run(
        podman(
            "exec",
            "-i",
            "--user",
            AGENT_USER,
            agent["Id"],
            "/usr/bin/python3",
            "-I",
            "-",
            str(AGENT_PIDS_LIMIT),
        ),
        input_text=_AGENT_PROBE,
        check=False,
    )
    lines = result.stderr.strip().splitlines()
    _require(result.returncode == 0, lines[-1] if lines else "agent probe failed")
    verify_process_status(result.stdout)
    return agent["Id"]
