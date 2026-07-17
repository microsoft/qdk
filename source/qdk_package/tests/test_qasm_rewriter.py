# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import random
import threading
from typing import Any

import pytest

from qdk import openqasm
from qdk.openqasm import parser, semantic
from qdk.openqasm.parser import QASMNode


def test_rewriter_replaces_deletes_inserts_and_returns_independent_snapshot() -> None:
    source = "OPENQASM 3.0;\nqubit q;\nx q;\ny q;\n"
    original = parser.parse(source, path="main.qasm")

    class RewriteGates(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> parser.RewriteReturn:
            insertion = self.replace(
                parser.SourceRange(
                    0,
                    parser.Position(1, 0, parser.PositionEncoding.UTF8),
                    parser.Position(1, 0, parser.PositionEncoding.UTF8),
                ),
                "// generated\n",
            )
            self.generic_visit(node)
            return insertion

        def visit_QuantumGate(self, node: Any) -> parser.RewriteReturn:
            if node.name.name == "x":
                return self.replace(node.name, "h")
            return self.delete(node)

    rewritten = RewriteGates().rewrite(original)

    assert original.document.entry.text == source
    assert rewritten.document.entry.text == (
        "OPENQASM 3.0;\n// generated\nqubit q;\nh q;\n\n"
    )
    assert not rewritten.has_errors
    assert rewritten is not original
    assert rewritten.program is not original.program
    assert rewritten.document is not original.document
    assert rewritten.program.document is rewritten.document


def test_rewriter_applies_multiline_annotation_and_adjacent_edits() -> None:
    source = "OPENQASM 3.0;\n@tag value\nx q;\ny q;\n"
    original = parser.parse(source)

    class RewriteRanges(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> parser.RewriteReturn:
            del node
            multiline = parser.SourceRange(
                0,
                parser.Position(2, 0, parser.PositionEncoding.UTF8),
                parser.Position(4, 0, parser.PositionEncoding.UTF8),
            )
            return self.replace(multiline, "h q;\n")

        def visit_Annotation(self, node: QASMNode) -> parser.RewriteReturn:
            return self.replace(node, "@renamed payload")

    class RewriteAnnotation(parser.QASMRewriter):
        def visit_Annotation(self, node: QASMNode) -> parser.RewriteReturn:
            return self.replace(node, "@renamed payload")

    annotation_result = RewriteAnnotation().rewrite(original)
    multiline_result = RewriteRanges().rewrite(annotation_result)
    adjacent_source = parser.parse("OPENQASM 3.0;\nx q;\n")

    class Adjacent(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> parser.RewriteReturn:
            del node
            return [
                parser.SourceEdit(
                    parser.SourceRange(
                        0,
                        parser.Position(1, 1, parser.PositionEncoding.UTF8),
                        parser.Position(1, 3, parser.PositionEncoding.UTF8),
                    ),
                    " r",
                ),
                parser.SourceEdit(
                    parser.SourceRange(
                        0,
                        parser.Position(1, 0, parser.PositionEncoding.UTF8),
                        parser.Position(1, 1, parser.PositionEncoding.UTF8),
                    ),
                    "h",
                ),
            ]

    adjacent_result = Adjacent().rewrite(adjacent_source)

    assert annotation_result.document.entry.text == (
        "OPENQASM 3.0;\n@renamed payload\nx q;\ny q;\n"
    )
    assert multiline_result.document.entry.text == (
        "OPENQASM 3.0;\n@renamed payload\nh q;\n"
    )
    assert adjacent_result.document.entry.text == "OPENQASM 3.0;\nh r;\n"


def test_callback_return_is_eager_and_generic_visit_collects_children_once() -> None:
    original = parser.parse("OPENQASM 3.0; qubit q; x q;")
    visited: list[str] = []
    yielded: list[str] = []

    class Eager(parser.QASMRewriter):
        def visit_QuantumGate(self, node: Any) -> parser.RewriteReturn:
            visited.append(node.name.name)
            self.generic_visit(node)

            def edits() -> Any:
                yielded.append("first")
                yield self.replace(node.name, "h")
                yielded.append("complete")

            return edits()

        def visit_Identifier(self, node: Any) -> None:
            visited.append(node.name)

    rewritten = Eager().rewrite(original)

    assert rewritten.document.entry.text == "OPENQASM 3.0; qubit q; h q;"
    assert yielded == ["first", "complete"]
    assert visited.count("x") == 2
    assert visited.count("q") == 2


def test_specialized_callback_controls_child_traversal() -> None:
    original = parser.parse("OPENQASM 3.0; qubit q; x q;")
    identifiers: list[str] = []

    class StopAtGate(parser.QASMRewriter):
        def visit_QuantumGate(self, node: Any) -> None:
            del node

        def visit_Identifier(self, node: Any) -> None:
            identifiers.append(node.name)

    StopAtGate().rewrite(original)

    assert identifiers == ["q"]


def test_existing_include_replays_snapshot_but_new_include_is_unresolved() -> None:
    source = 'OPENQASM 3.0;\ninclude "defs.inc";\nqubit q;\nlocal q;\n'
    includes = {"defs.inc": "gate local q { x q; }"}
    original = parser.parse(source, path="main.qasm", includes=includes)

    class NoEdits(parser.QASMRewriter):
        pass

    replayed = NoEdits().rewrite(original)

    class RenameInclude(parser.QASMRewriter):
        def visit_Include(self, node: QASMNode) -> parser.RewriteReturn:
            return self.replace(node, 'include "renamed.inc";')

    renamed = RenameInclude().rewrite(original)

    assert not replayed.has_errors
    assert replayed.document.source_map.get(1).path == "defs.inc"
    assert replayed.document.source_map.get(1).text == includes["defs.inc"]
    assert renamed.has_errors
    assert renamed.document.source_map.get(1).path == "renamed.inc"
    assert renamed.document.source_map.get(1).resolution_status == "unresolved"


def test_snapshot_resolver_replays_resolved_alias() -> None:
    source = 'OPENQASM 3.0;\ninclude "../defs.inc";\n'
    original = parser.parse(
        source,
        path="pkg/app/main.qasm",
        includes={"pkg/defs.inc": "gate local q { x q; }"},
    )

    rewritten = parser.QASMRewriter().rewrite(original)

    assert not rewritten.has_errors
    assert rewritten.document.source_map.get(1).path == "pkg/defs.inc"
    assert rewritten.document.source_map.get(1).text == "gate local q { x q; }"


@pytest.mark.parametrize(
    ("edit_factory", "code"),
    [
        (
            lambda result: parser.SourceEdit(
                parser.SourceRange(
                    99,
                    parser.Position(0, 0, parser.PositionEncoding.UTF8),
                    parser.Position(0, 0, parser.PositionEncoding.UTF8),
                ),
                "x",
            ),
            "unknown-source",
        ),
        (
            lambda result: parser.SourceEdit(
                result.document.source_map.range_from_span(
                    result.document.source_map.get(1).span,
                    encoding=parser.PositionEncoding.UTF8,
                ),
                "x",
            ),
            "include-edit",
        ),
        (
            lambda result: parser.SourceEdit(
                parser.SourceRange(
                    0,
                    parser.Position(0, 0, parser.PositionEncoding.UTF8),
                    parser.Position(0, 1, parser.PositionEncoding.CODE_POINT),
                ),
                "x",
            ),
            "mixed-encoding",
        ),
        (
            lambda result: parser.SourceEdit(
                parser.SourceRange(
                    0,
                    parser.Position(0, 2, parser.PositionEncoding.UTF8),
                    parser.Position(0, 1, parser.PositionEncoding.UTF8),
                ),
                "x",
            ),
            "reversed-range",
        ),
    ],
)
def test_rewriter_rejects_invalid_ranges_before_publication(
    edit_factory: Any,
    code: str,
) -> None:
    original = parser.parse(
        'OPENQASM 3.0; include "defs.inc"; aé;',
        includes={"defs.inc": ""},
    )
    original_text = original.document.entry.text

    class Invalid(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> parser.RewriteReturn:
            del node
            return edit_factory(original)

    with pytest.raises(parser.QASMRewriteError) as caught:
        Invalid().rewrite(original)

    assert caught.value.code == code
    assert caught.value.edit_index == 0
    assert caught.value.range is not None
    assert original.document.entry.text == original_text
    assert original.program.document is original.document


def test_rewriter_rejects_invalid_utf8_boundary() -> None:
    original = parser.parse("é")

    class InvalidBoundary(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> parser.RewriteReturn:
            del node
            return parser.SourceEdit(
                parser.SourceRange(
                    0,
                    parser.Position(0, 1, parser.PositionEncoding.UTF8),
                    parser.Position(0, 2, parser.PositionEncoding.UTF8),
                ),
                "x",
            )

    with pytest.raises(parser.QASMRewriteError) as caught:
        InvalidBoundary().rewrite(original)

    assert caught.value.code == "invalid-position"


def test_rewriter_rejects_overlap_and_duplicate_insertions() -> None:
    original = parser.parse("OPENQASM 3.0;\nqubit q;\n")

    def source_range(start: int, end: int) -> parser.SourceRange:
        return parser.SourceRange(
            0,
            parser.Position(1, start, parser.PositionEncoding.UTF8),
            parser.Position(1, end, parser.PositionEncoding.UTF8),
        )

    class Overlap(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> parser.RewriteReturn:
            del node
            return [
                parser.SourceEdit(source_range(0, 4), "bit"),
                parser.SourceEdit(source_range(3, 5), "X"),
            ]

    class DuplicateInsertion(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> parser.RewriteReturn:
            del node
            return [
                parser.SourceEdit(source_range(0, 0), "// one\n"),
                parser.SourceEdit(source_range(0, 0), "// two\n"),
            ]

    with pytest.raises(parser.QASMRewriteError) as overlap:
        Overlap().rewrite(original)
    with pytest.raises(parser.QASMRewriteError) as duplicate:
        DuplicateInsertion().rewrite(original)

    assert overlap.value.code == "overlap"
    assert duplicate.value.code == "ambiguous-insertion"


def test_rewriter_rejects_stale_range() -> None:
    first = parser.parse("OPENQASM 3.0; qubit q; x q;")
    second = parser.parse("OPENQASM 3.0; qubit q; x q;")
    stale_range = first.document.source_map.range_from_span(
        first.program.statements[1].span,
        encoding=parser.PositionEncoding.UTF8,
    )

    class StaleRange(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> parser.RewriteReturn:
            del node
            return parser.SourceEdit(stale_range, "h q;")

    with pytest.raises(parser.QASMRewriteError) as range_error:
        StaleRange().rewrite(second)

    assert range_error.value.code == "invalid-position"


def test_invalid_callback_return_and_iterator_exception_propagate() -> None:
    original = parser.parse("OPENQASM 3.0; qubit q;")
    expected = RuntimeError("iteration failed")

    class InvalidReturn(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> object:
            del node
            return object()

    class FailingIterator(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> Any:
            del node

            def edits() -> Any:
                yield parser.SourceEdit(
                    parser.SourceRange(
                        0,
                        parser.Position(0, 0, parser.PositionEncoding.UTF8),
                        parser.Position(0, 0, parser.PositionEncoding.UTF8),
                    ),
                    "",
                )
                raise expected

            return edits()

    with pytest.raises(TypeError):
        InvalidReturn().rewrite(original)
    with pytest.raises(RuntimeError) as caught:
        FailingIterator().rewrite(original)

    assert caught.value is expected
    assert original.document.entry.text == "OPENQASM 3.0; qubit q;"


def test_callback_exception_cleans_state_and_original_remains_usable() -> None:
    original = parser.parse("OPENQASM 3.0; qubit q;")
    expected = RuntimeError("callback failed")

    class SometimesFails(parser.QASMRewriter):
        fail = True

        def visit_Program(self, node: QASMNode) -> None:
            if self.fail:
                raise expected
            self.generic_visit(node)

    rewriter = SometimesFails()
    with pytest.raises(RuntimeError) as caught:
        rewriter.rewrite(original)
    rewriter.fail = False
    rewritten = rewriter.rewrite(original)

    assert caught.value is expected
    assert rewritten.document.entry.text == original.document.entry.text
    assert rewritten.document is not original.document


def test_reentrant_and_concurrent_rewrite_calls_are_rejected() -> None:
    original = parser.parse("OPENQASM 3.0; qubit q;")

    class Reentrant(parser.QASMRewriter):
        nested = True

        def visit_Program(self, node: QASMNode) -> None:
            if self.nested:
                self.rewrite(original)
            self.generic_visit(node)

    reentrant = Reentrant()
    with pytest.raises(RuntimeError, match="already rewriting"):
        reentrant.rewrite(original)
    reentrant.nested = False
    assert not reentrant.rewrite(original).has_errors

    entered = threading.Event()
    release = threading.Event()
    errors: list[BaseException] = []

    class Blocking(parser.QASMRewriter):
        def visit_Program(self, node: QASMNode) -> None:
            entered.set()
            release.wait(timeout=5)
            self.generic_visit(node)

    blocking = Blocking()

    def run_rewrite() -> None:
        try:
            blocking.rewrite(original)
        except BaseException as error:
            errors.append(error)

    worker = threading.Thread(target=run_rewrite)
    worker.start()
    assert entered.wait(timeout=5)
    with pytest.raises(RuntimeError, match="already rewriting"):
        blocking.rewrite(original)
    release.set()
    worker.join(timeout=5)

    assert not worker.is_alive()
    assert errors == []


def test_rewriter_returns_diagnostic_result_for_malformed_syntax() -> None:
    original = parser.parse("OPENQASM 3.0; qubit q; x q;")

    class BreakSyntax(parser.QASMRewriter):
        def visit_QuantumGate(self, node: QASMNode) -> parser.RewriteReturn:
            return self.replace(node, "x (")

    rewritten = BreakSyntax().rewrite(original)

    assert rewritten.has_errors
    assert rewritten.diagnostics
    assert not original.has_errors
    assert original.document.entry.text == "OPENQASM 3.0; qubit q; x q;"


def test_rewriter_output_can_be_canonicalized_and_semantically_reanalyzed() -> None:
    original = parser.parse(
        'OPENQASM 3.0; include "stdgates.inc"; qubit q; x q;'
    )

    class RenameGate(parser.QASMRewriter):
        def visit_QuantumGate(self, node: Any) -> parser.RewriteReturn:
            return self.replace(node.name, "h")

    rewritten = RenameGate().rewrite(original)
    canonical = parser.dumps(rewritten.program)
    analyzed = semantic.analyze(canonical)

    assert canonical == (
        'OPENQASM 3.0;\ninclude "stdgates.inc";\nqubit q;\nh q;\n'
    )
    assert not analyzed.has_errors


def test_edit_application_is_deterministic_across_generated_boundaries() -> None:
    randomizer = random.Random(0x45444954)
    original = parser.parse(
        "OPENQASM 3.0;\n// abcdefghijklmnopqrstuvwxyz\nqubit q;\n"
    )

    for _ in range(32):
        points = sorted(randomizer.sample(range(26), 8))
        ranges = list(zip(points[::2], points[1::2]))
        replacements = [
            "".join(randomizer.choice("XYZ") for _ in range(randomizer.randrange(4)))
            for _ in ranges
        ]
        expected_comment = "abcdefghijklmnopqrstuvwxyz"
        for (start, end), replacement in reversed(list(zip(ranges, replacements))):
            expected_comment = (
                expected_comment[:start] + replacement + expected_comment[end:]
            )
        expected = f"OPENQASM 3.0;\n// {expected_comment}\nqubit q;\n"

        for _ in range(4):
            order = list(range(len(ranges)))
            randomizer.shuffle(order)

            class GeneratedEdits(parser.QASMRewriter):
                def visit_Program(self, node: QASMNode) -> parser.RewriteReturn:
                    del node
                    return [
                        parser.SourceEdit(
                            parser.SourceRange(
                                0,
                                parser.Position(
                                    1,
                                    ranges[index][0] + 3,
                                    parser.PositionEncoding.UTF8,
                                ),
                                parser.Position(
                                    1,
                                    ranges[index][1] + 3,
                                    parser.PositionEncoding.UTF8,
                                ),
                            ),
                            replacements[index],
                        )
                        for index in order
                    ]

            rewritten = GeneratedEdits().rewrite(original)
            assert rewritten.document.entry.text == expected
            assert original.document.entry.text.endswith("abcdefghijklmnopqrstuvwxyz\nqubit q;\n")


def test_rewriter_public_contract_and_final_visit() -> None:
    assert openqasm.QASMRewriter is parser.QASMRewriter
    assert openqasm.QASMRewriteError is parser.QASMRewriteError
    assert issubclass(parser.QASMRewriteError, ValueError)

    with pytest.raises(TypeError, match="final"):

        class Invalid(parser.QASMRewriter):
            def visit(self, node: Any) -> None:
                del node

    with pytest.raises(TypeError):
        parser.QASMRewriter().rewrite(object())  # type: ignore[arg-type]
    with pytest.raises(RuntimeError):
        parser.QASMRewriter().visit(object())
    with pytest.raises(RuntimeError):
        parser.QASMRewriter().replace(
            parser.SourceRange(
                0,
                parser.Position(0, 0, parser.PositionEncoding.UTF8),
                parser.Position(0, 0, parser.PositionEncoding.UTF8),
            ),
            "",
        )
    with pytest.raises(RuntimeError):
        parser.QASMRewriter().delete(
            parser.SourceRange(
                0,
                parser.Position(0, 0, parser.PositionEncoding.UTF8),
                parser.Position(0, 0, parser.PositionEncoding.UTF8),
            )
        )
