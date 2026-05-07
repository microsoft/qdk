namespace Test {

    import Std.Intrinsic.*;
    import Std.Math.*;
    import Std.Measurement.*;

    // Demonstrates nested branching.
    // Expected output: (([1, 1, 0], 6), ([1, 1, 1, 0], true))
    @EntryPoint()
    operation Main() : ((Result[], Int), (Result[], Bool)) {
        // Nested branching using bool literals.
        use registerA = Qubit[3];
        if true {
            X(registerA[0]);
            if true {
                X(registerA[1]);
                if false {
                    X(registerA[2]);
                }
            }
        }
        let registerAMeasurements = MeasureEachZ(registerA);

        // Nested branching using measurement results to control the values of integers with no top-level branching
        // coming from elif instructions.
        mutable a = 0;
        if registerAMeasurements[0] == Zero {
            if registerAMeasurements[1] == Zero and registerAMeasurements[2] == Zero {
                set a = 0;
            } elif registerAMeasurements[1] == Zero and registerAMeasurements[2] == One {
                set a = 1;
            } elif registerAMeasurements[1] == One and registerAMeasurements[2] == Zero {
                set a = 2;
            } else {
                set a = 3;
            }
        } else {
            if registerAMeasurements[1] == Zero and registerAMeasurements[2] == Zero {
                set a = 4;
            } elif registerAMeasurements[1] == Zero and registerAMeasurements[2] == One {
                set a = 5;
            } elif registerAMeasurements[1] == One and registerAMeasurements[2] == Zero {
                set a = 6;
            } else {
                set a = 7;
            }
        }
        ResetAll(registerA);

        // Triple-nested branches with multiple quantum instructions inside using measurement results for conditions.
        use registerB = Qubit[4];
        use target = Qubit();
        X(target);
        SetIntToQubitRegister(7, registerB);
        let registerBMeasurements = MeasureEachZ(registerB);
        if registerBMeasurements[0] == Zero {
            if registerBMeasurements[1] == Zero {
                if registerBMeasurements[2] == Zero {
                    I(target);
                    I(target);
                } else {
                    X(target);
                    X(target);
                }
            } else {
                if registerBMeasurements[2] == Zero {
                    Y(target);
                    Y(target);
                } else {
                    Z(target);
                    Z(target);
                }
            }
        } elif registerBMeasurements[0] == Zero and registerBMeasurements[1] == One {
            if registerBMeasurements[1] == Zero {
                if registerBMeasurements[2] == Zero {
                    I(target);
                    I(target);
                } else {
                    X(target);
                    X(target);
                }
            } else {
                if registerBMeasurements[2] == Zero {
                    Y(target);
                    Y(target);
                } else {
                    Z(target);
                    Z(target);
                }
            }
        } elif registerBMeasurements[0] == One and registerBMeasurements[1] == Zero {
            if registerBMeasurements[1] == Zero {
                if registerBMeasurements[2] == Zero {
                    I(target);
                    I(target);
                } else {
                    X(target);
                    X(target);
                }
            } else {
                if registerBMeasurements[2] == Zero {
                    Y(target);
                    Y(target);
                } else {
                    Z(target);
                    Z(target);
                }
            }
        } else {
            if registerBMeasurements[1] == Zero {
                if registerBMeasurements[2] == Zero {
                    I(target);
                    I(target);
                } else {
                    X(target);
                    X(target);
                }
            } else {
                if registerBMeasurements[2] == Zero {
                    Y(target);
                    Y(target);
                } else {
                    Z(target);
                    Z(target);
                }
            }
        }
        ResetAll(registerB);

        return ((registerAMeasurements, a), (registerBMeasurements, MResetZ(target) == One));
    }

    operation SetIntToQubitRegister(integer : Int, register : Qubit[]) : Unit {
        mutable bits = integer;
        for q in register {
            if (bits &&& 1) == 1 {
                X(q);
            }
            set bits = bits >>> 1;
        }
    }
}
