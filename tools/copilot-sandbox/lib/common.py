"""Small, non-secret command and private-file helpers."""

from __future__ import annotations

import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
from typing import Mapping, Sequence


class LauncherError(Exception):
    """A user-facing error containing no command output or credentials."""


def is_link(path: Path) -> bool:
    try:
        info = path.lstat()
    except FileNotFoundError:
        return False
    return stat.S_ISLNK(info.st_mode) or bool(
        getattr(info, "st_file_attributes", 0) & 0x400
    )


def raise_if_link(path: Path) -> Path:
    path = Path(os.path.abspath(path.expanduser()))
    for component in (*reversed(path.parents), path):
        if is_link(component):
            raise ValueError(f"Symlinks and junctions are not allowed: {component}")
    return path


def make_process_env() -> dict[str, str]:
    return {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("GIT_", "COMPOSE_", "QDK_SANDBOX_", "QDK_SEED_"))
        and key not in {"COPILOT_GITHUB_TOKEN", "GH_TOKEN", "QDK_BRANCH"}
    }


def run(
    args: Sequence[str | os.PathLike[str]],
    *,
    env: Mapping[str, str] | None = None,
    check: bool = True,
    interactive: bool = False,
    input_text: str | None = None,
) -> subprocess.CompletedProcess[str]:
    command = [os.fspath(arg) for arg in args]
    try:
        result = subprocess.run(
            command,
            env=dict(env) if env is not None else make_process_env(),
            input=input_text,
            text=True,
            check=False,
            stdout=None if interactive else subprocess.PIPE,
            stderr=None if interactive else subprocess.PIPE,
        )
    except OSError:
        raise LauncherError(
            f"Cannot execute {command[0]}; check prerequisites."
        ) from None
    if check and result.returncode:
        raise LauncherError(
            f"{command[0]} failed (exit {result.returncode}); command output suppressed."
        )
    return result


def make_private_dir(path: Path) -> Path:
    path = raise_if_link(path)
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    if not path.is_dir():
        raise LauncherError("Instance state must be a directory.")
    if os.name != "nt" and path.stat().st_uid != os.getuid():
        raise LauncherError("Instance state must be owned by the current user.")
    path.chmod(0o700)
    return path


# Write to a private temp file and then copy that to the destination
def write_atomic(path: Path, content: bytes) -> None:
    raise_if_link(path)
    descriptor, temporary = tempfile.mkstemp(prefix=".write-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(content)
        raise_if_link(path)
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def read_json(path: Path) -> dict:
    raise_if_link(path)
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (ValueError, OSError):
        raise LauncherError(f"Invalid JSON state: {path.name}") from None
    if not isinstance(data, dict):
        raise LauncherError(f"Expected a JSON object: {path.name}")
    return data


def write_json(path: Path, data: dict) -> None:
    write_atomic(path, (json.dumps(data, indent=2) + "\n").encode())
