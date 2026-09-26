---
title: Qodec Runtime Fixtures
description: Provenance and scope of the bundled runtime conformance fixtures.
---

## Provenance

The [Steane](steane/qodec.yaml) and [C4/C6](c4c6/qodec.yaml) fixtures come from
[microsoft/qdk-ec revision 3e5d197706986ef640c1f46ebc9765e64fbf10dc](https://github.com/microsoft/qdk-ec/tree/3e5d197706986ef640c1f46ebc9765e64fbf10dc/qodec/examples),
under its [MIT license](https://github.com/microsoft/qdk-ec/blob/3e5d197706986ef640c1f46ebc9765e64fbf10dc/LICENSE).

Qodec 0.1.0 loaded the upstream artifacts and saved self-contained bundles with
`save(destination, single_file=True)`. Their gadget semantics are unchanged.

The [three-qubit repetition code](repetition3.qodec.yaml) and runtime tests were
ported from the Qodec runtime sandbox. Tests require no network access.

The [C4 fixture](c4.qodec.yaml) extends the vendored C4 test qodec with
`__quantum__qis__x__body`, `__quantum__qis__m__body`, and
`__quantum__qis__mresetz__body`, and its copy in the qdk_ec sample notebooks
matches it.

## QIR instruction names

A QIR call runs the top-layer instruction whose mnemonic is exactly the callee's
name, such as `__quantum__qis__m__body` for a Q# `M`. The repetition and C4
fixtures declare those names so QIR programs can run on them. The Steane and
C4/C6 fixtures keep their upstream names and serve layer-level tests only.

## Coverage

[Conformance tests](../test_conformance.py) exercise Steane preparation,
measurement, and flagged syndrome rounds; C4 frame transport; and C4/C6
preparation and teleporting idle.

Full C4/C6 uses the optional Stim tableau backend and noiseless frame tracking.
These cases do not establish noisy decoding performance, a fault-tolerance
threshold, or support for every program using those instruction sets.
