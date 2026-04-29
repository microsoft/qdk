import pytest
from qdk import qsharp

@pytest.fixture(autouse=True)
def setup():
    """Fixture to execute before a test is run"""
    # Setting the project root to current folder.
    qsharp.init(project_root=".")
    yield # this is where the testing happens

# 1/2 (|00⟩ + i|01⟩ - |10⟩ - i|11⟩)
# The basis states are converted to indices of amplitudes in the array using big endian notation.
expected_state = [0.5, 0.5j, -0.5, -0.5j]

def test_state_exact() -> None:
    """Test that Q# code prepares the expected state exactly using Python test code."""
    # Run Q# code that allocates the qubits and prepares the state but doesn't deallocate the qubits.
    qsharp.eval(f"use qs = Qubit[2]; StatePrep.PrepareStateWithComplexPhases(qs);")
    # Get the state of the allocated qubits and convert it to a dense vector.
    state = qsharp.dump_machine().as_dense_state()
    # Compare two vectors.
    assert state == pytest.approx(expected_state)


def test_state_exact_rejects_global_phase() -> None:
    """Test that shows that the exact check from the previous test fails if the state is different by a global phase."""
    # Run Q# code that allocates the qubits and prepares the state but doesn't deallocate the qubits.
    qsharp.eval(f"use qs = Qubit[2]; StatePrep.PrepareStateWithGlobalPhase(qs);")
    # Get the state of the allocated qubits and convert it to a dense vector.
    state = qsharp.dump_machine().as_dense_state()
    # Compare two vectors. Here we expect them to be _not equal_ due to the global phase -1.
    assert state != pytest.approx(expected_state)


def test_state_global_phase() -> None:
    """Test that Q# code prepares the expected state up to a global phase using Python test code."""
    # Run Q# code that allocates the qubits and prepares the state but doesn't deallocate the qubits.
    qsharp.eval(f"use qs = Qubit[2]; StatePrep.PrepareStateWithGlobalPhase(qs);")
    # Get the state of the allocated qubits.
    state = qsharp.dump_machine()
    # Compare the state to the expected one, taking into account the possible global phase difference.
    assert state.check_eq(expected_state)
