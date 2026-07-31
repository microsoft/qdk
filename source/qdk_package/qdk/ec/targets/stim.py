"""StimSampler: stochastic sampler that compiles to stim and runs the
detector sampler.

A `StimSampler` binds a codec and a noise model at construction. Programs
in any source layer of the codec are first lowered to the second-to-bottom
layer via the supplied compiler (default: `RecursiveLowering`). The
sampler then performs the final hop into stim: each remaining call's
gadget contributes a stim circuit fragment, with detector and observable
directives appended from the gadget's checks and observables.
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np
import numpy.typing as npt

import stim

import qodec
from qodec.circuits import Program

from .compilers import Compiler, RecursiveLowering
from .compilers.recursive_lowering import (
    _build_namespaced_remap,
    _remap_call,
)
from .results import Batch
from .._qodec_compat import (
    check_outcomes,
    observable_names,
    outcome_indices,
    realization,
    _readout_equation,
)
from ._coerce import coerce_program
from ._qubit_alloc import PhysicalQubitAllocator, remap_call_source
from ._recursive_emit import (
    _RecursiveEmitState,
    _call_readout_prov,
    _has_out_stab,
    _observe_names,
    _parse_logical_in_atom,
    _parse_logical_out_atom,
    _parse_stab_in_atom,
    _parse_stab_out_atom,
    _resolve_atoms_records,
    _update_frame_map_recursive,
)
from .base import Target


class StimEmitter:
    """Codec-aware Program → stim circuit (with DEM annotations).

    Knows nothing about sampling. Its sole responsibilities are:

    * lower a Program from any source layer down to the codec's
      bottom-layer ISA (via the supplied ``compiler``);
    * concatenate each gadget's raw stim source;
    * inject gate-level noise (optional);
    * append ``DETECTOR`` and ``OBSERVABLE_INCLUDE`` directives derived
      from the gadget's checks, observables, and flags.

    The codec must have at least one translation. The emitter uses the
    *last* translation (bottom layer) to emit stim; any earlier
    translations are handled by ``compiler`` (default:
    `RecursiveLowering` over the codec's pre-bottom slice).

    .. note::

        **Multi-layer decoding surfaces.** When the codec has more than
        one translation *and* no explicit ``compiler`` is supplied, the
        emitter recurses through every translation, folding each edge's
        ``checks`` / ``frames`` / ``readouts`` down to physical
        measurement records (see :meth:`_build_circuit_recursive`). This
        composes intermediate-layer decoding surfaces into the flat
        circuit rather than discarding them.

        The recursive path targets the *fully declared* subset: gadgets
        whose decoding surface is expressed through declared
        ``body.readouts`` (positional or observe-named), ``checks``,
        ``frames``, and ``readouts``. Features such as ``capture`` /
        ``assume`` readouts, undeclared frames, or flags on
        non-bottom gadgets raise ``NotImplementedError``. Single-
        translation codecs (or any codec given an explicit ``compiler``)
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
        codec: qodec.Qodec,
        *,
        noise: dict[str, float] | None = None,
        compiler: Compiler | None = None,
        emit_flags: bool = True,
    ) -> None:
        if len(codec.layers) < 2:
            raise ValueError(
                "StimEmitter requires a codec with at least two layers "
                "(one lowering edge)"
            )
        layer_count = len(codec.layers)
        self._codec = codec
        self._emit_flags = emit_flags
        # The bottom non-empty layer: its gadgets lower the second-to-bottom
        # ISA into the physical (stim) ISA. (Kept under the historical name
        # ``_stim_translation``; ``.gadgets`` works on a Layer.)
        self._stim_translation = codec.layers[-2]
        self._stim_source_isa = codec.layers[-2].isa
        self._stim_target_isa = codec.layers[-1].isa
        # When the caller supplies no compiler and the codec has more than one
        # lowering edge, the emitter walks the layer chain itself
        # (``_build_circuit_recursive``), composing every intermediate
        # layer's decoding surface (checks / readouts) down to physical
        # records. With a single edge — or a user-supplied compiler that
        # pre-lowers to the bottom-1 layer — the flat single-edge path
        # (``_build_circuit_from_lowered``) is used.
        self._recursive = compiler is None and layer_count > 2
        if compiler is None:
            pre_bottom = codec.slice(0, layer_count - 1)
            compiler = RecursiveLowering(pre_bottom)
        self._compiler = compiler
        self._noise = dict(noise) if noise else {}
        self._raw_circuits: dict[str, stim.Circuit] = {}
        self._m2d_cache: dict[
            int, "stim.CompiledMeasurementsToDetectionEventsConverter"
        ] = {}

    @property
    def codec(self) -> qodec.Qodec:
        return self._codec

    @property
    def compiler(self) -> Compiler:
        return self._compiler

    @property
    def translation(self) -> qodec.Layer:
        """The bottom layer: the one whose gadgets drive stim emission."""
        return self._stim_translation

    @property
    def noise(self) -> dict[str, float]:
        return dict(self._noise)

    def with_noise(self, noise: dict[str, float] | None) -> "StimEmitter":
        """Return a fresh emitter with a new noise dict.

        Shares the codec and compiler with ``self``; raw-circuit cache
        is rebuilt independently so that mutating one emitter cannot
        affect the other.
        """
        return StimEmitter(
            self._codec,
            noise=noise,
            compiler=self._compiler,
            emit_flags=self._emit_flags,
        )

    def detector_counts(self) -> dict[str, int]:
        """Detector counts per gadget mnemonic in the bottom translation."""
        result: dict[str, int] = {}
        for name, gadget in self._stim_translation.gadgets.items():
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
        program = coerce_program(program, self._codec.layers[0].isa)
        if self._recursive:
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
        program_coerced = coerce_program(program, self._codec.layers[0].isa)
        if self._recursive:
            # Logical observables come from the *top* layer's gadget
            # readouts (intermediate readouts are consumed as body records,
            # not emitted as observables).
            return _build_logical_observable_mask(
                program_coerced, self._codec.layers[0], emit_flags=self._emit_flags
            )
        lowered = self._compiler.compile(program_coerced).program
        return _build_logical_observable_mask(
            lowered, self._stim_translation, emit_flags=self._emit_flags
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
            channel = realization(self._stim_translation.gadgets[mnemonic])
            circuit = stim.Circuit(channel.body)
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
        # stabilizer frames that reach back past intervening gadgets.
        global_measurement_count = 0
        # Persistent stabilizer frame map: (operand, stabilizer index) ->
        # the set of absolute measurement-record indices whose XOR currently
        # carries that stabilizer's value. Updated from each gadget's
        # ``out.<op>.stabilizers[i]`` checks and consumed by later gadgets'
        # ``in.<op>.stabilizers[i]`` references.
        frame_map: dict[tuple[int, int], frozenset[int]] = {}
        # Persistent logical-observable frame map: (operand, basis, index) ->
        # the set of absolute measurement-record indices whose XOR currently
        # carries that logical sign's accumulated Pauli frame. Seeded/updated
        # from each gadget's ``out.<op>.(x|z)[i]`` frame declarations and
        # consumed by terminal ``in.<op>.(x|z)[i]`` readout atoms. An unseeded
        # logical frame resolves to the empty set (deterministic +1), which
        # reproduces the historical behaviour for static-logical codecs whose
        # readouts reference ``in.<op>.z[0]`` purely as documentation.
        logical_frame_map: dict[tuple[int, str, int], frozenset[int]] = {}

        for call in lowered.instructions:
            mnemonic = call.mnemonic
            if mnemonic not in self._stim_translation.gadgets:
                raise KeyError(
                    f"no gadget for instruction {mnemonic!r} in translation "
                    f"{self._stim_source_isa.name!r} -> "
                    f"{self._stim_target_isa.name!r}"
                )
            gadget = self._stim_translation.gadgets[mnemonic]
            channel = realization(gadget)
            base_circuit = self._load_circuit(mnemonic)

            num_needed = _virtual_input_count(channel)
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
                channel,
                call,
                allocator,
            )
            combined += remapped_circuit
            channel_measurement_count = remapped_circuit.num_measurements

            body_base = global_measurement_count
            global_measurement_count += channel_measurement_count

            observable_offset += _append_gadget_directives(
                combined,
                gadget,
                channel_measurement_count,
                observable_offset,
                _FrameContext(
                    frame_map=frame_map,
                    logical_frame_map=logical_frame_map,
                    body_base=body_base,
                    global_measurement_count=global_measurement_count,
                ),
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
        if program.isa.name != self._codec.layers[0].isa.name:
            raise ValueError(
                f"recursive emitter expected a program in the codec's top "
                f"layer {self._codec.layers[0].isa.name!r}; got {program.isa.name!r}"
            )

        state = _RecursiveEmitState(
            combined=stim.Circuit(),
            allocator=PhysicalQubitAllocator(),
            global_rec=0,
            frame_maps=[{} for _ in self._codec.layers[:-1]],
            logical_frame_maps=[{} for _ in self._codec.layers[:-1]],
            noise=self._noise,
        )
        top_translation = self._codec.layers[0]
        observable_offset = 0

        for call in program.instructions:
            readout_prov = self._emit_call(state, call, 0)
            gadget = top_translation.gadgets[call.mnemonic]
            if gadget.implements.flags and self._emit_flags:
                raise NotImplementedError(
                    f"gadget {call.mnemonic!r} carries flags; the recursive "
                    f"multi-layer emitter does not yet compose flag "
                    f"observables across translations"
                )
            for name in observable_names(gadget):
                records = readout_prov[name]
                targets = [
                    stim.target_rec(-(state.global_rec - r)) for r in sorted(records)
                ]
                state.combined.append("OBSERVABLE_INCLUDE", targets, observable_offset)
                observable_offset += 1

        return state.combined

    def _emit_call(
        self,
        state: "_RecursiveEmitState",
        call: qodec.instructions.InstructionCall,
        level: int,
    ) -> dict[str, frozenset[int]]:
        """Emit ``call`` at translation ``level``; return its readout
        provenance (``readout name -> physical record indices``).

        Side effects: appends this call's body (recursively) and this
        level's detectors to ``state.combined``, and updates
        ``state.frame_maps[level]``.
        """
        translation = self._codec.layers[level]
        gadget = translation.gadgets.get(call.mnemonic)
        if gadget is None:
            raise KeyError(
                f"no gadget for instruction {call.mnemonic!r} in translation "
                f"{self._codec.layers[level].isa.name!r} -> "
                f"{self._codec.layers[level + 1].isa.name!r}"
            )

        is_bottom = level == len(self._codec.layers) - 2
        if is_bottom:
            base_circuit = self._load_circuit(call.mnemonic)
            noisy_circuit = _inject_noise(base_circuit, self._noise)
            remapped_circuit = remap_call_source(
                noisy_circuit, realization(gadget), call, state.allocator
            )
            state.combined += remapped_circuit
            measurement_count = remapped_circuit.num_measurements
            body_prov = [
                frozenset({state.global_rec + i}) for i in range(measurement_count)
            ]
            state.global_rec += measurement_count
        else:
            if gadget.implements.flags and self._emit_flags:
                raise NotImplementedError(
                    f"gadget {call.mnemonic!r} carries flags on an "
                    f"intermediate translation; the recursive emitter only "
                    f"supports flags on the top-level program"
                )
            remap = _build_namespaced_remap(
                gadget,
                call,
                call.mnemonic,
                namespace_internal_blocks=True,
            )
            child_translation = self._codec.layers[level + 1]
            body_prov = []
            for body_call in realization(gadget).instructions:
                child_call = _remap_call(body_call, remap)
                child_prov = self._emit_call(state, child_call, level + 1)
                child_gadget = child_translation.gadgets[child_call.mnemonic]
                for name in _observe_names(child_gadget):
                    body_prov.append(child_prov[name])

        frame_map = state.frame_maps[level]
        logical_frame_map = state.logical_frame_maps[level]
        self._emit_recursive_detectors(
            state, gadget, body_prov, frame_map, logical_frame_map
        )
        _update_frame_map_recursive(gadget, frame_map, logical_frame_map, body_prov)
        return _call_readout_prov(gadget, body_prov, frame_map, logical_frame_map)

    def _emit_recursive_detectors(
        self,
        state: "_RecursiveEmitState",
        gadget: qodec.Gadget,
        body_prov: list[frozenset[int]],
        frame_map: dict[tuple[int, int], frozenset[int]],
        logical_frame_map: dict[tuple[int, str, int], frozenset[int]],
    ) -> None:
        for check in gadget.checks:
            if _has_out_stab(check):
                continue
            records = _resolve_atoms_records(
                check, body_prov, frame_map, logical_frame_map, gadget
            )
            targets = [
                stim.target_rec(-(state.global_rec - r)) for r in sorted(records)
            ]
            state.combined.append("DETECTOR", targets)


class StimSampler(Target[Batch]):
    """Compile programs to stim circuits, inject noise, sample.

    Thin layer over :class:`StimEmitter`: the emitter handles all
    codec-aware circuit construction (including DEM annotations), and
    this class adds the detector-sampler invocation plus a
    :class:`SampleResult` with the logical-observable mask.

    The emitter is accessible via :attr:`emitter` for callers (e.g.
    decoders) that only need the circuit / DEM and not the sampling.
    """

    def __init__(
        self,
        codec: qodec.Qodec,
        *,
        noise: dict[str, float] | None = None,
        compiler: Compiler | None = None,
        emitter: StimEmitter | None = None,
        emit_flags: bool = True,
    ) -> None:
        super().__init__(codec)
        if emitter is None:
            emitter = StimEmitter(
                codec, noise=noise, compiler=compiler, emit_flags=emit_flags
            )
        elif noise is not None or compiler is not None:
            raise ValueError(
                "StimSampler(emitter=…) is mutually exclusive with the "
                "noise/compiler kwargs; pass them to StimEmitter directly"
            )
        elif emitter.codec is not codec:
            raise ValueError(
                "StimSampler(codec, emitter=…): emitter is bound to a "
                "different codec"
            )
        self._emitter = emitter

    @property
    def emitter(self) -> StimEmitter:
        return self._emitter

    @property
    def compiler(self) -> Compiler:
        return self._emitter.compiler

    @property
    def translation(self) -> qodec.Layer:
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
    program: Program, translation: qodec.Layer, *, emit_flags: bool = True
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
        # trailing readout entries are the flags (non-logical).
        observables = observable_names(gadget)
        for _name in observables:
            mask.append(True)
        if emit_flags:
            for _ in list(gadget.readouts)[len(observables) :]:
                mask.append(False)
    return np.array(mask, dtype=np.bool_)


def _virtual_input_count(channel: qodec.Channel) -> int:
    count = 0
    for encoding in channel.encoding_in:
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


def _emitted_detector_count(gadget: qodec.Gadget) -> int:
    """Number of DETECTORs this target emits for the gadget."""
    return sum(1 for check in gadget.checks if not _has_out_stab(check))


@dataclass(frozen=True)
class _FrameContext:
    """Cross-gadget frame-resolution state for one gadget.

    ``frame_map`` is the persistent (operand, stabilizer index) -> absolute
    record-index-set mapping (mutated in place across gadgets).
    ``logical_frame_map`` is the analogous (operand, basis, index) -> absolute
    record-index-set mapping for logical observable signs (``basis`` is
    ``"x"`` or ``"z"``); it carries a rotating logical's accumulated Pauli
    frame across gadgets so terminal ``in.<op>.(x|z)[i]`` readout atoms
    resolve to the correct records. ``body_base`` is the absolute index of
    this gadget's first body record; it is used when declaring new frames.
    ``global_measurement_count`` is the total number of records appended so
    far (after this gadget's body), used to convert an absolute record index
    into a stim relative ``rec[-k]`` target.
    """

    frame_map: dict[tuple[int, int], frozenset[int]]
    logical_frame_map: dict[tuple[int, str, int], frozenset[int]]
    body_base: int
    global_measurement_count: int


def _append_gadget_directives(
    combined: stim.Circuit,
    gadget: qodec.Gadget,
    channel_measurement_count: int,
    observable_offset: int,
    frames: _FrameContext,
    *,
    emit_flags: bool = True,
) -> int:
    channel = realization(gadget)
    n = channel_measurement_count
    stab_offset_from_end = _stab_offset_from_end_map(channel)

    for check in gadget.checks:
        if _has_out_stab(check):
            continue
        targets: list[stim.GateTarget] = []
        for outcome in check_outcomes(check):
            targets.append(stim.target_rec(-(n - outcome)))
        for atom in check:
            ref = _parse_stab_in_atom(atom)
            if ref is None:
                continue
            if ref in frames.frame_map:
                # Cross-gadget frame: this stabilizer's value is carried by
                # the XOR of these absolute measurement records, which may
                # live in any earlier gadget (not just the adjacent one).
                for absolute in sorted(frames.frame_map[ref]):
                    targets.append(
                        stim.target_rec(-(frames.global_measurement_count - absolute))
                    )
            else:
                # Backward-compatible positional fallback: reach into the
                # immediately preceding gadget's records (padded by MPAD).
                offset = stab_offset_from_end[ref]
                targets.append(stim.target_rec(-(n + 1 + offset)))
        combined.append("DETECTOR", targets)

    new_observable_count = 0
    observables = observable_names(gadget)
    for position, _name in enumerate(observables):
        readout_records = _resolve_observable_records(
            _readout_equation(gadget.readouts[position]), frames
        )
        rec_targets = [
            stim.target_rec(-(frames.global_measurement_count - record))
            for record in sorted(readout_records)
        ]
        combined.append(
            "OBSERVABLE_INCLUDE",
            rec_targets,
            observable_offset + new_observable_count,
        )
        new_observable_count += 1

    if emit_flags:
        # Flags are the trailing readout entries (after the observe outcomes):
        # decoder-blind side-channel bits, emitted as observables so the sampled
        # column layout matches observable_names() followed by the flags.
        for flag_readout in list(gadget.readouts)[len(observables) :]:
            flag_records = _resolve_observable_records(
                _readout_equation(flag_readout), frames
            )
            rec_targets = [
                stim.target_rec(-(frames.global_measurement_count - record))
                for record in sorted(flag_records)
            ]
            combined.append(
                "OBSERVABLE_INCLUDE",
                rec_targets,
                observable_offset + new_observable_count,
            )
            new_observable_count += 1

    _update_frame_map(gadget, frames.frame_map, frames.body_base)
    _update_logical_frame_map(
        gadget, frames.frame_map, frames.logical_frame_map, frames.body_base
    )

    return new_observable_count


def _resolve_observable_records(atoms: list[str], frames: _FrameContext) -> set[int]:
    """Absolute records whose XOR carries an observable readout's value.

    Resolves three atom kinds: ``body.readouts[k]`` (this gadget's own
    measurement, at ``body_base + k``); ``in.<op>.stabilizers[i]`` (via the
    stabilizer frame map); and ``in.<op>.(x|z)[i]`` (via the logical frame
    map — the accumulated Pauli frame of a rotating logical). An unseeded
    logical reference resolves to the empty set (deterministic +1).
    """
    records: set[int] = set()
    for index in outcome_indices(atoms):
        records ^= {frames.body_base + index}
    for atom in atoms:
        stab_ref = _parse_stab_in_atom(atom)
        if stab_ref is not None:
            records ^= set(frames.frame_map.get(stab_ref, frozenset()))
            continue
        logical_ref = _parse_logical_in_atom(atom)
        if logical_ref is not None:
            records ^= set(frames.logical_frame_map.get(logical_ref, frozenset()))
    return records


def _update_logical_frame_map(
    gadget: qodec.Gadget,
    frame_map: dict[tuple[int, int], frozenset[int]],
    logical_frame_map: dict[tuple[int, str, int], frozenset[int]],
    body_base: int,
) -> None:
    """Apply this gadget's ``out[entry].(x|z)[i]`` logical frame declarations.

    Logical frames are *replaced* (full XOR of the declared source atoms),
    exactly like stabilizer frames: when a gadget re-expresses a rotating
    logical's representative, the new record-set carrying its sign is fully
    determined by that round's source atoms. A check carrying an
    ``out[entry].(x|z)[i]`` atom is such a declaration; its sources are the
    check's body readouts, referenced stabilizer frames, and other logical
    frames. Static-logical codecs (c4, surface) declare no out-logical
    atoms, so this leaves ``logical_frame_map`` untouched.
    """
    new_entries: dict[tuple[int, str, int], frozenset[int]] = {}
    for check in gadget.checks:
        logical_outs = [
            ref
            for ref in (_parse_logical_out_atom(atom) for atom in check)
            if ref is not None
        ]
        if not logical_outs:
            continue
        records: set[int] = set()
        for index in outcome_indices(check):
            records ^= {body_base + index}
        for atom in check:
            stab_ref = _parse_stab_in_atom(atom)
            if stab_ref is not None:
                records ^= set(frame_map.get(stab_ref, frozenset()))
                continue
            logical_ref = _parse_logical_in_atom(atom)
            if logical_ref is not None:
                records ^= set(logical_frame_map.get(logical_ref, frozenset()))
        frozen = frozenset(records)
        for out_ref in logical_outs:
            new_entries[out_ref] = frozen
    logical_frame_map.update(new_entries)


def _update_frame_map(
    gadget: qodec.Gadget,
    frame_map: dict[tuple[int, int], frozenset[int]],
    body_base: int,
) -> None:
    """Apply this gadget's frame-propagation declarations to ``frame_map``.

    A frame declares the new record-set carrying an output stabilizer's
    sign as the XOR (symmetric difference of record sets) of the gadget's
    own body readouts and any referenced input stabilizer frames.
    Stabilizers the gadget does not declare keep their existing frame,
    giving carry-forward across gadgets that only re-measure part of the
    code.

    Output-stabilizer frames are declared by ``gadget.checks`` entries that
    carry an ``out[entry].stabilizers[i]`` atom (the ``state-passing`` check
    idiom); each such check's other atoms (body readouts and ``in`` frames)
    XOR to the new frame value.
    """
    new_entries: dict[tuple[int, int], frozenset[int]] = {}

    def record_declaration(
        out_refs: list[tuple[int, int]],
        outcomes: list[int],
        in_refs: list[tuple[int, int]],
    ) -> None:
        if not out_refs:
            return
        if not outcomes and not in_refs:
            # A pure deterministic declaration (e.g. a preparation asserting
            # ``out.block.stabilizers[i]`` with no measured body readout and no
            # carried-forward input frame). The agreed model (Q2) is to seed
            # such a frame to the empty record set (an empty XOR is
            # deterministic ``+1``) — which the recursive emitter does in
            # ``_update_frame_map_recursive``. This flat path instead leaves the
            # frame unset so downstream references fall back to the positional
            # virtual-record model, preserving legacy behaviour for codecs that
            # do not yet declare their preparation frames. This fallback is
            # slated for removal once those codecs declare prep frames, at which
            # point an unseeded ``in`` frame becomes a hard error.
            return
        records: set[int] = set()
        for outcome in outcomes:
            records ^= {body_base + outcome}
        for in_ref in in_refs:
            records ^= set(frame_map.get(in_ref, frozenset()))
        frozen = frozenset(records)
        for out_ref in out_refs:
            new_entries[out_ref] = frozen

    for check in gadget.checks:
        out_refs = [
            ref
            for ref in (_parse_stab_out_atom(atom) for atom in check)
            if ref is not None
        ]
        if not out_refs:
            continue
        in_refs = [
            ref
            for ref in (_parse_stab_in_atom(atom) for atom in check)
            if ref is not None
        ]
        record_declaration(out_refs, list(check_outcomes(check)), in_refs)

    frame_map.update(new_entries)


def _stab_offset_from_end_map(channel: object) -> dict[tuple[int, int], int]:
    encodings = list(channel.encoding_in)  # type: ignore[attr-defined]
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
