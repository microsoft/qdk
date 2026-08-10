// # Sample
// Circuit Integration
//
// # Description
// This sample demonstrates the ability to use circuit files in Q# projects.
// It shows how to import and use custom quantum circuits defined in their own files.
// The circuit file, JointMeasurement.qsc, contains a joint measurement circuit for three qubits.
// This circuit file can be opened in VS Code and edited with a visual editor.
// Here, we import a circuit for performing a joint measurement of three
// qubits, with one auxiliary qubit. The results of the measurements should always
// contain 1 or 3 `Zero` results.

import JointMeasurement.JointMeasurement;

// Sample program using custom gates from a hardware provider.
operation Main() : Result[] {
    use qs = Qubit[4];
    ApplyToEach(H, qs[0..2]);
    let results = JointMeasurement(qs);
    ResetAll(qs);
    results
}
