# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for the visitor, rewriter, and ``unparse`` surface."""

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


def test_syntax_visitor_counts_nodes():
    program = q.parse(SAMPLE).program

    class Counter(q.SyntaxVisitor):
        def __init__(self):
            self.count = 0

        def generic_visit(self, node):
            self.count += 1
            super().generic_visit(node)

    counter = Counter()
    counter.visit(program)
    assert counter.count > 0


def test_syntax_visitor_dispatch_by_type():
    program = q.parse(SAMPLE).program

    class GateCollector(q.SyntaxVisitor):
        def __init__(self):
            self.gates = []

        def visit_GateCallStmt(self, node):
            self.gates.append(node)
            self.generic_visit(node)

    collector = GateCollector()
    collector.visit(program)
    assert len(collector.gates) >= 2


def test_unparse_native():
    program = q.parse(SAMPLE).program
    emitted = q.unparse(program)
    assert isinstance(emitted, str)
    assert "OPENQASM 3.0" in emitted


def test_unparse_round_trips():
    program = q.parse(SAMPLE).program
    emitted = q.unparse(program)
    # Re-parsing the emitted source must succeed without errors.
    reparsed = q.parse(emitted)
    assert not reparsed.has_errors


def test_syntax_rewriter_unparse():
    program = q.parse(SAMPLE).program
    rewriter = q.SyntaxRewriter()
    rewritten = rewriter.rewrite(program)
    emitted = rewriter.unparse(rewritten)
    assert isinstance(emitted, str)
    assert "OPENQASM 3.0" in emitted
