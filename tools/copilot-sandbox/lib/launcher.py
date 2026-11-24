from __future__ import annotations

import argparse
from contextlib import ExitStack
import json
import logging
from pathlib import Path
import re
import shutil
import signal
import sys

from .boundary import verify_boundary
from .common import (
    LauncherError,
    format_error,
    make_private_dir,
    raise_if_link,
    write_atomic,
    run,
    write_json,
)
from .exchange import sanitize_exchange_folder, validate_exchange_directories
from .git import (
    fetch_outbox_repo_safe,
    run_git_command,
    get_seed_repo_state,
    configure_outbox_repo,
)
from .images import (
    LOCAL_IMAGE_TAG,
    get_image_names_for_tag,
    get_image_pair,
    get_reused_image_pair,
)
from .layout import AGENT_USER, LABEL_PREFIX, PROXY_URL, Session
from .podman import Engine, check_podman, machine_shares, podman, setup_machine
from .secrets import TokenStore, request_and_store_token, validate_token_properties
from .state import (
    LockBusyError,
    get_sandbox_instance_config,
    get_sandbox_instance_dir,
    validate_sandbox_instance_name,
    session_lock,
    get_sandbox_state_dir,
)

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parents[1]
LOGGER = logging.getLogger(__name__)


def _check_prerequisites() -> Engine:
    engine = check_podman()
    version = run(["git", "--version"]).stdout
    match = re.search(r"(\d+)\.(\d+)\.(\d+)", version)
    if not match or tuple(map(int, match.groups())) < (2, 45, 1):
        print(
            "Warning: Git >= 2.45.1 is recommended for local-repository security fixes."
        )
    return engine


def _list_containers(
    name: str, *, role: str | None = None, active: bool = False
) -> list[str]:
    args = ["ps", "--all", "--no-trunc"]
    args += ["--filter", f"label={LABEL_PREFIX}instance={name}"]
    if role is not None:
        args += ["--filter", f"label={LABEL_PREFIX}role={role}"]
    if active:
        # Podman ORs status filters; plain `ps` hides paused agents, which still hold the instance.
        args += ["--filter", "status=running", "--filter", "status=paused"]
    return run(podman(*args, "--format", "{{.ID}}")).stdout.split()


def _get_running_agents(name: str) -> list[str]:
    return _list_containers(name, role="agent", active=True)


def _ensure_idle(name: str) -> None:
    if _get_running_agents(name):
        raise LauncherError(f"An agent is already running for instance '{name}'.")


def _stop_agent(container: str) -> None:
    print("Requesting graceful agent shutdown...", flush=True)
    try:
        result = run(
            podman(
                "exec",
                "--user",
                AGENT_USER,
                container,
                "timeout",
                "--signal=KILL",
                "30s",
                "bash",
                "-c",
                """
pidfile=$1
[[ -f $pidfile ]] || exit 0
read -r pid < "$pidfile"
[[ $pid =~ ^[1-9][0-9]*$ ]] || exit 1
kill -TERM "$pid" 2>/dev/null || exit 0
while [[ -e $pidfile ]] && kill -0 "$pid" 2>/dev/null; do
    sleep 0.1
done
""",
                "sandbox-stop",
                "/tmp/qdk-sandbox-entrypoint.pid",
            ),
            check=False,
        )
        if result.returncode:
            LOGGER.warning(
                "Graceful agent shutdown failed or timed out; forcing removal."
            )
    except (LauncherError, OSError, ValueError, KeyboardInterrupt) as error:
        LOGGER.warning(
            "Graceful agent shutdown interrupted or failed; forcing removal.\n%s",
            format_error(error),
        )
    # --ignore: the agent ends its own container if it kills the relay.
    print("Removing sandbox agent container...", flush=True)
    run(podman("rm", "--force", "--ignore", "--time", "3", container))


def _validate_resource_limits(cpus: str, memory: str) -> tuple[str, str]:
    if not re.fullmatch(r"(?:0|[1-9]\d*)(?:\.\d+)?", cpus):
        raise LauncherError("--cpus must be a nonnegative decimal; 0 means unlimited.")
    if not re.fullmatch(r"0|[1-9]\d*(?:[bBkKmMgG](?:[bB])?)?", memory):
        raise LauncherError(
            "--memory must be bytes or an integer with b/k/m/g units; 0 means unlimited."
        )
    return cpus, memory


def _get_host_git_identity() -> dict[str, str]:
    # Unlike run_git_command, this reads global config, where the identity usually lives.
    values = []
    for key in ("user.name", "user.email"):
        value = run(["git", "-C", REPO, "config", "--get", key], check=False)
        values.append(value.stdout.strip())
    name, email = values
    if not name or not email:
        print("Host Git identity not configured; sandbox commits use 'agent'.")
        return {}
    return {"QDK_GIT_NAME": name, "QDK_GIT_EMAIL": email}


def _make_agent_environment(
    name: str, branch: str, sha: str, selected_branch: str
) -> dict[str, str]:
    return {
        **_get_host_git_identity(),
        "HOME": "/home/agent",
        "USER": "agent",
        "SHELL": "/bin/bash",
        "QDK_SEED_BRANCH": branch,
        "QDK_SEED_SHA": sha,
        "QDK_BRANCH": selected_branch,
        "QDK_SANDBOX_INSTANCE": name,
        "HTTPS_PROXY": PROXY_URL,
        "HTTP_PROXY": PROXY_URL,
        "https_proxy": PROXY_URL,
        "http_proxy": PROXY_URL,
        "NO_PROXY": "localhost,127.0.0.1,::1",
        "no_proxy": "localhost,127.0.0.1,::1",
        "NODE_USE_ENV_PROXY": "1",
        "COPILOT_AUTO_UPDATE": "false",
    }


def _warn_about_machine_shares(common: Path) -> None:
    allowed = {str(REPO), str(common), str(get_sandbox_state_dir())}
    extra = [share for share in machine_shares() if share not in allowed]
    if extra:
        print(
            "Warning: the Podman machine shares more than this checkout and the sandbox state;"
            " a container escape could reach:"
        )
        for share in extra:
            print("  " + share)


def _get_exchange_directories(
    args: argparse.Namespace, config: dict, directory: Path, common: Path
) -> tuple[Path, Path]:
    default = REPO / "exchange" / args.instance
    incoming = Path(
        args.to_copilot_dir or config.get("to_copilot", str(default / "to_copilot"))
    )
    outgoing = Path(
        args.from_copilot_dir
        or config.get("from_copilot", str(default / "from_copilot"))
    )
    return validate_exchange_directories(
        incoming, outgoing, REPO, directory, (common, directory / "outbox.git")
    )


def _load_local_allowlist(directory: Path) -> tuple[Path, bytes]:
    path = raise_if_link(directory / "allowlist.local.txt")
    if not path.exists():
        write_atomic(path, b"")
    path.chmod(0o644)
    contents = path.read_bytes()
    try:
        lines = contents.decode("ascii").splitlines()
    except UnicodeError:
        raise LauncherError(
            "Local allowlist must contain ASCII domain names."
        ) from None
    domains = []
    for line in lines:
        domain = line.split("#", 1)[0].strip()
        if not domain:
            continue
        if (
            not re.fullmatch(r"\.?[a-zA-Z0-9](?:[a-zA-Z0-9.-]*[a-zA-Z0-9])?", domain)
            or ".." in domain
            or not any(character.isalpha() for character in domain)
        ):
            raise LauncherError(
                "Local allowlist accepts domain names only, one per line."
            )
        if "." not in domain.lstrip("."):
            raise LauncherError(
                "Local allowlist entries must have at least two labels; top-level domains are refused."
            )
        domains.append(domain)
    print("Local allowlist additions (each widens agent egress):")
    for domain in domains:
        print("  " + domain)
    if not domains:
        print("  (none)")
    return path, contents


def _run_session(
    args: argparse.Namespace, directory: Path, config: dict, engine: Engine
) -> int:
    common, branch, sha = get_seed_repo_state(REPO)
    _warn_about_machine_shares(common)
    if config.get("repo", str(REPO)) != str(REPO):
        print(
            "Warning: this instance was last used from another checkout; preserving its work clone and updating the seed checkout.",
            file=sys.stderr,
        )
    incoming, outgoing = _get_exchange_directories(args, config, directory, common)
    cpus, memory = _validate_resource_limits(
        args.cpus or config.get("cpus", "0"), args.memory or config.get("memory", "0")
    )
    if (
        args.branch
        and run_git_command(
            REPO, "check-ref-format", "--branch", args.branch, check=False
        ).returncode
    ):
        raise LauncherError("--branch must be a valid Git branch name.")
    make_private_dir(directory)
    store = TokenStore(args.instance, directory)
    token = store.load()
    validate_token_properties(token, instance=args.instance)
    if args.reuse:
        agent_id, proxy_id, build_id = get_reused_image_pair(
            config, engine.architecture, instance=args.instance
        )
        print(f"Reusing build {build_id} for instance '{args.instance}'.")
        print(f"  agent: {agent_id}\n  proxy: {proxy_id}")
        args.local = False
    else:
        saved_source = config.get("image_source", "registry")
        if args.local is None:
            args.local = saved_source == "local"
        image_source = "local" if args.local else "registry"
        tag = (
            args.tag
            or (config.get("tag") if image_source == saved_source else None)
            or (LOCAL_IMAGE_TAG if args.local else "latest")
        )
        base, _ = get_image_names_for_tag(tag, local=args.local)
        agent_id, proxy_id, build_id = get_image_pair(
            base, engine.architecture, local=args.local
        )
        if args.save:
            config.update({"image_source": image_source, "tag": tag})
    allowlist, _ = _load_local_allowlist(directory)
    print("Preparing exchange directories...", flush=True)
    incoming.mkdir(parents=True, exist_ok=True)
    outgoing.mkdir(parents=True, exist_ok=True)
    print("Sanitizing outgoing exchange directory...", flush=True)
    sanitize_exchange_folder(outgoing)
    print("Configuring Git outbox...", flush=True)
    outbox = configure_outbox_repo(REPO, directory, args.instance)
    config["repo"] = str(REPO)
    if args.save:
        config.update(
            {
                "cpus": cpus,
                "memory": memory,
                "to_copilot": str(incoming),
                "from_copilot": str(outgoing),
            }
        )
    session = Session(
        instance=args.instance,
        engine=engine,
        agent_image=agent_id,
        proxy_image=proxy_id,
        seed=common,
        outbox=outbox,
        to_copilot=incoming,
        from_copilot=outgoing,
        allowlist=allowlist,
        cpus=cpus,
        memory=memory,
        environment=_make_agent_environment(
            args.instance, branch, sha, args.branch or ""
        ),
    )
    status = 1
    cleanup_failed = False
    try:
        print("Removing previous sandbox containers...", flush=True)
        run(podman("rm", "--force", "--ignore", session.proxy, session.agent))
        # The relay volume holds only the proxy's socket; recreating it discards anything stale.
        print("Recreating proxy relay volume...", flush=True)
        run(podman("volume", "rm", "--force", session.volume("relay")))
        print("Starting sandbox proxy...", flush=True)
        run(
            podman(
                "run", "--detach", "--name", session.proxy, *session.proxy_arguments()
            )
        )
        # The relay is the agent's main process; the workload starts only after verification.
        print("Starting sandbox agent relay...", flush=True)
        run(
            podman(
                "run",
                "--detach",
                "--rm",
                "--name",
                session.agent,
                *session.agent_arguments(),
            )
        )
        verified_container = verify_boundary(session)
        # Deliver the PAT over stdin into tmpfs: never in a command line, the environment, or on disk.
        run(
            podman(
                "exec",
                "-i",
                "--user",
                AGENT_USER,
                verified_container,
                "/opt/sandbox/runtime/receive-token",
            ),
            input_text=token,
        )
        token = ""
        print("Sandbox boundary verified; starting workload.")
        arguments = ["copilot", "--allow-all"]
        if args.command:
            arguments = args.command
            if arguments[0] == "--":
                arguments = arguments[1:]
        config.update(
            {
                "agent_id": agent_id,
                "proxy_id": proxy_id,
                "build_id": build_id,
                "verified_container": verified_container,
            }
        )
        write_json(directory / "config.json", config)
        execute = ["exec", "-i"]
        if sys.stdin.isatty() and sys.stdout.isatty():
            execute.append("-t")
        status = run(
            podman(
                *execute,
                "--user",
                AGENT_USER,
                verified_container,
                "/opt/sandbox/runtime/entrypoint.sh",
                *arguments,
            ),
            interactive=True,
            check=False,
        ).returncode
    finally:
        token = ""
        try:
            print("Stopping sandbox agents...", flush=True)
            for container in _get_running_agents(args.instance):
                _stop_agent(container)
        except (LauncherError, OSError, ValueError) as error:
            cleanup_failed = True
            LOGGER.warning(
                "Could not stop the agent; quarantine and fetch were skipped.\n%s",
                format_error(error),
            )
        try:
            # The proxy is the only container with egress; don't leave it idle.
            print("Removing sandbox proxy...", flush=True)
            run(podman("rm", "--force", "--ignore", "--time", "3", session.proxy))
            print("Removing proxy relay volume...", flush=True)
            run(podman("volume", "rm", "--force", session.volume("relay")))
        except (LauncherError, OSError, ValueError) as error:
            LOGGER.warning("Could not remove the proxy.\n%s", format_error(error))
        if not cleanup_failed:
            for action, progress, message in (
                (
                    lambda: sanitize_exchange_folder(outgoing),
                    "Sanitizing outgoing exchange directory...",
                    "Output quarantine failed; do not open the exchange output in Git-aware tools.",
                ),
                (
                    lambda: fetch_outbox_repo_safe(REPO, outbox, args.instance),
                    "Fetching sandbox commits into the host repository...",
                    "Host fetch failed; commits remain in the instance outbox.",
                ),
            ):
                try:
                    print(progress, flush=True)
                    action()
                except (LauncherError, OSError, ValueError) as error:
                    cleanup_failed = True
                    LOGGER.warning("%s\n%s", message, format_error(error))
    return status or (1 if cleanup_failed else 0)


def _fetch_outbox_repo_for_instance(
    instance_name: str, directory: Path, *, idle: bool
) -> None:
    if get_sandbox_instance_config(directory).get("repo", str(REPO)) != str(REPO):
        raise LauncherError(
            "This instance was last launched from another checkout; fetch from there."
        )
    outbox = raise_if_link(directory / "outbox.git")
    if not outbox.is_dir():
        raise LauncherError("This instance has no outbox yet; launch it first.")
    running = _get_running_agents(instance_name)
    if not idle and not running:
        raise LauncherError(
            "The instance is starting or busy; retry once its agent is running."
        )
    paused = []
    try:
        # Frozen, the agent cannot swap in symlinks or alternates after validation.
        for container in running:
            run(podman("pause", container))
            paused.append(container)
        fetch_outbox_repo_safe(REPO, outbox, instance_name)
    finally:
        for container in paused:
            run(podman("unpause", container), check=False)


def _list_instances() -> None:
    root = get_sandbox_state_dir()
    if not root.exists():
        print("No sandbox instances.")
        return
    for child in sorted(root.iterdir()):
        if child.name.startswith("."):
            continue
        validate_sandbox_instance_name(child.name)
        raise_if_link(child)
        if child.is_dir():
            config = get_sandbox_instance_config(child)
            running = _get_running_agents(child.name)
            active = ""
            if running:
                status = "running"
                result = run(
                    podman("inspect", "--format", "{{json .HostConfig}}", running[0])
                )
                try:
                    limits = json.loads(result.stdout)
                    active = (
                        f", active_cpus={limits['NanoCpus'] / 1_000_000_000:g}"
                        f", active_memory={limits['Memory']}b"
                    )
                except (ValueError, KeyError, TypeError) as error:
                    raise LauncherError(
                        f"Cannot read active resource limits for instance '{child.name}'."
                    ) from error
            else:
                try:
                    with session_lock(root, f".instance-{child.name}.lock"):
                        status = "stopped"
                except LockBusyError:
                    status = "busy"
            print(
                f"{child.name}: {status}, saved_cpus={config.get('cpus', '0')}, "
                f"saved_memory={config.get('memory', '0')}{active}"
            )


def _stop_instance(name: str) -> None:
    running = _get_running_agents(name)
    if not running:
        with session_lock(get_sandbox_state_dir(), f".instance-{name}.lock"):
            print(f"Instance '{name}' is not running.")
        return
    run(podman("stop", *running))
    print(f"Stopped agent for instance '{name}'. Work and settings are retained.")


def _delete_instance(args: argparse.Namespace, directory: Path, config: dict) -> None:
    if not directory.exists():
        raise LauncherError("Instance does not exist.")
    if _get_running_agents(args.instance):
        raise LauncherError("Cannot delete a running instance. Exit its agent first.")
    print(
        "This deletes work and Copilot volumes, saved PAT, settings, and outbox commits."
    )
    print("Exchange folders and any already-fetched host commits are retained.")
    if (
        input(f"Type '{args.instance}' to delete this instance: ").strip()
        != args.instance
    ):
        raise LauncherError("Deletion cancelled.")
    for container in _list_containers(args.instance):
        run(podman("rm", "--force", "--ignore", "--time", "3", container))
    prefix = "qdk-copilot-sandbox-" + args.instance
    for kind in ("work", "copilot", "relay"):
        run(podman("volume", "rm", "--force", f"{prefix}_{kind}"))
    TokenStore(args.instance, directory).delete()
    if config.get("repo") == str(REPO):
        with session_lock(get_sandbox_state_dir(), ".git-config.lock", blocking=True):
            run_git_command(
                REPO, "remote", "remove", "sandbox-" + args.instance, check=False
            )
    else:
        print(
            "The last checkout differs; remove its sandbox remote manually if it still exists."
        )
    shutil.rmtree(directory)


def make_argument_parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(
        description="Launch the QDK Copilot sandbox (Python >= 3.10; rootless Podman >= 4.9)."
    )
    instance_options = argparse.ArgumentParser(add_help=False)
    instance_options.add_argument(
        "--instance", default="default", type=validate_sandbox_instance_name
    )
    commands = result.add_subparsers(dest="action", required=True)
    launch = commands.add_parser(
        "run", help="Launch a sandbox", parents=[instance_options]
    )
    commands.add_parser("list", help="List private instances and running status")
    commands.add_parser(
        "setup-machine",
        help="Windows/macOS: create the dedicated qdk-sandbox Podman machine",
    )
    for command, help_text in (
        ("fetch", "Fetch sandbox branches now, briefly pausing a running agent"),
        ("set-token", "Create and store a seven-day fine-grained PAT"),
        ("shell", "Open a shell in a running agent; never start a sandbox"),
        ("stop", "Stop the selected instance's agent without deleting its work"),
    ):
        commands.add_parser(command, help=help_text, parents=[instance_options])
    commands.add_parser(
        "delete",
        help="Confirm and delete the selected instance, including its volumes",
        parents=[instance_options],
    )
    launch.add_argument(
        "--tag",
        help=f"Image tag (default: saved tag, latest for registry, {LOCAL_IMAGE_TAG} for local)",
    )
    image_source = launch.add_mutually_exclusive_group()
    image_source.add_argument(
        "--local",
        action="store_true",
        default=None,
        help=f"Use prebuilt local images (default tag {LOCAL_IMAGE_TAG}); never pull",
    )
    image_source.add_argument(
        "--registry",
        dest="local",
        action="store_false",
        default=None,
        help="Use published registry images instead of a saved local-image preference",
    )
    image_source.add_argument(
        "--reuse",
        action="store_true",
        help="Reuse this instance's last image pair by ID; never contact the registry or pull",
    )
    launch.add_argument(
        "--branch", help="Ask the runtime to switch branches, only with a clean tree"
    )
    launch.add_argument("--cpus", help="CPU limit; 0 means unlimited")
    launch.add_argument("--memory", help="Memory limit, such as 8g; 0 means unlimited")
    launch.add_argument(
        "--save",
        action="store_true",
        help="Persist resource limits, image selection, and exchange paths for this instance",
    )
    launch.add_argument(
        "--to-copilot-dir", help="Relocate the read-only context exchange directory"
    )
    launch.add_argument(
        "--from-copilot-dir", help="Relocate the writable output exchange directory"
    )
    launch.add_argument(
        "command",
        nargs=argparse.REMAINDER,
        help="Full command after --; defaults to copilot --allow-all",
    )
    return result


def main(argv: list[str] | None = None) -> int:
    logging.basicConfig(format="%(levelname)s: %(message)s")
    if sys.version_info < (3, 10):
        print("Python >= 3.10 is required.", file=sys.stderr)
        return 1
    parser = make_argument_parser()
    args = parser.parse_args(argv)
    if args.action == "run" and args.reuse and args.tag is not None:
        parser.error("--reuse cannot be combined with --tag.")
    if args.action == "run" and args.command:
        if args.command[0] != "--":
            parser.error(
                "Custom commands must follow '--'; put sandbox options before '--'."
            )
        if len(args.command) == 1:
            parser.error("Expected a command after '--'.")
    try:
        if args.action == "shell":
            return _open_shell_on_existing_session(args.instance)
        if args.action == "stop":
            _stop_instance(args.instance)
            return 0
        if args.action == "list":
            _check_prerequisites()
            _list_instances()
            return 0
        if args.action == "setup-machine":
            common = get_seed_repo_state(REPO)[0]
            shared = [REPO, get_sandbox_state_dir()]
            if not common.is_relative_to(REPO):
                shared.append(common)
            make_private_dir(get_sandbox_state_dir())
            setup_machine(shared)
            return 0
        directory = get_sandbox_instance_dir(args.instance)
        if args.action == "set-token":
            with session_lock(get_sandbox_state_dir(), f".token-{args.instance}.lock"):
                make_private_dir(directory)
                request_and_store_token(TokenStore(args.instance, directory))
            return 0
        if args.action == "fetch":
            _check_prerequisites()
            root = get_sandbox_state_dir()
            with (
                session_lock(root, f".fetch-{args.instance}.lock"),
                ExitStack() as stack,
            ):
                try:
                    stack.enter_context(
                        session_lock(root, f".instance-{args.instance}.lock")
                    )
                    idle = True
                except LockBusyError:
                    idle = False
                _fetch_outbox_repo_for_instance(args.instance, directory, idle=idle)
            print(f"Fetched sandbox branches into sandbox-{args.instance}/*.")
            return 0
        with session_lock(get_sandbox_state_dir(), f".instance-{args.instance}.lock"):
            config = get_sandbox_instance_config(directory)
            engine = _check_prerequisites()
            if args.action == "delete":
                with session_lock(
                    get_sandbox_state_dir(), f".token-{args.instance}.lock"
                ):
                    _delete_instance(args, directory, config)
                return 0
            _ensure_idle(args.instance)
            # Route termination signals through session cleanup, then restore the original handlers.
            with ExitStack() as stack:
                for name in ("SIGTERM", "SIGHUP"):
                    signum = getattr(signal, name, None)
                    if signum is not None:
                        previous = signal.signal(signum, signal.default_int_handler)
                        stack.callback(signal.signal, signum, previous)
                return _run_session(args, directory, config, engine)
    except KeyboardInterrupt:
        print("Sandbox interrupted.", file=sys.stderr)
        return 130
    except (LauncherError, ValueError, OSError, EOFError) as error:
        print("error: " + format_error(error), file=sys.stderr)
        return 1


def _open_shell_on_existing_session(instance: str) -> int:
    running = _get_running_agents(instance)
    if len(running) != 1:
        raise LauncherError(
            "The selected instance has no single active agent; launch sandbox.py run first."
        )
    actual = run(
        podman("container", "inspect", "--format", "{{.Id}}", running[0])
    ).stdout.strip()
    config = get_sandbox_instance_config(get_sandbox_instance_dir(instance))
    if not actual or config.get("verified_container") != actual:
        raise LauncherError(
            "The agent container has not completed boundary verification."
        )
    return run(
        podman(
            "exec",
            "-it",
            "--user",
            AGENT_USER,
            "--workdir",
            "/work/qdk",
            running[0],
            "/bin/bash",
        ),
        interactive=True,
        check=False,
    ).returncode
