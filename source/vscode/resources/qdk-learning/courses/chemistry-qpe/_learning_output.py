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

import ast
import json
import random
import re
from dataclasses import dataclass
from functools import partial
from html import escape
from pathlib import Path
from typing import Any, Callable, Iterable, Mapping, Sequence

MIME_TYPE = "application/vnd.qdk.learning+json"

_CELL_ROLE_TAGS = frozenset(
    {"exercise", "quiz", "hint", "solution", "explanation", "author-only"}
)

#: A quiz or option id has to survive the trip to the extension host, which
#: accepts only this shape from a notebook — nothing longer, and nothing that
#: could read as prose. Enforcing it here means an author finds out when they
#: run the cell, rather than a learner finding the "Why is that wrong?" button
#: quietly doing less than it should. Keep in step with `ID_PATTERN` in
#: `schema.ts`.
_ID_RE = re.compile(r"\A[a-z0-9][a-z0-9-]{0,63}\Z")

# Keep the literal-ID header in step with QUIZ_HEADER in notebookExercises.ts.
_QUIZ_TRIVIA = r"(?:\s|#[^\r\n]*(?:\r?\n|$))*"
_QUIZ_HEADER = re.compile(
    rf"""^{_QUIZ_TRIVIA}quiz[ \t]*\({_QUIZ_TRIVIA}(["'])([^"'\\\r\n]*)\1{_QUIZ_TRIVIA},"""
)

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
# Notebook-owned quizzes
# ---------------------------------------------------------------------------


def quiz_for_notebook(notebook: Path) -> Callable[..., None]:
    """Bind learner reruns to one authored notebook, without registering IDs."""
    if not notebook.is_absolute():
        raise ValueError("quiz notebook path must be absolute")
    return partial(quiz, notebook=notebook)


def _shuffled(seed: str, options: Sequence[Sequence[Any]]) -> list[Sequence[Any]]:
    """Permute options deterministically from a string seed.

    ``random.Random`` derives its state from a hash of the string rather than
    from ``PYTHONHASHSEED``, so the same id yields the same order on every
    machine and every run.
    """
    shuffled = list(options)
    random.Random(seed).shuffle(shuffled)  # DevSkim: ignore DS148264
    return shuffled


def quiz(
    quiz_id: str,
    *,
    prompt: str | None = None,
    options: Sequence[Sequence[Any]] | None = None,
    multi_select: bool = False,
    shuffle: bool = True,
    notebook: Path | None = None,
) -> None:
    """Display an author definition or resolve a named learner quiz from source.

    Supply one id, ``prompt`` and ``options`` to construct an authored quiz
    directly. ID-only calls read saved source on every rerun, never outputs,
    and do not change the author notebook.
    """
    try:
        from IPython.display import display
    except ImportError as exc:  # pragma: no cover - notebooks always have IPython
        raise RuntimeError(
            "quiz() displays its output and so needs IPython; "
            "use multiple_choice() directly outside a notebook."
        ) from exc

    if prompt is not None or options is not None or multi_select or not shuffle:
        if prompt is None or options is None:
            raise ValueError("an authored quiz needs one quiz id, prompt and options")
        display(
            _quiz_output(
                quiz_id,
                prompt,
                options,
                multi_select=multi_select,
                shuffle=shuffle,
            )
        )
        return

    if notebook is None:
        raise ValueError("learner quiz() calls need a source notebook binding")
    outputs = {
        output.payload["payloadId"]: output
        for rendered in notebook_quiz_outputs(read_quiz_notebook(notebook)).values()
        for output in rendered
    }
    if quiz_id not in outputs:
        raise ValueError(f"no quiz {quiz_id!r} is authored in {notebook}")
    display(outputs[quiz_id])


def _quiz_output(
    quiz_id: str,
    prompt: str,
    options: Sequence[Sequence[Any]],
    *,
    multi_select: bool = False,
    shuffle: bool = True,
) -> LearningOutput:
    ordered = _shuffled(quiz_id, options) if shuffle else options
    return multiple_choice(
        prompt, ordered, multi_select=multi_select, payload_id=quiz_id
    )


def read_quiz_notebook(notebook: Path) -> dict[str, Any]:
    try:
        document = json.loads(notebook.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read quiz notebook {notebook}: {error}") from error
    if not isinstance(document, dict) or not isinstance(document.get("cells"), list):
        raise ValueError(f"malformed quiz notebook {notebook}: expected cells")
    return document


def _quiz_label(quiz_id: object, cell_number: int | None = None) -> str:
    location = f"cell {cell_number}" if cell_number is not None else ""
    if isinstance(quiz_id, str) and quiz_id.strip():
        return f"Quiz {quiz_id!r}" + (f" ({location})" if location else "")
    return location.capitalize() if location else "Quiz cell"


def _cell_source(cell: Mapping[str, Any], label: str) -> str:
    source = cell.get("source", "")
    if isinstance(source, list) and all(isinstance(line, str) for line in source):
        source = "".join(source)
    if not isinstance(source, str):
        raise ValueError(f"{label}: expected source text")
    return source


def authored_quizzes(
    cell: Mapping[str, Any], *, cell_number: int | None = None
) -> list[tuple[str, dict[str, Any]]]:
    """Read literal quiz definitions without executing any notebook code."""
    if (
        cell.get("cell_type") != "code"
        or "quiz" not in cell.get("metadata", {}).get("tags", [])
    ):
        return []
    cell_label = _quiz_label(None, cell_number)
    source = _cell_source(cell, cell_label)
    try:
        tree = ast.parse(source)
    except SyntaxError as error:
        raise ValueError(f"{cell_label}: invalid quiz syntax: {error.msg}") from error
    if len(tree.body) != 1:
        raise ValueError(f"{cell_label}: write exactly one quiz() definition per quiz cell")
    authored = []
    for statement in tree.body:
        if (
            not isinstance(statement, ast.Expr)
            or not isinstance(statement.value, ast.Call)
            or not isinstance(statement.value.func, ast.Name)
            or statement.value.func.id != "quiz"
        ):
            raise ValueError(f"{cell_label}: only quiz() calls and comments are allowed")
        call = statement.value
        if len(call.args) != 1:
            raise ValueError(f"{cell_label}: expected quiz(id, prompt=..., options=...)")
        fields: dict[str, Any] = {}
        try:
            quiz_id = ast.literal_eval(call.args[0])
        except (ValueError, TypeError):
            raise ValueError(f"{cell_label}: quiz id must be a literal string") from None
        label = _quiz_label(quiz_id, cell_number)
        header = _QUIZ_HEADER.match(source)
        if header is None or header.group(2) != quiz_id:
            raise ValueError(
                f'{label}: write the quiz ID as a plain quoted string, e.g. quiz("my-quiz", ...)'
            )
        for keyword in call.keywords:
            if keyword.arg is None:
                raise ValueError(
                    f"{label}: **kwargs are not supported; write quiz arguments explicitly"
                )
            if (
                keyword.arg not in {"prompt", "options", "multi_select", "shuffle"}
                or keyword.arg in fields
            ):
                raise ValueError(
                    f"{label}: unsupported or duplicate quiz field {keyword.arg!r}"
                )
            try:
                fields[keyword.arg] = ast.literal_eval(keyword.value)
            except (ValueError, TypeError):
                raise ValueError(
                    f"{label}: {keyword.arg} must be literal data, not a variable or call"
                ) from None
        if not isinstance(quiz_id, str) or not {"prompt", "options"} <= fields.keys():
            raise ValueError(f"{label}: quiz needs a string id, prompt and options")
        if any(
            not isinstance(fields.get(name, False), bool)
            for name in ("multi_select", "shuffle")
        ):
            raise ValueError(f"{label}: multi_select and shuffle must be booleans")
        authored.append((quiz_id, fields))
    return authored


def notebook_quiz_outputs(
    document: Mapping[str, Any],
) -> dict[str, list[LearningOutput]]:
    """Generate each quiz cell's outputs from source, ignoring saved outputs."""
    cells = document.get("cells")
    if not isinstance(cells, list):
        raise ValueError("quiz notebook must contain a cells array")
    outputs = {}
    seen_ids: set[str] = set()
    for cell_number, cell in enumerate(cells, start=1):
        if not isinstance(cell, dict):
            raise ValueError(f"Cell {cell_number}: expected a notebook cell object")
        definitions = authored_quizzes(cell, cell_number=cell_number)
        if not definitions:
            continue
        cell_id = cell.get("id")
        if not isinstance(cell_id, str) or not cell_id or cell_id in outputs:
            raise ValueError(
                f"{_quiz_label(definitions[0][0], cell_number)}: "
                "quiz cells need unique, nonempty notebook cell IDs"
            )
        rendered = []
        for quiz_id, fields in definitions:
            label = _quiz_label(quiz_id, cell_number)
            if quiz_id in seen_ids:
                raise ValueError(
                    f"{label}: duplicate quiz ID; choose a unique ID within this notebook"
                )
            seen_ids.add(quiz_id)
            try:
                rendered.append(_quiz_output(quiz_id, **fields))
            except (TypeError, ValueError) as error:
                raise ValueError(f"{label}: {error}") from None
        outputs[cell_id] = rendered
    return outputs


def validate_cell_roles(cell: Mapping[str, Any], *, cell_number: int) -> None:
    """Reject conflicting content roles, allowing unrelated modifier tags."""
    roles = sorted(
        _CELL_ROLE_TAGS.intersection(cell.get("metadata", {}).get("tags", []))
    )
    if len(roles) > 1:
        names = ", ".join(repr(role) for role in roles)
        raise ValueError(
            f"Cell {cell_number}: conflicting content roles {names}; "
            "keep only one content-role tag"
        )


def validate_quizzes(notebook: Path) -> None:
    """Validate saved author source without running cells or changing outputs."""
    document = read_quiz_notebook(notebook)
    for cell_number, cell in enumerate(document["cells"], start=1):
        label = _quiz_label(None, cell_number)
        if not isinstance(cell, dict):
            raise ValueError(f"{label}: expected a notebook cell object")
        tags = cell.get("metadata", {}).get("tags", [])
        if "quiz" in tags:
            validate_cell_roles(cell, cell_number=cell_number)
            if cell.get("cell_type") != "code":
                raise ValueError(f"{label}: the 'quiz' tag belongs on a code cell")
            continue
        if cell.get("cell_type") != "code":
            continue
        source = _cell_source(cell, label)
        if not re.search(r"\bquiz\s*\(", source):
            continue
        try:
            tree = ast.parse(source)
        except SyntaxError as error:
            raise ValueError(
                f"{label}: cannot check quiz calls in invalid Python: {error.msg}"
            ) from error
        for statement in tree.body:
            if (
                isinstance(statement, ast.Expr)
                and isinstance(statement.value, ast.Call)
                and isinstance(statement.value.func, ast.Name)
                and statement.value.func.id == "quiz"
            ):
                argument = statement.value.args[0] if statement.value.args else None
                label = _quiz_label(
                    argument.value if isinstance(argument, ast.Constant) else None,
                    cell_number,
                )
                raise ValueError(f"{label}: add the 'quiz' tag to this code cell")
    outputs = notebook_quiz_outputs(document)
    count = sum(len(rendered) for rendered in outputs.values())
    noun = "quiz" if count == 1 else "quizzes"
    print(f"Validated {count} {noun} in {notebook.name}.")


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
