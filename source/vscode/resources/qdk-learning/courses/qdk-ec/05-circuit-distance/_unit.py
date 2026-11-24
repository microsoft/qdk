"""Unit helpers - course-infrastructure imports for the notebook."""

from pathlib import Path
import sys

_COURSE_ROOT = str(Path(__file__).resolve().parent.parent)
if _COURSE_ROOT not in sys.path:
    sys.path.insert(0, _COURSE_ROOT)

from _check_env import check as check_env  # noqa: E402, F401
from _course_lib import (  # noqa: E402, F401
    exercise,
    register_exercise,
    register_value_exercise,
)

register_value_exercise('baseline_syndrome_distance', expected=1)

_SIM_EXPORTS = {
    "compile_c4_benchmarks",
    "make_baseline_protocol",
    "make_c4",
    "make_decoder",
    "make_five_qubit_code",
    "make_openqasm_c4_protocol",
    "make_unfilled_hswap_gadget",
    "make_upgraded_protocol",
    "make_upgraded_syndrome_protocol",
    "plot_sweep",
    "sample_sweep",
    "show_stim_diagram",
    "validate_logical_swap",
    "validate_measure_x_gadget",
}

def __getattr__(name: str):
    if name in _SIM_EXPORTS:
        import _walkthrough_simulation as _sim
        return getattr(_sim, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
