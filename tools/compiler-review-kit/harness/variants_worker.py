# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Runs every variant of a trusted variants file at one revision: simulator, compile, qir-runner."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from harness.common import DEFAULT_PROFILES, ReviewError  # noqa: E402
from harness.records import encode, parse_output  # noqa: E402
from harness.tables import format_value  # noqa: E402


def load_variants(path):
    spec = importlib.util.spec_from_file_location("review_variants", path)
    if spec is None or spec.loader is None:
        raise ReviewError(f"Cannot load variants file: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    variants = getattr(module, "VARIANTS", None)
    if not isinstance(variants, list) or not variants:
        raise ReviewError("Variants file must define a non-empty VARIANTS list")
    names = [v.get("name") for v in variants]
    if any(not isinstance(n, str) or not n for n in names) or len(set(names)) != len(names):
        raise ReviewError("Every variant needs a unique non-empty name")
    return module, variants


def failure(error):
    kind = "panic" if type(error).__name__ == "PanicException" else (
        "diagnostic" if type(error).__name__ == "QSharpError" else "error")
    lines = [line.strip() for line in str(error).strip().splitlines() if line.strip()]
    message = lines[0] if lines else type(error).__name__
    causes = [line[3:].strip() for line in lines if line.startswith("`->")]
    if causes:
        message += ": " + causes[-1]
    return {"status": kind, "message": message[:300]}


def run_variant(qdk, module, variant, profiles, shots, seed, runner, project_root):
    source = getattr(module, "PRELUDE", "") + variant.get("source", variant.get("body", ""))

    def context(profile):
        ctx = qdk.Context(target_profile=getattr(qdk.TargetProfile, profile), project_root=project_root)
        if source.strip():
            ctx.eval(source)
        if "build" in variant:
            entry, args = variant["build"](ctx)
        else:
            entry, args = variant["entry"], ()
        return ctx, entry, tuple(args)

    out = {"name": variant["name"], "expected": variant.get("expected")}
    try:
        ctx, entry, args = context("Unrestricted")
        values = ctx.run(entry, shots, *args, seed=seed)
        out["simulator"] = {"status": "ok", "values": sorted({format_value(encode(v)) for v in values})}
    except BaseException as error:
        if isinstance(error, (KeyboardInterrupt, SystemExit)):
            raise
        out["simulator"] = failure(error)
    out["profiles"] = {}
    for profile in variant.get("profiles", profiles):
        try:
            ctx, entry, args = context(profile)
            qir = str(ctx.compile(entry, *args))
        except BaseException as error:
            if isinstance(error, (KeyboardInterrupt, SystemExit)):
                raise
            out["profiles"][profile] = failure(error)
            continue
        proc = subprocess.run([runner, "-s", str(shots), "-r", str(seed)], input=qir, capture_output=True, text=True)
        if proc.returncode:
            out["profiles"][profile] = {"status": "runner_error", "message": (proc.stderr or proc.stdout).strip()[:300]}
            continue
        try:
            values = parse_output(proc.stdout, shots)
        except ReviewError as error:
            out["profiles"][profile] = {"status": "parser_error", "message": str(error)[:300]}
            continue
        out["profiles"][profile] = {"status": "ok", "values": sorted({format_value(v) for v in values})}
    return out


def main():
    job = json.load(sys.stdin)
    import qdk

    module, variants = load_variants(job["variants"])
    project_root = None
    files = getattr(module, "PROJECT", None)
    if files:
        project_root = tempfile.mkdtemp(prefix="variants-project-")
        (Path(project_root) / "qsharp.json").write_text("{}")
        for name, text in files.items():
            path = Path(project_root) / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
    profiles = job["profiles"] or getattr(module, "PROFILES", None) or DEFAULT_PROFILES
    results = []
    with contextlib.redirect_stdout(io.StringIO()):
        for variant in variants:
            results.append(run_variant(qdk, module, variant, profiles, job["shots"], job["seed"], job["runner"],
                                       project_root))
    print(json.dumps({"status": "ok", "qdk": qdk.__file__, "profiles": profiles, "variants": results}))


if __name__ == "__main__":
    main()
