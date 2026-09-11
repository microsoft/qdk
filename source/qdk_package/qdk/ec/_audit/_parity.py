"""Exact affine parity evaluation with arbitrary incoming Pauli frames."""

from __future__ import annotations

from collections.abc import Iterable
from functools import cached_property
import json

from binar import BitMatrix, BitVector, solve
import qodec as qc
from qodec.gadgets import Reference

from .._analysis.channel_action import declared_action_of, realized_codes_of
from .._analysis.check_discovery import choi_prepare
from .._analysis.propagation.frames import FrameGroup, PauliFrame
from .._analysis.propagation.interpreter import _FramePropagator, walk_program
from .._analysis.propagation.isa_actions import remap_pauli
from .._analysis.propagation.pauli import Pauli, complex_conjugate_of, identity, relabel
from .._analysis.propagation.pauli_remap import (
    encoding_qubit_relocation,
)
from .._layout import ProgramLayout


def terms_of(equation: Iterable[Reference]) -> tuple[str, ...]:
    terms: dict[str, None] = {}
    for reference in equation:
        for atom in reference.expand():
            if atom.kind == "circuit_readout":
                path = f"circuit.readouts[{atom.index}]"
            elif atom.kind == "readout":
                path = f"readouts[{atom.index}]"
            else:
                path = f"{atom.boundary}[{atom.entry}].{atom.encoding_property}[{atom.index}]"
            if path in terms:
                del terms[path]
            else:
                terms[path] = None
    return tuple(terms)


def _rows(rows: Iterable[BitVector], width: int) -> BitMatrix:
    values = list(rows)
    return BitMatrix(values) if values else BitMatrix.zeros(0, width)


def _unresolved_signs(
    output_signs: list[str], equations: list[tuple[str, ...]]
) -> tuple[str, ...]:
    unknowns = dict.fromkeys(output_signs)
    for equation in equations:
        for path in equation:
            if path.startswith(("out[", "readouts[")):
                unknowns[path] = None
    positions = {path: index for index, path in enumerate(unknowns)}
    matrix = _rows(
        (BitVector(path in equation for path in unknowns) for equation in equations),
        len(unknowns),
    )
    return tuple(
        path
        for path in output_signs
        if solve(
            matrix.T,
            BitVector(index == positions[path] for index in range(len(unknowns))),
        )
        is None
    )


class ParityAnalysis:
    """Check parities against called instructions' noiseless, zero-flag contract.

    Each called instruction's implementation is audited separately.
    """

    def __init__(self, gadget: qc.Gadget) -> None:
        self.gadget = gadget
        self.checks = tuple(terms_of(check) for check in gadget.checks)
        self.readouts = tuple(terms_of(readout.equation) for readout in gadget.readouts)

    def _rotation_invariants_only(self) -> bool:
        """Replacing pure rotations by identity is exact for commuting stabilizers only."""
        program = self.gadget.circuit
        if program.readouts or any(
            path.startswith("out[") and "].stabilizers[" not in path
            for equation in (*self.checks, *self.readouts)
            for path in equation
        ):
            return False
        layout = ProgramLayout.of(program)
        axes = []
        for call in program.calls:
            instruction = program.instruction_set.instructions[call.mnemonic]
            for action in instruction.action:
                if (
                    not isinstance(action, qc.actions.Rotate)
                    or action.condition is not None
                ):
                    return False
                axes.append(remap_pauli(action.pauli, layout.call_qubit_map(call)))
        return bool(axes) and all(
            axis.commutes_with(
                relabel(Pauli(str(operator)), encoding_qubit_relocation(encoding))
            )
            for axis in axes
            for encoding in self.gadget.outputs
            for operator in encoding.code.stabilizers
        )

    @cached_property
    def _simulation(
        self,
    ) -> tuple[dict[str, BitVector], dict[int, BitVector], dict[str, str]]:
        gadget = self.gadget
        program = gadget.circuit
        for call in program.calls:
            if call.predicates:
                raise NotImplementedError(
                    "conditional circuit calls are not supported by parity verification"
                )
            instruction = program.instruction_set.instructions[call.mnemonic]
            if any(
                name not in instruction.flags
                for pattern in call.select
                for name in pattern
            ):
                raise ValueError("selection names an undeclared instruction flag")
            if call.select and not any(
                all(value == 0 for value in pattern.values()) for pattern in call.select
            ):
                raise NotImplementedError(
                    "selection rejects the noiseless zero-flag branch"
                )
            if any(
                getattr(action, "condition", None) is not None
                and not isinstance(action, qc.actions.Pauli)
                for action in instruction.action
            ):
                raise NotImplementedError(
                    "conditional circuit actions are not supported by parity verification"
                )
        simulation = choi_prepare(gadget)
        input_qubits = sorted(
            {
                qubit
                for encoding in gadget.inputs
                for qubit in encoding_qubit_relocation(encoding).values()
            }
        )
        auxiliary_origin = simulation.qubit_count - len(input_qubits)
        partners = {
            qubit: auxiliary_origin + offset
            for offset, qubit in enumerate(input_qubits)
        }
        frame_basis = [
            Pauli({qubit: basis}) for qubit in input_qubits for basis in ("X", "Z")
        ]
        frames = _FramePropagator(len(frame_basis))
        for index, pauli in enumerate(frame_basis):
            frames.apply_pauli_to_shot(index, pauli)
        rows: dict[str, int] = {}
        deltas: dict[str, list[bool]] = {}
        unavailable: dict[str, str] = {}
        initial_rows = []

        def measure(pauli: Pauli) -> int:
            row = simulation.outcome_count
            simulation.measure(pauli)
            return row

        for entry, encoding in enumerate(gadget.inputs):
            relocation = encoding_qubit_relocation(encoding)
            for basis, operators in (
                ("stabilizers", encoding.code.stabilizers),
                ("x", encoding.code.x),
                ("z", encoding.code.z),
            ):
                for index, operator in enumerate(operators):
                    path = f"in[{entry}].{basis}[{index}]"
                    pauli = relabel(Pauli(str(operator)), relocation)
                    deltas[path] = [
                        not error.commutes_with(pauli) for error in frame_basis
                    ]
                    if basis == "stabilizers":
                        rows[path] = measure(pauli)
                        initial_rows.append(rows[path])

        observe_outcomes = ()
        if not self._rotation_invariants_only():
            walk = walk_program(program, simulation=simulation, extra_engines=[frames])
            observe_outcomes = walk.observe_outcomes
            if len(observe_outcomes) != sum(
                not isinstance(readout, qc.gadgets.Flag) for readout in program.readouts
            ):
                raise NotImplementedError(
                    "circuit observations are not fully simulated by parity verification"
                )
        outcome_rows = iter(observe_outcomes)
        for position, readout in enumerate(program.readouts):
            path = f"circuit.readouts[{position}]"
            if isinstance(readout, qc.gadgets.Flag):
                deltas[path] = [False] * len(frame_basis)
                continue
            row = next(outcome_rows)
            rows[path] = row
            deltas[path] = [
                bool(frames.outcome_deltas[row - len(initial_rows), index])
                for index in range(len(frame_basis))
            ]

        output_operators: dict[str, Pauli] = {}
        for entry, encoding in enumerate(gadget.outputs):
            relocation = encoding_qubit_relocation(encoding)
            for index, operator in enumerate(encoding.code.stabilizers):
                path = f"out[{entry}].stabilizers[{index}]"
                pauli = relabel(Pauli(str(operator)), relocation)
                output_operators[path] = pauli
                rows[path] = measure(pauli)

        logical_offset = 0
        referenced = {
            term for equation in (*self.checks, *self.readouts) for term in equation
        }
        objective = None
        for entry, encoding in enumerate(gadget.outputs):
            relocation = encoding_qubit_relocation(encoding)
            for basis, operators in (("x", encoding.code.x), ("z", encoding.code.z)):
                for index, operator in enumerate(operators):
                    path = f"out[{entry}].{basis}[{index}]"
                    if path not in referenced:
                        continue
                    pauli = relabel(Pauli(str(operator)), relocation)
                    try:
                        if any(
                            isinstance(action, qc.actions.Observe)
                            for action in gadget.implements.action
                        ):
                            raise NotImplementedError(
                                "output logical frames after logical measurements are not supported"
                            )
                        if objective is None:
                            objective = declared_action_of(gadget)
                        if any(
                            item.frame for item in objective._stabilizers.generators
                        ):
                            raise NotImplementedError(
                                "outcome-dependent output logical frames are not supported"
                            )
                        mapping = list(objective._mapping.items())
                        images = FrameGroup(
                            [
                                PauliFrame(image.pauli, frozenset({position}))
                                for position, (_, image) in enumerate(mapping)
                            ]
                            + [
                                PauliFrame(item.pauli)
                                for item in objective._stabilizers.generators
                            ]
                        )
                        factors = images.factorization_of(
                            Pauli(
                                {logical_offset + index: "X" if basis == "x" else "Z"}
                            )
                        )
                        if factors is None:
                            raise ValueError(
                                "instruction does not determine this output logical frame"
                            )
                        anchor = Pauli.identity()
                        for factor in factors:
                            if not factor.pauli.weight:
                                anchor *= identity(factor.pauli.phase)
                            for position in factor.frame:
                                if position >= len(mapping):
                                    raise NotImplementedError(
                                        "outcome-dependent output frames are not supported"
                                    )
                                logical = mapping[position][0]
                                physical = realized_codes_of(gadget)[
                                    0
                                ].representative_of(logical)
                                anchor *= relabel(
                                    complex_conjugate_of(physical), partners
                                )
                        joint = pauli * anchor
                        if not simulation.is_stabilizer(joint, ignore_sign=True):
                            raise ValueError(
                                "circuit does not realize the declared output logical frame"
                            )
                        rows[path] = measure(joint)
                        output_operators[path] = pauli
                    except (
                        KeyError,
                        ValueError,
                        TypeError,
                        NotImplementedError,
                    ) as error:
                        unavailable[path] = str(error)
            logical_offset += len(encoding.code.x)

        for path, pauli in output_operators.items():
            frame_row = frames.measure(pauli)
            deltas[path] = [
                bool(frames.outcome_deltas[frame_row, index])
                for index in range(len(frame_basis))
            ]

        expected_rows = {}
        observe_actions = [
            action
            for action in gadget.implements.action
            if isinstance(action, qc.actions.Observe)
        ]
        input_code = realized_codes_of(gadget)[0]
        probes = [
            input_code.representative_of(Pauli(str(operator)))
            for action in observe_actions
            for operator in action.observables
        ]
        if any(
            not isinstance(action, qc.actions.Observe)
            for action in gadget.implements.action[: len(observe_actions)]
        ):
            unavailable["observables"] = (
                "readout verification of interleaved logical actions is not supported"
            )
        elif any(
            not left.commutes_with(right)
            for index, left in enumerate(probes)
            for right in probes[index + 1 :]
        ):
            unavailable["observables"] = (
                "noncommuting logical readouts require sequential verification"
            )
        else:
            for position, probe in enumerate(probes):
                expected_rows[position] = measure(
                    relabel(complex_conjugate_of(probe), partners)
                )

        matrix = simulation.outcome_matrix
        width = matrix.column_count
        constraints = _rows(
            (
                BitVector(matrix[row, column] for column in range(width))
                for row in initial_rows
            ),
            width,
        )
        shift = BitVector(bool(simulation.outcome_shift[row]) for row in initial_rows)
        particular = solve(constraints, shift)
        if particular is None:
            raise ValueError("input code has no noiseless +1 codeword")
        kernel = list(constraints.kernel().rows)

        def signature(row: int | None, frame: list[bool]) -> BitVector:
            linear = (
                BitVector.zeros(width)
                if row is None
                else BitVector(matrix[row, column] for column in range(width))
            )
            constant = False if row is None else bool(simulation.outcome_shift[row])
            return BitVector(
                [
                    *(linear.dot(vector) for vector in kernel),
                    *frame,
                    constant ^ linear.dot(particular),
                ]
            )

        values = {
            path: signature(rows.get(path), delta) for path, delta in deltas.items()
        }
        expected = {
            position: signature(row, [False] * len(frame_basis))
            for position, row in expected_rows.items()
        }
        values["0"] = signature(None, [False] * len(frame_basis))
        return values, expected, unavailable

    @property
    def values(self) -> dict[str, BitVector]:
        return self._simulation[0]

    @property
    def expected(self) -> dict[int, BitVector]:
        if "observables" in self._simulation[2]:
            raise NotImplementedError(self._simulation[2]["observables"])
        return self._simulation[1]

    def external(self, path: str) -> BitVector:
        if path in self._simulation[2]:
            raise NotImplementedError(f"{path}: {self._simulation[2][path]}")
        if path not in self.values:
            raise ValueError(f"unresolved reference {path}")
        return self.values[path]

    @cached_property
    def resolved(self) -> tuple[tuple[BitVector, ...], frozenset[int], str | None]:
        count = len(self.readouts)
        matrix = BitMatrix.identity(count)
        right = []
        for position, equation in enumerate(self.readouts):
            value = self.values["0"].copy()
            for path in equation:
                reference = Reference(path)
                if reference.kind == "readout":
                    matrix[position, reference.index] = not matrix[
                        position, reference.index
                    ]
                else:
                    value = value ^ self.external(path)
            right.append(value)
        for dependency in matrix.T.kernel().rows:
            value = self.values["0"].copy()
            for index in dependency.support:
                value = value ^ right[index]
            if not value.is_zero:
                return (
                    (),
                    frozenset(),
                    f"Inconsistent readout equations at positions {dependency.support}.",
                )
        unresolved = frozenset(
            index for vector in matrix.kernel().rows for index in vector.support
        )
        columns = [
            solve(matrix, BitVector(value[column] for value in right))
            for column in range(len(self.values["0"]))
        ]
        if any(column is None for column in columns):
            raise ValueError("readout equations have no solution")
        values = [
            BitVector(column[position] for column in columns if column is not None)
            for position in range(count)
        ]
        return tuple(values), unresolved, None

    def value(self, equation: Iterable[str]) -> BitVector:
        result = self.values["0"].copy()
        for path in equation:
            reference = Reference(path)
            if reference.kind == "readout":
                values, unresolved, error = self.resolved
                if error or reference.index in unresolved:
                    raise ValueError(
                        error or f"readouts[{reference.index}] is undetermined"
                    )
                result = result ^ values[reference.index]
            else:
                result = result ^ self.external(path)
        return result

    def candidate(self, expected: BitVector) -> tuple[str, ...] | None:
        paths = [path for path in self.values if path.startswith(("circuit.", "in["))]
        matrix = _rows((self.values[path] for path in paths), len(expected))
        solution = solve(matrix.T, expected)
        return (
            None
            if solution is None
            else tuple(paths[index] for index in solution.support)
        )

    def missing_checks(self) -> tuple[tuple[str, ...], ...]:
        """Independent measurement checks absent from the declared relation span.

        Candidates use circuit bits and incoming stabilizer signs. Input-only
        identities supply no measurement information; output-frame relations
        are checked separately. Readout definitions permit substitution, while
        verified zero-valued flags already expose their syndrome information.
        """
        if not self.gadget.circuit.readouts:
            return ()
        paths = [
            path
            for path in self.values
            if path.startswith("circuit.")
            or (path.startswith("in[") and "].stabilizers[" in path)
        ]
        signatures = _rows((self.values[path] for path in paths), len(self.values["0"]))
        candidates = [
            tuple(paths[index] for index in row.support)
            for row in signatures.T.kernel().rows
        ]
        known = []
        for equation in self.checks:
            try:
                if self.value(equation).is_zero:
                    known.append(equation)
            except (KeyError, ValueError, TypeError, NotImplementedError):
                continue
        if self.readouts:
            values, unresolved, error = self.resolved
            if error is None:
                for position, equation in enumerate(self.readouts):
                    if position in unresolved:
                        continue
                    path = f"readouts[{position}]"
                    known.append(
                        terms_of(Reference(term) for term in (path, *equation))
                    )
                    if (
                        self.gadget.readouts[position].is_flag
                        and values[position].is_zero
                    ):
                        known.append((path,))
        known.extend(
            equation
            for equation in candidates
            if not any(path.startswith("circuit.") for path in equation)
        )
        columns = tuple(
            dict.fromkeys((*paths, *(path for equation in known for path in equation)))
        )
        rows = [BitVector(path in equation for path in columns) for equation in known]
        missing = []
        for equation in candidates:
            if not any(path.startswith("circuit.") for path in equation):
                continue
            target = BitVector(path in equation for path in columns)
            if solve(_rows(rows, len(columns)).T, target) is None:
                missing.append(equation)
                rows.append(target)
        return tuple(missing)

    def unresolved_outputs(self) -> tuple[str, ...]:
        output_signs = [
            f"out[{entry}].stabilizers[{index}]"
            for entry, encoding in enumerate(self.gadget.outputs)
            for index in range(len(encoding.code.stabilizers))
        ]
        if not output_signs:
            return ()
        if not any(self.checks) and not any(self.readouts):
            return tuple(output_signs)
        equations = []
        for equation in self.checks:
            try:
                if self.value(equation).is_zero:
                    equations.append(equation)
            except (KeyError, ValueError, TypeError, NotImplementedError):
                continue
        if not _unresolved_signs(output_signs, equations):
            return ()
        values, unresolved, error = self.resolved
        if error is None:
            for position, equation in enumerate(self.readouts):
                if position in unresolved:
                    continue
                expected = self.expected.get(position, self.values["0"])
                if values[position] == expected:
                    equations.append(
                        terms_of(
                            Reference(path)
                            for path in (f"readouts[{position}]", *equation)
                        )
                    )
        return _unresolved_signs(output_signs, equations)

    def witness(self, equation: Iterable[str], difference: BitVector) -> str:
        assignment = BitVector.zeros(len(difference))
        assignment[len(difference) - 1] = True
        if not difference[len(difference) - 1]:
            assignment[
                next(
                    index for index in difference.support if index < len(difference) - 1
                )
            ] = True
        values = {
            path: int(self.value((path,)).dot(assignment))
            for path in dict.fromkeys(equation)
        }
        return json.dumps(values)
