import Gadgets.*;

operation be_rcc_18Q_18L_all() : Result[] {
    use q = Qubit[18];

    // Layer 0 operations
    Be_Gate_7(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_15(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_0(q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_85(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_167(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_59(q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_CX(q[0], q[1], q[2], q[3], q[4], q[5], q[6], q[7], q[8], q[9], q[10], q[11]);

    // Layer 1 operations
    Be_Gate_46(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_10(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_192(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_107(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_CX(q[0], q[1], q[2], q[3], q[4], q[5], q[12], q[13], q[14], q[15], q[16], q[17]);

    // Layer 2 operations
    Be_Gate_47(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_16(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_50(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_146(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_CX(q[0], q[1], q[2], q[3], q[4], q[5], q[12], q[13], q[14], q[15], q[16], q[17]);

    // Layer 3 operations
    Be_Gate_37(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_29(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_0(q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_69(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_170(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_59(q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_CX(q[6], q[7], q[8], q[9], q[10], q[11], q[0], q[1], q[2], q[3], q[4], q[5]);

    // Layer 4 operations
    Be_Gate_28(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_14(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_107(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_158(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_CX(q[12], q[13], q[14], q[15], q[16], q[17], q[6], q[7], q[8], q[9], q[10], q[11]);

    // Layer 5 operations
    Be_Gate_7(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_4(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_0(q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_83(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_79(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_59(q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_CX(q[6], q[7], q[8], q[9], q[10], q[11], q[0], q[1], q[2], q[3], q[4], q[5]);

    // Layer 6 operations
    Be_Gate_40(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_19(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_86(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_197(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_CX(q[12], q[13], q[14], q[15], q[16], q[17], q[6], q[7], q[8], q[9], q[10], q[11]);

    // Layer 7 operations
    Be_Gate_5(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_41(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_75(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_147(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_CX(q[6], q[7], q[8], q[9], q[10], q[11], q[12], q[13], q[14], q[15], q[16], q[17]);

    // Layer 8 operations
    Be_Gate_13(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_16(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_182(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_138(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_CX(q[0], q[1], q[2], q[3], q[4], q[5], q[12], q[13], q[14], q[15], q[16], q[17]);

    // Layer 9 operations
    Be_Gate_CX(q[0], q[1], q[2], q[3], q[4], q[5], q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_182(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_138(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_13(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_16(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[6], q[7], q[8], q[9], q[10], q[11]);

    // Layer 10 operations
    Be_Gate_CX(q[6], q[7], q[8], q[9], q[10], q[11], q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_75(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_147(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_4(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_34(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[0], q[1], q[2], q[3], q[4], q[5]);

    // Layer 11 operations
    Be_Gate_CX(q[12], q[13], q[14], q[15], q[16], q[17], q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_86(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_197(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_40(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_3(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[0], q[1], q[2], q[3], q[4], q[5]);

    // Layer 12 operations
    Be_Gate_CX(q[6], q[7], q[8], q[9], q[10], q[11], q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_83(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_79(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_59(q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_18(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_5(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_0(q[12], q[13], q[14], q[15], q[16], q[17]);

    // Layer 13 operations
    Be_Gate_CX(q[12], q[13], q[14], q[15], q[16], q[17], q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_107(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_158(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_28(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_15(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[0], q[1], q[2], q[3], q[4], q[5]);

    // Layer 14 operations
    Be_Gate_CX(q[6], q[7], q[8], q[9], q[10], q[11], q[0], q[1], q[2], q[3], q[4], q[5]);

    Be_Gate_69(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_170(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_59(q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_36(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_29(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_0(q[12], q[13], q[14], q[15], q[16], q[17]);

    // Layer 15 operations
    Be_Gate_CX(q[0], q[1], q[2], q[3], q[4], q[5], q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_50(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_146(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_47(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_16(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[6], q[7], q[8], q[9], q[10], q[11]);

    // Layer 16 operations
    Be_Gate_CX(q[0], q[1], q[2], q[3], q[4], q[5], q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_192(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_107(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_59(q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_43(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_10(q[12], q[13], q[14], q[15], q[16], q[17]);
    Be_Gate_0(q[6], q[7], q[8], q[9], q[10], q[11]);

    // Layer 17 operations
    Be_Gate_CX(q[0], q[1], q[2], q[3], q[4], q[5], q[6], q[7], q[8], q[9], q[10], q[11]);

    Be_Gate_85(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_167(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_59(q[12], q[13], q[14], q[15], q[16], q[17]);

    Be_Gate_18(q[0], q[1], q[2], q[3], q[4], q[5]);
    Be_Gate_14(q[6], q[7], q[8], q[9], q[10], q[11]);
    Be_Gate_0(q[12], q[13], q[14], q[15], q[16], q[17]);

    // Measure all qubits
    MeasureEachZ(q)
}
