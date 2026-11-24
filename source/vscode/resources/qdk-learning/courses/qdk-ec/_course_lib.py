"""Shared course utilities for the exercise harness.

This module lives at the course root. Per-unit helper files (`_unit.py`) import
from it and re-export the small surface the notebooks need.
"""

from typing import Callable

from IPython.core.getipython import get_ipython
from IPython.display import HTML, display

ExerciseFunction = Callable[[], object]
Checker = Callable[[ExerciseFunction], None]
_checkers: dict[str, Checker] = {}


class ExerciseError(AssertionError):
    """Raised when an exercise is not yet correct."""


def _hide_traceback() -> None:
    """Show only the failure banner for a wrong answer."""
    shell = get_ipython()
    if shell is None:
        return
    shell.set_custom_exc((ExerciseError,), lambda *args, **kwargs: None)


_hide_traceback()


def _register(name: str, checker: Checker) -> str:
    """Record a checker under ``name`` and return the name."""
    _checkers[name] = checker
    return name


def _pass(message: str) -> None:
    """Render a green success banner."""
    display(
        HTML(
            '<div style="font-family:system-ui,sans-serif;margin:8px 0;'
            "padding:10px 14px;"
            "background:var(--vscode-notifications-background, #e8f5e9);"
            "color:var(--vscode-notifications-foreground, #1b5e20);"
            "border-left:4px solid var(--vscode-testing-iconPassed, #2e7d32);"
            'border-radius:4px">'
            f"&#x2705; <strong>{message}</strong>"
            "</div>"
        )
    )


def _banner(message: str) -> None:
    """Render an orange failure banner."""
    display(
        HTML(
            '<div style="font-family:system-ui,sans-serif;margin:8px 0;'
            "padding:10px 14px;"
            "background:var(--vscode-inputValidation-warningBackground, #fff3e0);"
            "color:var(--vscode-inputValidation-warningForeground, #e65100);"
            "border-left:4px solid var(--vscode-inputValidation-warningBorder, #e65100);"
            'border-radius:4px">'
            f"&#x274C; <strong>{message}</strong>"
            "</div>"
        )
    )


def _fail(message: str) -> None:
    """Render an orange failure banner and raise ExerciseError."""
    _banner(message)
    raise ExerciseError(message)


def exercise(fn: ExerciseFunction) -> ExerciseFunction:
    """Decorator for a learner's exercise function."""
    checker = _checkers.get(fn.__name__)
    if checker is None:
        _fail(
            f"No checker is registered for an exercise named "
            f"<code>{fn.__name__}</code>. Don't rename the function — "
            "it must keep the name we gave you."
        )
        return fn
    checker(fn)
    return fn


def _run(fn: ExerciseFunction) -> object:
    """Call the learner's function, keeping the traceback for real errors."""
    try:
        return fn()
    except ExerciseError:
        raise
    except Exception:  # noqa: BLE001
        _banner(
            f"Your <code>{fn.__name__}</code> function raised an error. "
            "The traceback below shows where."
        )
        raise


def register_value_exercise(name: str, *, expected: object) -> str:
    """Register an exercise whose function must return ``expected``."""

    def checker(fn: ExerciseFunction) -> None:
        actual = _run(fn)
        if actual != expected:
            _fail(
                f"<code>{name}()</code> returned <code>{actual!r}</code>, "
                "which is not correct. Use <b>Ask for a Hint</b> below the cell, "
                "then run it again."
            )
        else:
            _pass(f"Correct! <code>{name}()</code> returned {actual!r}.")

    return _register(name, checker)


def register_exercise(
    name: str,
    validate: Callable[[object], str | None],
    *,
    success_message: str = "Correct!",
    on_success: Callable[[object], None] | None = None,
) -> str:
    """Register an exercise with a unit-defined validation function."""

    def checker(fn: ExerciseFunction) -> None:
        result = _run(fn)
        error = validate(result)
        if error:
            _fail(error)
            return
        _pass(success_message)
        if on_success is not None:
            on_success(result)

    return _register(name, checker)
