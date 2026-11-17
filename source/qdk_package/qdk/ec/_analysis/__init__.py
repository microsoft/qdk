"""Analysis engines shared by more than one ``qdk.ec`` module.

Nothing here is public API. A module belongs in this package when it has
several consumers. This includes the propagation interpreter and stabilizer
algebra used by the private channel-action, check, completion, distance, fault,
profile, readout, build, and audit modules. Code with a single consumer lives
in that module instead.

Import the submodules directly; the layout here is free to change.
"""
