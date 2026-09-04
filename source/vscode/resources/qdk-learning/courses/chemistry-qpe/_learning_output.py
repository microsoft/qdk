"""Interactive outputs for QDK learning notebooks.

This module lives at the course root. Per-unit helper files (``_unit.py``)
import from it and re-export the small authoring surface the notebooks need.

The output model
----------------
Each helper returns a lightweight display object carrying three views of the
same content: a custom QDK MIME payload, a ``text/html`` fallback, and a
``text/plain`` fallback. IPython writes all three into the ``.ipynb``; VS Code
picks the custom one and renders it interactively, while other notebook hosts
fall back to clean, non-interactive HTML or plain text.

The payload shape is mirrored by ``src/notebookRenderer/schema.ts``. The two
are compared at build time by ``checkRendererContract()`` in ``build.mjs``, so
renaming a field on one side without the other fails the build rather than
producing an empty cell in front of a learner.
"""

from __future__ import annotations

import random
from dataclasses import dataclass
from html import escape
from typing import Any, Iterable, Mapping, Sequence

MIME_TYPE = "application/vnd.qdk.learning+json"

_CARD_STYLE = (
    "font-family:var(--qdk-font-family, system-ui, sans-serif);"
    "color:var(--qdk-host-foreground, #222);"
    "background:var(--qdk-host-background, #fff);"
    "border:1px solid var(--qdk-widget-outline, #ccc);"
    "border-radius:6px;"
    "margin:10px 0;"
    "line-height:1.45;"
    "overflow:hidden;"
)

#: The burnt-orange band the chemistry tutorial already uses to mark a
#: self-check question. The fallback matters: this HTML is what a host without
#: the QDK renderer shows, and it has no ``--qdk-*`` palette to resolve.
_QUIZ_HEADER_STYLE = (
    "background:var(--qdk-quiz-accent, #8c4a00);"
    "color:var(--qdk-quiz-accent-foreground, #ffffff);"
    "padding:0.35em 0.8em;"
    "font-weight:600;"
)
_MUTED_STYLE = "color:var(--qdk-description-foreground, #666);font-size:0.9em"


@dataclass(frozen=True)
class LearningOutput:
    """Display object for one QDK learning output.

    Parameters
    ----------
    payload : mapping
        JSON-serializable payload for ``application/vnd.qdk.learning+json``.
    html : str
        Non-interactive, escaped HTML fallback for notebook hosts that do not
        know about the QDK learning renderer.
    text : str
        Plain text fallback for terminals and text-only exports.
    """

    payload: Mapping[str, Any]
    html: str
    text: str

    def _repr_mimebundle_(
        self,
        include: Iterable[str] | None = None,
        exclude: Iterable[str] | None = None,
    ) -> dict[str, Any]:
        """Return the custom MIME payload plus HTML and plain text fallbacks."""
        bundle: dict[str, Any] = {
            MIME_TYPE: dict(self.payload),
            "text/html": self.html,
            "text/plain": self.text,
        }
        if include is not None:
            allowed = set(include)
            bundle = {key: value for key, value in bundle.items() if key in allowed}
        if exclude is not None:
            blocked = set(exclude)
            bundle = {key: value for key, value in bundle.items() if key not in blocked}
        return bundle

    def __str__(self) -> str:
        """Return the plain text fallback."""
        return self.text


def multiple_choice(
    prompt: str,
    options: Sequence[Sequence[Any]],
    *,
    multi_select: bool = False,
    cell_id: str | None = None,
) -> LearningOutput:
    """Create a multiple-choice learning output.

    ``options`` are ``(id, text, correct, explanation)`` tuples. Option ids must
    be unique, and a question needs at least two options, so authoring mistakes
    fail loudly when the notebook cell is run.

    A single-select question needs exactly one correct option. Pass
    ``multi_select=True`` for a question with several right answers; the learner
    then has to find all of them, and is told so.
    """
    normalized = _normalize_options(options, multi_select=multi_select)
    payload: dict[str, Any] = {
        "schemaVersion": 1,
        "kind": "multiple-choice",
        "prompt": str(prompt),
        "options": normalized,
    }
    if multi_select:
        payload["multiSelect"] = True
    if cell_id is not None:
        payload["cellId"] = str(cell_id)

    html = _mcq_html(str(prompt), normalized, multi_select=multi_select)
    text = _mcq_text(str(prompt), normalized, multi_select=multi_select)
    return LearningOutput(payload, html, text)


# ---------------------------------------------------------------------------
# Registered quizzes
# ---------------------------------------------------------------------------

# Quiz definitions live here, keyed by id, and units register into it from
# `_unit.py`. The notebook cell only names the quiz.
_quizzes: dict[str, LearningOutput] = {}


def register_quiz(
    quiz_id: str,
    prompt: str,
    options: Sequence[Sequence[Any]],
    *,
    multi_select: bool = False,
    shuffle: bool = True,
) -> str:
    """Register a quiz under ``quiz_id`` so a notebook can show it by name.

    Answers are kept out of the *cell source*. A quiz written inline would put
    the ``correct`` flags and the per-option explanations right there in the
    code the learner reads, where reading them is easier than answering. So
    quizzes are declared in the unit's ``_unit.py`` — the same place exercise
    checkers already live — and the notebook cell just says ``quiz("...")``.

    The answers do still reach the browser, in the baked cell output, because
    the renderer grades without a kernel. This raises the effort of cheating; it
    does not make it impossible, and it is no weaker than the collapsible
    answers it replaced.

    Pass ``multi_select=True`` for a question with several right answers; the
    learner gets checkboxes and is told to select all that apply.

    Options are shuffled by default. It is natural to write the correct answer
    first and the distractors after it, which makes "always pick A" a winning
    strategy across a unit. The shuffle is seeded from ``quiz_id``, so the
    order is stable: re-running a notebook, or baking its outputs again, does
    not reorder the options or produce a spurious diff. Pass ``shuffle=False``
    for a question whose options have a meaningful order of their own.
    """
    if quiz_id in _quizzes:
        raise ValueError(f"a quiz is already registered as {quiz_id!r}")
    ordered = _shuffled(quiz_id, options) if shuffle else options
    _quizzes[quiz_id] = multiple_choice(
        prompt, ordered, multi_select=multi_select, cell_id=quiz_id
    )
    return quiz_id


def _shuffled(seed: str, options: Sequence[Sequence[Any]]) -> list[Sequence[Any]]:
    """Permute options deterministically from a string seed.

    ``random.Random`` derives its state from a hash of the string rather than
    from ``PYTHONHASHSEED``, so the same id yields the same order on every
    machine and every run.
    """
    shuffled = list(options)
    random.Random(seed).shuffle(shuffled)
    return shuffled


def quiz(*quiz_ids: str) -> None:
    """Show the quizzes registered under ``quiz_ids``.

    Accepts more than one id because the progress tree names a code cell after
    the heading above it, so two adjacent quiz cells in one section would
    appear twice under the same name. Where the chapter asks two questions
    back to back, one cell shows both.

    Displays rather than returns, so the call does not have to be the last
    expression in the cell and one cell can produce several outputs.
    """
    if not quiz_ids:
        raise ValueError('quiz() needs at least one quiz id, e.g. quiz("my-quiz")')

    try:
        from IPython.display import display
    except ImportError as exc:  # pragma: no cover - notebooks always have IPython
        raise RuntimeError(
            "quiz() displays its output and so needs IPython; "
            "use multiple_choice() directly outside a notebook."
        ) from exc

    for quiz_id in quiz_ids:
        display(_lookup_quiz(quiz_id))


def _lookup_quiz(quiz_id: str) -> LearningOutput:
    try:
        return _quizzes[quiz_id]
    except KeyError:
        known = ", ".join(sorted(_quizzes)) or "none"
        raise ValueError(
            f"no quiz is registered as {quiz_id!r}; registered quizzes: {known}. "
            "Quizzes are registered in the unit's _unit.py."
        ) from None


# ---------------------------------------------------------------------------
# Internals
# ---------------------------------------------------------------------------


def _normalize_options(
    options: Sequence[Sequence[Any]],
    *,
    multi_select: bool = False,
) -> list[dict[str, Any]]:
    """Validate and normalize the option tuples an author wrote.

    One accepted shape, ``(id, text, correct, explanation)``. Earlier drafts
    also took dicts and shorter tuples; nothing used them, and each extra shape
    was another way for two quizzes to end up subtly inconsistent.
    """
    if len(options) < 2:
        raise ValueError("multiple_choice requires at least two options")

    normalized: list[dict[str, Any]] = []
    seen_ids: set[str] = set()
    correct_count = 0
    for option in options:
        parts = list(option)
        if len(parts) != 4:
            raise ValueError(
                "multiple_choice options must be "
                "(id, text, correct, explanation) tuples; "
                f"got {len(parts)} value(s)"
            )
        option_id, text, correct, explanation = parts
        option_id = str(option_id)
        text = str(text)

        if not isinstance(correct, bool):
            raise ValueError("multiple_choice option 'correct' values must be bool")
        if not option_id:
            raise ValueError("multiple_choice option ids must not be empty")
        if option_id in seen_ids:
            raise ValueError(f"duplicate multiple_choice option id: {option_id!r}")
        seen_ids.add(option_id)

        item: dict[str, Any] = {"id": option_id, "text": text, "correct": correct}
        if explanation is not None:
            item["explanation"] = str(explanation)
        if correct:
            correct_count += 1
        normalized.append(item)

    if correct_count == 0:
        raise ValueError("multiple_choice requires at least one correct option")

    if multi_select:
        # A "select all that apply" with one answer teaches the learner to
        # distrust the instruction, and one where everything applies isn't a
        # question. Both are authoring mistakes, not learner mistakes.
        if correct_count < 2:
            raise ValueError(
                "multi_select questions need at least two correct options; "
                "drop multi_select=True for a single-answer question"
            )
        if correct_count == len(normalized):
            raise ValueError(
                "every option is marked correct, so the question cannot be "
                "answered wrongly"
            )
    elif correct_count > 1:
        # The renderer grades by comparing the selected set with the correct
        # set, and a radio group holds one selection — so a single-select
        # question with two correct answers can never be answered right.
        raise ValueError(
            f"multiple_choice got {correct_count} correct options; pass "
            "multi_select=True for a question with several right answers"
        )
    return normalized


def _mcq_html(
    prompt: str, options: Sequence[Mapping[str, Any]], *, multi_select: bool
) -> str:
    """Render the static fallback.

    Deliberately inert: no answers, no ``correct`` flags, no explanations. This
    is what a host without the QDK renderer shows, and it is also what lands in
    an exported HTML page, so anything revealed here is revealed to everyone.
    """
    # A box for "pick several", a circle for "pick one" — the same distinction
    # the interactive version draws with checkboxes and radios.
    marker = "&#9744;" if multi_select else "&#9711;"
    hint = (
        f"<p style='{_MUTED_STYLE};margin:0 0 8px'>Select all that apply.</p>"
        if multi_select
        else ""
    )
    rows = [
        "<li style='margin:6px 0'>"
        f"<span aria-hidden='true'>{marker}</span> {escape(str(option['text']))}"
        "</li>"
        for option in options
    ]
    return (
        f"<div style='{_CARD_STYLE}'>"
        f"<div style='{_QUIZ_HEADER_STYLE}'>&#10067;&nbsp;Check your understanding</div>"
        "<div style='padding:12px 14px'>"
        f"<p style='margin:0 0 8px'>{escape(prompt)}</p>"
        f"{hint}"
        "<ol style='margin:0 0 0 20px;padding:0'>" + "".join(rows) + "</ol>"
        f"<p style='{_MUTED_STYLE};margin:10px 0 0'>Open this lesson in VS Code "
        "for interactive checking and explanations.</p>"
        "</div>"
        "</div>"
    )


def _mcq_text(
    prompt: str, options: Sequence[Mapping[str, Any]], *, multi_select: bool
) -> str:
    lines = [f"Check your understanding: {prompt}"]
    if multi_select:
        lines.append("  (select all that apply)")
    marker = "[ ]" if multi_select else "( )"
    lines.extend(f"  {marker} {option['text']}" for option in options)
    return "\n".join(lines)
