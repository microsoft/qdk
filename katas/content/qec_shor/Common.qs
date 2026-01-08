namespace Kata.Verification {
    import Std.Diagnostics.*;
    import Std.Math.*;
    import Std.Random.*;

    operation CheckErrorDetection(
        n : Int,
        encode : (Qubit[] => Unit is Adj),
        error : (Qubit => Unit is Adj),
        detect : (Qubit[] => Int)
    ) : Bool {
        for err_ind in -1..n - 1 {
            for _ in 1..10 {
                use qs = Qubit[n];
                let theta = DrawRandomDouble(0.0, 1.0);
                let correct = within {
                    // Prepare logical state on first qubit
                    Ry(2.0 * theta * PI(), qs[0]);
                    // Encode the state in multiple qubits
                    encode(qs);
                    // Introduce X error
                    if err_ind > -1 {
                        error(qs[err_ind]);
                    }
                } apply {
                    // Call solution to detect index
                    let detected = detect(qs);
                    // Check that it is correct
                    if detected != err_ind {
                        Message("Incorrect.");
                        let actual = err_ind == -1 ? "No error happened" | $"Error happened on qubit {err_ind}";
                        Message($"{actual}, but solution returned {detected}");
                        ResetAll(qs);
                        false
                    } else {
                        true
                    }
                };
                if not correct {
                    return false;
                }
                // Check that the state was not modified by the solution
                if not CheckAllZero(qs) {
                    Message("Incorrect.");
                    Message("The state of the qubits changed after the solution was applied");
                    ResetAll(qs);
                    return false;
                }
            }
        }

        Message("Correct!");
        true
    }
}
