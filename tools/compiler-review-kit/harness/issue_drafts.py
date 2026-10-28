# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Draft GitHub issues in the usual [Miscompile]/[Panic] format from a trusted issue-definition file.

Each issue's repro is rendered from templates/issue_repro.py.template, executed at every
requested revision with `exec`, and its Outputs table is filled from the actual results.
"""

import importlib.util
import json
from pathlib import Path
import re
import tempfile

from harness.cases import write_project
from harness.common import DEFAULT_PROFILES, ReviewError, digest, preflight, read_json, resolve_ref, write_json
from harness.execution import exec_script, prepare_builds
from harness.repros import render_repro

REQUIRED = ("slug", "title", "kind", "affects", "regression", "found_in", "related", "description", "source",
            "expected", "affected", "correct", "cause")
SIMPLE_BUILD = '''def build(ctx):
    """Returns the entry expression and its arguments."""
    return "Main()", ()


FORMS = [("", build)]'''


def load_definitions(path):
    spec = importlib.util.spec_from_file_location("review_issues", path)
    if spec is None or spec.loader is None:
        raise ReviewError(f"Cannot load issue definitions: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    issues = getattr(module, "ISSUES", None)
    if not isinstance(issues, list) or not issues:
        raise ReviewError("Issue definitions must define a non-empty ISSUES list")
    for issue in issues:
        missing = [field for field in REQUIRED if field not in issue]
        if missing:
            raise ReviewError(f"Issue {issue.get('slug', '?')} lacks {', '.join(missing)}")
        if not re.fullmatch(r"[a-z0-9_]+", issue["slug"]):
            raise ReviewError(f"Invalid slug {issue['slug']!r}: use lowercase letters, digits and underscores")
    slugs = [issue["slug"] for issue in issues]
    if len(set(slugs)) != len(slugs):
        raise ReviewError("Duplicate issue slugs")
    return module, issues


def render(issue, profiles=None, limits=None):
    """The standalone repro script for one issue; exits 0 when the bug no longer reproduces."""
    title = issue.get("title_text") or re.sub(r"^\[[^\]]*\]\s*", "", issue["title"])
    build = issue.get("build", SIMPLE_BUILD).strip()
    namespace = {"SOURCE": issue["source"]}
    exec(compile("from __future__ import annotations\n" + build, "<issue-build>", "exec"), namespace)
    forms = namespace.get("FORMS")
    if not isinstance(forms, list) or not forms or any(
        not isinstance(form, (tuple, list)) or len(form) != 2 or not isinstance(form[0], str) or not callable(form[1])
        for form in forms
    ):
        raise ReviewError("Issue build must define FORMS as a non-empty list of (suffix, build) pairs")
    if len({suffix for suffix, _ in forms}) != len(forms):
        raise ReviewError("Issue forms must have unique suffixes")
    with tempfile.TemporaryDirectory(prefix="issue-repro-") as directory:
        root = Path(directory)
        project = issue.get("project")
        if project:
            write_project(project, root / "project")
        cases = []
        for index, _ in enumerate(forms):
            adapter = root / f"form-{index}.py"
            adapter.write_text(
                "from __future__ import annotations\nfrom qdk import Context, TargetProfile\n"
                f"SOURCE = {issue['source']!r}\n\n{build}\n\n"
                f"def prepare(ctx):\n    return FORMS[{index}][1](ctx)\n"
            )
            case = {"id": f"{issue['slug']}-{index}", "source": issue["source"], "adapter": str(adapter),
                    "profiles": DEFAULT_PROFILES if profiles is None else profiles,
                    "shots": 10, "seed": 1, "oracle": {"kind": "observe"}}
            if project:
                case["project_root"] = str(root / "project")
            cases.append(case)
        return render_repro(cases, issue["expected"], issue["affected"], title,
                            issue.get("doc", ""), [suffix for suffix, _ in forms], limits, issue.get("requires", ""))


def _results(rows):
    results = {}
    for row in rows:
        log = Path(row["log"]).read_text() if Path(row["log"]).is_file() else ""
        match = re.search(r"^JSON: (.*)$", log, re.M)
        results[row["label"]] = {"exit": row["exit"], "rows": json.loads(match.group(1)) if match else None,
                                 "panics": row["panics"], "sha": row["sha"],
                                 "reproduction": row["reproduction"]}
    return results


def code(text):
    return f"`` {text} ``" if "`" in text else f"`{text}`"


def short(value):
    for prefix in ("panics: ", "does not compile: "):
        if value.startswith(prefix):
            message = re.sub(r"\s*\(https://aka\.ms[^)]*\)", "", value[len(prefix):])
            head = message.split(": ")[0]
            return f"{prefix}{code(head if len(head) <= 90 else head[:87] + '...')}"
    return code(value)


def cell(values, row, issue):
    text = " / ".join(short(v) for v in values)
    wrong = values != [issue["expected"]]
    panicked = any(v.startswith("panics") for v in values)
    plain_wrong = wrong and not any(v.startswith("does not compile") for v in values)
    return f"**{text}**" if (row in issue["affected"] and wrong) or panicked or plain_wrong else text


def panic_link(panic, sha):
    url = f"https://github.com/microsoft/qdk/blob/{sha}/{panic['path']}#L{panic['line']}"
    where = panic["path"].replace("source/compiler/", "")
    function = f", in `{panic['function']}`" if panic.get("function") else ""
    return f"[`{where}:{panic['line']}`]({url}){function}"


def markdown(issue, labels, columns, results, script):
    out = [f"## {issue['title']}", ""]
    kind = issue["kind"]
    for label in labels:
        seen, links = set(), []
        for panic in results[label]["panics"]:
            if (panic["path"], panic["line"]) not in seen:
                seen.add((panic["path"], panic["line"]))
                links.append(panic_link(panic, results[label]["sha"]))
        kind = kind.replace("{panic:" + label + "}", "; ".join(links) or "no panic")
    out += [f"**Kind:** {kind}  ", f"**Affects:** {issue['affects']}  ", f"**Regression:** {issue['regression']}  ",
            f"**Found in:** {issue['found_in']}  ", f"**Related:** {issue['related']}", "",
            issue["description"].strip(), "", "## Q#", ""]
    for name, text in (issue.get("project") or {}).items():
        out += [f"Project file `{name}`:", "", "```qsharp", text.strip(), "```", ""]
    if issue.get("project"):
        out += ["Evaluated afterwards with `ctx.eval` (a separate compilation unit):", ""]
    out += ["```qsharp", issue["source"].strip(), "```", ""]
    if issue.get("how"):
        out += [issue["how"].strip(), ""]
    out += ["## Outputs", "",
            "10 shots each. The simulator runs the program directly; each profile compiles it to QIR, which is run "
            "with qir-runner. Wrong outputs and panics are in bold.", ""]
    rows = next((list(r["rows"]) for r in results.values() if r["rows"]), [])
    out.append("| Run | " + " | ".join(columns[label] for label in labels) + " |")
    out.append("|---|" + "---|" * len(labels))
    for row in rows:
        cells = [cell(results[label]["rows"].get(row, ["(not run)"]), row, issue) if results[label]["rows"]
                 else "(script failed)" for label in labels]
        out.append(f"| {row[0].upper() + row[1:]} | " + " | ".join(cells) + " |")
    out.append("| **Correct output** | " + " | ".join(f"`{issue['expected']}`" for _ in labels) + " |")
    out.append("")
    messages = []
    for label in labels:
        seen = set()
        for row, values in (results[label]["rows"] or {}).items():
            for value in values:
                loud = value.startswith("panics") or (value.startswith("does not compile") and row in issue["affected"])
                if loud and value not in seen:
                    seen.add(value)
                    messages.append(f"{label}: {value}")
    if messages:
        out += ["Messages:", "", "```text", *messages, "```", ""]
    out += ["## Correct output", "", issue["correct"].strip(), "", "## Probable cause", "", issue["cause"].strip(), "",
            "## Reproduce", "", "```python", script.rstrip(), "```"]
    return "\n".join(out)


def draft_issues(config, definitions, output, revisions=None, only=(), cached=False, mode="release", timeout=900):
    """Render, execute and format every issue; returns (output path, per-issue reproduction status)."""
    definitions = Path(definitions).resolve()
    module, issues = load_definitions(definitions)
    output = Path(output)
    work = Path(config["work_dir"]) / "draft-issues" / output.stem
    scripts, execs = work / "scripts", work / "exec"
    scripts.mkdir(parents=True, exist_ok=True)
    execs.mkdir(parents=True, exist_ok=True)
    default = revisions or getattr(module, "REVISIONS", None) or ["reference", "candidate"]
    labels_for = getattr(module, "COLUMNS", {})
    sections, status = [], {}
    tools, builds = preflight(config), {}
    resolved = dict(config["refs"])
    for index, issue in enumerate(issues, 1):
        script_text = render(issue, config["profiles"], config["limits"])
        path = scripts / f"{index:02d}_{issue['slug']}.py"
        path.write_text(script_text)
        wanted = issue.get("revisions") or default
        targets = []
        for revision in wanted:
            if revision not in resolved:
                resolved[revision] = resolve_ref(config["repository"], revision)
            targets.append((revision, resolved[revision]))
        prepare_builds(config, targets, mode, builds, tools)
        identity = digest({
            "script": script_text, "targets": targets, "mode": mode, "timeout": timeout,
            "builds": {sha: {key: builds[sha][key] for key in
                            ("native_digest", "package_digest", "library_digest", "tools")} for _, sha in targets},
        })
        cache = execs / f"{issue['slug']}.results.json"
        skip = (only and issue["slug"] not in only) or cached
        previous = read_json(cache) if skip and cache.is_file() else {}
        if previous.get("identity") == identity:
            results = previous["results"]
        else:
            results = _results(exec_script(config, path, targets, mode, execs, timeout, builds=builds))
            write_json(cache, {"identity": identity, "results": results})
        labels = [label for label, _ in targets]
        columns = {label: labels_for.get(label) or f"{label} (`{sha[:8]}`)" for label, sha in targets}
        status[issue["slug"]] = {label: results[label]["reproduction"] for label in labels}
        sections.append(markdown(issue, labels, columns, results, script_text))
    text = getattr(module, "HEADER", "").rstrip()
    text = (text + "\n\n---\n\n" if text else "") + "\n\n---\n\n".join(sections)
    comments = getattr(module, "COMMENTS", "").strip()
    if comments:
        text += "\n\n---\n\n" + comments
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(text + "\n")
    return output, status
