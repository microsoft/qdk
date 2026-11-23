"""Unit helpers - course-infrastructure imports for the notebook."""

from pathlib import Path
import sys

_COURSE_ROOT = str(Path(__file__).resolve().parent.parent)
if _COURSE_ROOT not in sys.path:
    sys.path.insert(0, _COURSE_ROOT)

from _check_env import check as check_env  # noqa: E402, F401
