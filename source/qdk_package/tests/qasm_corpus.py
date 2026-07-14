# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""The shared OpenQASM source corpus used by the node-surface guards.

The corpus exists to cover exported node classes, not the OpenQASM grammar.
Adding a construct is only warranted when it reaches a class nothing else
reaches. ``test_qasm_reachability`` uses it to prove every exported class is
producible; ``test_qasm_unparse`` uses it to pin the canonical text ``dumps``
emits for each source.
"""

from __future__ import annotations

# Abstract bases exist only for `isinstance` dispatch and are never instantiated.
ABSTRACT = {
    "QASMNode",
    "Expression",
    "Statement",
    "ClassicalType",
    "SemanticExpression",
    "SemanticStatement",
}

# Recovery placeholders. Each stands in for a subtree the parser or lowerer could
# not build, so they are reachable only from malformed input. The corpus is
# deliberately well-formed except where a source is listed in
# `SOURCES_WITH_EXPECTED_ERRORS`, so these may legitimately go unseen.
UNPRODUCIBLE = {
    "ErrorExpression": "parser recovery placeholder for an unparsable expression",
    "ErrorStatement": "parser recovery placeholder for an unparsable statement",
    "ErrorType": "parser recovery placeholder for an unparsable type",
}

# Sources that intentionally carry a semantic error, because the class they cover
# is only reachable through a construct the lowerer rejects.
SOURCES_WITH_EXPECTED_ERRORS = {
    "string_literal": "the syntax layer parses string literals; the lowerer rejects them",
}

CORPUS: dict[str, str] = {
    "gates": """OPENQASM 3.0;
include "stdgates.inc";
qubit[2] q;
bit[2] c;
h q[0];
ctrl @ x q[0], q[1];
inv @ s q[0];
pow(2) @ x q[1];
negctrl @ y q[0], q[1];
gphase(0.5);
barrier q;
reset q[0];
c[0] = measure q[0];
measure q[1] -> c[1];
""",
    "classical": """OPENQASM 3.0;
int[8] i = 3;
uint[16] u = 7;
float[64] f = 1.5;
angle[32] a = 0.25;
bit[4] b = "1010";
bool flag = true;
complex[float[64]] z = 2.0im;
const int[8] cint = 4;
duration d = 100ns;
stretch st;
array[int[8], 4] arr = {1, 2, 3, 4};
i = -i;
i += 1;
u = u + 1;
f = float[64](i);
i[0] = 1;
""",
    "io": """OPENQASM 3.0;
input int[8] shots;
output bit[2] result;
""",
    # Switch was introduced in OpenQASM 3.1.
    "control_flow": """OPENQASM 3.1;
include "stdgates.inc";
qubit[2] q;
int[8] i = 0;
if (i == 0) {
  x q[0];
} else {
  y q[0];
}
for int[8] k in [0:3] {
  if (k == 2) { break; }
  if (k == 1) { continue; }
  h q[0];
}
for int[8] m in {1, 2, 3} {
  h q[1];
}
while (i < 3) {
  i += 1;
}
switch (i) {
  case 0 { x q[0]; }
  case 1, 2 { y q[0]; }
  default { z q[0]; }
}
""",
    "subroutines": """OPENQASM 3.0;
include "stdgates.inc";
extern ext_fn(int[8]) -> int[8];
def sub(int[8] n, qubit[2] qs, readonly array[int[8], #dim = 1] ro) -> int[8] {
  h qs[0];
  return n + 1;
}
def takes_mutable(mutable array[int[8], 4] mu) {
}
gate mygate(theta) a, b {
  rz(theta) a;
  cx a, b;
}
qubit[2] q;
array[int[8], 4] data = {1, 2, 3, 4};
int[8] r = sub(1, q, data);
mygate(0.5) q[0], q[1];
""",
    "aliases_and_boxes": """OPENQASM 3.0;
include "stdgates.inc";
qubit[4] q;
let alias = q[0:1];
float[64] v = sin(0.5);
float[64] w = (1.0 + 2.0) * 3.0;
duration dt = durationof({ h q[0]; });
delay[100ns] q[0];
box {
  h q[0];
}
{
  h q[1];
}
end;
""",
    # `Concatenation` survives lowering only when both operands are arrays.
    "concatenation": """OPENQASM 3.0;
array[int[8], 2] lo = {1, 2};
array[int[8], 2] hi = {3, 4};
array[int[8], 4] both = lo ++ hi;
""",
    "runtime_sizeof": """OPENQASM 3.0;
def f(readonly array[int[8], #dim = 1] a) -> uint[32] {
  return sizeof(a);
}
""",
    # A subroutine referring to an enclosing const produces `CapturedIdentifier`.
    "captures": """OPENQASM 3.0;
const int[8] outer = 3;
def g() -> int[8] {
  return outer;
}
""",
    # `ExpressionStatement` wrapping a call, and a subroutine call in statement position.
    "expression_statement": """OPENQASM 3.0;
def noret() {
}
noret();
""",
    "annotations_and_pragmas": """OPENQASM 3.0;
pragma qdk.box.unroll
@my.annotation payload
qubit[1] q;
""",
    "calibration": """OPENQASM 3.0;
defcalgrammar "openpulse";
cal {
  extra
}
defcal mydefcal $0 {
  extra
}
""",
    "hardware_qubits": """OPENQASM 3.0;
include "stdgates.inc";
h $0;
cx $0, $1;
""",
    # An unsized `qubit` lowers to `QubitDeclaration`; a sized one to `QubitArrayDeclaration`.
    "single_qubit_decl": """OPENQASM 3.0;
qubit q;
""",
    "string_literal": """OPENQASM 3.0;
"a string expression statement";
""",
}
