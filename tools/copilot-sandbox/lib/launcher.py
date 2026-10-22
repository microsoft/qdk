from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import json
import logging
import os
from pathlib import Path
import re
import shutil
import sys

from .common import (
    LauncherError,
    format_error,
    make_private_dir,
    raise_if_link,
    write_atomic,
    make_process_env,
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
)
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
ROLE_LABEL = "io.qdk.copilot-sandbox.role=agent"
LOGGER = logging.getLogger(__name__)


def _get_project_name(name: str) -> str:
    return "qdk-copilot-sandbox-" + validate_sandbox_instance_name(name)


def _check_prerequisites() -> str:
    result = run(["docker", "version", "--format", "{{json .Server}}"])
    try:
        server = json.loads(result.stdout)
        major = int(server["Version"].split(".")[0])
        architecture = server["Arch"]
    except (KeyError, TypeError, ValueError):
        raise LauncherError(
            "Cannot determine Docker Engine version and architecture."
        ) from None
    if major < 28 or server.get("Os") != "linux":
        raise LauncherError(
            "Docker Engine >= 28 with Linux containers is required for isolated networking."
        )
    run(["docker", "compose", "version", "--short"])
    version = run(["git", "--version"]).stdout
    match = re.search(r"(\d+)\.(\d+)\.(\d+)", version)
    if not match or tuple(map(int, match.groups())) < (2, 45, 1):
        print(
            "Warning: Git >= 2.45.1 is recommended for local-repository security fixes."
        )
    return architecture


def _make_owner_id() -> str:
    return hashlib.sha256(str(get_sandbox_state_dir()).encode()).hexdigest()[:20]


def _get_running_agents(name: str | None = None, *, owned: bool = False) -> list[str]:
    args = ["docker", "ps", "--filter", "label=" + ROLE_LABEL]
    if name is not None:
        args.extend(
            ["--filter", "label=com.docker.compose.project=" + _get_project_name(name)]
        )
    if owned:
        args.extend(
            ["--filter", "label=io.qdk.copilot-sandbox.owner=" + _make_owner_id()]
        )
    args.extend(["--format", "{{.ID}}"])
    return run(args).stdout.split()


def _ensure_idle(name: str) -> None:
    if _get_running_agents(name):
        raise LauncherError(f"An agent is already running for instance '{name}'.")


def _check_project_owner(name: str) -> None:
    result = run(
        [
            "docker",
            "ps",
            "-a",
            "--filter",
            "label=com.docker.compose.project=" + _get_project_name(name),
            "--format",
            '{{.Label "io.qdk.copilot-sandbox.owner"}}',
        ]
    )
    if any(value != _make_owner_id() for value in result.stdout.splitlines()):
        raise LauncherError(
            "This Compose project has containers owned by another user or launcher; refusing to change it."
        )


def _run_compose_command(
    args: list[str],
    environment: dict[str, str],
    *,
    local: bool = False,
    interactive: bool = False,
    check: bool = True,
):
    command = [
        "docker",
        "compose",
        "--env-file",
        os.devnull,
        "--project-directory",
        str(ROOT),
        "--project-name",
        _get_project_name(environment["QDK_SANDBOX_INSTANCE"]),
        "-f",
        str(ROOT / "compose.yaml"),
    ]
    if local:
        command.extend(["-f", str(ROOT / "compose.local.yaml")])
    return run([*command, *args], env=environment, interactive=interactive, check=check)


def _validate_resource_limits(cpus: str, memory: str) -> tuple[str, str]:
    if not re.fullmatch(r"(?:0|[1-9]\d*)(?:\.\d+)?", cpus):
        raise LauncherError("--cpus must be a nonnegative decimal; 0 means unlimited.")
    if not re.fullmatch(r"0|[1-9]\d*(?:[bBkKmMgG](?:[bB])?)?", memory):
        raise LauncherError(
            "--memory must be bytes or an integer with b/k/m/g units; 0 means unlimited."
        )
    return cpus, memory


def _make_base_environment(name: str) -> dict[str, str]:
    environment = make_process_env()
    environment.update(
        {
            "QDK_SANDBOX_INSTANCE": name,
            "QDK_SANDBOX_OWNER": _make_owner_id(),
        }
    )
    return environment


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
    args: argparse.Namespace, directory: Path, config: dict, architecture: str
) -> int:
    common, branch, sha = get_seed_repo_state(REPO)
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
    agent_id, proxy_id, _ = get_image_pair(base, architecture, local=args.local)
    allowlist, _ = _load_local_allowlist(directory)
    incoming.mkdir(parents=True, exist_ok=True)
    outgoing.mkdir(parents=True, exist_ok=True)
    sanitize_exchange_folder(outgoing)
    outbox = configure_outbox_repo(REPO, directory, args.instance)
    config.update(
        {"repo": str(REPO), "to_copilot": str(incoming), "from_copilot": str(outgoing)}
    )
    if args.save:
        config.update(
            {"cpus": cpus, "memory": memory, "image_source": image_source, "tag": tag}
        )
    write_json(directory / "config.json", config)
    environment = _make_base_environment(args.instance)
    environment.update(
        {
            "QDK_SANDBOX_AGENT_IMAGE": agent_id,
            "QDK_SANDBOX_PROXY_IMAGE": proxy_id,
            "QDK_SANDBOX_SEED_DIR": str(common),
            "QDK_SANDBOX_OUTBOX_DIR": str(outbox),
            "QDK_SANDBOX_TO_DIR": str(incoming),
            "QDK_SANDBOX_FROM_DIR": str(outgoing),
            "QDK_SANDBOX_ALLOWLIST": str(allowlist),
            "QDK_SANDBOX_CPUS": cpus,
            "QDK_SANDBOX_MEMORY": memory,
            "QDK_SANDBOX_TOKEN": "unused",
        }
    )
    proxy_args = [
        "up",
        "-d",
        "--no-build",
        "--pull",
        "never",
        "--force-recreate",
        "proxy",
    ]
    status = 1
    cleanup_failed = False
    try:
        _run_compose_command(proxy_args, environment, local=args.local)
        environment.update(
            {
                "QDK_SANDBOX_TOKEN": token,
                "QDK_SEED_BRANCH": branch,
                "QDK_SEED_SHA": sha,
                "QDK_BRANCH": args.branch or "",
            }
        )
        command = [
            "run",
            "--rm",
            "--no-deps",
            "--pull",
            "never",
            "--name",
            _get_project_name(args.instance) + "-agent",
            "agent",
        ]
        if args.copilot_args:
            arguments = args.copilot_args
            if arguments[0] == "--":
                arguments = arguments[1:]
            command.extend(arguments)
        status = _run_compose_command(
            command, environment, local=args.local, interactive=True, check=False
        ).returncode
    finally:
        environment.pop("QDK_SANDBOX_TOKEN", None)
        token = ""
        try:
            for container in _get_running_agents(args.instance, owned=True):
                run(["docker", "rm", "--force", container])
        except LauncherError as error:
            cleanup_failed = True
            LOGGER.warning(
                "Could not stop the agent; quarantine and fetch were not run while it may still be writing.\n%s",
                format_error(error),
            )
        if not cleanup_failed:
            for action, message in (
                (
                    lambda: sanitize_exchange_folder(outgoing),
                    "Output quarantine failed; do not open the exchange output in Git-aware tools.",
                ),
                (
                    lambda: fetch_outbox_repo_safe(REPO, outbox, args.instance),
                    "Host fetch failed; commits remain in the instance outbox.",
                ),
            ):
                try:
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
    _check_project_owner(instance_name)
    running = _get_running_agents(instance_name, owned=True)
    if not idle and not running:
        raise LauncherError(
            "The instance is starting or busy; retry once its agent is running."
        )
    paused = []
    try:
        # Frozen, the agent cannot swap in symlinks or alternates after validation.
        for container in running:
            run(["docker", "pause", container])
            paused.append(container)
        fetch_outbox_repo_safe(REPO, outbox, instance_name)
    finally:
        for container in paused:
            run(["docker", "unpause", container], check=False)


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
            running = _get_running_agents(child.name, owned=True)
            active = ""
            if running:
                status = "running"
                result = run(
                    [
                        "docker",
                        "inspect",
                        "--format",
                        "{{json .HostConfig}}",
                        running[0],
                    ]
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


def _delete_instance(args: argparse.Namespace, directory: Path, config: dict) -> None:
    if not directory.exists():
        raise LauncherError("Instance does not exist.")
    if _get_running_agents(args.instance):
        raise LauncherError("Cannot delete a running instance. Exit its agent first.")
    print(
        "This deletes work and Copilot volumes, saved PAT, settings, extra Dockerfile, and outbox commits."
    )
    print("Exchange folders and any already-fetched host commits are retained.")
    if (
        input(f"Type '{args.instance}' to delete this instance: ").strip()
        != args.instance
    ):
        raise LauncherError("Deletion cancelled.")
    environment = _make_base_environment(args.instance)
    environment.update(
        {
            "QDK_SANDBOX_AGENT_IMAGE": "unused",
            "QDK_SANDBOX_PROXY_IMAGE": "unused",
            "QDK_SANDBOX_SEED_DIR": str(REPO / ".git"),
            "QDK_SANDBOX_OUTBOX_DIR": str(directory / "outbox.git"),
            "QDK_SANDBOX_TO_DIR": str(directory),
            "QDK_SANDBOX_FROM_DIR": str(directory),
            "QDK_SANDBOX_ALLOWLIST": str(directory / "allowlist.local.txt"),
            "QDK_SANDBOX_TOKEN": "unused",
        }
    )
    _run_compose_command(
        ["down", "--volumes", "--remove-orphans"], environment, local=args.local
    )
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
        description="Launch the QDK Copilot sandbox (Python >= 3.10; Docker >= 28)."
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
    for command, help_text in (
        ("fetch", "Fetch sandbox branches now, briefly pausing a running agent"),
        ("set-token", "Create and store a seven-day fine-grained PAT"),
        ("shell", "Open a shell in a running agent; never start a sandbox"),
    ):
        commands.add_parser(command, help=help_text, parents=[instance_options])
    delete = commands.add_parser(
        "delete-instance",
        help="Confirm and delete the selected instance, including its volumes",
        parents=[instance_options],
    )
    delete.add_argument(
        "--local",
        action="store_true",
        help="Use the local-image Compose configuration",
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
    launch.add_argument(
        "--branch", help="Ask the runtime to switch branches, only with a clean tree"
    )
    launch.add_argument("--cpus", help="CPU limit; 0 means unlimited")
    launch.add_argument("--memory", help="Memory limit, such as 8g; 0 means unlimited")
    launch.add_argument(
        "--save",
        action="store_true",
        help="Persist CPU, memory, image source and tag for this instance",
    )
    launch.add_argument(
        "--to-copilot-dir", help="Relocate the read-only context exchange directory"
    )
    launch.add_argument(
        "--from-copilot-dir", help="Relocate the writable output exchange directory"
    )
    launch.add_argument(
        "copilot_args",
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
    if args.action == "run" and args.copilot_args:
        if args.copilot_args[0] != "--":
            parser.error(
                "Custom commands must follow '--'; put sandbox options before '--'."
            )
        if len(args.copilot_args) == 1:
            parser.error("Expected a command after '--'.")
    try:
        if args.action == "shell":
            return _open_shell_on_existing_session(args.instance)
        if args.action == "list":
            _check_prerequisites()
            _list_instances()
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
            architecture = _check_prerequisites()
            _check_project_owner(args.instance)
            if args.action == "delete-instance":
                with session_lock(
                    get_sandbox_state_dir(), f".token-{args.instance}.lock"
                ):
                    _delete_instance(args, directory, config)
                return 0
            _ensure_idle(args.instance)
            return _run_session(args, directory, config, architecture)
    except KeyboardInterrupt:
        print("Sandbox interrupted.", file=sys.stderr)
        return 130
    except (LauncherError, ValueError, OSError, EOFError) as error:
        print("error: " + format_error(error), file=sys.stderr)
        return 1


def _open_shell_on_existing_session(instance: str) -> int:
    running = _get_running_agents(instance, owned=True)
    if len(running) != 1:
        raise LauncherError(
            "The selected instance has no single active agent; launch sandbox.py run first."
        )
    return run(
        [
            "docker",
            "exec",
            "-it",
            "--user",
            "1000:1000",
            "--workdir",
            "/work/qdk",
            running[0],
            "/bin/bash",
        ],
        interactive=True,
        check=False,
    ).returncode
