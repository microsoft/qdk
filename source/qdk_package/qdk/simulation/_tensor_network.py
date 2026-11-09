# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""``tensornetwork_qir``: the shots of a QIR program simulated with a
tensor-network method, returned with the run's report; and
``check_tensornetwork_setup``, which says whether the GPU is set up for it.

A chemistry user runs programs too entangled for a sparse simulator and too
large for a state vector, such as H₂'s phase estimation (29 qubits, Adaptive
profile), on a matrix product state (MPS) on an NVIDIA GPU. The method's
cost and accuracy depend on the max bond dimension χ, so every call returns, beside
its results, the run's report: the largest bond reached and the weight
truncation discarded; a run that lost more than 10⁻³ also raises a
``RuntimeWarning`` saying to raise χ.

How: the program is lowered as the other QIR simulators lower it
(``DecomposeCcxPass``, then ``AdaptiveProfilePass(Bytecode.Bit64)``); the
native side (``src/qir_simulation/tensor_network.rs``) builds it, finds
cuTensorNet (with the paths of NVIDIA's wheels found here,
``_packaged_libraries``) and runs the shots with the GIL released, Ctrl-C
stopping the run within one batch; each shot's records are formatted as
``run_qir`` formats them. The same core will serve ``run_qir(type="mps")``.

The method runs on the GPU only: install it with
``pip install "qdk[tensornetwork]"`` (Linux x86-64, an NVIDIA GPU of compute
capability 7.5 or later, a driver for CUDA 12). A setup problem raises
``OSError`` with the text ``check_tensornetwork_setup()`` reports. Noise is
not supported.
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


@dataclass(frozen=True)
class MpsOptions:
    """Options of the MPS method (``method="mps"``).

    :param max_bond_dimension: The max bond dimension χ: every bond of the
        matrix product state is at most χ. Larger is more accurate and costs
        more time and memory; the report's ``discarded_weight`` says what
        truncating to it lost.
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


Query = Sample


@dataclass(frozen=True)
class TensorNetworkReport:
    """What a :func:`tensornetwork_qir` run cost, over its shots.

    :param largest_bond: The largest bond a state held where a run of gates
        ended: what the run's costliest gates paid for.
    :param discarded_weight: The most weight a shot's state lost to
        truncation (0 when the run was exact, to rounding). The readouts may
        be off by up to about its square root in total variation distance.
    :param max_bond_dimension: The max bond dimension χ the run used.
    """

    largest_bond: int
    discarded_weight: float
    max_bond_dimension: int


class TensorNetworkResults(tuple):
    """The results of a :func:`tensornetwork_qir` call, one per query in
    order (``rows, = results``), and the run's :class:`TensorNetworkReport`
    (``results.report``)."""

    report: TensorNetworkReport

    def __new__(
        cls, values: Sequence[object], report: TensorNetworkReport
    ) -> "TensorNetworkResults":
        results = super().__new__(cls, values)
        results.report = report
        return results


@dataclass(frozen=True)
class TensorNetworkSetup:
    """Whether :func:`tensornetwork_qir`'s GPU engine is set up, as
    :func:`check_tensornetwork_setup` found it. ``str()`` gives ``report``.

    :param ready: Whether no problem was found: the libraries load, the
        driver supports CUDA 12, device 0 has compute capability 7.5 or
        later, and an engine ran one gate on it.
    :param report: The libraries loaded with their versions, the CUDA
        version the driver supports, each device's compute capability and
        free and total memory, the engine's check, the warnings, and the fix
        for the first problem.
    :param advice: What to do about the first problem; ``None`` when ready.
    :param warnings: What may go wrong though the setup is ready: another
        copy of a library visible beside the one loaded.
    """

    ready: bool
    report: str
    advice: Optional[str]
    warnings: Tuple[str, ...]

    def __str__(self) -> str:
        return self.report


def tensornetwork_qir(
    input: Union[QirInputData, str, bytes],
    queries: Sequence[Query],
    *,
    method: Literal["mps"] = "mps",
    options: Optional[MpsOptions] = None,
    seed: Optional[int] = None,
) -> TensorNetworkResults:
    """Answer ``queries`` on a QIR program simulated with a tensor-network
    method, and report what the run cost.

    Preview: this API may change.

    ::

        results = tensornetwork_qir(qir, [Sample(50)], method="mps",
                                    options=MpsOptions(max_bond_dimension=128), seed=7)
        rows, = results
        results.report.largest_bond, results.report.discarded_weight

    :param input: The QIR program, Base or Adaptive profile, as text or
        bitcode.
    :param queries: One :class:`Sample`.
    :param method: ``"mps"``: a matrix product state on an NVIDIA GPU
        (cuTensorNet), each bond at most ``options.max_bond_dimension``.
    :param options: :class:`MpsOptions`; ``None`` takes the defaults.
    :param seed: Reproduces the shots: the same seed gives each shot the same
        records, whichever number of shots runs. In [0, 2**32); ``None``
        draws one.
    :return: One result per query, in order, with the run's report as
        ``.report``. A run whose truncation lost more than 10⁻³ of a shot's
        weight also warns (``RuntimeWarning``).
    :raises ValueError: For invalid arguments, a program the method does not
        run (a noise call, a gate on one qubit twice), or a max bond
        dimension too large for the program's qubits.
    :raises TypeError: For an argument of the wrong type.
    :raises OSError: When the GPU engine is not set up (no library, a version
        not supported, no device) or the device fails; the message is the
        :func:`check_tensornetwork_setup` report.
    :raises RuntimeError: When a shot traps (a division by zero, say); the
        message names the shot.
    """
    return _run(
        input, queries, method=method, options=options, seed=seed, engine="cutensornet"
    )


def check_tensornetwork_setup() -> TensorNetworkSetup:
    """Check that :func:`tensornetwork_qir`'s GPU engine is set up, and say
    what to fix if it is not. Never raises.

    It loads cuTensorNet and the CUDA Runtime (each library's override,
    ``QDK_CUTENSORNET_LIBRARY`` or ``QDK_CUDART_LIBRARY``, alone when set; else
    the copy ``pip install "qdk[tensornetwork]"`` installed; else the
    system's), checks their versions, reads each CUDA device's compute
    capability and memory, and runs one gate on CUDA device 0. It warns of
    another copy of a library visible beside the one loaded.

    ::

        print(check_tensornetwork_setup())
    """
    from .. import _native

    ready, report, advice, warnings = _native._check_tensornetwork_setup(
        _packaged_libraries()
    )
    return TensorNetworkSetup(
        ready=ready, report=report, advice=advice, warnings=tuple(warnings)
    )


def _run(
    input: Union[QirInputData, str, bytes],
    queries: Sequence[Query],
    *,
    method: str,
    options: Optional[MpsOptions],
    seed: Optional[int],
    engine: str,
) -> TensorNetworkResults:
    """:func:`tensornetwork_qir` on the engine named ``engine``:
    ``"cutensornet"``, or ``"reference"``, the exact reference, which the
    package's tests run on hosts with no GPU."""
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


def _packaged_libraries() -> Tuple[Optional[str], Optional[str]]:
    """The paths of ``libcutensornet.so.2`` and ``libcudart.so.12`` that
    NVIDIA's wheels installed in this environment, each ``None`` when its
    wheel is not installed: Python finds them from the installed
    distributions, so that no ``LD_LIBRARY_PATH`` is needed."""

    def find(distribution: str, file: str) -> Optional[str]:
        try:
            path = str(metadata.distribution(distribution).locate_file(file))
        except metadata.PackageNotFoundError:
            return None
        return path if os.path.isfile(path) else None

    found: List[Optional[str]] = [find(*library) for library in _PACKAGED_LIBRARIES]
    return found[0], found[1]
