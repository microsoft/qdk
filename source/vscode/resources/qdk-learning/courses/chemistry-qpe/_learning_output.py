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

import json
import random
import re
from dataclasses import dataclass
from html import escape
from pathlib import Path
from typing import Any, Iterable, Mapping, Sequence

MIME_TYPE = "application/vnd.qdk.learning+json"

#: A quiz or option id has to survive the trip to the extension host, which
#: accepts only this shape from a notebook — nothing longer, and nothing that
#: could read as prose. Enforcing it here means an author finds out when they
#: run the cell, rather than a learner finding the "Why is that wrong?" button
#: quietly doing less than it should. Keep in step with `ID_PATTERN` in
#: `schema.ts`.
_ID_RE = re.compile(r"\A[a-z0-9][a-z0-9-]{0,63}\Z")

#: The host drops an action naming more options than this, so a longer question
#: would render fine and then leave "Why is that wrong?" quietly doing less than
#: it should for every learner who selected them all. Refusing here makes that
#: an authoring error instead. Keep in step with `MAX_OPTION_IDS` in
#: `schema.ts`.
_MAX_OPTIONS = 8

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
    payload_id: str | None = None,
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
    prompt = str(prompt)
    # The renderer refuses a payload with no question text, so an empty prompt
    # would bake cleanly and then fail for the learner. Catch it here, where
    # the author is the one running the cell.
    if not prompt.strip():
        raise ValueError("multiple_choice requires question text")
    payload: dict[str, Any] = {
        "schemaVersion": 1,
        "kind": "multiple-choice",
        "prompt": prompt,
        "options": normalized,
    }
    if multi_select:
        payload["multiSelect"] = True
    if payload_id is not None:
        payload_id = str(payload_id)
        # This is what the renderer sends as the quiz id when a learner asks
        # why an answer was wrong, and the extension drops a message whose id
        # is not this shape. An id that fails here would leave the button
        # doing nothing at all, with nothing said to anyone.
        if not _ID_RE.match(payload_id):
            raise ValueError(
                f"payload_id {payload_id!r} must be lowercase letters, digits and "
                "hyphens, start with a letter or digit, and be at most 64 "
                "characters; the renderer's Copilot action drops anything else"
            )
        payload["payloadId"] = payload_id

    html = _mcq_html(prompt, normalized, multi_select=multi_select)
    text = _mcq_text(prompt, normalized, multi_select=multi_select)
    return LearningOutput(payload, html, text)


# ---------------------------------------------------------------------------
# Registered quizzes
# ---------------------------------------------------------------------------

# ID-only calls resolve either a legacy output or an authored notebook path.
# Authored calls construct their output directly without changing this registry.
_quizzes: dict[str, LearningOutput | Path] = {}


def register_quiz_notebook(quiz_id: str, notebook: Path) -> None:
    """Bind an ID to a notebook beside the importing unit's helper.

    Only the location is registered; each lookup reads the saved bundle again.
    """
    if quiz_id in _quizzes:
        raise ValueError(f"a quiz is already registered as {quiz_id!r}")
    if not _ID_RE.match(quiz_id):
        raise ValueError(f"invalid quiz id: {quiz_id!r}")
    if not notebook.is_absolute():
        raise ValueError("quiz notebook path must be absolute")
    _quizzes[quiz_id] = notebook


def register_quiz(
    quiz_id: str,
    prompt: str,
    options: Sequence[Sequence[Any]],
    *,
    multi_select: bool = False,
    shuffle: bool = True,
) -> str:
    """Register a quiz under ``quiz_id`` so a notebook can show it by name.

    This supports existing ID-only runtime calls. New quiz content belongs in
    the authored notebook; do not maintain a second registration of its
    question, answers, or configuration.

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
    if not _ID_RE.match(quiz_id):
        raise ValueError(
            f"quiz id {quiz_id!r} must be lowercase letters, digits and hyphens, "
            "start with a letter or digit, and be at most 64 characters; the "
            "renderer's Copilot action drops anything else"
        )
    ordered = _shuffled(quiz_id, options) if shuffle else options
    _quizzes[quiz_id] = multiple_choice(
        prompt, ordered, multi_select=multi_select, payload_id=quiz_id
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


def quiz(
    *quiz_ids: str,
    prompt: str | None = None,
    options: Sequence[Sequence[Any]] | None = None,
    multi_select: bool = False,
    shuffle: bool = True,
) -> None:
    """Display an authored quiz or the quizzes registered under ``quiz_ids``.

    Supply one id, ``prompt`` and ``options`` to construct an authored quiz
    directly. Its configuration applies to this display only; the registry is
    neither read nor changed. With IDs only, display the registered outputs
    or load their bound authored notebooks.

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

    if prompt is not None or options is not None or multi_select or not shuffle:
        if len(quiz_ids) != 1 or prompt is None or options is None:
            raise ValueError("an authored quiz needs one quiz id, prompt and options")
        quiz_id = quiz_ids[0]
        ordered = _shuffled(quiz_id, options) if shuffle else options
        display(
            multiple_choice(
                prompt, ordered, multi_select=multi_select, payload_id=quiz_id
            )
        )
        return

    for quiz_id in quiz_ids:
        display(_lookup_quiz(quiz_id))


def _lookup_quiz(quiz_id: str) -> LearningOutput:
    try:
        registered = _quizzes[quiz_id]
    except KeyError:
        known = ", ".join(sorted(_quizzes)) or "none"
        raise ValueError(
            f"no quiz is registered as {quiz_id!r}; registered quizzes: {known}. "
            "Quizzes are registered in the unit's _unit.py."
        ) from None
    if isinstance(registered, Path):
        return _notebook_quiz(registered, quiz_id)
    return registered


def _notebook_quiz(notebook: Path, quiz_id: str) -> LearningOutput:
    label = f"quiz {quiz_id!r} in {notebook}"
    try:
        document = json.loads(notebook.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read {label}: {error}") from error
    if not isinstance(document, dict) or not isinstance(document.get("cells"), list):
        raise ValueError(f"malformed notebook for {label}: expected cells")

    matches = []
    for cell in document["cells"]:
        if not isinstance(cell, dict):
            raise ValueError(f"malformed cell in {label}")
        if cell.get("cell_type") != "code":
            continue
        if not isinstance(cell.get("outputs"), list):
            raise ValueError(f"malformed outputs in {label}")
        for output in cell["outputs"]:
            if not isinstance(output, dict):
                raise ValueError(f"malformed output in {label}")
            data = output.get("data", {})
            if not isinstance(data, dict):
                raise ValueError(f"malformed output data in {label}")
            if MIME_TYPE not in data:
                continue
            payload = data[MIME_TYPE]
            if not isinstance(payload, dict):
                raise ValueError(f"malformed MCQ payload in {label}")
            if payload.get("payloadId") == quiz_id:
                matches.append(output)

    if len(matches) != 1:
        raise ValueError(
            f"expected exactly one saved MCQ output for {label}; found {len(matches)}"
        )
    output = matches[0]
    data = output["data"]
    payload = data[MIME_TYPE]
    if (
        output.get("output_type") != "display_data"
        or type(payload.get("schemaVersion")) is not int
        or payload["schemaVersion"] != 1
        or payload.get("kind") != "multiple-choice"
        or not isinstance(payload.get("prompt"), str)
        or not payload["prompt"].strip()
        or not isinstance(payload.get("options"), list)
        or not isinstance(payload.get("multiSelect", False), bool)
    ):
        raise ValueError(f"malformed saved MCQ output for {label}")
    options = []
    for option in payload["options"]:
        if (
            not isinstance(option, dict)
            or not isinstance(option.get("id"), str)
            or not isinstance(option.get("text"), str)
            or (
                "explanation" in option
                and not isinstance(option["explanation"], str)
            )
        ):
            raise ValueError(f"malformed saved MCQ option for {label}")
        options.append(
            (
                option["id"],
                option["text"],
                option.get("correct"),
                option.get("explanation"),
            )
        )
    try:
        _normalize_options(options, multi_select=payload.get("multiSelect", False))
    except ValueError as error:
        raise ValueError(f"malformed saved MCQ output for {label}: {error}") from error

    fallbacks = []
    for mime in ("text/html", "text/plain"):
        value = data.get(mime)
        if isinstance(value, list) and all(isinstance(line, str) for line in value):
            value = "".join(value)
        if not isinstance(value, str) or not value.strip():
            raise ValueError(f"missing or malformed {mime} output for {label}")
        fallbacks.append(value)
    return LearningOutput(payload, fallbacks[0], fallbacks[1])


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
    if len(options) > _MAX_OPTIONS:
        raise ValueError(
            f"multiple_choice takes at most {_MAX_OPTIONS} options; "
            f"got {len(options)}"
        )

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
        if not _ID_RE.match(option_id):
            raise ValueError(
                f"multiple_choice option id {option_id!r} must be lowercase "
                "letters, digits and hyphens, start with a letter or digit, and "
                "be at most 64 characters; the renderer's Copilot action drops "
                "anything else"
            )
        if option_id in seen_ids:
            raise ValueError(f"duplicate multiple_choice option id: {option_id!r}")
        seen_ids.add(option_id)
        # An option the renderer would refuse to draw, for the same reason as
        # the prompt: it validates the text is there before rendering, so an
        # empty one fails for the learner rather than the author.
        if not text.strip():
            raise ValueError(
                f"multiple_choice option {option_id!r} needs text to show"
            )

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
