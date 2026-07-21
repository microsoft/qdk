# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import gc
import weakref
from enum import Enum

import pytest

from qdk.openqasm import (
    Position,
    PositionEncoding,
    SourceRange,
    parser,
)
from qdk.openqasm.parser import Span
from qdk.openqasm.source import SourceDocument, SourceFile, SourceMap


def test_position_encoding_uses_native_value_class_protocol() -> None:
    assert not issubclass(PositionEncoding, Enum)
    assert PositionEncoding.UTF8.value == "utf8"
    assert PositionEncoding.CODE_POINT.value == "code-point"
    assert PositionEncoding.UTF16.value == "utf16"
    assert int(PositionEncoding.UTF8) == 0
    assert PositionEncoding.CODE_POINT == 1
    assert hash(PositionEncoding.UTF16) == hash(PositionEncoding.UTF16)


def test_parse_result_and_program_share_document_identity() -> None:
    result = parser.parse("OPENQASM 3.0; qubit q;")

    assert result.program.document is result.document
    assert isinstance(result.document, SourceDocument)
    assert isinstance(result.document.source_map, SourceMap)
    assert isinstance(result.document.entry, SourceFile)


def test_source_map_uses_preorder_ids_status_and_exact_text() -> None:
    source = (
        'OPENQASM 3.0; include "empty.inc"; '
        'include "missing.inc"; include "nested.inc";'
    )
    includes = {
        "empty.inc": "",
        "nested.inc": 'include "leaf.inc";',
        "leaf.inc": "gate leaf q {}",
    }
    result = parser.parse(source, path="main.qasm", includes=includes)
    files = result.document.source_map.files

    assert isinstance(files, tuple)
    assert [source_file.id for source_file in files] == [0, 1, 2, 3, 4]
    assert [source_file.path for source_file in files] == [
        "main.qasm",
        "empty.inc",
        "missing.inc",
        "nested.inc",
        "leaf.inc",
    ]
    assert [source_file.resolution_status for source_file in files] == [
        "entry",
        "resolved",
        "unresolved",
        "resolved",
        "resolved",
    ]
    assert files[0].text == source
    assert files[1].text == ""
    assert files[2].text == ""
    assert files[3].text == includes["nested.inc"]
    assert files[4].text == includes["leaf.inc"]
    assert files[0].is_entry
    assert files[0].is_resolved
    assert not files[2].is_entry
    assert not files[2].is_resolved


def test_source_map_lookup_is_exact_and_preserves_duplicates() -> None:
    result = parser.parse(
        'OPENQASM 3.0; include "shared.inc"; include "shared.inc";',
        path="main.qasm",
        includes={"shared.inc": ""},
    )
    source_map = result.document.source_map

    assert list(source_map) == list(source_map.files)
    assert len(source_map) == 3
    assert source_map.entry == source_map.get(0)
    assert source_map.find("shared.inc") == source_map.get(1)
    assert source_map.find("SHARED.INC") is None
    assert source_map.find_all("shared.inc") == (
        source_map.get(1),
        source_map.get(2),
    )
    with pytest.raises(KeyError):
        source_map.get(100)


def test_source_values_are_frozen_and_compare_by_value() -> None:
    position = Position(2, 3)
    same_position = Position(2, 3, PositionEncoding.CODE_POINT)

    assert position == same_position
    assert hash(position) == hash(same_position)
    with pytest.raises(AttributeError):
        position.line = 4

    first = parser.parse("OPENQASM 3.0;").document
    second = parser.parse("OPENQASM 3.0;").document
    assert first == second
    assert first.source_map == second.source_map
    assert first.entry == second.entry


def test_source_ranges_from_different_documents_are_not_equal() -> None:
    first = parser.parse("OPENQASM 3.0; qubit a;").document.source_map
    second = parser.parse("OPENQASM 3.0; qubit b;").document.source_map

    first_range = first.range_from_span(first.entry.span)
    second_range = second.range_from_span(second.entry.span)

    assert first_range != second_range
    assert len({first_range, second_range}) == 2
    assert first.span_from_range(first_range) == first.entry.span
    with pytest.raises(ValueError, match="different document"):
        first.span_from_range(second_range)


def test_source_value_repr_and_hash_policy_is_explicit() -> None:
    position = Position(2, 3, PositionEncoding.UTF8)
    source_range = SourceRange(0, position, Position(2, 4, PositionEncoding.UTF8))
    first = parser.parse("OPENQASM 3.0;").document
    second = parser.parse("OPENQASM 3.0;").document

    scalar_values = [
        parser.Span(1, 2),
        position,
        source_range,
        first.entry,
    ]
    for value in scalar_values:
        assert type(value).__name__ in repr(value)
        assert hash(value) == hash(value)

    collection_values = [first.source_map, first]
    for value in collection_values:
        assert type(value).__name__ in repr(value)
        with pytest.raises(TypeError):
            hash(value)

    assert first == second
    assert first is not second
    assert first.source_map == second.source_map
    assert first.source_map is not second.source_map


def test_document_program_and_result_have_independent_lifetimes() -> None:
    result = parser.parse("OPENQASM 3.0; qubit q;")
    program = result.program
    document = result.document
    del result
    gc.collect()

    assert program.document is document
    assert document.entry.text == "OPENQASM 3.0; qubit q;"

    del program
    gc.collect()
    assert document.source_map.get(0).text == "OPENQASM 3.0; qubit q;"


def test_parse_result_does_not_retain_resolver_callback() -> None:
    class Resolver:
        def __call__(self, path: str) -> str | None:
            return "" if path == "empty.inc" else None

    resolver = Resolver()
    resolver_ref = weakref.ref(resolver)
    result = parser.parse(
        'OPENQASM 3.0; include "empty.inc";',
        includes=resolver,
    )
    del resolver
    gc.collect()

    assert resolver_ref() is None
    assert result.document.source_map.get(1).text == ""


def test_source_document_preserves_uri_paths() -> None:
    result = parser.parse(
        'OPENQASM 3.0; include "child.inc";',
        path="memory://workspace/main.qasm",
        includes={"memory://workspace/child.inc": ""},
    )

    assert [source_file.path for source_file in result.document.source_map] == [
        "memory://workspace/main.qasm",
        "memory://workspace/child.inc",
    ]


def test_resolver_exception_publishes_unresolved_source() -> None:
    def resolver(path: str) -> str | None:
        raise RuntimeError(f"cannot resolve {path}")

    result = parser.parse(
        'OPENQASM 3.0; include "broken.inc";',
        path="main.qasm",
        includes=resolver,
    )
    unresolved = result.document.source_map.get(1)

    assert result.has_errors
    assert unresolved.path == "broken.inc"
    assert unresolved.text == ""
    assert unresolved.resolution_status == "unresolved"


@pytest.mark.parametrize(
    ("encoding", "expected"),
    [
        (
            PositionEncoding.UTF8,
            [(0, 0), (0, 1), (0, 3), (0, 7), (0, 8), (1, 0), (2, 0), (2, 1)],
        ),
        (
            PositionEncoding.CODE_POINT,
            [(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (1, 0), (2, 0), (2, 1)],
        ),
        (
            PositionEncoding.UTF16,
            [(0, 0), (0, 1), (0, 2), (0, 4), (0, 5), (1, 0), (2, 0), (2, 1)],
        ),
    ],
)
def test_source_map_positions_round_trip_every_unicode_boundary(
    encoding: PositionEncoding,
    expected: list[tuple[int, int]],
) -> None:
    source_map = parser.parse("aé𝑓\r\n\nZ").document.source_map
    offsets = [0, 1, 3, 7, 8, 9, 10, 11]

    positions = [
        source_map.position_at(0, offset, encoding=encoding) for offset in offsets
    ]

    assert [(position.line, position.column) for position in positions] == expected
    assert all(position.encoding == encoding for position in positions)
    assert [source_map.byte_offset(0, position) for position in positions] == offsets


def test_source_coordinates_match_python_reference_at_generated_boundaries() -> None:
    fragments = ["a", "é", "Σ", "𝑓", "\r\n", "\n"]
    sources = [
        "".join(fragments[(start + index) % len(fragments)] for index in range(9))
        for start in range(len(fragments))
    ]
    sources.extend(["", "aéΣ𝑓", "aé\r\nΣ𝑓\n", "\r\n", "\n\n"])

    for source in sources:
        source_map = parser.parse(source).document.source_map
        source_bytes = source.encode("utf-8")
        boundaries = [0]
        byte_offset = 0
        for character in source:
            byte_offset += len(character.encode("utf-8"))
            boundaries.append(byte_offset)

        for offset in boundaries:
            prefix = source_bytes[:offset].decode("utf-8")
            line = prefix.count("\n")
            line_prefix = prefix.rsplit("\n", 1)[-1]
            expected_columns = {
                PositionEncoding.UTF8: len(line_prefix.encode("utf-8")),
                PositionEncoding.CODE_POINT: len(line_prefix),
                PositionEncoding.UTF16: len(line_prefix.encode("utf-16-le")) // 2,
            }
            for encoding, expected_column in expected_columns.items():
                position = source_map.position_at(0, offset, encoding=encoding)
                assert position == Position(line, expected_column, encoding)
                assert source_map.byte_offset(0, position) == offset

        invalid_offsets = set(range(len(source_bytes) + 1)) - set(boundaries)
        for offset in invalid_offsets:
            with pytest.raises(ValueError):
                source_map.position_at(0, offset)


@pytest.mark.parametrize(
    "operation",
    [
        lambda source_map: source_map.position_at(0, 2),
        lambda source_map: source_map.position_at(0, 12),
        lambda source_map: source_map.byte_offset(
            0, Position(0, 2, PositionEncoding.UTF8)
        ),
        lambda source_map: source_map.byte_offset(
            0, Position(0, 3, PositionEncoding.UTF16)
        ),
        lambda source_map: source_map.byte_offset(0, Position(1, 1)),
        lambda source_map: source_map.byte_offset(0, Position(3, 0)),
        lambda source_map: source_map.position_at(100, 0),
        lambda source_map: source_map.byte_offset(100, Position(0, 0)),
    ],
)
def test_source_map_invalid_coordinates_raise_value_error(operation: object) -> None:
    source_map = parser.parse("aé𝑓\r\n\nZ").document.source_map

    with pytest.raises(ValueError):
        operation(source_map)  # type: ignore[operator]


def test_source_map_converts_global_spans_and_rejects_invalid_ownership() -> None:
    result = parser.parse(
        'OPENQASM 3.0; include "child.inc";',
        path="main.qasm",
        includes={"child.inc": "gate child q {}"},
    )
    source_map = result.document.source_map
    entry = source_map.get(0)
    child = source_map.get(1)

    entry_range = source_map.range_from_span(entry.span, encoding=PositionEncoding.UTF16)
    child_range = source_map.range_from_span(child.span)
    entry_eof = source_map.range_from_span(Span(entry.span.hi, entry.span.hi))

    assert entry_range.source_id == 0
    assert entry_range.start == Position(0, 0, PositionEncoding.UTF16)
    assert source_map.span_from_range(entry_range) == entry.span
    assert child_range.source_id == 1
    assert source_map.span_from_range(child_range) == child.span
    assert entry_eof.start == entry_eof.end
    assert source_map.span_from_range(entry_eof) == Span(entry.span.hi, entry.span.hi)

    with pytest.raises(ValueError):
        source_map.range_from_span(Span(entry.span.hi, child.span.lo))
    with pytest.raises(ValueError):
        source_map.range_from_span(Span(entry.span.hi - 1, child.span.lo + 1))
    with pytest.raises(ValueError):
        source_map.range_from_span(Span(child.span.lo, entry.span.lo))
    with pytest.raises(ValueError):
        source_map.span_from_range(
            SourceRange(
                0,
                Position(0, 0, PositionEncoding.UTF8),
                Position(0, 1, PositionEncoding.CODE_POINT),
            )
        )
    with pytest.raises(ValueError):
        source_map.span_from_range(SourceRange(100, Position(0, 0), Position(0, 0)))


@pytest.mark.parametrize(
    "constructor",
    [
        lambda: Position(-1, 0),
        lambda: Position(0, -1),
        lambda: Position(2**32, 0),
        lambda: Position(0, 2**32),
        lambda: SourceRange(-1, Position(0, 0), Position(0, 0)),
        lambda: SourceRange(2**32, Position(0, 0), Position(0, 0)),
    ],
)
def test_source_coordinate_constructor_overflow_raises_overflow_error(
    constructor: object,
) -> None:
    with pytest.raises(OverflowError):
        constructor()  # type: ignore[operator]
