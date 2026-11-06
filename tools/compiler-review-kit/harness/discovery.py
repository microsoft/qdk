# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Discover sample candidates and snapshot issues without executing issue text."""

from datetime import datetime, timezone
import json
from pathlib import Path
import re
import sys
from urllib.parse import urlencode

from harness.builds import checkout_revision, locked
from harness.common import ReviewError, command, git, read_json, write_json


def infer_entry(source):
    source = re.sub(r"/\*.*?\*/|//[^\n]*", "", source, flags=re.S)
    entries = re.findall(
        r"@EntryPoint\s*\([^)]*\)\s*(?:operation|function)\s+([A-Za-z_]\w*)\s*\(\s*\)",
        source,
    )
    if not entries:
        entries = re.findall(r"(?:operation|function)\s+(Main)\s*\(\s*\)", source)
    if len(entries) != 1:
        return None
    namespaces = re.findall(r"\bnamespace\s+([\w.]+)", source)
    if len(namespaces) > 1:
        return None
    prefix = namespaces[0] + "." if namespaces else ""
    return prefix + entries[0] + "()"


def discover_samples(config, output, revision="candidate"):
    work = Path(config["build_dir"])
    work.mkdir(parents=True, exist_ok=True)
    with locked(work / "build.lock"):
        root = checkout_revision(config, config["refs"][revision])
    samples = root / "samples"
    cases, pending, project_files = [], [], set()
    projects = sorted(samples.rglob("qsharp.json"))
    for manifest in projects:
        project = manifest.parent
        sources = sorted((project / "src").rglob("*.qs"))
        project_files.update(sources)
        entries = [infer_entry(source.read_text()) for source in sources]
        entries = [entry for entry in entries if entry]
        relative = str(project.relative_to(samples))
        if len(entries) != 1:
            pending.append({"origin": relative, "reason": "Choose an explicit project entry and arguments"})
            continue
        cases.append(
            {
                "id": "sample-project-" + re.sub(r"[^A-Za-z0-9_.-]", "-", relative),
                "project_root": str(project),
                "entry": entries[0],
                "oracle": {"kind": "observe"},
                "origin": f"{revision}:{config['refs'][revision]}:samples/{relative}",
                "notes": "Inferred entry: inspect before running; add a justified deterministic oracle where possible.",
            }
        )
    for source in sorted(samples.rglob("*.qs")):
        if source in project_files or any(project.parent in source.parents for project in projects):
            continue
        relative = str(source.relative_to(samples))
        entry = infer_entry(source.read_text())
        if entry is None:
            pending.append({"origin": relative, "reason": "No unambiguous zero-argument entry; curate manually"})
            continue
        cases.append(
            {
                "id": "sample-" + re.sub(r"[^A-Za-z0-9_.-]", "-", relative),
                "source": source.read_text(),
                "entry": entry,
                "oracle": {"kind": "observe"},
                "origin": f"{revision}:{config['refs'][revision]}:samples/{relative}",
                "notes": "Inferred entry: inspect before running; observe does not establish correctness.",
            }
        )
    if not cases:
        raise ReviewError("No sample entries found; inspect the checkout and curate cases manually.")
    ids = [case["id"] for case in cases]
    if len(set(ids)) != len(ids):
        raise ReviewError("Sample IDs collide after path normalization; assign IDs manually.")
    write_json(output, cases)
    write_json(
        Path(output).with_suffix(".discovery.json"),
        {
            "revision": config["refs"][revision],
            "pending": pending,
            "notes": "Q# files/projects only. Notebook, OpenQASM, and Python scenarios require manual curation.",
        },
    )
    return len(cases), len(pending)


def _api(endpoint, paginated=False):
    args = ["gh", "api"]
    if paginated:
        args += ["--paginate", "--slurp"]
    args.append(endpoint)
    result = json.loads(command(args))
    return [item for page in result for item in page] if paginated else result


def commit_messages(repository, reference, candidate):
    """Commit messages in the range; bot-authored commits contribute only their subject line.

    Bots such as dependabot quote upstream release notes whose `#N` numbers belong to other
    repositories but often also exist here, silently pulling in unrelated issues.
    """
    log = git(repository, "log", "--format=%an%x1f%ae%x1f%B%x1e", f"{reference}..{candidate}")
    messages = []
    for record in filter(str.strip, log.split("\x1e")):
        name, email, body = record.strip("\n").split("\x1f", 2)
        bot = "[bot]" in name or "[bot]" in email
        messages.append(body.split("\n", 1)[0] if bot else body)
    return messages


def snapshot_issues(config, output):
    selection = config["input"].get("issues", {})
    repo = selection.get("repository", "microsoft/qdk")
    prefix = f"repos/{repo}"
    wanted = {}
    if "author" in selection:
        query = urlencode({"creator": selection["author"], "state": "all", "per_page": 100})
        for issue in _api(f"{prefix}/issues?{query}", paginated=True):
            if "pull_request" not in issue and issue["number"] >= selection.get("min_number", 1):
                wanted.setdefault(issue["number"], []).append("author sweep")
    referenced = set()
    for message in commit_messages(config["repository"], config["refs"]["reference"], config["refs"]["candidate"]):
        referenced.update(int(number) for number in re.findall(r"(?<!\w)#(\d+)\b", message))
        referenced.update(
            int(number)
            for number in re.findall(rf"https://github\.com/{re.escape(repo)}/issues/(\d+)", message)
        )
    for number in sorted(referenced):
        wanted.setdefault(number, []).append("commit reference (may be a PR)")
    for number in selection.get("include", []):
        wanted.setdefault(number, []).append("explicit include")
    snapshots, prs, unresolved = [], [], []
    for number, reasons in sorted(wanted.items()):
        try:
            issue = _api(f"{prefix}/issues/{number}")
        except ReviewError as err:
            # Commit messages can quote another repository's numbers; only those references may be
            # skipped when missing (404) or deleted (410). Selected issues must resolve.
            if not re.search(r"\(HTTP (404|410)\)", str(err)) or any(
                not reason.startswith("commit reference") for reason in reasons
            ):
                raise
            unresolved.append(number)
            continue
        if "pull_request" in issue:
            prs.append(number)
            continue
        comments = _api(f"{prefix}/issues/{number}/comments?per_page=100", paginated=True)
        snapshots.append(
            {
                "number": number,
                "url": issue["html_url"],
                "title": issue["title"],
                "state": issue["state"],
                "state_reason": issue.get("state_reason"),
                "body": issue["body"],
                "updated_at": issue["updated_at"],
                "selected_by": reasons,
                "comments": [
                    {"url": c["html_url"], "body": c["body"], "updated_at": c["updated_at"]}
                    for c in comments
                ],
                "repro_status": "not_curated",
            }
        )
    write_json(
        output,
        {
            "repository": repo,
            "captured": datetime.now(timezone.utc).isoformat(),
            "selection": selection,
            "issues": snapshots,
            "pr_references": prs,
            "unresolved_references": unresolved,
        },
    )
    if unresolved:
        print(f"Skipped commit references not found in {repo}: {unresolved}", file=sys.stderr)
    return len(snapshots)


def snapshot_all_issues(config, output):
    """Whole-repository issue snapshot (titles/bodies, no comments) for overlap searches."""
    repo = config["input"].get("issues", {}).get("repository", "microsoft/qdk")
    query = urlencode({"state": "all", "per_page": 100})
    issues = [
        {
            "number": issue["number"],
            "url": issue["html_url"],
            "title": issue["title"],
            "state": issue["state"],
            "state_reason": issue.get("state_reason"),
            "author": (issue.get("user") or {}).get("login"),
            "labels": [label["name"] for label in issue.get("labels", [])],
            "body": issue.get("body") or "",
            "updated_at": issue["updated_at"],
        }
        for issue in _api(f"repos/{repo}/issues?{query}", paginated=True)
        if "pull_request" not in issue
    ]
    write_json(
        output,
        {
            "repository": repo,
            "captured": datetime.now(timezone.utc).isoformat(),
            "scope": "all issues, open and closed; comments not included",
            "issues": sorted(issues, key=lambda issue: issue["number"]),
        },
    )
    return len(issues)
