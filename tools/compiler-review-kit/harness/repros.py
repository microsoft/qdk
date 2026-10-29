# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Render portable, self-contained repros from frozen cases and the shared runtime."""

import inspect
from functools import lru_cache
from pathlib import Path
import pprint
import re
import tempfile

from harness import builds, common, records, runner, tables, worker
from harness.cases import bundle_cases
from harness.common import KIT, read_json


@lru_cache(maxsize=1)
def _runtime_source():
    functions = [
        common.file_digest, common.oom_kills, common.failure_status, common.supervise, common.isolated,
        builds.worker_environment, records.double, records.encode, records.counts, records.parse_output,
        tables.format_value, worker.import_overlay, worker.cap_memory, worker.load_case, worker.failure,
        worker.execute, runner.stage, runner.run_case,
    ]
    license_text = (KIT.parents[1] / "LICENSE.txt").read_text()
    notice = "\n".join("# " + line.strip() if line.strip() else "#" for line in license_text.splitlines())
    runtime = notice + "\n\n" + "\n\n".join(inspect.getsource(function) for function in functions)
    return runtime + "\n\n" + inspect.getsource(worker.main).replace("def main(", "def stage_main(", 1)


def render_repro(cases, expected, affected, title, doc="", suffixes=None, limits=None):
    with tempfile.TemporaryDirectory(prefix="review-repro-") as directory:
        root = Path(directory)
        bundle_cases(cases, Path.cwd(), root)
        entries = read_json(root / "cases.json")
        files = {str(path.relative_to(root)): path.read_bytes() for path in sorted(root.rglob("*"))
                 if path.is_file() and path != root / "cases.json"}
    description = (f"{title}\n\n{doc.strip()}\n"
                   "Exits 0 when the bug no longer reproduces.\n"
                   f"Reproduces when {', '.join(affected)} return something other than {expected}.\n"
                   "Exits 2 for incomplete or unvalidated execution.\n"
                   "Linux only. Uses qdk and qir-runner from the running interpreter's environment.")
    settings = common.DEFAULT_LIMITS | (limits or {})
    values = {
        "__DOCSTRING__": repr(description),
        "__CASES__": pprint.pformat(entries, width=100, sort_dicts=False),
        "__FILES__": pprint.pformat(files, width=100, sort_dicts=False),
        "__EXPECTED__": repr(expected),
        "__AFFECTED__": repr(affected),
        "__SUFFIXES__": repr(suffixes or [""] * len(entries)),
        "__PROFILES__": repr(entries[0]["profiles"]),
        "__SHOTS__": repr(entries[0]["shots"]),
        "__SEED__": repr(entries[0]["seed"]),
        "__LIMITS__": repr({key: settings[key] for key in
                           ("memory_mb", "compile_seconds", "simulate_seconds", "runner_seconds")}),
        "__RUNTIME__": _runtime_source(),
    }
    script = re.sub(r"__[A-Z_]+__", lambda match: values[match.group()], (
        KIT / "templates/issue_repro.py.template"
    ).read_text())
    compile(script, "<review-repro>", "exec")
    return script
