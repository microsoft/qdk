# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for the semantic ``analyze`` surface."""

import qdk_openqasm_parser as q

SAMPLE = """
OPENQASM 3.0;
include "stdgates.inc";
qubit[2] qs;
bit[2] c;
h qs[0];
cx qs[0], qs[1];
c = measure qs;
"""


def test_analyze_returns_program():
    result = q.analyze(SAMPLE)
    assert isinstance(result, q.SemanticResult)
    assert not result.has_errors
    assert isinstance(result.program, q.SemProgram)


def test_analyze_symbol_table_iterable():
    result = q.analyze(SAMPLE)
    table = result.symbols
    assert isinstance(table, q.SymbolTable)
    symbols = list(table)
    assert len(symbols) == len(table)
    assert len(symbols) > 0
    for symbol in symbols:
        assert isinstance(symbol, q.Symbol)
        assert isinstance(symbol.name, str)
        assert isinstance(symbol.ty, q.Type)
        assert isinstance(symbol.span, q.Span)


def test_analyze_symbol_lookup():
    result = q.analyze(SAMPLE)
    table = result.symbols
    qs = table.lookup("qs")
    assert qs is not None
    assert qs.name == "qs"
    # get() round-trips on the symbol id.
    assert table.get(qs.id) is not None
    assert table.get(qs.id).name == "qs"


def test_analyze_missing_symbol_lookup():
    result = q.analyze(SAMPLE)
    assert result.symbols.lookup("does_not_exist") is None


def test_analyze_reports_semantic_errors():
    # Using an undeclared identifier is a semantic error.
    result = q.analyze("OPENQASM 3.0; x undeclared_qubit;")
    assert result.has_errors
    assert len(result.diagnostics) > 0
