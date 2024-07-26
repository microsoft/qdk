namespace Kata {
    open Microsoft.Quantum.Math;

    function ComplexPolarToComplex(x : ComplexPolar) : Complex {
        let (r, theta) = (x.Magnitude, x.Argument);
        return Complex(r * Cos(theta), r * Sin(theta));
    }
}
