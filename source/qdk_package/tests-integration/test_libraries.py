import pytest
from qdk import Context
from qdk.test_utils import run_tests


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
    run_tests(Context(project_root=f"library/{library_name}"))


# Use this test case for library development.
# Run with:
# pytest source/qdk_package/tests-integration/test_libraries.py::test_single -s
def test_single():
    ctx = Context(project_root="library/table_lookup")
    run_tests(ctx, verbose=3, regex="TestLookupMatchesStd")
