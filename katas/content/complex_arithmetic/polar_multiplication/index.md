**Inputs:**

1. A complex polar number $x = r_{1}e^{i\theta_1}$.
2. A complex polar number $y = r_{2}e^{i\theta_2}$.

**Goal:**
Return the product of $x$ and $y$ as a complex polar number $x \cdot y = r_{3}e^{i\theta_3}$.

* $r_3$ should be non-negative: $r_3 \geq 0$
* $\theta_3$ should be between $-\pi$ and $\pi$: $-\pi < \theta_3 \leq \pi$
* Try to avoid converting the numbers into their Cartesian form.

<details>
  <summary><b>Need a hint?</b></summary>
  
  Remember, a number written in polar form already involves multiplication. What is $r_1e^{i\theta_1} \cdot r_2e^{i\theta_2}$?

  Is the value of $\theta$ in the product incorrect? Remember you might have to check your boundaries and adjust it to be in the range requested.
</details>

> Q# function `TimesCP` from `Microsoft.Quantum.Math` namespace multiplies two complex numbers, but it doesn't normalize the argument of the resulting number. For educational purposes, try to do this task by hand.
