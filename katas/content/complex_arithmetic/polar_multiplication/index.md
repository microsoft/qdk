**Inputs:**

1. A complex polar number $x = r_{1}e^{i\theta_1}$
 where $r_{1} \geq 0$ and $-\pi < \theta_1 \leq \pi$.
2. A complex polar number $y = r_{2}e^{i\theta_2}$
 where $r_{2} \geq 0$ and $-\pi < \theta_2 \leq \pi$.

**Goal:**
Return the product of $x$ and $y$ as a complex polar number $x \cdot y = r_{3}e^{i\theta_3}$.

* $r_3$ should be non-negative: $r_3 \geq 0$
* $\theta_3$ should be between $-\pi$ and $\pi$: $-\pi < \theta_3 \leq \pi$
* Try to avoid converting the numbers into their Cartesian form.

:::hint
Remember, a number written in polar form already involves multiplication. What is $r_1e^{i\theta_1} \cdot r_2e^{i\theta_2}$?

  Is the value of $\theta$ in the product incorrect? Remember you might have to check your boundaries and adjust it to be in the range requested.
:::

> Q# function `TimesCP` from `Std.Math` namespace multiplies two complex numbers, but it doesn't normalize the argument of the resulting number. For educational purposes, try to do this task by hand.
