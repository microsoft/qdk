# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import random

import pytest

from qdk.openqasm import parser, tokens
from qdk.openqasm.parser import RawToken, RawTokenKind


def test_tokens_reconstruct_source_with_contiguous_utf8_spans() -> None:
    source = "OPENQASM 3.0;\r\n  // µs\nqubit q; §"
    result = tokens.tokenize(source)

    assert "".join(token.text for token in result) == source
    assert result[0].span.lo == 0
    assert result[-1].span.hi == len(source.encode("utf-8"))
    assert all(left.span.hi == right.span.lo for left, right in zip(result, result[1:]))
    assert all(
        token.text.encode("utf-8")
        == source.encode("utf-8")[token.span.lo : token.span.hi]
        for token in result
    )


def test_tokens_reconstruct_deterministic_arbitrary_utf8_inputs() -> None:
    randomizer = random.Random(0x5141534D)
    alphabet = [
        "a",
        "Z",
        "0",
        " ",
        "\t",
        "\n",
        "\r",
        '"',
        "/",
        "@",
        "§",
        "é",
        "Σ",
        "𝑓",
        "\u0000",
    ]
    cases = [
        "",
        '"unterminated',
        '"01',
        "/* unterminated",
        "OPENQASM 3.1;\r\n@tag µ\n",
    ]
    cases.extend(
        "".join(randomizer.choice(alphabet) for _ in range(randomizer.randrange(65)))
        for _ in range(123)
    )

    for source in cases:
        source_bytes = source.encode("utf-8")
        result = tokens.tokenize(source)
        assert "".join(token.text for token in result) == source
        assert all(
            token.text.encode("utf-8") == source_bytes[token.span.lo : token.span.hi]
            for token in result
        )
        assert all(
            left.span.hi == right.span.lo
            for left, right in zip(result, result[1:])
        )
        if result:
            assert result[0].span.lo == 0
            assert result[-1].span.hi == len(source_bytes)


def test_raw_token_kind_values_are_stable() -> None:
    assert {kind.value for kind in RawTokenKind} == {
        "bitstring",
        "comment",
        "hardware-qubit",
        "identifier",
        "literal-fragment",
        "newline",
        "number",
        "punctuation",
        "string",
        "unknown",
        "whitespace",
    }


def test_raw_token_details_and_trivia_are_stable() -> None:
    source = "// line\n/* block */ 0b10 0o7 12 0xF 1.5 10ns +"
    result = tokens.tokenize(source)

    details = {(token.kind, token.detail) for token in result}
    assert (RawTokenKind.COMMENT, "line") in details
    assert (RawTokenKind.COMMENT, "block") in details
    assert (RawTokenKind.NUMBER, "binary") in details
    assert (RawTokenKind.NUMBER, "octal") in details
    assert (RawTokenKind.NUMBER, "decimal") in details
    assert (RawTokenKind.NUMBER, "hex") in details
    assert (RawTokenKind.NUMBER, "float") in details
    assert (RawTokenKind.LITERAL_FRAGMENT, "ns") in details
    assert (RawTokenKind.PUNCTUATION, "+") in details
    assert all(
        token.is_trivia
        == (
            token.kind
            in {RawTokenKind.COMMENT, RawTokenKind.NEWLINE, RawTokenKind.WHITESPACE}
        )
        for token in result
    )


@pytest.mark.parametrize(
    ("source", "kind"),
    [
        ('"01', RawTokenKind.BITSTRING),
        ('"text', RawTokenKind.STRING),
    ],
)
def test_unterminated_literals_are_visible_and_incomplete(
    source: str, kind: RawTokenKind
) -> None:
    result = tokens.tokenize(source)

    assert len(result) == 1
    assert result[0].kind == kind
    assert result[0].text == source
    assert not result[0].is_complete


def test_unknown_characters_are_visible_and_complete() -> None:
    result = tokens.tokenize("§")

    assert len(result) == 1
    assert result[0].kind == RawTokenKind.UNKNOWN
    assert result[0].text == "§"
    assert result[0].span.lo == 0
    assert result[0].span.hi == 2
    assert result[0].is_complete


def test_unterminated_block_comment_remains_visible_as_trivia() -> None:
    source = "/* unterminated µ"
    result = tokens.tokenize(source)

    assert len(result) == 1
    assert result[0].kind == RawTokenKind.COMMENT
    assert result[0].detail == "block"
    assert result[0].text == source
    assert result[0].is_trivia
    assert result[0].is_complete
    assert result[0].span.hi == len(source.encode("utf-8"))


def test_tokens_are_frozen_eager_values() -> None:
    token = tokens.tokenize("q")[0]
    same_token = tokens.tokenize("q")[0]

    assert isinstance(token, RawToken)
    assert token == same_token
    assert hash(token) == hash(same_token)
    assert repr(token).startswith("RawToken(")
    assert repr(token.kind) == "<RawTokenKind.IDENTIFIER: 'identifier'>"
    with pytest.raises(AttributeError):
        token.text = "changed"  # type: ignore[misc]


def test_empty_input_and_parser_reexports() -> None:
    assert tokens.tokenize("") == []
    assert parser.tokenize is tokens.tokenize
    assert parser.RawToken is tokens.RawToken
    assert parser.RawTokenKind is tokens.RawTokenKind
