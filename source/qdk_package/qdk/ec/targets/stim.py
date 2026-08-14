"""StimSampler: stochastic sampler that compiles to stim and runs the
detector sampler.

A `StimSampler` binds a qodec and a noise model at construction. Programs
in any source layer of the qodec are first lowered to the second-to-bottom
layer via the supplied compiler (default: `RecursiveLowering`). The
sampler then performs the final hop into stim: each remaining call's
gadget contributes a stim circuit fragment, with detector and observable
directives appended from the gadget's checks and observables.
"""

from __future__ import annotations

from typing import Iterable

import numpy as np
import numpy.typing as npt

import stim

import qodec as qc
from qodec.circuits import Program

from .compilers import Compiler, RecursiveLowering
from .compilers.recursive_lowering import (
    _build_namespaced_remap,
    _remap_call,
)
from .results import Batch
from .._readouts import observable_slots, readout_slots
from .._references import (
    outcomes_of,
    parse_equations,
    stabilizer_signs_of,
)
from ._coerce import coerce_program
from ._qubit_alloc import PhysicalQubitAllocator, remap_call_source
from ._recursive_emit import (
    FrameMaps,
    Provenance,
    _has_out_stab,
    _RecursiveEmitState,
    exposed_readout_records,
    resolve_records,
    update_frame_maps,
)
from .base import Target


class StimEmitter:
    """Qodec-aware Program → stim circuit (with DEM annotations).

    Knows nothing about sampling. Its sole responsibilities are:

    * lower a Program from any source layer down to the qodec's
      bottom-layer ISA (via the supplied ``compiler``);
    * concatenate each gadget's raw stim source;
    * inject gate-level noise (optional);
    * append ``DETECTOR`` and ``OBSERVABLE_INCLUDE`` directives derived
      from the gadget's checks, observables, and flags.

    The qodec must have at least one translation. The emitter uses the
    *last* translation (bottom layer) to emit stim; any earlier
    translations are handled by ``compiler`` (default:
    `RecursiveLowering` over the qodec's pre-bottom slice).

    .. note::

        **Multi-layer decoding surfaces.** When the qodec has more than
        one translation *and* no explicit ``compiler`` is supplied, the
        emitter recurses through every translation, folding each edge's
        ``checks`` / ``frames`` / ``readouts`` down to physical
        measurement records (see :meth:`_build_circuit_recursive`). This
        composes intermediate-layer decoding surfaces into the flat
        circuit rather than discarding them.

        The recursive path targets the *fully declared* subset: gadgets
        whose decoding surface is expressed through declared
        ``circuit.readouts`` (positional or observe-named), ``checks``,
        ``frames``, and ``readouts``. Features such as ``capture`` /
        ``assume`` readouts, undeclared frames, or flags on
        non-bottom gadgets raise ``NotImplementedError``. Single-
        translation qodecs (or any qodec given an explicit ``compiler``)
        keep the original flat emission path unchanged.

    Stim source files must be metadata-free: ``DETECTOR`` and
    ``OBSERVABLE_INCLUDE`` directives in raw sources are rejected at
    load time.

    Noise is layered, not baked in: pass a different noise dict at
    construction (or via :meth:`with_noise`) to get a separate emitter
    that shares the same compiler and translation but a fresh circuit
    cache. With ``noise=None`` or ``{}`` the emitter is exactly noiseless.
    :func:`qdk.ec.targets.detector_error_model_of` passes target noise
    explicitly when constructing a DEM.
    """

    def __init__(
        self,
        qodec: qc.Qodec,
        *,
        noise: dict[str, float] | None = None,
        compiler: Compiler | None = None,
        emit_flags: bool = True,
    ) -> None:
        if len(qodec.layers) < 2:
            raise ValueError(
                "StimEmitter requires a qodec with at least two layers "
                "(one lowering edge)"
            )
        layer_count = len(qodec.layers)
        self._qodec = qodec
        self._emit_flags = emit_flags
        # The bottom non-empty layer: its gadgets lower the second-to-bottom
        # ISA into the physical (stim) ISA.
        self._stim_layer = qodec.layers[-2]
        self._stim_source_isa = qodec.layers[-2].isa
        self._stim_target_isa = qodec.layers[-1].isa
        # With a caller-supplied compiler the program arrives pre-lowered to the
        # bottom edge, so there is only ever one decoding surface to emit.
        # Without one, every extra lowering edge carries its own checks and
        # readouts, which have to be composed down to physical records
        # (``_build_circuit_recursive``) rather than discarded.
        self._composes_layers = compiler is None and layer_count > 2
        if compiler is None:
            pre_bottom = qodec.slice(0, layer_count - 1)
            compiler = RecursiveLowering(pre_bottom)
        self._compiler = compiler
        self._noise = dict(noise) if noise else {}
        self._raw_circuits: dict[str, stim.Circuit] = {}
        self._m2d_cache: dict[
            int, "stim.CompiledMeasurementsToDetectionEventsConverter"
        ] = {}

    @property
    def qodec(self) -> qc.Qodec:
        return self._qodec

    @property
    def compiler(self) -> Compiler:
        return self._compiler

    @property
    def translation(self) -> qc.Layer:
        """The bottom layer: the one whose gadgets drive stim emission."""
        return self._stim_layer

    @property
    def noise(self) -> dict[str, float]:
        return dict(self._noise)

    def with_noise(self, noise: dict[str, float] | None) -> "StimEmitter":
        """Return a fresh emitter with a new noise dict.

        Shares the qodec and compiler with ``self``; raw-circuit cache
        is rebuilt independently so that mutating one emitter cannot
        affect the other.
        """
        return StimEmitter(
            self._qodec,
            noise=noise,
            compiler=self._compiler,
            emit_flags=self._emit_flags,
        )

    def detector_counts(self) -> dict[str, int]:
        """Detector counts per gadget mnemonic in the bottom translation."""
        result: dict[str, int] = {}
        for name, gadget in self._stim_layer.gadgets.items():
            base = self._load_circuit(name).num_detectors
            result[name] = base + _emitted_detector_count(gadget)
        return result

    def build_circuit(self, program: object) -> stim.Circuit:
        """Lower ``program`` and emit the (optionally noisy) stim circuit.

        The returned circuit carries the full DEM annotation
        (``DETECTOR`` and ``OBSERVABLE_INCLUDE`` directives) appended
        after each gadget. Call ``.detector_error_model(...)`` on it
        for the DEM directly, or :meth:`build_dem`.
        """
        program = coerce_program(program, self._qodec.layers[0].isa)
        if self._composes_layers:
            return self._build_circuit_recursive(program)
        lowered = self._compiler.compile(program).program
        return self._build_circuit_from_lowered(lowered)

    def build_dem(
        self,
        program: object,
        *,
        decompose_errors: bool = False,
    ) -> stim.DetectorErrorModel:
        """Build the DEM for ``program`` under this emitter's noise.

        For matching-style decoders pass ``decompose_errors=True``.
        For hypergraph decoders (e.g. relay-BP) leave the default.
        """
        return self.build_circuit(program).detector_error_model(
            decompose_errors=decompose_errors
        )

    def detection_events(
        self,
        program: object,
        physical_readouts: npt.NDArray[np.bool_],
    ) -> npt.NDArray[np.bool_]:
        """Derive detector events from raw measurements.

        Uses stim's ``compile_m2d_converter`` against the (cached)
        emitted circuit. Shape: ``(shots, num_detectors)``.
        """
        events, _ = self._m2d_convert(program, physical_readouts)
        return events

    def observable_flips(
        self,
        program: object,
        physical_readouts: npt.NDArray[np.bool_],
    ) -> npt.NDArray[np.bool_]:
        """Derive observable flips from raw measurements.

        Uses stim's ``compile_m2d_converter`` against the (cached)
        emitted circuit. Shape: ``(shots, num_observables)``.
        """
        _, observables = self._m2d_convert(program, physical_readouts)
        return observables

    def logical_observable_mask(self, program: object) -> npt.NDArray[np.bool_]:
        """Bool mask of shape ``(num_observables,)``.

        ``True`` for observables that come from an `Observe` action atom
        carrying a non-None Pauli (the gadget's logical content).
        ``False`` for flag observables (one per ``gadget.flags`` entry).
        """
        program_coerced = coerce_program(program, self._qodec.layers[0].isa)
        if self._recursive:
            # Logical observables come from the *top* layer's gadget
            # readouts (intermediate readouts are consumed as body records,
            # not emitted as observables).
            return _build_logical_observable_mask(
                program_coerced, self._qodec.layers[0], emit_flags=self._emit_flags
            )
        lowered = self._compiler.compile(program_coerced).program
        return _build_logical_observable_mask(
            lowered, self._stim_layer, emit_flags=self._emit_flags
        )

    def _m2d_convert(
        self,
        program: object,
        physical_readouts: npt.NDArray[np.bool_],
    ) -> tuple[npt.NDArray[np.bool_], npt.NDArray[np.bool_]]:
        circuit = self.build_circuit(program)
        cache_key = id(circuit)
        converter = self._m2d_cache.get(cache_key)
        if converter is None:
            converter = circuit.compile_m2d_converter()
            self._m2d_cache[cache_key] = converter
        events, observables = converter.convert(
            measurements=np.ascontiguousarray(physical_readouts, dtype=np.bool_),
            separate_observables=True,
        )
        return (
            np.asarray(events, dtype=np.bool_),
            np.asarray(observables, dtype=np.bool_),
        )

    def _load_circuit(self, mnemonic: str) -> stim.Circuit:
        if mnemonic not in self._raw_circuits:
            gadget = self._stim_layer.gadgets[mnemonic]
            circuit = stim.Circuit(gadget.circuit.source)
            _reject_source_metadata(circuit, mnemonic)
            self._raw_circuits[mnemonic] = circuit
        return self._raw_circuits[mnemonic]

    def _build_circuit_from_lowered(self, lowered: Program) -> stim.Circuit:
        if lowered.isa.name != self._stim_source_isa.name:
            raise ValueError(
                f"compiler produced a program in ISA {lowered.isa.name!r}; "
                f"expected {self._stim_source_isa.name!r} "
                f"(the layer just above the emitter's bottom layer)"
            )

        allocator = PhysicalQubitAllocator()

        combined = stim.Circuit()
        virtual_records_available = 0
        observable_offset = 0
        # Absolute index of the next measurement record appended to
        # ``combined`` (counting MPAD pads). Used to resolve cross-gadget
        # frames that reach back past intervening gadgets.
        global_measurement_count = 0
        # Boundary signs in flight across gadgets, as the absolute
        # measurement-record sets currently carrying them. Updated from each
        # gadget's ``out[...]`` checks and consumed by later gadgets'
        # ``in[...]`` references. An unseeded logical sign resolves to the empty
        # set (deterministic +1), which reproduces the historical behaviour for
        # static-logical qodecs whose readouts reference ``in[0].z[0]`` purely
        # as documentation.
        frames = FrameMaps()

        for call in lowered.instructions:
            mnemonic = call.mnemonic
            if mnemonic not in self._stim_layer.gadgets:
                raise KeyError(
                    f"no gadget for instruction {mnemonic!r} in lowering "
                    f"{self._stim_source_isa.name!r} -> "
                    f"{self._stim_target_isa.name!r}"
                )
            gadget = self._stim_layer.gadgets[mnemonic]
            base_circuit = self._load_circuit(mnemonic)

            num_needed = _virtual_input_count(gadget)
            if num_needed > virtual_records_available:
                padding = num_needed - virtual_records_available
                # MPAD args are *assertion values* for each padding slot
                # (stim treats `MPAD 0 1` as "pad one record asserted to 0
                # and another asserted to 1"). Virtual stabilizer
                # placeholders for absent prior gadgets should all be 0.
                combined.append("MPAD", [0] * padding)
                virtual_records_available += padding
                global_measurement_count += padding

            noisy_circuit = _inject_noise(base_circuit, self._noise)
            remapped_circuit = remap_call_source(
                noisy_circuit,
                gadget,
                call,
                allocator,
            )
            combined += remapped_circuit
            channel_measurement_count = remapped_circuit.num_measurements

            provenance = Provenance.own_records(
                global_measurement_count, channel_measurement_count
            )
            global_measurement_count += channel_measurement_count

            observable_offset += _append_gadget_directives(
                combined,
                gadget,
                channel_measurement_count,
                observable_offset,
                frames,
                provenance,
                global_measurement_count,
                emit_flags=self._emit_flags,
            )

            virtual_records_available = channel_measurement_count

        return combined

    def _build_circuit_recursive(self, program: Program) -> stim.Circuit:
        """Emit a stim circuit by walking the full translation chain.

        Unlike :meth:`_build_circuit_from_lowered` (which sees only the
        bottom translation's surface), this recurses through every
        translation, composing each intermediate edge's checks / frames /
        readouts into the flat circuit. Logical observables are emitted once,
        from the top-level program's gadget readouts. Only the fully-declared
        gadget subset is supported; features that defer surface
        reconstruction to the decoder (``capture``, ``assume``, intermediate
        flags) raise :class:`NotImplementedError`.
        """
        if program.isa.name != self._qodec.layers[0].isa.name:
            raise ValueError(
                f"recursive emitter expected a program in the qodec's top "
                f"layer {self._qodec.layers[0].isa.name!r}; got {program.isa.name!r}"
            )

        state = _RecursiveEmitState(
            combined=stim.Circuit(),
            allocator=PhysicalQubitAllocator(),
            global_rec=0,
            frames=[FrameMaps() for _ in self._qodec.layers[:-1]],
            noise=self._noise,
        )
        top_layer = self._qodec.layers[0]
        observable_offset = 0

        for call in program.instructions:
            exposed = self._emit_call(state, call, 0)
            gadget = top_layer.gadgets[call.mnemonic]
            if gadget.implements.flags and self._emit_flags:
                raise NotImplementedError(
                    f"gadget {call.mnemonic!r} carries flags; the layer-composing "
                    f"emitter does not yet compose flag observables across layers"
                )
            for slot in observable_slots(gadget):
                targets = [
                    stim.target_rec(-(state.global_rec - record))
                    for record in sorted(exposed[slot.name])
                ]
                state.combined.append("OBSERVABLE_INCLUDE", targets, observable_offset)
                observable_offset += 1

        return state.combined

    def _emit_call(
        self,
        state: "_RecursiveEmitState",
        call: qc.instructions.InstructionCall,
        level: int,
    ) -> dict[str, frozenset[int]]:
        """Emit ``call`` at lowering edge ``level``; return the physical records
        behind each readout it exposes to its parent.

        Side effects: appends this call's body (recursively) and this level's
        detectors to ``state.combined``, and updates ``state.frames[level]``.
        """
        layer = self._qodec.layers[level]
        gadget = layer.gadgets.get(call.mnemonic)
        if gadget is None:
            raise KeyError(
                f"no gadget for instruction {call.mnemonic!r} in lowering "
                f"{self._qodec.layers[level].isa.name!r} -> "
                f"{self._qodec.layers[level + 1].isa.name!r}"
            )

        is_bottom = level == len(self._qodec.layers) - 2
        if is_bottom:
            base_circuit = self._load_circuit(call.mnemonic)
            noisy_circuit = _inject_noise(base_circuit, self._noise)
            remapped_circuit = remap_call_source(
                noisy_circuit, gadget, call, state.allocator
            )
            state.combined += remapped_circuit
            measurement_count = remapped_circuit.num_measurements
            provenance = Provenance.own_records(state.global_rec, measurement_count)
            state.global_rec += measurement_count
        else:
            if gadget.implements.flags and self._emit_flags:
                raise NotImplementedError(
                    f"gadget {call.mnemonic!r} carries flags on an "
                    f"intermediate layer; the layer-composing emitter only "
                    f"supports flags on the top-level program"
                )
            remap = _build_namespaced_remap(
                gadget,
                call,
                call.mnemonic,
                namespace_internal_blocks=True,
            )
            child_layer = self._qodec.layers[level + 1]
            body_records: list[frozenset[int]] = []
            for body_call in gadget.circuit.instructions:
                child_call = _remap_call(body_call, remap)
                child_exposed = self._emit_call(state, child_call, level + 1)
                child_gadget = child_layer.gadgets[child_call.mnemonic]
                for slot in observable_slots(child_gadget):
                    body_records.append(child_exposed[slot.name])
            provenance = Provenance(tuple(body_records))

        frames = state.frames[level]
        self._emit_composed_detectors(state, gadget, provenance, frames)
        update_frame_maps(gadget, provenance, frames, seed_deterministic=True)
        return exposed_readout_records(gadget, provenance, frames)

    def _emit_composed_detectors(
        self,
        state: "_RecursiveEmitState",
        gadget: qc.Gadget,
        provenance: Provenance,
        frames: FrameMaps,
    ) -> None:
        for check in parse_equations(gadget.checks):
            if _has_out_stab(check):
                continue
            records = resolve_records(check, provenance, frames, gadget, strict=True)
            targets = [
                stim.target_rec(-(state.global_rec - r)) for r in sorted(records)
            ]
            state.combined.append("DETECTOR", targets)


class StimSampler(Target[Batch]):
    """Compile programs to stim circuits, inject noise, sample.

    Thin layer over :class:`StimEmitter`: the emitter handles all
    qodec-aware circuit construction (including DEM annotations), and
    this class adds the detector-sampler invocation plus a
    :class:`SampleResult` with the logical-observable mask.

    The emitter is accessible via :attr:`emitter` for callers (e.g.
    decoders) that only need the circuit / DEM and not the sampling.
    """

    def __init__(
        self,
        qodec: qc.Qodec,
        *,
        noise: dict[str, float] | None = None,
        compiler: Compiler | None = None,
        emitter: StimEmitter | None = None,
        emit_flags: bool = True,
    ) -> None:
        super().__init__(qodec)
        if emitter is None:
            emitter = StimEmitter(
                qodec, noise=noise, compiler=compiler, emit_flags=emit_flags
            )
        elif noise is not None or compiler is not None:
            raise ValueError(
                "StimSampler(emitter=…) is mutually exclusive with the "
                "noise/compiler kwargs; pass them to StimEmitter directly"
            )
        elif emitter.qodec is not qodec:
            raise ValueError(
                "StimSampler(qodec, emitter=…): emitter is bound to a "
                "different qodec"
            )
        self._emitter = emitter

    @property
    def emitter(self) -> StimEmitter:
        return self._emitter

    @property
    def compiler(self) -> Compiler:
        return self._emitter.compiler

    @property
    def translation(self) -> qc.Layer:
        """The bottom layer: the one whose gadgets drive stim emission."""
        return self._emitter.translation

    @property
    def noise(self) -> dict[str, float]:
        return self._emitter.noise

    def detector_counts(self) -> dict[str, int]:
        """Detector counts per gadget mnemonic in the bottom translation."""
        return self._emitter.detector_counts()

    def build_circuit(self, program: object) -> stim.Circuit:
        """Lower ``program`` and emit the noisy stim circuit it represents.

        Public so that decoders and other tools can reuse the sampler's
        circuit construction (for DEM export, visualisation, etc.) without
        re-implementing the gadget-concatenation logic.
        """
        return self._emitter.build_circuit(program)

    def execute(self, program: object, *, shots: int) -> Batch:
        circuit = self._emitter.build_circuit(program)
        sampler = circuit.compile_sampler()
        measurements = np.asarray(sampler.sample(shots), dtype=np.bool_)
        rows: list[list[bool]] = measurements.tolist()
        return rows


def _build_logical_observable_mask(
    program: Program, translation: qc.Layer, *, emit_flags: bool = True
) -> npt.NDArray[np.bool_]:
    """Mark each observable column as logical (True) or flag/check (False).
    A column is logical when it comes from an `Observe` action atom (every
    observe outcome carries a Pauli). Flag columns (emitted alongside the
    gadget's Pauli observables) are always non-logical.
    """
    mask: list[bool] = []
    for call in program.instructions:
        gadget = translation.gadgets.get(call.mnemonic)
        if gadget is None:
            continue
        # Every observe outcome is a logical (Pauli-bearing) observable; the
        # trailing flag entries are not.
        for slot in readout_slots(gadget):
            if slot.is_flag and not emit_flags:
                continue
            mask.append(not slot.is_flag)
    return np.array(mask, dtype=np.bool_)


def _virtual_input_count(gadget: qc.Gadget) -> int:
    count = 0
    for encoding in gadget.inputs:
        count += len(encoding.code.stabilizers)
    return count


def _reject_source_metadata(circuit: stim.Circuit, mnemonic: str) -> None:
    forbidden = {"DETECTOR", "OBSERVABLE_INCLUDE"}
    found: set[str] = set()
    for instruction in circuit:
        if isinstance(instruction, stim.CircuitInstruction):
            if instruction.name in forbidden:
                found.add(instruction.name)
    if found:
        raise ValueError(
            f"channel {mnemonic!r}: stim source contains "
            f"{sorted(found)} directives; remove them and let the "
            f"gadget's checks/observables drive metadata"
        )


def _emitted_detector_count(gadget: qc.Gadget) -> int:
    """Number of DETECTORs this target emits for the gadget."""
    return sum(
        1 for check in parse_equations(gadget.checks) if not _has_out_stab(check)
    )


def _append_gadget_directives(
    combined: stim.Circuit,
    gadget: qc.Gadget,
    channel_measurement_count: int,
    observable_offset: int,
    frames: FrameMaps,
    provenance: Provenance,
    global_measurement_count: int,
    *,
    emit_flags: bool = True,
) -> int:
    n = channel_measurement_count
    stab_offset_from_end = _stab_offset_from_end_map(gadget)

    def rec_targets(records: Iterable[int]) -> list[stim.GateTarget]:
        return [
            stim.target_rec(-(global_measurement_count - record))
            for record in sorted(records)
        ]

    for check in parse_equations(gadget.checks):
        if _has_out_stab(check):
            continue
        targets: list[stim.GateTarget] = [
            stim.target_rec(-(n - outcome)) for outcome in outcomes_of(check)
        ]
        for sign in stabilizer_signs_of(check, side="in"):
            if sign.key in frames.stabilizers:
                # Cross-gadget frame: this stabilizer's value is carried by
                # the XOR of these absolute measurement records, which may
                # live in any earlier gadget (not just the adjacent one).
                targets.extend(rec_targets(frames.stabilizers[sign.key]))
            else:
                # Backward-compatible positional fallback: reach into the
                # immediately preceding gadget's records (padded by MPAD).
                targets.append(
                    stim.target_rec(-(n + 1 + stab_offset_from_end[sign.key]))
                )
        combined.append("DETECTOR", targets)

    # Flags are emitted as observables too, so the sampled column layout matches
    # the gadget's own readout order: observables first, then flags.
    emitted = [slot for slot in readout_slots(gadget) if emit_flags or not slot.is_flag]
    for offset, slot in enumerate(emitted):
        combined.append(
            "OBSERVABLE_INCLUDE",
            rec_targets(resolve_records(slot.equation, provenance, frames, gadget)),
            observable_offset + offset,
        )

    update_frame_maps(gadget, provenance, frames, seed_deterministic=False)
    return len(emitted)


def _stab_offset_from_end_map(gadget: qc.Gadget) -> dict[tuple[int, int], int]:
    encodings = list(gadget.inputs)
    total = sum(len(e.code.stabilizers) for e in encodings)
    result: dict[tuple[int, int], int] = {}
    position = 0
    for entry, encoding in enumerate(encodings):
        for stab_idx in range(len(encoding.code.stabilizers)):
            result[(entry, stab_idx)] = total - 1 - position
            position += 1
    return result


def _inject_noise(circuit: stim.Circuit, noise: dict[str, float]) -> stim.Circuit:
    if not noise:
        return circuit

    noisy = stim.Circuit()
    for instruction in circuit:
        if isinstance(instruction, stim.CircuitInstruction):
            name = instruction.name
            targets = instruction.targets_copy()
            qubit_targets = [
                t.value for t in targets if not t.is_measurement_record_target
            ]

            if name == "M" and "p_meas" in noise and noise["p_meas"] > 0:
                for qubit in qubit_targets:
                    noisy.append("X_ERROR", [qubit], [noise["p_meas"]])
                noisy.append(instruction)
            elif (
                name in ("H", "S", "S_DAG")
                and "p_data" in noise
                and noise["p_data"] > 0
            ):
                noisy.append(instruction)
                for qubit in qubit_targets:
                    noisy.append("DEPOLARIZE1", [qubit], [noise["p_data"]])
            elif (
                name in ("CX", "CZ", "CY") and "p_data" in noise and noise["p_data"] > 0
            ):
                for i in range(0, len(qubit_targets), 2):
                    noisy.append(name, qubit_targets[i : i + 2])
                    noisy.append(
                        "DEPOLARIZE2",
                        qubit_targets[i : i + 2],
                        [noise["p_data"]],
                    )
            else:
                noisy.append(instruction)
        else:
            noisy.append(instruction)
    return noisy
