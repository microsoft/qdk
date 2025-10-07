// # Sample
// Custom Operations
//
// # Description
// The @Measurement attribute in Q# allows you to define custom measurements
// that are lowered to QIR in the same way the `M` measurement in the standard
// library is lowered. That means an `"irreversible"` attribute is added to
// the callable declaration and the output results are moved to the parameters
// and treated as result registers.
//
// # Who is this for?
// The target audience are library authors targeting specific hardware.

/// Try running the command `Q#: Get QIR for the current Q# program`
/// in VS-Code's Command Palette.
operation Main() : Result {
    use q = Qubit();
    H(q);
    __quantum__qis__mx__body(q)
}

@Measurement()
@SimulatableIntrinsic()
operation __quantum__qis__mx__body(q : Qubit) : Result {
    H(q);
    M(q)
}
