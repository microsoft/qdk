# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from .._native import Output  # type: ignore

_in_jupyter = False
try:
    from IPython.display import display

    if get_ipython().__class__.__name__ == "ZMQInteractiveShell":  # type: ignore
        _in_jupyter = True  # Jupyter notebook or qtconsole
except:
    pass


def display_or_print(output: Output) -> None:
    if _in_jupyter:
        try:
            display(output)
            return
        except:
            # If IPython is not available, fall back to printing the output
            pass
    print(output, flush=True)
