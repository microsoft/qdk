"""Tests for the single-gadget audit convenience API."""
from qdk.ec.lint import why_not_valid
import qodec


def test_why_not_valid_passes_valid_gadget(idle_gadget: qodec.Gadget) -> None:
    assert why_not_valid(idle_gadget) == ""
