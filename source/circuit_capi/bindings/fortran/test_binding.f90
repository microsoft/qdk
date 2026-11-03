! Copyright (c) Microsoft Corporation.
! Licensed under the MIT License.

module binding_test_cases
    use qdk_circuit
    use, intrinsic :: iso_fortran_env, only: error_unit
    implicit none
    type(body_t) :: retained_body
    type(circuit_t) :: active_alias

contains

    subroutine require(condition, message)
        logical, intent(in) :: condition
        character(*), intent(in) :: message
        if (.not. condition) then
            write(error_unit, '(a)') message
            error stop 1
        end if
    end subroutine

    subroutine check(error)
        type(error_t), intent(in) :: error
        if (error%failed()) then
            write(error_unit, '(a)') error%message
            error stop 1
        end if
    end subroutine

    subroutine correct(body, error)
        type(body_t), intent(in) :: body
        type(error_t), intent(out) :: error
        call body%x(1, error)
    end subroutine

    subroutine reject(body, error)
        type(body_t), intent(in) :: body
        type(error_t), intent(out) :: error
        call body%x(1, error)
        if (error%failed()) return
        error%code = 123
        error%message = "caller rejected the body"
    end subroutine

    subroutine invalid_gate(body, error)
        type(body_t), intent(in) :: body
        type(error_t), intent(out) :: error
        call body%x(99, error)
    end subroutine

    subroutine retain(body, error)
        type(body_t), intent(in) :: body
        type(error_t), intent(out) :: error
        retained_body = body
        call body%x(1, error)
    end subroutine

    subroutine retain_and_reject(body, error)
        type(body_t), intent(in) :: body
        type(error_t), intent(out) :: error
        retained_body = body
        call reject(body, error)
    end subroutine

    subroutine check_retired_body(body, error)
        type(body_t), intent(in) :: body
        type(error_t), intent(out) :: error
        type(error_t) :: retired
        call retained_body%x(1, retired)
        call require(retired%code == error_closed, "retained body accessed a reused slot")
        call body%x(1, error)
    end subroutine

    subroutine try_close_owner(body, error)
        type(body_t), intent(in) :: body
        type(error_t), intent(out) :: error
        type(error_t) :: refusal
        call active_alias%close(refusal)
        call require(refusal%code > 0, "active circuit close should return a native error")
        call require(active_alias%is_open(), "refused close lost the owner")
        call body%x(1, error)
    end subroutine

    subroutine all_body_gates(body, error)
        type(body_t), intent(in) :: body
        type(error_t), intent(out) :: error
        call body%h(0, error)
        if (error%failed()) return
        call body%cx(0, 1, error)
        if (error%failed()) return
        call body%x(1, error)
        if (error%failed()) return
        call body%measure(1, 1, error)
    end subroutine

    subroutine test_binary_survives_owner_close()
        type(circuit_t) :: c, loaded
        type(error_t) :: error
        character(kind=qdk_char, len=:), allocatable :: data, restored
        call c%init(2, 2, error)
        call check(error)
        call c%h(0, error)
        call check(error)
        call c%marshal_binary(data, error)
        call check(error)
        call c%close(error)
        call check(error)
        call loaded%load_binary(data, error)
        call check(error)
        call loaded%marshal_binary(restored, error)
        call check(error)
        call require(len(restored) == len(data) .and. restored == data, "binary roundtrip changed the snapshot")
        call loaded%close(error)
        call check(error)
    end subroutine

    subroutine test_text_matches_binary()
        type(circuit_t) :: c, loaded
        type(error_t) :: error
        character(kind=qdk_char, len=:), allocatable :: text, binary, restored
        call c%init(2, 2, error)
        call check(error)
        call c%x(1, error)
        call check(error)
        call c%marshal_binary(binary, error)
        call check(error)
        call c%marshal_text(text, error)
        call check(error)
        call loaded%load_text(text, error)
        call check(error)
        call loaded%marshal_binary(restored, error)
        call check(error)
        call require(len(restored) == len(binary) .and. restored == binary, "text and binary snapshots disagree")
        call c%close(error)
        call check(error)
        call loaded%close(error)
        call check(error)
    end subroutine

    subroutine test_clone_survives_original_close()
        type(circuit_t) :: c, copy
        type(error_t) :: error
        call c%init(2, 2, error)
        call check(error)
        call c%clone(copy, error)
        call check(error)
        call c%close(error)
        call check(error)
        call copy%cx(0, 1, error)
        call check(error)
        call copy%close(error)
        call check(error)
    end subroutine

    subroutine test_assignment_shares_lifetime()
        type(circuit_t) :: c, alias
        type(error_t) :: error
        call c%init(2, 2, error)
        call check(error)
        alias = c
        call alias%close(error)
        call check(error)
        call c%x(0, error)
        call require(error%code == error_closed, "copy retained a freed native pointer")
        call c%close(error)
        call check(error)
    end subroutine

    subroutine test_stale_alias_cannot_access_reused_slot()
        type(circuit_t) :: c, old, fresh
        type(error_t) :: error
        call c%init(2, 2, error)
        call check(error)
        old = c
        call c%close(error)
        call check(error)
        call fresh%init(2, 2, error)
        call check(error)
        call old%x(0, error)
        call require(error%code == error_closed, "stale alias accessed the new circuit")
        call old%close(error)
        call check(error)
        call fresh%x(1, error)
        call check(error)
        call fresh%close(error)
        call check(error)
    end subroutine

    subroutine test_live_initialization_is_refused()
        type(circuit_t) :: c
        type(error_t) :: error
        call c%init(2, 2, error)
        call check(error)
        call c%init(1, 0, error)
        call require(error%code == error_invalid, "initialization overwrote a live owner")
        call c%x(1, error)
        call check(error)
        call c%close(error)
        call check(error)
    end subroutine

    subroutine test_negative_dimensions_return_error()
        type(circuit_t) :: c
        type(error_t) :: error
        call c%init(-1, 0, error)
        call require(error%code > 0 .and. .not. c%is_open(), "accepted a negative dimension")
    end subroutine

    subroutine test_invalid_gate_returns_diagnostic()
        type(circuit_t) :: c
        type(error_t) :: error
        call c%init(2, 2, error)
        call check(error)
        call c%x(99, error)
        call require(error%code > 0, "missing native bounds diagnostic")
        call require(len(error%message) > 0, "native diagnostic was empty")
        call c%close(error)
        call check(error)
    end subroutine

    subroutine test_default_circuit_is_closed()
        type(circuit_t) :: c
        type(error_t) :: error
        call require(.not. c%is_open(), "default circuit is unexpectedly open")
        call c%x(0, error)
        call require(error%code == error_closed, "default circuit allowed operations")
        call c%close(error)
        call check(error)
    end subroutine

    subroutine test_adaptive_body_checks_profile()
        type(circuit_t) :: c
        type(error_t) :: error
        character(kind=qdk_char, len=:), allocatable :: data
        call c%init(2, 2, error)
        call check(error)
        call c%h(0, error)
        call check(error)
        call c%measure(0, 0, error)
        call check(error)
        call c%if_one(0, correct, error)
        call check(error)
        call c%measure(1, 1, error)
        call check(error)
        data = "previous output"
        call c%qir(profile_base, data, error)
        call require(error%failed(), "Base accepted feedback")
        call require(index(error%message, "Adaptive") > 0, "missing profile diagnostic")
        call require(.not. allocated(data), "failed export retained success-shaped output")
        call c%qir(profile_adaptive, data, error)
        call check(error)
        call require(len(data) > 0, "Adaptive QIR is empty")
        call c%close(error)
        call check(error)
    end subroutine

    subroutine test_callback_error_cancels_body()
        type(circuit_t) :: c
        type(error_t) :: error
        character(kind=qdk_char, len=:), allocatable :: before, after
        call c%init(2, 2, error)
        call check(error)
        call c%qir(profile_adaptive, before, error)
        call check(error)
        call c%if_one(0, reject, error)
        call require(error%code == 123, "callback error was lost")
        call require(error%message == "caller rejected the body", "callback diagnostic changed")
        call c%qir(profile_adaptive, after, error)
        call check(error)
        call require(len(after) == len(before) .and. after == before, "cancelled body changed the program")
        call c%close(error)
        call check(error)
    end subroutine

    subroutine test_append_error_releases_body()
        type(circuit_t) :: c
        type(error_t) :: error
        call c%init(2, 2, error)
        call check(error)
        call c%if_one(0, invalid_gate, error)
        call require(error%code > 0, "missing body append error")
        call c%h(0, error)
        call check(error)
        call c%close(error)
        call check(error)
    end subroutine

    subroutine test_retained_body_is_closed()
        type(circuit_t) :: c
        type(error_t) :: error
        call c%init(2, 2, error)
        call check(error)
        call c%if_one(0, retain, error)
        call check(error)
        call retained_body%x(1, error)
        call require(error%code == error_closed, "body escaped its scope")
        call c%if_one(0, check_retired_body, error)
        call check(error)
        call c%close(error)
        call check(error)
    end subroutine

    subroutine test_cancelled_body_is_closed()
        type(circuit_t) :: c
        type(error_t) :: error
        call c%init(2, 2, error)
        call check(error)
        call c%if_one(0, retain_and_reject, error)
        call require(error%code == 123, "callback error was lost")
        call retained_body%x(1, error)
        call require(error%code == error_closed, "cancelled body remained usable")
        call c%close(error)
        call check(error)
    end subroutine

    subroutine test_close_during_callback_is_refused()
        type(circuit_t) :: c
        type(error_t) :: error
        call c%init(2, 2, error)
        call check(error)
        active_alias = c
        call c%if_one(0, try_close_owner, error)
        call check(error)
        call c%close(error)
        call check(error)
    end subroutine

    subroutine test_registry_growth_preserves_handles()
        type(circuit_t) :: owners(20), first
        type(error_t) :: error
        integer :: i
        call owners(1)%init(1, 0, error)
        call check(error)
        first = owners(1)
        do i = 2, size(owners)
            call owners(i)%init(1, 0, error)
            call check(error)
        end do
        call first%h(0, error)
        call check(error)
        do i = 1, size(owners)
            call owners(i)%close(error)
            call check(error)
        end do
        call require(.not. first%is_open(), "growth invalidated shared lifetime tracking")
    end subroutine

    subroutine test_malformed_snapshot_returns_error()
        type(circuit_t) :: c
        type(error_t) :: error
        call c%load_binary("not a snapshot", error)
        call require(error%code > 0 .and. .not. c%is_open(), "invalid snapshot was accepted")
    end subroutine

    subroutine test_live_clone_destination_is_preserved()
        type(circuit_t) :: source, destination
        type(error_t) :: error
        call source%init(1, 0, error)
        call check(error)
        call destination%init(2, 0, error)
        call check(error)
        call source%clone(destination, error)
        call require(error%code == error_invalid, "clone overwrote a live destination")
        call destination%x(1, error)
        call check(error)
        call source%close(error)
        call check(error)
        call destination%close(error)
        call check(error)
    end subroutine

    subroutine test_body_gate_methods()
        type(circuit_t) :: c
        type(error_t) :: error
        character(kind=qdk_char, len=:), allocatable :: data
        call c%init(2, 2, error)
        call check(error)
        call c%if_one(0, all_body_gates, error)
        call check(error)
        call c%qir(profile_adaptive, data, error)
        call check(error)
        call c%close(error)
        call check(error)
    end subroutine
end module

program test_binding
    use binding_test_cases
    implicit none
    call test_binary_survives_owner_close()
    call test_text_matches_binary()
    call test_clone_survives_original_close()
    call test_assignment_shares_lifetime()
    call test_stale_alias_cannot_access_reused_slot()
    call test_live_initialization_is_refused()
    call test_negative_dimensions_return_error()
    call test_invalid_gate_returns_diagnostic()
    call test_default_circuit_is_closed()
    call test_adaptive_body_checks_profile()
    call test_callback_error_cancels_body()
    call test_append_error_releases_body()
    call test_retained_body_is_closed()
    call test_cancelled_body_is_closed()
    call test_close_during_callback_is_refused()
    call test_registry_growth_preserves_handles()
    call test_malformed_snapshot_returns_error()
    call test_live_clone_destination_is_preserved()
    call test_body_gate_methods()
    print '(a)', "19 reusable Fortran binding tests passed"
end program
