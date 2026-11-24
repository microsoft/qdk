"""Interactive outputs for QDK learning notebooks.

This module lives at the course root. Per-unit helper files (``_unit.py``)
import from it and re-export the small authoring surface the notebooks need.

The output model
----------------
Quiz outputs include a QDK MIME payload for the interactive VS Code renderer,
static HTML for other notebook frontends, and plain text for text-only
frontends such as terminal IPython.

The QDK renderer defines the payload shape (``source/vscode/src/notebookRenderer/schema.ts``
in ``microsoft/qdk``). When the VS Code extension builds, its
``checkRendererContract()`` check compares shared constants and payload fields
in the Python emitter and renderer.
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

# Well-known cell roles in the learning notebook format.
_CELL_ROLE_TAGS = frozenset(
    {"exercise", "quiz", "hint", "solution", "explanation", "author-only"}
)

# Match the quiz and option IDs accepted by the VS Code extension host
# (ID_PATTERN in source/vscode/src/notebookRenderer/schema.ts in microsoft/qdk).
_ID_RE = re.compile(r"\A[a-z0-9][a-z0-9-]{0,63}\Z")

# Match the literal-ID header used to generate learner quiz calls in VS Code
# (QUIZ_HEADER in source/vscode/src/learning/notebookExercises.ts in microsoft/qdk).
_QUIZ_TRIVIA = r"(?:\s|#[^\r\n]*(?:\r?\n|$))*"
_QUIZ_HEADER = re.compile(
    rf"""^{_QUIZ_TRIVIA}quiz[ \t]*\({_QUIZ_TRIVIA}(["'])([^"'\\\r\n]*)\1{_QUIZ_TRIVIA},"""
)

# The VS Code extension host limits the number of IDs in a quiz message
# (MAX_OPTION_IDS in source/vscode/src/notebookRenderer/schema.ts in microsoft/qdk).
_MAX_OPTIONS = 8
_QuizOption = tuple[str, str, bool, str | None]

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

# Use the tutorial's self-check band even without QDK theme variables.
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
        """Return the three representations, filtered as requested by IPython."""
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
    options: Sequence[_QuizOption],
    *,
    multi_select: bool = False,
    payload_id: str | None = None,
) -> LearningOutput:
    """Create a multiple-choice learning output.

    ``options`` are ``(id, text, correct, explanation)`` tuples. Option ids must
    be unique, and a question needs at least two options, so authoring mistakes
    fail when the author runs the cell that declares the quiz.

    A single-select question needs exactly one correct option. Pass
    ``multi_select=True`` to show checkboxes. The learner must select every
    correct option and no others, even when one or all options are correct.
    """
    normalized = _normalize_options(options, multi_select=multi_select)
    if not isinstance(prompt, str) or not prompt.strip():
        raise ValueError("multiple_choice requires question text as a non-empty string")
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


def find_author_notebook(unit_dir: Path) -> Path:
    """Find the unit's single authored notebook, excluding learner workbooks."""
    notebooks = [
        path
        for path in unit_dir.glob("*.ipynb")
        if not path.name.endswith(".workbook.ipynb") and path.is_file()
    ]
    if len(notebooks) != 1:
        raise ValueError(
            f"expected exactly one author notebook in {unit_dir}, found {len(notebooks)}"
        )
    return notebooks[0]


def quiz_for_notebook(notebook: Path) -> Callable[..., None]:
    """Bind learner reruns to one authored notebook, without registering IDs."""
    if not notebook.is_absolute():
        raise ValueError("quiz notebook path must be absolute")
    return partial(quiz, notebook=notebook)


def _shuffled(seed: str, options: Sequence[_QuizOption]) -> list[_QuizOption]:
    """Shuffle a copy in a repeatable order for the same seed and option list."""
    shuffled = list(options)
    random.Random(seed).shuffle(shuffled)  # DevSkim: ignore DS148264
    return shuffled


def quiz(
    quiz_id: str,
    *,
    prompt: str | None = None,
    options: Sequence[_QuizOption] | None = None,
    multi_select: bool = False,
    shuffle: bool = True,
    notebook: Path | None = None,
) -> None:
    """Display an author definition or resolve a named learner quiz from source.

    Supply one id, ``prompt`` and ``options`` to construct an authored quiz
    directly. Use ``shuffle=False`` to keep the options in author order.
    ID-only calls read saved source on every rerun, never outputs, and do not
    change the author notebook.
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
        for output in notebook_quiz_outputs(read_quiz_notebook(notebook)).values()
    }
    if quiz_id not in outputs:
        raise ValueError(f"no quiz {quiz_id!r} is authored in {notebook}")
    display(outputs[quiz_id])


def _quiz_output(
    quiz_id: str,
    prompt: str,
    options: Sequence[_QuizOption],
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


def _quiz_error_context(quiz_id: object, cell_number: int | None = None) -> str:
    location = f"cell {cell_number}" if cell_number is not None else ""
    if isinstance(quiz_id, str) and quiz_id.strip():
        return f"Quiz {quiz_id!r}" + (f" ({location})" if location else "")
    return location.capitalize() if location else "Quiz cell"


def _cell_source(cell: Mapping[str, Any], error_context: str) -> str:
    source = cell.get("source", "")
    if isinstance(source, list) and all(isinstance(line, str) for line in source):
        source = "".join(source)
    if not isinstance(source, str):
        raise ValueError(f"{error_context}: expected source text")
    return source


def _quiz_call(statement: ast.stmt) -> ast.Call | None:
    if (
        isinstance(statement, ast.Expr)
        and isinstance(statement.value, ast.Call)
        and isinstance(statement.value.func, ast.Name)
        and statement.value.func.id == "quiz"
    ):
        return statement.value
    return None


def authored_quiz(
    cell: Mapping[str, Any], *, cell_number: int | None = None
) -> tuple[str, dict[str, Any]] | None:
    """Read a quiz cell's single literal definition without executing Python."""
    if (
        cell.get("cell_type") != "code"
        or "quiz" not in cell.get("metadata", {}).get("tags", [])
    ):
        return None
    cell_context = _quiz_error_context(None, cell_number)
    source = _cell_source(cell, cell_context)
    try:
        tree = ast.parse(source)
    except SyntaxError as error:
        raise ValueError(f"{cell_context}: invalid quiz syntax: {error.msg}") from error
    if len(tree.body) != 1:
        raise ValueError(
            f"{cell_context}: write exactly one quiz() definition per quiz cell"
        )
    call = _quiz_call(tree.body[0])
    if call is None:
        raise ValueError(f"{cell_context}: only quiz() calls and comments are allowed")
    if len(call.args) != 1:
        raise ValueError(f"{cell_context}: expected quiz(id, prompt=..., options=...)")
    try:
        quiz_id = ast.literal_eval(call.args[0])
    except (ValueError, TypeError):
        raise ValueError(f"{cell_context}: quiz id must be a literal string") from None
    context = _quiz_error_context(quiz_id, cell_number)
    header = _QUIZ_HEADER.match(source)
    if header is None or header.group(2) != quiz_id:
        raise ValueError(
            f'{context}: write the quiz ID as a plain quoted string, e.g. quiz("my-quiz", ...)'
        )
    fields: dict[str, Any] = {}
    for keyword in call.keywords:
        if keyword.arg is None:
            raise ValueError(
                f"{context}: **kwargs are not supported; write quiz arguments explicitly"
            )
        if (
            keyword.arg not in {"prompt", "options", "multi_select", "shuffle"}
            or keyword.arg in fields
        ):
            raise ValueError(
                f"{context}: unsupported or duplicate quiz field {keyword.arg!r}"
            )
        try:
            fields[keyword.arg] = ast.literal_eval(keyword.value)
        except (ValueError, TypeError):
            raise ValueError(
                f"{context}: {keyword.arg} must be literal data, not a variable or call"
            ) from None
    if not isinstance(quiz_id, str) or not {"prompt", "options"} <= fields.keys():
        raise ValueError(f"{context}: quiz needs a string id, prompt and options")
    if any(
        not isinstance(fields.get(name, False), bool)
        for name in ("multi_select", "shuffle")
    ):
        raise ValueError(f"{context}: multi_select and shuffle must be booleans")
    return quiz_id, fields


def notebook_quiz_outputs(
    document: Mapping[str, Any],
) -> dict[str, LearningOutput]:
    """Generate each quiz cell's outputs from source, ignoring saved outputs."""
    cells = document.get("cells")
    if not isinstance(cells, list):
        raise ValueError("quiz notebook must contain a cells array")
    outputs: dict[str, LearningOutput] = {}
    seen_ids: set[str] = set()
    for cell_number, cell in enumerate(cells, start=1):
        if not isinstance(cell, dict):
            raise ValueError(f"Cell {cell_number}: expected a notebook cell object")
        definition = authored_quiz(cell, cell_number=cell_number)
        if definition is None:
            continue
        quiz_id, fields = definition
        context = _quiz_error_context(quiz_id, cell_number)
        cell_id = cell.get("id")
        if not isinstance(cell_id, str) or not cell_id or cell_id in outputs:
            raise ValueError(
                f"{context}: quiz cells need unique, nonempty notebook cell IDs"
            )
        if quiz_id in seen_ids:
            raise ValueError(
                f"{context}: duplicate quiz ID; choose a unique ID within this notebook"
            )
        seen_ids.add(quiz_id)
        try:
            outputs[cell_id] = _quiz_output(quiz_id, **fields)
        except (TypeError, ValueError) as error:
            raise ValueError(f"{context}: {error}") from None
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
        context = _quiz_error_context(None, cell_number)
        if not isinstance(cell, dict):
            raise ValueError(f"{context}: expected a notebook cell object")
        tags = cell.get("metadata", {}).get("tags", [])
        if "quiz" in tags:
            validate_cell_roles(cell, cell_number=cell_number)
            if cell.get("cell_type") != "code":
                raise ValueError(f"{context}: the 'quiz' tag belongs on a code cell")
            continue
        if cell.get("cell_type") != "code":
            continue
        source = _cell_source(cell, context)
        if not re.search(r"\bquiz\s*\(", source):
            continue
        try:
            tree = ast.parse(source)
        except SyntaxError as error:
            raise ValueError(
                f"{context}: cannot check quiz calls in invalid Python: {error.msg}"
            ) from error
        for statement in tree.body:
            call = _quiz_call(statement)
            if call is not None:
                argument = call.args[0] if call.args else None
                quiz_id = argument.value if isinstance(argument, ast.Constant) else None
                context = _quiz_error_context(quiz_id, cell_number)
                raise ValueError(f"{context}: add the 'quiz' tag to this code cell")
    count = len(notebook_quiz_outputs(document))
    noun = "quiz" if count == 1 else "quizzes"
    print(f"Validated {count} {noun} in {notebook.name}.")


# ---------------------------------------------------------------------------
# Internals
# ---------------------------------------------------------------------------


def _normalize_options(
    options: Sequence[_QuizOption],
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

    if not multi_select and correct_count > 1:
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
