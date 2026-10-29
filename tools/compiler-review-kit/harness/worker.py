# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""One simulator, compiler, or runner stage per disposable subprocess."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import resource
import subprocess
import sys
import traceback

if not __package__:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from harness.common import ReviewError, failure_status, file_digest  # noqa: E402
from harness.records import counts, encode, parse_output  # noqa: E402


def load_case(ctx, case):
    if "source" in case:
        ctx.eval(case["source"])
    args, entry = (), case.get("entry")
    if "adapter" in case:
        spec = importlib.util.spec_from_file_location("review_adapter", case["adapter"])
        if spec is None or spec.loader is None:
            raise ReviewError(f"Cannot load adapter: {case['adapter']}")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        entry, args = module.prepare(ctx)
        if not isinstance(args, (tuple, list)):
            raise ReviewError("Adapter prepare(ctx) must return (entry, argument_sequence)")
    if entry is None:
        raise ReviewError("Case has no entry expression or callable")
    return entry, args


def failure(error):
    if isinstance(error, (KeyboardInterrupt, SystemExit)):
        raise error
    name = type(error).__name__
    if name == "PanicException":
        return {"status": "panic", "message": str(error)}
    if name == "QSharpError":
        return {"status": "diagnostic", "message": str(error)}
    if isinstance(error, MemoryError):
        return {"status": "resource_exhausted", "message": f"MemoryError: {error}"}
    return {"status": "infrastructure_error", "message": f"{name}: {error}", "traceback": traceback.format_exc()}


def execute(job):
    cap = job["memory_mb"] * 1024 * 1024
    resource.setrlimit(resource.RLIMIT_AS, (cap, cap))
    if job["stage"] == "runner":
        # qir-runner reads stdin only from 0.9.6; a file argument works with every release.
        proc = subprocess.run(
            [job["runner"], "-f", job["qir_path"], "-s", str(job["shots"]), "-r", str(job["seed"])],
            capture_output=True,
            text=True,
        )
        result = {"stdout": proc.stdout, "runner_stderr": proc.stderr, "returncode": proc.returncode}
        if proc.returncode:
            status = failure_status(proc.returncode, proc.stderr)
            return result | {"status": {"crash": "runner_crash", "infrastructure_error": "runner_error"}.get(status, status)}
        try:
            values = parse_output(proc.stdout, job["shots"])
        except ReviewError as error:
            return result | {"status": "parser_error", "message": str(error)}
        return result | {"status": "ok", "values": values, "counts": counts(values)}

    import qdk
    import qdk._native

    overlay = Path(job["build"]["overlay"]).resolve()
    if Path(qdk.__file__).resolve() != overlay / "qdk/__init__.py":
        raise ReviewError(f"Wrong qdk imported: {qdk.__file__}")
    if Path(qdk._native.__file__).resolve() != Path(job["build"].get(
        "native_path", str(overlay / "qdk/_native.abi3.so")
    )).resolve():
        raise ReviewError(f"Wrong native module imported: {qdk._native.__file__}")
    if file_digest(qdk._native.__file__) != job["build"]["native_digest"]:
        raise ReviewError("Native module digest differs from the build manifest")
    profile = "Unrestricted" if job["stage"] == "simulate" else job["profile"]
    if not hasattr(qdk.TargetProfile, profile):
        return {"status": "unsupported_profile", "message": f"Profile unavailable: {profile}"}
    case = job["case"]
    ctx = qdk.Context(
        target_profile=getattr(qdk.TargetProfile, profile),
        project_root=case.get("project_root"),
        language_features=case.get("language_features"),
    )
    ctx.set_classical_seed(job["seed"])
    ctx.set_quantum_seed(job["seed"])
    entry, args = load_case(ctx, case)
    if job["stage"] == "compile":
        return {"status": "ok", "qir": str(ctx.compile(entry, *args))}
    values = ctx.run(entry, job["shots"], *args, seed=job["seed"])
    for value in values:
        if isinstance(value, Exception):
            return {"status": "runtime_error", "message": str(value)}
    if len(values) != job["shots"]:
        raise ReviewError("Simulator returned the wrong number of shots")
    encoded = [encode(value) for value in values]
    return {"status": "ok", "values": encoded, "counts": counts(encoded)}


def main():
    job = json.load(sys.stdin)
    captured = io.StringIO()
    try:
        with contextlib.redirect_stdout(captured):
            result = execute(job)
    except BaseException as error:
        result = failure(error)
    result["messages"] = captured.getvalue()
    print(json.dumps(result, allow_nan=False))


if __name__ == "__main__":
    main()
