# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Reports drift between the vendored compiler files and their origins.

Run from anywhere; paths in the manifest are relative to the repository root.
Exits non-zero when a pair has drifted, and prints what to do about it.

    python check_vendor_sync.py            # report drift
    python check_vendor_sync.py --update   # re-pin reviewed fork origins
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MANIFEST = Path(__file__).resolve().parent / "manifest.json"
REGENERATE = "python3 source/qdk_openqasm/vendor-sync/copy_vendored.py"


def sha256(path: Path) -> str:
    """Returns the hex digest of the file's bytes."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_pairs() -> list[dict]:
    """Returns the manifest's pairs."""
    return json.loads(MANIFEST.read_text(encoding="utf-8"))["pairs"]


def check_copy(pair: dict) -> str | None:
    """Returns a drift report for a copy pair, or None when it is in sync."""
    vendored = ROOT / pair["vendored"]
    origin = ROOT / pair["origin"]

    if not vendored.exists():
        return f"the vendored file is missing; restore it with `{REGENERATE}`"
    if not origin.exists():
        return (
            f"the origin {pair['origin']} is missing, so this pair no longer has "
            "a source; either point the manifest at the origin's new home or "
            "promote the pair to a fork"
        )
    if vendored.read_bytes() == origin.read_bytes():
        return None

    return (
        f"the vendored copy no longer matches {pair['origin']}\n"
        f"    if the origin changed: copy it over with `{REGENERATE} --file "
        f"{pair['name']}`\n"
        "    if the vendored copy was edited directly: that edit belongs in the "
        "origin, so move it there and copy back, or make this pair a fork in "
        "manifest.json and say why"
    )


def check_fork(pair: dict) -> str | None:
    """Returns a drift report for a fork pair, or None when nothing to review."""
    origin = ROOT / pair["origin"]

    if not (ROOT / pair["vendored"]).exists():
        return "the vendored file is missing; a fork has no other copy, so restore it from git"
    if not origin.exists():
        return (
            f"the origin {pair['origin']} is missing, so this fork has nothing "
            "left to track; drop origin_sha256 from the manifest and note that "
            "the fork now stands alone"
        )
    if sha256(origin) == pair["origin_sha256"]:
        return None

    return (
        f"{pair['origin']} changed since this fork was last reviewed against it\n"
        f"    this file is a deliberate fork, so nothing is copied automatically: "
        f"{pair['divergence']}\n"
        "    read the origin's change, port it by hand if it applies, then re-pin "
        "with `python3 source/qdk_openqasm/vendor-sync/check_vendor_sync.py --update`"
    )


def report(pairs: list[dict]) -> int:
    """Prints a report for every drifted pair and returns the process exit code."""
    drifted = []
    for pair in pairs:
        problem = check_copy(pair) if pair["tier"] == "copy" else check_fork(pair)
        if problem is not None:
            drifted.append((pair, problem))

    if not drifted:
        print(f"vendored files are in sync with their origins ({len(pairs)} pairs)")
        return 0

    print("Vendored file drift\n")
    for pair, problem in drifted:
        exposure = (
            "some of it is re-exported from this crate's public API"
            if pair["public"]
            else "it is crate-private"
        )
        print(f"  {pair['name']} ({pair['tier']}, {exposure})")
        print(f"    {problem}\n")

    print(f"{len(drifted)} of {len(pairs)} pairs drifted. See vendor-sync/README.md.")
    return 1


def update(pairs: list[dict]) -> int:
    """Re-pins each fork's origin hash and reports what moved."""
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    repinned = []
    for entry in manifest["pairs"]:
        if entry["tier"] != "fork":
            continue
        current = sha256(ROOT / entry["origin"])
        if current != entry["origin_sha256"]:
            entry["origin_sha256"] = current
            repinned.append(entry["name"])

    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    if repinned:
        print("re-pinned: " + ", ".join(repinned))
    else:
        print("no fork origin moved; manifest unchanged")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--update",
        action="store_true",
        help="re-pin fork origin hashes after reviewing the origin's change",
    )
    args = parser.parse_args()

    pairs = load_pairs()
    return update(pairs) if args.update else report(pairs)


if __name__ == "__main__":
    sys.exit(main())
