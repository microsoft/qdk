"""Validate host exchange mounts and neutralize Git metadata in agent output."""

from __future__ import annotations

import os
from pathlib import Path
import uuid

from .common import is_link, raise_if_link


def validate_exchange_directories(
    incoming: Path,
    outgoing: Path,
    repo: Path,
    instance: Path,
    git_dirs: tuple[Path, ...],
    *,
    home: Path | None = None,
) -> tuple[Path, Path]:
    home = home or Path.home()
    paths = (raise_if_link(incoming), raise_if_link(outgoing))
    protected = tuple(raise_if_link(path) for path in (repo, instance, *git_dirs))
    for path in paths:
        if path == Path(path.anchor) or path == home:
            raise ValueError("Exchange folders cannot be a filesystem root or home.")
        if any(path == item or path in item.parents for item in protected):
            raise ValueError(
                "Exchange folders cannot contain the repo, instance, or Git directories."
            )
        if any(item in path.parents for item in protected[1:]):
            raise ValueError(
                "Exchange folders cannot be inside instance or Git directories."
            )
        if any(part.casefold() == ".git" for part in path.parts):
            raise ValueError("Exchange folders cannot be inside .git.")
        if path.exists() and not path.is_dir():
            raise ValueError("Exchange paths must be directories.")
    if (
        paths[0] == paths[1]
        or paths[0] in paths[1].parents
        or paths[1] in paths[0].parents
    ):
        raise ValueError("Exchange folders cannot be equal or nested.")
    return paths


def sanitize_exchange_folder(path: Path) -> None:
    raise_if_link(path)
    with os.scandir(path) as entries:
        for entry in entries:
            item = Path(entry.path)
            if is_link(item):
                if not entry.is_symlink() and entry.is_dir(follow_symlinks=False):
                    item.rmdir()
                else:
                    item.unlink()
                continue
            is_git = (
                entry.name.casefold() == ".git"
                if os.name == "nt"
                else entry.name == ".git"
            )
            if is_git:
                destination = item.with_name(f".git.quarantined-{uuid.uuid4().hex}")
                item.rename(destination)
                item = destination
            if entry.is_dir(follow_symlinks=False):
                sanitize_exchange_folder(item)
