"""Lazy, snapshot-based semantic profiles for gadgets and bare circuits."""

from __future__ import annotations

from functools import cached_property
from itertools import product
from typing import TYPE_CHECKING, Sequence

import qodec as qc
from qodec.gadgets import Circuit

from ._analysis.check_discovery import checks_of, profile_of
from ._analysis.channel_action import (
    ChannelAction,
    _identity_codes_over,
    action_of,
    declared_action_of,
    input_qubits_of,
    realized_action_of,
    realized_codes_of,
)
from ._analysis.equivalence import gadgets_equivalent, why_not_equivalent
from ._analysis.propagation.frames import FrameGroup, PauliFrame
from ._analysis.propagation.interpreter import propagate_faults
from ._analysis.propagation.pauli import Pauli, PauliCharacter
from ._layout import ProgramLayout
from ._readouts import observe_count_of
from ._references import outcomes_of
from ._checks import OutcomeCode, outcome_code_of
from ._faults import FaultEffect, FaultEvent, _probe_flips, fault_effects_of
from ._distance_result import Distance

if TYPE_CHECKING:
    from ._analysis.distance_solvers import BoundsSolver, ExactSolver
    from ._distance import _FaultDistanceData


class GadgetProfile:
    """What exact simulation says a gadget or bare circuit does.

    A bare :class:`qodec.gadgets.Circuit` is treated as a gadget whose inputs
    and outputs are identity-encoded on the qubits it does not prepare, so it
    has an action, checks, readouts, and fault effects like any other. Only
    :attr:`objective` is undefined there, because a circuit implements no
    instruction and deriving one from the circuit would make the comparison
    vacuous.

    Members are computed on first access and cached, but do not share one
    simulation. The target is snapshotted at construction, so a profile
    describes the gadget as it was then.
    """

    def __init__(self, target: qc.Gadget | Circuit) -> None:
        if not isinstance(target, (qc.Gadget, Circuit)):
            raise TypeError(
                "expected qodec.Gadget or qodec.gadgets.Circuit, got "
                f"{type(target).__name__}"
            )
        self._target = _snapshot(target)

    @cached_property
    def action(self) -> ChannelAction:
        """The circuit's logical action, including declared output-frame corrections."""
        if isinstance(self._target, qc.Gadget):
            return realized_action_of(self._target)
        return action_of(self._target)

    @cached_property
    def objective(self) -> ChannelAction | None:
        """What the implemented instruction demands, or ``None`` for a circuit.

        ``objective`` names the concept here, not the retired
        ``gadget.objective`` field that proposal 0025 replaced with
        ``gadget.implements``.
        """
        if isinstance(self._target, qc.Gadget):
            return declared_action_of(self._target)
        return None

    @cached_property
    def checks(self) -> tuple[frozenset[int], ...]:
        """One parity per check, over positions in the measurement record.

        The full discovered set, not the essential reduction.
        """
        if isinstance(self._target, qc.Gadget):
            return tuple(
                frozenset(outcomes_of(equation)) for equation in checks_of(self._target)
            )
        return tuple(self._outcome_code.checks())

    @cached_property
    def readouts(self) -> tuple[frozenset[int], ...]:
        """One parity per readout, over positions in the measurement record.

        For a gadget these are ``gadget.readouts`` in order: observe outcomes
        first, then flags. For a bare circuit, whose readouts are the
        measurements themselves, each record position is its own readout.
        """
        if isinstance(self._target, qc.Gadget):
            discovered = profile_of(self._target).readouts
            names = [
                *(
                    str(index)
                    for index in range(observe_count_of(self._target.implements))
                ),
                *self._target.implements.flags,
            ]
            return tuple(frozenset(discovered[name]) for name in names)
        return tuple(
            frozenset({position})
            for position in range(self._outcome_code.measurement_count)
        )

    @cached_property
    def fault_effects(self) -> tuple[tuple[FaultEvent, FaultEffect], ...]:
        """Effects over the canonical fault basis, paired with their cause.

        The canonical basis is one X and one Z fault after every instruction on
        every qubit it touches, plus one recorded-bit flip per circuit readout.
        Products span the Pauli/readout fault model, and effects are linear over
        GF(2). This compact propagation basis is not the unit-cost circuit fault
        set used by distance and distance_bounds.
        """
        basis = self._canonical_fault_basis()
        return tuple(zip(basis, self.effects_of(basis)))

    def effects_of(self, faults: Sequence[FaultEvent]) -> tuple[FaultEffect, ...]:
        """Effects of an explicit fault basis, positionally aligned with it.

        Plural because the whole basis is evaluated in one simulation.
        For gadgets, evaluate the complete declared checks and readouts,
        including output signs and uniquely defined readout dependencies.
        Incoming signs have zero change for circuit-internal faults.
        FaultEvent.after(..., readout_flips=...) indexes the selected call's
        own readouts, starting at zero, whereas
        FaultEffect.readout_flips indexes the gadget's declared readouts,
        including logical measurements and flags.
        Recorded-bit flips do not themselves change the surviving quantum state.
        Invalid references or ambiguous readouts raise ValueError.
        Conditional/selected calls and circuit instruction flags are unsupported.
        """
        if isinstance(self._target, qc.Gadget):
            return fault_effects_of(self._target, faults)
        return self._circuit_fault_data(tuple(faults))[0]

    def distance(
        self,
        *,
        faults: Sequence[FaultEvent] | None = None,
        upper_bound: int | None = None,
        solver: ExactSolver | None = None,
    ) -> Distance[FaultEvent]:
        """Minimum fault count causing logical failure with all checks and flags zero.

        By default, allow every combination of a post-call Pauli on a call's
        qubits and flips of its recorded readout bits, except the identity event.
        A call with n qubits and r readouts contributes 4**n * 2**r - 1 events.
        Calls without readouts retain 3 one-qubit or 15 two-qubit Pauli faults.
        Each event costs one, including correlated quantum/readout errors at
        one call; this is not FaultEvent.weight. A pure readout flip changes
        only the reported bit, not the quantum state after measurement.
        Pass an explicit sequence to replace that fault set, including [].
        An explicit event may span several call positions and still costs one.

        The combined fault must leave every declared check and flag zero and
        commute with every output-code stabilizer: a nonzero output
        syndrome is not a logical error. Failure means changing the realized
        logical action: its prepared-state stabilizers, preserved logical
        mappings, or logical measurement signs. A logical Z on a prepared
        logical zero is harmless. Output errors and measurement-dependent
        signs are evaluated together and may cancel. Individual factors may leave
        the codespace as long as their combined output syndromes cancel.
        These constraints do not add declared checks or alter FaultEffect.syndrome.
        Individual factors may raise flags as long as their combined flag flips
        cancel. A flag alone is not a logical failure. A bare circuit
        uses its discovered checks and identity encodings on the qubits it
        does not prepare. No decoder or additional output recovery is assumed.

        No logical measurement is required: a readout-free gadget is assessed
        through its output encodings. Audit noiseless validity separately with
        ec.audit(protocol). Distance retains only calculation preconditions;
        the action must be interpretable against the boundary codes, and
        declared but unbound logical readouts or flags cannot silently be ignored.

        The result's witness contains the selected factors and their product.
        Replay the combined fault with effects_of([result.witness.product]).
        Select solver="enumeration" (the default), "mwpf", or "highs".
        HiGHS requires the optional qdk[ec,ec-highs] installation.
        Both bounds are None only when no logical failure is possible. A cutoff
        or an open bound gap raises RuntimeError rather than claiming exactness.
        FaultEvent.after selects a zero-based Circuit.calls index; its readout
        indexes are local to that call, excluding hidden reset outcomes.
        Invalid indices, references, or unbound readouts raise ValueError. Propagation
        restrictions are the same as for effects_of. The fault set grows
        exponentially with call support; exact search is also combinatorial.
        """
        from ._analysis.distance_solvers import EnumerationSolverOptions
        from ._distance import _copy_fault, _fault_product, distance_result_of

        data = self._distance_data(faults)
        return distance_result_of(
            data.odd_cycles,
            data.faults,
            solver=EnumerationSolverOptions() if solver is None else solver,
            upper_bound=upper_bound,
            exact=True,
            product=_fault_product,
            copy=_copy_fault,
        )

    def distance_bounds(
        self,
        *,
        faults: Sequence[FaultEvent] | None = None,
        upper_bound: int | None = None,
        solver: BoundsSolver | None = None,
    ) -> Distance[FaultEvent]:
        """Bound the undetected fault count and return an upper-bound witness.

        Faults and failure have the same meaning as in distance, with all checks
        and flags zero for the combined fault.
        Select solver="mwpf" (the default), "enumeration", or "highs".
        HiGHS requires qdk[ec,ec-highs]. Enumeration and HiGHS searches use
        upper_bound as a search cutoff; MWPF does not use it. An upper bound of
        None means no finite bound is established; both bounds being None
        proves no allowed failure exists. Limits may leave a gap between bounds.
        Backend failures, invalid witnesses, or unavailable bound certificates
        raise RuntimeError rather than returning a partial or uncertified bound.
        """
        from ._analysis.distance_solvers import MwpfSolverOptions
        from ._distance import _copy_fault, _fault_product, distance_result_of

        data = self._distance_data(faults)
        return distance_result_of(
            data.odd_cycles,
            data.faults,
            solver=MwpfSolverOptions() if solver is None else solver,
            upper_bound=upper_bound,
            product=_fault_product,
            copy=_copy_fault,
        )

    def _distance_data(self, faults: Sequence[FaultEvent] | None) -> _FaultDistanceData:
        from ._distance import _FaultDistanceData
        from ._faults import _gadget_fault_data

        allowed = self._circuit_faults() if faults is None else tuple(faults)
        observable_count = (
            observe_count_of(self._target.implements)
            if isinstance(self._target, qc.Gadget)
            else len(self.readouts)
        )
        if (
            isinstance(self._target, qc.Gadget)
            and len(self._target.readouts) < observable_count
        ):
            raise ValueError(
                "distance requires every logical measurement readout to be bound"
            )
        observables = self._fault_probes if allowed else FrameGroup(())
        flag_positions = frozenset()
        if isinstance(self._target, qc.Gadget):
            if len(self._target.readouts) < observable_count + len(
                self._target.implements.flags
            ):
                raise ValueError("distance requires every flag readout to be bound")
            flag_positions = frozenset(
                position
                for position, readout in enumerate(self._target.readouts)
                if readout.is_flag
            )
            effects, output_syndromes, indicators = _gadget_fault_data(
                self._target, allowed, observables=observables
            )
        else:
            effects, indicators = self._circuit_fault_data(
                allowed, observables=observables
            )
            output_syndromes = tuple(frozenset() for _ in allowed)
        return _FaultDistanceData.of(
            allowed, effects, output_syndromes, indicators, flag_positions=flag_positions
        )

    @cached_property
    def _fault_probes(self) -> FrameGroup:
        """Physical action probes with signs indexed by circuit-walk outcomes."""
        if isinstance(self._target, qc.Gadget):
            input_code, output_code = realized_codes_of(self._target)
            action = self.action
        else:
            input_code = output_code = _identity_codes_over(self._circuit_outputs)
            action = action_of(self._circuit, with_respect_to=(input_code, output_code))
        observables = _fault_observables(action).generators
        outcome_offset = 2 * len(input_code.support) + len(input_code.stabilizers)
        return FrameGroup(
            PauliFrame(
                abs(output_code.representative_of(observable.pauli)),
                frozenset(
                    outcome - outcome_offset
                    for outcome in observable.frame
                    if outcome >= outcome_offset
                ),
            )
            for observable in observables
        )

    def is_equivalent_to(self, other: "GadgetProfile") -> bool:
        if isinstance(self._target, qc.Gadget) and isinstance(other._target, qc.Gadget):
            return gadgets_equivalent(self._target, other._target)
        return self.action.is_equivalent_to(other.action)

    def why_not_equivalent_to(self, other: "GadgetProfile") -> str:
        """One sentence naming the first difference, or ``""`` if equivalent."""
        if isinstance(self._target, qc.Gadget) and isinstance(other._target, qc.Gadget):
            return why_not_equivalent(self._target, other._target)
        return self.action.why_not_equivalent_to(other.action)

    @property
    def _circuit(self) -> Circuit:
        return (
            self._target.circuit
            if isinstance(self._target, qc.Gadget)
            else self._target
        )

    @cached_property
    def _outcome_code(self) -> OutcomeCode:
        return outcome_code_of(self._circuit)

    @cached_property
    def _circuit_outputs(self) -> tuple[int, ...]:
        """The qubits a bare circuit carries through: those it does not prepare."""
        return tuple(sorted(input_qubits_of(self._circuit)))

    def _circuit_fault_data(
        self, basis: tuple[FaultEvent, ...], *, observables: FrameGroup = FrameGroup(())
    ) -> tuple[tuple[FaultEffect, ...], tuple[frozenset[int], ...]]:
        if not basis:
            return (), ()
        outputs = self._circuit_outputs
        z_probes = [Pauli({qubit: "Z"}) for qubit in outputs]
        x_probes = [Pauli({qubit: "X"}) for qubit in outputs]
        deltas, hidden_count, outcome_count = propagate_faults(
            self._circuit,
            basis,
            z_probes + x_probes + [item.pauli for item in observables.generators],
            residual_frames=[frozenset() for _ in z_probes + x_probes]
            + [item.frame for item in observables.generators],
        )
        z_offset = hidden_count + outcome_count
        x_offset = z_offset + len(z_probes)
        checks = self.checks
        effects = []
        for index in range(len(basis)):
            flipped = frozenset(
                outcome
                for outcome in range(outcome_count)
                if deltas[hidden_count + outcome, index]
            )
            effects.append(
                FaultEffect(
                    frozenset(
                        position
                        for position, check in enumerate(checks)
                        if len(check & flipped) % 2
                    ),
                    flipped,
                    {
                        entry: _residual(
                            deltas[z_offset + entry, index],
                            deltas[x_offset + entry, index],
                        )
                        for entry in range(len(outputs))
                    },
                )
            )
        indicators = _probe_flips(
            deltas, x_offset + len(x_probes), len(observables.generators), len(basis)
        )
        return tuple(effects), indicators

    def _canonical_fault_basis(self) -> tuple[FaultEvent, ...]:
        program = self._circuit
        layout = ProgramLayout.of(program)
        calls = program.calls()
        quantum = tuple(
            FaultEvent.after(index, Pauli({qubit: basis}))
            for index, call in enumerate(calls)
            for qubit in sorted(set(layout.call_qubit_map(call).values()))
            for basis in ("X", "Z")
        )
        return quantum + tuple(
            FaultEvent.after(index, readout_flips=readout)
            for index, call in enumerate(calls)
            for readout in range(
                observe_count_of(program.instruction_set.instructions[call.mnemonic])
            )
        )

    def _circuit_faults(self) -> tuple[FaultEvent, ...]:
        program = self._circuit
        layout = ProgramLayout.of(program)
        faults = []
        for index, call in enumerate(program.calls()):
            support = sorted(set(layout.call_qubit_map(call).values()))
            readout_count = observe_count_of(
                program.instruction_set.instructions[call.mnemonic]
            )
            for characters in product(("I", "X", "Y", "Z"), repeat=len(support)):
                error = Pauli(
                    {
                        qubit: character
                        for qubit, character in zip(support, characters)
                        if character != "I"
                    }
                )
                for flipped in product((False, True), repeat=readout_count):
                    readouts = tuple(
                        position for position, flip in enumerate(flipped) if flip
                    )
                    if error.weight or readouts:
                        faults.append(
                            FaultEvent.after(index, error, readout_flips=readouts)
                        )
        return tuple(faults)


__all__ = ["GadgetProfile"]


def _fault_observables(action: ChannelAction) -> FrameGroup:
    """Distance indicators with signs indexed by simulation outcomes."""
    return FrameGroup(
        (
            *action._stabilizers.generators,
            *action._mapping.values(),
            *(
                PauliFrame(Pauli.identity(), observable.frame)
                for observable in action._observables.generators
            ),
        )
    )


def _residual(z_probe_flipped: bool, x_probe_flipped: bool) -> Pauli:
    """A flipped Z probe reports an X error on that output, and vice versa."""
    if z_probe_flipped and x_probe_flipped:
        character: PauliCharacter = "Y"
    elif z_probe_flipped:
        character = "X"
    elif x_probe_flipped:
        character = "Z"
    else:
        return Pauli.identity()
    return Pauli({0: character})


def _snapshot(target: qc.Gadget | Circuit) -> qc.Gadget | Circuit:
    if isinstance(target, Circuit):
        return Circuit(target.instruction_set, target.source, format=target.format)
    circuit = Circuit(
        target.circuit.instruction_set,
        target.circuit.source,
        format=target.circuit.format,
    )
    return qc.Gadget(
        target.implements,
        circuit,
        inputs=list(target.inputs),
        outputs=list(target.outputs),
        checks=[list(check) for check in target.checks],
        readouts=target.readouts,
        frames=target.frames,
        parameter_bindings=dict(target.parameter_bindings),
        metadata=dict(target.metadata),
    )
