"""Unit helpers — course-infrastructure imports for the notebook."""

import json
import sys
from pathlib import Path

from IPython.display import display

_course_root = str(Path(__file__).resolve().parent.parent)
if _course_root not in sys.path:
    sys.path.insert(0, _course_root)

from _check_env import check as check_env  # noqa: E402, F401
from _course_lib import (  # noqa: E402, F401
    exercise,
    register_exercise,
    complete_unit,
)


def _missing_gates(circuit, required_gates: list[str]) -> list[str]:
    diagram = str(circuit)
    return [g for g in required_gates if g not in diagram]


def _is_flat(circuit) -> bool:
    """True if the circuit has no grouped (nested) operations."""
    data = json.loads(circuit.json())
    operations = data.get("operations", [])
    return not any("children" in op for op in operations)


def _display_circuit(circuit) -> None:
    from qdk.widgets import Circuit

    display(Circuit(circuit))


def register_circuit_exercise(
    name: str, *, required_gates: list[str], flat: bool = False
) -> str:
    """Register an exercise whose function must return a ``Circuit``.

    Verifies the circuit contains ``required_gates`` (and, when ``flat`` is
    True, that no operations are grouped), then displays the widget as
    confirmation. The learner returns the circuit; rendering is our job.
    """

    def validate(circuit) -> str | None:
        if circuit is None:
            return (
                f"<code>{name}()</code> returned <code>None</code>. "
                "Did you forget to <code>return</code> the circuit?"
            )
        missing = _missing_gates(circuit, required_gates)
        if missing:
            gate_list = ", ".join(f"<code>{g}</code>" for g in missing)
            return (
                f"Your circuit is missing: {gate_list}. "
                "Check your code and re-run the cell."
            )
        if flat and not _is_flat(circuit):
            return (
                "Your circuit still has grouped operations. "
                "Did you set <code>group_by_scope=False</code>?"
            )
        return None

    return register_exercise(
        name,
        validate,
        success_message=f"Correct! Here's your <code>{name}</code> circuit:",
        on_success=_display_circuit,
    )


# Register the this unit's exercises.
register_circuit_exercise("cat_circuit", required_gates=["H", "X"])
register_circuit_exercise("flat_circuit", required_gates=["H", "X"], flat=True)
