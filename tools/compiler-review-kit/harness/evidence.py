# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""A small, durable-but-disposable evidence cache outside the kit's work directory, and rechecks from it.

The cache may be wiped at any time: everything in it is either regenerable (generator seeds,
extracted tests) or recoverable from a report's embedded repro scripts. It only makes the next
review's "status of earlier findings" quick and exact.
"""

from datetime import datetime, timezone
import ast
import gzip
import json
from pathlib import Path
import re
import shutil
import sys
import tempfile

from harness.analysis import load_run, summarize_run
from harness.cases import bundle_cases, write_case_manifest
from harness.common import ReviewError, read_json, write_json
from harness.execution import exec_script, prepare_builds
from harness.manifests import merge_manifests

REPRO_MARKER = re.compile(r"Exits 0 (?:when the bug no longer reproduces|while the bug reproduces)")
GZIP_ABOVE = 1024 * 1024
QUIET = {"validated_unchanged", "simulator_observed"}


def review_dir(config, name=None):
    name = config["name"] if name is None else name
    if not re.fullmatch(r"[A-Za-z0-9_.-]+", name) or name in (".", ".."):
        raise ReviewError("Review name must be a single safe path component")
    return Path(config["cache_dir"]) / name


def tree_size(path):
    path = Path(path)
    if path.is_file():
        return path.stat().st_size
    return sum(p.stat().st_size for p in path.rglob("*") if p.is_file())


def _stored_path(source, destination):
    """Large JSON is stored gzipped under the same name plus `.gz`."""
    if source.suffix == ".json" and source.stat().st_size > GZIP_ABOVE:
        return destination.with_name(destination.name + ".gz")
    return destination


def _stored_bytes(path):
    return gzip.decompress(path.read_bytes()) if path.suffix == ".gz" else path.read_bytes()


def _copy_file(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.suffix == ".gz":
        with source.open("rb") as stream, gzip.open(destination, "wb") as packed:
            shutil.copyfileobj(stream, packed)
    else:
        shutil.copy2(source, destination)


def _cached_path(path, work_dir):
    """A saved input's cache location: its path within the review's scratch, else under its parent's name.

    Basenames alone collide (every generator run writes generator.json, every probe report.md).
    """
    path = Path(path).resolve()
    try:
        return path.relative_to(Path(work_dir).resolve())
    except ValueError:
        return Path(path.parent.name, path.name)


def _plan_files(items, work_dir, destination, recorded):
    """[(source, stored path)] for every saved file; refuses to let one file replace a different one."""
    planned = {}
    for item in map(Path, items):
        base = _cached_path(item, work_dir)
        if item.is_dir():
            sources = [(p, base / p.relative_to(item)) for p in sorted(item.rglob("*"))
                       if p.is_file() and "__pycache__" not in p.parts]
        else:
            sources = [(item, base)]
        for source, name in sources:
            source = source.resolve()
            stored = _stored_path(source, destination / "files" / name)
            key = str(stored.relative_to(destination))
            if key in planned and planned[key][0] != source and planned[key][0].read_bytes() != source.read_bytes():
                raise ReviewError(f"{planned[key][0]} and {source} would both be cached as {key}; rename one.")
            previous = recorded.get(key)
            if (stored.exists() and previous != str(source)
                    and _stored_bytes(stored) != source.read_bytes()):
                raise ReviewError(f"Cache entry {key} already holds {previous or 'an earlier file'}; "
                                  f"refusing to replace it with {source}. Rename one.")
            planned.setdefault(key, (source, stored))
    return planned


def _finding_key(manifest):
    manifest = Path(manifest).resolve()
    return f"{manifest.parent.name}-{manifest.stem}"  # many manifests are called cases.json


def interesting_cases(run):
    """Select frozen inputs, never the mutable manifest that originally supplied them."""
    _, cases, rows = load_run(run)
    chosen = set()
    for case_id, row in rows.items():
        verdicts = {c["verdict"] for c in row["comparison"]}
        verdicts |= {c["verdict"] for c in (row.get("previous") or [])}
        layer_statuses = {s for statuses in (row.get("layers") or {}).values() for s in statuses.values()}
        if verdicts - QUIET or layer_statuses - {"ok", "validated", "observed_unvalidated", "accepted_unvalidated",
                                                  "unit_unvalidated"}:
            chosen.add(case_id)
    return [case for case in cases.values() if case["id"] in chosen], Path(run).resolve()


def prune(root, budget_bytes, keep):
    """Best-effort pruning of recognized reviews; never delete the current review."""
    root = Path(root)
    total = tree_size(root)
    if tree_size(keep) > budget_bytes:
        return total, []  # Evicting other reviews cannot make the protected review fit.
    reviews = []
    for path in root.iterdir() if root.is_dir() else []:
        index = path / "evidence.json"
        if not path.is_dir() or path.is_symlink() or not index.is_file() or index.is_symlink():
            continue
        try:
            metadata = read_json(index)
        except (ReviewError, json.JSONDecodeError) as error:
            print(f"WARNING: not pruning unrecognized cache entry {path}: {error}", file=sys.stderr)
            continue
        if isinstance(metadata, dict) and metadata.get("name") == path.name and {"findings", "runs", "files"} <= metadata.keys():
            reviews.append(path)
    removed = []
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
    sources = index.setdefault("sources", {})  # cached file -> the file it was saved from
    # Validate every name before writing anything, so a conflict leaves the cache untouched.
    manifests = {}
    for manifest in map(Path, findings):
        key, source = _finding_key(manifest), str(manifest.resolve())
        recorded = manifests.get(key) or index["findings"].get(key, {}).get("source")
        if recorded not in (None, source):
            raise ReviewError(f"Findings {recorded} and {source} would both be cached as {key}; rename one.")
        manifests[key] = source
    planned = _plan_files(files, config["work_dir"], destination, sources)
    for key, source in manifests.items():
        target = destination / "findings" / key
        if target.exists():
            shutil.rmtree(target)
        count = bundle_cases(read_json(source), Path(source).parent, target)
        index["findings"][key] = {"source": source, "cases": count}
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
    for key, (source, stored) in planned.items():
        _copy_file(source, stored)
        sources[key] = str(source)
        index["files"].append(key)
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
    budget = config["cache_budget_mb"] * 1024 * 1024
    total, removed = prune(config["cache_dir"], budget, destination)
    if total > budget:
        print(f"WARNING: evidence cache uses {total} bytes, above its {budget}-byte budget; "
              f"current review {destination.name} uses {size} bytes and is protected. "
              "Compact retained evidence or increase cache_budget_mb. Unrecognized entries are not pruned; inspect them manually.",
              file=sys.stderr)
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
        try:
            description = ast.get_docstring(ast.parse(block))
        except SyntaxError as error:
            raise ReviewError(f"Invalid embedded repro {index}: {error}") from error
        title = (description or block.strip()).splitlines()[0] or f"script {index}"
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
    """Merge immutable evidence with the same conflict checks as ordinary subsets."""
    merged = merge_manifests(manifests, patterns=["*"])
    write_case_manifest(merged, output)
    return len(merged)


def recheck_scripts(config, scripts, targets, mode, output):
    """Use printed verdicts, falling back to not reproduced for exit 0."""
    table = []
    builds = prepare_builds(config, targets, mode) if scripts else {}
    for script in scripts:
        rows = exec_script(config, script, targets, mode, output, timeout=900, builds=builds)
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
