# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import hashlib
import logging
from dataclasses import dataclass, field
from itertools import combinations_with_replacement
from math import ceil
from pathlib import Path
from typing import Callable, Generator, Iterable, Optional, Sequence

from ..._qre import ISA, InstructionFrontier, ISARequirements, _Instruction, _binom_ppf
from ..._instruction import (
    LOGICAL,
    PHYSICAL,
    ISAQuery,
    ISATransform,
    constraint,
    instruction,
)
from ..._architecture import _Context
from ...instruction_ids import CNOT, LATTICE_SURGERY, T, MEAS_ZZ
from ..qec import SurfaceCode


logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class RoundBasedFactory(ISATransform):
    """
    A magic state factory that produces T gate instructions using round-based
    distillation pipelines.

    This factory explores combinations of distillation units (such as "15-to-1
    RM prep" and "15-to-1 space efficient") to find optimal configurations that
    minimize time and space while achieving target error rates.  It supports
    both physical-level distillation (when the input T gate is physically
    encoded) and logical-level distillation (using lattice surgery via surface
    codes).

    In order to account for the success probability of distillation rounds, the
    factory models the pipeline using a failure probability requirement
    (defaulting to 1%) that each round must meet.  The number of distillation
    units per round is adjusted to meet this requirement, which in turn affects
    the overall space requirements.

    Space requirements are calculated using a user-provided function that
    aggregates per-round space (e.g., sum or max).  The `sum` function models
    the case in which qubits are not reused across rounds, while the `max`
    function models the case in which qubits are reused across rounds.

    For the enumeration of logical-level distillation units, the factory relies
    on a user-provided `ISAQuery` (defaulting to `SurfaceCode.q()`) to explore
    different surface code configurations and their corresponding lattice
    surgery instructions.  These need to be provided by the user and cannot
    automatically be derived from the provided implementation ISA, as they can
    only contain a subset of the required instructions.  The user needs to
    ensure that the provided query matches the architecture for which this
    factory is being used.

    Results are cached to disk for efficiency.

    Attributes:
        code_query: ISAQuery
            Query to enumerate QEC codes for logical distillation units.
            Defaults to SurfaceCode.q().
        physical_qubit_calculation: Callable[[Iterable], int]
            Function to calculate total physical qubits from per-round space
            requirements, e.g., sum or max.  Defaults to sum.
        cache_dir: Path
            Directory for caching computed factory configurations. Defaults to
            ~/.cache/re3/round_based.
        use_cache: bool
            Whether to use cached results.  Defaults to True.

    References:

    - Sergei Bravyi, Alexei Kitaev: Universal Quantum Computation with ideal
      Clifford gates and noisy ancillas,
      [arXiv:quant-ph/0403025](https://arxiv.org/abs/quant-ph/0403025)
    - Michael E. Beverland, Prakash Murali, Matthias Troyer, Krysta M. Svore,
      Torsten Hoefler, Vadym Kliuchnikov, Guang Hao Low, Mathias Soeken, Aarthi
      Sundaram, Alexander Vaschillo: Assessing requirements to scale to
      practical quantum advantage,
      [arXiv:2211.07629](https://arxiv.org/pdf/2211.07629)
    """

    code_query: ISAQuery = field(default_factory=lambda: SurfaceCode.q())
    physical_qubit_calculation: Callable[[Iterable], int] = field(default=sum)
    # optional: make cache directory configurable
    cache_dir: Path = field(
        default=Path.home() / ".cache" / "re3" / "round_based", repr=False
    )
    use_cache: bool = field(default=True, repr=False)

    @staticmethod
    def required_isa() -> ISARequirements:
        # NOTE: A T gate is required, but a CNOT is only required to explore
        # physical units.
        return ISARequirements(
            constraint(T),
        )

    def provided_isa(self, impl_isa: ISA, ctx: _Context) -> Generator[ISA, None, None]:
        cache_path = self._cache_path(impl_isa)

        # 1) Try to load from cache
        if self.use_cache and cache_path.exists():
            cached_states = InstructionFrontier.load(str(cache_path))
            for state in cached_states:
                yield ISA(state)
            return

        # 2) Compute as before
        t_gate_error = impl_isa[T].expect_error_rate()

        units: list[_DistillationUnit] = []
        initial_unit = []

        # Physical units?
        if impl_isa[T].encoding == PHYSICAL:
            clifford_gate = impl_isa.get(CNOT) or impl_isa.get(MEAS_ZZ)
            if clifford_gate is None:
                raise ValueError(
                    "CNOT or MEAS_ZZ instruction is required for physical units"
                )

            gate_time = clifford_gate.expect_time()
            clifford_error = clifford_gate.expect_error_rate()
            units.extend(self._physical_units(gate_time, clifford_error))
        else:
            initial_unit.append(
                _DistillationUnit(
                    1,
                    impl_isa[T].expect_time(),
                    impl_isa[T].expect_space(),
                    [1, 0],
                    [0],
                )
            )

        for code_isa in self.code_query.enumerate(ctx):
            units.extend(self._logical_units(code_isa[LATTICE_SURGERY]))

        optimal_states = InstructionFrontier()

        for r in range(1, 4 - len(initial_unit)):
            for k in combinations_with_replacement(units, r):
                pipeline = _Pipeline.try_create(
                    initial_unit + list(k),
                    t_gate_error,
                    physical_qubit_calculation=self.physical_qubit_calculation,
                )
                if pipeline is not None:
                    state = self._state_from_pipeline(pipeline)
                    optimal_states.insert(state)
            logger.debug(f"Optimal states after {r} rounds: {len(optimal_states)}")

        # 3) Save to cache, then yield
        if self.use_cache:
            optimal_states.dump(str(cache_path))

        for state in optimal_states:
            yield ISA(ctx.set_source(self, state, [impl_isa[T]]))

    def _physical_units(self, gate_time, clifford_error) -> list[_DistillationUnit]:
        return [
            _DistillationUnit(
                num_input_states=15,
                time=24 * gate_time,
                space=31,
                error_rate_coeffs=[35, 0.0, 0.0, 7.1 * clifford_error],
                failure_probability_coeffs=[15, 356 * clifford_error],
                name="15-to-1 RM prep",
            ),
            _DistillationUnit(
                num_input_states=15,
                time=45 * gate_time,
                space=12,
                error_rate_coeffs=[35, 0.0, 0.0, 7.1 * clifford_error],
                failure_probability_coeffs=[15, 356 * clifford_error],
                name="15-to-1 space efficient",
            ),
        ]

    def _logical_units(
        self, lattice_surgery_instruction: _Instruction
    ) -> list[_DistillationUnit]:
        logical_cycle_time = lattice_surgery_instruction.expect_time(1)
        logical_error = lattice_surgery_instruction.expect_error_rate(1)

        return [
            _DistillationUnit(
                num_input_states=15,
                time=11 * logical_cycle_time,
                space=lattice_surgery_instruction.expect_space(31),
                error_rate_coeffs=[35, 0.0, 0.0, 7.1 * logical_error],
                failure_probability_coeffs=[15, 356 * logical_error],
                name="15-to-1 RM prep",
            ),
            _DistillationUnit(
                num_input_states=15,
                time=13 * logical_cycle_time,
                space=lattice_surgery_instruction.expect_space(20),
                error_rate_coeffs=[35, 0.0, 0.0, 7.1 * logical_error],
                failure_probability_coeffs=[15, 356 * logical_error],
                name="15-to-1 space efficient",
            ),
        ]

    def _state_from_pipeline(self, pipeline: _Pipeline) -> _Instruction:
        return instruction(
            T,
            encoding=LOGICAL,
            time=pipeline.time,
            error_rate=pipeline.error_rate,
            space=pipeline.space,
        )

    def _cache_key(self, impl_isa: ISA) -> str:
        """Build a deterministic key from factory configuration and impl_isa."""
        # You can refine this if ISA has a better serialization method.
        payload = {
            "factory": type(self).__qualname__,
            "code_query": getattr(
                self.code_query, "__qualname__", repr(self.code_query)
            ),
            "impl_isa": str(impl_isa),
        }
        data = repr(payload).encode("utf-8")
        return hashlib.sha256(data).hexdigest()

    def _cache_path(self, impl_isa: ISA) -> Path:
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        return self.cache_dir / f"{self._cache_key(impl_isa)}.json"


class _Pipeline:
    def __init__(
        self,
        units: Sequence[_DistillationUnit],
        initial_input_error_rate: float,
        *,
        failure_probability_requirement: float = 0.01,
        physical_qubit_calculation: Callable[[Iterable], int] = sum,
    ):
        self.failure_probability_requirement = failure_probability_requirement
        self.rounds: list["_DistillationRound"] = []
        self.output_error_rate: float = initial_input_error_rate
        self.physical_qubit_calculation = physical_qubit_calculation

        self._add_rounds(units)

    @classmethod
    def try_create(
        cls,
        units: Sequence[_DistillationUnit],
        initial_input_error_rate: float,
        *,
        failure_probability_requirement: float = 0.01,
        physical_qubit_calculation: Callable[[Iterable], int] = sum,
    ) -> Optional[_Pipeline]:
        pipeline = cls(
            units,
            initial_input_error_rate,
            failure_probability_requirement=failure_probability_requirement,
            physical_qubit_calculation=physical_qubit_calculation,
        )
        if not pipeline._compute_units_per_round():
            return None
        return pipeline

    def _compute_units_per_round(self) -> bool:
        if len(self.rounds) > 0:
            states_needed_next = self.rounds[-1].unit.num_output_states

            for dist_round in reversed(self.rounds):
                if not dist_round.adjust_num_units_to(states_needed_next):
                    return False
                states_needed_next = dist_round.num_input_states

        return True

    def _add_rounds(self, units: Sequence[_DistillationUnit]):
        per_round_failure_prob_req = self.failure_probability_requirement / len(units)

        for unit in units:
            self.rounds.append(
                _DistillationRound(
                    unit,
                    per_round_failure_prob_req,
                    self.output_error_rate,
                )
            )
            # TODO: handle case when output_error_rate is larger than input_error_rate
            self.output_error_rate = unit.error_rate(self.output_error_rate)

    @property
    def space(self) -> int:
        return self.physical_qubit_calculation(round.space for round in self.rounds)

    @property
    def time(self) -> int:
        return sum(round.unit.time for round in self.rounds)

    @property
    def error_rate(self) -> float:
        return self.output_error_rate

    @property
    def num_output_states(self) -> int:
        return self.rounds[-1].compute_num_output_states()


@dataclass(slots=True)
class _DistillationUnit:
    num_input_states: int
    time: int
    space: int
    error_rate_coeffs: Sequence[float]
    failure_probability_coeffs: Sequence[float]
    name: Optional[str] = None
    num_output_states: int = 1

    def error_rate(self, input_error_rate: float) -> float:
        result = 0.0
        for c in self.error_rate_coeffs:
            result = result * input_error_rate + c
        return result

    def failure_probability(self, input_error_rate: float) -> float:
        result = 0.0
        for c in self.failure_probability_coeffs:
            result = result * input_error_rate + c
        return result


@dataclass(slots=True)
class _DistillationRound:
    unit: _DistillationUnit
    failure_probability_requirement: float
    input_error_rate: float
    num_units: int = 1
    failure_probability: float = field(init=False)

    def __post_init__(self):
        self.failure_probability = self.unit.failure_probability(self.input_error_rate)

    def adjust_num_units_to(self, output_states_needed_next: int) -> bool:
        if self.failure_probability == 0.0:
            self.num_units = output_states_needed_next
            return True

        # Binary search to find the minimal number of units needed
        self.num_units = ceil(output_states_needed_next / self.max_num_output_states)

        while True:
            num_output_states = self.compute_num_output_states()
            if num_output_states < output_states_needed_next:
                self.num_units *= 2

                # Distillation round requires unreasonably high number of units
                if self.num_units >= 1_000_000_000_000_000:
                    return False
            else:
                break

        upper = self.num_units
        lower = self.num_units // 2
        while lower < upper:
            self.num_units = (lower + upper) // 2
            num_output_states = self.compute_num_output_states()
            if num_output_states >= output_states_needed_next:
                upper = self.num_units
            else:
                lower = self.num_units + 1
        self.num_units = upper

        return True

    @property
    def space(self) -> int:
        return self.num_units * self.unit.space

    @property
    def num_input_states(self) -> int:
        return self.num_units * self.unit.num_input_states

    @property
    def max_num_output_states(self) -> int:
        return self.num_units * self.unit.num_output_states

    def compute_num_output_states(self) -> int:
        failure_prob = self.failure_probability

        if failure_prob <= 1e-8:
            return self.num_units * self.unit.num_output_states

        # A replacement for SciPy's binom.ppf that is faster
        k = _binom_ppf(
            self.failure_probability_requirement,
            self.num_units,
            1.0 - failure_prob,
        )

        return int(k) * self.unit.num_output_states
