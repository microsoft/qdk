export PrepareZZ, PrepareXX;

import Utils.TransversalCNOT, Utils.PreparePlus, Utils.PrepareZero;
import C4;

operation PrepareZZ(block : Qubit[], ancillas : Qubit[]) : Result[] {
    PreparePlus(ancillas[0]);
    C4.PrepareXX(block[...3], ancillas[0]);
    PrepareZero(ancillas[1]);
    C4.PrepareZZ(block[4..7], ancillas[1]);
    let z_res = MResetZ(ancillas[1]);
    TransversalCNOT(block[...3], block[4..7]);

    C4.PrepareZZ(block[8..11], ancillas[2]);
    TransversalCNOT(block[4..7], block[8..11]);

    // SwapLabels(block[9], block[10]);
    // SwapLabels(block[10], block[11]);
    Relabel(block[9..11], [block[10], block[11], block[9]]);

    // SwapLabels(block[6], block[7]);
    // SwapLabels(block[5], block[6]);
    Relabel(block[5..7], [block[7], block[5], block[6]]);

    let check1_res = C4.DetectOn(block[4..7], ancillas[0], ancillas[1]);
    PreparePlus(ancillas[3]);
    let check2_res = C4.DetectZ(block[8..11], ancillas[3], ancillas[2]);

    [z_res] + check1_res + check2_res
}

operation PrepareXX(block : Qubit[], ancillas : Qubit[]) : Result[] {
    let res = PrepareZZ(block, ancillas);
    ApplyToEach(H, block);
    
    // SwapLabels(block[1], block[2]);
    Relabel(block[1..2], [block[2], block[1]]);
    // SwapLabels(block[5], block[6]);
    Relabel(block[5..6], [block[6], block[5]]);
    // SwapLabels(block[9], block[10]);
    Relabel(block[9..10], [block[10], block[9]]);

    res
}
