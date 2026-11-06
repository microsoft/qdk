# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Extract test programs with explicit expected values by instrumenting an owned worktree."""

import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess

from harness.builds import locked, offline_failure, refresh_timestamps
from harness.common import ReviewError, git, read_json, write_json
from harness.discovery import infer_entry
from harness.records import double, validate_value

DUMP_HELPER = r'''
// ---- REVIEW-KIT INSTRUMENTATION (owned extraction worktree only) ----
#[cfg(test)]
pub(crate) fn review_dump(kind: &str, source: &str, expected: Option<String>) {
    use std::io::Write;
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let Ok(path) = std::env::var("REVIEW_DUMP") else {
        return;
    };
    let escape = |s: &str| {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    };
    let test = std::thread::current().name().unwrap_or("?").to_string();
    let line = format!(
        "{{\"kind\":{},\"test\":{},\"source\":{},\"expected\":{}}}\n",
        escape(kind),
        escape(&test),
        escape(source),
        expected.unwrap_or_else(|| "null".to_string())
    );
    let _guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("review dump file");
    file.write_all(line.as_bytes()).expect("review dump write");
}
'''

EVAL_VALUE_HELPER = r'''
#[cfg(test)]
pub(crate) fn review_value_json(value: &qsc_eval::val::Value) -> Option<String> {
    use qsc_eval::val::{Result as R, Value as V};
    Some(match value {
        V::Int(n) => format!("{{\"type\":\"int\",\"value\":{n}}}"),
        V::Bool(b) => format!("{{\"type\":\"bool\",\"value\":{b}}}"),
        V::Double(d) => format!("{{\"type\":\"double_bits\",\"value\":{}}}", d.to_bits()),
        V::Result(R::Val(b)) => format!("{{\"type\":\"result\",\"value\":{}}}", u8::from(*b)),
        V::Tuple(items, _) if items.is_empty() => "{\"type\":\"unit\"}".to_string(),
        V::Tuple(items, _) => format!(
            "{{\"type\":\"tuple\",\"items\":[{}]}}",
            items.iter().map(review_value_json).collect::<Option<Vec<_>>>()?.join(",")
        ),
        V::Array(items) => format!(
            "{{\"type\":\"array\",\"items\":[{}]}}",
            items.iter().map(review_value_json).collect::<Option<Vec<_>>>()?.join(",")
        ),
        _ => return None,
    })
}
'''

# Hook sites are data: when a helper is renamed, extraction fails loudly instead of
# silently producing an empty corpus. Override with --hooks FILE (same JSON shape).
DEFAULT_HOOKS = {
    "qsc_fir_transforms": {
        "helper_file": "source/compiler/qsc_fir_transforms/src/test_utils.rs",
        "helper_code": DUMP_HELPER + EVAL_VALUE_HELPER,
        "hooks": [
            {
                "file": "source/compiler/qsc_fir_transforms/src/test_utils.rs",
                "function": "check_semantic_equivalence_impl",
                "code": 'if let Some(value) = &expected_value { crate::test_utils::review_dump("semantic", source, crate::test_utils::review_value_json(value)); }',
            },
            {
                "file": "source/compiler/qsc_fir_transforms/src/defunctionalize/codegen_tests.rs",
                "function": "check_qir_int_result",
                "code": 'crate::test_utils::review_dump("qir_int", source, Some(format!("{{\\"type\\":\\"int\\",\\"value\\":{expected}}}")));',
            },
        ],
    }
}


def insert_at_function_start(text, function, code):
    """Insert code as the first statement of `fn function`; raise if the site is not unique."""
    matches = list(re.finditer(rf"\bfn\s+{re.escape(function)}\b", text))
    if len(matches) != 1:
        raise ReviewError(f"Expected exactly one `fn {function}`, found {len(matches)}")
    index, depth, opened = matches[0].end(), 0, False
    while index < len(text):
        char = text[index]
        if char == "(":
            depth, opened = depth + 1, True
        elif char == ")":
            depth -= 1
        elif char == "{" and opened and depth == 0:
            return text[: index + 1] + "\n    " + code + text[index + 1 :]
        index += 1
    raise ReviewError(f"Could not find the body of `fn {function}`")


def instrument(worktree, spec):
    worktree = Path(worktree)
    edits = {}
    for hook in spec["hooks"]:
        path = worktree / hook["file"]
        text = edits.get(path, path.read_text())
        edits[path] = insert_at_function_start(text, hook["function"], hook["code"])
    helper = worktree / spec["helper_file"]
    edits[helper] = edits.get(helper, helper.read_text()) + spec["helper_code"]
    for path, text in edits.items():
        path.write_text(text)
    return sorted(str(path.relative_to(worktree)) for path in edits)


def typed(value):
    if value["type"] == "double_bits":
        return double(struct.unpack("<d", struct.pack("<Q", value["value"]))[0])
    if value["type"] in ("tuple", "array"):
        return {"type": value["type"], "items": [typed(item) for item in value["items"]]}
    return value


def convert_dump(lines, revision, crate):
    """Dump records -> schema-valid cases with source-hash IDs; conflicting expectations are dropped."""
    by_source, stats = {}, {"records": 0, "unsupported_value": 0, "no_entry": 0, "conflict": 0}
    for line in lines:
        if not line.strip():
            continue
        stats["records"] += 1
        record = json.loads(line)
        if record["expected"] is None:
            stats["unsupported_value"] += 1
            continue
        expected = typed(record["expected"])
        previous = by_source.get(record["source"])
        if previous is not None:
            if previous["expected"] != expected:
                previous["conflict"] = True
            continue
        by_source[record["source"]] = {"expected": expected, "test": record["test"], "kind": record["kind"]}
    cases = []
    for source, info in by_source.items():
        if info.get("conflict"):
            stats["conflict"] += 1
            continue
        entry = infer_entry(source)
        if entry is None:
            stats["no_entry"] += 1
            continue
        validate_value(info["expected"])
        kind = re.sub(r"[^A-Za-z0-9_.-]", "-", info["kind"])
        cases.append({
            "id": f"t-{kind}-{hashlib.sha256(source.encode()).hexdigest()[:12]}",
            "source": source,
            "entry": entry,
            "oracle": {"kind": "deterministic", "expected": info["expected"]},
            "origin": f"{revision[:12]} {crate} test {info['test']} (extracted)",
            "notes": "Expected value is the test's explicit assertion; the source hash keeps IDs stable across revisions.",
        })
    cases.sort(key=lambda case: case["id"])
    return cases, stats


def extract_tests(config, sha, crate, output, hooks_path=None, test_filter=None):
    specs = read_json(hooks_path) if hooks_path else DEFAULT_HOOKS
    if crate not in specs:
        raise ReviewError(f"No extraction hooks for {crate}; supply --hooks")
    spec = specs[crate]
    work = Path(config["work_dir"])
    builds = Path(config["build_dir"])
    builds.mkdir(parents=True, exist_ok=True)
    worktree = builds / "extract" / sha
    dump = work / "extract" / f"{sha[:12]}-{crate}.jsonl"
    log = work / "extract" / f"{sha[:12]}-{crate}.log"
    dump.parent.mkdir(parents=True, exist_ok=True)
    dump.unlink(missing_ok=True)
    # Extractions share worktrees and a target directory, and instrument tracked files in place.
    with locked(builds / "extract.lock"):
        if not worktree.exists():
            with locked(builds / "build.lock"):  # serializes Git worktree registration with builds
                worktree.parent.mkdir(parents=True, exist_ok=True)
                git(config["repository"], "worktree", "add", "--detach", str(worktree), sha)
        if git(worktree, "rev-parse", "HEAD") != sha or git(worktree, "status", "--porcelain"):
            raise ReviewError(f"Extraction worktree is not a clean checkout of {sha}: {worktree}")
        try:
            edited = instrument(worktree, spec)
            refresh_timestamps(worktree)
            env = dict(
                os.environ,
                REVIEW_DUMP=str(dump),
                CARGO_TARGET_DIR=str(builds / "extract-target"),
                CARGO_INCREMENTAL="0",
                PYO3_PYTHON=str(Path(config["venv"]) / "bin/python"),
            )
            args = ["cargo", "test", "--locked", "-p", crate, "--lib"]
            if test_filter:
                args += ["--", test_filter]
            print(f"Running {' '.join(args)} at {sha[:12]}; log: {log}", flush=True)
            with log.open("w") as stream:
                status = subprocess.run(args, cwd=worktree, env=env, stdout=stream, stderr=stream).returncode
        finally:
            git(worktree, "checkout", "--", ".")
    if not dump.exists():
        raise offline_failure(log, sha, f"Testing {crate}") or ReviewError(
            f"No dump produced (cargo exit {status}); see {log}"
        )
    cases, stats = convert_dump(dump.read_text().splitlines(), sha, crate)
    write_json(output, cases)
    write_json(
        Path(output).with_suffix(".extraction.json"),
        {"revision": sha, "crate": crate, "edited_files": edited, "cargo_exit": status,
         "log": str(log), "dump": str(dump), "stats": stats, "cases": len(cases)},
    )
    return len(cases), stats, status
