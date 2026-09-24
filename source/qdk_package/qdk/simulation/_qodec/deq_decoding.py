from __future__ import annotations

import asyncio
from collections.abc import Coroutine, Sequence
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack, closing
from dataclasses import dataclass
from typing import Any, TypeVar, cast

import numpy as np
from deq.proto import coordinator_pb2 as coordinator
from deq.proto import deq_bin_pb2 as model
from deq.proto import util_pb2 as util
from deq.runtime import Runtime
from paulimer import DensePauli
from qodec import Gadget, Layer

from .decoding import CodeDecoder, SyndromeModel, SyndromeSession
from .protocols import (
    BatchUnsupported,
    Decoded,
    ExecutionUnresolved,
    Invocation,
    ReadoutBatch,
    Readouts,
)
from .readout_equations import InconsistentParity

ResultT = TypeVar("ResultT")


class DeqModel(SyndromeModel):
    def __init__(self, layer: Layer, probability: float) -> None:
        super().__init__(layer)
        self.probability = probability
        self._batch_readouts: dict[tuple[str, int], _DeqReadouts | None] = {}

    def new_session(
        self, seed: int | None = None, *, transport: _DeqTransport | None = None
    ) -> DeqSession:
        return DeqSession(self, self.probability, seed, transport=transport)

    def prepare_batch(self) -> _DeqBatchSession:
        return _DeqBatchSession(self)

    def prepare_readouts(
        self, gadget: Gadget, record_count: int
    ) -> _DeqReadouts | None:
        key = (gadget.implements.mnemonic, record_count)
        if key not in self._batch_readouts:
            self._batch_readouts[key] = _DeqReadouts.prepare(self, gadget, record_count)
        return self._batch_readouts[key]


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


class _DeqTransport:
    def __init__(self) -> None:
        self.loop = asyncio.new_event_loop()
        try:
            self.worker = ThreadPoolExecutor(
                max_workers=1, thread_name_prefix="qdk-deq"
            )
        except BaseException:
            self.loop.close()
            raise

    def run(self, operation: Coroutine[Any, Any, ResultT]) -> ResultT:
        return self.worker.submit(self.loop.run_until_complete, operation).result()

    def close(self) -> None:
        try:
            self.loop.close()
        finally:
            self.worker.shutdown()


class DeqSession(SyndromeSession):
    def __init__(
        self,
        prepared: SyndromeModel,
        probability: float,
        seed: int | None,
        *,
        transport: _DeqTransport | None = None,
    ) -> None:
        super().__init__(prepared)
        self._probability = probability
        self._seed = seed
        self._transport = transport
        self._owns_transport = transport is None
        self._runtime: Runtime | None = None

    def _ensure_started(self) -> None:
        if self._runtime is not None:
            return
        try:
            if self._transport is None:
                self._transport = _DeqTransport()
            self._wait(self._start(self._seed))
        except BaseException:
            try:
                self.close()
            except BaseException:
                pass
            raise

    def _wait(self, operation: Coroutine[Any, Any, ResultT]) -> ResultT:
        assert self._transport is not None
        return self._transport.run(operation)

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
        self._ensure_started()
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
            if self._runtime is not None:
                self._wait(self._stop())
        finally:
            if self._owns_transport and self._transport is not None:
                self._transport.close()


def _terminal_readouts(
    session: SyndromeSession, invocation: Invocation, records: Readouts
) -> Readouts:
    with closing(session.decode(invocation, records)) as corrections:
        try:
            next(corrections)
        except StopIteration as completed:
            result = cast(Decoded, completed.value).readouts
            if any(value is None for value in result):
                raise ExecutionUnresolved(
                    "Terminal decoding requires unavailable readouts"
                )
            return result
        raise BatchUnsupported("Terminal decoding requires physical corrections")


class _ReadoutProbe(SyndromeSession):
    needs_correction = False

    def correct(self, decoder: CodeDecoder, syndrome: Readouts) -> DensePauli:
        self.needs_correction |= any(syndrome)
        return DensePauli.identity(decoder.width)


@dataclass(frozen=True)
class _DeqReadouts:
    model: DeqModel
    invocation: Invocation
    known: tuple[Readouts | None, ...]

    @classmethod
    def prepare(
        cls, model: DeqModel, gadget: Gadget, width: int
    ) -> _DeqReadouts | None:
        invocation = model.terminal_invocation(gadget, width)
        if invocation is None:
            return None
        known = []
        with closing(_ReadoutProbe(model)) as probe:
            for pattern in range(1 << width):
                probe.needs_correction = False
                records = tuple(bool(pattern & (1 << index)) for index in range(width))
                try:
                    result = _terminal_readouts(probe, invocation, records)
                except (InconsistentParity, ExecutionUnresolved, BatchUnsupported):
                    return None
                known.append(None if probe.needs_correction else result)
        return cls(model, invocation, tuple(known))

    def decode_batch(
        self, readouts: Sequence[Readouts], seeds: Sequence[int], /
    ) -> Sequence[Readouts | Exception]:
        if len(readouts) != len(seeds):
            raise ValueError("Each decoder input requires a shot seed")
        width = (len(self.known) - 1).bit_length()
        results: list[Readouts | Exception] = []
        with ExitStack() as resources:
            transport = None
            for records, seed in zip(readouts, seeds):
                if len(records) != width:
                    raise ValueError(
                        "Readout width does not match the prepared decoder"
                    )
                if any(value is None for value in records):
                    results.append(
                        ExecutionUnresolved("Terminal records contain unknown readouts")
                    )
                    continue
                pattern = sum(
                    bool(value) << index for index, value in enumerate(records)
                )
                result = self.known[pattern]
                if result is not None:
                    results.append(result)
                    continue
                if transport is None:
                    transport = resources.enter_context(closing(_DeqTransport()))
                try:
                    with closing(
                        self.model.new_session(seed, transport=transport)
                    ) as session:
                        results.append(
                            _terminal_readouts(session, self.invocation, records)
                        )
                except (InconsistentParity, ExecutionUnresolved) as error:
                    results.append(error)
        return results


class _DeqBatchSession(DeqSession):
    def __init__(self, model: DeqModel) -> None:
        super().__init__(model, model.probability, 0)
        self.prepared = model

    def correct(self, decoder: CodeDecoder, syndrome: Readouts) -> DensePauli:
        if any(syndrome):
            raise BatchUnsupported("Prefix corrections depend on the decoder seed")
        return super().correct(decoder, syndrome)

    def prepare_readouts(
        self, invocation: Invocation, record_count: int, /
    ) -> ReadoutBatch | None:
        return self.prepared.prepare_readouts(invocation.gadget, record_count)
