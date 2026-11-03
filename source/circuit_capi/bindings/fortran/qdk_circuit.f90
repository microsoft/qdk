! Copyright (c) Microsoft Corporation.
! Licensed under the MIT License.

module qdk_circuit
    use, intrinsic :: iso_c_binding
    implicit none
    private

    integer, parameter, public :: qdk_char = c_char
    integer, parameter, public :: profile_base = 1, profile_adaptive_ri = 2
    integer, parameter, public :: profile_adaptive_rif = 3, profile_adaptive = 4, profile_unrestricted = 5
    integer(c_int64_t), parameter, public :: error_closed = -1, error_invalid = -2, error_allocation = -3
    integer, parameter :: circuit_kind = 1, body_kind = 2
    integer, parameter :: gate_h = 0, gate_x = 1, gate_cx = 2, gate_measure = 3

    type, public :: error_t
        integer(c_int64_t) :: code = 0
        character(:), allocatable :: message
    contains
        procedure :: failed => error_failed
        procedure :: clear => error_clear
    end type

    type, public :: circuit_t
        private
        integer :: slot = 0
        integer(c_int64_t) :: generation = 0
    contains
        procedure :: init => circuit_init
        procedure :: clone => circuit_clone
        procedure :: close => circuit_close
        procedure :: is_open => circuit_is_open
        procedure :: h => circuit_h
        procedure :: x => circuit_x
        procedure :: cx => circuit_cx
        procedure :: measure => circuit_measure
        procedure :: if_one => circuit_if_one
        procedure :: marshal_binary
        procedure :: marshal_text
        procedure :: qir
        procedure :: load_binary
        procedure :: load_text
    end type

    type, public :: body_t
        private
        integer :: slot = 0
        integer(c_int64_t) :: generation = 0
    contains
        procedure :: h => body_h
        procedure :: x => body_x
        procedure :: cx => body_cx
        procedure :: measure => body_measure
    end type

    ! Assignment aliases a generation-checked slot, never a freed native pointer.
    ! Slots are reused, so registry storage follows peak concurrent handle count.
    type :: handle_slot
        type(c_ptr) :: native = c_null_ptr
        integer(c_int64_t) :: generation = 0
        integer :: kind = 0
    end type
    type(handle_slot), allocatable, save :: slots(:)
    integer(c_int64_t), save :: next_generation = 0

    type, bind(C) :: native_bytes
        type(c_ptr) :: data = c_null_ptr
        integer(c_size_t) :: len = 0
    end type

    abstract interface
        subroutine body_callback(body, error)
            import :: body_t, error_t
            type(body_t), intent(in) :: body
            type(error_t), intent(out) :: error
        end subroutine
    end interface

    interface
        function native_new(qubits, clbits, out, error) bind(C, name="qdk_f_new") result(status)
            import :: c_int64_t, c_ptr
            integer(c_int64_t), value :: qubits, clbits
            type(c_ptr) :: out, error
            integer(c_int64_t) :: status
        end function
        function native_clone(circuit, out, error) bind(C, name="qdk_f_clone") result(status)
            import :: c_int64_t, c_ptr
            type(c_ptr), value :: circuit
            type(c_ptr) :: out, error
            integer(c_int64_t) :: status
        end function
        function native_close(circuit, error) bind(C, name="qdk_f_close") result(status)
            import :: c_int64_t, c_ptr
            type(c_ptr) :: circuit, error
            integer(c_int64_t) :: status
        end function
        function native_append(circuit, body, gate, first, second, bit, error) &
                bind(C, name="qdk_f_append") result(status)
            import :: c_int64_t, c_ptr
            type(c_ptr), value :: circuit, body
            integer(c_int64_t), value :: gate, first, second, bit
            type(c_ptr) :: error
            integer(c_int64_t) :: status
        end function
        function native_clbit(circuit, bit, out, error) bind(C, name="qdk_f_clbit") result(status)
            import :: c_int64_t, c_ptr
            type(c_ptr), value :: circuit
            integer(c_int64_t), value :: bit
            type(c_ptr) :: out, error
            integer(c_int64_t) :: status
        end function
        function native_begin(circuit, condition, out, error) bind(C, name="qdk_f_begin") result(status)
            import :: c_int64_t, c_ptr
            type(c_ptr), value :: circuit, condition
            type(c_ptr) :: out, error
            integer(c_int64_t) :: status
        end function
        function native_finish(body, error) bind(C, name="qdk_f_finish") result(status)
            import :: c_int64_t, c_ptr
            type(c_ptr), value :: body
            type(c_ptr) :: error
            integer(c_int64_t) :: status
        end function
        function native_cancel(body, error) bind(C, name="qdk_f_cancel") result(status)
            import :: c_int64_t, c_ptr
            type(c_ptr), value :: body
            type(c_ptr) :: error
            integer(c_int64_t) :: status
        end function
        function native_body_free(body, error) bind(C, name="qdk_f_body_free") result(status)
            import :: c_int64_t, c_ptr
            type(c_ptr) :: body, error
            integer(c_int64_t) :: status
        end function
        function native_expression_free(expression, error) bind(C, name="qdk_f_expression_free") result(status)
            import :: c_int64_t, c_ptr
            type(c_ptr) :: expression, error
            integer(c_int64_t) :: status
        end function
        function native_export(circuit, format, profile, out, error) bind(C, name="qdk_f_export") result(status)
            import :: c_int64_t, c_ptr, native_bytes
            type(c_ptr), value :: circuit
            integer(c_int64_t), value :: format, profile
            type(native_bytes) :: out
            type(c_ptr) :: error
            integer(c_int64_t) :: status
        end function
        function native_import(text, data, length, out, error) bind(C, name="qdk_f_import") result(status)
            import :: c_int64_t, c_ptr, c_size_t
            integer(c_int64_t), value :: text
            type(c_ptr), value :: data
            integer(c_size_t), value :: length
            type(c_ptr) :: out, error
            integer(c_int64_t) :: status
        end function
        function native_message(error, out) bind(C, name="qdk_f_message") result(status)
            import :: c_int64_t, c_ptr, native_bytes
            type(c_ptr), value :: error
            type(native_bytes) :: out
            integer(c_int64_t) :: status
        end function
        subroutine diagnostic_free(error) bind(C, name="qdk_diagnostic_free")
            import :: c_ptr
            type(c_ptr) :: error
        end subroutine
        subroutine bytes_free(bytes) bind(C, name="qdk_bytes_free")
            import :: native_bytes
            type(native_bytes) :: bytes
        end subroutine
    end interface

contains

    logical function error_failed(this)
        class(error_t), intent(in) :: this
        error_failed = this%code /= 0
    end function

    subroutine error_clear(this)
        class(error_t), intent(inout) :: this
        this%code = 0
        this%message = ""
    end subroutine

    subroutine fail(error, code, message)
        type(error_t), intent(out) :: error
        integer(c_int64_t), intent(in) :: code
        character(*), intent(in) :: message
        error%code = code
        error%message = message
    end subroutine

    subroutine merge_error(error, cleanup)
        type(error_t), intent(inout) :: error
        type(error_t), intent(in) :: cleanup
        if (.not. cleanup%failed()) return
        if (.not. error%failed()) then
            error = cleanup
        else if (allocated(error%message)) then
            error%message = error%message//"; cleanup: "//cleanup%message
        else
            error%message = "cleanup: "//cleanup%message
        end if
    end subroutine

    subroutine take_bytes(bytes, data, error)
        type(native_bytes), intent(inout) :: bytes
        character(kind=c_char, len=:), allocatable, intent(out) :: data
        type(error_t), intent(out) :: error
        character(kind=c_char), pointer :: view(:)
        integer :: i, length, status
        call error%clear()
        if (bytes%len < 0_c_size_t .or. bytes%len > int(huge(0), c_size_t)) then
            call fail(error, error_invalid, "native buffer exceeds Fortran string bounds")
        else if (bytes%len > 0 .and. .not. c_associated(bytes%data)) then
            call fail(error, error_invalid, "nonempty native buffer has a null pointer")
        else
            length = int(bytes%len)
            allocate(character(kind=c_char, len=length) :: data, stat=status)
            if (status /= 0) then
                call fail(error, error_allocation, "could not allocate output buffer")
            else if (length > 0) then
                call c_f_pointer(bytes%data, view, [length])
                do i = 1, length
                    data(i:i) = view(i)
                end do
                nullify(view)
            end if
        end if
        call bytes_free(bytes)
    end subroutine

    subroutine capture(status, diagnostic, error)
        integer(c_int64_t), intent(in) :: status
        type(c_ptr), intent(inout) :: diagnostic
        type(error_t), intent(out) :: error
        type(error_t) :: copying
        type(native_bytes) :: bytes
        character(kind=c_char, len=:), allocatable :: message
        integer(c_int64_t) :: read_status
        call error%clear()
        if (status == 0) then
            if (c_associated(diagnostic)) call fail(error, error_invalid, "QDK returned a diagnostic on success")
        else if (.not. c_associated(diagnostic)) then
            call fail(error, status, "QDK failed without a diagnostic")
        else
            read_status = native_message(diagnostic, bytes)
            if (read_status /= 0) then
                call bytes_free(bytes)
                call fail(error, status, "could not read QDK diagnostic")
            else
                call take_bytes(bytes, message, copying)
                if (copying%failed()) then
                    call fail(error, status, copying%message)
                else
                    call fail(error, status, message)
                end if
            end if
        end if
        call diagnostic_free(diagnostic)
    end subroutine

    logical function live(slot, generation, kind)
        integer, intent(in) :: slot, kind
        integer(c_int64_t), intent(in) :: generation
        live = .false.
        if (.not. allocated(slots)) return
        if (slot < 1 .or. slot > size(slots)) return
        live = slots(slot)%generation == generation .and. slots(slot)%kind == kind &
            .and. c_associated(slots(slot)%native)
    end function

    function handle(slot, generation, kind, error) result(native)
        integer, intent(in) :: slot, kind
        integer(c_int64_t), intent(in) :: generation
        type(error_t), intent(out) :: error
        type(c_ptr) :: native
        call error%clear()
        native = c_null_ptr
        if (.not. live(slot, generation, kind)) then
            call fail(error, error_closed, "circuit is closed or body is outside its callback")
            return
        end if
        native = slots(slot)%native
    end function

    subroutine register(native, kind, slot, generation, error)
        type(c_ptr), intent(in) :: native
        integer, intent(in) :: kind
        integer, intent(out) :: slot
        integer(c_int64_t), intent(out) :: generation
        type(error_t), intent(out) :: error
        type(handle_slot), allocatable :: grown(:)
        integer :: i, capacity, status
        call error%clear()
        slot = 0
        generation = 0
        if (next_generation == huge(next_generation)) then
            call fail(error, error_allocation, "handle generation space exhausted")
            return
        end if
        capacity = 0
        if (allocated(slots)) then
            capacity = size(slots)
            do i = 1, capacity
                if (.not. c_associated(slots(i)%native)) then
                    slot = i
                    exit
                end if
            end do
        end if
        if (slot == 0) then
            if (capacity > huge(capacity) / 2 - 8) then
                call fail(error, error_allocation, "handle registry capacity exhausted")
                return
            end if
            allocate(grown(max(8, capacity * 2)), stat=status)
            if (status /= 0) then
                call fail(error, error_allocation, "could not grow handle registry")
                return
            end if
            if (capacity > 0) grown(:capacity) = slots
            call move_alloc(grown, slots)
            slot = capacity + 1
        end if
        next_generation = next_generation + 1
        generation = next_generation
        slots(slot) = handle_slot(native, generation, kind)
    end subroutine

    subroutine adopt(this, native, error)
        class(circuit_t), intent(inout) :: this
        type(c_ptr), intent(inout) :: native
        type(error_t), intent(out) :: error
        type(error_t) :: cleanup
        type(c_ptr) :: diagnostic
        integer(c_int64_t) :: status
        call register(native, circuit_kind, this%slot, this%generation, error)
        if (error%failed()) then
            status = native_close(native, diagnostic)
            call capture(status, diagnostic, cleanup)
            call merge_error(error, cleanup)
        end if
    end subroutine

    logical function circuit_is_open(this)
        class(circuit_t), intent(in) :: this
        circuit_is_open = live(this%slot, this%generation, circuit_kind)
    end function

    subroutine circuit_init(this, qubits, clbits, error)
        class(circuit_t), intent(inout) :: this
        integer, intent(in) :: qubits, clbits
        type(error_t), intent(out) :: error
        type(c_ptr) :: native, diagnostic
        integer(c_int64_t) :: status
        if (this%is_open()) then
            call fail(error, error_invalid, "close the circuit before initializing it again")
            return
        end if
        status = native_new(int(qubits, c_int64_t), int(clbits, c_int64_t), native, diagnostic)
        call capture(status, diagnostic, error)
        if (.not. error%failed()) call adopt(this, native, error)
    end subroutine

    subroutine circuit_clone(this, copy, error)
        class(circuit_t), intent(in) :: this
        type(circuit_t), intent(inout) :: copy
        type(error_t), intent(out) :: error
        type(c_ptr) :: source, native, diagnostic
        integer(c_int64_t) :: status
        if (copy%is_open()) then
            call fail(error, error_invalid, "close the destination before cloning into it")
            return
        end if
        source = handle(this%slot, this%generation, circuit_kind, error)
        if (error%failed()) return
        status = native_clone(source, native, diagnostic)
        call capture(status, diagnostic, error)
        if (.not. error%failed()) call adopt(copy, native, error)
    end subroutine

    subroutine circuit_close(this, error)
        class(circuit_t), intent(inout) :: this
        type(error_t), intent(out) :: error
        type(c_ptr) :: native, diagnostic
        integer(c_int64_t) :: status
        call error%clear()
        if (.not. this%is_open()) return
        native = slots(this%slot)%native
        status = native_close(native, diagnostic)
        call capture(status, diagnostic, error)
        if (.not. error%failed()) then
            slots(this%slot)%native = c_null_ptr
            this%slot = 0
            this%generation = 0
        end if
    end subroutine

    subroutine append(slot, generation, kind, gate, first, second, bit, error)
        integer, intent(in) :: slot, kind, gate, first, second, bit
        integer(c_int64_t), intent(in) :: generation
        type(error_t), intent(out) :: error
        type(c_ptr) :: native, circuit, body, diagnostic
        integer(c_int64_t) :: status
        native = handle(slot, generation, kind, error)
        if (error%failed()) return
        circuit = c_null_ptr
        body = c_null_ptr
        if (kind == circuit_kind) then
            circuit = native
        else
            body = native
        end if
        status = native_append(circuit, body, int(gate, c_int64_t), int(first, c_int64_t), &
            int(second, c_int64_t), int(bit, c_int64_t), diagnostic)
        call capture(status, diagnostic, error)
    end subroutine

    subroutine circuit_h(this, qubit, error)
        class(circuit_t), intent(inout) :: this
        integer, intent(in) :: qubit
        type(error_t), intent(out) :: error
        call append(this%slot, this%generation, circuit_kind, gate_h, qubit, 0, 0, error)
    end subroutine

    subroutine circuit_x(this, qubit, error)
        class(circuit_t), intent(inout) :: this
        integer, intent(in) :: qubit
        type(error_t), intent(out) :: error
        call append(this%slot, this%generation, circuit_kind, gate_x, qubit, 0, 0, error)
    end subroutine

    subroutine circuit_cx(this, control, target, error)
        class(circuit_t), intent(inout) :: this
        integer, intent(in) :: control, target
        type(error_t), intent(out) :: error
        call append(this%slot, this%generation, circuit_kind, gate_cx, control, target, 0, error)
    end subroutine

    subroutine circuit_measure(this, qubit, clbit, error)
        class(circuit_t), intent(inout) :: this
        integer, intent(in) :: qubit, clbit
        type(error_t), intent(out) :: error
        call append(this%slot, this%generation, circuit_kind, gate_measure, qubit, 0, clbit, error)
    end subroutine

    subroutine body_h(this, qubit, error)
        class(body_t), intent(in) :: this
        integer, intent(in) :: qubit
        type(error_t), intent(out) :: error
        call append(this%slot, this%generation, body_kind, gate_h, qubit, 0, 0, error)
    end subroutine

    subroutine body_x(this, qubit, error)
        class(body_t), intent(in) :: this
        integer, intent(in) :: qubit
        type(error_t), intent(out) :: error
        call append(this%slot, this%generation, body_kind, gate_x, qubit, 0, 0, error)
    end subroutine

    subroutine body_cx(this, control, target, error)
        class(body_t), intent(in) :: this
        integer, intent(in) :: control, target
        type(error_t), intent(out) :: error
        call append(this%slot, this%generation, body_kind, gate_cx, control, target, 0, error)
    end subroutine

    subroutine body_measure(this, qubit, clbit, error)
        class(body_t), intent(in) :: this
        integer, intent(in) :: qubit, clbit
        type(error_t), intent(out) :: error
        call append(this%slot, this%generation, body_kind, gate_measure, qubit, 0, clbit, error)
    end subroutine

    subroutine circuit_if_one(this, clbit, action, error)
        class(circuit_t), intent(inout) :: this
        integer, intent(in) :: clbit
        procedure(body_callback) :: action
        type(error_t), intent(out) :: error
        type(error_t) :: cleanup
        type(body_t) :: body
        type(c_ptr) :: circuit, condition, builder, diagnostic
        integer(c_int64_t) :: status
        logical :: finished
        circuit = handle(this%slot, this%generation, circuit_kind, error)
        if (error%failed()) return
        status = native_clbit(circuit, int(clbit, c_int64_t), condition, diagnostic)
        call capture(status, diagnostic, error)
        if (error%failed()) return
        status = native_begin(circuit, condition, builder, diagnostic)
        call capture(status, diagnostic, error)
        if (.not. error%failed()) then
            finished = .false.
            call register(builder, body_kind, body%slot, body%generation, error)
            if (.not. error%failed()) call action(body, error)
            if (.not. error%failed()) then
                status = native_finish(builder, diagnostic)
                call capture(status, diagnostic, error)
                finished = .not. error%failed()
            end if
            if (.not. finished) then
                status = native_cancel(builder, diagnostic)
                call capture(status, diagnostic, cleanup)
                call merge_error(error, cleanup)
            end if
            status = native_body_free(builder, diagnostic)
            call capture(status, diagnostic, cleanup)
            call merge_error(error, cleanup)
            if (body%slot > 0) then
                ! Invalidate escaped body copies even if native cleanup reports failure.
                slots(body%slot)%kind = 0
                slots(body%slot)%native = builder
            end if
        end if
        status = native_expression_free(condition, diagnostic)
        call capture(status, diagnostic, cleanup)
        call merge_error(error, cleanup)
    end subroutine

    subroutine export(this, format, profile, data, error)
        class(circuit_t), intent(in) :: this
        integer, intent(in) :: format, profile
        character(kind=c_char, len=:), allocatable, intent(out) :: data
        type(error_t), intent(out) :: error
        type(c_ptr) :: native, diagnostic
        type(native_bytes) :: bytes
        integer(c_int64_t) :: status
        native = handle(this%slot, this%generation, circuit_kind, error)
        if (error%failed()) return
        status = native_export(native, int(format, c_int64_t), int(profile, c_int64_t), bytes, diagnostic)
        call capture(status, diagnostic, error)
        if (error%failed()) then
            call bytes_free(bytes)
        else
            call take_bytes(bytes, data, error)
        end if
    end subroutine

    subroutine marshal_binary(this, data, error)
        class(circuit_t), intent(in) :: this
        character(kind=c_char, len=:), allocatable, intent(out) :: data
        type(error_t), intent(out) :: error
        call export(this, 0, 0, data, error)
    end subroutine

    subroutine marshal_text(this, data, error)
        class(circuit_t), intent(in) :: this
        character(kind=c_char, len=:), allocatable, intent(out) :: data
        type(error_t), intent(out) :: error
        call export(this, 1, 0, data, error)
    end subroutine

    subroutine qir(this, profile, data, error)
        class(circuit_t), intent(in) :: this
        integer, intent(in) :: profile
        character(kind=c_char, len=:), allocatable, intent(out) :: data
        type(error_t), intent(out) :: error
        call export(this, 2, profile, data, error)
    end subroutine

    subroutine load(this, data, text, error)
        class(circuit_t), intent(inout) :: this
        character(kind=c_char, len=*), target, intent(in) :: data
        integer, intent(in) :: text
        type(error_t), intent(out) :: error
        type(c_ptr) :: buffer, native, diagnostic
        integer(c_int64_t) :: status
        if (this%is_open()) then
            call fail(error, error_invalid, "close the circuit before importing into it")
            return
        end if
        buffer = c_null_ptr
        if (len(data) > 0) buffer = c_loc(data(1:1))
        status = native_import(int(text, c_int64_t), buffer, len(data, kind=c_size_t), native, diagnostic)
        call capture(status, diagnostic, error)
        if (.not. error%failed()) call adopt(this, native, error)
    end subroutine

    subroutine load_binary(this, data, error)
        class(circuit_t), intent(inout) :: this
        character(kind=c_char, len=*), intent(in) :: data
        type(error_t), intent(out) :: error
        call load(this, data, 0, error)
    end subroutine

    subroutine load_text(this, data, error)
        class(circuit_t), intent(inout) :: this
        character(kind=c_char, len=*), intent(in) :: data
        type(error_t), intent(out) :: error
        call load(this, data, 1, error)
    end subroutine
end module
