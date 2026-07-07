import pytest
from qdk import Context


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
    Context(project_root=f"library/{library_name}").run_tests()


# Use this test case for library development.
# Run with:
# pytest source/qdk_package/tests-integration/test_libraries.py::test_single -s
def test_single():
    ctx = Context(project_root="library/table_lookup")
    ctx.run_tests(verbose=3, regex="TestLookupMatchesStd")
