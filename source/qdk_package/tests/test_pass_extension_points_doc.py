# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Checks that the documented pass surface is the published pass surface.

A named extension point that exists only in prose drifts the moment the code
moves. Reading the names out of the document and comparing them against what
the binding publishes makes a rename break visibly, and the document's own
examples run here, so a claim it makes that stops being true fails.
"""

import pathlib
import re
import warnings

import pytest

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk.circuit import PassAnchor, PassStage  # noqa: E402

DOC = pathlib.Path(__file__).resolve().parents[1] / "PASS-EXTENSION-POINTS.md"


def _python_blocks() -> list[str]:
    text = DOC.read_text(encoding="utf-8")
    return re.findall(r"```python\n(.*?)```", text, flags=re.DOTALL)


def _documented_names() -> list[str]:
    text = DOC.read_text(encoding="utf-8")
    return re.findall(r"^### `([a-z_]+)`$", text, flags=re.MULTILINE)


def test_document_exists():
    assert DOC.is_file(), f"missing {DOC}"


def test_every_documented_example_runs():
    blocks = _python_blocks()
    assert blocks, "the document should pin its claims with runnable examples"

    namespace: dict = {}
    for index, block in enumerate(blocks):
        try:
            exec(block, namespace)  # noqa: S102
        except Exception as error:  # noqa: BLE001
            pytest.fail(f"example {index + 1} failed: {error}\n\n{block}")


def test_the_document_describes_every_published_anchor():
    assert _documented_names() == [anchor.name for anchor in PassAnchor.all()]


def test_the_set_is_small_enough_to_state_on_one_page():
    # Growth is the failure mode for an extension-point set, so it is asserted.
    anchors = PassAnchor.all()
    assert len(anchors) == 4
    stages = [anchor.stage for anchor in anchors]
    assert stages.count(PassStage.Circuit) == 2
    assert stages.count(PassStage.Graph) == 2
    assert [anchor.outbound for anchor in anchors] == [False, False, False, True]


def test_the_document_states_the_current_posture_and_both_tiers():
    # The failure this guards against is a stability statement that names the
    # eventual contract without saying that none of it holds yet, which would
    # read as a promise.
    text = DOC.read_text(encoding="utf-8")
    assert "## What is stable" in text
    assert "Nothing here is stable yet" in text
    assert "The stable tier" in text
    assert "The unstable tier" in text


if __name__ == "__main__":
    pytest.main([__file__])
