**Inputs:** 

1. An angle $\theta \in [0.01 \pi; 0.99 \pi]$.
2. An operation that implements a single-qubit unitary transformation:
either the [$R_z(\theta)$ gate](https://learn.microsoft.com/qsharp/api/qsharp-lang/microsoft.quantum.intrinsic/rz) or the [$R_y(\theta)$ gate](https://learn.microsoft.com/qsharp/api/qsharp-lang/microsoft.quantum.intrinsic/ry). 

> As a reminder,
> 
> $$R_z(\theta) = \begin{bmatrix} e^{-i\theta/2} & 0 \\ 0 & e^{i\theta/2} \end{bmatrix}$$
> $$R_y(\theta) = \begin{bmatrix} \cos\frac{\theta}{2} & -\sin\frac{\theta}{2} \\ \sin\frac{\theta}{2} & \cos\frac{\theta}{2} \end{bmatrix}$$

**Output:**  0 if the given operation is the $R_z$ gate, 1 if the given operation is the $R_y$ gate.

The operation will have Adjoint and Controlled variants defined.
You are allowed to apply the given operation and its adjoint/controlled variants **any number of times**.