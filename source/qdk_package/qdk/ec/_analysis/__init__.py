"""Internal analysis engines behind the ``qdk.ec`` profiling surface.

Nothing here is public API. The modules in this package implement the exact
propagation, stabilizer algebra, and solver machinery that the public
:mod:`qdk.ec.action`, :mod:`qdk.ec.checks`, :mod:`qdk.ec.code`,
:mod:`qdk.ec.distance`, :mod:`qdk.ec.faults`, and :mod:`qdk.ec.readouts` modules
present in typed, question-shaped form.

Import from the public modules instead; the layout here is free to change.
"""
