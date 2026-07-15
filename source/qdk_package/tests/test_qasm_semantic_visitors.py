# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Proves that every ``visit_<NodeType>`` callback of the semantic visitor is
dispatched correctly.

A comprehensive `OpenQASM` program is analyzed, and a :class:`QASMVisitor`
subclass is built with a ``visit_<NodeType>`` method for *every* semantic node
class exported by :mod:`qdk.openqasm.semantic`. Each callback records that it
fired and asserts it received a node of exactly its own type. The test then
checks that the set of callbacks that fired equals the set of node types
actually present in the tree (collected independently), and that both counts
match.
"""

from typing import Any, Callable, Dict, List, Tuple

from qdk.openqasm import semantic
from qdk.openqasm.semantic import QASMNode, QASMVisitor, Statement

# A single program exercising a broad range of OpenQASM 3 constructs so that as
# many distinct semantic node types as possible appear in the analyzed tree.
_PROGRAM = """OPENQASM 3.0;
include "stdgates.inc";
extern myext(int) -> int;
gate mygate(theta) q { rz(theta) q; }
def add(int a, int b) -> int { return a + b; }
const int N = 3;
input int shots;
output bit result;
qubit q;
qubit[2] qs;
bit c;
bit[2] cs;
int a = 1 + 2;
float f = -1.5;
int b = (a * 2) - 1;
int d = add(a, N);
bit[4] joined = cs ++ cs;
bool cond = a > 0;
a = 5;
a += 1;
let myalias = qs[0:1];
x q;
x $0;
mygate(0.5) q;
ctrl @ x qs[0], qs[1];
h qs[0];
gphase(pi / 2);
c = measure q;
measure qs[0];
reset q;
barrier q, qs;
delay[10ns] q;
box { x q; }
pragma qdk.example some custom pragma text
duration dur = durationof({x q;});
if (a == 3) { x q; } else { y q; }
for int i in [0:2] { x q; if (cond) { break; } else { continue; } }
while (a > 0) { a -= 1; }
switch (a) { case 1 { x q; } default { y q; } }
int g = int(f);
add(a, N);
end;
"""

# Core node types the program is guaranteed to produce. Kept as a stable subset
# so the test remains a meaningful proof of dispatch even if the analyzer gains
# or loses more exotic node kinds over time.
_REQUIRED_TYPES = {
    "Program",
    "QubitDeclaration",
    "QubitArrayDeclaration",
    "ClassicalDeclaration",
    "QuantumGate",
    "QuantumGateDefinition",
    "SubroutineDefinition",
    "Identifier",
    "BinaryExpression",
    "UnaryExpression",
    "LiteralExpression",
    "IndexExpression",
    "ParenExpression",
    "FunctionCall",
    "Cast",
    "BranchingStatement",
    "ForInLoop",
    "WhileLoop",
    "ReturnStatement",
    "QuantumMeasurement",
    "QuantumReset",
    "QuantumBarrier",
    "ClassicalAssignment",
    "CompoundStatement",
}

_MIN_DISTINCT_TYPES = 35


def _node_class_names() -> List[str]:
    """Every semantic node class name exported by ``qdk.openqasm.semantic``."""
    names = []
    for name in semantic.__all__:
        obj = getattr(semantic, name)
        if isinstance(obj, type) and issubclass(obj, QASMNode):
            names.append(name)
    return names


def _collect_present_types(program: Any) -> Dict[str, int]:
    """Independently collects ``type(node).__name__`` -> count over the tree."""
    counts: Dict[str, int] = {}

    class Collector(QASMVisitor):
        def generic_visit(self, node: Any) -> None:
            name = type(node).__name__
            counts[name] = counts.get(name, 0) + 1
            super().generic_visit(node)

    Collector().visit(program)
    return counts


def _make_callback(expected: str, dispatched: Dict[str, int], mismatches: List[Tuple[str, str]]) -> Callable:
    def callback(self: QASMVisitor, node: Any) -> None:
        actual = type(node).__name__
        if actual != expected:
            mismatches.append((expected, actual))
        dispatched[expected] = dispatched.get(expected, 0) + 1
        self.generic_visit(node)

    return callback


def test_every_semantic_visit_callback_dispatches() -> None:
    program = semantic.analyze(_PROGRAM).program

    present = _collect_present_types(program)
    assert _REQUIRED_TYPES <= set(present)
    assert len(present) >= _MIN_DISTINCT_TYPES

    dispatched: Dict[str, int] = {}
    mismatches: List[Tuple[str, str]] = []

    # Build a visitor with a visit_<NodeType> callback for every semantic node
    # class, so no node can fall back to generic_visit unhandled.
    attrs = {
        f"visit_{name}": _make_callback(name, dispatched, mismatches)
        for name in _node_class_names()
    }
    all_callbacks_visitor = type("AllSemanticCallbacks", (QASMVisitor,), attrs)

    all_callbacks_visitor().visit(program)

    # Every node was routed to the callback named after its own type.
    assert mismatches == []
    # Each present type's callback fired for exactly its nodes, and no present
    # type was missing a callback.
    assert dispatched == present
    # And every present type does have a dedicated callback method.
    for name in present:
        assert hasattr(all_callbacks_visitor, f"visit_{name}")


def test_error_statement_callback_dispatches() -> None:
    # A deliberately-invalid program (a trailing binary operator with no
    # right-hand operand) makes the analyzer emit an error statement node.
    result = semantic.analyze("OPENQASM 3.0; int a = 1 + ; ")
    assert result.has_errors

    error_nodes: List[Any] = []

    class Collector(QASMVisitor):
        def generic_visit(self, node: Any) -> None:
            if type(node).__name__ == "ErrorStatement":
                error_nodes.append(node)
            super().generic_visit(node)

    Collector().visit(result.program)
    assert error_nodes, "expected at least one ErrorStatement node in the tree"
    for node in error_nodes:
        assert isinstance(node, Statement)

    fired: List[Any] = []

    class ErrorVisitor(QASMVisitor):
        def visit_ErrorStatement(self, node: Any) -> None:
            fired.append(node)
            self.generic_visit(node)

    ErrorVisitor().visit(result.program)
    assert len(fired) >= 1
    for node in fired:
        assert type(node).__name__ == "ErrorStatement"
        assert isinstance(node, Statement)

