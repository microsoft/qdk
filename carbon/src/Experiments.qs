export C12_Mark2_1Q_Teleport;

import Std.Arrays.Mapped;
import Std.Convert.ResultArrayAsInt;
import Std.Diagnostics.Fact;
import Utils.TransversalCNOT;
import C12;

// @EntryPoint()
// operation RunComputedExperiment() : (Int, (Int, Int)[], Int) {
//     let (preselect, ecs, final) = C12_Mark2_1Q_Teleport(1, PauliZ);
//     (ResultArrayAsInt(preselect), Mapped(m -> {let (a, b) = m; (ResultArrayAsInt(a), ResultArrayAsInt(b))}, ecs), ResultArrayAsInt(final))
// }

@EntryPoint()
operation RunTeleportExperiment() : (Result[], (Result[], Result[])[], Result[]) {
    C12_Mark2_1Q_Teleport(1, PauliZ)
}

operation C12_Mark2_1Q_Teleport(ec_repetitions : Int, basis : Pauli) : (Result[], (Result[], Result[])[], Result[]) {
    Fact(basis == PauliZ or basis == PauliX or basis == PauliI, "only PauliZ and PauliX supported");

    use logical_block = Qubit[12];
    use ancillas = Qubit[16];

    // Prepare in the requested basis
    mutable preselect = if basis == PauliX {
        C12.PrepareXX(logical_block, ancillas[...3])
    } else {
        C12.PrepareZZ(logical_block, ancillas[...3])
    };

    mutable syndromes = [];

    for _ in 1..(ec_repetitions) {
        // Sequential teleport on..
        // Prepare Z, Teleport X
        set preselect += C12.PrepareZZ(ancillas[...11], ancillas[12...]);
        TransversalCNOT(logical_block, ancillas[...11]);
        ApplyToEach(H, logical_block);
        let syndrome_x = MResetEachZ(logical_block);

        // Prepare X, Teleport Z
        set preselect += C12.PrepareXX(logical_block, ancillas[12...]);
        TransversalCNOT(logical_block, ancillas[...11]);
        let syndrome_z = MResetEachZ(ancillas[...11]);
        set syndromes += [(syndrome_x, syndrome_z)];
    }

    // Final measurement
    if basis == PauliX {
        ApplyToEach(H, logical_block);
    }
    let final = MResetEachZ(logical_block);

    (preselect, syndromes, final)
}