<h2 style="color:#D30982;">Part 5: Comparing Selectors</h2>

With `energy_window` registered, it participates in the same `create()` / `settings()` / `run()` workflow as any built-in selector. Varying `window_hartree` controls how many orbitals are included: a narrow window (0.3 Ha) captures only the few orbitals near the gap; a wider window (1.0 Ha) captures more of the occupied-virtual frontier.

The comparison below shows how the energy-window active space relates to the `qdk_valence` selection used throughout this course. For strongly correlated systems like stretched N₂, occupation-based and entropy-based selectors (Chapters 2–3) are more physically targeted; energy-window selection is useful as a validation cross-check or when post-HF data is not yet available.