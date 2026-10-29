# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Run standalone repro scripts under pinned revision overlays and record panic provenance."""

from pathlib import Path
import re

from harness.builds import build_revision, worker_environment
from harness.common import ReviewError, git, ordered_roles, preflight, resolve_ref, supervise, write_json

PANIC = re.compile(r"panicked at ([^\s:]+):(\d+):(\d+):\n(.*)")
EXCEPTION = re.compile(r"^((?:[A-Za-z_][\w]*\.)*[A-Za-z_]\w*(?:Error|Exception)): ?(.*)$", re.M)
FUNCTION = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:const|async|unsafe|extern\s+\"[^\"]*\")\s+)*fn\s+([A-Za-z_]\w*)")


def parse_panics(stderr):
    """Distinct (path, line, column, first message line) panics, in order of appearance."""
    seen, panics = set(), []
    for match in PANIC.finditer(stderr or ""):
        key = (match.group(1), int(match.group(2)), int(match.group(3)), match.group(4).strip())
        if key not in seen:
            seen.add(key)
            panics.append({"path": key[0], "line": key[1], "column": key[2], "message": key[3]})
    return panics


def enclosing_function(text, line):
    """Name of the nearest `fn` declared at or above a 1-based line, or None."""
    lines = text.splitlines()
    for index in range(min(line, len(lines)) - 1, -1, -1):
        match = FUNCTION.match(lines[index])
        if match:
            return match.group(1)
    return None


def last_exception(output):
    matches = EXCEPTION.findall(output or "")
    return f"{matches[-1][0]}: {matches[-1][1]}".strip() if matches else None


def reproduction_status(stdout, exit_code):
    """Prefer the last printed verdict, including repros using the legacy exit convention."""
    for line in reversed(stdout.splitlines()):
        match = re.fullmatch(r"(NOT REPRODUCED|REPRODUCED)(?:[: ].*)?", line.strip())
        if match:
            return "not reproduced" if match.group(1) == "NOT REPRODUCED" else "reproduces"
    if exit_code == 0:
        return "not reproduced"
    return exit_code if isinstance(exit_code, str) else f"exit {exit_code}"


def resolve_targets(config, roles=None, refs=None):
    if refs:
        return [(ref, resolve_ref(config["repository"], ref)) for ref in refs]
    roles = roles or ordered_roles(config["refs"])
    missing = [role for role in roles if role not in config["refs"]]
    if missing:
        raise ReviewError(f"Configuration has no {', '.join(missing)}")
    return [(role, config["refs"][role]) for role in roles]


def prepare_builds(config, targets, mode, builds=None, tools=None):
    tools = preflight(config) if tools is None else tools
    builds = {} if builds is None else builds
    for _, sha in targets:
        if sha not in builds:
            builds[sha] = build_revision(config, sha, mode, tools)
    return builds


def exec_script(config, script, targets, mode, output_dir, timeout, args=(), builds=None):
    script = Path(script).resolve()
    if not script.is_file():
        raise ReviewError(f"Missing script: {script}")
    builds = prepare_builds(config, targets, mode) if builds is None else builds
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    python = str(Path(config["venv"]) / "bin/python")
    rows = []
    for label, sha in targets:
        build = builds[sha]
        env = worker_environment(config, build)
        observed = supervise([python, str(script), *args], cwd=script.parent, env=env, timeout=timeout)
        stdout, stderr = observed["stdout"], observed["stderr"]
        status = (observed["status"] if observed["status"] in ("timeout", "resource_exhausted", "interrupted")
                  else observed["returncode"])
        panics = parse_panics(stderr)
        for panic in panics:
            try:
                text = git(build["worktree"], "show", f"{sha}:{panic['path']}")
                panic["function"] = enclosing_function(text, panic["line"])
            except ReviewError:
                panic["function"] = None
        log = output_dir / f"{script.stem}-{sha[:12]}-{mode}.txt"
        log.write_text(
            f"# {script.name} at {label} {sha} ({mode}); exit={status}\n"
            f"# qdk overlay: {build['overlay']}\n\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
        )
        rows.append({
            "label": label, "sha": sha, "mode": mode, "exit": status,
            "reproduction": reproduction_status(stdout, status),
            "exception": last_exception(stderr), "panics": panics, "log": str(log),
            "stdout_tail": stdout[-2000:],
        })
    write_json(output_dir / f"{script.stem}.exec.json", {"script": str(script), "args": list(args), "rows": rows})
    return rows


def print_exec(rows):
    for row in rows:
        print(f"=== {row['label']} {row['sha'][:12]} ({row['mode']}): exit={row['exit']}")
        for panic in row["panics"]:
            where = f"{panic['path']}:{panic['line']}"
            if panic.get("function"):
                where += f" in {panic['function']}"
            print(f"  panic at {where}\n    {panic['message']}")
        if row["exception"] and not row["panics"]:
            print(f"  {row['exception']}")
        if row["exit"] == 0:
            tail = row["stdout_tail"].strip().splitlines()
            print("  stdout: " + (tail[-1][:200] if tail else "(empty)"))
        print(f"  log: {row['log']}")
