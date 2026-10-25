! Copyright (c) Microsoft Corporation.
! Licensed under the MIT License.

program shor
    use, intrinsic :: iso_c_binding, only: c_char, c_null_char, c_size_t, c_double
    use, intrinsic :: iso_fortran_env, only: int64, error_unit
    implicit none
    include 'qep_interop.fi'

    integer, parameter :: max_bits = 15, max_attempts = 100
    integer(c_size_t), parameter :: no_control = -1_c_size_t

    type :: circuit_layout
        integer :: bits
        integer(c_size_t) :: phase, enable, carry, compare, scratch
    end type circuit_layout

    character(len=128) :: argument
    integer :: status
    integer(int64) :: number, factor

    if (command_argument_count() > 1) call fail('usage: shor_fortran [number | --self-test]')
    call init(c_char_"sparse_sim" // c_null_char)
    number = 187_int64
    if (command_argument_count() == 1) then
        call get_command_argument(1, argument, status=status)
        if (status /= 0) call fail('could not read the command-line argument')
        if (trim(argument) == '--self-test') then
            call self_test()
            stop
        end if
        if (len_trim(argument) == 0 .or. verify(trim(argument), '0123456789') /= 0) then
            call fail('number must be an integer')
        end if
        read(argument, *, iostat=status) number
        if (status /= 0) call fail('number must be an integer')
    end if
    if (number < 4_int64 .or. number >= shiftl(1_int64, max_bits)) call fail('number must be in [4, 32767]')
    call random_seed()
    factor = factor_semiprime(number)
    print '(A, I0, A, I0, A, I0)', 'Found factorization ', number, ' = ', factor, ' * ', number / factor

contains

    ! Stops on an invalid input or a failed simulator invariant.
    subroutine fail(message)
        character(len=*), intent(in) :: message
        write(error_unit, '(A)') 'shor: ' // message
        error stop 1
    end subroutine fail

    ! Computes the nonnegative greatest common divisor.
    function gcd(first, second) result(value)
        integer(int64), intent(in) :: first, second
        integer(int64) :: value, b, remainder
        value = abs(first)
        b = second
        do while (b /= 0_int64)
            remainder = mod(value, b)
            value = b
            b = remainder
        end do
    end function gcd

    ! Computes a modular power without constructing the full integer power.
    function power_mod(base, exponent, modulus) result(value)
        integer(int64), intent(in) :: base, exponent, modulus
        integer(int64) :: value, multiplier, remaining
        value = 1_int64
        multiplier = base
        remaining = exponent
        do while (remaining /= 0_int64)
            if (btest(remaining, 0)) value = mod(value * multiplier, modulus)
            multiplier = mod(multiplier * multiplier, modulus)
            remaining = shiftr(remaining, 1)
        end do
    end function power_mod

    ! Finds the inverse of a coprime constant using extended Euclid.
    function inverse_mod(value, modulus) result(inverse)
        integer(int64), intent(in) :: value, modulus
        integer(int64) :: inverse, a, b, u, v, quotient, next
        a = modulus
        b = value
        u = 0_int64
        v = 1_int64
        do while (b /= 0_int64)
            quotient = a / b
            next = a - quotient * b
            a = b
            b = next
            next = u - quotient * v
            u = v
            v = next
        end do
        if (a /= 1_int64) call fail('modular multiplier must be coprime to the modulus')
        inverse = modulo(u, modulus)
    end function inverse_mod

    ! Assigns disjoint eigenstate, work, control, carry, and clean scratch registers.
    function make_circuit(bits) result(circuit)
        integer, intent(in) :: bits
        type(circuit_layout) :: circuit
        circuit%bits = bits
        circuit%phase = int(2 * bits, c_size_t)
        circuit%enable = int(2 * bits + 1, c_size_t)
        circuit%carry = int(2 * bits + 2, c_size_t)
        circuit%compare = int(2 * bits + 3, c_size_t)
        circuit%scratch = int(2 * bits + 4, c_size_t)
    end function make_circuit

    ! Decomposes a multi-controlled X into a clean, uncomputed Toffoli ladder.
    subroutine controlled_x(circuit, controls, count, target)
        type(circuit_layout), intent(in) :: circuit
        integer(c_size_t), intent(in) :: controls(0:), target
        integer, intent(in) :: count
        integer :: i
        select case (count)
        case (0)
            call x(target)
        case (1)
            call cx(controls(0), target)
        case (2)
            call ccx(controls(0), controls(1), target)
        case default
            call ccx(controls(0), controls(1), circuit%scratch)
            do i = 2, count - 2
                call ccx(circuit%scratch + int(i - 2, c_size_t), controls(i), circuit%scratch + int(i - 1, c_size_t))
            end do
            call ccx(circuit%scratch + int(count - 3, c_size_t), controls(count - 1), target)
            do i = count - 2, 2, -1
                call ccx(circuit%scratch + int(i - 2, c_size_t), controls(i), circuit%scratch + int(i - 1, c_size_t))
            end do
            call ccx(controls(0), controls(1), circuit%scratch)
        end select
    end subroutine controlled_x

    ! Adds a signed constant modulo 2^width using controlled binary increments.
    subroutine add_constant(circuit, constant, targets, width, control)
        type(circuit_layout), intent(in) :: circuit
        integer(int64), intent(in) :: constant
        integer(c_size_t), intent(in) :: targets(0:), control
        integer, intent(in) :: width
        integer(c_size_t) :: controls(0:max_bits)
        integer(int64) :: encoded
        integer :: bit, target, count, i
        encoded = modulo(constant, shiftl(1_int64, width))
        do bit = 0, width - 1
            if (.not. btest(encoded, bit)) cycle
            do target = width - 1, bit, -1
                count = 0
                if (control /= no_control) then
                    controls(count) = control
                    count = count + 1
                end if
                do i = bit, target - 1
                    controls(count) = targets(i)
                    count = count + 1
                end do
                call controlled_x(circuit, controls, count, targets(target))
            end do
        end do
    end subroutine add_constant

    ! Implements the sample's controlled modular addition, restoring both borrow ancillas.
    subroutine modular_add(circuit, modulus, constant)
        type(circuit_layout), intent(in) :: circuit
        integer(int64), intent(in) :: modulus, constant
        integer(c_size_t) :: work(0:max_bits)
        integer :: i
        if (constant == 0_int64) return
        do i = 0, circuit%bits - 1
            work(i) = int(circuit%bits + i, c_size_t)
        end do
        work(circuit%bits) = circuit%carry
        call add_constant(circuit, constant, work, circuit%bits + 1, circuit%enable)
        call add_constant(circuit, -modulus, work, circuit%bits + 1, circuit%enable)
        call add_constant(circuit, modulus, work, circuit%bits, circuit%carry)

        ! Toggle the carry iff the reduced work register is at least the added constant.
        work(circuit%bits) = circuit%compare
        call add_constant(circuit, -constant, work, circuit%bits + 1, no_control)
        call x(circuit%compare)
        call ccx(circuit%enable, circuit%compare, circuit%carry)
        call x(circuit%compare)
        call add_constant(circuit, constant, work, circuit%bits + 1, no_control)
    end subroutine modular_add

    ! Multiplies the eigenstate register modulo N, controlled by the phase qubit.
    subroutine modular_multiply(circuit, modulus, constant)
        type(circuit_layout), intent(in) :: circuit
        integer(int64), intent(in) :: modulus, constant
        integer(int64) :: shifted, inverse
        integer(c_size_t) :: work
        integer :: i
        do i = 0, circuit%bits - 1
            shifted = mod(shiftl(constant, i), modulus)
            call ccx(circuit%phase, int(i, c_size_t), circuit%enable)
            call modular_add(circuit, modulus, shifted)
            call ccx(circuit%phase, int(i, c_size_t), circuit%enable)
        end do
        do i = 0, circuit%bits - 1
            work = int(circuit%bits + i, c_size_t)
            call cx(work, int(i, c_size_t))
            call ccx(circuit%phase, int(i, c_size_t), work)
            call cx(work, int(i, c_size_t))
        end do
        inverse = inverse_mod(constant, modulus)
        do i = 0, circuit%bits - 1
            shifted = mod(shiftl(inverse, i), modulus)
            call ccx(circuit%phase, int(i, c_size_t), circuit%enable)
            call modular_add(circuit, modulus, modulo(modulus - shifted, modulus))
            call ccx(circuit%phase, int(i, c_size_t), circuit%enable)
        end do
    end subroutine modular_multiply

    ! Measures and resets a qubit, rejecting backend error outcomes.
    function measure_reset(qubit) result(value)
        integer(c_size_t), intent(in) :: qubit
        integer(c_size_t) :: outcome
        integer(int64) :: value
        outcome = mz_reset(qubit)
        if (outcome < 0_c_size_t .or. outcome > 1_c_size_t) call fail('backend returned a measurement error')
        value = int(outcome, int64)
    end function measure_reset

    ! Measures a little-endian register and restores it to zero.
    function read_register(start, bits) result(value)
        integer(c_size_t), intent(in) :: start
        integer, intent(in) :: bits
        integer(int64) :: value, bit
        integer :: i
        value = 0_int64
        do i = 0, bits - 1
            bit = measure_reset(start + int(i, c_size_t))
            value = ior(value, shiftl(bit, i))
        end do
    end function read_register

    ! Runs the sample's iterative phase estimation with semiclassical Fourier feedback.
    function estimate_frequency(generator, modulus, bits) result(frequency)
        integer(int64), intent(in) :: generator, modulus
        integer, intent(in) :: bits
        integer(int64) :: frequency, constant, outcome
        integer :: precision, index, step
        type(circuit_layout) :: circuit
        circuit = make_circuit(bits)
        precision = 2 * bits + 1
        frequency = 0_int64
        call x(0_c_size_t)
        do index = precision - 1, 0, -1
            step = precision - 1 - index
            call h(circuit%phase)
            constant = power_mod(generator, shiftl(1_int64, index), modulus)
            call modular_multiply(circuit, modulus, constant)
            ! R1Frac and Rz differ only by an irrelevant global phase here.
            call rz(circuit%phase, acos(-1.0_c_double) * real(frequency, c_double) / real(shiftl(1_int64, step), c_double))
            call h(circuit%phase)
            outcome = measure_reset(circuit%phase)
            if (outcome == 1_int64) frequency = ibset(frequency, step)
        end do
        do index = 0, bits - 1
            call reset(int(index, c_size_t))
        end do
        print '(A, I0, A, I0)', 'Estimated frequency=', frequency, '/', shiftl(1_int64, precision)
    end function estimate_frequency

    ! Recovers the bounded continued-fraction denominator used by the Q# sample.
    function period_from_frequency(modulus, frequency, precision) result(period)
        integer(int64), intent(in) :: modulus, frequency
        integer, intent(in) :: precision
        integer(int64) :: period, r1, r2, s1, s2, quotient, next
        period = 1_int64
        if (frequency == 0_int64) return
        r1 = frequency
        r2 = shiftl(1_int64, precision)
        s1 = 1_int64
        s2 = 0_int64
        do while (r2 /= 0_int64 .and. abs(s2) <= modulus)
            quotient = r1 / r2
            next = r1 - quotient * r2
            r1 = r2
            r2 = next
            next = s1 - quotient * s2
            s1 = s2
            s2 = next
        end do
        if (r2 == 0_int64 .and. abs(s2) <= modulus) then
            period = abs(s2)
        else
            period = abs(s1)
        end if
    end function period_from_frequency

    ! Attempts the sample's even-period reduction to a nontrivial factor.
    function factor_from_period(number, generator, period) result(factor)
        integer(int64), intent(in) :: number, generator, period
        integer(int64) :: factor, half, a, b
        factor = 1_int64
        if (mod(period, 2_int64) /= 0_int64) return
        half = power_mod(generator, period / 2_int64, number)
        if (half == number - 1_int64) return
        a = gcd(half - 1_int64, number)
        b = gcd(half + 1_int64, number)
        factor = max(a, b)
        if (factor <= 1_int64 .or. factor >= number) factor = 1_int64
    end function factor_from_period

    ! Factors an input with quantum order finding and at most 100 generator attempts.
    function factor_semiprime(number) result(factor)
        integer(int64), intent(in) :: number
        integer(int64) :: factor, value, generator, frequency, period
        integer :: bits, attempt
        real(c_double) :: random_value
        factor = 2_int64
        if (mod(number, 2_int64) == 0_int64) return
        bits = 0
        value = number
        do while (value /= 0_int64)
            bits = bits + 1
            value = shiftr(value, 1)
        end do
        do attempt = 1, max_attempts
            ! Start with a coprime base for odd inputs so the example exercises quantum order finding.
            generator = 2_int64
            if (attempt /= 1) then
                call random_number(random_value)
                generator = 1_int64 + int(random_value * real(number - 1_int64, c_double), int64)
            end if
            print '(A, I0, A, I0, A, I0)', 'Factorizing ', number, ', attempt ', attempt, ', generator=', generator
            factor = gcd(generator, number)
            if (factor == 1_int64) then
                frequency = estimate_frequency(generator, number, bits)
                period = period_from_frequency(number, frequency, 2 * bits + 1)
                print '(A, I0)', 'Estimated period=', period
                factor = factor_from_period(number, generator, period)
            else
                print '(A, I0)', 'Guessed a divisor by accident: ', factor
            end if
            if (factor > 1_int64 .and. factor < number) return
        end do
        call fail('failed to find factors after 100 attempts')
    end function factor_semiprime

    ! Verifies all arithmetic work qubits are restored rather than hiding garbage by resetting it.
    subroutine check_clean(circuit)
        type(circuit_layout), intent(in) :: circuit
        integer :: qubit
        do qubit = 0, 3 * circuit%bits + 3
            if (measure_reset(int(qubit, c_size_t)) /= 0_int64) call fail('arithmetic left a dirty ancilla')
        end do
    end subroutine check_clean

    ! Exhaustively tests controlled modular arithmetic on small basis-state inputs.
    subroutine self_test()
        integer(int64) :: modulus, constant, value, expected
        integer :: bits, i, enabled
        type(circuit_layout) :: circuit
        do modulus = 3_int64, 7_int64
            bits = 3
            if (modulus < 4_int64) bits = 2
            circuit = make_circuit(bits)
            do constant = 0_int64, modulus - 1_int64
                do value = 0_int64, modulus - 1_int64
                    do enabled = 0, 1
                        do i = 0, bits - 1
                            if (btest(value, i)) call x(int(bits + i, c_size_t))
                        end do
                        if (enabled /= 0) call x(circuit%enable)
                        call modular_add(circuit, modulus, constant)
                        expected = value
                        if (enabled /= 0) expected = mod(value + constant, modulus)
                        if (read_register(int(bits, c_size_t), bits) /= expected) call fail('modular addition failed')
                        if (measure_reset(circuit%enable) /= int(enabled, int64)) call fail('addition changed its control')
                        call check_clean(circuit)

                        if (gcd(constant, modulus) /= 1_int64) cycle
                        do i = 0, bits - 1
                            if (btest(value, i)) call x(int(i, c_size_t))
                        end do
                        if (enabled /= 0) call x(circuit%phase)
                        call modular_multiply(circuit, modulus, constant)
                        expected = value
                        if (enabled /= 0) expected = mod(value * constant, modulus)
                        if (read_register(0_c_size_t, bits) /= expected) call fail('modular multiplication failed')
                        if (measure_reset(circuit%phase) /= int(enabled, int64)) then
                            call fail('multiplication changed its control')
                        end if
                        call check_clean(circuit)
                    end do
                end do
            end do
            call x(int(bits, c_size_t))
            call h(circuit%enable)
            call modular_add(circuit, modulus, modulus - 1_int64)
            call modular_add(circuit, modulus, 1_int64)
            call h(circuit%enable)
            if (read_register(int(bits, c_size_t), bits) /= 1_int64) call fail('coherent addition did not invert')
            call check_clean(circuit)

            call x(0_c_size_t)
            call h(circuit%phase)
            call modular_multiply(circuit, modulus, modulus - 1_int64)
            call modular_multiply(circuit, modulus, modulus - 1_int64)
            call h(circuit%phase)
            if (read_register(0_c_size_t, bits) /= 1_int64) call fail('coherent multiplication did not invert')
            call check_clean(circuit)
        end do
        if (period_from_frequency(15_int64, 32_int64, 7) /= 4_int64) call fail('continued fractions failed')
        if (factor_from_period(15_int64, 2_int64, 4_int64) /= 5_int64) call fail('period-to-factor reduction failed')
        print '(A)', 'Arithmetic self-test passed.'
    end subroutine self_test
end program shor
