# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Per-commit outcome matrices and bounded, predicate-preserving reduction."""

import ast
from dataclasses import dataclass, field
from pathlib import Path
import re
import tempfile

from harness.builds import build_revision
from harness.cases import export_cases, load_cases, read_case_manifest
from harness.common import ReviewError, git, is_ancestor, preflight, resolve_ref, write_json
from harness.execution import prepare_builds
from harness.runner import compare, endpoint, run_case, stage


def attribute(config, cases_path, case_id, output, refs=None, mode="release"):
    tools = preflight(config)
    baseline, candidate = config["refs"]["reference"], config["refs"]["candidate"]
    if refs:
        revisions = [baseline] + [resolve_ref(config["repository"], ref) for ref in refs]
    else:
        if not is_ancestor(config["repository"], baseline, candidate):
            raise ReviewError("Attribution across non-ancestral refs requires an explicit ordered --refs list.")
        revisions = [baseline] + git(
            config["repository"], "rev-list", "--reverse", "--topo-order", f"{baseline}..{candidate}"
        ).splitlines()
    revisions = list(dict.fromkeys(revisions))
    work = Path(config["work_dir"])
    work.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="attribute-", dir=work))
    cases, _ = load_cases(cases_path, config, directory / "inputs")
    case = next((case for case in cases if case["id"] == case_id), None)
    if case is None:
        raise ReviewError(f"Case not found or excluded: {case_id}")
    rows, reference = [], None
    for sha in revisions:
        build = build_revision(config, sha, mode, tools)
        result = run_case(config, build, case, directory / sha)
        if reference is None:
            reference = result
        row = {
            "sha": sha,
            "parents": git(config["repository"], "show", "-s", "--format=%P", sha).split(),
            "outcomes": {p: endpoint(case, result, p) for p in case["profiles"]},
            "comparison": compare(case, reference, result),
            "result": result,
        }
        rows.append(row)
        write_json(output, {"case": case, "tools": tools, "rows": rows, "complete": False})
        print(f"{sha[:12]}: {row['outcomes']}", flush=True)
    write_json(output, {"case": case, "tools": tools, "rows": rows, "complete": True})
    return rows


def ddmin(parts, test):
    granularity = 2
    while len(parts) >= 2:
        size = (len(parts) + granularity - 1) // granularity
        for start in range(0, len(parts), size):
            candidate = parts[:start] + parts[start + size :]
            if candidate and test(candidate):
                parts = candidate
                granularity = max(2, granularity - 1)
                break
        else:
            if granularity >= len(parts):
                break
            granularity = min(len(parts), granularity * 2)
    return parts


@dataclass
class ReductionPredicate:
    config: dict
    tools: dict
    case: dict
    directory: Path
    kind: str
    profile: str
    needle: str
    attempts: int
    preserve_regression: bool
    tested: int = 0
    cache: dict = field(default_factory=dict)
    builds: dict = field(default_factory=dict)

    def build(self, role):
        sha = self.config["refs"][role]
        prepare_builds(self.config, [(role, sha)], "release", self.builds, self.tools)
        return self.builds[sha]

    def __call__(self, text):
        if text in self.cache:
            return self.cache[text]
        if self.tested >= self.attempts:
            return False
        self.tested += 1
        attempt = self.directory / str(self.tested)
        if "adapter" in self.case:
            attempt.mkdir(parents=True)
            adapter = attempt / "adapter.py"
            adapter.write_text(text)
            current = self.case | {"adapter": str(adapter)}
        else:
            current = self.case | {"source": text}
        build = self.build("candidate")
        if self.kind == "wrong_result":
            candidate = run_case(self.config, build, current, attempt / "candidate")
        else:
            # Most variants do not retain the compile failure; do not simulate them.
            compiled = stage(self.config, build, current, "compile", self.profile)
            candidate = {"profiles": {self.profile: {"compile": compiled}}}
        matched = endpoint(current, candidate, self.profile) == self.kind
        if matched and self.needle:
            compiled = candidate["profiles"][self.profile]["compile"]
            matched = self.needle in (str(compiled.get("message", "")) + compiled.get("stderr", ""))
        if matched and self.preserve_regression:
            if "simulator" not in candidate and current["oracle"]["kind"] != "compile_only":
                candidate["simulator"] = stage(self.config, build, current, "simulate")
            reference = run_case(self.config, self.build("reference"), current, attempt / "reference")
            matched = compare(current, reference, candidate)[0]["verdict"] == "regression_candidate"
        self.cache[text] = matched
        return matched


def reduce_case(config, cases_path, case_id, output, kind, profile, needle, attempts, preserve_regression):
    if kind in ("panic", "rejected") and not needle:
        raise ReviewError("Panic/rejection reduction requires --needle to preserve the failure signature.")
    tools = preflight(config)
    work = Path(config["work_dir"])
    work.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="reduce-", dir=work))
    cases, _ = load_cases(cases_path, config, directory / "inputs")
    original = next((case for case in cases if case["id"] == case_id), None)
    if original is None or "source" not in original or any(k in original for k in ("project_root", "adapter")):
        raise ReviewError("Reduction requires a standalone source case; minimize projects/adapters manually first.")
    if profile not in original["profiles"]:
        raise ReviewError(f"Case does not select profile {profile}")
    case = original | {"profiles": [profile]}
    predicate = ReductionPredicate(config, tools, case, directory, kind, profile, needle, attempts, preserve_regression)

    if not predicate(case["source"]):
        raise ReviewError("Input does not satisfy the requested failure/regression predicate.")
    lines = ddmin(case["source"].splitlines(keepends=True), lambda parts: predicate("".join(parts)))
    reduced = "".join(lines)
    clean = dict(original)
    clean.update(source=reduced, profiles=[profile])
    write_json(output, [clean])
    write_json(
        Path(output).with_suffix(".reduction.json"),
        {
            "attempts": predicate.tested,
            "budget_exhausted": predicate.tested >= attempts,
            "kind": kind,
            "profile": profile,
            "needle": needle,
            "preserve_regression": preserve_regression,
            "reference": config["refs"]["reference"],
            "candidate": config["refs"]["candidate"],
            "original_lines": len(case["source"].splitlines()),
            "reduced_lines": len(lines),
        },
    )
    return predicate.tested, len(lines)


def _prepare_nodes(tree):
    prepare = next(
        (node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "prepare"), None
    )
    if prepare is None:
        raise ReviewError("Adapter has no top-level prepare(ctx)")
    return [node for node in ast.walk(prepare) if isinstance(node, (ast.Call, ast.List))]


def render_adapter(tree):
    """Unparse an adapter, keeping embedded multi-line Q# strings readable."""
    parts = []
    for statement in tree.body:
        value = getattr(statement, "value", None)
        if (
            isinstance(statement, ast.Assign) and len(statement.targets) == 1
            and isinstance(statement.targets[0], ast.Name) and isinstance(value, ast.Constant)
            and isinstance(value.value, str) and "\n" in value.value
            and '"""' not in value.value and "\\" not in value.value
        ):
            parts.append(f'{statement.targets[0].id} = """{value.value}"""')
        else:
            parts.append(ast.unparse(statement))
    return "\n\n".join(parts) + "\n"


def adapter_variants(text):
    """One-step simplifications of prepare(): unwrap a call to a call/list argument, or drop a list element."""
    count = len(_prepare_nodes(ast.parse(text)))
    for index in range(count):
        node = _prepare_nodes(ast.parse(text))[index]
        if isinstance(node, ast.Call):
            options = [arg for arg in node.args if isinstance(arg, (ast.Call, ast.List))]
        else:
            options = [
                ast.List(elts=node.elts[:j] + node.elts[j + 1 :], ctx=ast.Load()) for j in range(len(node.elts))
            ] if len(node.elts) > 1 else []
        for option in range(len(options)):
            tree = ast.parse(text)
            target = _prepare_nodes(tree)[index]
            replacement = (
                [arg for arg in target.args if isinstance(arg, (ast.Call, ast.List))][option]
                if isinstance(target, ast.Call)
                else ast.List(elts=target.elts[:option] + target.elts[option + 1 :], ctx=ast.Load())
            )

            class Replace(ast.NodeTransformer):
                def visit(self, node):
                    return replacement if node is target else super().visit(node)

            yield render_adapter(ast.fix_missing_locations(Replace().visit(tree)))


def qsharp_constants(text):
    """Module-level string constants that look like Q# source."""
    return [
        statement.targets[0].id
        for statement in ast.parse(text).body
        if isinstance(statement, ast.Assign) and len(statement.targets) == 1
        and isinstance(statement.targets[0], ast.Name) and isinstance(statement.value, ast.Constant)
        and isinstance(statement.value.value, str)
        and re.search(r"\b(operation|function|namespace)\b", statement.value.value)
    ]


def replace_constant(text, name, value):
    tree = ast.parse(text)
    for statement in tree.body:
        if isinstance(statement, ast.Assign) and getattr(statement.targets[0], "id", None) == name:
            statement.value = ast.Constant(value)
    return render_adapter(ast.fix_missing_locations(tree))


def constant_value(text, name):
    for statement in ast.parse(text).body:
        if isinstance(statement, ast.Assign) and getattr(statement.targets[0], "id", None) == name:
            return statement.value.value
    raise ReviewError(f"No constant {name}")


def reduce_adapter(config, cases_path, case_id, output, kind, profile, needle, attempts, preserve_regression):
    if kind in ("panic", "rejected") and not needle:
        raise ReviewError("Panic/rejection reduction requires --needle to preserve the failure signature.")
    tools = preflight(config)
    work = Path(config["work_dir"])
    work.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="reduce-adapter-", dir=work))
    cases, _ = load_cases(cases_path, config, directory / "inputs")
    original = next((case for case in cases if case["id"] == case_id), None)
    if original is None or "adapter" not in original:
        raise ReviewError("reduce-adapter requires a case with a Python adapter")
    if profile not in original["profiles"]:
        raise ReviewError(f"Case does not select profile {profile}")
    case = original | {"profiles": [profile]}
    predicate = ReductionPredicate(config, tools, case, directory, kind, profile, needle, attempts, preserve_regression)

    text = render_adapter(ast.parse(Path(original["adapter"]).read_text()))
    if not predicate(text):
        raise ReviewError("Adapter does not satisfy the requested failure/regression predicate.")
    changed = True
    while changed and predicate.tested < attempts:
        changed = False
        for variant in adapter_variants(text):
            if variant != text and predicate(variant):
                text, changed = variant, True
                break
        if changed:
            continue
        for name in qsharp_constants(text):
            lines = constant_value(text, name).splitlines(keepends=True)
            reduced = ddmin(lines, lambda parts, n=name: predicate(replace_constant(text, n, "".join(parts))))
            if len(reduced) < len(lines):
                text, changed = replace_constant(text, name, "".join(reduced)), True
    output = Path(output)
    adapter_path = directory / "reduced.py"
    adapter_path.write_text(text)
    export_cases([original | {"adapter": str(adapter_path), "profiles": [profile]}], output)
    adapter_path = Path(read_case_manifest(output)[0]["adapter"])
    write_json(
        output.with_suffix(".reduction.json"),
        {
            "attempts": predicate.tested,
            "budget_exhausted": predicate.tested >= attempts,
            "kind": kind,
            "profile": profile,
            "needle": needle,
            "preserve_regression": preserve_regression,
            "reference": config["refs"]["reference"],
            "candidate": config["refs"]["candidate"],
            "adapter": str(adapter_path),
        },
    )
    return predicate.tested, adapter_path
