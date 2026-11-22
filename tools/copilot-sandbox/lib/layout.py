"""The sandbox's containers and mounts, used both to launch and to verify them."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .podman import SECCOMP_PROFILE, Engine, vm_path
from .state import validate_sandbox_instance_name

LABEL_PREFIX = "io.qdk.copilot-sandbox."
AGENT_USER = "1000:1000"
PROXY_USER = "squid"
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


@dataclass(frozen=True)
class Session:
    instance: str
    engine: Engine
    agent_image: str
    proxy_image: str
    seed: Path
    outbox: Path
    to_copilot: Path
    from_copilot: Path
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
        return f"{self.prefix}_{kind}"

    @property
    def seccomp(self) -> str:
        # The VM reads the profile, so pass the path it sees.
        return "seccomp=" + vm_path(SECCOMP_PROFILE)

    def agent_mounts(self) -> list[Mount]:
        return [
            Mount("volume", self.volume("work"), "/work", False),
            Mount("volume", self.volume("copilot"), "/home/agent/.copilot", False),
            Mount("volume", self.volume("relay"), RELAY_DIRECTORY, True),
            Mount("bind", vm_path(self.seed), "/seed.git", True),
            Mount("bind", vm_path(self.outbox), "/outbox.git", False),
            Mount("bind", vm_path(self.to_copilot), "/exchange/to_copilot", True),
            Mount("bind", vm_path(self.from_copilot), "/exchange/from_copilot", False),
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
            # Maps the agent to your user, so its outbox and exchange writes are yours on the host.
            "--userns=keep-id:uid=1000,gid=1000",
            "--network=none",
            "--pids-limit=4096",
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
            "--network=" + self.engine.network,
            "--pids-limit=512",
        ]
        for host in PROBE_HOSTS:
            arguments.append("--add-host=" + host)
        for mount in self.proxy_mounts():
            arguments += mount.argument()
        return arguments + [self.proxy_image]
