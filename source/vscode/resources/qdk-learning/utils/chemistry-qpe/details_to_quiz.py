"""Bake a chapter's self-check questions into answerable quizzes.

This is the quiz counterpart of ``bake_outputs.py``: it stores each question's
rendered output in the notebook so a learner sees it on opening the file, with
no kernel. Where ``bake_outputs.py`` starts a kernel to run the chemistry, a
quiz is pure data, so this just calls the emitter and keeps what it returns.

The first run on a chapter also has to create the cells to bake. Chapters
written by ``rst_to_notebook.py`` render each ``quiz-question`` admonition as a
``<details>`` disclosure whose only interaction is revealing the answer, so
those blocks are replaced with ``quiz()`` calls. Afterwards there is nothing
left to convert and every run is only a rebake.

The choices are written by hand in ``_unit.py``: deciding which wrong answers
are worth offering is the author's judgement, not something to generate. This
never writes ``_unit.py``.

Both steps are idempotent. The one-time conversion:

* a markdown cell is split at each quiz block, and the block becomes a code
  cell between the prose that surrounded it;
* quiz blocks with no prose between them share one code cell, because the
  progress tree names a code cell after the heading above it and two cells in
  one section would appear twice under the same name;
* the first fragment keeps the original cell's id and tags, so a ``section:``
  tag is not duplicated onto a fragment that does not start a section.

A first conversion needs the ids to substitute, in document order::

    python details_to_quiz.py 06-iterative-phase-estimation --ids iqpe-grid-target ...

After that the notebook names them, so rebaking after editing ``_unit.py`` is
just the unit, and ``--check`` reports drift without writing::

    python details_to_quiz.py 06-iterative-phase-estimation --check
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any

COURSE = Path(__file__).resolve().parent.parent.parent / "courses" / "chemistry-qpe"

#: The wrapper the generator emits around every self-check question. Matching
#: the whole wrapper, not just the ``<details>``, keeps the tinted border from
#: being left behind as an empty box.
QUIZ_BLOCK = re.compile(
    r'<div style="border-left:4px solid #8c4a00;[^"]*">\s*<details>.*?</details>\s*</div>',
    re.S,
)

#: How `rst_to_notebook.py` names a generated cell, so converted notebooks and
#: regenerated ones agree.
def _cell_id(body: str) -> str:
    return "c-" + hashlib.sha256(body.encode()).hexdigest()[:12]


#: A converted question. One ``quiz()`` call can name several ids.
#:
#: `verify_course.py` carries the same pair, deliberately: it is a script that
#: verifies on import, so importing it here would run the whole course check.
#: Two one-line regexes are cheaper than that coupling — but if the `quiz()`
#: call shape changes, both files need it.
QUIZ_CALL = re.compile(r"^quiz\(([^)]*)\)", re.M)
QUIZ_ID = re.compile(r'"([^"]+)"')

#: Deliberately looser than `QUIZ_CALL`, and matched anywhere in a line rather
#: than at its start: it answers "is there a question here at all", so a call
#: this tool cannot read — `quiz('a')`, `_unit.quiz("a")`, `x = quiz("a")` — is
#: reported rather than mistaken for a cell whose question was deleted.
QUIZ_MENTION = re.compile(r"\bquiz\s*\(")


def _load_unit_module(unit_dir: Path) -> tuple[Any, Any]:
    """Import a unit's ``_unit.py`` so its ``register_quiz`` calls run.

    Returns the unit module and the emitter module it registered into. The
    emitter is reached through ``sys.modules`` rather than the unit's
    namespace: importing ``_unit`` puts ``_learning_output`` there, and going
    to the source avoids depending on which names a unit chose to re-export.
    """
    course_root = str(unit_dir.parent)
    if course_root not in sys.path:
        sys.path.insert(0, course_root)
    if str(unit_dir) not in sys.path:
        sys.path.insert(0, str(unit_dir))

    spec = importlib.util.spec_from_file_location(
        f"_unit_{unit_dir.name}", unit_dir / "_unit.py"
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {unit_dir / '_unit.py'}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    emitter = sys.modules.get("_learning_output")
    if emitter is None:
        raise SystemExit(
            f"{unit_dir / '_unit.py'} does not import _learning_output, so it "
            "registers no quizzes."
        )
    return module, emitter


def _cell_tags(cell: dict[str, Any]) -> list[str]:
    tags = cell.get("metadata", {}).get("tags", [])
    return [str(t) for t in tags]


def _split_cell(source: str) -> list[tuple[str, str]]:
    """Split markdown into an alternating run of prose and quiz markers.

    Returns ``("prose", text)`` and ``("quiz", block)`` pairs in document
    order, with empty prose dropped.
    """
    pieces: list[tuple[str, str]] = []
    cursor = 0
    for match in QUIZ_BLOCK.finditer(source):
        prose = source[cursor : match.start()].strip("\n")
        if prose.strip():
            pieces.append(("prose", prose))
        pieces.append(("quiz", match.group(0)))
        cursor = match.end()
    tail = source[cursor:].strip("\n")
    if tail.strip():
        pieces.append(("prose", tail))
    return pieces


def _group_adjacent_quizzes(
    pieces: list[tuple[str, str]],
) -> list[tuple[str, list[str]]]:
    """Collapse a run of quizzes with no prose between them into one group."""
    grouped: list[tuple[str, list[str]]] = []
    for kind, text in pieces:
        if kind == "quiz" and grouped and grouped[-1][0] == "quiz":
            grouped[-1][1].append(text)
            continue
        grouped.append((kind, [text]))
    return grouped


def _baked_outputs(emitter: Any, ids: list[str]) -> list[dict[str, Any]]:
    """Render the display bundles the cell would produce when run.

    ``quiz()`` displays rather than returns, so running it yields one
    ``display_data`` output per question.
    """
    outputs = []
    for quiz_id in ids:
        bundle = emitter._lookup_quiz(quiz_id)._repr_mimebundle_()
        outputs.append(
            {
                "output_type": "display_data",
                "data": bundle,
                "metadata": {},
            }
        )
    return outputs


def _ensure_quiz_import(cell: dict[str, Any]) -> bool:
    """Add ``quiz`` to the unit import in the setup cell.

    Returns whether this cell *is* the setup cell, not whether it changed — so
    a re-run stops at the same place a first run did.

    The notebook's first code cell already imports from ``_unit``; extending
    that line keeps the plumbing a learner sees to the one import they were
    always going to run.
    """
    source = "".join(cell["source"])
    match = re.search(r"^from _unit import (.+)$", source, re.M)
    if match is None:
        # Not the setup cell. Distinct from "found it and it already imports
        # quiz", so the caller can stop scanning once the line is seen and not
        # go on to rewrite an exercise cell's own `from _unit import`.
        return False

    names = [name.strip() for name in match.group(1).split(",")]
    if "quiz" not in names:
        replacement = f"from _unit import {', '.join(sorted({*names, 'quiz'}))}"
        updated = source[: match.start()] + replacement + source[match.end() :]
        cell["source"] = updated.splitlines(keepends=True)
    return True


def _cell_quiz_ids(cell: dict[str, Any]) -> list[str]:
    """The quiz ids a single cell shows, in order.

    Code cells only. A ``quiz("id")`` inside fenced prose is an example, and a
    markdown cell has no outputs to bake into in any case.
    """
    if cell.get("cell_type") != "code":
        return []
    source = "".join(cell["source"])
    return [
        quiz_id
        for call in QUIZ_CALL.findall(source)
        for quiz_id in QUIZ_ID.findall(call)
    ]


def _calls_quiz(cell: dict[str, Any]) -> bool:
    """Whether a code cell calls ``quiz()`` in any form, readable or not.

    ``_cell_quiz_ids`` reads one spelling: the one this tool writes. Telling
    "no question here" apart from "a question written differently" is what
    stops a cell that says ``quiz('id')`` being mistaken for a removed
    question and having its baked output deleted.
    """
    if cell.get("cell_type") != "code":
        return False
    return QUIZ_MENTION.search("".join(cell["source"])) is not None


def _notebook_quiz_ids(notebook: dict[str, Any]) -> list[str]:
    """The quiz ids the notebook already shows, in document order."""
    found: list[str] = []
    for cell in notebook["cells"]:
        found.extend(_cell_quiz_ids(cell))
    return found


def _already_converted(notebook: dict[str, Any], quiz_ids: list[str]) -> bool:
    """True when the notebook already shows exactly these quizzes."""
    return _notebook_quiz_ids(notebook) == quiz_ids


def convert(notebook_path: Path, unit_dir: Path, quiz_ids: list[str]) -> dict[str, Any]:
    _unit_module, emitter = _load_unit_module(unit_dir)
    notebook = json.loads(notebook_path.read_text(encoding="utf-8"))

    imported = False
    remaining = list(quiz_ids)
    converted: list[dict[str, Any]] = []
    for cell in notebook["cells"]:
        source = "".join(cell["source"])
        if cell["cell_type"] == "code" and not imported:
            imported = _ensure_quiz_import(cell)
        if cell["cell_type"] != "markdown" or not QUIZ_BLOCK.search(source):
            converted.append(cell)
            continue

        groups = _group_adjacent_quizzes(_split_cell(source))
        # The original cell's id and tags belong to whichever fragment comes
        # first, whether that is prose or a question. Tying them to "the first
        # prose fragment" would drop a section: tag from a cell that opens
        # with a question.
        identity_used = False
        for kind, texts in groups:
            if kind == "prose":
                body = texts[0]
                if identity_used:
                    # Key order matches the cells `rst_to_notebook.py` emits,
                    # so a split fragment looks like every other cell.
                    fragment: dict[str, Any] = {
                        "cell_type": "markdown",
                        "id": _cell_id(body),
                        "metadata": {},
                        "source": [],
                    }
                else:
                    fragment = dict(cell)
                    identity_used = True
                fragment["source"] = body.splitlines(keepends=True)
                converted.append(fragment)
            else:
                # Mirrors the surplus-id check below. Without this the list
                # comprehension pops an empty list and the author gets an
                # IndexError traceback instead of being told what to fix.
                if len(remaining) < len(texts):
                    raise SystemExit(
                        f"not enough quiz ids: {len(quiz_ids)} given, but the "
                        "notebook asks more questions than that. Ids must be "
                        "given in document order, one per question; a single "
                        "quiz() call can name several, so count questions "
                        "rather than cells."
                    )
                ids = [remaining.pop(0) for _ in texts]
                call = "quiz({})\n".format(", ".join(f'"{i}"' for i in ids))
                # Tagged so the progress tree looks past this cell for the
                # section heading: a quiz sits inside a section rather than
                # starting one.
                tags = ["quiz"]
                quiz_cell_id = _cell_id(call)
                if not identity_used:
                    tags = sorted({*_cell_tags(cell), "quiz"})
                    quiz_cell_id = cell["id"]
                    identity_used = True
                converted.append(
                    {
                        "cell_type": "code",
                        "id": quiz_cell_id,
                        "execution_count": None,
                        "metadata": {"tags": tags},
                        "outputs": _baked_outputs(emitter, ids),
                        "source": [call],
                    }
                )

    if remaining:
        raise SystemExit(
            f"{len(remaining)} unused quiz id(s): {', '.join(remaining)}. "
            "Ids must be given in document order, one per question."
        )
    if not imported and quiz_ids:
        raise SystemExit(
            "could not find a 'from _unit import ...' line to add quiz to; "
            "add the import to the notebook's setup cell by hand."
        )

    notebook["cells"] = converted
    return notebook


def _rebake(notebook: dict[str, Any], emitter: Any, stale: dict[int, list[str]]) -> None:
    """Re-render the outputs of the cells ``_stale_baked_outputs`` named.

    Rebaking is per cell because a cell can hold several quizzes, so one stale
    question re-renders its neighbours too. That is why only the cells that
    need it are touched: it keeps the write to what the report named.
    """
    for index in stale:
        cell = notebook["cells"][index]
        ids = _cell_quiz_ids(cell)
        if ids:
            cell["outputs"] = _baked_outputs(emitter, ids)
            continue
        # The cell no longer calls quiz(), so there is nothing to re-render:
        # drop the orphaned questions and leave whatever else the cell
        # produced, which may be the output of code that replaced them.
        cell["outputs"] = [
            output
            for output in cell.get("outputs", [])
            if not _is_learning_output(output, emitter.MIME_TYPE)
        ]


def _normalize_bundle(data: Any) -> Any:
    """Undo nbformat's line splitting so two bundles can be compared.

    nbformat stores every non-JSON MIME value as a list of lines, and applies
    that on read *and* on write. So a notebook saved by VS Code, by Jupyter, or
    by ``bake_outputs.py`` holds lists where this tool wrote strings. Comparing
    without rejoining would report every quiz as stale forever.
    """
    if isinstance(data, list) and all(isinstance(x, str) for x in data):
        return "".join(data)
    if isinstance(data, dict):
        return {key: _normalize_bundle(value) for key, value in data.items()}
    return data


def _is_learning_output(output: dict[str, Any], mime_type: str) -> bool:
    """Whether this output is a baked learning payload rather than the cell's own.

    Takes the MIME type from the emitter instead of naming it again: the
    string already lives in `_learning_output.py`, `schema.ts` and
    `package.json`, and a fourth copy here is a fourth thing to drift.
    """
    return mime_type in (output.get("data") or {})


def _stale_baked_outputs(
    notebook: dict[str, Any], emitter: Any
) -> dict[int, list[str]]:
    """Report the cells whose baked output no longer matches ``_unit.py``.

    This is the drift that matters once a notebook is converted: the questions
    a learner sees are the outputs stored in the file, so editing a quiz's
    wording or its options without re-running this tool would leave the old
    version on screen.

    Keyed by cell index, and not by quiz id, because a cell can lose its last
    ``quiz()`` call. Its baked questions are then orphaned with no surviving id
    to name them, so anything keyed on ids alone could neither report them nor
    clear them. The value is what to name in the report: the stale ids, or an
    empty list for a cell that should no longer show a question at all.
    """
    stale: dict[int, list[str]] = {}
    for index, cell in enumerate(notebook["cells"]):
        ids = _cell_quiz_ids(cell)
        outputs = cell.get("outputs", [])

        if not ids:
            # A cell whose quiz() calls were all removed keeps rendering the
            # questions until its outputs are cleared. Only learning outputs
            # count, so an ordinary code cell's own outputs never look stale.
            if any(_is_learning_output(output, emitter.MIME_TYPE) for output in outputs):
                if _calls_quiz(cell):
                    # The cell still asks a question, in a spelling this tool
                    # does not read. Clearing its output would delete a live
                    # question, so say so instead.
                    raise SystemExit(
                        f"cell {cell.get('id', '?')} calls quiz() in a form "
                        'this tool cannot read. Write it as quiz("id"), with '
                        "double quotes at the start of a line, so a removed "
                        "question can be told apart from one written "
                        "differently."
                    )
                stale[index] = []
            continue

        for position, quiz_id in enumerate(ids):
            expected = _normalize_bundle(
                emitter._lookup_quiz(quiz_id)._repr_mimebundle_()
            )
            actual = (
                _normalize_bundle(outputs[position].get("data"))
                if position < len(outputs)
                else None
            )
            if actual != expected:
                stale.setdefault(index, []).append(quiz_id)

        # An output past the last quiz the cell still calls is left over from a
        # question that was removed. Nothing above compares it, so without this
        # the notebook keeps showing a deleted question while --check reports
        # the file as up to date. Naming the cell's remaining quizzes is what
        # makes `_rebake` re-render it, which drops the extra output.
        if len(outputs) > len(ids) and index not in stale:
            stale[index] = list(ids)
    return stale


def _describe_stale(cell: dict[str, Any], names: list[str]) -> str:
    """Name a stale cell for the report."""
    if names:
        return ", ".join(names)
    return f"a removed question in cell {cell.get('id', '?')}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("unit", help="unit folder name, e.g. 06-iterative-phase-estimation")
    parser.add_argument(
        "--ids",
        nargs="+",
        help="quiz ids in document order; only needed for a first conversion, "
        "since a converted notebook already names them",
    )
    parser.add_argument("--course", default=str(COURSE), help="course root")
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify without writing; exits non-zero if the notebook is stale",
    )
    args = parser.parse_args()

    unit_dir = Path(args.course) / args.unit
    notebooks = sorted(unit_dir.glob("*.ipynb"))
    if len(notebooks) != 1:
        raise SystemExit(f"expected exactly one notebook in {unit_dir}, found {len(notebooks)}")
    notebook_path = notebooks[0]

    original = notebook_path.read_text(encoding="utf-8")

    # Fall back to the ids the notebook already names, so re-running the
    # converter or checking for drift does not mean repeating the list every
    # time. A first conversion has none to read and still has to be told.
    quiz_ids = list(args.ids) if args.ids else _notebook_quiz_ids(json.loads(original))

    # Re-runnable on purpose. The conversion is a step after
    # `rst_to_notebook.py`, so a pipeline should be able to run it without
    # first checking whether the notebook was regenerated.
    if _already_converted(json.loads(original), quiz_ids):
        _unit_module, emitter = _load_unit_module(unit_dir)
        notebook = json.loads(original)
        stale = _stale_baked_outputs(notebook, emitter)

        # No quiz() calls and nothing baked to clear means the chapter was
        # never converted, and it has to be told the ids. Deciding that from
        # the baked outputs rather than from a `<details>` search is deliberate:
        # `QUIZ_BLOCK` scrapes a wrapper `rst_to_notebook.py` writes, and "that
        # pattern found nothing" must never be read as "nothing to do" — that
        # would report an unconverted chapter as up to date the day the wrapper
        # changes.
        if not quiz_ids and not stale:
            raise SystemExit(
                f"{notebook_path.name} has no quiz() calls yet, so --ids is "
                "required to say which questions to substitute, in document order"
            )

        if not stale:
            print(f"{notebook_path.name}: already converted and up to date")
            return 0

        listed = ", ".join(
            sorted(
                _describe_stale(notebook["cells"][index], names)
                for index, names in stale.items()
            )
        )
        if args.check:
            print(f"{notebook_path.name}: baked output is stale for {listed}")
            return 1

        # Rebake in place rather than refusing: the questions live in
        # _unit.py, and the notebook is only a rendering of them.
        _rebake(notebook, emitter, stale)
        notebook_path.write_text(
            json.dumps(notebook, indent=1, ensure_ascii=False) + "\n",
            encoding="utf-8",
            newline="\n",
        )
        print(f"{notebook_path.name}: rebaked {listed}")
        return 0

    converted = convert(notebook_path, unit_dir, quiz_ids)
    # nbformat writes one-space indent and a trailing newline; match it so the
    # file stays comparable with the ones `rst_to_notebook.py` produces.
    text = json.dumps(converted, indent=1, ensure_ascii=False) + "\n"

    quiz_cells = sum(
        1 for c in converted["cells"] if c["cell_type"] == "code" and c["source"][0].startswith("quiz(")
    )
    print(f"{notebook_path.name}: {len(quiz_ids)} questions in {quiz_cells} cells")

    if args.check:
        print("unchanged" if text == original else "would rewrite")
        return 0 if text == original else 1

    notebook_path.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {notebook_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
