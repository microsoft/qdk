# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""``tensornetwork_qir`` simulates a QIR program's shots on a matrix product
state (MPS), for programs too entangled for a sparse simulator and too large
for a state vector, and returns them with the run's report: what the max bond
dimension cost in accuracy. ``check_tensornetwork_setup`` says whether the
GPU is set up for the GPU engine (``engine="gpu"``, the default, which needs
``pip install "qdk[tensornetwork]"``); the CPU engine (``engine="cpu"``)
needs no setup.
"""

from dataclasses import dataclass
from importlib import metadata
import os
from typing import List, Literal, Optional, Sequence, Tuple, Union
import warnings

from .._adaptive_pass import AdaptiveProfilePass, Bytecode
from .._types import QirInputData
from ._simulation import (
    DecomposeCcxPass,
    OutputRecordingPass,
    preprocess_simulation_input,
)

_U32_MAX = 2**32 - 1

# The NVIDIA wheels that `qdk[tensornetwork]` installs, and where each puts
# its library, relative to the environment's site-packages.
_PACKAGED_LIBRARIES = (
    ("cutensornet-cu12", "cuquantum/lib/libcutensornet.so.2"),
    ("nvidia-cuda-runtime-cu12", "nvidia/cuda_runtime/lib/libcudart.so.12"),
)

# The command that installs the wheels above, which the setup check's advice
# gives when a library is missing.
_INSTALL_COMMAND = 'pip install "qdk[tensornetwork]"'


@dataclass(frozen=True)
class MpsOptions:
    """Options of the MPS simulation method (``method="mps"``).

    :param max_bond_dimension: The max bond dimension χ: every bond of the
        matrix product state is at most χ. Larger is more accurate and costs
        more time and memory; the run's report's ``discarded_weight`` says
        what truncating to it lost.
    """

    max_bond_dimension: int = 128

    def __post_init__(self) -> None:
        bond = self.max_bond_dimension
        if (
            isinstance(bond, bool)
            or not isinstance(bond, int)
            or not 1 <= bond <= _U32_MAX
        ):
            raise ValueError("max_bond_dimension must be an integer in [1, 2**32)")


@dataclass(frozen=True)
class Sample:
    """Query ``shots`` shots, as ``run_qir`` returns them: the result is a
    list with one entry per shot.

    :param shots: The number of shots, in [1, 2**32).
    """

    shots: int

    def __post_init__(self) -> None:
        shots = self.shots
        if (
            isinstance(shots, bool)
            or not isinstance(shots, int)
            or not 1 <= shots <= _U32_MAX
        ):
            raise ValueError("shots must be an integer in [1, 2**32)")


# The queries `tensornetwork_qir` will answer: planned, as M2 adds others
# beside `Sample`; signatures name `Sample` until then.
Query = Sample


@dataclass(frozen=True)
class TensorNetworkReport:
    """The run's report: what a :func:`tensornetwork_qir` run cost, over its
    shots.

    :param largest_bond: The largest bond a state held where a run of gates
        ended: what the run's costliest gates paid for.
    :param discarded_weight: The most weight a shot's state lost to
        truncation: an estimate of the fidelity lost, 0 when the run was
        exact (to rounding).
    :param max_bond_dimension: The max bond dimension χ the run used.
    """

    largest_bond: int
    discarded_weight: float
    max_bond_dimension: int


class TensorNetworkResults(tuple):
    """The results of a :func:`tensornetwork_qir` call, one per query in
    order (``rows, = results``), and the run's report, a
    :class:`TensorNetworkReport` (``results.report``)."""

    _report: TensorNetworkReport

    def __new__(
        cls, values: Sequence[object], report: TensorNetworkReport
    ) -> "TensorNetworkResults":
        results = super().__new__(cls, values)
        results._report = report
        return results

    @property
    def report(self) -> TensorNetworkReport:
        """The run's report. Read-only, as the frozen dataclasses beside
        it are."""
        return self._report


@dataclass(frozen=True)
class TensorNetworkSetup:
    """Whether :func:`tensornetwork_qir`'s GPU engine is set up, as
    :func:`check_tensornetwork_setup` found it. ``str()`` gives ``report``.

    :param ready: Whether every step :func:`check_tensornetwork_setup`
        lists found no problem.
    :param report: The setup check's report, as text: what each step
        found, the warnings, and the fix for the first problem.
    :param advice: What to do about the first problem; ``None`` when ready.
    :param warnings: What may go wrong whether or not the setup is ready:
        another copy of a library visible beside the one loaded.
    """

    ready: bool
    report: str
    advice: Optional[str]
    warnings: Tuple[str, ...]

    def __str__(self) -> str:
        return self.report


def tensornetwork_qir(
    input: Union[QirInputData, str, bytes],
    queries: Sequence[Sample],
    *,
    method: Literal["mps"] = "mps",
    options: Optional[MpsOptions] = None,
    seed: Optional[int] = None,
    engine: Literal["gpu", "cpu"] = "gpu",
) -> TensorNetworkResults:
    """Answer ``queries`` on a QIR program simulated with a tensor-network
    simulation method, and report what the run cost.

    Preview: this API may change.

    ::

        results = tensornetwork_qir(qir, [Sample(50)], method="mps",
                                    options=MpsOptions(max_bond_dimension=128), seed=7)
        rows, = results
        results.report.largest_bond, results.report.discarded_weight

    The run leaves the Python thread, and Ctrl-C stops it. Noise is not
    simulated: a noise intrinsic (a function with the ``qdk_noise``
    attribute) runs as identity, as no noise model is given, and a
    readout-noise call or a loss check raises ``ValueError``.

    :param input: The QIR program, Base or Adaptive profile, as text or
        bitcode.
    :param queries: One :class:`Sample`.
    :param method: ``"mps"``, the MPS simulation method: a matrix product
        state on the engine ``engine`` names, each bond at most
        ``options.max_bond_dimension``.
    :param options: :class:`MpsOptions`; ``None`` takes the defaults.
    :param seed: Reproduces the shots: the same seed gives each shot the same
        records, whichever number of shots runs. In [0, 2**32); ``None``
        draws one.
    :param engine: Where the matrix product state lives: ``"gpu"``, the
        canonical MPS engine on CUDA device 0 (cuTensorNet's tensor calls),
        which needs the setup :func:`check_tensornetwork_setup` checks; or
        ``"cpu"``, the same engine on the host CPU, which needs no setup.
    :return: One result per query, in order, with the run's report as
        ``.report``. A run whose truncation lost more than 10⁻³ of a shot's
        weight also warns (``RuntimeWarning``).
    :raises ValueError: For invalid arguments (an unknown ``engine``
        among them), a program the MPS simulation method does not run (a
        readout-noise call, a loss check, a gate on one qubit twice), a max
        bond dimension too large for the program's qubits, or a call the
        engine refuses (an operator on no site, say).
    :raises TypeError: For an argument of the wrong type.
    :raises OSError: When the GPU engine is not set up (no library, a version
        not supported, no device), with the :func:`check_tensornetwork_setup`
        report as the message, after the engine's error when no engine could
        be made, and a last line saying that ``engine="cpu"`` runs without a
        GPU; or when the engine fails during the run, with its error as the
        message.
    :raises RuntimeError: When a shot traps (a division by zero, say); the
        message names the shot.
    """
    return _run(
        input, queries, method=method, options=options, seed=seed, engine=engine
    )


def check_tensornetwork_setup() -> TensorNetworkSetup:
    """Check that the GPU engine of :func:`tensornetwork_qir` is set up, and
    say what to fix if it is not; ``engine="cpu"`` needs no setup. Never
    raises.

    It loads cuTensorNet and the CUDA Runtime (each library's override,
    ``QDK_CUTENSORNET_LIBRARY`` or ``QDK_CUDART_LIBRARY``, alone when set; else
    the copy ``pip install "qdk[tensornetwork]"`` installed; else the
    system's), checks their versions and the driver, reads how many CUDA
    devices there are and CUDA device 0's compute capability and memory, and
    runs one gate on CUDA device 0. It warns of another copy of a library
    visible beside the one loaded.

    ::

        print(check_tensornetwork_setup())
    """
    from .. import _native

    ready, report, advice, native_warnings = _native._check_tensornetwork_setup(
        _packaged_libraries(), _INSTALL_COMMAND
    )
    return TensorNetworkSetup(
        ready=ready, report=report, advice=advice, warnings=tuple(native_warnings)
    )


def _run(
    input: Union[QirInputData, str, bytes],
    queries: Sequence[Sample],
    *,
    method: str,
    options: Optional[MpsOptions],
    seed: Optional[int],
    engine: str,
) -> TensorNetworkResults:
    """:func:`tensornetwork_qir`, its arguments checked here but for
    ``engine``, which the native side checks when it makes the engine.

    The program is lowered as the other QIR simulators lower it
    (``DecomposeCcxPass``, then ``AdaptiveProfilePass(Bytecode.Bit64)``,
    with no noise config); the native side builds it, makes the engine and
    runs the shots; each shot's records are formatted as ``run_qir``
    formats them."""
    if method != "mps":
        raise ValueError(f'Invalid method: {method!r}. Use "mps".')
    if options is None:
        options = MpsOptions()
    elif not isinstance(options, MpsOptions):
        raise TypeError('options must be an MpsOptions instance for method="mps"')
    if seed is not None and (
        isinstance(seed, bool) or not isinstance(seed, int) or not 0 <= seed <= _U32_MAX
    ):
        raise ValueError("seed must be an integer in [0, 2**32) or None")
    queries = list(queries)
    for query in queries:
        if not isinstance(query, Sample):
            raise TypeError(f"Unsupported query {query!r}; use Sample")
    if len(queries) != 1:
        raise ValueError("queries must hold exactly one Sample")
    (sample,) = queries

    from .. import _native

    mod, _, _, seed = preprocess_simulation_input(input, None, None, seed)
    DecomposeCcxPass().run(mod)
    program = AdaptiveProfilePass(Bytecode.Bit64).run(mod).as_dict()
    records, report = _native._tensor_network_sample(
        program,
        sample.shots,
        seed,
        options.max_bond_dimension,
        _packaged_libraries(),
        _INSTALL_COMMAND,
        engine,
    )
    recorder = OutputRecordingPass()
    recorder.run(mod)
    rows = [recorder.process_output(shot) for shot in records]
    if report["truncation_warning"] is not None:
        warnings.warn(report["truncation_warning"], RuntimeWarning, stacklevel=3)
    return TensorNetworkResults(
        [rows],
        TensorNetworkReport(
            largest_bond=report["largest_bond"],
            discarded_weight=report["discarded_weight"],
            max_bond_dimension=report["max_bond_dimension"],
        ),
    )


def _packaged_libraries() -> List[str]:
    """The library files NVIDIA's wheels installed in this environment, for
    instance ``libcutensornet.so.2``, one for each wheel installed that holds
    its file: Python finds them from the installed distributions, so that no
    ``LD_LIBRARY_PATH`` is needed. The native side tries each for the
    library of its file name."""

    def find(distribution: str, file: str) -> Optional[str]:
        try:
            path = str(metadata.distribution(distribution).locate_file(file))
        except metadata.PackageNotFoundError:
            return None
        return path if os.path.isfile(path) else None

    found = (find(*library) for library in _PACKAGED_LIBRARIES)
    return [path for path in found if path is not None]
