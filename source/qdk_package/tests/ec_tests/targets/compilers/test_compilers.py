"""Tests for qdk.ec.targets.compilers."""
from __future__ import annotations

import pytest

import qodec
from qodec.circuits import Program
from ec_tests.testing.qodecs import c4
from qdk.ec.targets.compilers import (
    AutoRelocate,
    CompileResult,
    Compiler,
    IdentityCompiler,
    RecursiveLowering,
    Relocate,
)


@pytest.fixture
def codec() -> qodec.Qodec:
    return c4()


@pytest.fixture
def source_isa(codec: qodec.Qodec) -> qodec.InstructionSet:
    return codec.layers[0].isa


def _program(isa: qodec.InstructionSet, *mnemonics: str) -> Program:
    return Program(
        [_call(isa, m) for m in mnemonics],
        isa,
    )


def _call(isa: qodec.InstructionSet, mnemonic: str) -> qodec.instructions.InstructionCall:
    """Build an `InstructionCall` with explicit operand bindings.

    Every operand declared by the ISA's instruction is bound (positionally)
    to the single block name ``"q"`` — sufficient for these single-block
    tests.
    """
    instruction = isa.instruction(mnemonic)
    inputs = {str(i): "q" for i in range(len(list(instruction.inputs)))}
    outputs = {str(i): "q" for i in range(len(list(instruction.outputs)))}
    if not inputs and not outputs:
        return qodec.instructions.InstructionCall(mnemonic)
    return qodec.instructions.InstructionCall(mnemonic, inputs=inputs, outputs=outputs)


# ── Compiler protocol & identity ────────────────────────────────────────────


def test_identity_compiler_satisfies_protocol() -> None:
    assert isinstance(IdentityCompiler(), Compiler)


def test_identity_returns_input_program(source_isa: qodec.InstructionSet) -> None:
    program = _program(source_isa, "prepare_zz")
    result = IdentityCompiler().compile(program)
    assert isinstance(result, CompileResult)
    assert result.program is program


def test_recursive_lowering_satisfies_protocol(codec: qodec.Codec) -> None:
    assert isinstance(RecursiveLowering(codec), Compiler)


def test_relocate_satisfies_protocol() -> None:
    assert isinstance(Relocate({}), Compiler)


def test_auto_relocate_satisfies_protocol() -> None:
    assert isinstance(AutoRelocate(), Compiler)


# ── Recursive lowering: behavior ────────────────────────────────────────────


def test_recursive_lowering_lowers_to_bottom_layer(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    program = _program(source_isa, "prepare_zz", "measure_zz")
    result = RecursiveLowering(codec).compile(program)
    assert result.program.isa.name == codec.layers[-1].isa.name


def test_recursive_lowering_expands_calls(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    program = _program(source_isa, "prepare_zz")
    result = RecursiveLowering(codec).compile(program)
    assert len(result.program.instructions) > 1


def test_recursive_lowering_rejects_wrong_isa(codec: qodec.Codec) -> None:
    bottom_isa = codec.layers[-1].isa
    program = _program(bottom_isa, "H")
    with pytest.raises(ValueError, match="does not match"):
        RecursiveLowering(codec).compile(program)


def test_lowering_namespaces_block(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    """Calls bind to a single block ``"q"``; qubits become ``q.0`` etc."""
    program = _program(source_isa, "prepare_zz")
    result = RecursiveLowering(codec).compile(program)
    r_qubits = [
        c.inputs["target"]
        for c in result.program.instructions
        if c.mnemonic == "R"
    ]
    assert r_qubits[:4] == ["q.0", "q.1", "q.2", "q.3"]


def test_lowering_handles_multi_block_without_collision(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    """Two distinct blocks get distinct namespaces."""
    program = Program(
        [
            qodec.instructions.InstructionCall(
                "transversal_cx",
                inputs={"control": "alice", "target": "bob"},
                outputs={"control": "alice", "target": "bob"},
            ),
        ],
        source_isa,
    )
    result = RecursiveLowering(codec).compile(program)
    cx_calls = [c for c in result.program.instructions if c.mnemonic == "CX"]
    pairs = [(c.inputs["control"], c.inputs["target"]) for c in cx_calls]
    assert pairs == [
        ("alice.0", "bob.0"),
        ("alice.1", "bob.1"),
        ("alice.2", "bob.2"),
        ("alice.3", "bob.3"),
    ]


def test_lowering_passes_through_ancillas(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    """Qubits outside any encoding's support keep their authored indices."""
    program = _program(source_isa, "prepare_zz")
    result = RecursiveLowering(codec).compile(program)
    # The ancilla qubit 4 in prepare_zz's body is not in any encoding.support;
    # it should pass through as the integer string "4".
    m_qubits = [
        c.inputs["target"]
        for c in result.program.instructions
        if c.mnemonic == "M"
    ]
    assert "4" in m_qubits


# ── Subcodec composition ────────────────────────────────────────────────────


def test_subcodec_identity_slice_lowers_trivially(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    sub = codec.slice(0, 1)
    program = _program(source_isa, "prepare_zz", "measure_zz")
    result = RecursiveLowering(sub).compile(program)
    assert result.program.isa.name == source_isa.name
    assert [c.mnemonic for c in result.program.instructions] == [
        "prepare_zz",
        "measure_zz",
    ]


def test_subcodec_full_range_equivalent_to_full_codec(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    sub = codec.slice(0, len(codec.layers))
    program = _program(source_isa, "prepare_zz")
    full_calls = [c.mnemonic for c in RecursiveLowering(codec).compile(program).program.instructions]
    sub_calls = [c.mnemonic for c in RecursiveLowering(sub).compile(program).program.instructions]
    assert full_calls == sub_calls


# ── Relocate: explicit label remap ──────────────────────────────────────────


def test_relocate_rewrites_labels(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    program = _program(source_isa, "prepare_zz")
    lowered = RecursiveLowering(codec).compile(program).program
    relocated = Relocate({"q.0": "10", "q.1": "11", "q.2": "12", "q.3": "13"}).compile(lowered).program
    r_qubits = [
        c.inputs["target"]
        for c in relocated.instructions
        if c.mnemonic == "R"
    ]
    assert r_qubits[:4] == ["10", "11", "12", "13"]


def test_relocate_passes_through_unmapped(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    program = _program(source_isa, "prepare_zz")
    lowered = RecursiveLowering(codec).compile(program).program
    # Only relocate two labels; the rest pass through.
    relocated = Relocate({"q.0": "100", "q.1": "101"}).compile(lowered).program
    r_qubits = [
        c.inputs["target"]
        for c in relocated.instructions
        if c.mnemonic == "R"
    ]
    assert r_qubits[:4] == ["100", "101", "q.2", "q.3"]


def test_relocate_from_block_placement(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    program = Program(
        [
            qodec.instructions.InstructionCall(
                "transversal_cx",
                inputs={"control": "alice", "target": "bob"},
                outputs={"control": "alice", "target": "bob"},
            ),
        ],
        source_isa,
    )
    lowered = RecursiveLowering(codec).compile(program).program
    relocator = Relocate.from_block_placement(
        {"alice": [0, 1, 2, 3], "bob": [10, 11, 12, 13]}
    )
    relocated = relocator.compile(lowered).program
    cx_calls = [c for c in relocated.instructions if c.mnemonic == "CX"]
    pairs = [(c.inputs["control"], c.inputs["target"]) for c in cx_calls]
    assert pairs == [("0", "10"), ("1", "11"), ("2", "12"), ("3", "13")]


# ── AutoRelocate: first-seen integer assignment ────────────────────────────


def test_auto_relocate_assigns_first_seen_integers(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    """AutoRelocate over the lowered single-block program assigns integers
    in first-seen order, reproducing the c4 example's natural numbering."""
    program = _program(source_isa, "prepare_zz", "measure_zz")
    lowered = RecursiveLowering(codec).compile(program).program
    relocated = AutoRelocate().compile(lowered).program
    # First seen labels (in instruction order) should be "q.0", "q.1", "q.2", "q.3", "4".
    # AutoRelocate maps them to "0", "1", "2", "3", "4".
    r_qubits = [
        c.inputs["target"]
        for c in relocated.instructions
        if c.mnemonic == "R"
    ]
    assert r_qubits[:4] == ["0", "1", "2", "3"]
    m_qubits = [
        c.inputs["target"]
        for c in relocated.instructions
        if c.mnemonic == "M"
    ]
    assert m_qubits[0] == "4"


def test_auto_relocate_handles_multi_block(codec: qodec.Codec, source_isa: qodec.InstructionSet) -> None:
    program = Program(
        [
            qodec.instructions.InstructionCall(
                "transversal_cx",
                inputs={"control": "alice", "target": "bob"},
                outputs={"control": "alice", "target": "bob"},
            ),
        ],
        source_isa,
    )
    lowered = RecursiveLowering(codec).compile(program).program
    relocated = AutoRelocate().compile(lowered).program
    cx_calls = [c for c in relocated.instructions if c.mnemonic == "CX"]
    pairs = [(c.inputs["control"], c.inputs["target"]) for c in cx_calls]
    # First-seen order across the gadget's CX bodies:
    #   alice.0 → 0, bob.0 → 1, alice.1 → 2, bob.1 → 3, ...
    # Body is "CX 0 4 1 5 2 6 3 7" with alice=0..3, bob=4..7;
    # after namespacing: CX alice.0 bob.0 alice.1 bob.1 ...
    # The parser splits each CX into a per-pair call, so labels appear:
    #   alice.0, bob.0, alice.1, bob.1, alice.2, bob.2, alice.3, bob.3
    assert pairs == [
        ("0", "1"),
        ("2", "3"),
        ("4", "5"),
        ("6", "7"),
    ]
