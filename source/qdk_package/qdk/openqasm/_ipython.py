# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from .._native import Output  # type: ignore

_jupyter_display = None
try:
    from IPython.display import display  # type: ignore[import-not-found]

    if get_ipython().__class__.__name__ == "ZMQInteractiveShell":  # type: ignore
        _jupyter_display = display  # Jupyter notebook or qtconsole
except:
    pass


def display_or_print(output: Output) -> None:
    if _jupyter_display is not None:
        try:
            _jupyter_display(output)
            return
        except:
            # If IPython is not available, fall back to printing the output
            pass
    print(output, flush=True)
