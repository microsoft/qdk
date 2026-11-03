! Copyright (c) Microsoft Corporation.
! Licensed under the MIT License.

module qdk_circuit_bindings
    use, intrinsic :: iso_c_binding
    use, intrinsic :: iso_fortran_env, only: error_unit
    implicit none

    type, bind(C) :: qdk_bytes
        type(c_ptr) :: data = c_null_ptr
        integer(c_size_t) :: len = 0_c_size_t
    end type

    interface
        function circuit_new(qubits, clbits, out, error) bind(C, name="f_circuit_new") result(status)
            import :: c_int, c_ptr
            integer(c_int), value :: qubits, clbits
            type(c_ptr) :: out, error
            integer(c_int) :: status
        end function

        function circuit_clone(circuit, out, error) bind(C, name="f_circuit_clone") result(status)
            import :: c_int, c_ptr
            type(c_ptr), value :: circuit
            type(c_ptr) :: out, error
            integer(c_int) :: status
        end function

        function circuit_free(circuit, error) bind(C, name="f_circuit_free") result(status)
            import :: c_int, c_ptr
            type(c_ptr) :: circuit, error
            integer(c_int) :: status
        end function

        function append_h(circuit, qubit, error) bind(C, name="f_append_h") result(status)
            import :: c_int, c_ptr
            type(c_ptr), value :: circuit
            integer(c_int), value :: qubit
            type(c_ptr) :: error
            integer(c_int) :: status
        end function

        function measure(circuit, qubit, clbit, error) bind(C, name="f_measure") result(status)
            import :: c_int, c_ptr
            type(c_ptr), value :: circuit
            integer(c_int), value :: qubit, clbit
            type(c_ptr) :: error
            integer(c_int) :: status
        end function

        function clbit_expression(circuit, clbit, out, error) bind(C, name="f_clbit") result(status)
            import :: c_int, c_ptr
            type(c_ptr), value :: circuit
            integer(c_int), value :: clbit
            type(c_ptr) :: out, error
            integer(c_int) :: status
        end function

        function expression_free(expression, error) bind(C, name="f_expression_free") result(status)
            import :: c_int, c_ptr
            type(c_ptr) :: expression, error
            integer(c_int) :: status
        end function

        function begin_if(circuit, condition, out, error) bind(C, name="f_begin_if") result(status)
            import :: c_int, c_ptr
            type(c_ptr), value :: circuit, condition
            type(c_ptr) :: out, error
            integer(c_int) :: status
        end function

        function body_x(body, qubit, error) bind(C, name="f_body_x") result(status)
            import :: c_int, c_ptr
            type(c_ptr), value :: body
            integer(c_int), value :: qubit
            type(c_ptr) :: error
            integer(c_int) :: status
        end function

        function builder_finish(body, error) bind(C, name="f_builder_finish") result(status)
            import :: c_int, c_ptr
            type(c_ptr), value :: body
            type(c_ptr) :: error
            integer(c_int) :: status
        end function

        function builder_cancel(body, error) bind(C, name="f_builder_cancel") result(status)
            import :: c_int, c_ptr
            type(c_ptr), value :: body
            type(c_ptr) :: error
            integer(c_int) :: status
        end function

        function builder_free(body, error) bind(C, name="f_builder_free") result(status)
            import :: c_int, c_ptr
            type(c_ptr) :: body, error
            integer(c_int) :: status
        end function

        function qir_base(circuit, out, error) bind(C, name="f_qir_base") result(status)
            import :: c_int, c_ptr, qdk_bytes
            type(c_ptr), value :: circuit
            type(qdk_bytes) :: out
            type(c_ptr) :: error
            integer(c_int) :: status
        end function

        function qir_adaptive_ri(circuit, out, error) bind(C, name="f_qir_adaptive_ri") result(status)
            import :: c_int, c_ptr, qdk_bytes
            type(c_ptr), value :: circuit
            type(qdk_bytes) :: out
            type(c_ptr) :: error
            integer(c_int) :: status
        end function

        function qir_adaptive(circuit, out, error) bind(C, name="f_qir_adaptive") result(status)
            import :: c_int, c_ptr, qdk_bytes
            type(c_ptr), value :: circuit
            type(qdk_bytes) :: out
            type(c_ptr) :: error
            integer(c_int) :: status
        end function

        function diagnostic_message(error, out) bind(C, name="f_diagnostic_message") result(status)
            import :: c_int, c_ptr, qdk_bytes
            type(c_ptr), value :: error
            type(qdk_bytes) :: out
            integer(c_int) :: status
        end function

        subroutine diagnostic_free(error) bind(C, name="qdk_diagnostic_free")
            import :: c_ptr
            type(c_ptr) :: error
        end subroutine

        subroutine bytes_free(bytes) bind(C, name="qdk_bytes_free")
            import :: qdk_bytes
            type(qdk_bytes) :: bytes
        end subroutine

        function status_in_use() bind(C, name="f_status_in_use") result(status)
            import :: c_int
            integer(c_int) :: status
        end function

        function status_invalid_argument() bind(C, name="f_status_invalid_argument") result(status)
            import :: c_int
            integer(c_int) :: status
        end function
    end interface

contains

    subroutine require(condition, message)
        logical, intent(in) :: condition
        character(*), intent(in) :: message

        if (.not. condition) then
            write(error_unit, '(a)') message
            error stop 1
        end if
    end subroutine

    function take_bytes(bytes) result(text)
        type(qdk_bytes), intent(inout) :: bytes
        character(kind=c_char, len=:), allocatable :: text
        character(kind=c_char), pointer :: view(:)
        integer :: i, length

        call require(bytes%len >= 0_c_size_t .and. bytes%len <= int(huge(0), c_size_t), &
            "output length exceeds this example's Fortran array bounds")
        length = int(bytes%len)
        allocate(character(kind=c_char, len=length) :: text)
        if (length > 0) then
            call require(c_associated(bytes%data), "nonempty output has a null pointer")
            call c_f_pointer(bytes%data, view, [length])
            do i = 1, length
                text(i:i) = view(i)
            end do
            nullify(view)
        end if
        call bytes_free(bytes)
    end function

    function take_diagnostic(error) result(message)
        type(c_ptr), intent(inout) :: error
        character(kind=c_char, len=:), allocatable :: message
        type(qdk_bytes) :: bytes
        integer(c_int) :: status

        call require(c_associated(error), "native failure returned no diagnostic")
        status = diagnostic_message(error, bytes)
        call diagnostic_free(error)
        if (status /= 0_c_int) then
            call bytes_free(bytes)
            write(error_unit, '(a,i0)') "cannot read native diagnostic; status ", status
            error stop 1
        end if
        message = take_bytes(bytes)
    end function

    subroutine check(status, error)
        integer(c_int), intent(in) :: status
        type(c_ptr), intent(inout) :: error
        character(kind=c_char, len=:), allocatable :: message

        if (status /= 0_c_int) then
            write(error_unit, '(a,i0)') "native failure; status ", status
            message = take_diagnostic(error)
            write(error_unit, '(a)') message
            error stop 1
        end if
        if (c_associated(error)) then
            message = take_diagnostic(error)
            write(error_unit, '(a)') "success returned a diagnostic: " // message
            error stop 1
        end if
    end subroutine
end module
