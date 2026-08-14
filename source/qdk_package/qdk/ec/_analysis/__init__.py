"""Analysis engines shared by more than one ``qdk.ec`` module.

Nothing here is public API. A module earns a place in this package by having
several consumers — the propagation interpreter and stabilizer algebra behind
:mod:`qdk.ec.action`, :mod:`qdk.ec.checks`, :mod:`qdk.ec.code`,
:mod:`qdk.ec.distance`, :mod:`qdk.ec.equivalence`, :mod:`qdk.ec.faults`,
:mod:`qdk.ec.readouts`, :mod:`qdk.ec.lint` and :mod:`qdk.ec.targets`. Machinery
with a single public home lives in that public module instead.

Import from the public modules; the layout here is free to change.
"""
