"""Trusted host Git operations; never execute Git inside the writable outbox."""

from __future__ import annotations

import os
from pathlib import Path
import re
import tempfile

from .common import LauncherError, make_process_env, raise_if_link, run


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


def configure_outbox_repo(repo: Path, directory: Path, name: str) -> Path:
    outbox = raise_if_link(directory / "outbox.git")
    if repo == outbox or repo in outbox.parents:
        raise LauncherError(
            "The container-writable outbox must be outside the host repository."
        )
    if not outbox.exists():
        run_git_command(repo, "-c", "init.templateDir=", "init", "--bare", str(outbox))
    remote = "sandbox-" + name
    # Fetching the raw outbox would run upload-pack on agent-written config; fetch_outbox uses a clean view.
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


# Build a branch name to commit sha map
def _get_branch_commits(repo: Path, outbox: Path) -> dict[str, str]:
    refs: dict[str, str] = {}
    packed = raise_if_link(outbox / "packed-refs")
    if packed.exists():
        if packed.stat().st_size > 16 * 1024 * 1024:
            raise LauncherError("Outbox packed refs exceed the safety limit.")
        for line in packed.read_text(encoding="ascii").splitlines():
            if line and not line.startswith(("#", "^")):
                oid, separator, name = line.partition(" ")
                if not separator:
                    raise LauncherError("Malformed outbox packed refs.")
                if name.startswith("refs/heads/"):
                    refs[name] = oid
    heads = raise_if_link(outbox / "refs/heads")
    if heads.exists():
        for parent, directories, files in os.walk(heads, followlinks=False):
            for name in (*directories, *files):
                raise_if_link(Path(parent) / name)
            for name in files:
                # A paused agent may be mid ref update; Git ignores lock files too.
                if name.endswith(".lock"):
                    continue
                path = Path(parent) / name
                if path.stat().st_size > 256:
                    raise LauncherError("Malformed outbox branch ref.")
                refs["refs/heads/" + path.relative_to(heads).as_posix()] = (
                    path.read_text(encoding="ascii").strip()
                )
    for name, oid in refs.items():
        if not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", oid):
            raise LauncherError(
                "Outbox must contain direct, hexadecimal branch refs only."
            )
        if run_git_command(repo, "check-ref-format", name, check=False).returncode:
            raise LauncherError("Invalid outbox branch name.")
    return refs


# Make a clean temporary copy of the output repo and fetch from that,
# just in case copilot has done something weird to the configuration
# of the real one.
def fetch_outbox_repo_safe(repo: Path, outbox: Path, name: str) -> None:
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
    refs = _get_branch_commits(repo, outbox)
    with tempfile.TemporaryDirectory(
        prefix="qdk-outbox-", dir=outbox.parent
    ) as temporary:
        view = Path(temporary)
        run_git_command(repo, "-c", "init.templateDir=", "init", "--bare", str(view))
        (view / "objects/info/alternates").write_text(
            objects.as_posix() + "\n", encoding="utf-8", newline="\n"
        )
        for ref, oid in refs.items():
            destination = view / ref
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(oid + "\n", encoding="ascii")
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
            "--no-tags",
            "--no-recurse-submodules",
            "--upload-pack=git-upload-pack",
            "--prune",
            view.as_posix(),
            f"+refs/heads/*:refs/remotes/{remote}/*",
        )
