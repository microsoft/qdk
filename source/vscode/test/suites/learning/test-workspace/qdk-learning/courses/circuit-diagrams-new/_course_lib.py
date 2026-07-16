"""Shared course utilities — the exercise harness and unit completion.

This module lives at the course root. Per-unit helper files (`_unit.py`) import
from it and re-export the small surface the notebooks need.

The exercise model
------------------
Learners solve an exercise by implementing a function decorated with ``@exercise``.

A unit registers a checker for each exercise *by function name* (see
``register_value_exercise`` / ``register_circuit_exercise``). When the learner
runs their decorated cell, ``exercise`` looks up the matching checker, calls the
learner's function, validates the result and renders a pass/fail banner and other
relevant visuals or output.
"""

import re
from pathlib import Path
from typing import Callable

from IPython.display import HTML, display

# Registry of exercises that have passed in this kernel session.
_passed: set[str] = set()

# A checker takes the learner's function and verifies it.
Checker = Callable[[Callable[[], object]], None]

# Registry of checkers, keyed by exercise function name.
_checkers: dict[str, Checker] = {}

# Exercise names in registration order. A unit's required set is derived from
# this, so each exercise name is written exactly once (in its register call).
_registered: list[str] = []


def _register(name: str, checker: Checker) -> str:
    """Record a checker under ``name`` and return the name."""
    _checkers[name] = checker
    if name not in _registered:
        _registered.append(name)
    return name


# ---------------------------------------------------------------------------
# Rendering helpers
# ---------------------------------------------------------------------------


def _pass(message: str) -> None:
    """Render a green success banner."""
    display(
        HTML(
            '<div style="font-family:system-ui,sans-serif;margin:8px 0;'
            "padding:10px 14px;background:#e8f5e9;border-left:4px solid #2e7d32;"
            'border-radius:4px">'
            f"&#x2705; <strong>{message}</strong>"
            "</div>"
        )
    )


def _fail(message: str) -> None:
    """Render an orange failure banner and raise AssertionError."""
    display(
        HTML(
            '<div style="font-family:system-ui,sans-serif;margin:8px 0;'
            "padding:10px 14px;background:#fff3e0;border-left:4px solid #e65100;"
            'border-radius:4px">'
            f"&#x274C; <strong>{message}</strong>"
            "</div>"
        )
    )
    raise AssertionError(message)


# ---------------------------------------------------------------------------
# The exercise decorator
# ---------------------------------------------------------------------------


def exercise(fn):
    """Decorator for a learner's exercise function.

    Looks up the checker registered for ``fn.__name__`` and runs it. The
    learner just writes the function body and a ``return`` — running the cell
    runs the verification.
    """
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


def _run(fn):
    """Call the learner's function, surfacing errors as a failure banner."""
    try:
        return fn()
    except Exception as e:  # noqa: BLE001 — surface any learner error nicely
        _fail(
            f"Your <code>{fn.__name__}</code> function raised an error: "
            f"<code>{type(e).__name__}: {e}</code>"
        )


# ---------------------------------------------------------------------------
# Value exercises
# ---------------------------------------------------------------------------


def register_value_exercise(name: str, *, expected) -> str:
    """Register an exercise whose function must return ``expected``."""

    def checker(fn) -> None:
        actual = _run(fn)
        if actual != expected:
            _fail(
                f"<code>{name}()</code> returned <code>{actual!r}</code>, "
                f"but expected <code>{expected!r}</code>."
            )
        else:
            _passed.add(name)
            _pass(f"Correct! <code>{name}()</code> returned {actual!r}.")

    return _register(name, checker)


# ---------------------------------------------------------------------------
# Custom exercises
# ---------------------------------------------------------------------------


def register_exercise(
    name: str,
    validate: Callable[[object], str | None],
    *,
    success_message: str = "Correct!",
    on_success: Callable[[object], None] | None = None,
) -> str:
    """Register an exercise with a unit-defined validation function.

    Use this when a unit needs bespoke checking that isn't covered by the
    generic helpers above. ``validate(result)`` inspects the value returned by
    the learner's function and returns an HTML error message if it's wrong, or
    ``None`` if it's correct. On success a banner with ``success_message`` is
    shown, ``on_success(result)`` is called (e.g. to display a widget), and the
    exercise is recorded as passed.
    """

    def checker(fn) -> None:
        result = _run(fn)
        error = validate(result)
        if error:
            _fail(error)
            return
        _passed.add(name)
        _pass(success_message)
        if on_success is not None:
            on_success(result)

    return _register(name, checker)


# ---------------------------------------------------------------------------
# Unit completion
# ---------------------------------------------------------------------------


def complete_unit(required_exercises: list[str] | None = None) -> None:
    """Verify all exercises passed and write the unit-complete marker.

    When ``required_exercises`` is omitted, every exercise registered in this
    kernel session is required — i.e. all of the current unit's exercises.
    """
    if required_exercises is None:
        required_exercises = _registered
    missing = [e for e in required_exercises if e not in _passed]
    if missing:
        names = ", ".join(f"`{e}`" for e in missing)
        raise AssertionError(
            f"Not all exercises are complete. Missing: {names}. "
            "Run the exercise cells above first."
        )

    unit_id = re.sub(r"^\d+-", "", Path.cwd().name)

    marker = Path(".qdk-unit-complete")
    marker.write_text(f"{unit_id}\n")

    display(
        HTML(
            '<div style="font-family:system-ui,sans-serif;margin:12px 0;'
            "padding:14px 18px;background:#e8f5e9;border-left:4px solid #2e7d32;"
            'border-radius:4px;font-size:1.1em">'
            "&#x1F389; <strong>Congratulations — you've completed this unit!</strong>"
            "</div>"
        )
    )
