"""Emit ``.deq`` source from a qodec `Codec`+`Translation`(+`Program`).

The output is a ``.deq`` source string suitable for deq's own
``parse(...)`` and ``build_jit_library(...)``. We deliberately keep
this layer text-based: it leans on deq's mature parser/builder pipeline
for all the heavy lifting (check discovery, propagation matrices,
error-model construction).
"""

from __future__ import annotations

import re
from collections.abc import Callable, Iterable
from io import StringIO

import stim

import qodec
from qodec.actions import Observe

from qdk.ec._qodec_compat import (
    Channel,
    observe_count,
    outcome_indices,
    realization,
    _readout_equation,
)


def to_deq_source(
    codec: qodec.Codec,
    *,
    translation_index: int = -1,
    program: object | None = None,
    program_name: str = "Program",
) -> str:
    """Render ``codec`` as a ``.deq`` source string.

    Parameters
    ----------
    codec :
        The qodec codec to translate.
    translation_index :
        The *top* of the emitted translation stack and the layer the
        ``program`` is written against. Translations from this index down
        to the bottom (the stim layer) are all emitted, preserving the
        codec's abstraction layers: the bottom translation becomes
        physical ``GADGET`` blocks, and every translation above it becomes
        a ``COMPOSE`` block whose body applies the gadgets of the layer
        just below. Defaults to the bottom translation (``-1``), which
        emits a single flat layer of stim ``GADGET`` blocks (the common
        case). Pass ``0`` to emit the full stack from the top logical
        layer down.
    program :
        Optional ``qodec.Program``-like object to emit as a ``PROGRAM``
        block. May be a `Program` or any object whose ``.instructions``
        yields ``InstructionCall`` instances.
    program_name :
        Name to use for the emitted ``PROGRAM`` block.

    Post-selection
    --------------
    ``PRESELECT`` statements are emitted from each call's ``assume``
    clause. ``call.assume`` is a list of AND-conjunctions, each a
    ``{flag_name: expected_bit}`` mapping; a single AND-clause is the
    only shape currently supported (multi-clause OR is not expressible
    as a single ``PRESELECT``). Calls of the same mnemonic must agree
    on their assume clause; if they differ, emit per-call specialised
    gadgets instead.

    Without a program, or with all calls leaving ``assume`` empty, no
    ``PRESELECT`` is emitted — gadgets remain usable without forcing
    rejection.
    """
    translations = codec.layers[:-1]
    n_translations = len(translations)
    if n_translations == 0:
        raise ValueError("codec has no translations to emit")
    top = translation_index % n_translations
    bottom = n_translations - 1
    emitted = list(range(top, n_translations))
    assumed_flags = _collect_assumed_flags(program)
    resolve_name = _build_name_resolver(translations, emitted)

    out = StringIO()
    _emit_header(out, codec, emitted)
    for name, code in codec.codes.items():
        _emit_code(out, name, code)
    # Emit bottom-up so each COMPOSE references gadgets already declared
    # (deq's compose builder rejects forward references).
    for ti in reversed(emitted):
        layer = translations[ti]
        for mnemonic, gadget in layer.gadgets.items():
            deq_name = resolve_name(ti, mnemonic)
            if ti == bottom:
                if not _is_stim_emittable(gadget):
                    out.write(
                        f"# skipped gadget {deq_name!r}: body is not a stim "
                        f"circuit and has no .deq representation\n\n"
                    )
                    continue
                # Single-layer (top == bottom) keeps post-selection; in a
                # multi-layer stack PRESELECT can't live on the physical
                # gadget when the assertion is declared a layer above.
                expected = assumed_flags.get(mnemonic, {}) if top == bottom else {}
                _emit_gadget(out, deq_name, gadget, expected)
            else:
                _emit_compose(out, deq_name, gadget, ti, resolve_name)
    if program is not None:
        _emit_program(out, program_name, program, top, resolve_name)
    return out.getvalue()


def _build_name_resolver(
    translations: list[qodec.Layer], emitted: list[int]
) -> Callable[[int, str], str]:
    """Return a ``(translation_index, mnemonic) -> deq_name`` resolver.

    A mnemonic that is unique across all emitted translations keeps its
    bare name (so a single-layer export is byte-identical to before). A
    mnemonic realized at more than one emitted layer is disambiguated by
    its gadget's primary code name (``prepare_z_all__C6`` vs
    ``prepare_z_all__C4``), falling back to the translation index if the
    code names also collide.
    """
    counts: dict[str, int] = {}
    for ti in emitted:
        for mnemonic in translations[ti].gadgets:
            counts[mnemonic] = counts.get(mnemonic, 0) + 1

    def resolve(ti: int, mnemonic: str) -> str:
        if counts.get(mnemonic, 0) <= 1:
            return mnemonic
        code = _primary_code_name(translations[ti].gadgets[mnemonic])
        suffix = code if code else f"t{ti}"
        return f"{mnemonic}__{suffix}"

    return resolve


def _primary_code_name(gadget: qodec.Gadget) -> str | None:
    """The code name that identifies a gadget's encoding layer.

    Uses the output encoding's code when present (preparations,
    pass-throughs), else the input encoding's code (measurements).
    Returns ``None`` for a gadget with no encodings.
    """
    channel = realization(gadget)
    for enc in list(channel.encoding_out) + list(channel.encoding_in):
        return str(enc.code.name)
    return None


def _is_stim_emittable(gadget: qodec.Gadget) -> bool:
    """Whether a bottom-layer gadget's body is a stim circuit deq can hold.

    A ``.deq`` ``GADGET`` body is stim. Gadgets with a non-stim body (e.g. a
    parameterized ``rotate_z`` authored as inline YAML) have no ``.deq``
    representation, so :func:`to_deq` skips them rather than emit garbage.
    """
    try:
        stim.Circuit(realization(gadget).body)
    except ValueError:
        return False
    return True


def _collect_assumed_flags(program: object | None) -> dict[str, dict[str, int]]:
    """Walk ``program`` and collect, per mnemonic, the AND-clause of
    expected flag bits.

    Returns ``{mnemonic: {flag_name: expected_bit}}`` for those
    mnemonics that some call asserts. Raises ``ValueError`` if two
    calls of the same mnemonic declare different assumptions (a single
    gadget definition can't express both), or if a call uses
    multi-clause OR (no single ``PRESELECT`` can encode that).
    """
    if program is None:
        return {}
    instructions = getattr(program, "instructions", None)
    if instructions is None:
        return {}
    seen: dict[str, dict[str, int]] = {}
    for call in instructions:
        assume = getattr(call, "assume", None) or []
        if not assume:
            clause: dict[str, int] = {}
        elif len(assume) == 1:
            clause = dict(assume[0])
        else:
            raise ValueError(
                f"{call.mnemonic!r}: multi-clause OR assume "
                f"({len(assume)} clauses) is not expressible as a "
                f"single PRESELECT"
            )
        existing = seen.get(call.mnemonic)
        if existing is None:
            seen[call.mnemonic] = clause
        elif existing != clause:
            raise ValueError(
                f"calls of {call.mnemonic!r} use inconsistent assume "
                f"clauses: {existing} vs {clause}; emit per-call "
                f"specialised gadgets if you need both"
            )
    return seen


def _emit_header(out: StringIO, codec: qodec.Codec, emitted: list[int]) -> None:
    layers = codec.layers
    if len(emitted) == 1:
        ti = emitted[0]
        desc = f"translation #{ti}: {layers[ti].isa.name} -> {layers[ti + 1].isa.name}"
    else:
        stack = " -> ".join(
            [layers[ti].isa.name for ti in emitted] + [layers[emitted[-1] + 1].isa.name]
        )
        desc = f"translations #{emitted[0]}..#{emitted[-1]} ({stack})"
    out.write(f"# auto-generated from qodec codec {codec.name!r} ({desc})\n\n")


# ---------------------------------------------------------------------------
# CODE block
# ---------------------------------------------------------------------------


def _emit_code(out: StringIO, name: str, code: qodec.Code) -> None:
    out.write(f"CODE {name} {_code_parameters(code)} {{\n")
    for x_op, z_op in zip(list(code.x), list(code.z)):
        x_term = _pauli_term(str(x_op))
        z_term = _pauli_term(str(z_op))
        out.write(f"    LOGICAL {x_term} {z_term}\n")
    if code.stabilizers:
        out.write("    STABILIZER")
        for stab in code.stabilizers:
            out.write(f" {_pauli_term(str(stab))}")
        out.write("\n")
    out.write("}\n\n")


def _code_parameters(code: qodec.Code) -> str:
    """Render the ``[[n,k,d]]`` parameter triple.

    ``n`` is the physical qubit count, inferred from the highest index
    used in any stabilizer/logical. ``k`` is the number of logical
    qubits. ``d`` is left as ``1`` — qodec doesn't carry distance, and
    the value is not used by the JIT pipeline.
    """
    n = _qubit_count(code)
    k = len(list(code.x))
    return f"[[{n},{k},1]]"


def _qubit_count(code: qodec.Code) -> int:
    """Highest qubit index referenced + 1 across all Pauli strings."""
    high = -1
    for op in code.stabilizers:
        high = max(high, _max_qubit_index(str(op)))
    for x_op, z_op in zip(list(code.x), list(code.z)):
        high = max(high, _max_qubit_index(str(x_op)), _max_qubit_index(str(z_op)))
    return high + 1


def _max_qubit_index(pauli_string: str) -> int:
    """Largest qubit index appearing in a string like 'X_0 Z_3 Y_5'."""
    high = -1
    for term in pauli_string.split():
        if "_" not in term:
            continue
        try:
            idx = int(term.split("_", 1)[1])
        except ValueError:
            continue
        high = max(high, idx)
    return high


def _pauli_term(pauli_string: str) -> str:
    """Convert a qodec Pauli string ('X_0 X_1 X_2') to .deq syntax ('X0*X1*X2')."""
    parts: list[str] = []
    for term in pauli_string.split():
        if "_" not in term:
            parts.append(term)
            continue
        op, idx = term.split("_", 1)
        parts.append(f"{op}{idx}")
    return "*".join(parts) if parts else "I"


# ---------------------------------------------------------------------------
# GADGET block — implemented stub for now
# ---------------------------------------------------------------------------

#: qodec stabilizer-boundary reference shape that maps to a deq virtual record.
_BOUNDARY_STAB_REF = re.compile(r"(in|out)\[(\d+)\]\.stabilizers\[(\d+)\]$")


def _emit_gadget(
    out: StringIO,
    name: str,
    gadget: qodec.Gadget,
    expected_flags: dict[str, int] | None = None,
) -> None:
    channel = realization(gadget)
    body_lines = [
        stripped
        for line in channel.body.splitlines()
        if (stripped := line.strip()) and not stripped.startswith("#")
    ]
    measurement_count = sum(_stim_measurement_delta(line) for line in body_lines)
    check_lines = _check_lines(gadget, channel, measurement_count)

    if check_lines:
        out.write('@CHECKS("manual", verify=0)\n')
    out.write(f"GADGET {name} {{\n")
    for enc in channel.encoding_in:
        out.write(f"    INPUT {enc.code.name} {_qubit_list(enc.support)}\n")
    if channel.encoding_in:
        out.write("\n")

    for line in body_lines:
        out.write(f"    {line}\n")

    for line in _preselect_lines(gadget, measurement_count, expected_flags or {}):
        out.write(f"    {line}\n")
    for line in _readout_lines(gadget, measurement_count):
        out.write(f"    {line}\n")

    for enc in channel.encoding_out:
        out.write(f"    OUTPUT {enc.code.name} {_qubit_list(enc.support)}\n")
    # CHECK statements come after OUTPUT so deq's running record count includes
    # the output-virtual stabilizer measurements they may reference.
    for line in check_lines or []:
        out.write(f"    {line}\n")
    out.write("}\n\n")


def _check_lines(
    gadget: qodec.Gadget, channel: Channel, measurement_count: int
) -> list[str] | None:
    """Render the gadget's checks as deq ``CHECK rec[-k]`` statements.

    deq models each input/output boundary stabilizer as a *virtual*
    measurement: an ``INPUT`` port prepends one record per stabilizer, an
    ``OUTPUT`` port appends one, with the real measurements in between. So the
    global record stream is ``[input-virtual | real | output-virtual]`` and
    every qodec check reference resolves to a position in it:

    * ``circuit.readouts[i]`` (possibly a slice/union) -> real measurements,
    * ``in[entry].stabilizers[k]`` -> an input-virtual record,
    * ``out[entry].stabilizers[k]`` -> an output-virtual record.

    Statements are emitted after ``OUTPUT`` (running count ``= total``), so a
    global index ``g`` becomes ``rec[-(total - g)]``. Output-virtual
    stabilizers a gadget deterministically prepares (e.g. ``prepare_z``) carry
    no explicit qodec check; deq still requires them covered, so each uncovered
    output-virtual record gets a single-record ``CHECK`` — but only for a pure
    preparation (no inputs), where that is sound.

    Returns ``None`` to signal "emit no explicit checks for this gadget" — i.e.
    fall back to deq's own check discovery. That happens when a check uses an
    unsupported reference, references more than one output-virtual stabilizer
    (deq allows at most one per unfinished check), or leaves an output
    stabilizer of a *transforming* gadget uncovered (whose check space qodec
    intentionally leaves to discovery). Emitted checks carry ``verify=0``: the
    qodec checks are authoritative, so deq trusts them rather than requiring
    they match its own discovery basis.
    """
    in_stabs = [len(enc.code.stabilizers) for enc in channel.encoding_in]
    out_stabs = [len(enc.code.stabilizers) for enc in channel.encoding_out]
    num_input = sum(in_stabs)
    ov_start = num_input + measurement_count
    total = ov_start + sum(out_stabs)

    lines: list[str] = []
    covered: set[int] = set()
    for check in gadget.checks:
        indices: set[int] = set()
        for ref in check:
            resolved = _check_ref_global(
                str(ref), num_input, ov_start, in_stabs, out_stabs
            )
            if resolved is None:
                return None
            indices.symmetric_difference_update(resolved)
        if sum(1 for g in indices if g >= ov_start) > 1:
            return None
        covered.update(indices)
        recs = " ".join(f"rec[-{total - g}]" for g in sorted(indices))
        lines.append(f"CHECK {recs}")

    uncovered = [g for g in range(ov_start, total) if g not in covered]
    if uncovered:
        # Single-record coverage is only sound for a preparation that sets its
        # output stabilizers without measuring (e.g. ``prepare_z`` = ``R``).
        # Anything else (a transforming gadget, or a prep that measures) leaves
        # its output-stabilizer checks to deq's discovery.
        if num_input or measurement_count:
            return None
        lines.extend(f"CHECK rec[-{total - g}]" for g in uncovered)
    return lines


def _check_ref_global(
    ref: str,
    num_input: int,
    ov_start: int,
    in_stabs: list[int],
    out_stabs: list[int],
) -> list[int] | None:
    """Resolve a qodec check reference to global deq measurement indices.

    Returns the index list (a slice/union expands to several), or ``None`` if
    the reference is not representable as a deq ``CHECK`` target.
    """
    real = outcome_indices([ref])
    if real:
        return [num_input + i for i in real]
    match = _BOUNDARY_STAB_REF.match(ref)
    if match is not None:
        side, entry, index = match.group(1), int(match.group(2)), int(match.group(3))
        if side == "in":
            return [sum(in_stabs[:entry]) + index]
        return [ov_start + sum(out_stabs[:entry]) + index]
    return None


# ---------------------------------------------------------------------------
# COMPOSE block — an upper-translation gadget whose body applies the gadgets
# of the layer just below (preserving the codec's abstraction layers).
# ---------------------------------------------------------------------------


def _emit_compose(
    out: StringIO,
    deq_name: str,
    gadget: qodec.Gadget,
    translation_index: int,
    resolve_name: Callable[[int, str], str],
) -> None:
    """Emit an upper-layer gadget as a ``COMPOSE`` block.

    The gadget's inline-YAML body is a program of calls into the layer
    below; each call becomes a gadget application to that layer's gadget
    (resolved through ``resolve_name`` at ``translation_index + 1``).
    deq's compose builder derives the checks/observables by composing the
    sub-gadgets, so a ``COMPOSE`` carries only its boundary ports and the
    gadget applications — no ``CHECK`` / ``READOUT`` lines.
    """
    out.write(f"COMPOSE {deq_name} {{\n")
    channel = realization(gadget)
    for enc in channel.encoding_in:
        out.write(f"    INPUT {enc.code.name} {_qubit_list(enc.support)}\n")
    for call in gadget.circuit.instructions:
        target = resolve_name(translation_index + 1, call.mnemonic)
        blocks = _body_call_blocks(call)
        line = f"    {target} {_qubit_list(blocks)}".rstrip()
        out.write(f"{line}\n")
    for enc in channel.encoding_out:
        out.write(f"    OUTPUT {enc.code.name} {_qubit_list(enc.support)}\n")
    out.write("}\n\n")


def _body_call_blocks(call: qodec.InstructionCall) -> list[int]:
    """Block indices a body call targets, in port order.

    An inline-YAML body call addresses the layer below by *block index*
    (each block is one encoded instance at that layer). Operand values
    are integers; we take them in declaration order (outputs then inputs,
    de-duplicated) to feed deq's shortcut gadget-application form
    ``Name b0 b1 ...``, whose arity is the sub-gadget's ``max(n_in,
    n_out)``.
    """
    blocks: list[int] = []
    for source in (getattr(call, "inputs", {}), getattr(call, "outputs", {})):
        for value in source.values():
            block = int(value)
            if block not in blocks:
                blocks.append(block)
    return blocks


def _qubit_list(qubits: Iterable[object]) -> str:
    return " ".join(str(q) for q in qubits)


# Operations that produce one measurement record per target qubit. This is
# a conservative subset that covers the stim gates currently used in the
# qodec example codecs; if a future codec adds more measurement-producing
# gates we'll widen this here.
_MEAS_GATES_PER_QUBIT = {"M", "MX", "MY", "MZ", "MR", "MRX", "MRY", "MRZ"}
# Operations that produce one measurement record per pair of qubits.
_MEAS_GATES_PER_PAIR = {"MXX", "MYY", "MZZ"}


def _stim_measurement_delta(stim_line: str) -> int:
    """Return how many measurement records ``stim_line`` produces.

    Used to track the measurement count emitted so far within a gadget,
    which we need to translate ``body.readouts[i]`` references into
    ``rec[-N]`` offsets at the end of the gadget body.
    """
    tokens = stim_line.split()
    if not tokens:
        return 0
    head = tokens[0].split("(", 1)[0].upper()
    qubit_count = sum(1 for t in tokens[1:] if t.lstrip("!-").isdigit())
    if head in _MEAS_GATES_PER_QUBIT:
        return qubit_count
    if head in _MEAS_GATES_PER_PAIR:
        return qubit_count // 2
    if head == "MPAD":
        return qubit_count
    return 0


def _readout_lines(gadget: qodec.Gadget, measurement_count: int) -> list[str]:
    """Emit a ``READOUT`` statement per logical observable declared by
    the gadget's objective.

    deq's ``READOUT`` syntax accepts ``rec[-N]`` references and XORs
    them implicitly when several are listed on one line.
    """
    lines: list[str] = []
    position = 0
    for atom in gadget.implements.action:
        if not isinstance(atom, Observe):
            continue
        for _observable in atom.observables:
            record_refs = (
                list(gadget.readouts[position])
                if position < len(gadget.readouts)
                else []
            )
            position += 1
            if not record_refs:
                continue
            indices = outcome_indices(record_refs)
            if not indices:
                continue
            recs = [_index_to_rec(i, measurement_count) for i in indices]
            lines.append("READOUT " + " ".join(recs))
    return lines


def _index_to_rec(i: int, measurement_count: int) -> str:
    """Translate a 0-indexed measurement record into stim's ``rec[-N]``
    syntax, given the total measurement count emitted by the gadget."""
    offset = measurement_count - i
    if offset <= 0:
        raise ValueError(
            f"readout index {i} is past the end of the gadget "
            f"({measurement_count} measurements emitted)"
        )
    return f"rec[-{offset}]"


def _readout_to_rec(reference: str, measurement_count: int) -> str:
    """Translate a single-index ``body.readouts[i]`` (or ``body.readouts.i``)
    reference to stim's ``rec[-N]`` syntax. Used at call sites that expect
    exactly one record per reference (e.g. PRESELECT clauses)."""
    indices = outcome_indices([reference])
    if len(indices) != 1:
        raise ValueError(
            f"cannot translate readout reference {reference!r}: "
            "expected a single-index 'body.readouts[i]'"
        )
    return _index_to_rec(indices[0], measurement_count)


def _preselect_lines(
    gadget: qodec.Gadget,
    measurement_count: int,
    expected_flags: dict[str, int],
) -> list[str]:
    """Emit ``PRESELECT`` statements for each flag the program asserts.

    ``expected_flags`` maps flag name to its expected bit value (the
    value at which the shot is *kept*; any other value rejects). Each flag
    is a parity equation living in the trailing entries of the gadget's
    ``readouts`` (after the observe outcomes), positionally aligned with the
    implemented instruction's ``flags`` list.

    Supports the common case of single-record flags. Multi-record
    flags (where the flag is a parity of several measurements) raise
    ``NotImplementedError`` — deq's ``PRESELECT`` is a single-record
    equality and can't express those directly.
    """
    lines: list[str] = []
    flag_names = list(gadget.implements.flags)
    flag_readouts = list(gadget.readouts)[observe_count(gadget) :]
    for flag_name, expected_bit in expected_flags.items():
        if flag_name not in flag_names:
            raise ValueError(
                f"gadget {gadget.implements.mnemonic!r} declares no "
                f"{flag_name!r} flag; cannot honour assumed value"
            )
        flag_index = flag_names.index(flag_name)
        if flag_index >= len(flag_readouts):
            raise ValueError(
                f"gadget {gadget.implements.mnemonic!r}: flag {flag_name!r} "
                f"is declared but not bound to a readout"
            )
        equation = _readout_equation(flag_readouts[flag_index])
        if len(equation) != 1:
            raise NotImplementedError(
                f"gadget {gadget.implements.mnemonic!r}: flag {flag_name!r} "
                f"is a parity of {len(equation)} records; only single-record "
                f"flags can be encoded as PRESELECT"
            )
        # The flag's single record carries the flag parity directly; keep the
        # shot when that record equals the asserted bit.
        rec = _readout_to_rec(equation[0], measurement_count)
        lines.append(f"PRESELECT {rec} {int(expected_bit)}")
    return lines


# ---------------------------------------------------------------------------
# PROGRAM block
# ---------------------------------------------------------------------------


def _emit_program(
    out: StringIO,
    name: str,
    program: object,
    program_layer: int,
    resolve_name: Callable[[int, str], str],
) -> None:
    out.write(f"PROGRAM {name} {{\n")
    instructions = getattr(program, "instructions", None)
    if instructions is None:
        raise TypeError(
            f"program must have an .instructions attribute "
            f"(got a {type(program).__name__})"
        )
    instructions = list(instructions)
    block_indices = _assign_block_indices(instructions)
    for call in instructions:
        operands = _ordered_operand_names(call)
        indices = " ".join(str(block_indices[name]) for name in operands)
        target = resolve_name(program_layer, call.mnemonic)
        out.write(f"    {target} {indices}\n".rstrip() + "\n")

    # Assert all emitted readouts are 0 — sufficient for memory-experiment
    # programs (prepare→...→measure in same basis). Smarter assertions
    # (tracking through frames, conditional outcomes) are a future
    # refinement; for now, this matches what `qdk.ec` users would want
    # for the common LER-sweep workflow.
    isa = getattr(program, "isa", None)
    if isa is not None:
        readout_count = _program_readout_count(instructions, isa)
        for offset in range(readout_count, 0, -1):
            out.write(f"    ASSERT_EQ rec[-{offset}] 0\n")
    out.write("}\n")


def _assign_block_indices(instructions: Iterable[object]) -> dict[str, int]:
    """Collect unique block names across the program in first-seen order
    and assign each a sequential index starting at 0.

    deq's `PROGRAM` block uses positional integer operands; this
    function gives us the qodec-name → deq-index mapping.
    """
    indices: dict[str, int] = {}
    for call in instructions:
        for name in _ordered_operand_names(call):
            if name not in indices:
                indices[name] = len(indices)
    return indices


def _ordered_operand_names(call: qodec.InstructionCall) -> list[str]:
    """Return the union of ``inputs`` and ``outputs`` block names in a
    stable order.

    qodec's `InstructionCall` carries operands as ``inputs`` and
    ``outputs`` dicts keyed by operand slot name. For deq's positional
    convention we need a single ordered tuple. We emit outputs first
    (preparation-like gadgets) then inputs (measurement-like), de-duped
    by block name.
    """
    seen: dict[str, None] = {}
    for source in (getattr(call, "outputs", {}), getattr(call, "inputs", {})):
        for value in source.values():
            if isinstance(value, str) and value not in seen:
                seen[value] = None
    return list(seen)


def _program_readout_count(
    instructions: Iterable[qodec.InstructionCall],
    isa: qodec.InstructionSet,
) -> int:
    """Count the total number of logical readouts the program emits.

    Each `Observe` action atom on a called instruction contributes one
    readout per observable. Calls whose mnemonic is unknown to the ISA
    are silently skipped (the bridge surfaces those as parse errors
    earlier, so they shouldn't appear here in practice).
    """
    by_mnemonic = {instr.mnemonic: instr for instr in isa.instructions.values()}
    total = 0
    for call in instructions:
        instr = by_mnemonic.get(call.mnemonic)
        if instr is None:
            continue
        for atom in instr.action:
            if isinstance(atom, Observe):
                total += len(atom.observables)
    return total
