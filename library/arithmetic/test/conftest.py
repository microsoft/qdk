from pathlib import Path

import pytest
from qdk import Context


@pytest.fixture(scope="session")
def context(request: pytest.FixtureRequest) -> Context:
    """Shared qdk.Context object to be reused accross tests."""
    minimize_qubits = getattr(request, "param", "min_qubits") == "min_qubits"
    path = str(Path(__file__).resolve().parents[1])
    return Context(
        project_root=path, qsharp_config={"minimize_qubits": minimize_qubits}
    )
