"""Analysis engines shared by more than one ``qdk.ec`` module.

Nothing here is public API. A module earns a place in this package by having
several consumers — the propagation interpreter and stabilizer algebra behind
the private channel-action, check, completion, distance, fault, profile,
readout, synthesis, and audit modules. Machinery with a single consumer lives
in that module instead.

Import the submodules directly; the layout here is free to change.
"""
