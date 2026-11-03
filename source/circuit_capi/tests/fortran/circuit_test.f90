! Copyright (c) Microsoft Corporation.
! Licensed under the MIT License.

program circuit_tests
    use qdk_circuit_bindings
    implicit none

    call require(command_argument_count() == 2, "expected Adaptive and Adaptive_RI snapshot paths")
    call test_clone_outlives_original()
    call test_child_prevents_parent_release()
    call test_negative_dimensions_return_diagnostic()
    call test_feedback_rejects_base()
    call test_adaptive_ri_snapshot()
    call test_adaptive_snapshot()
    print '(a)', "6 Fortran circuit tests passed"

contains

    function feedback_circuit() result(circuit)
        type(c_ptr) :: circuit
        type(c_ptr) :: error, condition, body
        integer(c_int) :: status

        circuit = c_null_ptr
        error = c_null_ptr
        condition = c_null_ptr
        body = c_null_ptr
        status = circuit_new(2_c_int, 2_c_int, circuit, error)
        call check(status, error)
        status = append_h(circuit, 0_c_int, error)
        call check(status, error)
        status = measure(circuit, 0_c_int, 0_c_int, error)
        call check(status, error)
        status = clbit_expression(circuit, 0_c_int, condition, error)
        call check(status, error)
        status = begin_if(circuit, condition, body, error)
        call check(status, error)
        status = body_x(body, 1_c_int, error)
        call check(status, error)
        status = builder_finish(body, error)
        call check(status, error)
        status = builder_free(body, error)
        call check(status, error)
        status = expression_free(condition, error)
        call check(status, error)
        status = measure(circuit, 1_c_int, 1_c_int, error)
        call check(status, error)
    end function

    subroutine test_clone_outlives_original()
        type(c_ptr) :: circuit, copy, error
        type(qdk_bytes) :: bytes
        character(kind=c_char, len=:), allocatable :: original, cloned
        integer(c_int) :: status

        circuit = feedback_circuit()
        copy = c_null_ptr
        error = c_null_ptr
        status = qir_adaptive(circuit, bytes, error)
        call check(status, error)
        original = take_bytes(bytes)
        status = circuit_clone(circuit, copy, error)
        call check(status, error)
        status = circuit_free(circuit, error)
        call check(status, error)
        call require(.not. c_associated(circuit), "circuit release must clear its slot")
        status = qir_adaptive(copy, bytes, error)
        call check(status, error)
        cloned = take_bytes(bytes)
        call require(len(original) == len(cloned), "clone changed QIR length")
        call require(original == cloned, "clone changed QIR after original release")
        status = circuit_free(copy, error)
        call check(status, error)
        status = circuit_free(copy, error)
        call check(status, error)
        print '(a)', "PASS clone outlives original"
    end subroutine

    subroutine test_child_prevents_parent_release()
        type(c_ptr) :: circuit, saved, condition, body, error
        character(kind=c_char, len=:), allocatable :: message
        integer(c_int) :: status

        circuit = c_null_ptr
        condition = c_null_ptr
        body = c_null_ptr
        error = c_null_ptr
        status = circuit_new(1_c_int, 1_c_int, circuit, error)
        call check(status, error)
        status = clbit_expression(circuit, 0_c_int, condition, error)
        call check(status, error)
        status = begin_if(circuit, condition, body, error)
        call check(status, error)
        status = builder_cancel(body, error)
        call check(status, error)
        saved = circuit
        status = circuit_free(circuit, error)
        call require(status == status_in_use(), "live children must refuse parent release")
        message = take_diagnostic(error)
        call require(len(message) > 0, "InUse diagnostic must not be empty")
        call require(c_associated(circuit, saved), "refusal must retain the owning pointer")
        status = builder_free(body, error)
        call check(status, error)
        status = expression_free(condition, error)
        call check(status, error)
        status = circuit_free(circuit, error)
        call check(status, error)
        call require(.not. c_associated(circuit), "child-first release must clear the circuit")
        print '(a)', "PASS child prevents parent release"
    end subroutine

    subroutine test_negative_dimensions_return_diagnostic()
        type(c_ptr) :: circuit, error
        character(kind=c_char, len=:), allocatable :: message
        integer(c_int) :: status

        circuit = c_null_ptr
        error = c_null_ptr
        status = circuit_new(-1_c_int, 0_c_int, circuit, error)
        call require(status == status_invalid_argument(), "negative dimension must fail")
        call require(.not. c_associated(circuit), "invalid construction published a circuit")
        message = take_diagnostic(error)
        call require(index(message, "nonnegative") > 0, "missing signed-input diagnostic")
        call require(.not. c_associated(error), "diagnostic release must clear its slot")
        print '(a)', "PASS negative dimensions return diagnostic"
    end subroutine

    subroutine test_feedback_rejects_base()
        type(c_ptr) :: circuit, error
        type(qdk_bytes) :: bytes
        character(kind=c_char, len=:), allocatable :: message, empty
        integer(c_int) :: status

        circuit = feedback_circuit()
        error = c_null_ptr
        status = qir_base(circuit, bytes, error)
        call require(status == status_invalid_argument(), "Base must refuse feedback")
        message = take_diagnostic(error)
        call require(index(message, "Adaptive") > 0, "missing Adaptive capability diagnostic")
        call require(bytes%len == 0_c_size_t .and. .not. c_associated(bytes%data), &
            "failed compilation must leave empty output")
        empty = take_bytes(bytes)
        call require(len(empty) == 0, "empty output must copy without mapping a null pointer")
        status = circuit_free(circuit, error)
        call check(status, error)
        print '(a)', "PASS feedback rejects Base"
    end subroutine

    subroutine test_adaptive_ri_snapshot()
        type(c_ptr) :: circuit, error
        type(qdk_bytes) :: bytes
        character(kind=c_char, len=:), allocatable :: actual
        integer(c_int) :: status

        circuit = feedback_circuit()
        error = c_null_ptr
        status = qir_adaptive_ri(circuit, bytes, error)
        call check(status, error)
        actual = take_bytes(bytes)
        status = circuit_free(circuit, error)
        call check(status, error)
        call assert_snapshot(actual, 2)
        print '(a)', "PASS Adaptive_RI snapshot"
    end subroutine

    subroutine test_adaptive_snapshot()
        type(c_ptr) :: circuit, error
        type(qdk_bytes) :: bytes
        character(kind=c_char, len=:), allocatable :: actual
        integer(c_int) :: status

        circuit = feedback_circuit()
        error = c_null_ptr
        status = qir_adaptive(circuit, bytes, error)
        call check(status, error)
        actual = take_bytes(bytes)
        status = circuit_free(circuit, error)
        call check(status, error)
        call assert_snapshot(actual, 1)
        print '(a)', "PASS Adaptive snapshot"
    end subroutine

    subroutine assert_snapshot(actual, argument)
        character(kind=c_char, len=*), intent(in) :: actual
        integer, intent(in) :: argument
        character(:), allocatable :: filename
        character(kind=c_char, len=:), allocatable :: expected
        character(512) :: message
        integer :: length, status, unit, file_size, i

        call get_command_argument(argument, length=length, status=status)
        call require(status == 0 .and. length > 0, "cannot read snapshot path")
        allocate(character(length) :: filename)
        call get_command_argument(argument, value=filename, status=status)
        call require(status == 0, "snapshot path was truncated")
        message = ""
        open(newunit=unit, file=filename, status="old", action="read", access="stream", &
            form="unformatted", iostat=status, iomsg=message)
        call require(status == 0, "cannot open snapshot " // filename // ": " // trim(message))
        inquire(unit=unit, size=file_size, iostat=status, iomsg=message)
        call require(status == 0, "cannot determine snapshot size: " // trim(message))
        call require(file_size >= 0, "snapshot size is not representable")
        allocate(character(kind=c_char, len=file_size) :: expected)
        read(unit, iostat=status, iomsg=message) expected
        call require(status == 0, "cannot read snapshot: " // trim(message))
        close(unit, iostat=status, iomsg=message)
        call require(status == 0, "cannot close snapshot: " // trim(message))
        if (len(actual) /= len(expected)) then
            write(error_unit, '(a,i0,a,i0)') "snapshot byte lengths: expected ", len(expected), ", actual ", len(actual)
            call require(.false., "QIR snapshot length mismatch: " // filename)
        end if
        do i = 1, len(actual)
            if (actual(i:i) /= expected(i:i)) then
                write(error_unit, '(a,i0)') "first differing QIR byte: ", i
                call require(.false., "QIR snapshot mismatch: " // filename)
            end if
        end do
    end subroutine
end program
