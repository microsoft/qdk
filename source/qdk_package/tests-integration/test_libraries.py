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
