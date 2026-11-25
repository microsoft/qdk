"""The sandbox's containers and mounts, used both to launch and to verify them."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Sequence

from .podman import SECCOMP_PROFILE, Engine, vm_path
from .state import validate_sandbox_instance_name

LABEL_PREFIX = "io.qdk.copilot-sandbox."
AGENT_USER = "1000:1000"
# Volumes the agent writes for the host. The host never mounts them; helpers copy them out instead.
OUTBOX_DIRECTORY = "/outbox.git"
OUTPUT_DIRECTORY = "/exchange/from_copilot"
PROXY_USER = "squid"
AGENT_PIDS_LIMIT = 4096
PROXY_PIDS_LIMIT = 512
PROXY_URL = "http://127.0.0.1:3128"
RELAY_DIRECTORY = "/run/qdk-relay"
# The agent's main process: its only way out is the proxy's socket on the read-only relay volume.
RELAY_COMMAND = (
    "/usr/bin/socat",
    "TCP-LISTEN:3128,bind=127.0.0.1,fork,reuseaddr",
    f"UNIX-CONNECT:{RELAY_DIRECTORY}/proxy.sock",
)
PROXY_COMMAND = ("/usr/local/bin/proxy-entrypoint",)
AGENT_TMPFS = ("/tmp",)
# Squid needs writable shared memory; --read-only-tmpfs=false would otherwise leave /dev/shm read-only.
PROXY_TMPFS = ("/tmp", "/dev/shm")
# Allowlisted probe names (see allowlist.txt) that resolve to loopback and to your machine.
# Squid must refuse both, which shows that an allowlisted name can't reach host services.
PROBE_HOSTS = (
    "loopback.qdk-boundary.test:127.0.0.1",
    "host.qdk-boundary.test:host-gateway",
)
# Without these, the network helper fails to configure IPv6 once the sysctls disable it.
_IPV4_ONLY_NETWORKS = {
    "pasta": "pasta:--ipv4-only",
    "slirp4netns": "slirp4netns:enable_ipv6=false",
}


@dataclass(frozen=True)
class Mount:
    kind: str
    source: str
    target: str
    readonly: bool

    def argument(self) -> list[str]:
        # --mount is comma separated with no escaping.
        if "," in self.source:
            raise ValueError(f"Mount paths must not contain commas: {self.source}")
        value = f"type={self.kind},source={self.source},destination={self.target}"
        return ["--mount", value + (",ro=true" if self.readonly else "")]


def volume_name(instance: str, kind: str) -> str:
    return f"qdk-copilot-sandbox-{validate_sandbox_instance_name(instance)}_{kind}"


def helper_arguments(
    image: str,
    instance: str,
    mounts: Sequence[Mount],
    *command: str,
    interactive: bool = False,
) -> list[str]:
    """Podman arguments for a throwaway, offline container running as container root.

    With Podman's default IDs, container root is you, so files it writes to the host are yours.
    """
    arguments = [
        "run",
        "--rm",
        "--pull=never",
        "--network=none",
        "--user",
        "0:0",
        "--cap-drop=ALL",
        "--cap-add=CHOWN",
        "--cap-add=DAC_OVERRIDE",
        "--cap-add=DAC_READ_SEARCH",
        "--cap-add=FOWNER",
        "--security-opt=no-new-privileges",
        # Helpers read agent-written data as you, so they get at least the agent's restrictions.
        "--security-opt=seccomp=" + vm_path(SECCOMP_PROFILE),
        "--read-only",
        f"--pids-limit={PROXY_PIDS_LIMIT}",
        "--label",
        f"{LABEL_PREFIX}instance={validate_sandbox_instance_name(instance)}",
        "--label",
        f"{LABEL_PREFIX}role=helper",
    ]
    if interactive:
        arguments.insert(1, "-i")
    for mount in mounts:
        arguments += mount.argument()
    return arguments + ["--entrypoint", command[0], image, *command[1:]]


@dataclass(frozen=True)
class Session:
    instance: str
    engine: Engine
    agent_image: str
    proxy_image: str
    seed: Path
    to_copilot: Path
    allowlist: Path
    cpus: str
    memory: str
    environment: dict[str, str]

    def __post_init__(self) -> None:
        validate_sandbox_instance_name(self.instance)

    @property
    def prefix(self) -> str:
        return "qdk-copilot-sandbox-" + self.instance

    @property
    def agent(self) -> str:
        return self.prefix + "-agent"

    @property
    def proxy(self) -> str:
        return self.prefix + "-proxy"

    def volume(self, kind: str) -> str:
        return volume_name(self.instance, kind)

    @property
    def seccomp(self) -> str:
        # The VM reads the profile, so pass the path it sees.
        return "seccomp=" + vm_path(SECCOMP_PROFILE)

    def agent_mounts(self) -> list[Mount]:
        return [
            Mount("volume", self.volume("work"), "/work", False),
            Mount("volume", self.volume("copilot"), "/home/agent/.copilot", False),
            Mount("volume", self.volume("relay"), RELAY_DIRECTORY, True),
            Mount("volume", self.volume("outbox"), OUTBOX_DIRECTORY, False),
            Mount("volume", self.volume("output"), OUTPUT_DIRECTORY, False),
            Mount("bind", vm_path(self.seed), "/seed.git", True),
            Mount("bind", vm_path(self.to_copilot), "/exchange/to_copilot", True),
        ]

    def proxy_mounts(self) -> list[Mount]:
        return [
            Mount("volume", self.volume("relay"), RELAY_DIRECTORY, False),
            Mount(
                "bind", vm_path(self.allowlist), "/etc/squid/allowlist.local.txt", True
            ),
        ]

    def _common_arguments(self, role: str, tmpfs: tuple[str, ...]) -> list[str]:
        arguments = [
            "--pull=never",
            "--init",
            # Otherwise Podman copies host proxy variables, such as ftp_proxy, which can hold credentials.
            "--http-proxy=false",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--security-opt=" + self.seccomp,
            "--label",
            f"{LABEL_PREFIX}instance={self.instance}",
            "--label",
            f"{LABEL_PREFIX}role={role}",
        ]
        for path in tmpfs:
            arguments.append(f"--tmpfs={path}:rw,nosuid,nodev,noexec,mode=1777")
        return arguments

    def agent_arguments(self) -> list[str]:
        arguments = [
            *self._common_arguments("agent", AGENT_TMPFS),
            "--user",
            AGENT_USER,
            "--workdir",
            "/work",
            # Podman's default IDs, so the agent is a subordinate ID rather than you on the host.
            # Unlike omitting --userns, this gives the container its own user namespace, and unlike
            # other mappings, Podman needn't make a copy of the image with shifted owners.
            f"--uidmap=0:0:{self.engine.id_count}",
            f"--gidmap=0:0:{self.engine.id_count}",
            "--network=none",
            f"--pids-limit={AGENT_PIDS_LIMIT}",
        ]
        if self.cpus != "0":
            arguments.append("--cpus=" + self.cpus)
        if self.memory != "0":
            arguments.append("--memory=" + self.memory)
        for mount in self.agent_mounts():
            arguments += mount.argument()
        for key, value in self.environment.items():
            arguments += ["--env", f"{key}={value}"]
        arguments += ["--entrypoint", RELAY_COMMAND[0], self.agent_image]
        return arguments + list(RELAY_COMMAND[1:])

    def proxy_arguments(self) -> list[str]:
        arguments = [
            *self._common_arguments("proxy", PROXY_TMPFS),
            "--user",
            PROXY_USER,
            # Nothing in the proxy maps to your user.
            "--userns=nomap",
            "--read-only",
            "--read-only-tmpfs=false",
            # IPv4 only: LAN devices often have global IPv6 addresses, which Squid's private-address check allows.
            "--network=" + _IPV4_ONLY_NETWORKS[self.engine.network],
            "--sysctl=net.ipv6.conf.all.disable_ipv6=1",
            "--sysctl=net.ipv6.conf.default.disable_ipv6=1",
            f"--pids-limit={PROXY_PIDS_LIMIT}",
        ]
        for host in PROBE_HOSTS:
            arguments.append("--add-host=" + host)
        for mount in self.proxy_mounts():
            arguments += mount.argument()
        return arguments + [self.proxy_image]
