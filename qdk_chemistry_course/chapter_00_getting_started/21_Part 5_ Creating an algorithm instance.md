<h2 style="color:#D30982;">Part 5: Creating an algorithm instance</h2>

`create(type, name)` returns an algorithm instance. Settings can be passed at creation time as keyword arguments, or updated afterwards via `instance.settings().update(...)`.

In the cell below, create two SCF solvers — one with default settings, one with `max_iterations=100` and `convergence_threshold=1e-8`. Confirm the settings took effect by reading them back.