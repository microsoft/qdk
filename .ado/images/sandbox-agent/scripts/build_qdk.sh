#!/usr/bin/env bash
set -euo pipefail
repo=$(mktemp -d /tmp/qdk.XXXXXX)
trap 'rm -rf "$repo"' EXIT
tar -C /qdk --exclude=.git --exclude=target --exclude=node_modules --exclude=.venv --exclude=__pycache__ -cf - . | tar -C "$repo" -xf -
cd "$repo"
python - <<'PY'
import json
import os
import subprocess
import sys

arguments = json.loads(os.environ["QDK_BUILD_ARGS"])
if not isinstance(arguments, list) or not all(isinstance(argument, str) for argument in arguments):
    raise ValueError("QDK_BUILD_ARGS must be a JSON array of strings")
subprocess.run(
    [sys.executable, "version.py"],
    check=True,
    env={**os.environ, "BUILD_NUMBER": "0", "BUILD_TYPE": "dev"},
)
subprocess.run([sys.executable, "build.py", *arguments], check=True)
PY
