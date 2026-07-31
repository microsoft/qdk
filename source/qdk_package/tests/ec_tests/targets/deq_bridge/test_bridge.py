"""Tests for the qodec → deq bridge.

These tests are deq-aware: they exercise the bridge end-to-end through
deq's parser and library builder. They are skipped if deq or
deq_runtime is not importable.
"""
from __future__ import annotations

from pathlib import Path

import pytest

pytest.importorskip("deq")
pytest.importorskip("deq_runtime")

import qodec  # noqa: E402
import stim  # noqa: E402
import deq_runtime  # noqa: E402
from deq.proto import deq_bin_pb2  # noqa: E402

from ec_tests.testing.qodecs import c4  # noqa: E402
from qodec.circuits import header_for  # noqa: E402
from qdk.ec.targets._coerce import coerce_program  # noqa: E402
from qdk.ec.targets.deq import (  # noqa: E402
    from_deq,
    to_deq,
    to_deq_source,
    to_jit_library,
    to_stim_source,
)


EXAMPLES = Path("/home/adpaetzn/repositories/qodec/examples")


def _native_deq_runtime() -> bool:
    """Whether the native ``deq_runtime`` extension is actually built.

    The repo ships a pure-Python stub so ``import deq_runtime`` succeeds in
    Stim-only environments; any real call raises ``RuntimeError``. Tests that
    need JIT compilation skip when only the stub is present.
    """
    try:
        deq_runtime.static_jit_compile  # noqa: B018
    except RuntimeError:
        return False
    return True


def _load(name: str) -> qodec.Qodec:
    """Resolve a codec by name.

    ``c4-stim`` is the vendored ``c4`` fixture
    (:func:`tests.testing.qodecs.c4`); every other name is loaded from the
    qodec ``examples/`` directory.
    """
    if name == "c4-stim":
        return c4()
    return qodec.Qodec.load(str(EXAMPLES / name))


@pytest.mark.parametrize("name", ["c4-stim", "c4c6"])
def test_to_deq_source_produces_non_empty(name: str) -> None:
    src = to_deq_source(_load(name))
    assert "CODE" in src
    assert "GADGET" in src


@pytest.mark.parametrize("name", ["c4-stim", "c4c6"])
def test_to_jit_library_builds(name: str) -> None:
    lib = to_jit_library(_load(name))
    assert len(lib.port_types) > 0
    assert len(lib.gadget_types) > 0
    # Each port type should report a sensible k.
    for port in lib.port_types:
        assert port.k >= 1
    # Gadgets must round-trip their names from qodec.
    codec = _load(name)
    expected = set(codec.layers[-2].gadgets)
    actual = {g.base.name for g in lib.gadget_types}
    assert expected == actual


@pytest.mark.parametrize("name", ["c4-stim", "c4c6"])
def test_jit_library_compiles_to_bin(name: str) -> None:
    if not _native_deq_runtime():
        pytest.skip("deq_runtime native extension not built")
    lib = to_jit_library(_load(name))
    bin_bytes = deq_runtime.static_jit_compile(lib.SerializeToString())
    result = deq_bin_pb2.Library()
    result.ParseFromString(bin_bytes)
    assert len(result.gadget_types) == len(lib.gadget_types)
    assert len(result.port_types) == len(lib.port_types)


def _c4_slice_and_program() -> tuple[qodec.Qodec, object]:
    """The standalone C4 codec (bottom slice of c4c6) plus a prep+measure program."""
    full = qodec.Qodec.load(str(EXAMPLES / "c4c6"))
    codec = qodec.Qodec(layers=full.layers[1:], name="c4")
    isa = codec.layers[0].isa
    program = coerce_program(
        header_for(isa)
        + "\nqubit[2] q;\nbit reject = prepare_z_all(q);\nbit[2] result = measure_z_all(q);\n",
        isa,
    )
    return codec, program


def test_to_stim_source_requires_program() -> None:
    codec = qodec.Qodec.load(str(EXAMPLES / "c4c6"))
    with pytest.raises(ValueError, match="requires a program"):
        to_stim_source(codec)


def test_to_stim_source_emits_qdk_ready_physical_circuit() -> None:
    codec, program = _c4_slice_and_program()
    src = to_stim_source(codec, program=program)

    # deq-only bang-directives (e.g. its #!rhai logical-error block) must be
    # stripped; #!preselect would be kept but this program declares none.
    bang_lines = [
        line for line in src.splitlines() if line.lstrip().startswith("#!")
    ]
    assert all(line.lstrip().startswith("#!preselect") for line in bang_lines)
    assert "#!rhai" not in src

    # The remaining text is a valid physical circuit: two gadgets composed
    # into one program-wide qubit namespace (prepare_z_all -> measure_z_all
    # over the same 4 data wires), with 4 prep-ancilla + 4 data measurements.
    physical = stim.Circuit(
        "\n".join(l for l in src.splitlines() if not l.lstrip().startswith("#"))
    )
    assert physical.num_qubits == 8
    assert physical.num_measurements == 8

    # Logical/check structure survives: the prepared C4 block makes the four
    # prep-ancilla measurements (records 0..3) XOR to a fixed value on every
    # noiseless shot. (deq attributes such parities to checks/observables via
    # its Library; here we just confirm the determinism is present.)
    sample = physical.compile_sampler(seed=0).sample(4000)
    prep_ancilla_parity = sample[:, 0:4].sum(axis=1) % 2
    assert len(set(prep_ancilla_parity.tolist())) == 1


# A small hand-written `.deq` exercising the shapes from_deq must handle:
# a preparation (output only), a destructive measurement (input + readout),
# and a two-block transversal gate (two inputs + two outputs).
_REPETITION_DEQ = """\
CODE Rep [[3,1,3]] {
    LOGICAL X0*X1*X2 Z0
    STABILIZER Z0*Z1 Z1*Z2
}

GADGET PrepareZ {
    R 0 1 2
    OUTPUT Rep 0 1 2
}

GADGET MeasureZ {
    INPUT Rep 0 1 2
    M 0 1 2
    READOUT rec[-3]
}

GADGET TransversalCNOT {
    INPUT Rep 0 1 2
    INPUT Rep 3 4 5
    CX 0 3 1 4 2 5
    OUTPUT Rep 0 1 2
    OUTPUT Rep 3 4 5
}
"""


def test_from_deq_reconstructs_code_and_gadgets() -> None:
    codec = from_deq(_REPETITION_DEQ)
    assert [layer.isa.name for layer in codec.layers] == ["logical", "stim"]
    assert set(codec.codes) == {"Rep"}
    code = codec.codes["Rep"]
    assert list(code.stabilizers) == ["Z_0 Z_1", "Z_1 Z_2"]
    assert list(code.x) == ["X_0 X_1 X_2"]
    assert list(code.z) == ["Z_0"]
    assert set(codec.layers[0].gadgets) == {"PrepareZ", "MeasureZ", "TransversalCNOT"}


def test_deq_qodec_round_trip_is_stable_fixpoint() -> None:
    # `.deq` is lower-level than a qodec, so the invariant is a stable
    # fixpoint through qodec rather than byte-for-byte text equality.
    once = from_deq(_REPETITION_DEQ)
    twice = from_deq(to_deq(once))
    assert once == twice


def test_from_deq_rejects_unsupported_gate() -> None:
    source = (
        "CODE Rep [[3,1,3]] {\n    LOGICAL X0*X1*X2 Z0\n"
        "    STABILIZER Z0*Z1 Z1*Z2\n}\n"
        "GADGET Weird {\n    INPUT Rep 0 1 2\n    MPP Z0*Z1*Z2\n}\n"
    )
    with pytest.raises(NotImplementedError, match="unsupported stim gate"):
        from_deq(source)


def test_to_deq_skips_non_stim_gadget() -> None:
    # The qodec repetition3 example has a parameterized rotate_z gadget whose
    # inline-YAML body has no `.deq` representation; to_deq skips it cleanly.
    codec = qodec.Qodec.load(str(EXAMPLES / "repetition3"))
    source = to_deq(codec)
    assert "GADGET rotate_z" not in source
    assert "skipped gadget 'rotate_z'" in source
    rebuilt = from_deq(source)
    assert set(rebuilt.layers[0].gadgets) == {"idle", "measure_z", "prepare_z"}


def test_to_deq_is_to_deq_source_alias() -> None:
    codec = qodec.Qodec.load(str(EXAMPLES / "repetition3"))
    assert to_deq(codec) == to_deq_source(codec)


def _check_set(gadget: qodec.Gadget) -> set[frozenset[str]]:
    return {frozenset(str(ref) for ref in check) for check in gadget.checks}


def test_to_deq_captures_checks_and_from_deq_recovers_them() -> None:
    codec = qodec.Qodec.load(str(EXAMPLES / "repetition3"))
    source = to_deq(codec)

    # Checks are emitted as deq CHECK statements under a trusting @CHECKS.
    assert '@CHECKS("manual", verify=0)' in source
    assert "CHECK rec[" in source

    rebuilt = from_deq(source)
    # The explicit syndrome checks survive qodec -> .deq -> qodec (XOR order and
    # check order are irrelevant, so compare as sets of sets of references).
    for mnemonic in ("idle", "measure_z"):
        original = codec.layers[0].gadgets[mnemonic]
        recovered = rebuilt.layers[0].gadgets[mnemonic]
        assert _check_set(recovered) == _check_set(original)
