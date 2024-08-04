If all three players return TRUE, then A $\oplus$ B $\oplus$ C == TRUE by necessity (since the sum of their bits is odd).
This will win against inputs 011, 101, and 110 and lose against 000.
Another solution is one player retuns TRUE, and two others return FALSE.

Since the four above inputs have equal probability, and represent all possible inputs,
either of these deterministic strategies wins with $75\%$ probability.

@[solution]({
    "id": "nonlocal_games__ghz_classical_strategy_solution",
    "codePath": "Solution.qs"
})
