**Inputs:**

1. A complex number $x = a + bi$.
2. A complex number $y = c + di$.

**Goal:**
Return the product of $x$ and $y$ as a complex number.

:::hint
Multiplying complex numbers is just like multiplying polynomials. Distribute one of the complex numbers: 
$$(a + bi)(c + di) = a(c + di) + bi(c + di)$$ 
Then multiply through, keeping in mind that $i^2=-1$, and group the real and imaginary terms together.

A video explanation of multiplying complex numbers can be found [here](https://www.youtube.com/watch?v=cWn6g8Qqvs4).
:::

> Q# function `TimesC` from `Std.Math` namespace multiplies two complex numbers. For educational purposes, try to do this task by hand.
