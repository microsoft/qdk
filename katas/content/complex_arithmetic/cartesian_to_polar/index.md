**Input:**
A complex number $x = a + bi$.

**Goal:**
Return the polar representation of $x = re^{i\theta}$, that is, the distance from origin $r$ and phase $\theta$ as a `ComplexPolar`.

* $r$ should be non-negative: $r \geq 0$
* $\theta$ should be between $-\pi$ and $\pi$: $-\pi < \theta \leq \pi$

:::hint
A video explanation of this conversion can be found [here](https://www.youtube.com/watch?v=8RasCV_Lggg).

  Q# namespace `Std.Math` includes a useful function `ArcTan2()`.
:::

> Q# function `ComplexAsComplexPolar` from `Std.Math` namespace converts a complex number of type `Complex` to a complex number of type `ComplexPolar`. For educational purposes, try to do this task by hand.
