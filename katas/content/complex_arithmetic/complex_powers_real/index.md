**Inputs:**

1. A non-negative real number $r$.
2. A complex number $x = a + bi$.

**Goal:** Return the complex number $r^x = r^{a + bi}$.

:::hint
You can use the fact that $r = e^{\ln r}$ to convert exponent bases. Remember though, $\ln r$ is only defined for positive numbers - make sure to check for $r = 0$ separately!

  Q# namespace `Std.Math` includes useful functions `Log()`, `Sin()`, and `Cos()`.
:::
