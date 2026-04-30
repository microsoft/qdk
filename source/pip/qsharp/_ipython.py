# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""
_ipython.py

This module provides IPython magic functions for integrating Q# code
execution within Jupyter notebooks.
"""

from time import monotonic
from IPython.display import display, clear_output
from IPython.core.magic import register_cell_magic
from ._native import QSharpError
from ._qsharp import _get_session
from . import telemetry_events


def register_magic():
    @register_cell_magic
    def qsharp(line, cell):
        """Cell magic to interpret Q# code in Jupyter notebooks."""
        # This effectively pings the kernel to ensure it recognizes the cell is running and helps with
        # accureate cell execution timing.
        clear_output()

        def callback(output):
            display(output)
            # This is a workaround to ensure that the output is flushed. This avoids an issue
            # where the output is not displayed until the next output is generated or the cell
            # is finished executing.
            display(display_id=True)

        telemetry_events.on_run_cell()
        start_time = monotonic()

        try:
            session = _get_session()
            results = session._qsharp_value_to_python_value(
                session._interpreter.interpret(cell, callback)
            )

            durationMs = (monotonic() - start_time) * 1000
            telemetry_events.on_run_cell_end(durationMs)

            return results
        except QSharpError as e:
            # pylint: disable=raise-missing-from
            raise QSharpCellError(str(e))


class QSharpCellError(BaseException):
    """
    Error raised when a %%qsharp cell fails.
    """

    def __init__(self, traceback: str):
        self.traceback = traceback.splitlines()

    def _render_traceback_(self):
        # We want to specifically override the traceback so that
        # the Q# error directly from the interpreter is shown
        # instead of the Python error.
        return self.traceback
