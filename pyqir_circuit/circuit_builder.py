from itertools import chain
from dataclasses import dataclass, field
from typing import List, Optional, Any, Tuple, Dict
import pyqir


# Lightweight datatypes to mirror the Rust Circuit shape
@dataclass
class Register:
    qubit: Optional[int]
    result: Optional[int] = None


@dataclass
class ComponentColumn:
    components: List[Any] = field(default_factory=list)


@dataclass
class Unitary:
    gate: str
    args: List[str] = field(default_factory=list)
    children: List[ComponentColumn] = field(default_factory=list)
    targets: List[Register] = field(default_factory=list)
    controls: List[Register] = field(default_factory=list)
    is_adjoint: bool = False
    parsed_metadata: Optional[Dict[str, Any]] = None


@dataclass
class Measurement:
    gate: str
    args: List[str] = field(default_factory=list)
    children: List[ComponentColumn] = field(default_factory=list)
    qubits: List[Register] = field(default_factory=list)
    results: List[Register] = field(default_factory=list)
    parsed_metadata: Optional[Dict[str, Any]] = None


@dataclass
class Ket:
    gate: str
    args: List[str] = field(default_factory=list)
    children: List[ComponentColumn] = field(default_factory=list)
    targets: List[Register] = field(default_factory=list)
    parsed_metadata: Optional[Dict[str, Any]] = None


# A Component is one of Unitary, Measurement, Ket
Component = Any
ComponentGrid = List[ComponentColumn]


@dataclass
class Qubit:
    id: int
    num_results: int = 0


@dataclass
class Circuit:
    qubits: List[Qubit]
    component_grid: ComponentGrid


# ProgramMap and minimal expression/value tracking
class ProgramMap:
    def __init__(self, num_qubits: int):
        # qubits: list of (Qubit, [result_ids])
        self.qubits: List[Tuple[Qubit, List[int]]] = [
            (Qubit(id=i), []) for i in range(num_qubits)
        ]
        # results: result_id -> qubit_id
        self.results: Dict[int, int] = {}
        # variables: variable_id -> expression (opaque)
        self.variables: Dict[Any, Any] = {}
        # blocks: block_id -> {'operations': [Component], 'successor': Optional[block_id]}
        self.blocks: Dict[Any, Dict[str, Any]] = {}

    def into_qubits(self) -> List[Qubit]:
        return [
            Qubit(id=q.id, num_results=len(results)) for (q, results) in self.qubits
        ]

    def link_result_to_qubit(self, qubit_id: int, result_id: int) -> int:
        self.results[result_id] = qubit_id
        results_for_qubit = self.qubits[qubit_id][1]
        if result_id in results_for_qubit:
            return results_for_qubit.index(result_id)
        results_for_qubit.append(result_id)
        return len(results_for_qubit) - 1

    def result_register(self, result_id: int) -> Register:
        qubit_id = self.results.get(result_id)
        if qubit_id is None:
            raise RuntimeError(f"result {result_id} not linked to any qubit")
        idx = self.link_result_to_qubit(qubit_id, result_id)
        return Register(qubit=qubit_id, result=idx)

    def store_expr_in_variable(self, variable: Any, expr: Any):
        if variable in self.variables:
            raise RuntimeError(f"variable {variable} already stored")
        self.variables[variable] = expr

    def expr_for_variable(self, variable: Any) -> Any:
        if variable not in self.variables:
            raise RuntimeError(f"variable {variable} not linked to an expression")
        return self.variables[variable]

    def condition_for_variable(self, variable: Any) -> Tuple[List[int], str]:
        # Return a list of linked result ids and a string description of condition
        expr = self.expr_for_variable(variable)
        # If expr is a simple placeholder that contains 'results', return them
        results = expr.get("results", []) if isinstance(expr, dict) else []
        cond_str = expr.get("str", "<cond>") if isinstance(expr, dict) else str(expr)
        if not results:
            # In the Rust code, branching on a constant boolean is rejected. We'll emulate that by raising.
            if expr == True or expr == False:
                raise RuntimeError("constant condition in branch")
            # if no results found, assume it's unsupported here
        return results, cond_str


# Helpers for converting lists of operations into a grid
def operation_list_to_grid(
    operations: List[Component], num_qubits: int, loop_detection: bool
) -> ComponentGrid:
    """
    Converts a list of operations into a 2D grid of operations in col-row format.
    Operations will be left-justified as much as possible in the resulting grid.
    Children operations are recursively converted into a grid.

    This is a Python port of the Rust operation_list_to_grid function.
    Loop detection is not implemented (always assumed false).
    """
    # Since loop_detection is always false, we skip the collapse_repetition step
    return operation_list_to_grid_inner(operations, num_qubits)


def operation_list_to_grid_inner(
    operations: List[Component], num_qubits: int
) -> ComponentGrid:
    """
    Inner implementation of operation_list_to_grid that converts operations to a grid.
    """
    # Process children for each operation - convert single lists to grids
    for op in operations:
        # If the operation has children in a single list, convert to grid
        if len(op.children) == 1:
            if isinstance(op, Measurement):
                op.children = operation_list_to_grid_inner(
                    op.children[0].components, num_qubits
                )
            elif isinstance(op, Unitary):
                op.children = operation_list_to_grid_inner(
                    op.children[0].components, num_qubits
                )
            elif isinstance(op, Ket):
                op.children = operation_list_to_grid_inner(
                    op.children[0].components, num_qubits
                )

    # Convert the operations into a component grid
    padded_array = operation_list_to_padded_array(operations, num_qubits)
    unpacked_array = remove_padding(padded_array)

    component_grid = []
    for col in unpacked_array:
        component_grid.append(ComponentColumn(components=col))

    return component_grid


def operation_list_to_padded_array(
    operations: List[Component], num_qubits: int
) -> List[List[Optional[Component]]]:
    """
    Converts a list of operations into a padded 2D array of operations.
    """
    if not operations:
        return []

    grouped_ops = group_operations(operations, num_qubits)
    aligned_ops = transform_to_col_row(align_ops(grouped_ops))

    # Convert to optional operations so we can take operations out without messing up indexing
    operations_optional = [op for op in operations]

    result = []
    for col in aligned_ops:
        result_col = []
        for row_value in col:
            if row_value is not None:
                result_col.append(operations_optional[row_value])
            else:
                result_col.append(None)
        result.append(result_col)

    return result


def remove_padding(
    operations: List[List[Optional[Component]]],
) -> List[List[Component]]:
    """
    Removes padding (None values) from a 2D array of operations.
    """
    return [[op for op in col if op is not None] for col in operations]


def transform_to_col_row(
    aligned_ops: List[List[Optional[int]]],
) -> List[List[Optional[int]]]:
    """
    Transforms a row-col 2D array into an equivalent col-row 2D array.
    """
    if not aligned_ops:
        return []

    num_rows = len(aligned_ops)
    num_cols = max(len(row) for row in aligned_ops) if aligned_ops else 0

    col_row_array: List[List[Optional[int]]] = [
        [None for _ in range(num_rows)] for _ in range(num_cols)
    ]

    for row_idx, row_data in enumerate(aligned_ops):
        for col_idx, value in enumerate(row_data):
            if col_idx < len(col_row_array):
                col_row_array[col_idx][row_idx] = value

    return col_row_array


def group_operations(operations: List[Component], num_qubits: int) -> List[List[int]]:
    """
    Groups operations by their respective registers.
    Returns a 2D vector of indices where grouped_ops[i][j] is the index of the operation
    at register i and column j (not yet aligned/padded).
    """
    grouped_ops = [[] for _ in range(num_qubits)]

    max_q_id = max(0, num_qubits - 1) if num_qubits > 0 else 0

    for instr_idx, op in enumerate(operations):
        # Get controls and targets based on operation type
        if isinstance(op, Measurement):
            controls = op.qubits
            targets = op.results
        elif isinstance(op, Unitary):
            controls = op.controls
            targets = op.targets
        elif isinstance(op, Ket):
            controls = []
            targets = op.targets
        else:
            continue

        # Combine all quantum registers
        all_regs = list(controls) + list(targets)

        if not all_regs:
            # If no registers, skip this operation
            continue

        # Get qubit indices
        q_reg_indices = [reg.qubit for reg in all_regs if reg.qubit is not None]

        # Check if any controls are classical (have result field)
        classical_controls = [reg for reg in controls if reg.result is not None]
        is_classically_controlled = len(classical_controls) > 0

        if is_classically_controlled:
            # Classical control affects all qubits
            min_reg_idx = 0
            max_reg_idx = max_q_id
        else:
            if not q_reg_indices:
                continue
            min_reg_idx = min(q_reg_indices)
            max_reg_idx = max(q_reg_indices)

        # Add this operation to all affected qubit registers
        for reg_idx in range(min_reg_idx, max_reg_idx + 1):
            if reg_idx < len(grouped_ops):
                grouped_ops[reg_idx].append(instr_idx)

    return grouped_ops


def align_ops(ops: List[List[int]]) -> List[List[Optional[int]]]:
    """
    Aligns operations by padding registers with None to make sure that multiqubit
    gates are in the same column.
    """
    max_num_ops = max(len(reg_ops) for reg_ops in ops) if ops else 0

    # Convert to optional and initialize
    padded_ops = [
        [op for op in reg_ops] + [None] * (max_num_ops - len(reg_ops))
        for reg_ops in ops
    ]

    col = 0
    while col < max_num_ops:
        # For each register, check if we need to align this column
        for reg_idx in range(len(padded_ops)):
            if col < len(padded_ops[reg_idx]) and padded_ops[reg_idx][col] is not None:
                # This register has an operation at this column
                # We need to ensure all other registers involved in this operation
                # also have their operation at the same column
                op_idx = padded_ops[reg_idx][col]

                # Find all other registers that should have this same operation
                for other_reg_idx in range(len(padded_ops)):
                    if other_reg_idx != reg_idx:
                        # Check if this register should have the same operation
                        if op_idx in padded_ops[other_reg_idx]:
                            current_pos = padded_ops[other_reg_idx].index(op_idx)
                            if current_pos != col:
                                # Move the operation to the correct column
                                padded_ops[other_reg_idx][current_pos] = None
                                padded_ops[other_reg_idx][col] = op_idx

        col += 1

    # Convert back to proper Optional typing
    result = []
    for reg_ops in padded_ops:
        result.append([op if op is not None else None for op in reg_ops])

    return result


# Simplified mapping of callable invocations to operations, mirroring the Rust structure
def map_callable_to_operations(
    state: ProgramMap, instruction: pyqir.Call
) -> List[Component]:
    callable_name = instruction.callee.name
    operands = instruction.args

    # Measurement-like callables
    if callable_name in ("__quantum__qis__m__body", "__quantum__qis__mresetz__body"):
        gate = "MResetZ" if "mresetz" in callable_name else "M"
        # gather measurement operands: expect sequence [qubit_literal, result_literal, ...]
        qubits = []
        results = []
        current_qubit = None
        for op in operands:
            if pyqir.is_qubit_type(op.type):
                current_qubit = pyqir.qubit_id(op)
                if current_qubit is None:
                    raise RuntimeError("qubit operand missing value")
                qubits.append(Register(qubit=current_qubit))
            elif pyqir.is_result_type(op.type):
                r = pyqir.result_id(op)
                if r is None:
                    raise RuntimeError("result operand missing value")
                if current_qubit is None:
                    raise RuntimeError(
                        "measurement should have a qubit operand before a result"
                    )
                state.link_result_to_qubit(current_qubit, r)
                results.append(state.result_register(r))
            else:
                # unsupported operand in stub
                raise RuntimeError(
                    f"unsupported operand for measurement: {op.name} {op.type}"
                )

        ops: List[Component] = []

        if gate == "MResetZ":
            ops.append(
                Measurement(
                    gate=gate,
                    args=[],
                    children=[],
                    qubits=qubits.copy(),
                    results=results.copy(),
                )
            )
            ops.append(
                Ket(
                    gate="0",
                    args=[],
                    children=[],
                    targets=qubits,
                )
            )
        else:
            ops.append(
                Measurement(
                    gate=gate,
                    args=[],
                    children=[],
                    qubits=qubits,
                    results=results,
                )
            )
        return ops

    # Reset callable
    if callable_name == "__quantum__qis__reset__body":
        # expect a single qubit operand
        q = None
        if (
            operands
            and operands[0] is not None
            and pyqir.is_qubit_type(operands[0].type)
        ):
            q = pyqir.qubit_id(operands[0])
        if q is None:
            raise RuntimeError("reset expected a qubit operand")
        return [
            Ket(
                gate="0",
                args=[],
                children=[],
                targets=[Register(qubit=q)],
            )
        ]

    # Regular gate-like callables: map names to gate and operand types
    # Based on intrinsic.rs quantum gate mappings
    known = {
        # Single qubit gates
        "__quantum__qis__h__body": ("H", ["Target"]),
        "__quantum__qis__s__body": ("S", ["Target"]),
        "__quantum__qis__s__adj": (
            "S",
            ["Target"],
        ),  # Note: adjoint flag handled separately
        "__quantum__qis__sx__body": ("SX", ["Target"]),
        "__quantum__qis__t__body": ("T", ["Target"]),
        "__quantum__qis__t__adj": (
            "T",
            ["Target"],
        ),  # Note: adjoint flag handled separately
        "__quantum__qis__x__body": ("X", ["Target"]),
        "__quantum__qis__y__body": ("Y", ["Target"]),
        "__quantum__qis__z__body": ("Z", ["Target"]),
        # Single qubit rotations (angle + target)
        "__quantum__qis__rx__body": ("Rx", ["Arg", "Target"]),
        "__quantum__qis__ry__body": ("Ry", ["Arg", "Target"]),
        "__quantum__qis__rz__body": ("Rz", ["Arg", "Target"]),
        # Two qubit gates
        "__quantum__qis__cx__body": ("X", ["Control", "Target"]),  # Controlled X
        "__quantum__qis__cy__body": ("Y", ["Control", "Target"]),  # Controlled Y
        "__quantum__qis__cz__body": ("Z", ["Control", "Target"]),  # Controlled Z
        "__quantum__qis__swap__body": (
            "SWAP",
            ["Target", "Target"],
        ),  # Special case: two targets
        # Two qubit rotations (angle + two targets)
        "__quantum__qis__rxx__body": ("Rxx", ["Arg", "Target", "Target"]),
        "__quantum__qis__ryy__body": ("Ryy", ["Arg", "Target", "Target"]),
        "__quantum__qis__rzz__body": ("Rzz", ["Arg", "Target", "Target"]),
        # Three qubit gate
        "__quantum__qis__ccx__body": ("X", ["Control", "Control", "Target"]),  # Toffoli
    }
    if callable_name in known:
        gate, operand_types = known[callable_name]

        # Handle adjoint operations by setting the is_adjoint flag
        is_adjoint = callable_name.endswith("__adj")
        if is_adjoint:
            # Remove the "†" or similar notation from gate name if present
            # The adjoint flag will be set in the Unitary object
            pass
    else:
        gate = callable_name
        operand_types = []
        is_adjoint = False
        # in the Rust code the unknown callable inspects operands to build operand_types; we skip here

    # Gather targets, controls and args according to the operand types provided
    targets: List[Register] = []
    controls: List[Register] = []
    args: List[str] = []
    for operand, typ in zip(operands, operand_types):
        if typ == "Target":
            if pyqir.is_qubit_type(operand.type):
                val = pyqir.qubit_id(operand)
                if val is None:
                    raise RuntimeError("Missing qubit value for target operand")
                targets.append(Register(qubit=val))
        elif typ == "Control":
            if pyqir.is_qubit_type(operand.type):
                val = pyqir.qubit_id(operand)
                if val is None:
                    raise RuntimeError("Missing qubit value for control operand")
                controls.append(Register(qubit=val))
        elif typ == "Arg":
            # For now, just convert to string
            args.append(str(operand))

    # Skip operations that have neither controls nor targets per Rust logic
    if not targets and not controls:
        return []
    return [
        Unitary(
            gate=gate,
            args=args,
            children=[],
            targets=targets,
            controls=controls,
            is_adjoint=is_adjoint,
        )
    ]


# Main Python port of `make_circuit` (simplified / stubbed)
def make_circuit(
    module: pyqir.Module,
    package_store: Any = None,
    position_encoding: Any = None,
    loop_detection: bool = False,
    group_by_scope: bool = False,
) -> Circuit:
    """
    Create a Circuit object from a program representation.

    This is a light-weight Python port of the Rust `make_circuit` function. It does not
    attempt to implement all of the compiler logic — instead it mirrors the overall
    control flow: build a ProgramMap, iterate over blocks and instructions, convert
    instructions into operations, expand branches/successors, then produce a Circuit
    object with `qubits` and a `component_grid`.
    """
    # program is expected to expose: num_qubits: int, blocks: dict/block-list, callables: mapping, entry: entry_id
    callables = {i: callable for i, callable in enumerate(module.functions)}
    blocks = {
        i: block
        for i, block in enumerate(
            chain.from_iterable(f.basic_blocks for f in module.functions)
        )
    }
    entry_point = next(filter(lambda c: pyqir.is_entry_point(c[1]), callables.items()))
    num_qubits = pyqir.required_num_qubits(entry_point[1])
    if num_qubits is None:
        num_qubits = 0
    state = ProgramMap(num_qubits)

    if len(entry_point[1].basic_blocks) == 0:
        raise RuntimeError("entry point has no basic blocks")

    operations: List[Component] = []

    current_block = entry_point[1].basic_blocks[0]
    while current_block is not None:
        terminator = current_block.terminator
        if terminator is None:
            raise RuntimeError("block has no terminator")
        if len(terminator.successors) > 1:
            raise RuntimeError("entry block has multiple successors, unsupported")
        successor = terminator.successors[0] if terminator.successors else None

        for instruction in current_block.instructions:
            match instruction.opcode:
                case pyqir.Opcode.CALL:
                    assert isinstance(instruction, pyqir.Call)
                    ops = map_callable_to_operations(state, instruction)
                    operations.extend(ops)
                case pyqir.Opcode.RET:
                    # ignore return instructions
                    pass
                case _:
                    raise RuntimeError(
                        f"unsupported instruction opcode: {instruction.opcode}"
                    )

        current_block = successor

    # Convert operations into a grid and fill metadata
    component_grid = operation_list_to_grid(operations, num_qubits, loop_detection)

    circuit = Circuit(qubits=state.into_qubits(), component_grid=component_grid)
    return circuit


def register_to_dict(reg: Register) -> Dict[str, Any]:
    d: Dict[str, Any] = {"qubit": reg.qubit}
    if reg.result is not None:
        d["result"] = reg.result
    return d


def component_column_to_dict(col: ComponentColumn) -> Dict[str, Any]:
    return {"components": [operation_to_dict(op) for op in col.components]}


def operation_to_dict(op: Component) -> Dict[str, Any]:
    # produce an object with a `kind` tag plus the fields used by the Rust schema
    if isinstance(op, Measurement):
        d: Dict[str, Any] = {"kind": "measurement", "gate": op.gate}
        if op.args:
            d["args"] = list(op.args)
        if op.children:
            d["children"] = [component_column_to_dict(c) for c in op.children]
        # qubits and results are required in the Rust schema (not skipped)
        d["qubits"] = [register_to_dict(r) for r in op.qubits]
        d["results"] = [register_to_dict(r) for r in op.results]
        return d
    if isinstance(op, Unitary):
        d: Dict[str, Any] = {"kind": "unitary", "gate": op.gate}
        if op.args:
            d["args"] = list(op.args)
        if op.children:
            d["children"] = [component_column_to_dict(c) for c in op.children]
        # targets are required
        d["targets"] = [register_to_dict(r) for r in op.targets]
        if op.controls:
            d["controls"] = [register_to_dict(r) for r in op.controls]
        if op.is_adjoint:
            d["isAdjoint"] = True
        return d
    if isinstance(op, Ket):
        d: Dict[str, Any] = {"kind": "ket", "gate": op.gate}
        if op.args:
            d["args"] = list(op.args)
        if op.children:
            d["children"] = [component_column_to_dict(c) for c in op.children]
        d["targets"] = [register_to_dict(r) for r in op.targets]
        return d
    # Fallback for unknown component types
    return {"kind": "unknown"}


def qubit_to_dict(q: Qubit) -> Dict[str, Any]:
    return {"id": q.id, "numResults": q.num_results}


def circuit_to_dict(circ: Circuit) -> Dict[str, Any]:
    return {
        "qubits": [qubit_to_dict(q) for q in circ.qubits],
        "componentGrid": [component_column_to_dict(col) for col in circ.component_grid],
    }
