"""Compilers: rewrite a Program from one ISA layer of a Codec to another.

A compiler takes a `Program` and produces another `Program` (in the same
or a different ISA), wrapped in a `CompileResult`.

Recursive lowering (`RecursiveLowering`) walks a codec's translation
chain top-to-bottom, substituting each source instruction with the
gadget that realizes it. Block qubits in the lowered program are
labeled with namespaces of the form ``"<block_name>.<index>"``.

Relocation compilers (`Relocate`, `AutoRelocate`) follow lowering to
rewrite namespaced labels into concrete physical qubit identifiers
(typically integers).

To compile only a portion of a codec's chain, slice it with
`Codec.subcodec(top, bottom)` first.
"""

from .compiler import CompileResult, Compiler
from .identity import IdentityCompiler
from .lowering import RecursiveLowering
from .relocation import AutoRelocate, Relocate

__all__ = [
    "AutoRelocate",
    "CompileResult",
    "Compiler",
    "IdentityCompiler",
    "RecursiveLowering",
    "Relocate",
]
