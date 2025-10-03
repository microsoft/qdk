#!/usr/bin/env python3

# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import os
import re
import sys

# To be updated every time we start a new major.minor version.
major_minor = "1.21"

root_dir = os.path.dirname(os.path.abspath(__file__))
source_dir = os.path.join(root_dir, "source")


def update_file(
    file: str, old_text: str, new_text: str, is_regex: bool = False, count=1
):
    # Open the file and replace the first string matching the old text with the new text
    with open(file, "r+", newline="") as f:
        contents = f.read()
        new_contents = (
            contents.replace(old_text, new_text, count)
            if not is_regex
            else re.sub(old_text, new_text, contents)
        )
        f.seek(0)
        f.write(new_contents)
        f.truncate()


# If the first argument is "--set", update the repo to the version in the second argument.
# This should be a full triple and will update the refs in the library manifests also, e.g.
#  ./version.py --set 1.18.0
# IMPORTANT: This is for convenience and does simple pattern matching. Verify all changes manually before committing.
if len(sys.argv) > 1 and sys.argv[1] == "--set":
    if len(sys.argv) != 3:
        print("Usage: {} --set n.n.n".format(sys.argv[0]))
        exit(1)

    new_version = sys.argv[2]
    # Ensure new version is in the correct format and extract major, minor, and build numbers
    parts = new_version.split(".")
    if len(parts) != 3 or not all(part.isdigit() for part in parts):
        print("Version must be in the format 'n.n.n'")
        exit(1)

    # Update this file
    update_file(
        os.path.join(root_dir, "version.py"),
        r'major_minor = "{}"'.format(major_minor),
        r'major_minor = "{}"'.format(".".join(parts[:2])),
    )

    # Collect the files to update that have a full version reference
    # Update the pre-populated manifest references to the new version
    update_list = [os.path.join(source_dir, "vscode/src/registry.json")]

    # Collect any file named "qsharp.json" under the /library directory
    for root, dirs, files in os.walk(os.path.join(root_dir, "library")):
        update_list += [
            os.path.join(root, file) for file in files if file == "qsharp.json"
        ]

    for file in update_list:
        update_file(
            file,
            r'"ref": "v[0-9.]+"',
            r'"ref": "v{}"'.format(".".join(parts)),
            is_regex=True,
        )

    print("Updated version to {}".format(new_version))

    exit(0)


# Default to 'dev' builds
BUILD_TYPE = os.environ.get("BUILD_TYPE") or "dev"
BUILD_NUMBER = os.environ.get("BUILD_NUMBER")
if not BUILD_NUMBER:
    raise Exception("BUILD_NUMBER environment variable must be set")

if BUILD_TYPE not in ["dev", "rc", "stable"]:
    print("BUILD_TYPE environment variable must be 'dev', 'rc', or 'stable'")
    exit(1)

try:
    build_ver = int(BUILD_NUMBER)
except:
    print("BUILD_NUMBER environment variable must be set to a valid integer")
    exit(1)

print("Build type: {}".format(BUILD_TYPE))

version_triple = "{}.{}".format(major_minor, build_ver)

pip_suffix = {"stable": "", "rc": "rc0", "dev": ".dev0"}
npm_suffix = {"stable": "", "rc": "-rc", "dev": "-dev"}

pip_version = "{}{}".format(version_triple, pip_suffix.get(BUILD_TYPE))
npm_version = "{}{}".format(version_triple, npm_suffix.get(BUILD_TYPE))
# Publish the jupyterlab extension without the 'pre-release' tagging for rc builds.
# It is already stable and the prior publishing (and yanking) of release versions causes issues.
jupyterlab_version = pip_version if BUILD_TYPE == "dev" else version_triple

print("Pip version: {}".format(pip_version))
print("Npm version: {}".format(npm_version))
print("VS Code version: {}".format(version_triple))

update_file(
    os.path.join(source_dir, "pip/pyproject.toml"),
    r'version = "0.0.0"',
    r'version = "{}"'.format(pip_version),
)

update_file(
    os.path.join(source_dir, "pip/qsharp/telemetry.py"),
    r'QSHARP_VERSION = "0.0.0.dev0"',
    r'QSHARP_VERSION = "{}"'.format(pip_version),
)

update_file(
    os.path.join(source_dir, "widgets/pyproject.toml"),
    r'version = "0.0.0"',
    r'version = "{}"'.format(pip_version),
)

update_file(
    os.path.join(source_dir, "jupyterlab/pyproject.toml"),
    r'version = "0.0.0"',
    r'version = "{}"'.format(jupyterlab_version),
)

qdk_pyproject = os.path.join(source_dir, "qdk_package/pyproject.toml")

update_file(
    qdk_pyproject,
    r'version = "0.0.0"',
    r'version = "{}"'.format(pip_version),
)

update_file(
    qdk_pyproject,
    r"qsharp==0.0.0",
    r"qsharp=={}".format(pip_version),
)
update_file(
    qdk_pyproject,
    r"qsharp-widgets==0.0.0",
    r"qsharp-widgets=={}".format(pip_version),
    count=2,
)
update_file(
    qdk_pyproject,
    r"qsharp-jupyterlab==0.0.0",
    r"qsharp-jupyterlab=={}".format(jupyterlab_version),
    count=2,
)

update_file(
    os.path.join(source_dir, "npm/qsharp/package.json"),
    r'"version": "0.0.0",',
    r'"version": "{}",'.format(npm_version),
)

update_file(
    os.path.join(source_dir, "vscode/package.json"),
    r'"version": "0.0.0",',
    r'"version": "{}",'.format(version_triple),
)

# If not a 'dev' build, update the VS Code extension identifier to be the non-dev version
if BUILD_TYPE != "dev":
    update_file(
        os.path.join(source_dir, "vscode/package.json"),
        r'"name": "qsharp-lang-vscode-dev",',
        r'"name": "qsharp-lang-vscode",',
    )
    update_file(
        os.path.join(source_dir, "vscode/package.json"),
        r"[DEV BUILD] Azure Quantum Development Kit",
        r"Azure Quantum Development Kit",
    )

else:
    # Update the README to contain the dev version contents
    with open(
        os.path.join(source_dir, "vscode/README-DEV.md"), "r", newline=""
    ) as dev_readme:
        contents = dev_readme.read()
    with open(os.path.join(source_dir, "vscode/README.md"), "w", newline="") as readme:
        readme.write(contents)
