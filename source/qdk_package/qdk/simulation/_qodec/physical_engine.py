# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Shared physical noise orchestration for persistent engines."""

from __future__ import annotations

import random
from collections.abc import Sequence
from dataclasses import dataclass
from itertools import product
from typing import Protocol, cast

from ..._native import LossPolicy, NoiseConfig, NoiseTable, Result

__all__ = ["BatchResult", "PhysicalEngine"]

PhysicalOperation = tuple[str, Sequence[int], float | None]


@dataclass(frozen=True)
class BatchResult:
    completed: int
    observations: tuple[Result, ...]


class _Engine(Protocol):
    def apply(
        self, operation: str, targets: Sequence[int], *, angle: float | None = None
    ) -> None: ...

    def measure(self, target: int) -> int: ...

    def reset(self, target: int) -> None: ...

    def close(self) -> None: ...


class _RandomSource(Protocol):
    def random(self) -> float: ...


class PhysicalEngine:
    """Applies one configured physical noise site per semantic operation."""

    def __init__(self, inner: _Engine, noise: NoiseConfig, *, seed: int = 0) -> None:
        self._inner = inner
        self._noise = noise
        self._rng: _RandomSource = random.Random(seed)
        self._distributions: dict[tuple[int, int], tuple[tuple[str, float], ...]] = {}
        self._lost: set[int] = set()
        self._closed = False

    def execute_batch(self, operations: Sequence[PhysicalOperation]) -> BatchResult:
        if self._closed:
            raise RuntimeError("physical engine is closed")
        completed = 0
        observations = []
        try:
            for operation, targets, angle in operations:
                if operation == "measure":
                    if len(targets) != 1:
                        raise ValueError("measurement expects 1 qubit")
                    observations.append(self.measure(targets[0]))
                elif operation == "reset":
                    if len(targets) != 1:
                        raise ValueError("reset expects 1 qubit")
                    self.reset(targets[0])
                else:
                    self.apply(operation, targets, angle=angle)
                completed += 1
        except Exception as error:
            self.close()
            raise RuntimeError(
                f"physical batch failed after {completed} operation(s)"
            ) from error
        return BatchResult(completed, tuple(observations))

    def apply(
        self, operation: str, targets: Sequence[int], *, angle: float | None = None
    ) -> None:
        table = self._table(operation)
        distribution = self._distribution(table, len(targets), operation)
        lost = [target for target in targets if target in self._lost]
        if not lost:
            self._inner.apply(operation, targets, angle=angle)
        elif len(targets) == 2:
            degraded = self._apply_loss_policy(operation, targets, angle, table.on_loss)
            if degraded is not None:
                degraded_operation, degraded_targets = degraded
                degraded_table = self._table(degraded_operation)
                degraded_distribution = self._distribution(
                    degraded_table, len(degraded_targets), degraded_operation
                )
                self._sample_and_apply(
                    degraded_distribution,
                    degraded_targets,
                    degraded_operation,
                )
                return
        self._sample_and_apply(distribution, targets, operation)

    def measure(self, target: int) -> Result:
        table = self._noise.mresetz
        distribution = self._distribution(table, 1, "measure")
        if target in self._lost:
            self._lost.remove(target)
            outcome = Result.Loss
        else:
            outcome = Result.One if self._inner.measure(target) else Result.Zero
        self._sample_and_apply(distribution, (target,), "measure")
        return cast(Result, outcome)

    def reset(self, target: int) -> None:
        table = self._noise.mresetz
        distribution = self._distribution(table, 1, "reset")
        if target in self._lost:
            self._lost.remove(target)
        else:
            self._inner.reset(target)
        self._sample_and_apply(distribution, (target,), "reset")

    def peek_loss(self, target: int) -> bool:
        return target in self._lost

    def apply_readout_noise(
        self, result: Result, p_zero_as_one: float, p_one_as_zero: float
    ) -> Result:
        if not 0.0 <= p_zero_as_one <= 1.0 or not 0.0 <= p_one_as_zero <= 1.0:
            raise ValueError("readout probabilities must be between zero and one")
        sample = self._rng.random()
        if result == Result.Zero and sample < p_zero_as_one:
            return cast(Result, Result.One)
        if result == Result.One and sample < p_one_as_zero:
            return cast(Result, Result.Zero)
        return result

    def apply_intrinsic(self, name: str, targets: Sequence[int]) -> None:
        if name not in self._noise.intrinsics:
            raise NotImplementedError(f"unknown noise intrinsic {name!r}")
        table = self._noise.intrinsics[name]
        distribution = self._distribution(table, len(targets), name)
        self._sample_and_apply(distribution, targets, name)

    def close(self) -> None:
        if not self._closed:
            self._closed = True
            self._inner.close()

    def _table(self, operation: str) -> NoiseTable:
        table = getattr(self._noise, operation, None)
        if table is None:
            raise NotImplementedError(f"unsupported noisy operation {operation!r}")
        return table

    def _distribution(
        self, table: NoiseTable, width: int, operation: str
    ) -> tuple[tuple[str, float], ...]:
        key = (id(table), width)
        cached = self._distributions.get(key)
        if cached is not None:
            return cached
        weighted = []
        if not table.is_noiseless():
            for characters in product("IXYZL", repeat=width):
                fault = "".join(characters)
                if fault == "I" * width:
                    continue
                try:
                    probability = getattr(table, fault.lower())
                except AttributeError as error:
                    raise ValueError(
                        f"operation {operation!r} acts on {width} qubit(s), but "
                        "its noise table is written for a different arity"
                    ) from error
                if probability:
                    weighted.append((fault, probability))
        result = tuple(weighted)
        self._distributions[key] = result
        return result

    def _sample_and_apply(
        self,
        distribution: tuple[tuple[str, float], ...],
        targets: Sequence[int],
        operation: str,
    ) -> None:
        if not distribution:
            return
        draw = self._rng.random()
        for fault, probability in distribution:
            draw -= probability
            if draw < 0.0:
                self._apply_fault(fault, targets, operation)
                return

    def _apply_fault(self, fault: str, targets: Sequence[int], operation: str) -> None:
        for target, character in zip(targets, fault):
            if target in self._lost:
                continue
            if character == "L":
                self._inner.reset(target)
                self._lost.add(target)
            elif character != "I":
                self._inner.apply(character.lower(), (target,))

    def _apply_loss_policy(
        self,
        operation: str,
        targets: Sequence[int],
        angle: float | None,
        policy: LossPolicy,
    ) -> tuple[str, tuple[int, ...]] | None:
        surviving = [target for target in targets if target not in self._lost]
        if not surviving or policy == LossPolicy.SKIP:
            return None
        if policy == LossPolicy.PROPAGATE:
            for target in surviving:
                self._inner.reset(target)
                self._lost.add(target)
            return None
        if policy == LossPolicy.RESIDUAL_S_DAGGER:
            if operation == "swap":
                self._inner.apply("swap", targets)
                self._swap_loss(targets)
                surviving = [target for target in targets if target not in self._lost]
            for target in surviving:
                self._inner.apply("s_adj", (target,))
            return None
        if policy == LossPolicy.DEGRADE and operation in {"rxx", "ryy", "rzz"}:
            if angle is None:
                raise ValueError(f"operation {operation!r} requires an angle")
            degraded_operation = f"r{operation[-1]}"
            degraded_targets = (surviving[0],)
            self._inner.apply(degraded_operation, degraded_targets, angle=angle)
            return degraded_operation, degraded_targets
        if policy == LossPolicy.APPLY_ANYWAY and operation == "swap":
            self._inner.apply("swap", targets)
            self._swap_loss(targets)
            return None
        raise NotImplementedError(
            f"loss policy {policy.name} is not supported for operation {operation!r}"
        )

    def _swap_loss(self, targets: Sequence[int]) -> None:
        q1, q2 = targets
        self._lost = {
            q2 if target == q1 else q1 if target == q2 else target
            for target in self._lost
        }
