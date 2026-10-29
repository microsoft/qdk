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
    build_identity,
    digest,
    file_digest,
    git,
    read_json,
    tree_digest,
    write_json,
)

OFFLINE_MARKERS = ("--offline was specified", "you're using offline mode")


def offline_failure(log, sha, action):
    """A clear error when Cargo failed only because the offline cache lacks crates."""
    text = Path(log).read_text(errors="replace")
    if any(marker in text for marker in OFFLINE_MARKERS):
        return ReviewError(
            f"{action} at {sha[:12]} needs crates missing from the offline Cargo cache; "
            f"the sandbox image must provide them. See {log}"
        )
    return None


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


def verify_build(build):
    root = Path(build["overlay"])
    if tree_digest(root / "qdk") != build["package_digest"]:
        raise ReviewError(f"Build package changed: {root}")
    worktree = Path(build["worktree"])
    if git(worktree, "rev-parse", "HEAD") != build["sha"]:
        raise ReviewError(f"Build worktree revision changed: {worktree}")
    if git(worktree, "status", "--porcelain"):
        raise ReviewError(f"Build worktree has tracked modifications: {worktree}")
    return build


def build_revision(config, sha, mode, tools):
    """Overlays are keyed by build identity, so a toolchain change builds alongside older overlays."""
    work = Path(config["build_dir"])
    work.mkdir(parents=True, exist_ok=True)
    identity = build_identity(tools)
    with (work / "build.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        root = work / "builds" / f"{sha}-{mode}-{digest(identity)[:12]}"
        manifest = root / "manifest.json"
        if manifest.exists():
            cached = read_json(manifest)
            if (cached["sha"], cached["mode"], cached["identity"]) != (sha, mode, identity):
                raise ReviewError(f"Build identity differs from its cache path: {root}")
            return verify_build(cached)
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
        log = work / f"build-{root.name}.log"
        print(f"Building {sha[:12]} ({mode}); log: {log}", flush=True)
        with log.open("w") as stream:
            proc = subprocess.run(args, cwd=worktree, env=env, stdout=stream, stderr=stream)
        if proc.returncode:
            raise offline_failure(log, sha, "Building") or ReviewError(f"Build failed ({proc.returncode}); see {log}")
        native = target / mode / "libqdk.so"
        if not native.is_file():
            raise ReviewError(f"Build produced no {native}")
        root.parent.mkdir(parents=True, exist_ok=True)
        staging = Path(tempfile.mkdtemp(prefix="overlay-", dir=root.parent))
        try:
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
                "identity": identity,
                "overlay": str(root),
                "worktree": str(worktree),
                "native_digest": file_digest(staging / "qdk/_native.abi3.so"),
                "package_digest": tree_digest(staging / "qdk"),
                "command": args,
                "environment": {"CARGO_INCREMENTAL": "0", "timestamps": "refreshed before build"},
                "log": str(log),
            }
            write_json(staging / "manifest.json", build)
            staging.rename(root)
        except BaseException:
            shutil.rmtree(staging, ignore_errors=True)
            raise
        return build


def worker_environment(config, build):
    env = dict(os.environ)
    for key in ("PYTHONPATH", "QDK_LIB_OVERRIDE_MICROSOFT_QDK"):
        env.pop(key, None)
    env.update(
        PYTHONPATH=build["overlay"],
        PYTHONNOUSERSITE="1",
    )
    if build.get("worktree"):
        env["QDK_LIB_OVERRIDE_MICROSOFT_QDK"] = build["worktree"]
    return env
