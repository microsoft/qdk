# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""A small, durable-but-disposable evidence cache outside the kit's work directory, and rechecks from it.

The cache may be wiped at any time: everything in it is either regenerable (generator seeds,
extracted tests) or recoverable from a report's embedded repro scripts. It only makes the next
review's "status of earlier findings" quick and exact.
"""

from datetime import datetime, timezone
import gzip
import json
from pathlib import Path
import re
import shutil
import tempfile

from harness.analysis import load_run, summarize_run
from harness.common import ReviewError, read_json, write_json
from harness.execution import exec_script

REPRO_MARKER = re.compile(r"Exits 0 (?:when the bug no longer reproduces|while the bug reproduces)")
GZIP_ABOVE = 1024 * 1024
QUIET = {"validated_unchanged", "simulator_observed"}


def review_dir(config, name=None):
    return Path(config["cache_dir"]) / (name or config["name"])


def tree_size(path):
    path = Path(path)
    if path.is_file():
        return path.stat().st_size
    return sum(p.stat().st_size for p in path.rglob("*") if p.is_file())


def _copy_file(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    if source.suffix == ".json" and source.stat().st_size > GZIP_ABOVE:
        destination = destination.with_name(destination.name + ".gz")
        with source.open("rb") as stream, gzip.open(destination, "wb") as packed:
            shutil.copyfileobj(stream, packed)
    else:
        shutil.copy2(source, destination)
    return destination


def bundle_cases(cases, base, target):
    """Write cases into target/cases.json with sources inlined and adapters/projects copied beside them."""
    target = Path(target)
    bundled, projects = [], {}
    for case in cases:
        case = dict(case)
        if "source_file" in case:
            case["source"] = (base / case.pop("source_file")).read_text()
        if "adapter" in case:
            source = (base / case["adapter"]).resolve()
            copied = target / "adapters" / f"{case['id']}.py"
            _copy_file(source, copied)
            case["adapter"] = f"adapters/{copied.name}"
        if "project_root" in case:
            source = (base / case["project_root"]).resolve()
            if source not in projects:  # many cases share one project (e.g. a client's vendored Q#)
                projects[source] = f"projects/{len(projects) + 1:03d}-{source.name}"
                copied = target / projects[source]
                if copied.exists():
                    shutil.rmtree(copied)
                shutil.copytree(source, copied, ignore=shutil.ignore_patterns("target", "__pycache__", ".git"))
            case["project_root"] = projects[source]
        bundled.append(case)
    write_json(target / "cases.json", bundled)
    return len(bundled)


def interesting_cases(run):
    """Original case entries of a run whose outcome is anything but unchanged and validated/observed."""
    manifest, cases, rows = load_run(run)
    chosen = set()
    for case_id, row in rows.items():
        verdicts = {c["verdict"] for c in row["comparison"]}
        verdicts |= {c["verdict"] for c in (row.get("previous") or [])}
        layer_statuses = {s for statuses in (row.get("layers") or {}).values() for s in statuses.values()}
        if verdicts - QUIET or layer_statuses - {"validated", "observed_unvalidated", "accepted_unvalidated",
                                                  "unit_unvalidated"}:
            chosen.add(case_id)
    source = Path(manifest["cases_path"])
    if not source.is_file():
        raise ReviewError(f"Original case manifest is gone: {source}")
    return [case for case in read_json(source) if case["id"] in chosen], source.parent


def prune(root, budget_bytes, keep):
    """Delete whole review directories, oldest first, until the cache fits the budget (never `keep`)."""
    root = Path(root)
    reviews = [p for p in root.iterdir() if p.is_dir()] if root.is_dir() else []
    removed = []
    total = sum(tree_size(p) for p in reviews)
    for review in sorted(reviews, key=lambda p: p.stat().st_mtime):
        if total <= budget_bytes:
            break
        if review.resolve() == Path(keep).resolve():
            continue
        total -= tree_size(review)
        shutil.rmtree(review)
        removed.append(review.name)
    return total, removed


def save_evidence(config, findings=(), runs=(), files=(), name=None, note=None):
    destination = review_dir(config, name)
    destination.mkdir(parents=True, exist_ok=True)
    index_path = destination / "evidence.json"
    index = read_json(index_path) if index_path.is_file() else {"findings": {}, "runs": {}, "files": []}
    for manifest in map(Path, findings):
        key = f"{manifest.resolve().parent.name}-{manifest.stem}"  # many manifests are called cases.json
        target = destination / "findings" / key
        if target.exists():
            shutil.rmtree(target)
        count = bundle_cases(read_json(manifest), manifest.resolve().parent, target)
        index["findings"][key] = {"source": str(manifest.resolve()), "cases": count}
    for run in map(Path, runs):
        cases, base = interesting_cases(run)
        target = destination / "runs" / run.name
        if target.exists():
            shutil.rmtree(target)
        count = bundle_cases(cases, base, target) if cases else 0
        summary = summarize_run(run)
        keep = {k: summary[k] for k in ("reference", "candidate", "roles", "cases", "verdicts", "flagged", "silenced",
                                        "lost_fixes", "layer_hazards", "pipeline_gaps", "qir_identity")}
        keep["compile_outliers"] = summary["compile_outliers"][:50]
        write_json(target / "summary.json", keep)
        index["runs"][run.name] = {"cases": count, "of": summary["cases"]}
    for item in map(Path, files):
        if item.is_dir():
            for path in sorted(p for p in item.rglob("*") if p.is_file() and "__pycache__" not in p.parts):
                saved = _copy_file(path, destination / "files" / item.name / path.relative_to(item))
                index["files"].append(str(saved.relative_to(destination)))
        else:
            saved = _copy_file(item, destination / "files" / item.name)
            index["files"].append(str(saved.relative_to(destination)))
    index["files"] = sorted(set(index["files"]))
    index.update({
        "name": destination.name,
        "saved": datetime.now(timezone.utc).isoformat(),
        "refs": config["refs"],
        "report": config["report"],
        "config": config["input"],
        "note": note or index.get("note"),
        "warning": "Cache: may be deleted at any time. Recover findings from the report's embedded repros.",
    })
    write_json(index_path, index)
    size = tree_size(destination)
    total, removed = prune(config["cache_dir"], config["cache_budget_mb"] * 1024 * 1024, destination)
    return destination, size, total, removed


def report_scripts(report, directory):
    """Extract self-checking repro scripts with either the current or legacy exit convention."""
    text = Path(report).read_text()
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    paths = []
    for index, block in enumerate(re.findall(r"```python\n(.*?)^```", text, re.S | re.M), 1):
        if not REPRO_MARKER.search(block):
            continue
        title = block.strip().splitlines()[0].strip('"').strip() if block.strip() else f"script {index}"
        slug = re.sub(r"[^a-z0-9]+", "_", title.lower()).strip("_")[:50] or f"script_{index}"
        path = directory / f"{len(paths) + 1:02d}_{slug}.py"
        path.write_text(block)
        paths.append(path)
    return paths


def evidence_manifests(destination):
    """Every bundled manifest in a review's cache directory."""
    destination = Path(destination)
    return sorted(destination.glob("findings/*/cases.json")) + sorted(destination.glob("runs/*/cases.json"))


def merged_manifest(manifests, output):
    """One manifest with unique IDs and absolute paths; later duplicates of an ID are dropped."""
    merged, seen = [], set()
    for manifest in manifests:
        base = Path(manifest).resolve().parent
        for case in read_json(manifest):
            if case["id"] in seen:
                continue
            seen.add(case["id"])
            case = dict(case)
            for field in ("adapter", "project_root", "source_file"):
                if field in case:
                    case[field] = str((base / case[field]).resolve())
            merged.append(case)
    write_json(output, merged)
    return len(merged)


def recheck_scripts(config, scripts, targets, mode, output):
    """Use printed verdicts, falling back to not reproduced for exit 0."""
    table = []
    for script in scripts:
        rows = exec_script(config, script, targets, mode, output, timeout=900)
        table.append((Path(script).name, {row["label"]: row["reproduction"] for row in rows}))
    return table


def print_recheck(table, labels):
    width = max([len(name) for name, _ in table] + [10])
    print(f"{'script':<{width}}  " + "  ".join(f"{label[:14]:14}" for label in labels))
    for name, results in table:
        print(f"{name:<{width}}  " + "  ".join(f"{results.get(label, 'not run'):14}" for label in labels))


def recheck_inputs(config, name=None, report=None):
    """(manifests, scripts, source description); falls back to the report when the cache is gone."""
    destination = review_dir(config, name)
    manifests = evidence_manifests(destination) if destination.is_dir() else []
    scripts = [p for p in sorted((destination / "files").rglob("*.py"))
               if REPRO_MARKER.search(p.read_text())] \
        if destination.is_dir() else []
    if manifests or scripts:
        return manifests, scripts, f"evidence cache {destination}"
    if report and Path(report).is_file():
        work = Path(config["work_dir"])
        work.mkdir(parents=True, exist_ok=True)
        extracted = report_scripts(report, Path(tempfile.mkdtemp(prefix="recheck-", dir=work)))
        if extracted:
            return [], extracted, f"repro scripts embedded in {report} (cache {destination} is gone)"
    raise ReviewError(f"No evidence at {destination} and no report with embedded repro scripts")


def describe(destination):
    index = read_json(Path(destination) / "evidence.json")
    return json.dumps({k: index[k] for k in ("name", "saved", "findings", "runs") if k in index}, indent=2)
