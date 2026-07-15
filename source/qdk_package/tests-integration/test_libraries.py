from pathlib import Path

import pytest
from qdk import Context
from qdk.test_utils import run_tests

# Directory with all Q# libraries.
_LIB_DIR = str(Path(__file__).resolve().parents[3] / "library")


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
    run_tests(context=Context(project_root=f"{_LIB_DIR}/{library_name}"))


# Use this test case for library development.
# Run with:
# pytest source/qdk_package/tests-integration/test_libraries.py::test_single -s
def test_single():
    ctx = Context(project_root=f"{_LIB_DIR}/table_lookup")
    run_tests(context=ctx, verbose=3, regex="TestLookupMatchesStd")
