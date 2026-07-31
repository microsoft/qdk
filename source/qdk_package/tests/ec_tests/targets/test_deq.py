"""Tests for `qdk.ec.targets.DeqLerTarget`.

These are end-to-end tests: they invoke the ``deq`` CLI as a subprocess.
They are skipped if either the ``deq`` Python package or the ``deq``
executable on PATH is unavailable.
"""
from __future__ import annotations

import shutil

import pytest

pytest.importorskip("deq")
deq_runtime = pytest.importorskip("deq_runtime")
if shutil.which("deq") is None:
    pytest.skip("deq CLI not on PATH", allow_module_level=True)
try:
    # The repo ships a pure-Python stub so ``import deq_runtime`` succeeds in
    # Stim-only environments; any real call raises. Skip when only the stub
    # is present, since DeqLerTarget needs the native JIT compiler.
    deq_runtime.static_jit_compile  # noqa: B018
except RuntimeError:
    pytest.skip(
        "deq_runtime native extension not built", allow_module_level=True
    )

import qodec  # noqa: E402

from qodec.circuits import Program  # noqa: E402
from ec_tests.testing.qodecs import c4  # noqa: E402
from qdk.ec.targets import Biased, DeqLerTarget, LerResult, SI1000  # noqa: E402


def _memory_program(codec: qodec.Qodec) -> Program:
    return Program(
        [
            qodec.instructions.InstructionCall(
                "prepare_zz",
                outputs={"block": "data"},
                assume=[{"reject": 0}],
            ),
            qodec.instructions.InstructionCall(
                "idle", inputs={"block": "data"}, outputs={"block": "data"}
            ),
            qodec.instructions.InstructionCall(
                "measure_zz", inputs={"block": "data"}
            ),
        ],
        codec.layers[0].isa,
    )


def test_deq_ler_target_noiseless_memory() -> None:
    """c4-stim is noiseless → memory experiment should produce 0 errors."""
    codec = c4()
    target = DeqLerTarget(codec)
    result = target.execute(_memory_program(codec), shots=200, timeout=60)

    assert isinstance(result, LerResult)
    assert result.shots == 200
    assert result.logical_errors == 0
    assert result.error_rate == 0.0
    assert result.decode_time_per_shot >= 0.0


def test_deq_ler_target_si1000_produces_errors() -> None:
    """With SI1000 noise at p=1%, c4-stim memory experiment must see logical
    errors — sanity check that noise injection reaches the simulator."""
    codec = c4()
    target = DeqLerTarget(codec, noise=SI1000(0.01))
    result = target.execute(_memory_program(codec), shots=500, timeout=60)

    assert result.shots == 500
    assert result.logical_errors > 0
    assert 0.0 < result.error_rate < 1.0


def test_deq_ler_target_biased_runs() -> None:
    """Biased noise model also wires through end-to-end."""
    codec = c4()
    target = DeqLerTarget(codec, noise=Biased(0.005, eta=5.0))
    result = target.execute(_memory_program(codec), shots=200, timeout=60)

    assert result.shots == 200
    assert 0.0 <= result.error_rate <= 1.0
