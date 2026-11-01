"""Per-unit helper module for QDK-EC Learning Course."""

from pathlib import Path
import sys

_COURSE_ROOT = Path(__file__).resolve().parent.parent
if str(_COURSE_ROOT) not in sys.path:
    sys.path.insert(0, str(_COURSE_ROOT))

from _check_env import check as check_env
from _course_lib import exercise, register_exercise, register_value_exercise
import _walkthrough_simulation as simulation
from _walkthrough_simulation import ensure_course_state, plot_sweep, sample_sweep

register_value_exercise('is_postselected_below_physical', expected=True)

__all__ = [
    'check_env',
    'exercise',
    'simulation',
    'ensure_course_state',
    'plot_sweep',
    'sample_sweep',
]
