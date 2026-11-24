"""Unit helpers - course-infrastructure imports for the notebook."""

import sys
from functools import partial
from pathlib import Path

_course_root = str(Path(__file__).resolve().parent.parent)
if _course_root not in sys.path:
    sys.path.insert(0, _course_root)

from _check_env import check as check_env  # noqa: E402, F401
from _learning_output import (  # noqa: E402
    find_author_notebook,
    quiz_for_notebook,
    validate_quizzes as _validate_quizzes,
)

_notebook = find_author_notebook(Path(__file__).resolve().parent)
quiz = quiz_for_notebook(_notebook)
validate_quizzes = partial(_validate_quizzes, _notebook)
