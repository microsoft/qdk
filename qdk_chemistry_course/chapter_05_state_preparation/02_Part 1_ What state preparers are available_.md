<h2 style="color:#D30982;">Part 1: What state preparers are available?</h2>

`available("state_prep")` lists both preparers. `print_settings()` exposes three configurable parameters:

- `transpile` — if `True`, the synthesized circuit is recompiled into a specific native gate set after isometry synthesis. Leave `False` for simulation; set `True` when targeting hardware with a restricted gate vocabulary.
- `basis_gates` — the list of native gate names to target during transpilation (e.g., `["cx", "u3"]`). Only active when `transpile=True`; ignored otherwise.
- `dense_preparation_method` *(sparse only)* — controls how GF2X handles the dense sub-block of the amplitude matrix. The default works well for all wavefunctions seen in this course.