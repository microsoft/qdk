# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import io
from pathlib import Path
from typing import Any

import pytest

from qasm_corpus import CORPUS
from qdk import openqasm
from qdk.openqasm import parser, semantic

_OPENQASM_SAMPLE_DIR = Path(__file__).resolve().parents[3] / "samples" / "OpenQASM"
_OPENQASM_SAMPLES = sorted(_OPENQASM_SAMPLE_DIR.glob("*.qasm"))

# The exact text `dumps` emits for each shared-corpus source.
#
# The corpus is the one `test_qasm_reachability.py` sweeps, which proves it
# produces every concrete syntactic class except the handful that need
# malformed input. Pinning the whole corpus therefore pins the emitter across
# every node kind `dumps` can reach.
#
# `dumps` currently reparses the program's source and unparses the fresh Rust
# tree rather than walking the Python nodes. Widening `dumps` to accept any node
# would require an emitter that walks the Python tree instead, and that emitter
# is only a non-breaking substitution if it reproduces this text byte for byte.
# Nothing else in the suite pins it, so a change here is a compatibility
# decision about what users receive, not a test to be re-recorded.
_CANONICAL: dict[str, str] = {
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
duration d = 100.0ns;
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
  if (k == 2) {
    break;
  }
  if (k == 1) {
    continue;
  }
  h q[0];
}
for int[8] m in {1, 2, 3} {
  h q[1];
}
while (i < 3) {
  i += 1;
}
switch (i) {
  case 0 {
    x q[0];
  }
  case 1, 2 {
    y q[0];
  }
  default {
    z q[0];
  }
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
duration dt = durationof({
  h q[0];
});
delay[100.0ns] q[0];
box {
  h q[0];
}
{
  h q[1];
}
end;
""",
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
    "captures": """OPENQASM 3.0;
const int[8] outer = 3;
def g() -> int[8] {
  return outer;
}
""",
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
    "single_qubit_decl": """OPENQASM 3.0;
qubit q;
""",
    "string_literal": """OPENQASM 3.0;
"a string expression statement";
""",
}


@pytest.mark.parametrize("name", sorted(_CANONICAL), ids=str)
def test_dumps_emits_the_pinned_canonical_text(name: str) -> None:
    """Pins what `dumps` emits, so replacing the emitter cannot silently change it."""
    emitted = parser.dumps(parser.parse(CORPUS[name]).program)
    assert emitted == _CANONICAL[name], (
        f"`dumps` output changed for the {name!r} corpus source. Canonical output "
        "is part of the API contract: a future node-walking emitter is only a "
        "non-breaking substitution if it reproduces this text exactly. Re-record "
        "this expectation only as a deliberate decision to change what callers "
        "receive."
    )


def test_the_pinned_output_covers_the_whole_corpus() -> None:
    """Otherwise a corpus source could be added and go unpinned."""
    missing = sorted(set(CORPUS) - set(_CANONICAL))
    extra = sorted(set(_CANONICAL) - set(CORPUS))
    assert not missing, (
        "these corpus sources have no pinned `dumps` output; the classes they "
        "cover are unprotected against an emitter change:\n" + "\n".join(missing)
    )
    assert not extra, (
        "these pinned outputs name sources that left the corpus:\n" + "\n".join(extra)
    )


def test_dumps_canonicalizes_current_versions() -> None:
    cases = [
        ("OPENQASM 2.0; qreg q[2]; creg c[2];", "OPENQASM 2.0;"),
        ("OPENQASM 3.0; qubit[2] q; bit[2] c;", "OPENQASM 3.0;"),
        ("OPENQASM 3.1; qubit q;", "OPENQASM 3.1;"),
    ]
    for source, header in cases:
        result = parser.parse(source)
        assert not result.has_errors
        emitted = parser.dumps(result.program)
        assert emitted.startswith(header + "\n")
        assert emitted.endswith("\n")
        assert not emitted.endswith("\n\n")
        assert "\r" not in emitted
        reparsed = parser.parse(emitted)
        assert not reparsed.has_errors
        assert parser.dumps(reparsed.program) == emitted


@pytest.mark.parametrize("sample_path", _OPENQASM_SAMPLES, ids=lambda path: path.name)
def test_repository_sample_corpus_strictly_reparses_and_stabilizes(
    sample_path: Path,
) -> None:
    source = sample_path.read_text(encoding="utf-8")
    result = parser.parse(source, path=str(sample_path))

    assert not result.has_errors
    emitted = parser.dumps(result.program)
    strict_program = parser.parse_program(emitted)
    assert parser.dumps(strict_program) == emitted


def test_canonicalization_covers_annotations_pragmas_calibration_and_crlf() -> None:
    source = (
        "OPENQASM 3.1;\r\n"
        "@vendor.tag payload\r\n"
        "qubit q;\r\n"
        "pragma vendor.mode exact/*opaque*/  \r\n"
        'defcalgrammar "openpulse";\r\n'
        "cal { pulse frame; }\r\n"
        "defcal x $0 { play; }\r\n"
    )
    result = parser.parse(source)

    assert not result.has_errors
    emitted = parser.dumps(result.program)
    assert "\r" not in emitted
    assert emitted.startswith("OPENQASM 3.1;\n@vendor.tag payload\n")
    assert "pragma vendor.mode exact/*opaque*/  \n" in emitted
    assert 'defcalgrammar "openpulse";\n' in emitted
    assert "cal { pulse frame; }\n" in emitted
    assert "defcal x $0 { play; }\n" in emitted
    assert parser.dumps(parser.parse_program(emitted)) == emitted


def test_dumps_preserves_include_without_expanding_or_resolving() -> None:
    calls: list[str] = []

    def resolver(path: str) -> str:
        calls.append(path)
        return "gate local q { x q; }"

    result = parser.parse(
        'OPENQASM 3.0; include "custom.inc"; qubit q; local q;',
        includes=resolver,
    )
    assert not result.has_errors
    assert calls == ["custom.inc"]

    calls.clear()
    emitted = parser.dumps(result.program)
    assert calls == []
    assert 'include "custom.inc";' in emitted
    assert "gate local" not in emitted


def test_dump_writes_once_without_flush_or_close() -> None:
    program = parser.parse("OPENQASM 3.0; qubit q;").program

    class Stream:
        def __init__(self) -> None:
            self.calls: list[str] = []
            self.flushed = False
            self.closed = False

        def write(self, value: str) -> int:
            self.calls.append(value)
            return len(value)

        def flush(self) -> None:
            self.flushed = True

        def close(self) -> None:
            self.closed = True

    stream = Stream()
    assert parser.dump(program, stream) is None  # type: ignore[arg-type]
    assert stream.calls == [parser.dumps(program)]
    assert not stream.flushed
    assert not stream.closed


def test_dump_propagates_stream_exception() -> None:
    program = parser.parse("OPENQASM 3.0; qubit q;").program
    expected = RuntimeError("write failed")

    class FailingStream:
        def write(self, value: str) -> int:
            del value
            raise expected

    with pytest.raises(RuntimeError) as caught:
        parser.dump(program, FailingStream())  # type: ignore[arg-type]
    assert caught.value is expected


def test_dumps_rejects_recovered_entry_source_with_payload() -> None:
    result = parser.parse("OPENQASM 3.0; int value = ;")
    assert result.has_errors

    with pytest.raises(parser.QASMUnparseError) as caught:
        parser.dumps(result.program)

    error = caught.value
    assert error.code == "recovered-syntax"
    assert error.span is not None
    assert error.diagnostics
    assert isinstance(error.diagnostics, tuple)
    with pytest.raises(AttributeError):
        error.code = "changed"  # type: ignore[misc]
    with pytest.raises(AttributeError):
        error.span = None  # type: ignore[misc]
    with pytest.raises(AttributeError):
        error.diagnostics = ()  # type: ignore[misc]


def test_dumps_rejects_foreign_program() -> None:
    with pytest.raises(TypeError):
        parser.dumps(object())  # type: ignore[arg-type]


def test_dumps_names_both_program_types_when_given_a_semantic_program() -> None:
    """The two roots share the name `Program`, so PyO3's own message was useless.

    It read `'Program' object is not an instance of 'Program'`.
    """
    analyzed = semantic.analyze("OPENQASM 3.0; qubit q;").program
    with pytest.raises(TypeError) as caught:
        parser.dumps(analyzed)  # type: ignore[arg-type]

    message = str(caught.value)
    assert "qdk.openqasm.parser.Program" in message
    assert "qdk.openqasm.semantic.Program" in message
    assert "parse()" in message


@pytest.mark.parametrize(
    ("argument", "expected_actual"),
    [
        (object(), "builtins.object"),
        (None, "None"),
        ("OPENQASM 3.0;", "builtins.str"),
    ],
    ids=["foreign_object", "none", "source_string"],
)
def test_dumps_names_the_type_it_was_given(argument: Any, expected_actual: str) -> None:
    with pytest.raises(TypeError) as caught:
        parser.dumps(argument)

    message = str(caught.value)
    assert "qdk.openqasm.parser.Program" in message
    assert expected_actual in message


def test_dumps_rejects_a_node_that_is_not_the_root() -> None:
    statement = parser.parse("OPENQASM 3.0; qubit q;").program.statements[0]
    with pytest.raises(TypeError) as caught:
        parser.dumps(statement)  # type: ignore[arg-type]
    assert "qdk.openqasm.parser.QubitDeclaration" in str(caught.value)


def test_dump_reports_the_same_rejection_as_dumps() -> None:
    analyzed = semantic.analyze("OPENQASM 3.0; qubit q;").program
    with pytest.raises(TypeError) as from_dumps:
        parser.dumps(analyzed)  # type: ignore[arg-type]
    with pytest.raises(TypeError) as from_dump:
        parser.dump(analyzed, io.StringIO())  # type: ignore[arg-type]
    assert str(from_dump.value) == str(from_dumps.value)


def test_dump_supports_text_io() -> None:
    program = parser.parse("OPENQASM 3.0; qubit q;").program
    stream = io.StringIO()
    parser.dump(program, stream)
    assert stream.getvalue() == parser.dumps(program)


def test_dumps_ignores_mutable_foreign_attributes() -> None:
    program = parser.parse("OPENQASM 3.0; qubit q;").program
    with pytest.raises(AttributeError):
        setattr(program, "version", "2.0")
    assert parser.dumps(program).startswith("OPENQASM 3.0;\n")


def test_public_exception_is_value_error() -> None:
    assert issubclass(parser.QASMUnparseError, ValueError)
    assert openqasm.QASMUnparseError is parser.QASMUnparseError


def test_public_functions_reject_arbitrary_any() -> None:
    foreign: Any = None
    with pytest.raises(TypeError):
        openqasm.dumps(foreign)
