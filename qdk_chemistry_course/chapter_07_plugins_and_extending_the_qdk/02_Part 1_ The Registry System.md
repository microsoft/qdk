<h2 style="color:#D30982;">Part 1: The Registry System</h2>

`available()` with no arguments returns a dictionary mapping every registered algorithm type to its list of implementations. `registry.show_default()` shows which implementation `create()` picks when no name is specified.

Four functions drive the registry lifecycle:
- `available()` — query what is registered
- `create()` — instantiate by type + name
- `register(generator)` — add a new Python-implemented algorithm; `generator` is a lambda called each time a new instance is needed; it is also called once immediately to discover the algorithm's `type_name()` and `name()`
- `register_factory(factory)` — add an entirely new algorithm type (needed when your algorithm doesn't fit any existing type)