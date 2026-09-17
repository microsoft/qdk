from collections.abc import Sequence

_GATES = {
    "x": "x",
    "y": "y",
    "z": "z",
    "h": "h",
    "s": "s",
    "s_adj": "s_dag",
    "sx": "sqrt_x",
    "sx_adj": "sqrt_x_dag",
    "cx": "cx",
    "cy": "cy",
    "cz": "cz",
    "swap": "swap",
    "mov": None,
}
_TWO_QUBIT_OPERATIONS = frozenset({"cx", "cy", "cz", "swap"})


class TableauEngine:
    def __init__(self, num_qubits: int, *, seed: int | None = None) -> None:
        if num_qubits < 0:
            raise ValueError("num_qubits must be nonnegative")
        try:
            import stim
        except ModuleNotFoundError as error:
            if error.name != "stim":
                raise
            raise NotImplementedError(
                "The tableau backend requires the optional Stim package"
            ) from error

        self.simulator = stim.TableauSimulator(seed=seed)
        self.simulator.set_num_qubits(num_qubits)
        self._num_qubits = num_qubits
        self._closed = False

    def apply(
        self, operation: str, targets: Sequence[int], *, angle: float | None = None
    ) -> None:
        self._ensure_open()
        if operation not in _GATES:
            raise NotImplementedError(
                f"The tableau backend accepts Clifford gates only; unsupported operation {operation!r}"
            )
        if angle is not None:
            raise ValueError(f"operation {operation!r} does not accept an angle")
        arity = 2 if operation in _TWO_QUBIT_OPERATIONS else 1
        if len(targets) != arity:
            raise ValueError(f"operation {operation!r} expects {arity} qubits")
        for target in targets:
            self._validate_target(target)
        if len(set(targets)) != len(targets):
            raise ValueError(f"operation {operation!r} requires distinct qubits")
        name = _GATES[operation]
        if name is not None:
            getattr(self.simulator, name)(*targets)

    def measure(self, target: int) -> int:
        self._ensure_open()
        self._validate_target(target)
        return int(self.simulator.measure(target))

    def reset(self, target: int) -> None:
        self._ensure_open()
        self._validate_target(target)
        self.simulator.reset(target)

    def close(self) -> None:
        if not self._closed:
            self.simulator.set_num_qubits(0)
            self._closed = True

    def _ensure_open(self) -> None:
        if self._closed:
            raise RuntimeError("tableau engine is closed")

    def _validate_target(self, target: int) -> None:
        if target < 0 or target >= self._num_qubits:
            raise ValueError(f"qubit {target} is out of range")
