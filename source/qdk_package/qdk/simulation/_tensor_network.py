# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""``tensornetwork_qir`` simulates a QIR program's shots on a matrix product
state (MPS), for programs with too many qubits for a state vector that
entangle them little, and returns them with the run's report: the largest
bond it held and the weight truncation discarded.
``check_tensornetwork_setup`` says whether the GPU is set up for the GPU
engine (``engine="gpu"``, the default, which needs
``pip install "qdk[tensornetwork]"``).
"""

from dataclasses import dataclass
from importlib import metadata
import os
from typing import List, Literal, Optional, Sequence, Tuple, Union, cast
import warnings

import pyqir

from .. import _native
from .._adaptive_pass import AdaptiveProfilePass, Bytecode
from .._native import _MPS_DEFAULT_MAX_BOND_DIMENSION
from .._types import QirInputData
from ._simulation import (
    DecomposeCcxPass,
    OutputRecordingPass,
    preprocess_simulation_input,
)

# The NVIDIA wheels of `qdk[tensornetwork]` whose libraries the QDK opens,
# and where each puts its library, relative to the environment's
# site-packages. The extra's cuBLAS and cuSOLVER wheels are not listed:
# cuTensorNet's library finds them itself, in their wheels' folders.
_PACKAGED_LIBRARIES = (
    ("cutensornet-cu12", "cuquantum/lib/libcutensornet.so.2"),
    ("nvidia-cuda-runtime-cu12", "nvidia/cuda_runtime/lib/libcudart.so.12"),
)

# The command that installs the wheels above, which the setup check's advice
# gives when a library is missing.
_INSTALL_COMMAND = 'pip install "qdk[tensornetwork]"'


@dataclass(frozen=True)
class MpsOptions:
    """Options of the MPS simulation method (``method="mps"``), which
    :func:`tensornetwork_qir` takes as ``options``. They set how accurate,
    and so how costly, a run is.

    :param max_bond_dimension: The max bond dimension χ: every bond of the
        matrix product state is at most χ. Larger is more accurate and costs
        more time and memory; the run's report's ``discarded_weight`` says
        what truncating to it lost.
    """

    max_bond_dimension: int = _MPS_DEFAULT_MAX_BOND_DIMENSION

    def __post_init__(self) -> None:
        _check_u32("max_bond_dimension", self.max_bond_dimension, 1)


@dataclass(frozen=True)
class Sample:
    """A query for ``shots`` shots; its result is a list with one entry per
    shot, that shot's output in the structure the program records it: one
    recorded value alone (``Result`` for a measurement; ``bool``, ``int``
    or ``float`` for a classical value); a tuple or a list for a tuple or
    an array record, holding its values in recording order; or, when the
    program records nothing, its measurements as a string of ``"0"`` and
    ``"1"``.

    :param shots: The number of shots, in [1, 2**32).
    """

    shots: int

    def __post_init__(self) -> None:
        _check_u32("shots", self.shots, 1)


# The queries `tensornetwork_qir` answers. Planned: other queries
# (expectation values; the current state, to read it or to run on from it)
# will join `Sample`, and a run will take an initial state in place of all
# zeros (a Néel or Hartree–Fock state, or one another method found).
Query = Sample


@dataclass(frozen=True)
class TensorNetworkReport:
    """The run's report: what a :func:`tensornetwork_qir` run cost, over its
    shots.

    :param largest_bond: The largest bond a state held at the end of a
        sequence of gates, before the measurements after it; a gate inside
        a sequence may hold a larger one.
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
    :class:`TensorNetworkReport` (``results.report``). Equality is the
    tuple's: it compares the results, not the reports."""

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

    def __getnewargs__(self) -> Tuple[Tuple[object, ...], TensorNetworkReport]:
        # `copy`, `deepcopy` and `pickle` rebuild a tuple subclass with
        # `__new__(cls, *__getnewargs__())`; tuple's gives no report.
        return (tuple(self), self._report)

    def __repr__(self) -> str:
        return f"TensorNetworkResults({tuple.__repr__(self)}, report={self._report!r})"


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

    The run leaves the Python thread, and Ctrl-C stops it between batches
    of gates (a batch: the gates and measurements a program runs until it
    next reads or records a result the batch measured into, or writes one
    again). A Base profile program that measures into each result once and
    records its results only after its last measurement is one batch.
    Ctrl-C during such a batch lets the group of shots under way finish it,
    and stops the groups the batch's measurements split off before they run
    on.

    Noise is not yet supported by this method, and is planned: a call to a
    noise intrinsic (a function with the ``qdk_noise`` attribute) or to
    readout noise, or a loss check, raises ``ValueError``.

    :param input: The QIR program, Base or Adaptive profile, as text or
        bitcode.
    :param queries: One :class:`Sample`.
    :param method: ``"mps"``, the MPS simulation method: a matrix product
        state on the engine ``engine`` names, each bond at most
        ``options.max_bond_dimension``.
    :param options: :class:`MpsOptions`; ``None`` takes the defaults.
    :param seed: Reproduces the shots: the same seed gives each shot the same
        records, whichever number of shots runs: exactly on the CPU engine;
        on the GPU engine, to rounding (a draw that rounding decides may
        differ). In [0, 2**32); ``None`` draws one.
    :param engine: Where the matrix product state lives, both engines
        running the same method: ``"gpu"``, an NVIDIA GPU through NVIDIA's
        cuTensorNet library, on CUDA device 0, the first GPU the process
        sees (``CUDA_VISIBLE_DEVICES`` selects which), which needs the setup
        :func:`check_tensornetwork_setup` checks; or ``"cpu"``, the host
        CPU, which needs no setup.
    :return: One result per query, in order, with the run's report as
        ``.report``. A run whose truncation lost more of a shot's weight
        than the method tolerates also warns (``RuntimeWarning``, whose
        message gives the loss and the tolerance).
    :raises ValueError: For a ``method`` other than ``"mps"``, a ``seed``
        that is not an ``int`` in [0, 2**32) (``1.5``, ``True`` or ``"7"``,
        say), an ``engine`` other than ``"gpu"`` and ``"cpu"``, ``queries``
        not holding exactly one query, a program the MPS simulation method
        does not run (a noise intrinsic, a readout-noise call, a loss check,
        a two-qubit gate given one qubit twice), a max bond dimension too
        large for the program's qubits, or a call the engine refuses (a
        rotation by an angle that is not finite, say).
    :raises TypeError: When ``options`` is not an :class:`MpsOptions`,
        ``queries`` is not iterable (a bare :class:`Sample`, say), a query
        is not a :class:`Sample`, ``engine`` is not a ``str``, or ``input``
        is not text, bitcode or a ``QirInputData``.
    :raises OSError: When the GPU libraries are not set up (none found, a
        version not supported): the message is the
        :func:`check_tensornetwork_setup` report. When they were found but
        no engine could be made (no device, say): the engine's error, then
        that report. Both end with a line saying that ``engine="cpu"`` runs
        without a GPU. When the engine fails during the run: its error,
        with the errors that caused it.
    :raises RuntimeError: When a shot traps (a division by zero, say); the
        message names the shot.
    """
    # The arguments are checked here but for `engine`, which the native side
    # checks when it makes the engine.
    if method != "mps":
        raise ValueError(f'Invalid method: {method!r}. Use "mps".')
    if options is None:
        options = MpsOptions()
    elif not isinstance(options, MpsOptions):
        raise TypeError('options must be an MpsOptions instance for method="mps"')
    if seed is not None:
        _check_u32("seed", seed, 0)
    try:
        iterator = iter(queries)
    except TypeError:
        raise TypeError("queries must be an iterable holding one Sample") from None
    queries = list(iterator)
    for query in queries:
        if not isinstance(query, Sample):
            raise TypeError(f"Unsupported query {query!r}; use Sample")
    if len(queries) != 1:
        raise ValueError("queries must hold exactly one Sample")
    (sample,) = queries

    # Lowered as `run_qir`'s CPU simulators lower an Adaptive program, with
    # no noise config, which runs a noise intrinsic as identity: so a call
    # to one is refused first. The native side builds the program, makes
    # the engine and runs the shots; each shot's records are formatted as
    # `run_qir` formats them.
    mod, _, _, seed = preprocess_simulation_input(input, None, None, seed)
    _refuse_noise_intrinsics(mod)
    DecomposeCcxPass().run(mod)
    program = AdaptiveProfilePass(Bytecode.Bit64).run(mod).as_dict()
    records, report = _native._tensor_network_sample(
        program,
        sample.shots,
        seed,
        options.max_bond_dimension,
        _packaged_libraries() if engine == "gpu" else [],
        _INSTALL_COMMAND,
        engine,
    )
    recorder = OutputRecordingPass()
    recorder.run(mod)
    rows = [recorder.process_output(shot) for shot in records]
    if report["truncation_warning"] is not None:
        warnings.warn(report["truncation_warning"], RuntimeWarning, stacklevel=2)
    return TensorNetworkResults(
        [rows],
        TensorNetworkReport(
            largest_bond=report["largest_bond"],
            discarded_weight=report["discarded_weight"],
            max_bond_dimension=report["max_bond_dimension"],
        ),
    )


def check_tensornetwork_setup() -> TensorNetworkSetup:
    """Check that the GPU engine of :func:`tensornetwork_qir` is set up, and
    say what to fix if it is not; ``engine="cpu"`` needs no setup. Never
    raises.

    It checks the libraries (each library's override,
    ``QDK_CUTENSORNET_LIBRARY`` or ``QDK_CUDART_LIBRARY``, alone when set; else
    the copy ``pip install "qdk[tensornetwork]"`` installed; else the
    system's), the driver, CUDA device 0 and a one-gate trial run, as the
    report it returns lists them.

    ::

        print(check_tensornetwork_setup())
    """
    ready, report, advice, native_warnings = _native._check_tensornetwork_setup(
        _packaged_libraries(), _INSTALL_COMMAND
    )
    return TensorNetworkSetup(
        ready=ready, report=report, advice=advice, warnings=tuple(native_warnings)
    )


def _refuse_noise_intrinsics(mod: pyqir.Module) -> None:
    """Refuses a program that calls a noise intrinsic (a function with the
    ``qdk_noise`` attribute), naming the intrinsic: the MPS simulation
    method runs no noise yet."""
    for function in mod.functions:
        for block in function.basic_blocks:
            for instruction in block.instructions:
                if instruction.opcode != pyqir.Opcode.CALL:
                    continue
                callee = cast(pyqir.Call, instruction).callee
                if (
                    isinstance(callee, pyqir.Function)
                    and "qdk_noise" in callee.attributes.func
                ):
                    raise ValueError(
                        f"Unsupported noise intrinsic: {callee.name}. Noise is not "
                        "yet supported by the MPS simulation method; it is planned."
                    )


def _packaged_libraries() -> List[str]:
    """The paths of the library files NVIDIA's wheels installed in this
    environment, for instance
    ``<site-packages>/cuquantum/lib/libcutensornet.so.2``, one for each wheel
    installed that holds its file: Python finds them from the installed
    distributions, so that no ``LD_LIBRARY_PATH`` is needed. The native side
    tries each for the library of its file name."""

    def find(distribution: str, file: str) -> Optional[str]:
        try:
            path = str(metadata.distribution(distribution).locate_file(file))
        except metadata.PackageNotFoundError:
            return None
        return path if os.path.isfile(path) else None

    found = (find(*library) for library in _PACKAGED_LIBRARIES)
    return [path for path in found if path is not None]


def _check_u32(name: str, value: object, lowest: int) -> None:
    """Refuses `value` unless it is an `int` (not a `bool`) in
    [`lowest`, 2**32). The native side takes shots and the seed as 32-bit
    unsigned integers; the max bond dimension shares their bound, which no
    run reaches: a bond of 2**32 has at least 32 sites on each side, so the
    tensor of a site beside it holds at least 2**64 numbers."""
    if (
        isinstance(value, bool)
        or not isinstance(value, int)
        or not lowest <= value < 2**32
    ):
        raise ValueError(f"{name} must be an integer in [{lowest}, 2**32)")
