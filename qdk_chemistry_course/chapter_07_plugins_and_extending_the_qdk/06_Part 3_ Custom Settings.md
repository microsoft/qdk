<h2 style="color:#D30982;">Part 3: Custom Settings</h2>

Every algorithm has a `Settings` object that exposes configurable parameters via `settings().get()` and `settings().set()`. To add settings to a custom algorithm, subclass `Settings` and call `_set_default()` once per parameter in `__init__`. The arguments are:

- **key** — the string name used in `settings().get(key)` and `settings().set(key, value)`
- **type_str** — one of `'int'`, `'double'`, `'bool'`, `'string'`, `'list[int]'`, `'list[double]'`
- **default** — the value returned before the user calls `set()`
- **description** — shown in `print_settings()` output
- **limits** *(optional)* — `(min, max)` for numeric types, or a list of allowed values for strings

The custom `Settings` subclass is attached to the algorithm in `__init__` via `self._settings = MySettings()`. After calling `register()`, `print_settings("active_space_selector", "energy_window")` will display the parameter table, identical in format to any built-in algorithm.