"""Per-user instances and an OS-held, cross-platform session lock."""

from __future__ import annotations

from contextlib import contextmanager
import os
from pathlib import Path
import re
from typing import Generator

from .common import LauncherError, make_private_dir, raise_if_link, read_json


def validate_sandbox_instance_name(name: str) -> str:
    reserved = {
        "con",
        "prn",
        "aux",
        "nul",
        *(f"com{number}" for number in range(1, 10)),
        *(f"lpt{number}" for number in range(1, 10)),
    }
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,39}", name) or name in reserved:
        raise ValueError(
            "Instance names need 1-40 lowercase letters, digits, '_' or '-', starting with a letter or digit."
        )
    return name


def get_sandbox_state_dir() -> Path:
    return raise_if_link(Path.home() / "qdk-copilot-sandbox")


def get_sandbox_instance_dir(instance_name: str) -> Path:
    return raise_if_link(
        get_sandbox_state_dir() / validate_sandbox_instance_name(instance_name)
    )


def get_sandbox_instance_config(sandbox_instance_dir: Path) -> dict:
    config = read_json(sandbox_instance_dir / "config.json")
    allowed = {"cpus", "memory", "to_copilot", "from_copilot", "repo"}
    if set(config) - allowed or any(
        not isinstance(value, str) for value in config.values()
    ):
        raise LauncherError("Instance configuration has unsupported keys or values.")
    return config


# Lock access to the instance state
@contextmanager
def session_lock(
    root: Path, filename: str = ".launcher.lock"
) -> Generator[None, None, None]:
    make_private_dir(root)
    if not re.fullmatch(r"\.[a-z0-9_-]+\.lock", filename):
        raise ValueError("Invalid launcher lock name.")
    lock_path = raise_if_link(root / filename)
    descriptor = os.open(
        lock_path, os.O_RDWR | os.O_CREAT | getattr(os, "O_NOFOLLOW", 0), 0o600
    )
    with os.fdopen(descriptor, "r+b", buffering=0) as stream:
        if os.name != "nt":
            os.fchmod(stream.fileno(), 0o600)
        try:
            if os.name == "nt":
                import msvcrt

                if stream.seek(0, os.SEEK_END) == 0:
                    stream.write(b"\0")
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            raise LauncherError(
                "Another sandbox launcher is active; exit that session first."
            ) from None
        try:
            yield
        finally:
            if os.name == "nt":
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(stream.fileno(), fcntl.LOCK_UN)
