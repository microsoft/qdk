When raising $i$ to an integer power, the answer will vary according to a certain pattern.
To figure it out, notice that raising $i$ to the power of $4$ gives: 

$$i^4 = i^2 \cdot i^2 = (-1) \cdot (-1) = 1$$

Thus, when the exponent $n$ is divisible by 4, $i^n$ will always be 1.

When the exponent $n$ is not divisible by 4, you can use the previous observation to see that $i^n = i^{n \mod 4}$.
For an even exponent $n$ that isn't divisible by 4 you'll have $i^n = i^2 = -1.$

Here is the complete pattern that arises when raising $i$ to non-negative powers. Note that it is periodic with period $4$.

<table>
  <tr>
    <th>Power of $i$</th>
    <th> $i^0$ </th>
    <th> $i^1$ </th>
    <th> $i^2$ </th>
    <th> $i^3$ </th>
    <th> $i^4$ </th>
    <th> $i^5$ </th>
    <th> $i^6$ </th>
    <th> $i^7$ </th>
    <th> $i^8$ </th>
    <th> $\dots$ </th>
  </tr>
  <tr>
    <td>Result</td>
    <td> $1$ </td>
    <td> $i$ </td>
    <td> $-1$ </td>
    <td> $-i$ </td>
    <td> $1$ </td>
    <td> $i$ </td>
    <td> $-1$ </td>
    <td> $-i$ </td>
    <td> $1$ </td>
    <td> $\dots$ </td>
  </tr>
</table>

> `%` is the Q# modulo operator which returns the remainder of a division. For example, `7%2` gives $1$, because $1$ is the remainder of dividing $7$ by $2$.  
> We can use this operator to determine if the exponent $n$ is divisible by $4$.

@[solution]({"id": "complex_arithmetic__powers_of_i_solution", "codePath": "Solution.qs"})
