program ghz
    use, intrinsic :: iso_c_binding, only: c_char, c_null_char, c_size_t
    implicit none
    include 'qep_interop.fi'

    integer :: shot
    integer(c_size_t) :: outcome

    call qdk_init(c_char_"sparse_sim" // c_null_char)
    do shot = 1, 5
        outcome = ghz_example(10_c_size_t)
        print '(A, I0)', 'result = ', outcome
    end do

    call qdk_init(c_char_"temporal_profiler" // c_null_char)
    outcome = ghz_example(10_c_size_t)
    call qdk_save_profile(c_char_"source/qep/profile_viewer/profiles/ghz_profile_fortran.json" // c_null_char)

contains

    ! Measures a little-endian integer register and resets its qubits.
    function read_le_int_register(start_qubit_id, length) result(value)
        integer(c_size_t), intent(in) :: start_qubit_id, length
        integer(c_size_t) :: value, i, bit

        value = 0_c_size_t
        do i = 0_c_size_t, length - 1_c_size_t
            bit = qdk_mz_reset(start_qubit_id + i)
            if (bit > 1_c_size_t) error stop 'Backend returned a measurement error'
            value = ior(value, shiftl(bit, i))
        end do
    end function read_le_int_register

    ! Prepares an n-qubit GHZ state, measures it as an integer, and resets all qubits.
    function ghz_example(n) result(value)
        integer(c_size_t), intent(in) :: n
        integer(c_size_t) :: value, i

        call qdk_h(0_c_size_t)
        do i = 1_c_size_t, n - 1_c_size_t
            call qdk_cx(0_c_size_t, i)
        end do
        value = read_le_int_register(0_c_size_t, n)
    end function ghz_example
end program ghz
