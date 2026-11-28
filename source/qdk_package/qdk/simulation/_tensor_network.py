# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tensor-network simulation of QIR programs.

``tensornetwork_qir`` simulates a QIR program's shots with a tensor-network
simulation method. It returns them with the run's report: what the method
measured of the run. Each method takes its own options and gives its own
report. The matrix product state (MPS) method (``method="mps"``) suits
programs with too many qubits for a state vector that entangle them little;
it takes :class:`MpsOptions` and gives an :class:`MpsReport`.

``check_tensornetwork_setup`` says whether the GPU is set up for the GPU
engine (``engine="gpu"``, the default, which needs
``pip install "qdk[tensornetwork]"``).
"""

from dataclasses import dataclass
from importlib import metadata
import os
from typing import (
    Any,
    Callable,
    Dict,
    Iterator,
    List,
    Literal,
    Optional,
    Sequence,
    Tuple,
    Type,
    Union,
    cast,
)
import warnings

import pyqir

from .. import _native
from .._adaptive_pass import AdaptiveProfilePass, Bytecode
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

# The QIR function attribute that marks a noise intrinsic: Q#'s
# `NoiseIntrinsic` callables are compiled with it.
_NOISE_INTRINSIC_ATTRIBUTE = "qdk_noise"

# One past the largest 32-bit unsigned integer (Rust's `u32::MAX + 1`): the
# native side takes shots and the seed as such integers.
_U32_END = 2**32


@dataclass(frozen=True)
class TensorNetworkOptions:
    """The options of a tensor-network simulation method, which
    :func:`tensornetwork_qir` takes as ``options``. Each method's options
    are a subclass of this class, whose fields set how that method runs."""


@dataclass(frozen=True)
class MpsOptions(TensorNetworkOptions):
    """Options of the MPS simulation method (``method="mps"``).

    :param max_bond_dimension: The max bond dimension χ: every bond of the
        matrix product state is at most χ. Larger is more accurate and costs
        more time and memory; the run's :class:`MpsReport`'s
        ``discarded_weight`` says what truncating to it lost. In [1, 2**32).
        Defaults to 128.
    """

    max_bond_dimension: int = 128

    def __post_init__(self) -> None:
        """Refuses a max bond dimension out of its range (``ValueError``)."""
        _check_u32("max_bond_dimension", self.max_bond_dimension, 1)


@dataclass(frozen=True)
class Sample:
    """A query for ``shots`` shots. Its result is a list with one entry per
    shot: that shot's output, in the structure the program records it.

    - One recorded value alone: ``Result`` for a measurement; ``bool``,
      ``int`` or ``float`` for a classical value.
    - A tuple or a list for a tuple or an array record, holding its values
      in recording order.
    - When the program records nothing, its measurements as a string of
      ``"0"`` and ``"1"``.

    :param shots: The number of shots, in [1, 2**32).
    """

    shots: int

    def __post_init__(self) -> None:
        """Refuses a number of shots out of its range (``ValueError``)."""
        _check_u32("shots", self.shots, 1)


# The queries `tensornetwork_qir` answers. Planned: other queries
# (expectation values; the current state, to read it or to run on from it)
# will join `Sample`, and a run will take an initial state in place of all
# zeros (a Néel or Hartree–Fock state, or one another method found).
Query = Sample


@dataclass(frozen=True)
class TensorNetworkReport:
    """The run's report: what the method measured of a
    :func:`tensornetwork_qir` run, over its shots. Each method's report is a
    subclass of this class; a run gives the report of the method it used."""


@dataclass(frozen=True)
class MpsReport(TensorNetworkReport):
    """The report of a run of the MPS simulation method (``method="mps"``).

    :param largest_bond_dimension: The largest bond dimension a state held
        once its gates had run, read before each measurement and at the end
        of each batch (as :func:`tensornetwork_qir` defines it). Partway
        through the gates, a state may hold a larger one.
    :param discarded_weight: The most weight a shot's state lost to
        truncation, an estimate of the fidelity lost. Up to rounding, 0 when
        the run was exact.
    :param max_bond_dimension: The max bond dimension χ the run used.
    """

    largest_bond_dimension: int
    discarded_weight: float
    max_bond_dimension: int


class TensorNetworkResults(tuple):
    """The results of a :func:`tensornetwork_qir` call, one per query in
    order (``rows, = results``), and the run's report (``results.report``).
    Equality is the tuple's: it compares the results, not the reports."""

    # The run's report, which `report` gives.
    _report: TensorNetworkReport

    def __new__(
        cls, values: Sequence[object], report: TensorNetworkReport
    ) -> "TensorNetworkResults":
        """The results `values`, one per query in order, with the run's
        `report`."""
        results = super().__new__(cls, values)
        results._report = report
        return results

    @property
    def report(self) -> TensorNetworkReport:
        """The run's report."""
        return self._report

    def __getnewargs__(self) -> Tuple[Tuple[object, ...], TensorNetworkReport]:
        """The arguments `copy`, `deepcopy` and `pickle` rebuild the results
        with, ``__new__(cls, *__getnewargs__())``: the results and the
        report, which `__new__` needs and tuple's ``__getnewargs__`` does
        not give. The copy's report is then set again from the instance's
        ``__dict__``, its state."""
        return (tuple(self), self._report)

    def __repr__(self) -> str:
        """The results and the report, as the class's constructor takes
        them."""
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
        """The setup check's report, ``report``."""
        return self.report


def tensornetwork_qir(
    input: Union[QirInputData, str, bytes],
    queries: Sequence[Sample],
    *,
    method: Literal["mps"] = "mps",
    options: Optional[TensorNetworkOptions] = None,
    seed: Optional[int] = None,
    engine: Literal["gpu", "cpu"] = "gpu",
) -> TensorNetworkResults:
    """Answer ``queries`` on a QIR program simulated with a tensor-network
    simulation method, and report what the method measured of the run.

    Preview: this API may change.

    ::

        results = tensornetwork_qir(qir, [Sample(50)], method="mps",
                                    options=MpsOptions(max_bond_dimension=128), seed=7)
        rows, = results
        report = results.report  # an MpsReport, as method="mps" gives
        report.largest_bond_dimension, report.discarded_weight

    The run leaves the Python thread, and Ctrl-C stops it between batches
    of gates. A batch holds the gates and measurements a program runs until
    it next reads or records a result the batch measured into, or writes
    one again. A Base profile program that measures into each result once,
    and records its results after its last measurement, is one batch.
    By the MPS simulation method, Ctrl-C during such a batch lets the group
    of shots under way finish it, and stops the groups its measurements
    split off before they run on.

    Noise is not supported yet (planned). A call to a noise intrinsic (a
    function with the ``qdk_noise`` attribute) is refused. The MPS
    simulation method also refuses a readout-noise call and a qubit's loss
    check (``__quantum__qis__peek_loss__body``). Each raises ``ValueError``
    (below).

    :param input: The QIR program, Base or Adaptive profile, as text or
        bitcode.
    :param queries: One :class:`Sample`.
    :param method: The tensor-network simulation method, by name:
        ``"mps"``, the MPS simulation method, a matrix product state on the
        engine ``engine`` names, each bond at most
        ``options.max_bond_dimension``.
    :param options: The method's options, an instance of its subclass of
        :class:`TensorNetworkOptions` (:class:`MpsOptions` for ``"mps"``);
        ``None`` takes the method's defaults.
    :param seed: Reproduces the shots. Exactly on the CPU engine, and to
        rounding on the GPU engine, a seed gives each shot the same records
        whatever the number of shots. In [0, 2**32); ``None`` draws one.
    :param engine: Where the method's tensors live, both engines running the
        same method: ``"gpu"``, an NVIDIA GPU through NVIDIA's cuTensorNet
        library, on CUDA device 0, the first GPU the process sees
        (``CUDA_VISIBLE_DEVICES`` selects which), which needs the setup
        :func:`check_tensornetwork_setup` checks; or ``"cpu"``, the host
        CPU, which needs no setup.
    :return: One result per query, in order, with the run's report as
        ``.report``: the method's subclass of :class:`TensorNetworkReport`
        (an :class:`MpsReport` for ``"mps"``). A run of the MPS simulation
        method whose truncation lost more of a shot's weight than the
        method tolerates also warns (``RuntimeWarning``, whose message names
        the max bond dimension to raise).
    :raises ValueError: For an argument out of its range: a ``method``
        other than ``"mps"``, an ``engine`` other than ``"gpu"`` and
        ``"cpu"``, a ``seed`` that is not an ``int`` in [0, 2**32) (``1.5``,
        ``True`` or ``"7"``, say), or ``queries`` not holding exactly one
        query. For a program the run does not take: a noise intrinsic; by
        the MPS simulation method, a readout-noise call, a qubit's loss
        check or a two-qubit gate given one qubit twice; a max bond dimension too large
        for the program's qubits; or a call the engine refuses (a rotation
        by an angle that is not finite, say). When the GPU engine cannot be
        made, its ``OSError`` comes before the program faults found once the
        engine is made.
    :raises TypeError: When ``options`` is not the method's options (an
        :class:`MpsOptions` for ``"mps"``), ``queries`` is not iterable (a
        bare :class:`Sample`, say), a query is not a :class:`Sample`,
        ``engine`` is not a ``str``, or ``input`` is not text, bitcode or a
        ``QirInputData``.
    :raises OSError: When the GPU libraries are not set up (none found, or
        a version not supported, say): the message is the
        :func:`check_tensornetwork_setup` report. When they were found but
        no engine could be made (no device, say): the engine's error, then
        that report. Both end with a line saying that ``engine="cpu"`` runs
        without a GPU. When the engine fails during the run: its error,
        with the errors that caused it.
    :raises RuntimeError: When a shot traps (a division by zero, say); the
        message names the shot.
    """
    # The arguments are checked here but for `engine`, which the native side
    # checks when it picks the engine, once the program is built.
    sample_shots, options = _method_and_options(method, options)
    if seed is not None:
        _check_u32("seed", seed, 0)
    sample = _one_sample(queries)
    mod, program, seed = _lowered_program(input, seed)
    records, report = sample_shots(program, sample.shots, seed, options, engine)
    return TensorNetworkResults([_rows(mod, records)], report)


def check_tensornetwork_setup() -> TensorNetworkSetup:
    """Check that the GPU engine of :func:`tensornetwork_qir` is set up, and
    say what to fix if it is not; ``engine="cpu"`` needs no setup. Never
    raises.

    On Linux x86-64, the one platform the GPU engine runs on, it checks the
    libraries (each library's override, ``QDK_CUTENSORNET_LIBRARY`` or
    ``QDK_CUDART_LIBRARY``, alone when set; else the copy
    ``pip install "qdk[tensornetwork]"`` installed; else the system's), the
    driver, CUDA device 0 and a two-qubit trial run on that device, as the
    report it returns lists them. Elsewhere, the report says the platform is
    not supported, and nothing else is read.

    ::

        print(check_tensornetwork_setup())
    """
    ready, report, advice, native_warnings = _native._check_tensornetwork_setup(
        _packaged_libraries(), _INSTALL_COMMAND
    )
    return TensorNetworkSetup(
        ready=ready, report=report, advice=advice, warnings=tuple(native_warnings)
    )


# The function that samples a lowered program's shots by a method: given the
# program (the dict `AdaptiveProfilePass(Bytecode.Bit64)` writes), the shot
# count, the seed, the method's options and the engine's name, it gives each
# shot's output records and the method's report.
_SampleShots = Callable[
    [Dict[str, Any], int, int, Any, str],
    Tuple[List[List[Any]], TensorNetworkReport],
]


def _sample_mps(
    program: Dict[str, Any], shots: int, seed: int, options: MpsOptions, engine: str
) -> Tuple[List[List[Any]], MpsReport]:
    """Samples `shots` shots of `program` by the MPS simulation method, on
    the engine named `engine`, and warns when truncation lost more of a
    shot's weight than the method tolerates (``TRUNCATION_TOLERANCE``, in
    the Rust crate ``qdk_simulators``'s ``mps_simulator``)."""
    records, report = _native._sample_mps(
        program,
        shots,
        seed,
        options.max_bond_dimension,
        _packaged_libraries() if engine == "gpu" else [],
        _INSTALL_COMMAND,
        engine,
    )
    if report["truncation_warning"] is not None:
        # Level 3: the line that called `tensornetwork_qir`, which calls this.
        warnings.warn(report["truncation_warning"], RuntimeWarning, stacklevel=3)
    return records, MpsReport(
        largest_bond_dimension=report["largest_bond_dimension"],
        discarded_weight=report["discarded_weight"],
        max_bond_dimension=report["max_bond_dimension"],
    )


# The tensor-network simulation methods `tensornetwork_qir` runs, by the name
# its `method` takes: each method's options class and its sampling function.
# A method joins with its options class, its report class, its sampling
# function and an entry here.
_METHODS: Dict[str, Tuple[Type[TensorNetworkOptions], _SampleShots]] = {
    "mps": (MpsOptions, _sample_mps),
}


def _method_and_options(
    method: str, options: Optional[TensorNetworkOptions]
) -> Tuple[_SampleShots, TensorNetworkOptions]:
    """The sampling function of the method named `method`, and the options
    it runs with: `options`, or the method's defaults for ``None``. Refuses
    an unknown method (``ValueError``) and another method's options
    (``TypeError``)."""
    if method not in _METHODS:
        names = " or ".join(f'"{name}"' for name in _METHODS)
        raise ValueError(f"Invalid method: {method!r}. Use {names}.")
    options_type, sample_shots = _METHODS[method]
    if options is None:
        return sample_shots, options_type()
    if not isinstance(options, options_type):
        raise TypeError(
            f"options must be an instance of {options_type.__name__} "
            f'for method="{method}"'
        )
    return sample_shots, options


def _one_sample(queries: Sequence[Sample]) -> Sample:
    """The one query `queries` holds, a :class:`Sample`. Refuses queries
    that are not iterable or not a ``Sample`` (``TypeError``), and any
    number of queries but one (``ValueError``)."""
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
    return sample


def _lowered_program(
    input: Union[QirInputData, str, bytes], seed: Optional[int]
) -> Tuple[pyqir.Module, Dict[str, Any], int]:
    """`input` lowered as `run_qir`'s CPU simulators lower an Adaptive
    program: its module, the dict the native side builds the program from,
    and the seed (`seed`, or one drawn for ``None``). The lowering takes no
    noise config, so it would run a noise intrinsic as identity: a call to
    one is refused first."""
    mod, _, _, seed = preprocess_simulation_input(input, None, None, seed)
    _refuse_noise_intrinsics(mod)
    DecomposeCcxPass().run(mod)
    program = AdaptiveProfilePass(Bytecode.Bit64).run(mod).as_dict()
    return mod, program, seed


def _rows(mod: pyqir.Module, records: List[List[Any]]) -> List[Any]:
    """Each shot's output, its `records` formatted as `run_qir` formats
    them, by the output recording of `mod`, the lowered program."""
    recorder = OutputRecordingPass()
    recorder.run(mod)
    return [recorder.process_output(shot) for shot in records]


def _refuse_noise_intrinsics(mod: pyqir.Module) -> None:
    """Refuses a program that calls a noise intrinsic (a function with the
    ``qdk_noise`` attribute), naming the intrinsic. ``tensornetwork_qir``
    lowers a program with no noise config, which would run a noise
    intrinsic as identity."""
    for callee in _callees(mod):
        if _NOISE_INTRINSIC_ATTRIBUTE in callee.attributes.func:
            raise ValueError(
                f"Unsupported noise intrinsic: {callee.name}. Noise is not "
                "yet supported by tensor-network simulation; it is planned."
            )


def _callees(mod: pyqir.Module) -> Iterator[pyqir.Function]:
    """The function each call in `mod` calls, for each call whose callee
    is a function, not a pointer."""
    for function in mod.functions:
        for block in function.basic_blocks:
            for instruction in block.instructions:
                if instruction.opcode != pyqir.Opcode.CALL:
                    continue
                callee = cast(pyqir.Call, instruction).callee
                if isinstance(callee, pyqir.Function):
                    yield callee


def _packaged_libraries() -> List[str]:
    """The paths of the library files NVIDIA's wheels installed in this
    environment, one per installed wheel that holds its file (for instance
    ``<site-packages>/cuquantum/lib/libcutensornet.so.2``). Python finds them
    from the installed distributions, so no ``LD_LIBRARY_PATH`` is needed.
    The native side tries each for the library of its file name."""

    def find(distribution: str, file: str) -> Optional[str]:
        """The path of `file` in the installed `distribution`; ``None``
        when the distribution is not installed or does not hold the file."""
        try:
            path = str(metadata.distribution(distribution).locate_file(file))
        except metadata.PackageNotFoundError:
            return None
        return path if os.path.isfile(path) else None

    found = (find(*library) for library in _PACKAGED_LIBRARIES)
    return [path for path in found if path is not None]


def _check_u32(name: str, value: object, lowest: int) -> None:
    """Refuses `value` unless it is an `int` (not a `bool`) in
    [`lowest`, `_U32_END`), the native side's bound for shots and the seed.
    The max bond dimension shares that bound, which no run can reach."""
    if (
        isinstance(value, bool)
        or not isinstance(value, int)
        or not lowest <= value < _U32_END
    ):
        raise ValueError(f"{name} must be an integer in [{lowest}, 2**32)")
