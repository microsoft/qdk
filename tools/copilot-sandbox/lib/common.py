"""Small, non-secret command and private-file helpers."""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
from typing import Mapping, Sequence
from urllib.parse import urlsplit, urlunsplit


class LauncherError(Exception):
    """A user-facing failure; underlying errors use native exception chaining."""


_SECRET_VALUES: set[str] = set()


def register_secret(value: str) -> None:
    if value:
        _SECRET_VALUES.add(value)


def sanitize_diagnostic(text: str) -> str:
    secrets = _SECRET_VALUES | {
        value
        for key in ("GH_TOKEN", "COPILOT_GITHUB_TOKEN")
        if (value := os.environ.get(key))
    }
    for value in sorted(secrets, key=len, reverse=True):
        text = text.replace(value, "[redacted]")
    text = re.sub(r"(?:github_pat_|gh[pousr]_)[A-Za-z0-9_]+", "[redacted]", text)
    text = re.sub(
        r"(?im)\b(?:proxy-)?authorization\s*:[^\r\n]*",
        "Authorization: [redacted]",
        text,
    )
    text = re.sub(
        r"(?i)\b(?:[\w-]*token|password|client[_-]?secret)[\"']?\s*[=:]\s*"
        r"(?:\"[^\"]*\"|'[^']*'|[^\s,}]+)",
        "credential=[redacted]",
        text,
    )

    def redact_url(match: re.Match[str]) -> str:
        try:
            parts = urlsplit(match.group())
            return urlunsplit(
                (parts.scheme, parts.netloc.rsplit("@", 1)[-1], parts.path, "", "")
            )
        except ValueError:
            return "[redacted URL]"

    text = re.sub(r"https?://[^\s<>\"']+", redact_url, text)
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    text = "".join(
        character
        for character in text
        if character.isprintable() or character in "\n\t"
    ).strip()
    return text if len(text) <= 2000 else text[:2000] + "\n[diagnostic truncated]"


def format_error(error: BaseException) -> str:
    messages = []
    seen: set[int] = set()
    current: BaseException | None = error
    while current is not None and id(current) not in seen:
        seen.add(id(current))
        message = str(current) or type(current).__name__
        if isinstance(current, EOFError):
            message = "Input closed while waiting for a response. Run from an interactive terminal."
        messages.append(sanitize_diagnostic(message))
        current = current.__cause__
    return "\nCaused by: ".join(messages)


def command_error(
    result: subprocess.CompletedProcess[str], *, operation: str | None = None
) -> LauncherError:
    program = Path(os.fspath(result.args[0])).stem
    message = f"{operation or program} failed (exit {result.returncode})."
    if program in {"docker", "git"} and result.stderr:
        message += "\n" + sanitize_diagnostic(result.stderr)
    return LauncherError(message)


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
    except OSError as error:
        raise LauncherError(
            f"Cannot execute {Path(command[0]).name}; check that it is installed and available on PATH."
        ) from error
    if check and result.returncode:
        if input_text is not None:
            raise LauncherError(
                f"{Path(command[0]).name} failed (exit {result.returncode}); output suppressed for a command with input."
            )
        raise command_error(result)
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
    except (ValueError, OSError) as error:
        raise LauncherError(f"Invalid JSON state: {path}") from error
    if not isinstance(data, dict):
        raise LauncherError(f"Expected a JSON object: {path.name}")
    return data


def write_json(path: Path, data: dict) -> None:
    write_atomic(path, (json.dumps(data, indent=2) + "\n").encode())
