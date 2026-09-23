from __future__ import annotations

import asyncio
from collections.abc import Coroutine
from concurrent.futures import ThreadPoolExecutor
from typing import Any, TypeVar

import numpy as np
from deq.proto import coordinator_pb2 as coordinator
from deq.proto import deq_bin_pb2 as model
from deq.proto import util_pb2 as util
from deq.runtime import Runtime
from paulimer import DensePauli

from .decoding import CodeDecoder, SyndromeModel, SyndromeSession
from .protocols import ExecutionUnresolved, Readouts

ResultT = TypeVar("ResultT")


def _library(
    code: CodeDecoder, observed: tuple[int, ...], probability: float
) -> model.Library:
    check_count, fault_count = len(observed), len(code.faults)
    return model.Library(
        gadget_types=[
            model.GadgetType(
                gtype=1,
                name="syndrome",
                measurements=[model.GadgetType.Measurement() for _ in observed],
                readouts=[model.GadgetType.Readout() for _ in code.faults],
                correction_propagation=util.BitMatrix(rows=0, cols=1),
                readout_propagation=util.BitMatrix(rows=fault_count, cols=1),
                logical_correction=util.BitMatrix(rows=0, cols=fault_count),
                physical_correction=util.BitMatrix(rows=0, cols=check_count),
            )
        ],
        check_model_types=[
            model.CheckModelType(
                ctype=1,
                gtype=1,
                checks=[
                    model.CheckModelType.Check(
                        measurements=[
                            model.CheckModelType.RemoteMeasurement(
                                measurement_index=index
                            )
                        ]
                    )
                    for index in range(check_count)
                ],
            )
        ],
        error_model_types=[
            model.ErrorModelType(
                etype=1,
                ctype=1,
                errors=[
                    model.ErrorModelType.Error(
                        probability=probability,
                        readout_flips=[fault],
                        checks=[
                            model.ErrorModelType.RemoteCheck(check_index=index)
                            for index, check in enumerate(observed)
                            if code.syndromes[check, fault]
                        ],
                    )
                    for fault in range(fault_count)
                ],
            )
        ],
    )


class DeqSession(SyndromeSession):
    def __init__(
        self, prepared: SyndromeModel, probability: float, seed: int | None
    ) -> None:
        super().__init__(prepared)
        self._probability = probability
        self._loop = asyncio.new_event_loop()
        self._worker = ThreadPoolExecutor(max_workers=1, thread_name_prefix="qdk-deq")
        self._runtime: Runtime | None = None
        try:
            self._wait(self._start(seed))
        except BaseException:
            try:
                self.close()
            except BaseException:
                pass
            raise

    def _wait(self, operation: Coroutine[Any, Any, ResultT]) -> ResultT:
        return self._worker.submit(self._loop.run_until_complete, operation).result()

    async def _start(self, seed: int | None) -> None:
        self._runtime = Runtime(
            decoder="black-box-relay-bp",
            decoder_config={"parallel": 1, "seed": seed or 0},
            coordinator="monolithic",
            controller="none",
        )

    def correct(self, decoder: CodeDecoder, syndrome: Readouts) -> DensePauli:
        if self.closed:
            raise RuntimeError("Decoder session is closed")
        if len(syndrome) != len(decoder.stabilizers):
            raise ValueError("Syndrome positions must match the code stabilizers")
        correction = DensePauli.identity(decoder.width)
        if not any(syndrome):
            return correction
        observed = tuple(
            index for index, value in enumerate(syndrome) if value is not None
        )
        library = _library(decoder, observed, self._probability)
        outcomes = util.BitVector(
            size=len(observed),
            data=np.packbits([bool(syndrome[index]) for index in observed]).tobytes(),
        )
        result = self._wait(self._decode(library, outcomes))
        bits = result.readouts
        if (
            result.gid != 1
            or bits.size != len(decoder.faults)
            or len(bits.data) != (bits.size + 7) // 8
        ):
            raise ExecutionUnresolved("deq returned an invalid correction record")
        selected = np.unpackbits(np.frombuffer(bits.data, dtype=np.uint8))[: bits.size]
        for fault, enabled in zip(decoder.faults, selected):
            if enabled:
                correction *= fault
        if any(
            (not correction.commutes_with(decoder.stabilizers[index]))
            != syndrome[index]
            for index in observed
        ):
            raise ExecutionUnresolved("deq correction does not match the syndrome")
        return correction

    async def _decode(
        self, library: model.Library, outcomes: util.BitVector
    ) -> coordinator.Readouts:
        assert self._runtime is not None
        service = self._runtime.coordinator
        await service.reset(reset_library=True, reset_decoder_service=True)
        await service.load_library(library)
        for instruction in (
            model.Instruction(gadget=model.Gadget(gtype=1, gid=1)),
            model.Instruction(check_model=model.CheckModel(ctype=1, gid=1, cid=1)),
            model.Instruction(error_model=model.ErrorModel(etype=1, cid=1, eid=1)),
        ):
            if await service.execute(instruction) != 1:
                raise ExecutionUnresolved("deq returned an unexpected model identifier")
        return await service.decode(coordinator.Outcomes(gid=1, outcomes=outcomes))

    async def _stop(self) -> None:
        runtime, self._runtime = self._runtime, None
        if runtime is not None:
            try:
                await runtime.coordinator.reset(
                    reset_library=True, reset_decoder_service=True
                )
            finally:
                await runtime.shutdown()

    def close(self) -> None:
        if self.closed:
            return
        super().close()
        try:
            self._wait(self._stop())
        finally:
            self._loop.close()
            self._worker.shutdown()
