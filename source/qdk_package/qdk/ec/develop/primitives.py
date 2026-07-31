"""Primitive load/save operations for qodec artifacts.

These are thin, ``pathlib``-friendly wrappers over the ``qodec`` package's own
serialization entry points, plus in-memory YAML round-tripping (``from_yaml`` /
``to_yaml``) built on top of qodec's single-file bundle layout.
"""

from __future__ import annotations

import os
import tempfile
from pathlib import Path

import qodec

#: The filename qodec uses for a single-file bundle's manifest.
_MANIFEST_NAME = "qodec.yaml"


def load(path: str | os.PathLike[str]) -> qodec.Qodec:
    """Load a qodec from ``path``.

    ``path`` may be a directory containing a ``qodec.yaml`` manifest (or a
    single ``*.qodec.yaml`` when no canonical manifest exists), or the path to
    a specific ``*.qodec.yaml`` file.
    """
    return qodec.Qodec.load(str(Path(path)))


def save(
    codec: qodec.Qodec,
    path: str | os.PathLike[str],
    *,
    single_file: bool = False,
) -> None:
    """Write ``codec`` to ``path`` as a YAML bundle.

    By default every artifact is written back to its own qodec-root-relative
    path. With ``single_file=True`` the whole qodec is written as one
    multi-document YAML bundle instead.
    """
    destination = Path(path)
    destination.mkdir(parents=True, exist_ok=True)
    codec.save(str(destination), single_file=single_file)


def from_yaml(source: str) -> qodec.Qodec:
    """Parse a single-file qodec YAML bundle from an in-memory string.

    ``source`` is the multi-document YAML produced by :func:`to_yaml` (or by
    ``Qodec.save(..., single_file=True)``). Qodecs whose gadget circuits live in
    external sidecar files cannot be represented as a single string and must be
    loaded from disk with :func:`load` instead.
    """
    with tempfile.TemporaryDirectory() as directory:
        manifest = Path(directory) / _MANIFEST_NAME
        manifest.write_text(source, encoding="utf-8")
        return qodec.Qodec.load(str(manifest))


def to_yaml(codec: qodec.Qodec) -> str:
    """Serialize ``codec`` to a single-file qodec YAML bundle.

    Raises :class:`ValueError` when the qodec has external source-circuit
    sidecars, which a single string cannot carry; use :func:`save` for those.
    """
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        codec.save(str(root), single_file=True)
        written = sorted(path for path in root.rglob("*") if path.is_file())
        manifests = [path for path in written if path.suffix in (".yaml", ".yml")]
        if not manifests:
            raise ValueError("saving the qodec produced no YAML manifest")
        manifest = min(manifests, key=lambda path: len(path.relative_to(root).parts))
        sidecars = [path for path in written if path != manifest]
        if sidecars:
            names = ", ".join(
                str(path.relative_to(root)).replace(os.sep, "/") for path in sidecars
            )
            raise ValueError(
                "qodec has external source-circuit sidecars that a single YAML "
                f"string cannot carry ({names}); use save() instead"
            )
        return manifest.read_text(encoding="utf-8")


__all__ = ["from_yaml", "load", "save", "to_yaml"]
