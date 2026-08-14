"""Unit helpers - course-infrastructure imports for the notebook."""

import sys
from pathlib import Path

_course_root = str(Path(__file__).resolve().parent.parent)
if _course_root not in sys.path:
    sys.path.insert(0, _course_root)

from _check_env import check as check_env  # noqa: E402, F401
from _course_lib import (  # noqa: E402, F401
    exercise,
    register_exercise,
    register_value_exercise,
)

# The selected grid point 010000 is k=16 on the 64-point six-bit grid.
MEASURED_PHASE = register_value_exercise("measured_phase", expected=0.25)

# Six iterations over a 12-qubit compute register plus one readout ancilla.
_EXPECTED_CIRCUIT = {
    "iteration_circuits": 6,
    "compute_qubits": 12,
    "readout_ancillas": 1,
}


def _check_circuit(result):
    if not isinstance(result, dict):
        return "Return a dictionary with the three keys named in the exercise."
    missing = sorted(set(_EXPECTED_CIRCUIT) - set(result))
    if missing:
        keys = ", ".join(f"<code>{k}</code>" for k in missing)
        return f"The returned dictionary is missing {keys}."
    wrong = sorted(k for k, v in _EXPECTED_CIRCUIT.items() if result[k] != v)
    if wrong:
        keys = ", ".join(f"<code>{k}</code>" for k in wrong)
        return f"Not right yet: {keys}. Read each one off the circuit."
    return None


VALIDATE_CIRCUIT = register_exercise(
    "validate_circuit",
    _check_circuit,
    success_message=(
        "Correct. Twelve compute qubits plus one readout ancilla, "
        "rebuilt for each of the six iterations."
    ),
)
