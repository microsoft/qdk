"""Analysis engines shared by more than one ``qdk.ec`` module.

Nothing here is public API. A module earns a place in this package by having
several consumers — the propagation interpreter and stabilizer algebra behind
the private action, check, code, distance, equivalence, fault, readout, and
audit modules. Machinery with a single public
home lives in that public module instead.

Import from the public modules; the layout here is free to change.
"""
