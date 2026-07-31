"""Deq-backed logical-error-rate execution target."""

from __future__ import annotations

import json
import re
import subprocess
import tempfile
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

import qodec
from deq.noise import inject_biased, inject_si1000
from qodec.circuits import Program

from ..base import Target
from .interchange import to_deq_source
from .options import DeqOptions

NoiseModel = Callable[[str], str]
"""A source-to-source deq noise injection function."""


def SI1000(p: float) -> NoiseModel:
    """Uniform SI1000 depolarization at physical error rate ``p``."""
    return lambda source: inject_si1000(source, p)


def Biased(
    p: float,
    *,
    p1q: float | None = None,
    eta: float = 10.0,
) -> NoiseModel:
    """Biased deq noise with configurable one- and two-qubit strengths."""
    return lambda source: inject_biased(source, p, p1q=p1q, eta=eta)


@dataclass(frozen=True)
class LerResult:
    """Aggregated logical-error statistics reported by deq."""

    shots: int
    logical_errors: int
    error_rate: float
    decode_time_per_shot: float


class DeqLerTarget(Target[LerResult]):
    """Run a qodec program through deq's integrated sampler and decoder."""

    def __init__(
        self,
        codec: qodec.Qodec,
        *,
        translation_index: int = -1,
        noise: NoiseModel | None = None,
        options: DeqOptions | None = None,
    ) -> None:
        super().__init__(codec)
        self._translation_index = translation_index
        self._noise = noise
        self._options = options if options is not None else DeqOptions()

    @property
    def options(self) -> DeqOptions:
        return self._options

    def execute(
        self,
        program: Program,
        *,
        shots: int,
        target_errors: int | None = None,
        timeout: float | None = None,
    ) -> LerResult:
        source = to_deq_source(
            self.codec,
            translation_index=self._translation_index,
            program=program,
            program_name="Program",
        )
        if self._noise is not None:
            source = self._noise(source)
        with tempfile.TemporaryDirectory() as directory:
            deq_path = Path(directory) / "program.deq"
            deq_path.write_text(source)
            command = [
                self._options.binary,
                "simulate",
                "ler",
                str(deq_path),
                "--program",
                "Program",
                "--shots",
                str(shots),
                "--decoder",
                self._options.decoder,
            ]
            if self._options.decoder_config is not None:
                command += [
                    "--decoder-config",
                    json.dumps(self._options.decoder_config),
                ]
            if target_errors is not None:
                command += ["--errors", str(target_errors)]
            completed = subprocess.run(
                command,
                capture_output=True,
                text=True,
                timeout=timeout,
                check=False,
            )
            if completed.returncode != 0:
                raise RuntimeError(
                    f"deq simulate ler failed (exit {completed.returncode})\n"
                    f"stdout:\n{completed.stdout}\n"
                    f"stderr:\n{completed.stderr}"
                )
            return _parse_simulate_output(completed.stdout)


def _parse_simulate_output(text: str) -> LerResult:
    shots = _extract_int(text, r"Shots:\s+(\d+)")
    errors = _extract_int(text, r"Logical errors:\s+(\d+)")
    decode = _extract_float(text, r"Avg decode:\s+([\d.eE+\-]+)\s*s/shot") or 0.0
    rate = float(errors) / float(shots) if shots > 0 else float("nan")
    return LerResult(
        shots=shots,
        logical_errors=errors,
        error_rate=rate,
        decode_time_per_shot=decode,
    )


def _extract_int(text: str, pattern: str) -> int:
    match = re.search(pattern, text)
    if match is None:
        raise RuntimeError(f"could not find {pattern!r} in deq output:\n{text}")
    return int(match.group(1))


def _extract_float(text: str, pattern: str) -> float | None:
    match = re.search(pattern, text)
    return float(match.group(1)) if match else None


__all__ = [
    "Biased",
    "DeqLerTarget",
    "LerResult",
    "NoiseModel",
    "SI1000",
]
