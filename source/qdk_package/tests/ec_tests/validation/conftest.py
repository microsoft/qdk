"""Fixtures for validation tests.

The audit tests exercise against a vendored, current-model ``repetition3``
qodec kept under ``tests/validation/audit/fixtures/``.
"""
from pathlib import Path

import pytest
import qodec as qc

_AUDIT_FIXTURES = Path(__file__).parent / "audit" / "fixtures"


@pytest.fixture(scope="package")
def rep3_path() -> str:
    """Filesystem path to the vendored, current-model ``repetition3`` qodec."""
    return str(_AUDIT_FIXTURES / "repetition3.qodec.yaml")


@pytest.fixture
def rep3_qodec(rep3_path: str) -> qc.Qodec:
    """A freshly loaded ``repetition3`` qodec.

    Function-scoped so individual tests may mutate the returned object (e.g.
    swap a gadget) without affecting others.
    """
    return qc.Qodec.load(rep3_path)
