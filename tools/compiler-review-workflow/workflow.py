# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""GitHub Actions orchestration; never execute this file from a reviewed revision."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import signal
import subprocess
import sys
import tarfile
import time
import tomllib
import urllib.request
from urllib.parse import quote, urlencode


REPOSITORY = "microsoft/qdk"
ROOT = Path("/work")
SHA = re.compile(r"[0-9a-f]{40}")
PR_URL = re.compile(r"https://github\.com/microsoft/qdk/pull/([1-9][0-9]*)/?")
PACKAGE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.\[\],<>=!~ ;()\"'-]*")


class WorkflowError(Exception):
    pass


def command(*args, cwd=None, env=None):
    result = subprocess.run(args, cwd=cwd, env=env, text=True, capture_output=True)
    if result.returncode:
        raise WorkflowError(f"{args[0]} failed ({result.returncode}): {result.stderr}")
    return result.stdout.strip()


def write_json(path, value):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(value, indent=2) + "\n")


def github(path):
    request = urllib.request.Request(
        f"https://api.github.com/repos/{REPOSITORY}/{path}",
        headers={
            "Authorization": f"Bearer {os.environ['GH_TOKEN']}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)


def pages(path):
    separator = "&" if "?" in path else "?"
    for page in range(1, 101):
        batch = github(f"{path}{separator}per_page=100&page={page}")
        yield from batch
        if len(batch) < 100:
            return
    raise WorkflowError(f"Pagination limit reached: {path}")


def git(repo, *args):
    return command("git", "--no-pager", "-C", str(repo), *args)


def fetch(repo, sha):
    if not SHA.fullmatch(sha):
        raise WorkflowError(f"Not a full commit SHA: {sha}")
    git(repo, "fetch", "--no-tags", "origin", sha)
    return git(repo, "rev-parse", "--verify", f"{sha}^{{commit}}")


def ancestor(repo, older, newer):
    result = subprocess.run(
        ["git", "-C", str(repo), "merge-base", "--is-ancestor", older, newer],
        capture_output=True,
        text=True,
    )
    if result.returncode not in (0, 1):
        raise WorkflowError(result.stderr)
    return result.returncode == 0


def pr_number(value):
    match = PR_URL.fullmatch(value)
    if match:
        return int(match[1])
    if re.fullmatch(r"#?[1-9][0-9]*", value):
        return int(value.lstrip("#"))
    raise WorkflowError("Expected a microsoft/qdk PR URL or positive PR number")


def discover_stack(repo, number, read=github, listing=pages):
    """Follow base branches, then find exact lower-PR tips in the candidate ancestry."""
    top = read(f"pulls/{number}")
    current = top
    chain = {}
    while True:
        if current["number"] in chain:
            raise WorkflowError("Cyclic PR base relationships")
        if current["base"]["repo"]["full_name"] != REPOSITORY:
            raise WorkflowError("PR base must be microsoft/qdk")
        chain[current["number"]] = current
        fetch(repo, current["head"]["sha"])
        base = current["base"]
        fetch(repo, base["sha"])
        if base["ref"] == "main":
            break
        query = urlencode({"state": "open", "head": f"microsoft:{base['ref']}"})
        matches = [
            pr for pr in listing(f"pulls?{query}")
            if pr["head"]["repo"]
            and pr["head"]["repo"]["full_name"] == REPOSITORY
            and pr["head"]["ref"] == base["ref"]
        ]
        if len(matches) > 1:
            raise WorkflowError(f"Multiple open PRs own base branch {base['ref']}")
        if not matches:
            break
        current = read(f"pulls/{matches[0]['number']}")
        if current["head"]["sha"] != base["sha"]:
            raise WorkflowError("PR base moved while resolving the stack; retry")

    candidate = top["head"]["sha"]
    reference = git(repo, "merge-base", current["base"]["sha"], candidate)
    commits = git(repo, "rev-list", "--reverse", "--topo-order", f"{reference}..{candidate}").splitlines()
    if len(commits) > 500:
        raise WorkflowError("PR scope exceeds 500 commits; inspect the stack before reviewing")
    tips = {}
    for sha in commits:
        for pr in listing(f"commits/{sha}/pulls"):
            if pr["number"] == number or pr["head"]["sha"] != sha:
                continue
            if pr["base"]["repo"]["full_name"] != REPOSITORY:
                continue
            if sha in tips and tips[sha]["number"] != pr["number"]:
                raise WorkflowError(f"Multiple PRs share tip {sha}")
            tips[sha] = pr
    for pr in chain.values():
        sha = pr["head"]["sha"]
        if not ancestor(repo, reference, sha) or not ancestor(repo, sha, candidate):
            raise WorkflowError(f"PR #{pr['number']} tip is not in the pinned stack")
        if sha in tips and tips[sha]["number"] != pr["number"]:
            raise WorkflowError(f"Multiple PRs share tip {sha}")
        tips[sha] = pr

    ordered = [sha for sha in commits if sha in tips]
    for older, newer in zip([reference, *ordered], ordered):
        if older == newer or not ancestor(repo, older, newer):
            raise WorkflowError("PR tips do not form a strictly ordered ancestry chain")
    if not ordered or ordered[-1] != candidate:
        raise WorkflowError("Could not resolve the requested PR tip")
    return {
        "reference": reference,
        "candidate": candidate,
        "layers": ordered[:-1],
        "pull_requests": [
            {"number": tips[sha]["number"], "sha": sha, "url": tips[sha]["html_url"]}
            for sha in ordered
        ],
    }


def resolve(args):
    repo = Path(args.repository)
    if args.pr:
        if args.reference or args.candidate:
            raise WorkflowError("Use either a PR or a commit pair, not both")
        scope = discover_stack(repo, pr_number(args.pr))
    else:
        if not args.reference or not args.candidate:
            raise WorkflowError("Supply a PR or both reference and candidate")
        refs = []
        for value in (args.reference, args.candidate):
            if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._/-]*", value) or ".." in value:
                raise WorkflowError("Commit inputs must be SHAs, branch names, or tags")
            sha = github(f"commits/{quote(value, safe='')}")["sha"]
            refs.append(fetch(repo, sha))
        if refs[0] == refs[1]:
            raise WorkflowError("Reference and candidate are identical")
        scope = dict(reference=refs[0], candidate=refs[1], layers=[], pull_requests=[])
    scope.update(repository=REPOSITORY, control_sha=os.environ["GITHUB_SHA"])
    write_json(args.output, scope)
    if output := os.environ.get("GITHUB_OUTPUT"):
        with Path(output).open("a") as stream:
            stream.write("scope=" + json.dumps(scope, separators=(",", ":")) + "\n")


def materialize(scope, destination):
    git(
        Path(destination).parent, "clone", "--no-checkout", "--no-tags",
        f"https://github.com/{REPOSITORY}.git", str(destination),
    )
    for sha in [scope["reference"], *scope["layers"], scope["candidate"]]:
        fetch(destination, sha)
    git(destination, "checkout", "--detach", scope["candidate"])


def requirements(repo):
    project = tomllib.loads((repo / "source/qdk_package/pyproject.toml").read_text())
    integration = (repo / "source/qdk_package/tests-integration/test_requirements.txt").read_text()
    runner = re.findall(r"^qirrunner==[0-9.]+$", integration, re.MULTILINE)
    if len(runner) != 1:
        raise WorkflowError("Expected one pinned qirrunner requirement at the newest revision")
    result = [
        "build", "pytest", "expecttest", *project["build-system"]["requires"],
        *project["project"]["dependencies"], *runner,
    ]
    if any(not PACKAGE.fullmatch(item) for item in result):
        raise WorkflowError("Preparation supports public registry requirements, not URLs or local paths")
    return result


def prepare(args):
    scope = json.loads(args.scope)
    repo = Path(args.repository)
    materialize(scope, repo)
    wheels = Path(args.output) / "wheels"
    wheels.mkdir(parents=True)
    command(
        sys.executable, "-m", "pip", "download", "--only-binary=:all:",
        "--dest", str(wheels), *requirements(repo),
    )
    venv = Path(args.output) / "venv"
    command(sys.executable, "-m", "venv", str(venv))
    command(
        str(venv / "bin/python"), "-m", "pip", "install", "--no-index",
        "--find-links", str(wheels), *requirements(repo),
    )
    env = dict(os.environ, PYO3_PYTHON=str(venv / "bin/python"))
    command("cargo", "build", "--locked", "--release", "-p", "qdk", "--lib", cwd=repo, env=env)
    cargo = Path(os.environ["CARGO_HOME"])
    with tarfile.open(Path(args.output) / "dependencies.tar.gz", "w:gz") as archive:
        archive.add(wheels, arcname="wheels")
        for name in ("registry", "git"):
            source = cargo / name
            if source.exists():
                archive.add(source, arcname=f"cargo/{name}")


def extract_dependencies(archive, destination):
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        if len(members) > 250_000 or sum(item.size for item in members) > 12 * 1024**3:
            raise WorkflowError("Dependency bundle exceeds file-count or size limits")
        for item in members:
            parts = Path(item.name).parts
            if (
                not parts or parts[0] not in ("cargo", "wheels")
                or Path(item.name).is_absolute() or ".." in parts
                or not (item.isfile() or item.isdir())
            ):
                raise WorkflowError(f"Unsafe dependency bundle entry: {item.name}")
        bundle.extractall(destination, members=members, filter="data")


def policy():
    return {
        "sandbox": {
            "enabled": True,
            "failIfUnavailable": True,
            "allowBypass": False,
            "allowDevToolAccess": False,
            "sandboxMcpServers": True,
            "sandboxLspServers": True,
            "auth": {"git": False, "gh": False},
            "userPolicy": {
                "filesystem": {
                    "readwritePaths": ["/work", "/exchange/from_copilot"],
                    "readonlyPaths": [
                        "/opt/compiler-review-kit", "/opt/qdk-venv",
                        "/opt/review-tools", "/opt/review-rustup", "/opt/hostedtoolcache",
                    ],
                    "deniedPaths": ["/home/runner", "/home/qdk-review/.copilot", "/etc/github-copilot"],
                },
                "network": {
                    "allowOutbound": True,
                    "allowLocalNetwork": False,
                    "allowedHosts": ["github.com", "api.github.com"],
                    "blockedHosts": [
                        "127.0.0.0/8", "10.0.0.0/8", "172.16.0.0/12",
                        "192.168.0.0/16", "169.254.0.0/16", "::1", "fc00::/7", "fe80::/10",
                    ],
                },
            },
        },
    }


def configure(args):
    scope = json.loads(args.scope)
    ROOT.mkdir(exist_ok=True)
    extract_dependencies(args.archive, ROOT / "dependencies")
    materialize(scope, ROOT / "qdk")
    write_json("/etc/github-copilot/managed-settings.json", policy())
    os.chmod("/etc/github-copilot/managed-settings.json", 0o644)
    config = {
        "name": f"actions-{os.environ['GITHUB_RUN_ID']}",
        "repository": "/work/qdk",
        "reference": scope["reference"],
        "candidate": scope["candidate"],
        "report": "/exchange/from_copilot/report.md",
        "venv": "/opt/qdk-venv",
    }
    if scope["layers"]:
        config["layers"] = scope["layers"]
    write_json(ROOT / "review.json", config)
    write_json(ROOT / "scope.json", scope)
    write_json("/home/qdk-review/.copilot/settings.json", policy())
    write_json(
        "/home/qdk-review/.copilot/config.json",
        {"autoUpdate": False, "banner": "never"},
    )


def review_environment(token):
    return {
        "HOME": "/home/qdk-review",
        "USER": "qdk-review",
        "PATH": "/opt/copilot/bin:/opt/review-tools/bin:/usr/bin:/bin:/opt/qdk-venv/bin",
        "RUSTUP_HOME": "/opt/review-rustup",
        "RUSTUP_TOOLCHAIN": "1.95",
        "CARGO_HOME": "/work/dependencies/cargo",
        "CARGO_NET_OFFLINE": "true",
        "PIP_NO_INDEX": "1",
        "PIP_DISABLE_PIP_VERSION_CHECK": "1",
        "npm_config_offline": "true",
        "COPILOT_AUTO_UPDATE": "false",
        "COPILOT_GITHUB_TOKEN": token,
        "LANG": "C.UTF-8",
    }


def prompt():
    return """Use /opt/compiler-review-kit/reviewer.md and its tools to review
the immutable revisions in /work/review.json. /work/scope.json maps PR boundaries
to layers. Run the same relevant cases at the reference, every layer, and candidate;
attribute regressions and fixes to PR boundaries, including intermediate hazards.

Environment adaptation: this is a disposable GitHub runner with mandatory native
Copilot CLI sandboxing, not the interactive Docker sandbox. Its filesystem layout
and /opt/qdk-venv environment implement the kit's sandbox paths. No seed/outbox
exists. Revisions are already fetched and pinned: do not fetch or update them.
The authoritative toolkit is /opt/compiler-review-kit, not a reviewed revision.
Do not load repository hooks, MCP configuration, or plugins.

Dependency retrieval is blocked. Cargo, pip and npm are offline. The newest commit
was prebuilt to seed dependencies. Older layers may need unavailable dependencies;
record failures and incomplete coverage explicitly, never bypass the sandbox,
install dependencies, substitute another revision, or claim a clean review.
Do not ask questions: document unresolved ambiguity and stop that investigation.
Authenticated GitHub tools are unavailable; public GitHub reads are permitted.
Never create issues, comments, PRs, pushes, or remote session exports.

Write /exchange/from_copilot/report.md with inline Q# and standalone Python repros.
Also write repro files under repros/ and issue drafts under issue-drafts/ there.
Generate and execute repros with the kit before writing issue drafts.
Continuously save a partial report; label final coverage complete or incomplete.
Reports and logs will be public artifacts. Only public code/context is supplied.
Do not include secrets or dump environment variables or credential files.
"""


def run_review(args):
    logs = Path(args.logs)
    logs.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    execution = {
        "status": "startup_failed", "exit_code": None, "model": args.model,
        "coverage": "unverified; inspect report and evidence, not the CLI exit code",
    }
    try:
        execute_review(args, logs, execution)
    finally:
        execution["seconds"] = time.monotonic() - started
        write_json(logs / "execution.json", execution)


def execute_review(args, logs, execution):
    token = os.environ["COPILOT_GITHUB_TOKEN"]
    installed = command("/opt/copilot/bin/copilot", "--version")
    execution["cli_version"] = installed
    if "1.0.97" not in installed:
        raise WorkflowError(f"Unexpected Copilot version: {installed}")
    model = args.model
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._/-]*", model):
        raise WorkflowError("Set COMPILER_REVIEW_MODEL to an approved Copilot model identifier")
    environment = review_environment(token)
    inherited = ",".join(key for key in environment if key not in ("HOME", "USER", "PATH"))
    executable = [
        "/usr/bin/sudo", "-H", "-u", "qdk-review", f"--preserve-env={inherited}",
        "--", "/usr/bin/env", f"PATH={environment['PATH']}",
        "/opt/copilot/bin/copilot", "--sandbox", "--no-auto-update",
        "--no-custom-instructions", "--disable-builtin-mcps", "--no-ask-user",
        "--no-remote-export", "--secret-env-vars=COPILOT_GITHUB_TOKEN",
        "--allow-all-tools", "--add-dir=/work", "--add-dir=/exchange/from_copilot",
        "--add-dir=/opt/compiler-review-kit",
        "--allow-url=github.com", "--allow-url=api.github.com",
        "--excluded-tools=web_search", "--model", model, "--output-format=json",
        "--share=/exchange/from_copilot/session.md", "-p", prompt(),
    ]
    with (logs / "session.jsonl").open("wb") as stdout, (logs / "stderr.txt").open("wb") as stderr:
        process = subprocess.Popen(
            executable, cwd="/work/qdk", stdout=stdout, stderr=stderr, start_new_session=True,
            env=environment,
        )
        execution["status"] = "running"
        try:
            code = process.wait(timeout=180 * 60)
            status = "agent_exited" if code == 0 else "agent_failed"
            execution.update(status=status, exit_code=code)
        except subprocess.TimeoutExpired:
            status, code = "timed_out", 124
            execution.update(status=status, exit_code=code)
        finally:
            try:
                os.killpg(process.pid, signal.SIGTERM)
                time.sleep(2)
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            stop_worker()
    if code:
        raise WorkflowError(f"Review {status}; partial artifacts will still be collected")


def stop_worker():
    """Quiesce this run's dedicated account before reading its writable output."""
    uid = pwd.getpwnam("qdk-review").pw_uid
    for _ in range(5):
        processes = []
        for entry in Path("/proc").iterdir():
            if not entry.name.isdigit():
                continue
            try:
                if entry.stat().st_uid == uid:
                    state = (entry / "stat").read_text().rsplit(")", 1)[1].split()[0]
                    if state != "Z":
                        processes.append(int(entry.name))
            except FileNotFoundError:
                continue
        if not processes:
            return
        for sig in (signal.SIGSTOP, signal.SIGKILL):
            for pid in processes:
                try:
                    os.kill(pid, sig)
                except ProcessLookupError:
                    pass
        time.sleep(0.2)
    raise WorkflowError("Worker processes remain; refusing artifact collection")


def collect(args):
    stop_worker()
    destination = Path(args.output)
    destination.mkdir(parents=True, exist_ok=True)
    secrets = [value for key, value in os.environ.items() if key.endswith("TOKEN") and len(value) >= 8]
    count, total = 0, 0
    manifest = []
    sources = [("report", Path(args.report)), ("diagnostics", Path(args.logs))]
    for prefix, root in sources:
        if not root.is_dir() or root.is_symlink():
            raise WorkflowError(f"Missing or unsafe artifact directory: {root}")
        for source in sorted(root.rglob("*")):
            if source.is_symlink() or not (source.is_file() or source.is_dir()):
                raise WorkflowError(f"Unsafe artifact entry: {source}")
            if source.is_dir():
                continue
            size = source.stat().st_size
            count, total = count + 1, total + size
            if count > 1000 or size > 32 * 1024**2 or total > 128 * 1024**2:
                raise WorkflowError("Artifact collection exceeded limits")
            text = source.read_text(encoding="utf-8")
            for secret in secrets:
                text = text.replace(secret, "[REDACTED]")
            text = re.sub(r"\b(?:gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,})\b", "[REDACTED]", text)
            relative = Path(prefix) / source.relative_to(root)
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text)
            manifest.append({"path": str(relative), "sha256": hashlib.sha256(text.encode()).hexdigest()})
    if not (destination / "report/report.md").is_file():
        (destination / "report").mkdir(exist_ok=True)
        text = (
            "# Incomplete compiler review\n\nNo report was produced. See diagnostics; "
            "this is an infrastructure/agent failure, not a clean review.\n"
        )
        (destination / "report/report.md").write_text(text)
        manifest.append({"path": "report/report.md", "sha256": hashlib.sha256(text.encode()).hexdigest()})
    write_json(destination / "files.json", manifest)


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    resolve_parser = commands.add_parser("resolve")
    resolve_parser.add_argument("--repository", required=True)
    resolve_parser.add_argument("--pr", default="")
    resolve_parser.add_argument("--reference", default="")
    resolve_parser.add_argument("--candidate", default="")
    resolve_parser.add_argument("--output", required=True)
    for name in ("prepare", "configure"):
        child = commands.add_parser(name)
        child.add_argument("--scope", required=True)
        if name == "prepare":
            child.add_argument("--repository", required=True)
            child.add_argument("--output", required=True)
        else:
            child.add_argument("--archive", required=True)
    child = commands.add_parser("run")
    child.add_argument("--model", required=True)
    child.add_argument("--logs", required=True)
    child = commands.add_parser("collect")
    child.add_argument("--report", required=True)
    child.add_argument("--logs", required=True)
    child.add_argument("--output", required=True)
    return result


def main():
    args = parser().parse_args()
    try:
        handlers = {
            "resolve": resolve, "prepare": prepare, "configure": configure,
            "run": run_review, "collect": collect,
        }
        handlers[args.command](args)
    except (WorkflowError, OSError, ValueError) as error:
        print(f"compiler review workflow: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
