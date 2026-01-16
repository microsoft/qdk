/// # Sample
/// Estimating Frequency for Shor's algorithm
///
/// # Description
/// In this sample we concentrate on costing the `EstimateFrequency`
/// operation, which is the core quantum operation in Shor's algorithm, and
/// we omit the classical pre- and post-processing. This makes it ideal for
/// use with the Microsoft Quantum Resource Estimator.
import Std.Arrays.*;
import Std.Canon.*;
import Std.Convert.*;
import Std.Diagnostics.*;
import Std.Intrinsic.*;
import Std.Math.*;
import Std.Measurement.*;
import Std.Arithmetic.*;
import Std.ResourceEstimation.*;

operation Main() : Unit {
    let bitsize = 31;

    // When chooseing parameters for `EstimateFrequency`, make sure that
    // generator and modules are not co-prime
    let _ = EstimateFrequency(11, 2^bitsize - 1, bitsize);
}

/// # Summary
/// Estimates the frequency of a generator
/// in the residue ring Z mod `modulus`.
///
/// # Input
/// ## generator
/// The unsigned integer multiplicative order (period)
/// of which is being estimated. Must be co-prime to `modulus`.
/// ## modulus
/// The modulus which defines the residue ring Z mod `modulus`
/// in which the multiplicative order of `generator` is being estimated.
/// ## bitsize
/// Number of bits needed to represent the modulus.
///
/// # Output
/// The numerator k of dyadic fraction k/2^bitsPrecision
/// approximating s/r.
operation EstimateFrequency(
    generator : Int,
    modulus : Int,
    bitsize : Int
) : Int {
    mutable frequencyEstimate = 0;
    let bitsPrecision = 2 * bitsize + 1;

    // Allocate qubits for the superposition of eigenstates of
    // the oracle that is used in period finding.
    use eigenstateRegister = Qubit[bitsize];

    // Initialize eigenstateRegister to 1, which is a superposition of
    // the eigenstates we are estimating the phases of.
    // We first interpret the register as encoding an unsigned integer
    // in little endian encoding.
    ApplyXorInPlace(1, eigenstateRegister);
    let oracle = ApplyOrderFindingOracle(generator, modulus, _, _);

    // Use phase estimation with a semiclassical Fourier transform to
    // estimate the frequency.
    use c = Qubit();
    for idx in bitsPrecision - 1..-1..0 {
        within {
            H(c);
        } apply {
            // `BeginEstimateCaching` and `EndEstimateCaching` are the operations
            // exposed by Microsoft Quantum Resource Estimator. These will instruct
            // resource counting such that the if-block will be executed
            // only once, its resources will be cached, and appended in
            // every other iteration.
            if BeginEstimateCaching("ControlledOracle", SingleVariant()) {
                Controlled oracle([c], (1 <<< idx, eigenstateRegister));
                EndEstimateCaching();
            }
            R1Frac(frequencyEstimate, bitsPrecision - 1 - idx, c);
        }
        if MResetZ(c) == One {
            frequencyEstimate += 1 <<< (bitsPrecision - 1 - idx);
        }
    }

    // Return all the qubits used for oracle's eigenstate back to 0 state
    // using Std.Intrinsic.ResetAll.
    ResetAll(eigenstateRegister);

    return frequencyEstimate;
}

/// # Summary
/// Interprets `target` as encoding unsigned little-endian integer k
/// and performs transformation |k⟩ ↦ |gᵖ⋅k mod N ⟩ where
/// p is `power`, g is `generator` and N is `modulus`.
///
/// # Input
/// ## generator
/// The unsigned integer multiplicative order (period)
/// of which is being estimated. Must be co-prime to `modulus`.
/// ## modulus
/// The modulus which defines the residue ring Z mod `modulus`
/// in which the multiplicative order of `generator` is being estimated.
/// ## power
/// Power of `generator` by which `target` is multiplied.
/// ## target
/// Register interpreted as little-endian which is multiplied by
/// given power of the generator. The multiplication is performed modulo
/// `modulus`.
internal operation ApplyOrderFindingOracle(
    generator : Int,
    modulus : Int,
    power : Int,
    target : Qubit[]
) : Unit is Adj + Ctl {
    // The oracle we use for order finding implements |x⟩ ↦ |x⋅a mod N⟩. We
    // also use `ExpModI` to compute a by which x must be multiplied. Also
    // note that we interpret target as unsigned integer in little-endian
    // format.
    ModularMultiplyByConstant(
        modulus,
        ExpModI(generator, power, modulus),
        target
    );
}

/// # Summary
/// Performs modular in-place multiplication by a classical constant.
///
/// # Description
/// Given the classical constants `c` and `modulus`, and an input quantum
/// register |𝑦⟩ in little-endian format, this operation computes
/// `(c*x) % modulus` into |𝑦⟩.
///
/// # Input
/// ## modulus
/// Modulus to use for modular multiplication
/// ## c
/// Constant by which to multiply |𝑦⟩
/// ## y
/// Quantum register of target
internal operation ModularMultiplyByConstant(modulus : Int, c : Int, y : Qubit[]) : Unit is Adj + Ctl {
    use qs = Qubit[Length(y)];
    for idx in IndexRange(y) {
        let shiftedC = (c <<< idx) % modulus;
        Controlled ModularAddConstant(
            [y[idx]],
            (modulus, shiftedC, qs)
        );
    }
    for idx in IndexRange(y) {
        SWAP(y[idx], qs[idx]);
    }
    let invC = InverseModI(c, modulus);
    for idx in IndexRange(y) {
        let shiftedC = (invC <<< idx) % modulus;
        Controlled ModularAddConstant(
            [y[idx]],
            (modulus, modulus - shiftedC, qs)
        );
    }
}

/// # Summary
/// Performs modular in-place addition of a classical constant into a
/// quantum register.
///
/// Given the classical constants `c` and `modulus`, and an input quantum
/// register |𝑦⟩ in little-endian format, this operation computes
/// `(x+c) % modulus` into |𝑦⟩.
///
/// # Input
/// ## modulus
/// Modulus to use for modular addition
/// ## c
/// Constant to add to |𝑦⟩
/// ## y
/// Quantum register of target
internal operation ModularAddConstant(modulus : Int, c : Int, y : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Controlled ModularAddConstant([], (modulus, c, y));
    }
    controlled (ctrls, ...) {
        // We apply a custom strategy to control this operation instead of
        // letting the compiler create the controlled variant for us in
        // which the `Controlled` functor would be distributed over each
        // operation in the body.
        //
        // Here we can use some scratch memory to save ensure that at most
        // one control qubit is used for costly operations such as
        // `AddConstant` and `CompareGreaterThenOrEqualConstant`.
        if Length(ctrls) >= 2 {
            use control = Qubit();
            within {
                Controlled X(ctrls, control);
            } apply {
                Controlled ModularAddConstant([control], (modulus, c, y));
            }
        } else {
            use carry = Qubit();
            Controlled IncByI(ctrls, (c, y + [carry]));
            Controlled Adjoint IncByI(ctrls, (modulus, y + [carry]));
            Controlled IncByI([carry], (modulus, y));
            Controlled ApplyIfLessOrEqualL(ctrls, (X, IntAsBigInt(c), y, carry));
        }
    }
}
