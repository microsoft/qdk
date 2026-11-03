# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""The default pass pipeline has to preserve what a program computes.

This is the half of the semantics gate with an oracle nobody here wrote. The
Rust half checks the cancellation rule against its own specification, which
covers the class of defect that prompted both, but it cannot tell that an
operation was wrongly called self-inverse: a hand-written matrix table on that
side could encode the same misconception as the pass and compare equal to
itself. Qiskit's ``Operator`` was written by people who were not implementing
this pass, which is the whole reason it is worth the dependency.

Comparisons are exact rather than up to global phase, matching the interop
converters and for the same reason: phase is what a rewrite most easily gets
wrong, and it becomes observable the moment a circuit is used as a controlled
subroutine.
"""

import itertools
import math
import warnings

import numpy as np
import pytest

qiskit = pytest.importorskip(
    "qiskit",
    reason="the semantics gate over the default pass pipeline needs an independent "
    "unitary oracle; install the qiskit extra to enable it",
)

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qiskit.exceptions import QiskitError  # noqa: E402
from qiskit.quantum_info import DensityMatrix, Kraus, Operator  # noqa: E402

from qdk.circuit import (
    PassAnchor,
    PassPatch,
    PassPipeline,
    QuantumCircuit,
)  # noqa: E402
from qdk.qiskit import to_qiskit  # noqa: E402


def built(
    width: int, gates: list[tuple[str, tuple[float, ...]]], clbits: int = 0
) -> QuantumCircuit:
    circuit = QuantumCircuit(width, clbits)
    for name, operands in gates:
        getattr(circuit, name)(*operands)
    return circuit


def assert_preserves(pipeline: PassPipeline, width: int, gates: list) -> None:
    """Runs one pipeline over one program and asserts the unitary is unchanged."""
    before = built(width, gates)
    after = pipeline.run(before).circuit

    expected = Operator(to_qiskit(before)).data
    actual = Operator(to_qiskit(after)).data
    assert np.allclose(expected, actual), (
        f"the pipeline changed what {gates} computes\n"
        f"expected:\n{np.round(expected, 6)}\nactual:\n{np.round(actual, 6)}"
    )


def assert_pipeline_preserves(width: int, gates: list) -> None:
    """Runs the default pipeline and asserts the unitary is unchanged."""
    assert_preserves(PassPipeline(), width, gates)


"""How much of the program a run removed, at a named stage of the progression.

Stages are named rather than assumed because rewrites are placed at more than
one point in the progression, and a baseline built for one stage and retrofitted
to another encodes the first stage's assumptions. `graph` counts port-graph
nodes as the last graph anchor left them; `circuit` counts instructions in the
circuit that came back out. The two are not the same measurement: a node with no
circuit instruction behind it, such as the constant carrying a retired gate's
angle, moves one and not the other. They agree across the corpora below today,
which is a fact about what the delivered rules retire rather than a property of
the stages.

The expectation is a count, not a node-identity assertion. A count cannot tell a
run that removed the right node from one that removed a different node of the
same size. That gap is covered rather than accepted: the unitary and dephased
state comparisons in this file see any substitution that is not
semantics-preserving, and a substitution that is semantics-preserving is not the
defect a size baseline is for.
"""
STAGES = ("graph", "circuit")


def sizes_at(stage: str, before, result) -> tuple[int, int]:
    """The program's size at a named stage, before and after a run."""
    if stage == "graph":
        return len(before.to_graph().nodes), len(result.graph.nodes)
    if stage == "circuit":
        return to_qiskit(before).size(), to_qiskit(result.circuit).size()
    raise AssertionError(f"no pipeline stage is named '{stage}'")


def assert_delta(pipeline, width, gates, expected: dict, clbits: int = 0) -> None:
    """Asserts the run removed exactly what the program declared, at each stage.

    Without this the corpora would be satisfied by a pass that does nothing, and
    every preservation assertion in this file would hold for the wrong reason.
    """
    before = built(width, gates, clbits)
    result = pipeline.run(before)
    for stage in STAGES:
        start, end = sizes_at(stage, before, result)
        assert start - end == expected[stage], (
            f"at stage '{stage}' the program {gates} lost {start - end} of a "
            f"declared {expected[stage]}, going from {start} to {end}"
        )


def assert_reported(pipeline, pass_name: str, width, gates, expected: dict, clbits=0):
    """Asserts the named pass reported a change exactly when one was declared.

    The report and the size are different claims, and a pass that edits without
    reporting, or reports without editing, would slip past either one alone.
    """
    result = pipeline.run(built(width, gates, clbits))
    changed = any(delta != 0 for delta in expected.values())
    assert (pass_name in result.changed) == changed, (
        f"'{pass_name}' reported {'no ' if changed else ''}change on {gates}, "
        f"and the declared deltas {expected} say otherwise"
    )


# Every operation the default pipeline names, with its arity.
SELF_INVERSE = [
    ("i", 1),
    ("x", 1),
    ("y", 1),
    ("z", 1),
    ("h", 1),
    ("cx", 2),
    ("cy", 2),
    ("cz", 2),
    ("swap", 2),
    ("ccx", 3),
    ("cswap", 3),
]

# Repeated but not self-inverse. If one of these were ever added to the
# pipeline's table, the pair would cancel and the unitary would change. The
# Rust half cannot catch that, because it checks the rule rather than physics.
NOT_SELF_INVERSE = [("s", 1), ("t", 1), ("sx", 1)]


@pytest.mark.parametrize(
    ("gate", "ordering"),
    [
        (gate, ordering)
        for gate, arity in SELF_INVERSE
        for ordering in itertools.permutations(range(arity))
    ],
    ids=lambda value: str(value),
)
def test_a_pair_in_any_arrangement_computes_the_same_thing_afterwards(gate, ordering):
    # Every arrangement rather than a chosen one. Whether the pipeline cancels a
    # given pair or leaves it alone, it must not change what the pair computes,
    # and stating it that way removes the need to know in advance which
    # permutations happen to be identities. Choosing arrangements by hand is how
    # the defect this gate exists for got through in the first place.
    aligned = tuple(range(len(ordering)))
    assert_pipeline_preserves(3, [(gate, aligned), (gate, tuple(ordering))])


@pytest.mark.parametrize(
    ("gate", "arity"), NOT_SELF_INVERSE, ids=lambda value: str(value)
)
def test_a_repeated_gate_that_is_not_its_own_inverse_survives(gate, arity):
    operands = tuple(range(arity))
    assert_pipeline_preserves(3, [(gate, operands), (gate, operands)])


"""The corpus a pass names when it declares `DifferentialCorpus`.

A named artifact rather than a list inside one assertion, because the
obligation is a claim about an arbitrary pass and the driver refuses a
declaration naming a corpus that is not this one. The name is paired with
`DIFFERENTIAL_CORPORA` in source/compiler/qsc_codegen/src/pass_pipeline.rs; the
two agree by a shared literal, since the oracle is Python and the driver is
Rust.
"""
CORPUS_NAME = "circuit-pipeline"

# Each program declares what a default run removes, per stage. A rotation-
# bearing program is here because 87 of the adjacent pairs measured in real
# compiler output are rotations, and because rotation merging is where a global
# phase most easily goes missing: this tier is the exact, global-phase-sensitive
# comparison, so a merge that dropped a phase would be caught here and nowhere
# else.
CORPUS = [
    (
        "a pair with something after it",
        1,
        [("h", (0,)), ("h", (0,)), ("x", (0,))],
        {"graph": 2, "circuit": 2},
    ),
    ("a bell pair", 2, [("h", (0,)), ("cx", (0, 1))], {"graph": 0, "circuit": 0}),
    (
        "cancelling and non-cancelling pairs together",
        3,
        [
            ("cx", (0, 1)),
            ("cx", (0, 1)),
            ("cx", (1, 2)),
            ("cx", (2, 1)),
            ("h", (0,)),
        ],
        {"graph": 2, "circuit": 2},
    ),
    (
        "a pair separated by a gate that shares a qubit",
        2,
        [("x", (0,)), ("cz", (0, 1)), ("x", (0,))],
        {"graph": 0, "circuit": 0},
    ),
    (
        "nested pairs",
        1,
        [("h", (0,)), ("x", (0,)), ("x", (0,)), ("h", (0,))],
        {"graph": 4, "circuit": 4},
    ),
    (
        "two same-axis rotations whose merge is global-phase sensitive",
        1,
        [("rz", (math.pi / 2, 0)), ("rz", (math.pi / 2, 0))],
        {"graph": 0, "circuit": 0},
    ),
    (
        "a rotation pair around a cancelling pair",
        2,
        [("rx", (0.25, 0)), ("h", (1,)), ("h", (1,)), ("rx", (-0.25, 0))],
        {"graph": 2, "circuit": 2},
    ),
]


def assert_corpus_preserves(pipeline: PassPipeline) -> None:
    """Runs a pipeline over the whole corpus. This is the CI half of the
    `DifferentialCorpus` discharge; the driver's half is refusing a declaration
    that names no corpus."""
    for _label, width, gates, _delta in CORPUS:
        assert_preserves(pipeline, width, gates)


@pytest.mark.parametrize(
    ("label", "width", "gates", "delta"), CORPUS, ids=[entry[0] for entry in CORPUS]
)
def test_the_pipeline_preserves_what_a_program_computes(label, width, gates, delta):
    assert_pipeline_preserves(width, gates)


@pytest.mark.parametrize(
    ("label", "width", "gates", "delta"), CORPUS, ids=[entry[0] for entry in CORPUS]
)
def test_the_corpus_declares_what_each_program_loses(label, width, gates, delta):
    assert_delta(PassPipeline(), width, gates, delta)


@pytest.mark.parametrize(
    ("label", "width", "gates", "delta"),
    [entry for entry in CORPUS if any(entry[3].values())],
    ids=[entry[0] for entry in CORPUS if any(entry[3].values())],
)
def test_a_pipeline_that_does_nothing_fails_a_program_that_declared_a_loss(
    label, width, gates, delta
):
    # The guard the whole baseline rests on. A corpus that only asserted
    # preservation would be passed by a pipeline with every rule removed.
    with pytest.raises(AssertionError, match="of a declared"):
        assert_delta(PassPipeline(built_ins=False), width, gates, delta)


def test_a_delta_failure_names_the_stage_it_was_measured_at():
    # A baseline that reported a number without saying where it measured it
    # would be unreadable the moment rewrites run at more than one stage.
    with pytest.raises(AssertionError, match="at stage 'graph'"):
        assert_delta(
            PassPipeline(built_ins=False),
            1,
            [("h", (0,)), ("h", (0,)), ("x", (0,))],
            {"graph": 2, "circuit": 2},
        )


def test_a_stage_the_harness_does_not_measure_is_refused_rather_than_skipped():
    program = built(1, [("h", (0,))])
    with pytest.raises(AssertionError, match="no pipeline stage is named 'qir'"):
        sizes_at("qir", program, PassPipeline().run(program))


def test_the_exact_tier_refuses_a_program_that_measures():
    # The scope limit of this tier, stated as a check rather than as a comment.
    # A measuring program has no matrix here, and the exact corpus must refuse
    # one rather than quietly acquire a program it cannot compare.
    with pytest.raises(QiskitError, match="classical bits"):
        Operator(to_qiskit(built(1, [("h", (0,)), ("measure", (0, 0))], 1)))


def test_the_two_stages_are_different_measurements_of_the_same_program():
    # Naming the stage is not decoration. The graph carries nodes no circuit
    # instruction stands behind, including the constant carrying a gate's angle,
    # so a rewrite that retired one would move one number and not the other.
    program = built(1, [("rx", (0.25, 0))])
    result = PassPipeline().run(program)
    at_graph, _ = sizes_at("graph", program, result)
    at_circuit, _ = sizes_at("circuit", program, result)
    assert at_graph > at_circuit, (
        f"the graph should carry more than the {at_circuit} instruction(s) the "
        f"circuit does, and it carries {at_graph}"
    )


def test_the_corpus_catches_a_pass_that_changes_what_a_program_computes():
    # The reason the corpus exists. A pass that swaps X for Z declares nothing
    # false and breaks nothing structural; only running programs through it
    # finds the problem.
    pipeline = PassPipeline(built_ins=False)
    pipeline.add_rewrite_pass(
        anchor=PassAnchor.GraphRewritten,
        name="unsound",
        operations=["quantum.x"],
        pass_=lambda match: PassPatch.replace("quantum.z"),
    )

    with pytest.raises(AssertionError, match="changed what"):
        assert_corpus_preserves(pipeline)


def test_the_corpus_passes_for_the_pipeline_that_declares_it():
    assert_corpus_preserves(PassPipeline())


def test_declining_the_built_ins_changes_nothing_either():
    # The pipeline itself, with nothing registered, must be an identity on the
    # program. If the progression through the graph and back lost something,
    # every other test here would be measuring the wrong thing.
    gates = [("h", (0,)), ("cx", (0, 1)), ("t", (1,))]
    before = built(2, gates)
    after = PassPipeline(built_ins=False).run(before).circuit

    assert np.allclose(
        Operator(to_qiskit(before)).data, Operator(to_qiskit(after)).data
    ), "the progression alone changed the program"


"""The second oracle tier, for programs the first one cannot hold.

Nothing in Qiskit's ``quantum_info`` accepts a measurement with its classical
destination — checked for ``Operator``, ``SuperOp``, ``Choi``, ``Kraus``,
``DensityMatrix``, and ``Statevector`` — so a program that measures has no
matrix to compare and the tier above is silent about it.

The obvious repair is wrong in a way worth recording. Stripping the
measurements and comparing the remaining unitary would *reject* a sound
rewrite: a Z before a Z measurement changes the pre-measurement unitary and
changes nothing anyone can observe. An oracle that fails a sound rewrite is
worse than no oracle.

So this tier compares the whole post-run state. It strips the terminal
measurements, evolves what is left to a density matrix, applies complete
computational-basis dephasing to every qubit that was measured, and compares
exactly. Comparing only the measured qubits' outcome distribution would have
been strictly weaker and blind in a specific place: ``h(0); h(1); cz(0,1);
measure(0)`` has the same marginal on qubit 0 with and without the ``cz``,
while qubit 1's conditional state flips between ``|+>`` and ``|->``. That is
exactly the difference the rewrite's own operand condition exists to forbid,
so an oracle that could not see it could not witness the condition it is
supposed to be checking.

Two limits are inherent rather than accidental. A dephased state cannot see a
dropped global phase, which is correct here — a global phase is unobservable in
a standalone terminal-measurement program — and is why the exact ``Operator``
tier above stays as it is rather than being replaced. And the comparison reaches
Z-basis-terminal programs only: the converter expands ``measure_x`` and
``measure_y`` into a rotation around a Z measurement, so those programs have no
terminal measurement to strip and are covered structurally instead.

The name is paired with ``DIFFERENTIAL_CORPORA`` in
source/compiler/qsc_codegen/src/pass_pipeline.rs; the two agree by a shared
literal, since the oracle is Python and the driver is Rust.
"""
MEASURED_CORPUS_NAME = "circuit-pipeline-measured"

# Complete dephasing in the computational basis, as the two projectors. This is
# what a measurement does to the state once its outcome is recorded and no
# longer coherent with anything.
_COLLAPSE = Kraus(
    [np.array([[1.0, 0.0], [0.0, 0.0]]), np.array([[0.0, 0.0], [0.0, 1.0]])]
)

# The pass this tier exists to check. Named rather than inlined so a rename
# shows up as one failure here instead of as a corpus that silently stops
# testing anything.
COLLAPSE_PASS = "retire-diagonal-before-z-collapse"


def measurement_layout(circuit) -> list:
    """The basis, qubit, and classical bit of each measurement, and the program
    before them.

    Terminal is checked rather than assumed. A measurement with anything after
    it on the same qubit cannot be moved to the end, so stripping it would
    describe a different program than the one that ran. A basis measurement
    fails here too, and that is the intended report: the converter expands
    ``measure_x`` into ``h; measure; h``, so the trailing rotation is an
    operation on an already-measured qubit.
    """
    prefix = circuit.copy_empty_like()
    layout = []
    measured: set[int] = set()
    for instruction in circuit.data:
        qubits = tuple(circuit.find_bit(bit).index for bit in instruction.qubits)
        assert not measured.intersection(qubits), (
            f"'{instruction.operation.name}' acts on an already-measured qubit; "
            "this tier compares programs whose measurements are terminal, and a "
            "basis measurement is expanded into a rotation around a Z one"
        )
        if instruction.operation.name == "measure":
            clbits = tuple(circuit.find_bit(bit).index for bit in instruction.clbits)
            layout.append(("measure_z", qubits, clbits))
            measured.update(qubits)
            continue
        prefix.append(instruction)
    # Sorted, because two independent measurements are not ordered with respect
    # to each other and a reordering is not a change to the program.
    return prefix, sorted(layout)


def dephased_state(circuit):
    """The whole post-run state: the program before its measurements, evolved,
    then collapsed in the computational basis wherever it was measured."""
    prefix, layout = measurement_layout(circuit)
    state = DensityMatrix.from_instruction(prefix)
    for _basis, qubits, _clbits in layout:
        for qubit in qubits:
            state = state.evolve(_COLLAPSE, qargs=[qubit])
    return state.data, layout


def assert_measured_preserves(pipeline, width, clbits, gates) -> None:
    """Runs one pipeline over one measuring program and asserts the whole
    post-run state and the measurement layout are unchanged."""
    before = built(width, gates, clbits)
    after = pipeline.run(before).circuit

    expected, expected_layout = dephased_state(to_qiskit(before))
    actual, actual_layout = dephased_state(to_qiskit(after))
    assert actual_layout == expected_layout, (
        f"the pipeline moved a measurement in {gates}\n"
        f"expected: {expected_layout}\nactual: {actual_layout}"
    )
    assert np.allclose(expected, actual), (
        f"the pipeline changed what {gates} computes\n"
        f"expected:\n{np.round(expected, 6)}\nactual:\n{np.round(actual, 6)}"
    )


# Programs whose whole post-run state this tier compares. Each declares what a
# default run removes, per stage, so a rewrite that stops firing is a failure
# rather than a silently weaker corpus, and a rewrite that starts removing more
# than it did is not mistaken for an improvement.
MEASURED_CORPUS = [
    (
        "a z before a z measurement",
        1,
        1,
        [("z", (0,)), ("measure", (0, 0))],
        {"graph": 1, "circuit": 1},
    ),
    (
        "an s and a t before a z measurement",
        1,
        1,
        [("h", (0,)), ("s", (0,)), ("t", (0,)), ("measure", (0, 0))],
        {"graph": 2, "circuit": 2},
    ),
    (
        "a cz whose operands are both z-measured",
        2,
        2,
        [
            ("h", (0,)),
            ("h", (1,)),
            ("cz", (0, 1)),
            ("measure", (0, 0)),
            ("measure", (1, 1)),
        ],
        {"graph": 1, "circuit": 1},
    ),
    (
        "a diagonal gate before a reset",
        1,
        1,
        [("h", (0,)), ("t", (0,)), ("reset", (0,)), ("measure", (0, 0))],
        {"graph": 1, "circuit": 1},
    ),
    (
        "a cz reaching a reset on one operand and a measurement on the other",
        2,
        1,
        [
            ("h", (0,)),
            ("h", (1,)),
            ("cz", (0, 1)),
            ("reset", (0,)),
            ("measure", (1, 0)),
        ],
        {"graph": 1, "circuit": 1},
    ),
    (
        "a cz with only one operand measured",
        2,
        1,
        [("h", (0,)), ("h", (1,)), ("cz", (0, 1)), ("measure", (0, 0))],
        {"graph": 0, "circuit": 0},
    ),
    (
        "a gate between the diagonal gate and the measurement",
        1,
        1,
        [("z", (0,)), ("h", (0,)), ("measure", (0, 0))],
        {"graph": 0, "circuit": 0},
    ),
    (
        "a non-diagonal gate before a measurement",
        1,
        1,
        [("x", (0,)), ("measure", (0, 0))],
        {"graph": 0, "circuit": 0},
    ),
    (
        "a diagonal gate reaching nothing but the release",
        1,
        0,
        [("t", (0,))],
        {"graph": 0, "circuit": 0},
    ),
    (
        "a rotation about z before a z measurement",
        1,
        1,
        [("h", (0,)), ("rz", (0.375, 0)), ("measure", (0, 0))],
        {"graph": 1, "circuit": 1},
    ),
    (
        "two same-axis rotations before a z measurement",
        1,
        1,
        [("h", (0,)), ("rx", (0.25, 0)), ("rx", (0.5, 0)), ("measure", (0, 0))],
        {"graph": 0, "circuit": 0},
    ),
]

# Programs the state comparison cannot reach, because the converter leaves them
# with no terminal measurement to strip. Their semantic argument lives in the
# Rust decline tests; what is checked here is that the pipeline leaves them
# alone, which is the observable half.
UNREACHED_CORPUS = [
    (
        "a z before an x-basis measurement",
        1,
        1,
        [("h", (0,)), ("z", (0,)), ("measure_x", (0, 0))],
        {"graph": 0, "circuit": 0},
    ),
    (
        "a cz with one operand z-measured and the other x-measured",
        2,
        2,
        [
            ("h", (0,)),
            ("h", (1,)),
            ("cz", (0, 1)),
            ("measure", (0, 0)),
            ("measure_x", (1, 1)),
        ],
        {"graph": 0, "circuit": 0},
    ),
]


def assert_measured_corpus_preserves(pipeline: PassPipeline) -> None:
    """Runs a pipeline over the whole measuring corpus, both tiers of it."""
    for _label, width, clbits, gates, _delta in MEASURED_CORPUS:
        assert_measured_preserves(pipeline, width, clbits, gates)
    for _label, width, clbits, gates, delta in UNREACHED_CORPUS:
        assert_unchanged_in_size(pipeline, width, clbits, gates, delta)


def assert_unchanged_in_size(pipeline, width, clbits, gates, delta) -> None:
    """Asserts the collapse pass declined and the program did not shrink.

    Both halves are needed. The report says whether a pass claimed a change and
    the size says whether one happened, and a pass that edits without reporting
    would slip past either one alone.
    """
    assert_reported(pipeline, COLLAPSE_PASS, width, gates, delta, clbits)
    assert_delta(pipeline, width, gates, delta, clbits)


@pytest.mark.parametrize(
    ("label", "width", "clbits", "gates", "delta"),
    MEASURED_CORPUS,
    ids=[entry[0] for entry in MEASURED_CORPUS],
)
def test_the_pipeline_preserves_what_a_measuring_program_computes(
    label, width, clbits, gates, delta
):
    assert_measured_preserves(PassPipeline(), width, clbits, gates)


@pytest.mark.parametrize(
    ("label", "width", "clbits", "gates", "delta"),
    MEASURED_CORPUS,
    ids=[entry[0] for entry in MEASURED_CORPUS],
)
def test_the_measured_corpus_declares_what_each_program_loses(
    label, width, clbits, gates, delta
):
    # Without this the corpus would be satisfied by a pass that does nothing,
    # and the preservation assertions above would all hold for the wrong
    # reason. It is the same guard the counted-comparison idiom serves
    # elsewhere in this repository, with a count in place of a boolean so that
    # removing one node is distinguishable from removing thirty.
    assert_reported(PassPipeline(), COLLAPSE_PASS, width, gates, delta, clbits)
    assert_delta(PassPipeline(), width, gates, delta, clbits)


@pytest.mark.parametrize(
    ("label", "width", "clbits", "gates", "delta"),
    [entry for entry in MEASURED_CORPUS if any(entry[4].values())],
    ids=[entry[0] for entry in MEASURED_CORPUS if any(entry[4].values())],
)
def test_a_measuring_program_that_declared_a_loss_fails_a_pipeline_that_does_nothing(
    label, width, clbits, gates, delta
):
    with pytest.raises(AssertionError, match="of a declared"):
        assert_delta(PassPipeline(built_ins=False), width, gates, delta, clbits)


@pytest.mark.parametrize(
    ("label", "width", "clbits", "gates", "delta"),
    [entry for entry in MEASURED_CORPUS if not any(entry[4].values())],
    ids=[entry[0] for entry in MEASURED_CORPUS if not any(entry[4].values())],
)
def test_the_collapse_pass_declines_what_it_must_decline(
    label, width, clbits, gates, delta
):
    assert_unchanged_in_size(PassPipeline(), width, clbits, gates, delta)


@pytest.mark.parametrize(
    ("label", "width", "clbits", "gates", "delta"),
    UNREACHED_CORPUS,
    ids=[entry[0] for entry in UNREACHED_CORPUS],
)
def test_the_collapse_pass_declines_a_basis_measurement(
    label, width, clbits, gates, delta
):
    assert_unchanged_in_size(PassPipeline(), width, clbits, gates, delta)


def test_the_measured_tier_separates_an_observable_difference_from_an_unobservable_one():
    # The two directions, and only the second shows the oracle measures
    # anything. Both arrangements come from a probe rather than from reasoning:
    # `h; z` and `h` give identical Z-basis probabilities, while `h; z; h` and
    # `h; h` give opposite certain outcomes.
    unobservable = [("h", (0,)), ("measure", (0, 0))]
    same, _ = dephased_state(to_qiskit(built(1, unobservable, 1)))
    with_phase, _ = dephased_state(
        to_qiskit(built(1, [("h", (0,)), ("z", (0,)), ("measure", (0, 0))], 1))
    )
    assert np.allclose(
        same, with_phase
    ), "a computational-basis phase before a Z measurement is not observable"

    observable, _ = dephased_state(
        to_qiskit(built(1, [("h", (0,)), ("h", (0,)), ("measure", (0, 0))], 1))
    )
    flipped, _ = dephased_state(
        to_qiskit(
            built(1, [("h", (0,)), ("z", (0,)), ("h", (0,)), ("measure", (0, 0))], 1)
        )
    )
    assert not np.allclose(observable, flipped), (
        "the same phase with a basis change after it inverts the outcome, and "
        "an oracle that cannot see that is measuring nothing"
    )


def test_the_exact_tier_sees_a_merge_that_drops_a_global_phase():
    # Why a rotation program belongs in the exact corpus rather than only in
    # the measuring one. Same-axis rotations merge exactly, phase included, and
    # the tempting shorter form is the same operator times -i. This tier is the
    # only one in this file that can tell those apart.
    paired = Operator(
        to_qiskit(built(1, [("rz", (math.pi / 2, 0)), ("rz", (math.pi / 2, 0))]))
    ).data
    exact = Operator(to_qiskit(built(1, [("rz", (math.pi, 0))]))).data
    dropped = Operator(to_qiskit(built(1, [("z", (0,))]))).data

    assert np.allclose(paired, exact), "same-axis rotations merge exactly"
    assert not np.allclose(paired, dropped), "and the shorter form is not the same"
    assert np.allclose(
        paired, -1j * np.asarray(dropped)
    ), "it differs by exactly a global phase"

    # And the measuring tier is blind to it, which is the whole argument for
    # keeping both corpora rather than merging them.
    with_pair, _ = dephased_state(
        to_qiskit(
            built(
                1,
                [
                    ("h", (0,)),
                    ("rz", (math.pi / 2, 0)),
                    ("rz", (math.pi / 2, 0)),
                    ("measure", (0, 0)),
                ],
                1,
            )
        )
    )
    with_dropped, _ = dephased_state(
        to_qiskit(built(1, [("h", (0,)), ("z", (0,)), ("measure", (0, 0))], 1))
    )
    assert np.allclose(with_pair, with_dropped), (
        "a dephased state cannot see a dropped global phase, so the exact "
        "corpus is where a phase-sensitive program has to live"
    )


def test_the_measured_tier_sees_past_the_measured_qubits_marginal():
    # The reason this tier compares the whole state. An oracle built on the
    # measured qubits' outcome distribution would report these two equivalent,
    # and the difference it would miss is exactly the one the rewrite's operand
    # condition exists to forbid.
    entangling = [("h", (0,)), ("h", (1,)), ("cz", (0, 1)), ("measure", (0, 0))]
    without = [("h", (0,)), ("h", (1,)), ("measure", (0, 0))]

    with_cz, _ = dephased_state(to_qiskit(built(2, entangling, 1)))
    plain, _ = dephased_state(to_qiskit(built(2, without, 1)))

    marginal = DensityMatrix(with_cz).probabilities([0])
    assert np.allclose(marginal, DensityMatrix(plain).probabilities([0])), (
        "the measured qubit's marginal is the same either way, which is the "
        "blind spot"
    )
    assert not np.allclose(with_cz, plain), (
        "the unmeasured operand's conditional state is not the same, and the "
        "whole-state comparison is what sees it"
    )


def test_the_layout_assertion_sees_a_measurement_that_moved():
    # A state comparison alone would not notice a measurement recorded into a
    # different bit or taken on a different qubit, because neither changes the
    # dephased state.
    _, into_zero = dephased_state(
        to_qiskit(built(1, [("h", (0,)), ("measure", (0, 0))], 2))
    )
    _, into_one = dephased_state(
        to_qiskit(built(1, [("h", (0,)), ("measure", (0, 1))], 2))
    )
    assert into_zero != into_one, "the classical destination is part of the layout"

    _, on_first = dephased_state(
        to_qiskit(built(2, [("h", (0,)), ("h", (1,)), ("measure", (0, 0))], 1))
    )
    _, on_second = dephased_state(
        to_qiskit(built(2, [("h", (0,)), ("h", (1,)), ("measure", (1, 0))], 1))
    )
    assert on_first != on_second, "the measured qubit is part of the layout"


def test_a_basis_measurement_is_refused_rather_than_silently_compared():
    # The scope limit, stated as a check rather than as a comment. A basis
    # measurement expands into a rotation around a Z one, so it has no terminal
    # measurement, and the tier says so instead of comparing the wrong thing.
    with pytest.raises(AssertionError, match="already-measured qubit"):
        dephased_state(to_qiskit(built(1, [("measure_x", (0, 0))], 1)))


def test_the_measured_corpus_passes_for_the_pipeline_that_declares_it():
    assert_measured_corpus_preserves(PassPipeline())


def test_the_measured_corpus_catches_a_pass_that_changes_a_measuring_program():
    # The reason this corpus exists, in the same shape the unitary corpus uses.
    # A pass that swaps H for Z declares nothing false and breaks nothing
    # structural, and only running programs through it finds the problem.
    pipeline = PassPipeline(built_ins=False)
    pipeline.add_rewrite_pass(
        anchor=PassAnchor.GraphRewritten,
        name="unsound",
        operations=["quantum.h"],
        pass_=lambda match: PassPatch.replace("quantum.z"),
    )

    with pytest.raises(AssertionError, match="changed what"):
        assert_measured_corpus_preserves(pipeline)


if __name__ == "__main__":
    pytest.main([__file__])
