"""Trusted host Git operations; never execute Git inside the writable outbox."""

from __future__ import annotations

import os
from pathlib import Path
import re
import tempfile

from .common import LauncherError, make_process_env, raise_if_link, run
from .state import get_sandbox_state_dir, session_lock


def run_git_command(repo: Path, *args: str, check: bool = True):
    environment = make_process_env()
    environment.update(
        {
            "GIT_TERMINAL_PROMPT": "0",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
        }
    )
    return run(
        [
            "git",
            "-C",
            repo,
            "-c",
            "core.hooksPath=" + os.devnull,
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.pager=cat",
            *args,
        ],
        env=environment,
        check=check,
    )


def get_seed_repo_state(repo: Path) -> tuple[Path, str, str]:
    top = Path(
        run_git_command(repo, "rev-parse", "--show-toplevel").stdout.strip()
    ).resolve()
    if top != repo.resolve():
        raise LauncherError(
            "The launcher must live in the QDK repository's tools/copilot-sandbox directory."
        )
    common = raise_if_link(
        Path(
            run_git_command(
                repo, "rev-parse", "--path-format=absolute", "--git-common-dir"
            ).stdout.strip()
        )
    )
    actual = raise_if_link(
        Path(run_git_command(repo, "rev-parse", "--absolute-git-dir").stdout.strip())
    )
    if common != actual:
        raise LauncherError(
            "Linked worktrees are not supported. Launch from the main checkout; its common .git can be mounted safely."
        )
    if any(
        (common / "objects/info" / name).exists()
        for name in ("alternates", "http-alternates")
    ):
        raise LauncherError(
            "Seed uses external object alternates. Use a standalone clone for the sandbox."
        )
    sha = run_git_command(repo, "rev-parse", "--verify", "HEAD^{commit}").stdout.strip()
    branch = run_git_command(
        repo, "symbolic-ref", "--quiet", "--short", "HEAD", check=False
    ).stdout.strip()
    return common, branch, sha


def get_outbox_copy(directory: Path) -> Path:
    """The host-owned copy of the agent's outbox volume's objects; refs arrive separately, as data."""
    return raise_if_link(directory / "outbox")


def configure_outbox_copy(repo: Path, directory: Path, name: str) -> Path:
    """Creates the outbox copy's folder and configures the host's fetch-only sandbox remote."""
    outbox = get_outbox_copy(directory)
    if repo == outbox or repo in outbox.parents:
        raise LauncherError("The outbox copy must be outside the host repository.")
    outbox.mkdir(mode=0o700, exist_ok=True)
    remote = "sandbox-" + name
    # The copy holds agent-written data; fetch_outbox_repo_safe validates it and fetches through a clean view.
    with session_lock(get_sandbox_state_dir(), ".git-config.lock", blocking=True):
        run_git_command(
            repo,
            "config",
            "--replace-all",
            f"remote.{remote}.url",
            "disabled://use-sandbox.py",
        )
        run_git_command(
            repo, "config", "--replace-all", f"remote.{remote}.skipFetchAll", "true"
        )
        run_git_command(
            repo,
            "config",
            "--replace-all",
            f"remote.{remote}.fetch",
            f"+refs/heads/*:refs/remotes/{remote}/*",
        )
        run_git_command(
            repo,
            "config",
            "--replace-all",
            f"remote.{remote}.pushurl",
            "disabled://fetch-only",
        )
    return outbox


def _validate_branch_commits(repo: Path, refs: dict[str, str]) -> dict[str, str]:
    for name, oid in refs.items():
        if not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", oid):
            raise LauncherError(
                "Outbox must contain direct, hexadecimal branch refs only."
            )
        if (
            not name.startswith("refs/heads/")
            or any(character.isspace() for character in name)
            or run_git_command(repo, "check-ref-format", name, check=False).returncode
        ):
            raise LauncherError("Invalid outbox branch name.")
    return refs


# Fetch the copied outbox objects through a clean, host-configured view holding the validated refs,
# so no agent-written Git configuration is ever used.
def fetch_outbox_repo_safe(
    repo: Path, outbox: Path, name: str, refs: dict[str, str]
) -> None:
    raise_if_link(outbox)
    objects = raise_if_link(outbox / "objects")
    for parent, directories, files in os.walk(objects, followlinks=False):
        for entry in (*directories, *files):
            raise_if_link(Path(parent) / entry)
    if any(
        (objects / "info" / item).exists() for item in ("alternates", "http-alternates")
    ):
        raise LauncherError(
            "Outbox object alternates are forbidden; host fetch refused."
        )
    if "\n" in str(objects) or "\r" in str(objects):
        raise LauncherError("Outbox object path contains a line break.")
    refs = _validate_branch_commits(repo, refs)
    with tempfile.TemporaryDirectory(
        prefix="qdk-outbox-", dir=outbox.parent
    ) as temporary:
        view = Path(temporary)
        run_git_command(repo, "-c", "init.templateDir=", "init", "--bare", str(view))
        (view / "objects/info/alternates").write_text(
            objects.as_posix() + "\n", encoding="utf-8", newline="\n"
        )
        # One packed-refs file, so refs differing only in case can't collide on the host's file system.
        (view / "packed-refs").write_text(
            "".join(f"{oid} {ref}\n" for ref, oid in sorted(refs.items())),
            encoding="ascii",
            newline="\n",
        )
        remote = "sandbox-" + name
        run_git_command(
            repo,
            "-c",
            "protocol.file.allow=always",
            "-c",
            "fetch.fsckObjects=true",
            "-c",
            "transfer.fsckObjects=true",
            "fetch",
            "--no-write-fetch-head",
            "--no-tags",
            "--no-recurse-submodules",
            "--upload-pack=git-upload-pack",
            "--prune",
            view.as_posix(),
            f"+refs/heads/*:refs/remotes/{remote}/*",
        )
