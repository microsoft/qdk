"""Unit helpers - course-infrastructure imports for the notebook."""

import sys
from pathlib import Path

_course_root = str(Path(__file__).resolve().parent.parent)
if _course_root not in sys.path:
    sys.path.insert(0, _course_root)

from _check_env import check as check_env  # noqa: E402, F401
from _course_lib import (  # noqa: E402, F401
    exercise,
    register_value_exercise,
)

# The selected grid point 010000 is k=16 on the 64-point six-bit grid.
MEASURED_PHASE = register_value_exercise("measured_phase", expected=0.25)
