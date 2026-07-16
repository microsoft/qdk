# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Azure Quantum hardware and simulator targets.

This module re-exports all public symbols from [azure.quantum.target](:mod:`azure.quantum.target`),
making them available under the ``qdk.azure.target`` namespace.

Key exports:

- :class:`azure.quantum.target.Target` — base class for all Azure Quantum targets.
- :class:`azure.quantum.target.IonQ` — IonQ trapped-ion targets.
- :class:`azure.quantum.target.Quantinuum` — Quantinuum trapped-ion targets.
- :class:`azure.quantum.target.rigetti.Rigetti` — Rigetti superconducting targets.
- :class:`azure.quantum.target.pasqal.Pasqal` — Pasqal neutral-atom targets.

Requires the ``azure`` extra: ``pip install qdk[azure]``.
"""

try:
    from azure.quantum.target import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure requires the azure extra. Install with 'pip install qdk[azure]'."
    ) from ex
