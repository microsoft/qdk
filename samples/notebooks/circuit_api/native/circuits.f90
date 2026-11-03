! Copyright (c) Microsoft Corporation.
! Licensed under the MIT License.

module sample_feedback
    use qdk_circuit
    implicit none
contains
    subroutine correction(body, error)
        type(body_t), intent(in) :: body
        type(error_t), intent(out) :: error
        call body%x(1, error)
    end subroutine
end module

program circuits
    use qdk_circuit
    use sample_feedback, only: correction
    use, intrinsic :: iso_fortran_env, only: error_unit
    implicit none
    type(circuit_t) :: prefix, base, adaptive
    type(error_t) :: error
    character(kind=qdk_char, len=:), allocatable :: data

    call prefix%init(2, 2, error)
    call check(error)
    call prefix%h(0, error)
    call check(error)
    call prefix%clone(base, error)
    call check(error)
    call prefix%clone(adaptive, error)
    call check(error)
    call prefix%close(error)
    call check(error)

    call base%cx(0, 1, error)
    call check(error)
    call base%measure(0, 0, error)
    call check(error)
    call base%measure(1, 1, error)
    call check(error)
    call save(base, "base", profile_base)
    call base%close(error)
    call check(error)

    call adaptive%measure(0, 0, error)
    call check(error)
    call adaptive%if_one(0, correction, error)
    call check(error)
    call adaptive%measure(1, 1, error)
    call check(error)

    call adaptive%qir(profile_base, data, error)
    if (.not. error%failed()) error stop "Base must reject feedback"
    if (index(error%message, "Adaptive") == 0) then
        write(error_unit, '(a)') error%message
        error stop "expected a profile diagnostic"
    end if
    print '(a)', "Expected Base refusal: "//error%message
    call save(adaptive, "adaptive", profile_adaptive)
    call adaptive%close(error)
    call check(error)

contains

    subroutine check(error)
        type(error_t), intent(in) :: error
        if (error%failed()) then
            write(error_unit, '(a)') error%message
            error stop 1
        end if
    end subroutine

    subroutine save(circuit, name, profile)
        type(circuit_t), intent(in) :: circuit
        character(*), intent(in) :: name
        integer, intent(in) :: profile
        type(error_t) :: error
        character(kind=qdk_char, len=:), allocatable :: bytes
        call circuit%marshal_binary(bytes, error)
        call check(error)
        call write_file(name//".qdk", bytes)
        call circuit%marshal_text(bytes, error)
        call check(error)
        call write_file(name//".sexpr", bytes)
        call circuit%qir(profile, bytes, error)
        call check(error)
        call write_file(name//".ll", bytes)
    end subroutine

    subroutine write_file(path, bytes)
        character(*), intent(in) :: path
        character(kind=qdk_char, len=*), intent(in) :: bytes
        integer :: unit, status
        character(512) :: message
        open(newunit=unit, file=path, access="stream", form="unformatted", &
            status="replace", action="write", iostat=status, iomsg=message)
        if (status /= 0) then
            write(error_unit, '(a)') trim(message)
            error stop 1
        end if
        write(unit, iostat=status, iomsg=message) bytes
        if (status /= 0) then
            write(error_unit, '(a)') trim(message)
            error stop 1
        end if
        close(unit, iostat=status, iomsg=message)
        if (status /= 0) then
            write(error_unit, '(a)') trim(message)
            error stop 1
        end if
        print '(a,i0,a)', "Wrote "//path//" (", len(bytes), " bytes)"
    end subroutine
end program
