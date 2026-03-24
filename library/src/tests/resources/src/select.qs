namespace Test {
    import Std.Arrays.*;
    import Std.Convert.*;
    import Std.Diagnostics.*;
    import Std.Random.*;
    import Std.TableLookup.*;

    internal operation TestSelect(addressBits : Int, dataBits : Int) : Unit {
        use addressRegister = Qubit[addressBits];
        use temporaryRegister = Qubit[dataBits];
        use dataRegister = Qubit[dataBits];

        let data = DrawMany(_ => DrawMany(_ => (DrawRandomInt(0, 1) == 1), dataBits, 0), 2^addressBits, 0);

        for (index, expected) in Enumerated(data) {
            ApplyXorInPlace(index, addressRegister);

            // a temporary register is not necessary normally, but we want to
            // test the optimized adjoint operation as well.
            within {
                Select(data, addressRegister, temporaryRegister);
            } apply {
                ApplyToEach(CNOT, Zipped(temporaryRegister, dataRegister));
            }

            Fact(Mapped(ResultAsBool, MResetEachZ(dataRegister)) == expected, $"Invalid data result for address {index}");
            Fact(MeasureInteger(addressRegister) == index, $"Invalid address result for address {index}");
        }
    }

    internal operation TestSelectPhase() : Unit {
        use addressRegister = Qubit[3];
        use targetRegister = Qubit[4];

        // Could be random, but fixed for reproducibility
        let data = [
            [false, false, false, false],
            [false, false, true, false],
            [true, true, false, false],
            [false, true, false, false],
            [true, true, true, true],
            [true, false, false, false],
            [true, true, true, false],
            [true, false, true, false],
        ];

        // Select followed by unselect. This should be equivalent to identity.
        let selunsel = (addr) => {
            within {
                Select(data, addr, targetRegister);
            } apply {
                // Do nothing.
            }
        };

        // This test checks that the implementation of unselect
        // doesn't change address register phases and returns target register to |0⟩ state.
        let equal = CheckOperationsAreEqual(3, selunsel, (addr) => {});
        Fact(CheckAllZero(targetRegister), "Target register must be in |0⟩ state after unlookup.");
        Fact(equal, "Select+Unselect should be equivalent to identity up to global phase.");
    }

    internal operation TestSelectLongerAddress() : Unit {
        use addressRegister = Qubit[5];
        use targetRegister = Qubit[4];

        // Could be random, but fixed for reproducibility
        let data = [
            [false, false, false, false],
            [false, false, true, false],
            [true, true, false, false],
            [false, true, false, false],
        ];

        // Select followed by unselect. This should be equivalent to identity.
        within {
            Select(data, addressRegister, targetRegister);
        } apply {
            // Do nothing.
        }

        Fact(CheckAllZero(targetRegister), "Target register must be in |0⟩ state after unlookup.");
        Fact(CheckAllZero(addressRegister), "Address register must be in |0⟩ state after unlookup.");
    }

    internal operation TestSelectFuzz(rounds : Int) : Unit {
        for _ in 1..rounds {
            let addressBits = DrawRandomInt(2, 6);
            let dataBits = 10;
            let numData = DrawRandomInt(2^(addressBits - 1) + 1, 2^addressBits - 1);

            let data = DrawMany(_ => DrawMany(_ => (DrawRandomInt(0, 1) == 1), dataBits, 0), numData, 0);

            use addressRegister = Qubit[addressBits];
            use temporaryRegister = Qubit[dataBits];
            use dataRegister = Qubit[dataBits];

            for _ in 1..5 {
                let index = DrawRandomInt(0, numData - 1);

                ApplyXorInPlace(index, addressRegister);

                // a temporary register is not necessary normally, but we want to
                // test the optimized adjoint operation as well.
                within {
                    Select(data, addressRegister, temporaryRegister);
                } apply {
                    ApplyToEach(CNOT, Zipped(temporaryRegister, dataRegister));
                }

                Fact(Mapped(ResultAsBool, MResetEachZ(dataRegister)) == data[index], $"Invalid data result for address {index} (addressBits = {addressBits}, dataBits = {dataBits})");
                Fact(MeasureInteger(addressRegister) == index, $"Invalid address result for address {index} (addressBits = {addressBits}, dataBits = {dataBits})");
            }
        }
    }
}
