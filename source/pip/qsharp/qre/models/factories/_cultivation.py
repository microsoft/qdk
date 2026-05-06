from __future__ import annotations

import itertools
import sys
from dataclasses import KW_ONLY, dataclass, field
from math import ceil
from pathlib import Path
from typing import Generator

from ... import (
    ISA,
    ISARequirements,
    ISATransform,
    LOGICAL,
    PHYSICAL,
    ConstraintBound,
    constraint,
)
from ..._architecture import ISAContext
from ...instruction_ids import CNOT, T


@dataclass
class GSJ24Factory(ISATransform):
    """
    Implements the magic state cultivation factory from Gidney, Shutty, and
    Jones (2024) for producing logical |T⟩ states from physical-level
    operations.

    Magic state cultivation gradually grows the size and reliability of a
    magic state within a surface code patch, using roughly the same number of
    physical gates as a lattice surgery CNOT gate of equivalent reliability.
    The approach refines ideas from Knill (1996), Jones (2016), Chamberland
    (2020), Gidney (2023/2024), Bombin (2024), and Hirano (2024).

    Compared to prior magic state distillation approaches, cultivation uses an
    order of magnitude fewer qubit-rounds to reach logical error rates as low
    as 2·10⁻⁹ under 10⁻³ uniform depolarizing circuit noise. Halving the
    circuit noise to 5·10⁻⁴ improves the achievable logical error rate to
    4·10⁻¹¹.

    The factory is parameterized by pre-computed simulation data (from Monte
    Carlo sampling at https://doi.org/10.5281/zenodo.13777072) that maps
    physical error rates to (logical_error, num_qubits, volume, steps) tuples
    for supported distance pairs.

    Attributes:
        syndrome_extraction_depth: Number of surface code cycles needed per
            syndrome extraction round. Defaults to 4.
        passthrough: If True, the output ISA includes the input (physical)
            ISA instructions alongside the produced logical T states. If
            False (default), only the logical T states are provided.

    Hyper parameters:
        distance: Tuple (d_color, d_surface) specifying the color code
            distance and surface code distance used in the cultivation
            protocol. Supported values are (3, 15) and (5, 15). Larger
            color code distance (5 vs 3) yields lower logical error rates
            at the cost of higher qubit count and more time steps.

    Reference:
        - C. Gidney, C. Shutty, C. Jones, "Magic state cultivation: growing
          T states with 78% reduced overhead", arXiv:2409.17595 (2024).
          https://arxiv.org/abs/2409.17595
        - Simulation data: https://doi.org/10.5281/zenodo.13777072
    """

    syndrome_extraction_depth: int = 4
    passthrough: bool = False
    _: KW_ONLY
    distance: tuple[int, int] = field(
        default=(3, 15), metadata={"domain": [(3, 15), (5, 15)]}
    )

    def __post_init__(self):
        # Generated using the `_extract_data_from_simulations` function below
        # on the simulation data available at
        # https://doi.org/10.5281/zenodo.13777072
        self._data = {
            0.0005: {
                (3, 15): [
                    _Entry(
                        logical_error=2.9973593146121454e-07,
                        num_qubits=454,
                        volume=4433.630050313343,
                        steps=10,
                    ),
                    _Entry(
                        logical_error=3.9483871833384884e-07,
                        num_qubits=454,
                        volume=3963.1083605426047,
                        steps=9,
                    ),
                    _Entry(
                        logical_error=8.243994277120638e-07,
                        num_qubits=454,
                        volume=3564.499596930529,
                        steps=8,
                    ),
                    _Entry(
                        logical_error=1.292533506995642e-05,
                        num_qubits=454,
                        volume=3199.812603871103,
                        steps=8,
                    ),
                ],
                (5, 15): [
                    _Entry(
                        logical_error=4.325222592210133e-11,
                        num_qubits=463,
                        volume=18963.460657305823,
                        steps=41,
                    ),
                    _Entry(
                        logical_error=5.029777185838997e-11,
                        num_qubits=463,
                        volume=14701.668294938665,
                        steps=32,
                    ),
                    _Entry(
                        logical_error=5.976200770910425e-11,
                        num_qubits=463,
                        volume=13100.99605086575,
                        steps=29,
                    ),
                    _Entry(
                        logical_error=2.1473703555519034e-10,
                        num_qubits=463,
                        volume=11768.635135540251,
                        steps=26,
                    ),
                    _Entry(
                        logical_error=4.992706545478968e-10,
                        num_qubits=463,
                        volume=10423.795388030996,
                        steps=23,
                    ),
                    _Entry(
                        logical_error=1.0628008992770944e-09,
                        num_qubits=463,
                        volume=9227.194331685558,
                        steps=20,
                    ),
                    _Entry(
                        logical_error=1.147511919182804e-08,
                        num_qubits=463,
                        volume=8214.104892130046,
                        steps=18,
                    ),
                    _Entry(
                        logical_error=3.194642984285564e-08,
                        num_qubits=463,
                        volume=7375.755270727277,
                        steps=16,
                    ),
                ],
            },
            0.001: {
                (3, 15): [
                    _Entry(
                        logical_error=2.652826930675648e-06,
                        num_qubits=454,
                        volume=8070.606179632966,
                        steps=18,
                    ),
                    _Entry(
                        logical_error=2.9755933597311195e-06,
                        num_qubits=454,
                        volume=7067.132487830089,
                        steps=16,
                    ),
                    _Entry(
                        logical_error=3.3574635945939254e-06,
                        num_qubits=454,
                        volume=6284.105524329454,
                        steps=14,
                    ),
                    _Entry(
                        logical_error=4.14698465838952e-06,
                        num_qubits=454,
                        volume=5572.013837117182,
                        steps=13,
                    ),
                    _Entry(
                        logical_error=6.805514929263812e-06,
                        num_qubits=454,
                        volume=4948.483284091285,
                        steps=11,
                    ),
                    _Entry(
                        logical_error=2.9308873206642093e-05,
                        num_qubits=454,
                        volume=4291.075883865169,
                        steps=10,
                    ),
                    _Entry(
                        logical_error=6.425491255432556e-05,
                        num_qubits=454,
                        volume=3861.3125109633347,
                        steps=9,
                    ),
                    _Entry(
                        logical_error=0.0006441468000838591,
                        num_qubits=454,
                        volume=3466.8965131924924,
                        steps=8,
                    ),
                ],
                (5, 15): [
                    _Entry(
                        logical_error=7.885200415876903e-10,
                        num_qubits=463,
                        volume=127164.3639761698,
                        steps=275,
                    ),
                    _Entry(
                        logical_error=9.484121471230475e-10,
                        num_qubits=463,
                        volume=101966.74031969943,
                        steps=221,
                    ),
                    _Entry(
                        logical_error=1.366706735546955e-09,
                        num_qubits=463,
                        volume=88163.33566758082,
                        steps=191,
                    ),
                    _Entry(
                        logical_error=1.6414883261014871e-09,
                        num_qubits=463,
                        volume=75634.9355950081,
                        steps=164,
                    ),
                    _Entry(
                        logical_error=2.1231620948839324e-09,
                        num_qubits=463,
                        volume=57066.94042309941,
                        steps=124,
                    ),
                    _Entry(
                        logical_error=2.7025320732488425e-09,
                        num_qubits=463,
                        volume=51274.875954425035,
                        steps=111,
                    ),
                    _Entry(
                        logical_error=5.927193917808693e-09,
                        num_qubits=463,
                        volume=44459.37605622456,
                        steps=97,
                    ),
                    _Entry(
                        logical_error=9.96666522298153e-09,
                        num_qubits=463,
                        volume=37819.312343872756,
                        steps=82,
                    ),
                    _Entry(
                        logical_error=1.3094044583206633e-08,
                        num_qubits=463,
                        volume=32487.26462193073,
                        steps=71,
                    ),
                    _Entry(
                        logical_error=3.2242219813174955e-08,
                        num_qubits=463,
                        volume=28807.1580608257,
                        steps=63,
                    ),
                    _Entry(
                        logical_error=6.869319538705312e-08,
                        num_qubits=463,
                        volume=25913.75133805069,
                        steps=56,
                    ),
                    _Entry(
                        logical_error=9.098034600043679e-08,
                        num_qubits=463,
                        volume=23124.302662208655,
                        steps=50,
                    ),
                    _Entry(
                        logical_error=1.4186675692747566e-07,
                        num_qubits=463,
                        volume=20704.800496362262,
                        steps=45,
                    ),
                    _Entry(
                        logical_error=2.0609462073340207e-07,
                        num_qubits=463,
                        volume=18536.983107361142,
                        steps=41,
                    ),
                    _Entry(
                        logical_error=2.978734256336741e-07,
                        num_qubits=463,
                        volume=16619.252015567916,
                        steps=36,
                    ),
                    _Entry(
                        logical_error=6.861969312896777e-07,
                        num_qubits=463,
                        volume=14668.00319753873,
                        steps=32,
                    ),
                    _Entry(
                        logical_error=3.596202802726852e-06,
                        num_qubits=463,
                        volume=13144.278450583368,
                        steps=29,
                    ),
                ],
            },
            0.002: {
                (3, 15): [
                    _Entry(
                        logical_error=2.4910941050344295e-05,
                        num_qubits=454,
                        volume=37284.10300576104,
                        steps=83,
                    ),
                    _Entry(
                        logical_error=2.761905460971352e-05,
                        num_qubits=454,
                        volume=29792.666977290326,
                        steps=66,
                    ),
                    _Entry(
                        logical_error=2.998068211949661e-05,
                        num_qubits=454,
                        volume=26749.307666753448,
                        steps=59,
                    ),
                    _Entry(
                        logical_error=3.420819333029743e-05,
                        num_qubits=454,
                        volume=22236.378828005407,
                        steps=49,
                    ),
                    _Entry(
                        logical_error=4.191389985476303e-05,
                        num_qubits=454,
                        volume=18698.170346155115,
                        steps=42,
                    ),
                    _Entry(
                        logical_error=4.98420957665537e-05,
                        num_qubits=454,
                        volume=16332.448762359943,
                        steps=36,
                    ),
                    _Entry(
                        logical_error=6.0843462706063846e-05,
                        num_qubits=454,
                        volume=14529.582827180458,
                        steps=33,
                    ),
                    _Entry(
                        logical_error=7.4722791321815e-05,
                        num_qubits=454,
                        volume=13011.901619517843,
                        steps=29,
                    ),
                    _Entry(
                        logical_error=0.00010604029641121001,
                        num_qubits=454,
                        volume=11147.327513336535,
                        steps=25,
                    ),
                    _Entry(
                        logical_error=0.0001897094496473116,
                        num_qubits=454,
                        volume=9618.474016585498,
                        steps=22,
                    ),
                    _Entry(
                        logical_error=0.00028102936251604493,
                        num_qubits=454,
                        volume=8309.685206518456,
                        steps=19,
                    ),
                    _Entry(
                        logical_error=0.0004014066268503485,
                        num_qubits=454,
                        volume=7249.0561048705085,
                        steps=16,
                    ),
                    _Entry(
                        logical_error=0.0006279323413052242,
                        num_qubits=454,
                        volume=6396.40476692938,
                        steps=15,
                    ),
                    _Entry(
                        logical_error=0.001031660798536612,
                        num_qubits=454,
                        volume=5745.904210463877,
                        steps=13,
                    ),
                    _Entry(
                        logical_error=0.0021742383334305182,
                        num_qubits=454,
                        volume=5035.257409573139,
                        steps=12,
                    ),
                    _Entry(
                        logical_error=0.0062359457977829574,
                        num_qubits=454,
                        volume=4524.1137004324555,
                        steps=10,
                    ),
                ],
                (5, 15): [
                    _Entry(
                        logical_error=1.0439441504494727e-07,
                        num_qubits=463,
                        volume=2657933.624901944,
                        steps=5741,
                    ),
                    _Entry(
                        logical_error=1.519092236052502e-07,
                        num_qubits=463,
                        volume=1933842.1177957687,
                        steps=4177,
                    ),
                    _Entry(
                        logical_error=1.923379423808738e-07,
                        num_qubits=463,
                        volume=1632339.8276892796,
                        steps=3526,
                    ),
                    _Entry(
                        logical_error=3.2124499138902803e-07,
                        num_qubits=463,
                        volume=1363176.1559859414,
                        steps=2945,
                    ),
                    _Entry(
                        logical_error=4.582418310317106e-07,
                        num_qubits=463,
                        volume=1166706.447381707,
                        steps=2520,
                    ),
                    _Entry(
                        logical_error=5.112861803062121e-07,
                        num_qubits=463,
                        volume=1001353.8412128607,
                        steps=2163,
                    ),
                    _Entry(
                        logical_error=6.135995964696343e-07,
                        num_qubits=463,
                        volume=867919.460900446,
                        steps=1875,
                    ),
                    _Entry(
                        logical_error=8.264697950313585e-07,
                        num_qubits=463,
                        volume=779345.6004873333,
                        steps=1684,
                    ),
                    _Entry(
                        logical_error=9.862797392621568e-07,
                        num_qubits=463,
                        volume=697532.5477090707,
                        steps=1507,
                    ),
                    _Entry(
                        logical_error=1.4605751471361379e-06,
                        num_qubits=463,
                        volume=590269.3369238179,
                        steps=1275,
                    ),
                    _Entry(
                        logical_error=1.7050689627256938e-06,
                        num_qubits=463,
                        volume=456967.39083761314,
                        steps=987,
                    ),
                    _Entry(
                        logical_error=2.0612388049832663e-06,
                        num_qubits=463,
                        volume=391643.00823967945,
                        steps=846,
                    ),
                    _Entry(
                        logical_error=2.4994358324709424e-06,
                        num_qubits=463,
                        volume=326342.9586161042,
                        steps=705,
                    ),
                    _Entry(
                        logical_error=3.2153686389598566e-06,
                        num_qubits=463,
                        volume=272882.93849394773,
                        steps=590,
                    ),
                    _Entry(
                        logical_error=4.517133819097825e-06,
                        num_qubits=463,
                        volume=233757.05119250793,
                        steps=505,
                    ),
                    _Entry(
                        logical_error=6.302800575499024e-06,
                        num_qubits=463,
                        volume=205733.88608882687,
                        steps=445,
                    ),
                    _Entry(
                        logical_error=8.296938495749044e-06,
                        num_qubits=463,
                        volume=177964.7619389076,
                        steps=385,
                    ),
                    _Entry(
                        logical_error=1.1180919189933176e-05,
                        num_qubits=463,
                        volume=150619.97481794056,
                        steps=326,
                    ),
                    _Entry(
                        logical_error=1.666969413076334e-05,
                        num_qubits=463,
                        volume=130630.55920028445,
                        steps=283,
                    ),
                    _Entry(
                        logical_error=2.4912272860029657e-05,
                        num_qubits=463,
                        volume=116232.14818145634,
                        steps=252,
                    ),
                    _Entry(
                        logical_error=4.175232964826617e-05,
                        num_qubits=463,
                        volume=102018.71759015942,
                        steps=221,
                    ),
                    _Entry(
                        logical_error=6.92162424823325e-05,
                        num_qubits=463,
                        volume=91661.28628952683,
                        steps=198,
                    ),
                    _Entry(
                        logical_error=0.00016412344615470247,
                        num_qubits=463,
                        volume=81017.98096879711,
                        steps=175,
                    ),
                    _Entry(
                        logical_error=0.0005018430993482565,
                        num_qubits=463,
                        volume=72707.05555154958,
                        steps=158,
                    ),
                    _Entry(
                        logical_error=0.0050634348954634,
                        num_qubits=463,
                        volume=65406.324298290776,
                        steps=142,
                    ),
                ],
            },
        }

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(T, encoding=PHYSICAL),
            constraint(
                CNOT,
                arity=2,
                encoding=PHYSICAL,
                error_rate=ConstraintBound.le(1e-3),
            ),
        )

    def provided_isa(
        self, impl_isa: ISA, ctx: ISAContext
    ) -> Generator[ISA, None, None]:
        cnot = impl_isa[CNOT]

        error_rate = cnot.expect_error_rate()
        gate_time = cnot.expect_time()

        # Find the smallest error rate that is larger than the provided T
        # state error rate
        error_rate = min(
            (key for key in self._data.keys() if key >= error_rate),
            default=None,
        )
        if error_rate is None:
            raise RuntimeError(
                "Cannot determine provided ISA for GSJ24 factory: "
                "provided T state error rate is too high"
            )

        for entry in self._data[error_rate][self.distance]:
            isa = ctx.make_isa(
                ctx.add_instruction(
                    T,
                    encoding=LOGICAL,
                    error_rate=entry.logical_error,
                    space=entry.num_qubits,
                    time=ceil(
                        gate_time
                        * self.syndrome_extraction_depth
                        * (entry.volume / entry.num_qubits)
                    ),
                    transform=self,
                    source=[cnot],
                )
            )

            if self.passthrough:
                yield impl_isa + isa
            else:
                yield isa


@dataclass
class _Entry:
    logical_error: float
    """
    Logical error rate of the output magic state.
    """

    num_qubits: int
    """
    Number of qubits used in the factory.
    """

    volume: float
    """
    Volume of the factory, including the overhead to compensate for the
    acceptance probability.
    """

    steps: int
    """
    Time steps of the factory, including the overhead to compensate for the
    acceptance probability.
    """


def _extract_data_from_simulations(filename: Path, zenodo_path: Path):
    """
    Extracts and processes simulation data from the Zenodo dataset for the
    GSJ24 magic state cultivation protocol.

    Reads Monte Carlo simulation results from CSV files, processes them
    through the ``cultiv`` and ``gen`` libraries, and prints the resulting
    data dictionary suitable for embedding in the :class:`GSJ24Factory` class.

    Requires the ``stim``, ``sinter``, ``cultiv``, and ``gen`` packages.

    Args:
        filename: Path to the CSV file containing simulation statistics
            (from https://doi.org/10.5281/zenodo.13777072).
        zenodo_path: Path to the root of the extracted Zenodo archive,
            which must contain a ``src/`` directory with the ``cultiv``
            and ``gen`` modules.
    """
    try:
        import stim  # type: ignore
    except ImportError as e:
        raise ImportError(
            "The 'stim' package is required to load GSJ24 cultivation data. "
            "Please install it via 'pip install stim'."
        ) from e

    try:
        import sinter  # type: ignore
    except ImportError as e:
        raise ImportError(
            "The 'sinter' package is required to load GSJ24 cultivation data. "
            "Please install it via 'pip install sinter'."
        ) from e

    src_path = zenodo_path / "src"
    assert src_path.exists()
    sys.path.append(str(src_path))

    import cultiv  # type: ignore
    import gen  # type: ignore

    #############################################
    # Extracting and processing simulation data #
    #############################################
    def select_rep_stats(
        stats: list[sinter.TaskStats], circuit: stim.Circuit
    ) -> list[tuple[sinter.TaskStats, float]]:
        stats = [stat for stat in stats if stat.shots > stat.discards]
        vols = []
        errs = []
        baseline = cultiv.compute_expected_injection_growth_volume(
            circuit,
            discard_rate=0,
        )
        for stat in stats:
            keep_rate = (stat.shots - stat.discards) / stat.shots
            vols.append(baseline / keep_rate)
            errs.append(stat.errors / (stat.shots - stat.discards))
        indices = sorted(range(len(stats)), key=lambda e: (errs[e], vols[e]))
        vols = [vols[k] for k in indices]
        errs = [errs[k] for k in indices]
        stats = [stats[k] for k in indices]

        result = []
        prev_vol = None
        for k in range(len(stats)):
            if prev_vol is not None and vols[k] > prev_vol * 0.9 and errs[k] != 0:
                continue
            prev_vol = vols[k]
            result.append((stats[k], vols[k]))

        return result

    data = sinter.read_stats_from_csv_files(filename)
    DOUBLING_S_GIVES_T_ASSUMPTION = 2

    # Extract all relevant simulation data points from end2end magic state
    # cultivation
    selected = [
        stat
        for stat in data
        if stat.decoder == "desaturation"
        if stat.json_metadata.get("c") == "end2end-inplace-distillation"
        if stat.json_metadata.get("noise") == "uniform"
    ]

    # Sort by d1, d2, then p
    def sort_key(stat: sinter.TaskStats):
        return (
            stat.json_metadata.get("p"),
            stat.json_metadata.get("d1"),
            stat.json_metadata.get("d2"),
        )

    data = {}

    selected.sort(key=sort_key)
    for key, stats in itertools.groupby(selected, key=sort_key):
        stats = list(stats)
        if len(stats) != 1:
            continue

        p, d1, d2 = key

        circ = cultiv.make_end2end_cultivation_circuit(
            dcolor=d1,
            dsurface=d2,
            basis=stats[0].json_metadata.get("b", "Y"),
            r_growing=d1,
            r_end=1,
            inject_style="unitary",
        )

        gs = cultiv.stat_to_gap_stats(
            stats,
            rounding=1,
            func=lambda arg: sinter.AnonTaskStats(
                shots=arg.source.shots,
                discards=arg.at_least.discards + arg.less.shots,
                errors=arg.at_least.errors,
            ),
        )

        c3n = gen.NoiseModel.uniform_depolarizing(
            p
        ).noisy_circuit_skipping_mpp_boundaries(circ)

        if p not in data:
            data[p] = {}

        if (d1, d2) not in data[p]:
            data[p][(d1, d2)] = []

        for stat, cur_vol in select_rep_stats(gs, c3n):
            logical_error_rate = (stat.errors * DOUBLING_S_GIVES_T_ASSUMPTION) / (
                stat.shots - stat.discards
            )
            if logical_error_rate == 0:
                continue
            qubits = stat.json_metadata.get("q")
            steps = ceil(cur_vol / qubits)

            data[p][(d1, d2)].append(
                _Entry(
                    logical_error_rate,
                    qubits,
                    cur_vol,
                    steps,
                )
            )

    import pprint

    pprint.pprint(data)
