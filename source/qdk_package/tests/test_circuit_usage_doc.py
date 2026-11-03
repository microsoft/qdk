# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Checks that the usage documentation actually runs.

Documentation drifts silently. Extracting and executing the examples means a
change to the API breaks the docs visibly rather than leaving them wrong.
"""

import pathlib
import re
import warnings

import pytest

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

DOC = pathlib.Path(__file__).resolve().parents[1] / "CIRCUIT-USAGE.md"


def _python_blocks() -> list[str]:
    text = DOC.read_text(encoding="utf-8")
    return re.findall(r"```python\n(.*?)```", text, flags=re.DOTALL)


def test_usage_document_exists():
    assert DOC.is_file(), f"missing {DOC}"


def test_every_documented_example_runs():
    blocks = _python_blocks()
    assert blocks, "the usage document should contain runnable examples"

    # Examples build on one another, so they share a namespace in order.
    namespace: dict = {}
    for index, block in enumerate(blocks):
        try:
            exec(block, namespace)  # noqa: S102
        except Exception as error:  # noqa: BLE001
            pytest.fail(f"example {index + 1} failed: {error}\n\n{block}")


if __name__ == "__main__":
    pytest.main([__file__])
