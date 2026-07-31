import json
import os
from pathlib import Path

import pytest
from qdk import Context
from qdk.test_utils import run_tests

# Directory with all Q# libraries.
_LIB_PATH = Path(__file__).resolve().parents[3] / "library"


def patch_dependencies(manifest_path: Path) -> None:
    """Patches dependencies in given Q# manifest to depend on local version of QDK
    libraries instead of the ones on GitHub.
    """
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    updated = False

    for dependency in manifest.get("dependencies", {}).values():
        github = dependency.get("github")
        if not isinstance(github, dict):
            continue

        owner = github.get("owner")
        repo = github.get("repo")
        library_path = github.get("path")
        if (
            isinstance(owner, str)
            and owner.lower() == "microsoft"
            and isinstance(repo, str)
            and repo.lower() == "qdk"
            and isinstance(library_path, str)
        ):
            library_name = Path(library_path).name
            dependency.clear()
            dependency["path"] = str(_LIB_PATH / library_name)
            updated = True

    if updated:
        manifest_path.write_text(
            json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
        )


@pytest.mark.parametrize(
    "library_name",
    [
        "chemistry",
        "fixed_point",
        "qtest",
        "rotations",
        "signed",
        "table_lookup",
    ],
)
def test_library(library_name: str):
    if os.environ.get("CI") is not None:
        patch_dependencies(_LIB_PATH / library_name / "qsharp.json")
    run_tests(context=Context(project_root=f"{_LIB_PATH}/{library_name}"))


# Use this test case for library development.
# Run with:
# pytest source/qdk_package/tests-integration/test_libraries.py::test_single -s
def test_single():
    ctx = Context(project_root=f"{_LIB_PATH}/table_lookup")
    run_tests(context=ctx, verbose=3, regex="TestLookupMatchesStd")
