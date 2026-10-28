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

from harness.common import KIT, ReviewError, resolve_ref, write_json
from harness.execution import exec_script

TEMPLATE = KIT / "templates" / "issue_repro.py.template"
REQUIRED = ("slug", "title", "kind", "affects", "regression", "found_in", "related", "description", "source",
            "expected", "affected", "correct", "cause")
SIMPLE_BUILD = '''def build(ctx):
    """Returns the entry expression and its arguments."""
    return "Main()", ()


FORMS = [("", build)]'''
SIMPLE_CONTEXT = '''ctx = Context(target_profile=profile)
    ctx.eval(SOURCE)
    return ctx'''
PROJECT_CONTEXT = '''ctx = Context(project_root=PROJECT_ROOT, target_profile=profile)
    ctx.eval(SOURCE)  # a later fragment: a different compilation unit from the project
    return ctx'''


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


def project_block(files):
    body = ",\n".join(f'    "{name}": """{text}"""' for name, text in files.items())
    return (
        "PROJECT_FILES = {  # a small Q# project, written to a temporary directory\n" + body + ",\n}\n"
        "PROJECT_ROOT = tempfile.mkdtemp()\n"
        'with open(os.path.join(PROJECT_ROOT, "qsharp.json"), "w") as file:\n'
        '    file.write("{}")\n'
        "for name, text in PROJECT_FILES.items():\n"
        "    os.makedirs(os.path.dirname(os.path.join(PROJECT_ROOT, name)), exist_ok=True)\n"
        '    with open(os.path.join(PROJECT_ROOT, name), "w") as file:\n'
        "        file.write(text)\n"
    )


def render(issue):
    """The standalone repro script for one issue; exits 0 when the bug no longer reproduces."""
    title = issue.get("title_text") or re.sub(r"^\[[^\]]*\]\s*", "", issue["title"])
    replacements = {
        "__TITLE__": title,
        "__DOC__": (issue.get("doc") or "Compiles the entry for each target profile, runs the QIR with qir-runner, "
                    "and runs the same program in the Q# simulator for comparison.").strip(),
        "__AFFECTED_TEXT__": ", ".join(issue["affected"]) + f" return something other than {issue['expected']}",
        "__REQUIRES_EXTRA__": issue.get("requires", ""),
        "__SOURCE__": issue["source"].rstrip() + "\n",
        "__PROJECT_BLOCK__": project_block(issue["project"]) if issue.get("project") else "",
        "__EXPECTED__": issue["expected"],
        "__AFFECTED__": repr(issue["affected"]),
        "__BUILD__": issue.get("build", SIMPLE_BUILD).strip(),
        "__CONTEXT__": PROJECT_CONTEXT if issue.get("project") else SIMPLE_CONTEXT,
    }
    text = TEMPLATE.read_text()
    for key, value in replacements.items():
        text = text.replace(key, value)
    compile(text, f"{issue['slug']}.py", "exec")
    return text


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
    for index, issue in enumerate(issues, 1):
        script_text = render(issue)
        path = scripts / f"{index:02d}_{issue['slug']}.py"
        path.write_text(script_text)
        wanted = issue.get("revisions") or default
        targets = [(rev, config["refs"][rev] if rev in config["refs"] else resolve_ref(config["repository"], rev))
                   for rev in wanted]
        cache = execs / f"{issue['slug']}.results.json"
        skip = (only and issue["slug"] not in only) or cached
        if skip and cache.is_file() and json.loads(cache.read_text()).get("script") == script_text:
            results = json.loads(cache.read_text())["results"]
        else:
            results = _results(exec_script(config, path, targets, mode, execs, timeout))
            write_json(cache, {"script": script_text, "results": results})
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
