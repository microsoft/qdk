# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Pinned Linux native overlays; Python sources always match the native build."""

import fcntl
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from harness.common import (
    ReviewError,
    file_digest,
    git,
    read_json,
    tree_digest,
    write_json,
)


def checkout_revision(config, sha):
    worktree = Path(config["build_dir"]) / "worktrees" / sha
    if not worktree.exists():
        worktree.parent.mkdir(parents=True, exist_ok=True)
        git(config["repository"], "worktree", "add", "--detach", str(worktree), sha)
    elif git(worktree, "rev-parse", "HEAD") != sha:
        raise ReviewError(f"Unexpected worktree at {worktree}")
    if git(worktree, "status", "--porcelain"):
        raise ReviewError(f"Refusing to use a modified worktree: {worktree}")
    return worktree


def refresh_timestamps(worktree):
    """Make every checked-out file newer than any artifact in a shared Cargo target.

    Cargo hashes path dependencies by their workspace-relative path, so the same crate
    from two worktrees maps to the same artifact name. Its freshness check compares
    source mtimes with that artifact; a worktree created before another revision was
    built would otherwise reuse the other revision's stale artifacts (seen as E0061
    argument-count errors). Third-party dependencies remain shared and cached.
    """
    worktree = Path(worktree)
    for path in worktree.rglob("*"):
        if ".git" in path.relative_to(worktree).parts or path.is_symlink() or not path.is_file():
            continue
        os.utime(path, None)


def verify_build(build, tools):
    root = Path(build["overlay"])
    if build["tools"] != tools:
        raise ReviewError(f"Build environment changed for {root}; use a new build_dir.")
    if tree_digest(root / "qdk") != build["package_digest"]:
        raise ReviewError(f"Build package changed: {root}")
    worktree = Path(build["worktree"])
    if git(worktree, "rev-parse", "HEAD") != build["sha"]:
        raise ReviewError(f"Build worktree revision changed: {worktree}")
    if git(worktree, "status", "--porcelain"):
        raise ReviewError(f"Build worktree has tracked modifications: {worktree}")
    if tree_digest(worktree / "library") != build["library_digest"]:
        raise ReviewError(f"Build library inputs changed: {worktree}")
    return build


def build_revision(config, sha, mode, tools):
    work = Path(config["build_dir"])
    work.mkdir(parents=True, exist_ok=True)
    with (work / "build.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        root = work / "builds" / f"{sha}-{mode}"
        manifest = root / "manifest.json"
        if manifest.exists():
            cached = read_json(manifest)
            if cached["sha"] != sha or cached["mode"] != mode:
                raise ReviewError(f"Build identity differs from its cache path: {root}")
            return verify_build(cached, tools)
        if root.exists():
            raise ReviewError(f"Incomplete build directory {root}; inspect it before removing it.")
        worktree = checkout_revision(config, sha)
        target = work / "cargo-target"
        refresh_timestamps(worktree)
        env = dict(
            os.environ,
            PYO3_PYTHON=str(Path(config["venv"]) / "bin/python"),
            CARGO_TARGET_DIR=str(target),
            CARGO_INCREMENTAL="0",
        )
        args = ["cargo", "build", "--locked", "-p", "qdk", "--lib"]
        if mode == "release":
            args.append("--release")
        log = work / f"build-{sha}-{mode}.log"
        print(f"Building {sha[:12]} ({mode}); log: {log}", flush=True)
        with log.open("w") as stream:
            proc = subprocess.run(args, cwd=worktree, env=env, stdout=stream, stderr=stream)
        if proc.returncode:
            raise ReviewError(f"Build failed ({proc.returncode}); see {log}")
        native = target / mode / "libqdk.so"
        if not native.is_file():
            raise ReviewError(f"Build produced no {native}")
        root.parent.mkdir(parents=True, exist_ok=True)
        staging = Path(tempfile.mkdtemp(prefix="overlay-", dir=root.parent))
        package = worktree / "source/qdk_package/qdk"
        shutil.copytree(
            package,
            staging / "qdk",
            ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "_native*.so"),
        )
        shutil.copy2(native, staging / "qdk/_native.abi3.so")
        build = {
            "sha": sha,
            "mode": mode,
            "overlay": str(root),
            "worktree": str(worktree),
            "native_digest": file_digest(staging / "qdk/_native.abi3.so"),
            "package_digest": tree_digest(staging / "qdk"),
            "library_digest": tree_digest(worktree / "library"),
            "tools": tools,
            "command": args,
            "environment": {"CARGO_INCREMENTAL": "0", "timestamps": "refreshed before build"},
            "log": str(log),
        }
        write_json(staging / "manifest.json", build)
        staging.rename(root)
        return build


def worker_environment(config, build):
    env = dict(os.environ)
    for key in ("PYTHONPATH", "QDK_LIB_OVERRIDE_MICROSOFT_QDK"):
        env.pop(key, None)
    env.update(
        PYTHONPATH=build["overlay"],
        PYTHONNOUSERSITE="1",
        QDK_LIB_OVERRIDE_MICROSOFT_QDK=build["worktree"],
    )
    return env
