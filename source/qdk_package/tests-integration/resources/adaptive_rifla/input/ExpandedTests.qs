// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

namespace Test {
    import Std.Intrinsic.*;
    import Std.Convert.*;
    import Std.Math.*;
    import Std.Arrays.*;
    import Std.Measurement.*;
    import Std.Canon.*;

    @EntryPoint()
    operation Main() : (Result[], Result) {
        return (SearchForMarkedInput(), VerifyCNOTfromExp());
    }

    operation VerifyCNOTfromExp() : Result {
        use (control, target, paired) = (Qubit(), Qubit(), Qubit());

        within {
            H(paired);
            CNOT(paired, target);
            CNOT(paired, control);
        } apply {
            // CNOT
            let theta = PI() / 4.0;
            Rx(-2.0 * theta, target);
            Rz(-2.0 * theta, control);
            Adjoint Exp([PauliZ, PauliX], theta, [control, target]);

            Adjoint CNOT(control, target);
        }

        return M(target);
    }

    /// # Summary
    /// This operation applies Grover's algorithm to search all possible inputs
    /// to an operation to find a particular marked state.
    operation SearchForMarkedInput() : Result[] {
        let nQubits = 2;
        use qubits = Qubit[nQubits] {
            // Initialize a uniform superposition over all possible inputs.
            PrepareUniform(qubits);
            // The search itself consists of repeatedly reflecting about the
            // marked state and our start state, which we can write out in Q#
            // as a for loop.
            for idxIteration in 0..NIterations(nQubits) - 1 {
                ReflectAboutMarked(qubits);
                ReflectAboutUniform(qubits);
            }
            // Measure and return the answer.
            return MResetEachZ(qubits);
        }
    }

    /// # Summary
    /// Returns the number of Grover iterations needed to find a single marked
    /// item, given the number of qubits in a register.
    function NIterations(nQubits : Int) : Int {
        let nItems = 1 <<< nQubits; // 2^numQubits
        // compute number of iterations:
        let angle = ArcSin(1. / Sqrt(IntAsDouble(nItems)));
        let nIterations = Round(0.25 * PI() / angle - 0.5);
        return nIterations;
    }

    /// # Summary
    /// Reflects about the basis state marked by alternating zeros and ones.
    /// This operation defines what input we are trying to find in the main
    /// search.
    operation ReflectAboutMarked(inputQubits : Qubit[]) : Unit {
        use outputQubit = Qubit() {
            within {
                // We initialize the outputQubit to (|0⟩ - |1⟩) / √2,
                // so that toggling it results in a (-1) phase.
                X(outputQubit);
                H(outputQubit);
                // Flip the outputQubit for marked states.
                // Here, we get the state with alternating 0s and 1s by using
                // the X instruction on every other qubit.
                ApplyToEachA(X, inputQubits[...2...]);
            } apply {
                Controlled X(inputQubits, outputQubit);
            }
        }
    }

    /// # Summary
    /// Reflects about the uniform superposition state.
    operation ReflectAboutUniform(inputQubits : Qubit[]) : Unit {
        within {
            // Transform the uniform superposition to all-zero.
            Adjoint PrepareUniform(inputQubits);
            // Transform the all-zero state to all-ones
            PrepareAllOnes(inputQubits);
        } apply {
            // Now that we've transformed the uniform superposition to the
            // all-ones state, reflect about the all-ones state, then let
            // the within/apply block transform us back.
            ReflectAboutAllOnes(inputQubits);
        }
    }

    /// # Summary
    /// Reflects about the all-ones state.
    operation ReflectAboutAllOnes(inputQubits : Qubit[]) : Unit {
        Controlled Z(Most(inputQubits), Tail(inputQubits));
    }

    /// # Summary
    /// Given a register in the all-zeros state, prepares a uniform
    /// superposition over all basis states.
    operation PrepareUniform(inputQubits : Qubit[]) : Unit is Adj + Ctl {
        ApplyToEachCA(H, inputQubits);
    }

    /// # Summary
    /// Given a register in the all-zeros state, prepares an all-ones state
    /// by flipping every qubit.
    operation PrepareAllOnes(inputQubits : Qubit[]) : Unit is Adj + Ctl {
        ApplyToEachCA(X, inputQubits);
    }
}
