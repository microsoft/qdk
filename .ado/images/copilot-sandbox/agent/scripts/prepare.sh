#!/usr/bin/env bash
set -euo pipefail
[[ $PACKAGE_SOURCE == public || $PACKAGE_SOURCE == azure ]]
export HOME=/tmp/build-home
if [[ -f /run/secrets/npmrc ]]; then
    export NPM_CONFIG_USERCONFIG=/run/secrets/npmrc
else
    export NPM_CONFIG_USERCONFIG=/dev/null
    export NPM_CONFIG_REGISTRY=https://registry.npmjs.org/
fi
export npm_config_logs_dir=/tmp/npm-logs
export PIP_INDEX_URL
if [[ $PACKAGE_SOURCE == azure ]]; then
    PIP_INDEX_URL=$(</run/secrets/pip_index_url)
    export CARGO_REGISTRIES_SANDBOX_FEED_TOKEN="Bearer $(</run/secrets/cargo_token)"
else
    PIP_INDEX_URL=https://pypi.org/simple/
fi
export PIP_EXTRA_INDEX_URL= PIP_CONFIG_FILE=/dev/null PIP_NO_CACHE_DIR=1 PIP_DISABLE_PIP_VERSION_CHECK=1
export PYRIGHT_PYTHON_GLOBAL_NODE=1 PYRIGHT_PYTHON_NODEJS_WHEEL=0
export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
mkdir -p "$HOME" "$CARGO_HOME" "$npm_config_cache" "$PYRIGHT_PYTHON_CACHE_DIR"
python3 - <<'PY'
import json
import os
from pathlib import Path
import subprocess
from urllib.parse import urlsplit

registry = subprocess.check_output(["npm", "config", "get", "registry"], text=True).strip()
sources = [("pip", os.environ["PIP_INDEX_URL"]), ("npm", registry)]
azure = os.environ["PACKAGE_SOURCE"] == "azure"
if azure:
    sources.append(("cargo", os.environ["CRATES_FEED"]))
for name, url in sources:
    feed = urlsplit(url)
    if (
        feed.scheme != "https"
        or not (
            (azure and (
                feed.hostname == "pkgs.dev.azure.com"
                or (feed.hostname or "").endswith(".pkgs.visualstudio.com")
            ))
            or (not azure and feed.hostname == {"pip": "pypi.org", "npm": "registry.npmjs.org"}.get(name))
        )
        or feed.query or feed.fragment
        or (name != "pip" and feed.username is not None)
        or any(character.isspace() for character in url)
    ):
        raise ValueError("Package URL does not match the selected package source; npm/Cargo URLs must not contain credentials")

Path("/opt/sandbox/npmrc").write_text("registry=" + registry + "\n")
if azure:
    index = json.dumps("sparse+" + os.environ["CRATES_FEED"].rstrip("/") + "/")
    Path(os.environ["CARGO_HOME"], "config.toml").write_text(
        '[source.crates-io]\nreplace-with = "sandbox-feed"\n'
        '[source.sandbox-feed]\nregistry = ' + index + '\n'
        '[registries.sandbox-feed]\nindex = ' + index + '\n'
        'credential-provider = ["cargo:token"]\n'
    )
PY
python3 -m venv "$VIRTUAL_ENV"
cd /opt/sandbox/copilot
npm ci --ignore-scripts --no-audit --no-fund
./node_modules/.bin/copilot --version
bash /opt/sandbox/scripts/build_qdk.sh
