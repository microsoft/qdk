"""Tests for objective action profiling."""
from __future__ import annotations

import qodec
from qdk.ec.action import lift_objective, logical_action_of
from ec_tests.testing.qodecs import c4


def _swap_idle_objective(
    *,
    mnemonic: str,
    actions: list[qodec.Action],
    flags: list[str] | None = None,
) -> qodec.Instruction:
    """Build a single instruction matching the shape of `c4()`'s ``idle``
    (one input/output ``c4`` block, two logical qubits) but carrying
    ``actions`` instead. Returns the objective `Instruction`; the gadget
    body it is paired with supplies the realisation.
    """
    block_op = qodec.instructions.BlockOperand("c4")
    return qodec.Instruction(
        mnemonic=mnemonic,
        inputs=[block_op], outputs=[block_op],
        flags=list(flags) if flags else [],
        action=list(actions),
    )


def _bogus_gadget(
    base: qodec.Gadget,
    objective: qodec.Instruction,
    *,
    readouts: list[object] | None = None,
) -> qodec.Gadget:
    """Build a gadget that reuses ``base``'s realisation (circuit + boundary
    encodings + checks) but swaps in a custom implemented instruction."""
    return qodec.Gadget(
        implements=objective,
        circuit=base.circuit,
        inputs=list(base.inputs),
        outputs=list(base.outputs),
        checks=[list(check) for check in base.checks],
        readouts=readouts if readouts is not None else [list(r) for r in base.readouts],
    )


def test_lift_objective_happy_path_for_measure_zz() -> None:
    """`measure_zz` declares two Pauli observables; the lift should
    produce an expected `LogicalAction` and no missing/unsupported
    annotations."""
    codec = c4()
    gadget = codec.layers[0].gadgets["measure_zz"]
    lift = lift_objective(gadget)
    assert lift.expected is not None
    assert lift.missing_observables == ()
    assert lift.unsupported_atoms == ()
    # `measure_zz` declares no flags.
    assert lift.bound_flags == ()


def test_lift_objective_flags_prepare_zz_reject() -> None:
    """`prepare_zz` declares a flag named ``reject`` that the realisation binds."""
    codec = c4()
    gadget = codec.layers[0].gadgets["prepare_zz"]
    lift = lift_objective(gadget)
    assert "reject" in lift.bound_flags


def test_lift_objective_reports_missing_observable() -> None:
    """If the realisation drops an observable the objective declares,
    the lift records it under `missing_observables`."""
    codec = c4()
    measure_zz = codec.layers[0].gadgets["measure_zz"]
    bogus = qodec.Gadget(
        implements=measure_zz.implements,
        circuit=measure_zz.circuit,
        inputs=list(measure_zz.inputs),
        checks=[list(check) for check in measure_zz.checks],
        readouts=[],  # drop both positional observables
    )
    lift = lift_objective(bogus)
    # Observables are positional: the two missing observe outcomes are 0 and 1.
    assert set(lift.missing_observables) == {"0", "1"}
    assert lift.expected is None  # lift fails when observables go missing


def test_lift_objective_clean_on_idle() -> None:
    """`idle` has no objective action atoms; the lift produces an
    identity-shaped expected action with no flags or unsupported atoms."""
    codec = c4()
    gadget = codec.layers[0].gadgets["idle"]
    lift = lift_objective(gadget)
    assert lift.expected is not None
    assert lift.missing_observables == ()
    assert lift.unsupported_atoms == ()
    assert lift.bound_flags == ()


def test_lift_objective_records_unsupported_atom() -> None:
    """A `Rotate` atom (out of stabiliser scope) is reported in
    `unsupported_atoms` and lift returns no expected action."""
    codec = c4()
    measure_zz = codec.layers[0].gadgets["measure_zz"]
    bogus_objective = qodec.Instruction(
        mnemonic="rotated",
        inputs=[qodec.instructions.BlockOperand("c4")],
        action=[
            qodec.actions.Rotate("Z_0 Z_1", angle=0.5),
        ],
    )
    bogus = qodec.Gadget(
        implements=bogus_objective,
        circuit=measure_zz.circuit,
        inputs=list(measure_zz.inputs),
        checks=[list(check) for check in measure_zz.checks],
    )
    lift = lift_objective(bogus)
    assert "Rotate" in lift.unsupported_atoms
    assert lift.expected is None


def test_lift_objective_identity_clifford_matches_idle() -> None:
    """An identity `Clifford` (empty generators dict relying on the
    implicit identity) on the `idle` realisation lifts to the same
    `LogicalAction` as the realisation actually produces."""
    codec = c4()
    idle = codec.layers[0].gadgets["idle"]
    objective = _swap_idle_objective(
        mnemonic="id_clifford",
        actions=[qodec.actions.Clifford({})],
    )
    bogus = _bogus_gadget(idle, objective)
    lift = lift_objective(bogus)
    assert lift.expected is not None
    assert lift.unsupported_atoms == ()
    assert lift.expected == logical_action_of(bogus)


def test_lift_objective_non_trivial_clifford_composes() -> None:
    """A `Clifford` that swaps the two logical qubits of the `c4` block
    (X̄_0 ↔ X̄_1, Z̄_0 ↔ Z̄_1) lifts to the expected permutation of the
    flat image table — independently of the realisation's behaviour.
    """
    codec = c4()
    idle = codec.layers[0].gadgets["idle"]
    objective = _swap_idle_objective(
        mnemonic="swap_ls",
        actions=[qodec.actions.Clifford({
            "X_0": "X_1",
            "X_1": "X_0",
            "Z_0": "Z_1",
            "Z_1": "Z_0",
        })],
    )
    bogus = _bogus_gadget(idle, objective)
    lift = lift_objective(bogus)
    assert lift.expected is not None
    assert lift.unsupported_atoms == ()
    # Flat input ordering is (X̄_0, Z̄_0, X̄_1, Z̄_1); swap L↔S permutes
    # X̄_0↔X̄_1 (rows 0↔2) and Z̄_0↔Z̄_1 (rows 1↔3).
    images = lift.expected.images
    assert images[0].output_logical_flips == frozenset({3})
    assert images[1].output_logical_flips == frozenset({2})
    assert images[2].output_logical_flips == frozenset({1})
    assert images[3].output_logical_flips == frozenset({0})
    for image in images:
        assert image.observable_flips == frozenset()


def test_lift_objective_clifford_composition_order() -> None:
    """Two `Clifford` atoms compose left-to-right (sequential
    application). Applying the same L↔S swap twice yields identity.
    """
    codec = c4()
    idle = codec.layers[0].gadgets["idle"]
    swap = qodec.actions.Clifford({
        "X_0": "X_1",
        "X_1": "X_0",
        "Z_0": "Z_1",
        "Z_1": "Z_0",
    })
    objective = _swap_idle_objective(
        mnemonic="swap_twice", actions=[swap, swap],
    )
    bogus = _bogus_gadget(idle, objective)
    lift = lift_objective(bogus)
    assert lift.expected is not None
    assert lift.expected == logical_action_of(idle)


def test_lift_objective_unconditional_pauli_is_no_op() -> None:
    """An unconditional `Pauli` only changes signs, which `LogicalAction`
    does not track. The lift treats it as identity and reports no
    unsupported atoms."""
    codec = c4()
    idle = codec.layers[0].gadgets["idle"]
    objective = _swap_idle_objective(
        mnemonic="pauli_kick",
        actions=[qodec.actions.Pauli("X_0")],
    )
    bogus = _bogus_gadget(idle, objective)
    lift = lift_objective(bogus)
    assert lift.expected is not None
    assert lift.unsupported_atoms == ()
    assert lift.expected == logical_action_of(idle)


def test_lift_objective_conditional_clifford_unsupported() -> None:
    """A `Clifford` carrying a non-``None`` ``condition`` (feedforward
    Pauli correction) is reported in ``unsupported_atoms`` and the lift
    returns no expected action."""
    codec = c4()
    idle = codec.layers[0].gadgets["idle"]
    objective = _swap_idle_objective(
        mnemonic="cond_clifford",
        flags=["flag"],
        actions=[qodec.actions.Clifford(
            {"X_0": "X_1"},
            condition=qodec.actions.Condition(["flag"]),
        )],
    )
    bogus = _bogus_gadget(
        idle, objective,
        readouts=[{"flag": ["circuit.readouts[0]"]}],
    )
    lift = lift_objective(bogus)
    assert "Clifford" in lift.unsupported_atoms
    assert lift.expected is None


def test_lift_objective_conditional_pauli_unsupported() -> None:
    """A `Pauli` carrying a non-``None`` ``condition`` is reported in
    ``unsupported_atoms`` and the lift returns no expected action."""
    codec = c4()
    idle = codec.layers[0].gadgets["idle"]
    objective = _swap_idle_objective(
        mnemonic="cond_pauli",
        flags=["flag"],
        actions=[qodec.actions.Pauli(
            "X_0",
            condition=qodec.actions.Condition(["flag"]),
        )],
    )
    bogus = _bogus_gadget(
        idle, objective,
        readouts=[{"flag": ["circuit.readouts[0]"]}],
    )
    lift = lift_objective(bogus)
    assert "Pauli" in lift.unsupported_atoms
    assert lift.expected is None
