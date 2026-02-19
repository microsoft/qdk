# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from qsharp.qre import LOGICAL, PHYSICAL, Encoding, PropertyKey, instruction
from qsharp.qre.instruction_ids import (
    T,
    CCZ,
    CCX,
    CCY,
    CNOT,
    CZ,
    H,
    MEAS_Z,
    MEAS_X,
    MEAS_XX,
    MEAS_ZZ,
    PAULI_I,
    PREP_X,
    PREP_Z,
    LATTICE_SURGERY,
    MEMORY,
    SQRT_SQRT_X,
    SQRT_SQRT_X_DAG,
    SQRT_SQRT_Y,
    SQRT_SQRT_Y_DAG,
    SQRT_SQRT_Z,
    SQRT_SQRT_Z_DAG,
)
from qsharp.qre.models import (
    AQREGateBased,
    Majorana,
    RoundBasedFactory,
    MagicUpToClifford,
    Litinski19Factory,
    SurfaceCode,
    ThreeAux,
    YokedSurfaceCode,
)


# ---------------------------------------------------------------------------
# AQREGateBased architecture tests
# ---------------------------------------------------------------------------


class TestAQREGateBased:
    def test_default_error_rate(self):
        arch = AQREGateBased()
        assert arch.error_rate == 1e-4

    def test_custom_error_rate(self):
        arch = AQREGateBased(error_rate=1e-3)
        assert arch.error_rate == 1e-3

    def test_provided_isa_contains_expected_instructions(self):
        arch = AQREGateBased()
        isa = arch.provided_isa

        for instr_id in [PAULI_I, CNOT, CZ, H, MEAS_Z, T]:
            assert instr_id in isa

    def test_instruction_encodings_are_physical(self):
        arch = AQREGateBased()
        isa = arch.provided_isa

        for instr_id in [PAULI_I, CNOT, CZ, H, MEAS_Z, T]:
            assert isa[instr_id].encoding == PHYSICAL

    def test_instruction_error_rates_match(self):
        rate = 1e-3
        arch = AQREGateBased(error_rate=rate)
        isa = arch.provided_isa

        for instr_id in [PAULI_I, CNOT, CZ, H, MEAS_Z, T]:
            assert isa[instr_id].expect_error_rate() == rate

    def test_gate_times(self):
        arch = AQREGateBased()
        isa = arch.provided_isa

        # Single-qubit gates: 50ns
        for instr_id in [PAULI_I, H, T]:
            assert isa[instr_id].expect_time() == 50

        # Two-qubit gates: 50ns
        for instr_id in [CNOT, CZ]:
            assert isa[instr_id].expect_time() == 50

        # Measurement: 100ns
        assert isa[MEAS_Z].expect_time() == 100

    def test_arities(self):
        arch = AQREGateBased()
        isa = arch.provided_isa

        assert isa[PAULI_I].arity == 1
        assert isa[CNOT].arity == 2
        assert isa[CZ].arity == 2
        assert isa[H].arity == 1
        assert isa[MEAS_Z].arity == 1

    def test_context_creation(self):
        arch = AQREGateBased()
        ctx = arch.context()
        assert ctx is not None


# ---------------------------------------------------------------------------
# Majorana architecture tests
# ---------------------------------------------------------------------------


class TestMajorana:
    def test_default_error_rate(self):
        arch = Majorana()
        assert arch.error_rate == 1e-5

    def test_provided_isa_contains_expected_instructions(self):
        arch = Majorana()
        isa = arch.provided_isa

        for instr_id in [PREP_X, PREP_Z, MEAS_XX, MEAS_ZZ, MEAS_X, MEAS_Z, T]:
            assert instr_id in isa

    def test_all_times_are_1us(self):
        arch = Majorana()
        isa = arch.provided_isa

        for instr_id in [PREP_X, PREP_Z, MEAS_XX, MEAS_ZZ, MEAS_X, MEAS_Z, T]:
            assert isa[instr_id].expect_time() == 1000

    def test_clifford_error_rates_match_qubit_error(self):
        for rate in [1e-4, 1e-5, 1e-6]:
            arch = Majorana(error_rate=rate)
            isa = arch.provided_isa

            for instr_id in [PREP_X, PREP_Z, MEAS_XX, MEAS_ZZ, MEAS_X, MEAS_Z]:
                assert isa[instr_id].expect_error_rate() == rate

    def test_t_error_rate_mapping(self):
        """T error rate maps: 1e-4 -> 5%, 1e-5 -> 1.5%, 1e-6 -> 1%."""
        expected = {1e-4: 0.05, 1e-5: 0.015, 1e-6: 0.01}

        for qubit_rate, t_rate in expected.items():
            arch = Majorana(error_rate=qubit_rate)
            isa = arch.provided_isa
            assert isa[T].expect_error_rate() == t_rate

    def test_two_qubit_measurement_arities(self):
        arch = Majorana()
        isa = arch.provided_isa

        assert isa[MEAS_XX].arity == 2
        assert isa[MEAS_ZZ].arity == 2


# ---------------------------------------------------------------------------
# SurfaceCode QEC tests
# ---------------------------------------------------------------------------


class TestSurfaceCode:
    def test_required_isa(self):
        reqs = SurfaceCode.required_isa()
        assert reqs is not None

    def test_default_distance(self):
        sc = SurfaceCode(distance=3)
        assert sc.distance == 3

    def test_provides_lattice_surgery(self):
        arch = AQREGateBased()
        ctx = arch.context()
        sc = SurfaceCode(distance=3)

        isas = list(sc.provided_isa(arch.provided_isa, ctx))
        assert len(isas) == 1

        isa = isas[0]
        assert LATTICE_SURGERY in isa

        ls = isa[LATTICE_SURGERY]
        assert ls.encoding == LOGICAL

    def test_space_scales_with_distance(self):
        """Space = 2*d^2 - 1 physical qubits per logical qubit."""
        arch = AQREGateBased()

        for d in [3, 5, 7, 9]:
            ctx = arch.context()
            sc = SurfaceCode(distance=d)
            isas = list(sc.provided_isa(arch.provided_isa, ctx))
            ls = isas[0][LATTICE_SURGERY]
            expected_space = 2 * d**2 - 1
            assert ls.expect_space(1) == expected_space

    def test_time_scales_with_distance(self):
        """Time = (h_time + 4*cnot_time + meas_time) * d."""
        arch = AQREGateBased()
        # h=50, cnot=50, meas=100 for AQREGateBased
        syndrome_time = 50 + 4 * 50 + 100  # = 350

        for d in [3, 5, 7]:
            ctx = arch.context()
            sc = SurfaceCode(distance=d)
            isas = list(sc.provided_isa(arch.provided_isa, ctx))
            ls = isas[0][LATTICE_SURGERY]
            assert ls.expect_time(1) == syndrome_time * d

    def test_error_rate_decreases_with_distance(self):
        arch = AQREGateBased()

        errors = []
        for d in [3, 5, 7, 9, 11]:
            ctx = arch.context()
            sc = SurfaceCode(distance=d)
            isas = list(sc.provided_isa(arch.provided_isa, ctx))
            errors.append(isas[0][LATTICE_SURGERY].expect_error_rate(1))

        # Each successive distance should have a lower error rate
        for i in range(len(errors) - 1):
            assert errors[i] > errors[i + 1]

    def test_enumeration_via_query(self):
        """Enumerating SurfaceCode.q() should yield multiple distances."""
        arch = AQREGateBased()
        ctx = arch.context()

        count = 0
        for isa in SurfaceCode.q().enumerate(ctx):
            assert LATTICE_SURGERY in isa
            count += 1

        # domain is range(3, 26, 2) = 12 distances
        assert count == 12

    def test_custom_crossing_prefactor(self):
        arch = AQREGateBased()
        ctx = arch.context()

        sc_default = SurfaceCode(distance=5)
        sc_custom = SurfaceCode(crossing_prefactor=0.06, distance=5)

        default_error = list(sc_default.provided_isa(arch.provided_isa, ctx))[0][
            LATTICE_SURGERY
        ].expect_error_rate(1)

        ctx2 = arch.context()
        custom_error = list(sc_custom.provided_isa(arch.provided_isa, ctx2))[0][
            LATTICE_SURGERY
        ].expect_error_rate(1)

        # Doubling prefactor should double the error rate
        assert abs(custom_error - 2 * default_error) < 1e-20

    def test_custom_error_correction_threshold(self):
        arch = AQREGateBased()

        ctx1 = arch.context()
        sc_low_threshold = SurfaceCode(error_correction_threshold=0.005, distance=5)
        error_low = list(sc_low_threshold.provided_isa(arch.provided_isa, ctx1))[0][
            LATTICE_SURGERY
        ].expect_error_rate(1)

        ctx2 = arch.context()
        sc_high_threshold = SurfaceCode(error_correction_threshold=0.02, distance=5)
        error_high = list(sc_high_threshold.provided_isa(arch.provided_isa, ctx2))[0][
            LATTICE_SURGERY
        ].expect_error_rate(1)

        # Lower threshold means worse ratio => higher logical error
        assert error_low > error_high


# ---------------------------------------------------------------------------
# ThreeAux QEC tests
# ---------------------------------------------------------------------------


class TestThreeAux:
    def test_required_isa(self):
        reqs = ThreeAux.required_isa()
        assert reqs is not None

    def test_provides_lattice_surgery(self):
        arch = Majorana()
        ctx = arch.context()
        ta = ThreeAux(distance=3)

        isas = list(ta.provided_isa(arch.provided_isa, ctx))
        assert len(isas) == 1
        assert LATTICE_SURGERY in isas[0]

    def test_space_formula(self):
        """Space = 4*d^2 - 3 per logical qubit."""
        arch = Majorana()

        for d in [3, 5, 7]:
            ctx = arch.context()
            ta = ThreeAux(distance=d)
            isas = list(ta.provided_isa(arch.provided_isa, ctx))
            ls = isas[0][LATTICE_SURGERY]
            expected = 4 * d**2 - 3
            assert ls.expect_space(1) == expected

    def test_time_formula_double_rail(self):
        """Time = gate_time * (4*d + 4) for double-rail (default)."""
        arch = Majorana()

        for d in [3, 5, 7]:
            ctx = arch.context()
            ta = ThreeAux(distance=d, single_rail=False)
            isas = list(ta.provided_isa(arch.provided_isa, ctx))
            ls = isas[0][LATTICE_SURGERY]
            # MEAS_XX and MEAS_ZZ have time=1000 each; max is 1000
            expected_time = 1000 * (4 * d + 4)
            assert ls.expect_time(1) == expected_time

    def test_time_formula_single_rail(self):
        """Time = gate_time * (5*d + 4) for single-rail."""
        arch = Majorana()

        for d in [3, 5, 7]:
            ctx = arch.context()
            ta = ThreeAux(distance=d, single_rail=True)
            isas = list(ta.provided_isa(arch.provided_isa, ctx))
            ls = isas[0][LATTICE_SURGERY]
            expected_time = 1000 * (5 * d + 4)
            assert ls.expect_time(1) == expected_time

    def test_error_rate_decreases_with_distance(self):
        arch = Majorana()

        errors = []
        for d in [3, 5, 7, 9]:
            ctx = arch.context()
            ta = ThreeAux(distance=d)
            isas = list(ta.provided_isa(arch.provided_isa, ctx))
            errors.append(isas[0][LATTICE_SURGERY].expect_error_rate(1))

        for i in range(len(errors) - 1):
            assert errors[i] > errors[i + 1]

    def test_single_rail_has_different_error_threshold(self):
        """Single-rail has threshold 0.0051, double-rail 0.0066."""
        arch = Majorana()

        ctx1 = arch.context()
        double = ThreeAux(distance=5, single_rail=False)
        error_double = list(double.provided_isa(arch.provided_isa, ctx1))[0][
            LATTICE_SURGERY
        ].expect_error_rate(1)

        ctx2 = arch.context()
        single = ThreeAux(distance=5, single_rail=True)
        error_single = list(single.provided_isa(arch.provided_isa, ctx2))[0][
            LATTICE_SURGERY
        ].expect_error_rate(1)

        # Both should be positive but differ
        assert error_double > 0
        assert error_single > 0
        assert error_double != error_single

    def test_enumeration_via_query(self):
        arch = Majorana()
        ctx = arch.context()

        count = 0
        for isa in ThreeAux.q().enumerate(ctx):
            assert LATTICE_SURGERY in isa
            count += 1

        # domain: range(3, 26, 2) × {True, False} for single_rail
        # = 12 distances × 2 = 24
        assert count == 24


# ---------------------------------------------------------------------------
# YokedSurfaceCode tests
# ---------------------------------------------------------------------------


class TestYokedSurfaceCode:
    def _get_lattice_surgery_isa(self, distance=5):
        """Helper to get a lattice surgery ISA from SurfaceCode."""
        arch = AQREGateBased()
        ctx = arch.context()
        sc = SurfaceCode(distance=distance)
        isas = list(sc.provided_isa(arch.provided_isa, ctx))
        return isas[0], ctx

    def test_provides_memory_instruction(self):
        ls_isa, ctx = self._get_lattice_surgery_isa()
        ysc = YokedSurfaceCode()

        isas = list(ysc.provided_isa(ls_isa, ctx))
        assert len(isas) == 1
        assert MEMORY in isas[0]

    def test_memory_is_logical(self):
        ls_isa, ctx = self._get_lattice_surgery_isa()
        ysc = YokedSurfaceCode()

        isas = list(ysc.provided_isa(ls_isa, ctx))
        mem = isas[0][MEMORY]
        assert mem.encoding == LOGICAL

    def test_memory_arity_is_variable(self):
        ls_isa, ctx = self._get_lattice_surgery_isa()
        ysc = YokedSurfaceCode()

        isas = list(ysc.provided_isa(ls_isa, ctx))
        mem = isas[0][MEMORY]
        # arity=None means variable arity
        assert mem.arity is None

    def test_space_increases_with_arity(self):
        ls_isa, ctx = self._get_lattice_surgery_isa()
        ysc = YokedSurfaceCode()

        isas = list(ysc.provided_isa(ls_isa, ctx))
        mem = isas[0][MEMORY]

        spaces = [mem.expect_space(n) for n in [4, 16, 64]]
        for i in range(len(spaces) - 1):
            assert spaces[i] < spaces[i + 1]

    def test_time_increases_with_arity(self):
        ls_isa, ctx = self._get_lattice_surgery_isa()
        ysc = YokedSurfaceCode()

        isas = list(ysc.provided_isa(ls_isa, ctx))
        mem = isas[0][MEMORY]

        times = [mem.expect_time(n) for n in [4, 16, 64]]
        for i in range(len(times) - 1):
            assert times[i] < times[i + 1]

    def test_error_rate_increases_with_arity(self):
        ls_isa, ctx = self._get_lattice_surgery_isa()
        ysc = YokedSurfaceCode()

        isas = list(ysc.provided_isa(ls_isa, ctx))
        mem = isas[0][MEMORY]

        errors = [mem.expect_error_rate(n) for n in [4, 16, 64]]
        for i in range(len(errors) - 1):
            assert errors[i] < errors[i + 1]

    def test_distance_property_propagated(self):
        d = 7
        ls_isa, ctx = self._get_lattice_surgery_isa(distance=d)
        ysc = YokedSurfaceCode()

        isas = list(ysc.provided_isa(ls_isa, ctx))
        mem = isas[0][MEMORY]
        assert mem.get_property(PropertyKey.DISTANCE) == d


# ---------------------------------------------------------------------------
# Litinski19Factory tests
# ---------------------------------------------------------------------------


class TestLitinski19Factory:
    def test_required_isa(self):
        reqs = Litinski19Factory.required_isa()
        assert reqs is not None

    def test_table1_aqre_yields_t_and_ccz(self):
        """AQREGateBased (error 1e-4) matches Table 1 scenario: T & CCZ."""
        arch = AQREGateBased()
        ctx = arch.context()
        factory = Litinski19Factory()

        isas = list(factory.provided_isa(arch.provided_isa, ctx))

        # 6 T entries × 1 CCZ entry = 6 combinations
        assert len(isas) == 6

        for isa in isas:
            assert T in isa
            assert CCZ in isa
            assert len(isa) == 2

    def test_table1_instruction_properties(self):
        arch = AQREGateBased()
        ctx = arch.context()
        factory = Litinski19Factory()

        for isa in factory.provided_isa(arch.provided_isa, ctx):
            t_instr = isa[T]
            ccz_instr = isa[CCZ]

            assert t_instr.arity == 1
            assert t_instr.encoding == LOGICAL
            assert t_instr.expect_error_rate() > 0
            assert t_instr.expect_time() > 0
            assert t_instr.expect_space() > 0

            assert ccz_instr.arity == 3
            assert ccz_instr.encoding == LOGICAL
            assert ccz_instr.expect_error_rate() > 0

    def test_table1_t_error_rates_are_diverse(self):
        """T entries in Table 1 should span a range of error rates."""
        arch = AQREGateBased()
        ctx = arch.context()
        factory = Litinski19Factory()

        isas = list(factory.provided_isa(arch.provided_isa, ctx))
        t_errors = [isa[T].expect_error_rate() for isa in isas]

        # Should have multiple distinct T error rates
        unique_errors = set(t_errors)
        assert len(unique_errors) > 1

        # All error rates should be positive and very small
        for err in t_errors:
            assert 0 < err < 1e-5

    def test_table1_1e3_clifford_yields_6_isas(self):
        """AQREGateBased with 1e-3 error matches Table 1 at 1e-3 Clifford."""
        arch = AQREGateBased(error_rate=1e-3)
        ctx = arch.context()
        factory = Litinski19Factory()

        isas = list(factory.provided_isa(arch.provided_isa, ctx))

        # 6 T entries × 1 CCZ entry = 6 combinations
        assert len(isas) == 6

        for isa in isas:
            assert T in isa
            assert CCZ in isa

    def test_table2_scenario_no_ccz(self):
        """Table 2 scenario: T error ~10x higher than Clifford, no CCZ."""
        from qsharp.qre import ISA as ISAType

        arch = AQREGateBased()
        ctx = arch.context()

        # Manually create ISA with T error rate 10x Clifford
        isa_input = ISAType(
            instruction(
                CNOT, encoding=Encoding.PHYSICAL, arity=2, time=50, error_rate=1e-4
            ),
            instruction(
                H, encoding=Encoding.PHYSICAL, arity=1, time=50, error_rate=1e-4
            ),
            instruction(
                MEAS_Z, encoding=Encoding.PHYSICAL, arity=1, time=100, error_rate=1e-4
            ),
            instruction(T, encoding=Encoding.PHYSICAL, time=50, error_rate=1e-3),
        )

        factory = Litinski19Factory()
        isas = list(factory.provided_isa(isa_input, ctx))

        # Table 2 at 1e-4 Clifford: 4 T entries, no CCZ
        assert len(isas) == 4

        for isa in isas:
            assert T in isa
            assert CCZ not in isa

    def test_no_yield_when_error_too_high(self):
        """If T error > 10x Clifford, no entries match."""
        from qsharp.qre import ISA as ISAType

        arch = AQREGateBased()
        ctx = arch.context()

        isa_input = ISAType(
            instruction(
                CNOT, encoding=Encoding.PHYSICAL, arity=2, time=50, error_rate=1e-4
            ),
            instruction(
                H, encoding=Encoding.PHYSICAL, arity=1, time=50, error_rate=1e-4
            ),
            instruction(
                MEAS_Z, encoding=Encoding.PHYSICAL, arity=1, time=100, error_rate=1e-4
            ),
            instruction(T, encoding=Encoding.PHYSICAL, time=50, error_rate=0.05),
        )

        factory = Litinski19Factory()
        isas = list(factory.provided_isa(isa_input, ctx))
        assert len(isas) == 0

    def test_time_based_on_syndrome_extraction(self):
        """Time should be based on syndrome extraction time × cycles."""
        arch = AQREGateBased()
        ctx = arch.context()
        factory = Litinski19Factory()

        # For AQREGateBased: syndrome_extraction_time = 4*50 + 50 + 100 = 350
        syndrome_time = 4 * 50 + 50 + 100  # 350 ns

        isas = list(factory.provided_isa(arch.provided_isa, ctx))
        for isa in isas:
            t_time = isa[T].expect_time()
            assert t_time > 0
            # Time should be ceil(syndrome_time * cycles), so it must be at
            # least syndrome_time (cycles >= 1)
            assert t_time >= syndrome_time


# ---------------------------------------------------------------------------
# MagicUpToClifford tests
# ---------------------------------------------------------------------------


class TestMagicUpToClifford:
    def test_required_isa_is_empty(self):
        reqs = MagicUpToClifford.required_isa()
        assert reqs is not None

    def test_adds_clifford_equivalent_t_gates(self):
        """Given T gate, should add SQRT_SQRT_X/Y/Z and dagger variants."""
        arch = AQREGateBased()
        ctx = arch.context()
        factory = Litinski19Factory()
        modifier = MagicUpToClifford()

        for isa in factory.provided_isa(arch.provided_isa, ctx):
            modified_isas = list(modifier.provided_isa(isa, ctx))
            assert len(modified_isas) == 1
            modified_isa = modified_isas[0]

            # T family equivalents
            for equiv_id in [
                SQRT_SQRT_X,
                SQRT_SQRT_X_DAG,
                SQRT_SQRT_Y,
                SQRT_SQRT_Y_DAG,
                SQRT_SQRT_Z,
                SQRT_SQRT_Z_DAG,
            ]:
                assert equiv_id in modified_isa

            break  # Just test the first one

    def test_adds_clifford_equivalent_ccz(self):
        """Given CCZ, should add CCX and CCY."""
        arch = AQREGateBased()
        ctx = arch.context()
        factory = Litinski19Factory()
        modifier = MagicUpToClifford()

        for isa in factory.provided_isa(arch.provided_isa, ctx):
            modified_isas = list(modifier.provided_isa(isa, ctx))
            modified_isa = modified_isas[0]

            assert CCX in modified_isa
            assert CCY in modified_isa
            assert CCZ in modified_isa
            break

    def test_full_count_of_instructions(self):
        """T gate (1) + 5 equivalents (SQRT_SQRT_*) + CCZ (1) + 2 equivalents (CCX, CCY) = 9."""
        arch = AQREGateBased()
        ctx = arch.context()
        factory = Litinski19Factory()
        modifier = MagicUpToClifford()

        for isa in factory.provided_isa(arch.provided_isa, ctx):
            modified_isas = list(modifier.provided_isa(isa, ctx))
            assert len(modified_isas[0]) == 9
            break

    def test_equivalent_instructions_share_properties(self):
        """Clifford equivalents should have same time, space, error rate."""
        arch = AQREGateBased()
        ctx = arch.context()
        factory = Litinski19Factory()
        modifier = MagicUpToClifford()

        for isa in factory.provided_isa(arch.provided_isa, ctx):
            modified_isas = list(modifier.provided_isa(isa, ctx))
            modified_isa = modified_isas[0]

            t_instr = modified_isa[T]
            for equiv_id in [
                SQRT_SQRT_X,
                SQRT_SQRT_X_DAG,
                SQRT_SQRT_Y,
                SQRT_SQRT_Y_DAG,
                SQRT_SQRT_Z_DAG,
            ]:
                equiv = modified_isa[equiv_id]
                assert equiv.expect_error_rate() == t_instr.expect_error_rate()
                assert equiv.expect_time() == t_instr.expect_time()
                assert equiv.expect_space() == t_instr.expect_space()

            ccz_instr = modified_isa[CCZ]
            for equiv_id in [CCX, CCY]:
                equiv = modified_isa[equiv_id]
                assert equiv.expect_error_rate() == ccz_instr.expect_error_rate()

            break

    def test_modification_count_matches_factory_output(self):
        """MagicUpToClifford should produce one modified ISA per input ISA."""
        arch = AQREGateBased()
        ctx = arch.context()
        factory = Litinski19Factory()
        modifier = MagicUpToClifford()

        modified_count = 0
        for isa in factory.provided_isa(arch.provided_isa, ctx):
            for _ in modifier.provided_isa(isa, ctx):
                modified_count += 1

        assert modified_count == 6

    def test_no_family_present_passes_through(self):
        """If no family member is present, ISA passes through unchanged."""
        from qsharp.qre import ISA as ISAType

        arch = AQREGateBased()
        ctx = arch.context()
        modifier = MagicUpToClifford()

        # ISA with only a LATTICE_SURGERY instruction (no T or CCZ family)
        from qsharp.qre import linear_function

        ls = instruction(
            LATTICE_SURGERY,
            encoding=LOGICAL,
            arity=None,
            space=linear_function(17),
            time=1000,
            error_rate=linear_function(1e-10),
        )
        isa_input = ISAType(ls)

        results = list(modifier.provided_isa(isa_input, ctx))
        assert len(results) == 1
        # Should only contain the original instruction
        assert len(results[0]) == 1


# ---------------------------------------------------------------------------
# Litinski19Factory + MagicUpToClifford integration (from original test)
# ---------------------------------------------------------------------------


def test_isa_manipulation():
    arch = AQREGateBased()
    factory = Litinski19Factory()
    modifier = MagicUpToClifford()

    ctx = arch.context()

    # Table 1 scenario: should yield ISAs with both T and CCZ instructions
    isas = list(factory.provided_isa(arch.provided_isa, ctx))

    # 6 T entries × 1 CCZ entry = 6 combinations
    assert len(isas) == 6

    for isa in isas:
        # Each ISA should contain both T and CCZ instructions
        assert T in isa
        assert CCZ in isa
        assert len(isa) == 2

        t_instr = isa[T]
        ccz_instr = isa[CCZ]

        # Verify instruction properties
        assert t_instr.arity == 1
        assert t_instr.encoding == LOGICAL
        assert t_instr.expect_error_rate() > 0

        assert ccz_instr.arity == 3
        assert ccz_instr.encoding == LOGICAL
        assert ccz_instr.expect_error_rate() > 0

    # After MagicUpToClifford modifier
    modified_count = 0
    for isa in factory.provided_isa(arch.provided_isa, ctx):
        for modified_isa in modifier.provided_isa(isa, ctx):
            modified_count += 1
            # MagicUpToClifford should add derived instructions
            assert T in modified_isa
            assert CCZ in modified_isa
            assert CCX in modified_isa
            assert len(modified_isa) == 9

    assert modified_count == 6


# ---------------------------------------------------------------------------
# RoundBasedFactory tests
# ---------------------------------------------------------------------------


class TestRoundBasedFactory:
    def test_required_isa(self):
        reqs = RoundBasedFactory.required_isa()
        assert reqs is not None

    def test_produces_logical_t_gates(self):
        arch = AQREGateBased()

        for isa in RoundBasedFactory.q(use_cache=False).enumerate(arch.context()):
            t = isa[T]
            assert t.encoding == LOGICAL
            assert t.arity == 1
            assert t.expect_error_rate() > 0
            assert t.expect_time() > 0
            assert t.expect_space() > 0
            break  # Just check the first

    def test_error_rates_are_bounded(self):
        """Distilled T error rates should be bounded and mostly small."""
        arch = AQREGateBased()  # T error rate is 1e-4

        errors = []
        for isa in RoundBasedFactory.q(use_cache=False).enumerate(arch.context()):
            errors.append(isa[T].expect_error_rate())

        # All should be positive
        assert all(e > 0 for e in errors)
        # Most distilled error rates should be much lower than 1
        assert min(errors) < 1e-4
        # Median should be well below raw physical error
        sorted_errors = sorted(errors)
        median = sorted_errors[len(sorted_errors) // 2]
        assert median < 1e-3

    def test_max_produces_fewer_or_equal_results_than_sum(self):
        """Using max for physical_qubit_calculation may filter differently."""
        arch = AQREGateBased()

        sum_count = sum(
            1 for _ in RoundBasedFactory.q(use_cache=False).enumerate(arch.context())
        )
        max_count = sum(
            1
            for _ in RoundBasedFactory.q(
                use_cache=False, physical_qubit_calculation=max
            ).enumerate(arch.context())
        )

        assert max_count <= sum_count

    def test_max_space_less_than_or_equal_sum_space(self):
        """max-aggregated space should be <= sum-aggregated space for each."""
        arch = AQREGateBased()

        sum_spaces = sorted(
            isa[T].expect_space()
            for isa in RoundBasedFactory.q(use_cache=False).enumerate(arch.context())
        )

        max_spaces = sorted(
            isa[T].expect_space()
            for isa in RoundBasedFactory.q(
                use_cache=False, physical_qubit_calculation=max
            ).enumerate(arch.context())
        )

        # The minimum space with max should be <= minimum space with sum
        assert max_spaces[0] <= sum_spaces[0]

    def test_with_three_aux_code_query(self):
        """RoundBasedFactory with ThreeAux code query should produce results."""
        arch = Majorana()

        count = 0
        for isa in RoundBasedFactory.q(
            use_cache=False, code_query=ThreeAux.q()
        ).enumerate(arch.context()):
            assert T in isa
            assert isa[T].encoding == LOGICAL
            count += 1

        assert count > 0

    def test_round_based_aqre_sum(self):
        arch = AQREGateBased()

        total_space = 0
        total_time = 0
        total_error = 0.0
        count = 0

        for isa in RoundBasedFactory.q(use_cache=False).enumerate(arch.context()):
            count += 1
            total_space += isa[T].expect_space()
            total_time += isa[T].expect_time()
            total_error += isa[T].expect_error_rate()

        assert total_space == 12_946_488
        assert total_time == 12_032_250
        assert abs(total_error - 0.001_463_030_863_973_197_8) < 1e-8
        assert count == 107

    def test_round_based_aqre_max(self):
        arch = AQREGateBased()

        total_space = 0
        total_time = 0
        total_error = 0.0
        count = 0

        for isa in RoundBasedFactory.q(
            use_cache=False, physical_qubit_calculation=max
        ).enumerate(arch.context()):
            count += 1
            total_space += isa[T].expect_space()
            total_time += isa[T].expect_time()
            total_error += isa[T].expect_error_rate()

        assert total_space == 4_651_617
        assert total_time == 7_785_000
        assert abs(total_error - 0.001_463_030_863_973_197_8) < 1e-8
        assert count == 77

    def test_round_based_msft_sum(self):
        arch = Majorana()

        total_space = 0
        total_time = 0
        total_error = 0.0
        count = 0

        for isa in RoundBasedFactory.q(
            use_cache=False, code_query=ThreeAux.q()
        ).enumerate(arch.context()):
            count += 1
            total_space += isa[T].expect_space()
            total_time += isa[T].expect_time()
            total_error += isa[T].expect_error_rate()

        assert total_space == 255_952_723
        assert total_time == 478_235_000
        assert abs(total_error - 0.000_880_967_766_732_897_4) < 1e-8
        assert count == 301


# ---------------------------------------------------------------------------
# Cross-model integration tests
# ---------------------------------------------------------------------------


class TestCrossModelIntegration:
    def test_surface_code_feeds_into_litinski(self):
        """SurfaceCode -> Litinski19Factory pipeline works end to end."""
        arch = AQREGateBased()
        ctx = arch.context()

        # SurfaceCode takes AQRE physical ISA -> LATTICE_SURGERY
        sc = SurfaceCode(distance=5)
        sc_isas = list(sc.provided_isa(arch.provided_isa, ctx))
        assert len(sc_isas) == 1

        # Litinski takes H, CNOT, MEAS_Z, T from the physical ISA
        factory = Litinski19Factory()
        factory_isas = list(factory.provided_isa(arch.provided_isa, ctx))
        assert len(factory_isas) > 0

    def test_three_aux_feeds_into_round_based(self):
        """ThreeAux -> RoundBasedFactory pipeline works."""
        arch = Majorana()
        ctx = arch.context()

        count = 0
        for isa in RoundBasedFactory.q(
            use_cache=False, code_query=ThreeAux.q()
        ).enumerate(ctx):
            assert T in isa
            count += 1

        assert count > 0

    def test_litinski_with_magic_up_to_clifford_query(self):
        """Full query chain: Litinski19Factory -> MagicUpToClifford."""
        arch = AQREGateBased()
        ctx = arch.context()

        count = 0
        for isa in MagicUpToClifford.q(source=Litinski19Factory.q()).enumerate(ctx):
            assert T in isa
            assert CCX in isa
            assert CCY in isa
            assert CCZ in isa
            count += 1

        assert count == 6

    def test_surface_code_with_yoked_surface_code(self):
        """SurfaceCode -> YokedSurfaceCode pipeline provides MEMORY."""
        arch = AQREGateBased()
        ctx = arch.context()

        count = 0
        for isa in YokedSurfaceCode.q(source=SurfaceCode.q()).enumerate(ctx):
            assert MEMORY in isa
            count += 1

        # 12 distances × 2 shape heuristics = 24
        assert count == 24

    def test_majorana_three_aux_yoked(self):
        """Majorana -> ThreeAux -> YokedSurfaceCode pipeline."""
        arch = Majorana()
        ctx = arch.context()

        count = 0
        for isa in YokedSurfaceCode.q(source=ThreeAux.q()).enumerate(ctx):
            assert MEMORY in isa
            count += 1

        assert count > 0
