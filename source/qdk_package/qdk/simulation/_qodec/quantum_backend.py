from collections.abc import Sequence
from random import Random, SystemRandom

from qdk import Result
from .. import NoiseConfig
from .physical_engine import PhysicalEngine

from .protocols import QuantumEngine, QuantumEngineFactory, Readouts, Resources
from .quantum_operations import Operation, local_indices


class QuantumBackend:
    def __init__(
        self,
        noise: NoiseConfig | None,
        seed: int | None = None,
        *,
        engine_factory: QuantumEngineFactory,
    ) -> None:
        self.noise = noise if noise is not None else NoiseConfig()
        self.seed = SystemRandom().getrandbits(64) if seed is None else seed
        self.engine_factory = engine_factory
        self._engine: PhysicalEngine | None = None
        self._simulator: QuantumEngine | None = None

    @property
    def engine(self) -> PhysicalEngine:
        if self._engine is None:
            raise RuntimeError("The quantum backend has not been started")
        return self._engine

    def start(self, resources: Resources) -> None:
        if resources.blocks:
            raise ValueError("The quantum backend requires physical-qubit resources")
        # The engine and the noise sampler must draw from independent streams;
        # seeding both with the same value makes faults decide measurement outcomes.
        streams = Random(self.seed)
        engine_seed, noise_seed = streams.getrandbits(64), streams.getrandbits(64)
        self._simulator = self.engine_factory(resources.qubits, engine_seed)
        self._engine = PhysicalEngine(
            self._simulator,
            self.noise,
            seed=noise_seed,
        )

    def prepare(self, target: int) -> None:
        self.engine.reset(target)

    def execute(self, request: Operation) -> Readouts:
        targets = local_indices(request)
        if isinstance(request.angle, str):
            raise TypeError("Physical operation angles must be numeric")
        if request.name == "prepare":
            self.prepare(targets[0])
        elif request.name == "measure":
            return (self.measure(targets[0]),)
        elif request.name == "discard":
            self.discard(targets[0])
        else:
            self.apply(request.name, targets, angle=request.angle)
        return ()

    def apply(
        self, operation: str, targets: Sequence[int], angle: float | None = None
    ) -> None:
        self.engine.apply(operation, targets, angle=angle)

    def measure(self, target: int) -> bool | None:
        result = self.engine.measure(target)
        return None if result == Result.Loss else result == Result.One

    def discard(self, target: int) -> None:
        if self._simulator is None:
            raise RuntimeError("The quantum backend has not been started")
        self._simulator.reset(target)

    def close(self) -> None:
        try:
            if self._engine is not None:
                self._engine.close()
            elif self._simulator is not None:
                self._simulator.close()
        finally:
            self._engine = None
            self._simulator = None


def full_state_backend(noise: NoiseConfig | None, seed: int) -> QuantumBackend:
    from .full_state_engine import FullStateEngine

    return QuantumBackend(
        noise,
        seed,
        engine_factory=lambda num_qubits, engine_seed: FullStateEngine(
            num_qubits, seed=engine_seed
        ),
    )


def stabilizer_backend(noise: NoiseConfig | None, seed: int) -> QuantumBackend:
    from .stabilizer_engine import StabilizerEngine

    return QuantumBackend(
        noise,
        seed,
        engine_factory=lambda num_qubits, engine_seed: StabilizerEngine(
            num_qubits, seed=engine_seed
        ),
    )


def tableau_backend(noise: NoiseConfig | None, seed: int) -> QuantumBackend:
    from .tableau_engine import TableauEngine

    return QuantumBackend(
        noise,
        seed,
        engine_factory=lambda num_qubits, engine_seed: TableauEngine(
            num_qubits, seed=engine_seed
        ),
    )
