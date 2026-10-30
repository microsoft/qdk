"""Host-controlled checks of the actual Docker isolation boundary."""

from __future__ import annotations

import json
from decimal import Decimal

from .common import LauncherError, run

_AGENT_PROBE = r"""
import errno
import os
from pathlib import Path
import socket
import stat
import time
import uuid

# Confirm the expected kernel PID limit is set in cgroup v2 or v1.
pid_limits = (Path("/sys/fs/cgroup/pids.max"), Path("/sys/fs/cgroup/pids/pids.max"))
pid_limit = next((path.read_text().strip() for path in pid_limits if path.is_file()), None)
if pid_limit != "4096":
    raise RuntimeError("Kernel PID limit differs")

# Confirm the token is a root-owned, read-only regular file rather than a symlink.
secret = Path("/run/secrets/gh_token").lstat()
if not stat.S_ISREG(secret.st_mode) or secret.st_uid != 0 or stat.S_IMODE(secret.st_mode) != 0o444:
    raise RuntimeError("Secret file permissions differ")

# Confirm input mounts reject writes by attempting exclusive file creation.
for directory in ("/seed.git", "/exchange/to_copilot"):
    path = Path(directory) / (".qdk-boundary-" + uuid.uuid4().hex)
    try:
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except OSError as error:
        if error.errno not in (errno.EROFS, errno.EACCES):
            raise
    else:
        os.close(descriptor)
        path.unlink()
        raise RuntimeError("Protected mount is writable: " + directory)

# Confirm direct TCP connections to the sampled internet and metadata endpoints fail.
for address in (("1.1.1.1", 443), ("169.254.169.254", 80)):
    try:
        connection = socket.create_connection(address, timeout=1)
    except OSError:
        pass
    else:
        connection.close()
        raise RuntimeError("Direct egress is available")

# Confirm the proxy rejects forbidden hostname, loopback, and metadata tunnels with a 403 or closure.
for destination in ("qdk-boundary-" + uuid.uuid4().hex + ".invalid", "127.0.0.1", "169.254.169.254"):
    # Retry briefly while the proxy starts before testing its response.
    for attempt in range(20):
        try:
            connection = socket.create_connection(("proxy", 3128), timeout=1)
            break
        except OSError:
            if attempt == 19:
                raise RuntimeError("Proxy is unavailable")
            time.sleep(0.1)
    with connection:
        connection.settimeout(3)
        connection.sendall(("CONNECT " + destination + ":443 HTTP/1.1\r\nHost: " + destination + ":443\r\n\r\n").encode("ascii"))
        try:
            response = connection.recv(4096)
        except ConnectionResetError:
            response = b""
        if response and b" 403 " not in response.split(b"\r\n", 1)[0]:
            raise RuntimeError("Proxy did not reject a forbidden destination")

# Return kernel process restrictions for independent host-side validation.
print(Path("/proc/self/status").read_text())
"""


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise LauncherError("Sandbox boundary verification failed: " + message)


def verify_container_config(
    container: dict, role: str, project: str, environment: dict[str, str]
) -> None:
    try:
        config = container["Config"]
        host = container["HostConfig"]
        state = container["State"]
        # Confirm the container is running with the selected image and runtime identity.
        _require(state["Running"] and not state["Paused"], f"{role} is not running")
        _require(
            container["Image"] == environment[f"QDK_SANDBOX_{role.upper()}_IMAGE"],
            f"{role} image differs from the selected immutable image",
        )
        _require(
            config["User"] == ("1000:1000" if role == "agent" else "squid"),
            f"{role} has an unexpected user",
        )
        # Confirm privileged mode is disabled and all Linux capabilities are dropped.
        _require(not host["Privileged"], f"{role} is privileged")
        _require(not host.get("CapAdd"), f"{role} has added capabilities")
        _require("ALL" in host["CapDrop"], f"{role} does not drop all capabilities")
        # Confirm no-new-privileges is configured and seccomp is not explicitly unconfined.
        options = host["SecurityOpt"] or []
        _require(
            any(
                item in options
                for item in ("no-new-privileges", "no-new-privileges:true")
            ),
            f"{role} permits privilege escalation",
        )
        _require("seccomp=unconfined" not in options, f"{role} disables seccomp")
        # Confirm sensitive kernel files are masked and kernel settings are read-only.
        _require(
            {"/proc/kcore", "/proc/keys"} <= set(host["MaskedPaths"]),
            f"{role} exposes protected kernel files",
        )
        _require(
            "/proc/sys" in host["ReadonlyPaths"],
            f"{role} exposes writable kernel settings",
        )
        # Confirm no host devices are explicitly exposed and PID/IPC namespaces are private.
        _require(not host.get("Devices"), f"{role} exposes host devices")
        _require(not host.get("DeviceRequests"), f"{role} requests host devices")
        _require(not host.get("PidMode"), f"{role} shares a PID namespace")
        _require(host.get("IpcMode") in ("", "private"), f"{role} shares IPC")
        # Confirm no container ports are published on the host.
        _require(not host.get("PublishAllPorts"), f"{role} publishes ports")
        _require(not host.get("PortBindings"), f"{role} publishes ports")
        if role == "proxy":
            # Confirm the proxy's root filesystem is read-only.
            _require(host["ReadonlyRootfs"], "proxy root filesystem is writable")
        else:
            # Confirm Docker's PID and CPU limits match the expected resource policy.
            _require(host["PidsLimit"] == 4096, "agent PID limit is not enforced")
            _require(
                host["NanoCpus"]
                == int(Decimal(environment.get("QDK_SANDBOX_CPUS", "0")) * 10**9),
                "agent CPU limit differs",
            )
            # Confirm the memory limit matches after converting binary size suffixes to bytes.
            memory = (
                environment.get("QDK_SANDBOX_MEMORY", "0").lower().removesuffix("b")
            )
            if memory and memory[-1] in "kmg":
                expected_memory = int(memory[:-1]) * 1024 ** (
                    "kmg".index(memory[-1]) + 1
                )
            else:
                expected_memory = int(memory)
            _require(host["Memory"] == expected_memory, "agent memory limit differs")
            # Confirm DNS is configured for loopback only and IPv6 is disabled.
            _require(host["Dns"] == ["127.0.0.1"], "agent has external DNS")
            for key in ("all", "default"):
                _require(
                    host["Sysctls"][f"net.ipv6.conf.{key}.disable_ipv6"] == "1",
                    "agent IPv6 is enabled",
                )

        # Confirm /tmp is the only tmpfs and disables setuid, devices, and direct execution.
        tmpfs = host["Tmpfs"]
        _require(set(tmpfs) == {"/tmp"}, f"{role} has unexpected tmpfs mounts")
        _require(
            {"nosuid", "nodev", "noexec"} <= set(tmpfs["/tmp"].split(",")),
            f"{role} temporary storage is not hardened",
        )
        # Confirm mount destinations, types, and write permissions match the allowlist.
        expected = (
            {
                "/work": ("volume", True),
                "/home/agent/.copilot": ("volume", True),
                "/seed.git": ("bind", False),
                "/outbox.git": ("bind", True),
                "/exchange/to_copilot": ("bind", False),
                "/exchange/from_copilot": ("bind", True),
            }
            if role == "agent"
            else {"/etc/squid/allowlist.local.txt": ("bind", False)}
        )
        actual = {}
        for mount in container["Mounts"]:
            destination = mount["Destination"]
            _require(destination not in actual, f"{role} has duplicate mounts")
            if mount["Type"] == "tmpfs" and destination in tmpfs:
                continue
            actual[destination] = (mount["Type"], mount["RW"])
        if role == "agent" and "/run/secrets/gh_token" in actual:
            expected["/run/secrets/gh_token"] = ("bind", False)
        _require(actual == expected, f"{role} mount types or permissions differ")

        # Confirm only the proxy is attached to the external egress network.
        expected_networks = {project + "_internal"}
        if role == "proxy":
            expected_networks.add(project + "_egress")
        _require(
            set(container["NetworkSettings"]["Networks"]) == expected_networks,
            f"{role} has unexpected network attachments",
        )
        # Confirm the container carries the expected sandbox owner label.
        _require(
            config["Labels"]["io.qdk.copilot-sandbox.owner"]
            == environment["QDK_SANDBOX_OWNER"],
            f"{role} owner differs",
        )
        if role == "agent":
            # Confirm the agent is configured to remain idle until verification completes.
            _require(
                config["Entrypoint"] == ["/usr/bin/sleep"]
                and config["Cmd"] == ["infinity"],
                "agent workload started before boundary verification",
            )
    except (KeyError, TypeError, AttributeError) as error:
        raise LauncherError(
            "Cannot verify Docker container security metadata."
        ) from error


def verify_network_config(network: dict, name: str, *, internal: bool) -> None:
    try:
        # Confirm the configured network is the expected IPv4-only bridge.
        _require(network["Name"] == name, "network name differs")
        _require(network["Driver"] == "bridge", "network is not a bridge")
        _require(network["Internal"] == internal, "network isolation differs")
        _require(not network["EnableIPv6"], "network IPv6 is enabled")
        if internal:
            # Confirm the internal bridge has no host-gateway connection.
            _require(
                network["Options"]["com.docker.network.bridge.gateway_mode_ipv4"]
                == "isolated",
                "internal bridge can reach the host gateway",
            )
    except (KeyError, TypeError) as error:
        raise LauncherError(
            "Cannot verify Docker network security metadata."
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


def _inspect(kind: str, name: str) -> dict:
    try:
        data = json.loads(run(["docker", kind, "inspect", name]).stdout)
        if (
            not isinstance(data, list)
            or len(data) != 1
            or not isinstance(data[0], dict)
        ):
            raise ValueError("Expected one inspected Docker object")
        return data[0]
    except (ValueError, TypeError) as error:
        raise LauncherError("Cannot read Docker boundary metadata.") from error


def verify_boundary(
    agent: str, proxy: str, project: str, environment: dict[str, str]
) -> str:
    # Confirm Docker's container and network configuration before running live probes.
    agent_config = _inspect("container", agent)
    verify_container_config(agent_config, "agent", project, environment)
    verify_container_config(_inspect("container", proxy), "proxy", project, environment)
    for suffix, internal in (("internal", True), ("egress", False)):
        name = project + "_" + suffix
        verify_network_config(_inspect("network", name), name, internal=internal)
    # Confirm kernel restrictions using a fresh process under the proxy's configured user.
    verify_process_status(
        run(["docker", "exec", proxy, "/usr/bin/cat", "/proc/self/status"]).stdout
    )
    # Confirm the agent's restrictions using a host-supplied probe in isolated Python.
    result = run(
        [
            "docker",
            "exec",
            "-i",
            "--user",
            "1000:1000",
            agent,
            "/usr/bin/python3",
            "-I",
            "-",
        ],
        input_text=_AGENT_PROBE,
    )
    verify_process_status(result.stdout)
    return agent_config["Id"]
