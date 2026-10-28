# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""New (and removed) diagnostics and panic sites in a diff: each new rejection is a place to probe for
false rejections of valid programs, and each new panic site a place to probe for crashes."""

import re

from harness.common import git

CODE = re.compile(r'code\("([^"]+)"\)')
ERROR = re.compile(r'#\[error\("((?:[^"\\]|\\.)*)"')
PANIC = re.compile(r'\b(panic!|unreachable!|unimplemented!|todo!|assert!|assert_eq!|assert_ne!)\s*\(\s*(?:"((?:[^"\\]|\\.)*)")?')
EXPECT = re.compile(r'\.expect\(\s*"((?:[^"\\]|\\.)*)"')
HUNK = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@")


def is_test_path(path):
    return bool(re.search(r"(^|/)tests?(/|\.rs$)|_tests?\.rs$|/benches/", path))


def scan_diff_text(diff):
    """Parse `git diff -U0` text into added/removed diagnostics and panic sites."""
    found = {"codes_added": [], "codes_removed": [], "errors_added": [], "errors_removed": [], "panics_added": []}
    old_path, new_path, line = None, None, 0
    for raw in diff.splitlines():
        if raw.startswith("+++ "):
            new_path = raw[6:] if raw.startswith("+++ b/") else None
            continue
        if raw.startswith("--- "):
            old_path = raw[6:] if raw.startswith("--- a/") else None
            continue
        hunk = HUNK.match(raw)
        if hunk:
            line = int(hunk.group(1))
            continue
        path = new_path if raw.startswith("+") else old_path
        if path is None or is_test_path(path) or not path.endswith(".rs"):
            continue
        if raw.startswith("+"):
            text = raw[1:]
            for code in CODE.findall(text):
                found["codes_added"].append({"code": code, "path": path, "line": line})
            for message in ERROR.findall(text):
                found["errors_added"].append({"message": message, "path": path, "line": line})
            stripped = text.strip()
            if not stripped.startswith("//"):
                for macro, message in PANIC.findall(text):
                    found["panics_added"].append({"kind": macro, "message": message, "path": path, "line": line})
                for message in EXPECT.findall(text):
                    found["panics_added"].append({"kind": ".expect", "message": message, "path": path, "line": line})
            line += 1
        elif raw.startswith("-"):
            text = raw[1:]
            for code in CODE.findall(text):
                found["codes_removed"].append({"code": code, "path": path})
            for message in ERROR.findall(text):
                found["errors_removed"].append({"message": message, "path": path})
    # A code moved within the diff (removed and re-added) is not new.
    added = {c["code"] for c in found["codes_added"]}
    removed = {c["code"] for c in found["codes_removed"]}
    found["codes_added"] = [c for c in found["codes_added"] if c["code"] not in removed]
    found["codes_removed"] = [c for c in found["codes_removed"] if c["code"] not in added]
    return found


def scan(repo, base, head, paths=("source",)):
    return scan_diff_text(git(repo, "diff", "-U0", "--no-color", base, head, "--", *paths))


def print_scan(found, show=40):
    print(f"New diagnostic codes ({len(found['codes_added'])}): write valid programs at each one's boundary; "
          "they must still compile (false rejections are regressions).")
    for item in found["codes_added"]:
        print(f"  {item['code']}  {item['path']}:{item['line']}")
    print(f"\nNew #[error] messages ({len(found['errors_added'])}):")
    for item in found["errors_added"][:show]:
        print(f"  {item['message'][:100]}  {item['path']}:{item['line']}")
    print(f"\nRemoved diagnostic codes ({len(found['codes_removed'])}): confirm the condition is now handled, "
          "not silently accepted.")
    for item in found["codes_removed"]:
        print(f"  {item['code']}  {item['path']}")
    panics = found["panics_added"]
    print(f"\nNew panic/assert/expect sites outside test-named files ({len(panics)}): try to reach each with valid input.")
    for item in panics[:show]:
        message = f" {item['message'][:80]!r}" if item["message"] else ""
        print(f"  {item['kind']}{message}  {item['path']}:{item['line']}")
    if len(panics) > show:
        print(f"  ... {len(panics) - show} more (use --json)")
