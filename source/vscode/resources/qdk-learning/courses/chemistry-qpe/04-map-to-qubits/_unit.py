"""Unit helpers - course-infrastructure imports for the notebook."""

import sys
from functools import partial
from pathlib import Path

_course_root = str(Path(__file__).resolve().parent.parent)
if _course_root not in sys.path:
    sys.path.insert(0, _course_root)

from _check_env import check as check_env  # noqa: E402, F401
from _course_lib import (  # noqa: E402, F401
    exercise,
    register_value_exercise,
)
from _learning_output import (  # noqa: E402
    quiz_for_notebook,
    validate_quizzes as _validate_quizzes,
)

_notebook = Path(__file__).resolve().with_name("map_to_qubits.ipynb")
quiz = quiz_for_notebook(_notebook)
validate_quizzes = partial(_validate_quizzes, _notebook)

# Six active spatial orbitals give twelve spin orbitals, one compute qubit each.
QUBIT_COUNT = register_value_exercise("compute_qubit_count", expected=12)
