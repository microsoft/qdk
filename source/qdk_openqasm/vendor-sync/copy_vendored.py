# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Copies the vendored compiler files from their origins.

Only copy-tier pairs are copied. A fork is maintained by hand, so this refuses
to overwrite one rather than silently discarding the divergence.

    python copy_vendored.py                    # copy every copy-tier pair
    python copy_vendored.py --file span.rs     # copy one pair
    python copy_vendored.py --dry-run          # report what would change
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MANIFEST = Path(__file__).resolve().parent / "manifest.json"


def load_pairs() -> list[dict]:
    """Returns the manifest's pairs."""
    return json.loads(MANIFEST.read_text(encoding="utf-8"))["pairs"]


def select(pairs: list[dict], name: str | None) -> list[dict]:
    """Returns the pairs to copy, rejecting a name that is unknown or a fork."""
    if name is None:
        return [pair for pair in pairs if pair["tier"] == "copy"]

    matches = [pair for pair in pairs if pair["name"] == name]
    if not matches:
        known = ", ".join(pair["name"] for pair in pairs)
        raise SystemExit(f"unknown file {name!r}. Known files: {known}")

    pair = matches[0]
    if pair["tier"] == "fork":
        raise SystemExit(
            f"{name} is a fork, not a copy, so there is nothing to copy from the "
            f"origin: {pair['divergence']}\n"
            "Port the origin's change by hand, then re-pin with "
            "`check_vendor_sync.py --update`."
        )
    return [pair]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--file", help="copy only this manifest entry, by name")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="report what would change without writing",
    )
    args = parser.parse_args()

    changed = []
    for pair in select(load_pairs(), args.file):
        vendored = ROOT / pair["vendored"]
        contents = (ROOT / pair["origin"]).read_bytes()
        if vendored.exists() and vendored.read_bytes() == contents:
            continue

        changed.append(pair["name"])
        if not args.dry_run:
            vendored.parent.mkdir(parents=True, exist_ok=True)
            vendored.write_bytes(contents)

    if not changed:
        print("already in sync; nothing copied")
        return 0

    verb = "would copy" if args.dry_run else "copied"
    print(f"{verb}: " + ", ".join(changed))
    return 0


if __name__ == "__main__":
    sys.exit(main())
