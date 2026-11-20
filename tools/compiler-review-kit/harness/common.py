# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Configuration, provenance, and process boundaries shared by the CLI."""

import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time


KIT = Path(__file__).resolve().parents[1]
WORK_ROOT = Path("/work/compiler-review-kit")
PROFILES = ["Base", "Adaptive_RI", "Adaptive_RIF", "Adaptive"]
DEFAULT_PROFILES = ["Base", "Adaptive_RIF", "Adaptive"]
DEFAULT_LIMITS = {
    "jobs": 2,
    "compile_seconds": 300,
    "simulate_seconds": 120,
    "runner_seconds": 120,
    "memory_mb": 16384,
    "shots": 8,
    "seed": 1,
}
# Left outside worker address-space caps for the controller, the Copilot CLI, and page cache.
MEMORY_HEADROOM_MB = 2048
# Below this per-worker cap, fewer parallel jobs are preferable to more exhaustion failures.
WORKER_FLOOR_MB = 4096


class ReviewError(Exception):
    """An explicit configuration, infrastructure, or provenance failure."""


def _object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ReviewError(f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path):
    with Path(path).open(encoding="utf-8") as stream:
        return json.load(stream, object_pairs_hook=_object)


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(value, indent=2, allow_nan=False) + "\n"
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        try:
            stream.write(text)
            stream.close()
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def validate(value, schema, label="$"):
    """Validate the JSON Schema keywords used by the two bundled schemas."""
    kinds = {
        "object": lambda v: isinstance(v, dict),
        "array": lambda v: isinstance(v, list),
        "string": lambda v: isinstance(v, str),
        "integer": lambda v: type(v) is int,
        "boolean": lambda v: type(v) is bool,
    }
    kind = schema.get("type")
    if kind and not kinds[kind](value):
        raise ReviewError(f"{label}: expected {kind}")
    if "enum" in schema and value not in schema["enum"]:
        raise ReviewError(f"{label}: expected one of {schema['enum']}")
    if isinstance(value, dict):
        missing = set(schema.get("required", [])) - value.keys()
        if missing:
            raise ReviewError(f"{label}: missing {sorted(missing)}")
        properties = schema.get("properties", {})
        if schema.get("additionalProperties") is False:
            unknown = value.keys() - properties.keys()
            if unknown:
                raise ReviewError(f"{label}: unknown fields {sorted(unknown)}")
        for key, child in value.items():
            if key in properties:
                validate(child, properties[key], f"{label}.{key}")
    if isinstance(value, list):
        if len(value) < schema.get("minItems", 0):
            raise ReviewError(f"{label}: too few items")
        if schema.get("uniqueItems"):
            encoded = [json.dumps(v, sort_keys=True) for v in value]
            if len(set(encoded)) != len(value):
                raise ReviewError(f"{label}: duplicate items")
        for index, child in enumerate(value):
            validate(child, schema.get("items", {}), f"{label}[{index}]")
    if isinstance(value, str):
        if len(value) < schema.get("minLength", 0):
            raise ReviewError(f"{label}: empty string")
        if "pattern" in schema and not re.search(schema["pattern"], value):
            raise ReviewError(f"{label}: invalid format")
    if type(value) is int and value < schema.get("minimum", value):
        raise ReviewError(f"{label}: below minimum")


def command(args, cwd=None, env=None):
    proc = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True)
    if proc.returncode:
        raise ReviewError(
            f"Command failed ({proc.returncode}): {' '.join(map(str, args))}\n"
            f"{proc.stderr or proc.stdout}"
        )
    return proc.stdout.strip()


def git(repo, *args):
    return command(["git", "--no-pager", "-C", str(repo), *args])


def resolve_ref(repo, ref):
    return git(repo, "rev-parse", "--verify", "--end-of-options", f"{ref}^{{commit}}")


def is_ancestor(repo, older, newer):
    proc = subprocess.run(
        ["git", "-C", str(repo), "merge-base", "--is-ancestor", older, newer],
        capture_output=True,
        text=True,
    )
    if proc.returncode not in (0, 1):
        raise ReviewError(proc.stderr)
    return proc.returncode == 0


def container_memory_mb(path="/sys/fs/cgroup/memory.max"):
    """The container's cgroup v2 memory limit in MiB, or None when unlimited or unknown."""
    try:
        text = Path(path).read_text().strip()
    except OSError:
        return None
    return int(text) // (1024 * 1024) if text.isdigit() else None


def host_memory_mb(path="/proc/meminfo"):
    """Total machine (or VM) memory in MiB, or None when unknown."""
    try:
        for line in Path(path).read_text().splitlines():
            if line.startswith("MemTotal:"):
                return int(line.split()[1]) // 1024
    except (OSError, ValueError, IndexError):
        pass
    return None


def available_cpus(path="/sys/fs/cgroup/cpu.max"):
    """CPUs this process may use: its affinity, reduced by a cgroup v2 CPU quota."""
    count = len(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else os.cpu_count()
    try:
        quota, period = Path(path).read_text().split()
        if quota != "max":
            count = min(count or 1, max(1, int(quota) // int(period)))
    except (OSError, ValueError):
        pass
    return count


def sized_limits(requested, memory_mb, cpus):
    """Default to one job per CPU, then fit worker address-space caps inside memory.

    RLIMIT_AS then trips before the OOM killer, which picks its victim arbitrarily (possibly
    the controller or Copilot itself); a capped worker fails alone as resource exhaustion.
    """
    limits = DEFAULT_LIMITS | requested
    if "jobs" not in requested and cpus:
        limits["jobs"] = cpus
    if memory_mb is None:
        return limits, []
    budget = max(256, memory_mb - MEMORY_HEADROOM_MB)
    if "jobs" not in requested:
        limits["jobs"] = max(1, min(limits["jobs"], budget // WORKER_FLOOR_MB))
    if "memory_mb" not in requested:
        limits["memory_mb"] = max(256, min(limits["memory_mb"], budget // limits["jobs"]))
    if limits["jobs"] * limits["memory_mb"] > budget:
        return limits, [
            f"{limits['jobs']} jobs x {limits['memory_mb']} MiB exceeds the available {memory_mb} MiB "
            f"less {MEMORY_HEADROOM_MB} MiB headroom; the OOM killer may stop arbitrary processes."
        ]
    if limits["memory_mb"] < WORKER_FLOOR_MB:
        return limits, [
            f"The available {memory_mb} MiB allows only {limits['memory_mb']} MiB per worker; "
            "expect resource_exhausted results. Raise the sandbox memory limit if possible."
        ]
    return limits, []


def load_config(path):
    path = Path(path).resolve()
    raw = read_json(path)
    validate(raw, read_json(KIT / "review.schema.json"))
    base = path.parent
    repo = (base / raw.get("repository", "/work/qdk")).resolve()
    refs = {
        name: resolve_ref(repo, raw[name])
        for name in ("reference", "candidate", "historical_reference", "previously_reviewed")
        if name in raw
    }
    # Intermediate heads of a PR stack: extra columns named layer1, layer2, ... (oldest first).
    layers = []
    for index, ref in enumerate(raw.get("layers", []), 1):
        role = f"layer{index}"
        refs[role] = resolve_ref(repo, ref)
        layers.append({"role": role, "ref": ref, "sha": refs[role]})
    name = raw.get("name") or re.sub(r"[^A-Za-z0-9_.-]", "-", Path(raw.get("report", "review.md")).stem) or "review"
    if name in (".", ".."):
        raise ReviewError("Cannot derive a review name from the report path; set name explicitly.")
    report = (base / raw.get("report", f"/exchange/from_copilot/{name}.md")).resolve()
    cache_root = raw.get("cache_dir") or os.environ.get("REVIEW_KIT_CACHE") or "/work/cache/compiler-review-kit"
    resources = {"cpus": available_cpus(), "memory_mb": container_memory_mb(), "memory_source": "cgroup"}
    if resources["memory_mb"] is None:
        resources.update(memory_mb=host_memory_mb(), memory_source="host")
    limits, limit_warnings = sized_limits(raw.get("limits", {}), resources["memory_mb"], resources["cpus"])
    clients = []
    for client in raw.get("clients", []):
        root = (base / client["path"]).resolve()
        clients.append(
            {
                "name": client["name"],
                "path": str(root),
                "sha": resolve_ref(root, "HEAD"),
                "status": git(root, "status", "--porcelain"),
            }
        )
    return {
        "input": raw,
        "config_path": str(path),
        "repository": str(repo),
        "refs": refs,
        "venv": str((base / raw.get("venv", "/opt/qdk-venv")).resolve()),
        "work_dir": str((base / raw.get("work_dir", str(WORK_ROOT / "scratch" / name))).resolve()),
        "build_dir": str((base / raw.get("build_dir", str(WORK_ROOT / "build"))).resolve()),
        "report": str(report),
        "layers": layers,
        "name": name,
        "cache_dir": str((base / cache_root).resolve()),
        "cache_budget_mb": raw.get("cache_budget_mb", 50),
        "profiles": raw.get("profiles", DEFAULT_PROFILES),
        "debug_validation": raw.get("debug_validation", "none"),
        "keep_qir": raw.get("keep_qir", False),
        "limits": limits,
        "limit_warnings": limit_warnings,
        "resources": resources,
        "clients": clients,
        "checkout_status": git(repo, "status", "--porcelain"),
    }


def ordered_roles(refs):
    """Roles in chronological display order: historical, previous, reference, layers, candidate."""
    layers = sorted((r for r in refs if re.fullmatch(r"layer\d+", r)), key=lambda r: int(r[5:]))
    order = ["historical_reference", "previously_reviewed", "reference", *layers, "candidate"]
    return [role for role in order if role in refs] + [r for r in refs if r not in order]


def select_roles(config, extra=None):
    """Reference and candidate always; other configured roles unless `extra` narrows them."""
    roles = ordered_roles(config["refs"])
    if extra is None:
        return roles
    unknown = [role for role in extra if role not in config["refs"]]
    if unknown:
        raise ReviewError(f"Configuration has no {', '.join(unknown)}")
    return [role for role in roles if role in ("reference", "candidate") or role in extra]


def tool_version(args):
    """First line of a version report; qir-runner prints its version to stderr."""
    proc = subprocess.run(args, capture_output=True, text=True)
    text = proc.stdout.strip() or proc.stderr.strip()
    if proc.returncode or not text:
        raise ReviewError(f"Cannot read a version ({proc.returncode}): {' '.join(map(str, args))}\n{proc.stderr}")
    return text.splitlines()[0]


def preflight(config):
    """Tool provenance; `build_identity` selects the part that determines a native overlay."""
    venv = Path(config["venv"])
    for name in ("python", "qir-runner"):
        path = venv / "bin" / name
        if not path.is_file() or not os.access(path, os.X_OK):
            raise ReviewError(f"Missing executable {path}; do not silently reduce coverage.")
    metadata = {
        "python": tool_version([str(venv / "bin/python"), "--version"]),
        "runner": tool_version([str(venv / "bin/qir-runner"), "--version"]),
        "rustc": tool_version(["rustc", "--version"]),
        "cargo": tool_version(["cargo", "--version"]),
        "platform": command(["uname", "-sm"]),
        "packages": json.loads(
            command(
                [
                    str(venv / "bin/python"),
                    "-c",
                    "import importlib.metadata as m,json; "
                    "print(json.dumps(sorted((d.metadata['Name'],d.version) for d in m.distributions())))",
                ]
            )
        ),
        "build_environment": {
            key: os.environ[key]
            for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER")
            if key in os.environ
        },
        "resources": config.get("resources"),
    }
    return metadata


BUILD_IDENTITY = ("python", "rustc", "cargo", "platform", "build_environment")


def build_identity(tools):
    """Tools that determine a native overlay; the runner and other packages only affect results."""
    return {key: tools.get(key) for key in BUILD_IDENTITY}


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def file_digest(path):
    checksum = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            checksum.update(chunk)
    return checksum.hexdigest()


def tree_digest(root):
    root = Path(root)
    return digest(
        [
            (str(p.relative_to(root)), file_digest(p))
            for p in sorted(root.rglob("*"))
            if p.is_file() and "__pycache__" not in p.parts and p.suffix != ".pyc"
        ]
    )


def oom_kills(path="/sys/fs/cgroup/memory.events"):
    """The container's cgroup v2 OOM-kill count, or None when unavailable."""
    try:
        for line in Path(path).read_text().splitlines():
            name, _, value = line.partition(" ")
            if name == "oom_kill":
                return int(value)
    except (OSError, ValueError):
        pass
    return None


def oom_killed(before, after):
    """Whether the container's OOM killer fired between two counter reads, or None if unknown."""
    return None if before is None or after is None else after > before


def failure_status(returncode, stderr, oom=None):
    """Classify an abnormal exit; memory exhaustion and outside interruption are inconclusive, not crashes.

    The OOM killer and a person both send SIGKILL; `oom` (from the cgroup counter) tells them apart
    when available. Genuine crashes arrive as SIGSEGV, SIGABRT, SIGBUS, SIGILL, or SIGFPE.
    """
    if re.search(r"memory allocation of \d+ bytes failed", stderr or ""):
        return "resource_exhausted"
    if returncode == -signal.SIGKILL:
        return "interrupted" if oom is False else "resource_exhausted"
    if returncode in (-signal.SIGTERM, -signal.SIGINT, -signal.SIGHUP):
        return "interrupted"
    return "crash" if returncode < 0 else "infrastructure_error"


def supervise(args, *, input_text=None, env, cwd, timeout):
    """Capture a process and own its entire process group's lifetime."""
    start = time.monotonic()
    kills = oom_kills()
    proc = subprocess.Popen(
        args,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
        cwd=cwd,
        start_new_session=True,
    )
    try:
        stdout, stderr = proc.communicate(input_text, timeout=timeout)
    except subprocess.TimeoutExpired:
        termination = "terminated process group"
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            termination = "process exited while the timeout was being handled"
        try:
            stdout, stderr = proc.communicate(timeout=1)
        except subprocess.TimeoutExpired:
            termination = "killed process group"
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout, stderr = proc.communicate()
        return {
            "status": "timeout",
            "stdout": stdout,
            "stderr": stderr,
            "seconds": time.monotonic() - start,
            "termination": termination,
            "returncode": proc.returncode,
        }
    finally:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    elapsed = time.monotonic() - start
    if proc.returncode:
        oom = oom_killed(kills, oom_kills())
        status = failure_status(proc.returncode, stderr, oom)
        result = {"status": status, "returncode": proc.returncode, "stdout": stdout, "stderr": stderr,
                  "seconds": elapsed}
        if status == "resource_exhausted":
            result["message"] = (
                "killed by the container's OOM killer" if oom
                else "killed by SIGKILL or a failed allocation (likely memory exhaustion)"
            )
        elif status == "interrupted":
            result["message"] = f"stopped by signal {-proc.returncode} from outside the harness"
        return result
    return {"status": "ok", "returncode": proc.returncode, "stdout": stdout, "stderr": stderr, "seconds": elapsed}


def isolated(args, *, payload, env, cwd, timeout):
    observed = supervise(args, input_text=json.dumps(payload), env=env, cwd=cwd, timeout=timeout)
    if observed["status"] != "ok":
        return observed
    try:
        result = json.loads(observed["stdout"])
    except json.JSONDecodeError:
        return {
            "status": "infrastructure_error",
            "message": "Worker did not produce one JSON result",
            "stdout": observed["stdout"],
            "stderr": observed["stderr"],
            "seconds": observed["seconds"],
        }
    if not isinstance(result, dict) or "status" not in result:
        raise ReviewError(f"Invalid worker result: {result!r}")
    return result | {"stderr": observed["stderr"], "seconds": observed["seconds"]}
