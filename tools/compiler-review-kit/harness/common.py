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
import sys
import tempfile
import time


KIT = Path(__file__).resolve().parents[1]
PROFILES = ["Base", "Adaptive_RI", "Adaptive_RIF", "Adaptive"]
DEFAULT_LIMITS = {
    "jobs": 2,
    "compile_seconds": 300,
    "simulate_seconds": 120,
    "runner_seconds": 120,
    "memory_mb": 16384,
    "shots": 8,
    "seed": 1,
}


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
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


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
    work_root = Path(tempfile.gettempdir()) / "compiler-review-kit" / name
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
        "work_dir": str((base / raw.get("work_dir", str(work_root))).resolve()),
        "build_dir": str((base / raw.get("build_dir", "/work/compiler-review-kit/build")).resolve()),
        "report": str(report),
        "layers": layers,
        "name": name,
        "cache_dir": str((base / cache_root).resolve()),
        "cache_budget_mb": raw.get("cache_budget_mb", 50),
        "profiles": raw.get("profiles", PROFILES),
        "debug_validation": raw.get("debug_validation", "targeted"),
        "keep_qir": raw.get("keep_qir", False),
        "limits": DEFAULT_LIMITS | raw.get("limits", {}),
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


def preflight(config):
    if sys.platform != "linux":
        raise ReviewError("WARNING: review-kit supports Linux only.")
    venv = Path(config["venv"])
    for name in ("python", "qir-runner"):
        path = venv / "bin" / name
        if not path.is_file() or not os.access(path, os.X_OK):
            raise ReviewError(f"WARNING: missing executable {path}; do not silently reduce coverage.")
    metadata = {
        "python": command([str(venv / "bin/python"), "--version"]),
        "runner": command([str(venv / "bin/qir-runner"), "--version"]),
        "rustc": command(["rustc", "--version"]),
        "cargo": command(["cargo", "--version"]),
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
    }
    return metadata


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


def isolated(args, *, payload, env, cwd, timeout):
    """A new process group allows a timeout to kill the worker and its runner."""
    start = time.monotonic()
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
        stdout, stderr = proc.communicate(json.dumps(payload), timeout=timeout)
    except subprocess.TimeoutExpired:
        termination = "killed worker process group"
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            termination = "worker exited while the timeout was being handled"
        stdout, stderr = proc.communicate()
        return {
            "status": "timeout",
            "stdout": stdout,
            "stderr": stderr,
            "seconds": time.monotonic() - start,
            "termination": termination,
        }
    elapsed = time.monotonic() - start
    if proc.returncode:
        return {
            "status": "crash" if proc.returncode < 0 else "infrastructure_error",
            "returncode": proc.returncode,
            "stdout": stdout,
            "stderr": stderr,
            "seconds": elapsed,
        }
    try:
        result = json.loads(stdout)
    except json.JSONDecodeError:
        return {
            "status": "infrastructure_error",
            "message": "Worker did not produce one JSON result",
            "stdout": stdout,
            "stderr": stderr,
            "seconds": elapsed,
        }
    if not isinstance(result, dict) or "status" not in result:
        raise ReviewError(f"Invalid worker result: {result!r}")
    return result | {"stderr": stderr, "seconds": elapsed}
