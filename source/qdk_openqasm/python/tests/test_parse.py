# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for the syntactic ``parse`` surface."""

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


def test_parse_returns_program():
    result = q.parse(SAMPLE)
    assert isinstance(result, q.ParseResult)
    assert not result.has_errors
    assert isinstance(result.program, q.Program)
    assert len(result.program.statements) > 0


def test_parse_version():
    result = q.parse(SAMPLE)
    assert result.program.version == (3, 0)


def test_parse_node_subclasses_and_isinstance():
    result = q.parse(SAMPLE)
    # The qubit declaration is a QuantumDeclStmt, which is a subclass of Stmt.
    decls = [s for s in result.program.statements if isinstance(s, q.QuantumDeclStmt)]
    assert len(decls) == 1
    decl = decls[0]
    assert isinstance(decl, q.Stmt)
    assert decl.kind == q.StmtKind.QuantumDecl
    # Every parsed statement is a subclass of the Stmt backbone.
    assert all(isinstance(s, q.Stmt) for s in result.program.statements)


def test_parse_children_and_spans():
    result = q.parse(SAMPLE)
    for stmt in result.program.statements:
        assert isinstance(stmt.span, q.Span)
        assert stmt.span.hi >= stmt.span.lo
        # children() must be a list (possibly empty) of nodes.
        assert isinstance(stmt.children(), list)


def test_parse_reports_errors():
    result = q.parse("OPENQASM 3.0; qubit ;")
    assert result.has_errors
    assert len(result.diagnostics) > 0
    diag = result.diagnostics[0]
    assert diag.severity == q.Severity.Error
    assert isinstance(diag.message, str)


def test_parse_with_dict_includes():
    source = 'OPENQASM 3.0; include "custom.inc"; my_gate q;'
    includes = {"custom.inc": "gate my_gate q { }"}
    result = q.parse(source, includes=includes)
    assert not result.has_errors


def test_parse_with_callable_includes():
    source = 'OPENQASM 3.0; include "custom.inc"; my_gate q;'

    def resolver(path: str) -> str:
        assert path == "custom.inc"
        return "gate my_gate q { }"

    result = q.parse(source, includes=resolver)
    assert not result.has_errors


def test_tokenize():
    tokens = q.tokenize("h q[0];")
    assert len(tokens) > 0
    assert all(isinstance(t, q.Token) for t in tokens)
    assert all(isinstance(t.span, q.Span) for t in tokens)
