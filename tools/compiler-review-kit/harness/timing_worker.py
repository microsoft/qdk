# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Times context creation, loading and compilation of each case at one revision (one process per sweep)."""

import contextlib
import io
import json
from pathlib import Path
import sys
import time


def measure(cases, profiles):
    import qdk
    from harness.worker import failure, load_case

    results = {}
    for case in cases:
        rows = {}
        for profile in [p for p in profiles if p in case["profiles"]]:
            if not hasattr(qdk.TargetProfile, profile):
                rows[profile] = {"status": "unsupported_profile"}
                continue
            try:
                start = time.perf_counter()
                ctx = qdk.Context(
                    target_profile=getattr(qdk.TargetProfile, profile),
                    project_root=case.get("project_root"),
                    language_features=case.get("language_features"),
                )
                created = time.perf_counter()
                ctx.set_classical_seed(case["seed"])
                ctx.set_quantum_seed(case["seed"])
                entry, args = load_case(ctx, case)
                loaded = time.perf_counter()
                ctx.compile(entry, *args)
                compiled = time.perf_counter()
                rows[profile] = {"status": "ok", "context": created - start, "load": loaded - created,
                                 "compile": compiled - loaded}
            except BaseException as error:  # PanicException derives from BaseException
                rows[profile] = failure(error)
        results[case["id"]] = rows
    return {"status": "ok", "qdk": qdk.__file__, "results": results}


def main():
    job = json.load(sys.stdin)
    with contextlib.redirect_stdout(io.StringIO()):  # Q# Message output must not corrupt the JSON result
        result = measure(job["cases"], job["profiles"])
    print(json.dumps(result))


if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    main()
