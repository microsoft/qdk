"""Drive deq's pipeline from a qodec codec.

Thin wrappers that emit ``.deq`` source via :mod:`.source_emitter` and
feed it to deq's own pipeline:

* :func:`to_jit_library` — parse + ``build_jit_library`` into a
  ``JitLibrary`` protobuf.
* :func:`to_stim_source` — additionally run deq's stim exporter to
  produce the physical Stim circuit text (ready for a sampler such as
  ``qdk.stim.run``).
"""

from __future__ import annotations

import os
import tempfile
from contextlib import redirect_stdout
from io import StringIO

import qodec

# These imports require the `deq` package to be installed. The bridge is
# optional in qdk.ec; consumers that don't need deq integration can
# avoid importing this module.
from deq.circuit.parser import parse
from deq.cli.jit import jit_compile_program_to_file
from deq.proto import deq_jit_pb2 as jit_pb
from deq.transpiler.jit_library_builder import build_jit_library

from .source_emitter import to_deq_source


def _strip_non_preselect_directives(stim_text: str) -> str:
    """Drop deq-only ``#!`` annotations a QDK sampler can't parse.

    deq prefixes its stim with bang-directives for its own pipeline \u2014
    notably a ``#!rhai`` logical-error predicate block. The QDK's Stim
    front-end treats every ``#!`` line as an instruction and errors on
    anything but ``#!preselect``. We keep ``#!preselect`` (which the QDK
    consumes natively) and ordinary ``#`` comments (which Stim ignores),
    and drop the rest.
    """
    kept = [
        line
        for line in stim_text.splitlines()
        if not (
            line.lstrip().startswith("#!")
            and not line.lstrip().startswith("#!preselect")
        )
    ]
    return "\n".join(kept) + ("\n" if stim_text.endswith("\n") else "")


def to_jit_library(
    codec: qodec.Codec,
    *,
    translation_index: int = -1,
    program: object | None = None,
    program_name: str = "Program",
) -> jit_pb.JitLibrary:
    """Build a deq `JitLibrary` for ``codec``.

    The codec is rendered as ``.deq`` source, then parsed and lowered
    through deq's existing library builder. Any deq-side validation
    errors (unresolved checks, malformed circuits, etc.) surface as
    exceptions from the builder.
    """
    source = to_deq_source(
        codec,
        translation_index=translation_index,
        program=program,
        program_name=program_name,
    )
    deq_file = parse(source)
    return build_jit_library(deq_file)


def to_stim_source(
    codec: qodec.Codec,
    *,
    translation_index: int = -1,
    program: object | None = None,
    program_name: str = "Program",
) -> str:
    """Render ``codec`` + ``program`` as a physical Stim circuit string.

    Drives deq's full pipeline end to end: emit ``.deq`` source, parse it,
    build a ``JitLibrary``, then run deq's stim exporter
    (``jit_compile_program_to_file``) and read back the generated circuit.

    The result is the *physical* circuit deq produces — gates and
    measurements with a single program-wide qubit namespace composed
    across gadgets, plus any native ``#!preselect`` annotations emitted
    from ``PRESELECT`` clauses. Checks and observables are deliberately
    **not** emitted into the circuit: deq keeps the decoding surface in
    its binary ``Library``, so the cross-gadget detector/observable
    resolution is done deq's way rather than duplicated here. The output
    is therefore ready to feed straight to a measurement sampler such as
    ``qdk.stim.run``.

    deq-only ``#!`` directives that the QDK can't parse (e.g. its
    ``#!rhai`` logical-error block) are stripped; ``#!preselect``
    annotations and ordinary ``#`` comments are preserved (see
    :func:`_strip_non_preselect_directives`).

    A ``program`` is required — deq only emits a circuit when compiling a
    ``PROGRAM`` block.
    """
    if program is None:
        raise ValueError("to_stim_source requires a program to emit a stim circuit")

    source = to_deq_source(
        codec,
        translation_index=translation_index,
        program=program,
        program_name=program_name,
    )
    merged = parse(source)
    jit_library = build_jit_library(merged)

    with tempfile.TemporaryDirectory() as tmpdir:
        jit_out = os.path.join(tmpdir, "library.deq.jit")
        stim_out = os.path.join(tmpdir, "library.stim")
        with redirect_stdout(StringIO()):
            jit_compile_program_to_file(
                jit_library, merged, jit_out, program=program_name
            )
        if not os.path.exists(stim_out):
            raise RuntimeError(
                f"deq did not emit a stim circuit for program {program_name!r}"
            )
        with open(stim_out, encoding="utf8") as handle:
            return _strip_non_preselect_directives(handle.read())
