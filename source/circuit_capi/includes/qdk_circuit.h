// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// Experimental interface; no stable ABI compatibility promise.


#ifndef QDK_CIRCUIT_H
#define QDK_CIRCUIT_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#define QDK_ANGLE_COPY 0

#define QDK_ANGLE_ADD 1

#define QDK_ANGLE_SUB 2

#define QDK_ANGLE_MUL 3

#define QDK_ANGLE_DIV 4

#define QDK_ANGLE_NEG 5

#define QDK_BODY_FOR 6

#define QDK_BODY_BREAK 7

#define QDK_BODY_CONTINUE 8

#define QDK_BODY_IF 1

#define QDK_BODY_WHILE 2

#define QDK_BODY_MOMENT 3

#define QDK_BODY_BOX 4

#define QDK_BODY_SWITCH 5

#define QDK_LOOP_PARAMETER 1

#define QDK_LOOP_RUNTIME 2

#define QDK_JUMP_BREAK 1

#define QDK_JUMP_CONTINUE 2

#define QDK_VALUE_BOOL 1

#define QDK_VALUE_INT 2

#define QDK_VALUE_FLOAT 3

#define QDK_EXPR_LITERAL 1

#define QDK_EXPR_CLBIT 2

#define QDK_EXPR_VARIABLE 3

#define QDK_EXPR_NOT 4

#define QDK_EXPR_BINARY 5

#define QDK_EXPR_REGISTER_EQUAL 6

#define QDK_BINARY_EQ 1

#define QDK_BINARY_NE 2

#define QDK_BINARY_LT 3

#define QDK_BINARY_LE 4

#define QDK_BINARY_GT 5

#define QDK_BINARY_GE 6

#define QDK_BINARY_AND 7

#define QDK_BINARY_OR 8

#define QDK_BINARY_ADD 9

#define QDK_BINARY_SUB 10

#define QDK_BINARY_MUL 11

#define QDK_FORMAT_GRAPH_BINARY 8

#define QDK_FORMAT_GRAPH_SEXPR 10

#define QDK_PAYLOAD_NONE 0

#define QDK_PAYLOAD_NUMBER 1

#define QDK_PAYLOAD_CLASSICAL_BIT 2

#define QDK_PAYLOAD_ANGLE 3

#define QDK_GRAPH_REGION 1

#define QDK_GRAPH_NODE 2

#define QDK_GRAPH_PORT 3

#define QDK_GRAPH_OPERATION 1

#define QDK_GRAPH_SYMBOL_PACKAGE 2

#define QDK_GRAPH_SYMBOL_ITEM 3

#define QDK_GRAPH_EFFECT_RESOURCE 4

#define QDK_GRAPH_INPUTS 1

#define QDK_GRAPH_OUTPUTS 2

#define QDK_GRAPH_ARGUMENTS 3

#define QDK_GRAPH_RESULTS 4

#define QDK_EXIT_RETURN 1

#define QDK_EXIT_ABORT 2

#define QDK_EXIT_BRANCH 3

#define QDK_EXIT_BACKEDGE 4

#define QDK_REGION_MODULE 1

#define QDK_REGION_LAMBDA 2

#define QDK_REGION_GAMMA 3

#define QDK_REGION_THETA 4

#define QDK_REGION_RECURSION 5

#define QDK_REGION_HANDLER 6

#define QDK_REGION_STAGE 7

#define QDK_REGION_RESIDUAL_CFG 8

#define QDK_PORT_INPUT 1

#define QDK_PORT_OUTPUT 2

#define QDK_PORT_VALUE 1

#define QDK_PORT_CAPABILITY 2

#define QDK_PORT_HARD_EFFECT 3

#define QDK_PORT_CONTROL_EXIT 4

#define QDK_STAGE_GENERATION 1

#define QDK_STAGE_RESIDUAL 2

#define QDK_USAGE_COPYABLE 1

#define QDK_USAGE_AFFINE 2

#define QDK_USAGE_LINEAR 3

#define QDK_EDGE_VALUE 1

#define QDK_EDGE_CAPABILITY 2

#define QDK_EDGE_HARD_EFFECT 3

#define QDK_EDGE_CONTROL_EXIT 4

#define QDK_EFFECT_MEMORY 1

#define QDK_EFFECT_QUANTUM 2

#define QDK_EFFECT_RESULT 3

#define QDK_EFFECT_ALLOCATION 4

#define QDK_EFFECT_OUTPUT 5

#define QDK_EFFECT_ABORT 6

#define QDK_EFFECT_HOST 7

#define QDK_EFFECT_NOISE 8

#define QDK_EFFECT_UNKNOWN 9

#define QDK_EFFECT_READ 1

#define QDK_EFFECT_WRITE 2

#define QDK_EFFECT_IRREVERSIBLE 3

#define QDK_EFFECT_ALLOCATE 4

#define QDK_EFFECT_RELEASE 5

#define QDK_EFFECT_EMIT 6

#define QDK_EFFECT_ABORT_MODE 7

#define QDK_EFFECT_UNKNOWN_MODE 8

#define QDK_INTERFACE_ARGUMENT 1

#define QDK_INTERFACE_RESULT 2

#define QDK_INTERFACE_EXIT 3

#define QDK_INTERFACE_CAPTURE 4

#define QDK_INTERFACE_CARRIED 5

#define QDK_ORIGIN_NONE 0

#define QDK_ORIGIN_INSTRUCTION 1

#define QDK_ORIGIN_QUBIT 2

#define QDK_ORIGIN_CLBIT 3

#define QDK_ORIGIN_GLOBAL_PHASE 4

#define QDK_ORIGIN_FIR 5

#define QDK_FIR_PACKAGE 1

#define QDK_FIR_ITEM 2

#define QDK_FIR_BLOCK 3

#define QDK_FIR_EXPRESSION 4

#define QDK_FIR_PATTERN 5

#define QDK_FIR_STATEMENT 6

#define QDK_TYPE_UNIT 1

#define QDK_TYPE_BOOL 2

#define QDK_TYPE_INT 3

#define QDK_TYPE_DOUBLE 4

#define QDK_TYPE_STRING 5

#define QDK_TYPE_RESULT 6

#define QDK_TYPE_QUBIT 7

#define QDK_TYPE_UNKNOWN 8

#define QDK_TYPE_ARRAY 9

#define QDK_TYPE_TUPLE 10

#define QDK_TYPE_LOCATION 11

#define QDK_GRAPH_ANGLE_LITERAL 1

#define QDK_GRAPH_ANGLE_PARAMETER 2

#define QDK_GRAPH_ANGLE_EXPRESSION 3

#define QDK_GATE_I 1

#define QDK_GATE_X 2

#define QDK_GATE_Y 3

#define QDK_GATE_Z 4

#define QDK_GATE_H 5

#define QDK_GATE_S 6

#define QDK_GATE_SDG 7

#define QDK_GATE_SX 8

#define QDK_GATE_SXDG 9

#define QDK_GATE_T 10

#define QDK_GATE_TDG 11

#define QDK_GATE_CX 12

#define QDK_GATE_CY 13

#define QDK_GATE_CZ 14

#define QDK_GATE_SWAP 15

#define QDK_GATE_CCX 16

#define QDK_GATE_CSWAP 17

#define QDK_GATE_RX 18

#define QDK_GATE_RY 19

#define QDK_GATE_RZ 20

#define QDK_GATE_RXX 21

#define QDK_GATE_RYY 22

#define QDK_GATE_RZZ 23

#define QDK_GATE_CRX 24

#define QDK_GATE_CRY 25

#define QDK_GATE_CRZ 26

#define QDK_GATE_MEASURE_X 27

#define QDK_GATE_MEASURE_Y 28

#define QDK_GATE_MEASURE_Z 29

#define QDK_GATE_PREPARE_X 30

#define QDK_GATE_PREPARE_Y 31

#define QDK_GATE_PREPARE_Z 32

#define QDK_OPERATION_STANDARD 1

#define QDK_OPERATION_RESET 2

#define QDK_OPERATION_BARRIER 3

#define QDK_OPERATION_DELAY 4

#define QDK_OPERATION_CUSTOM 5

#define QDK_OPERATION_UNITARY 6

#define QDK_OPERATION_CONTROL_FLOW 7

#define QDK_ANGLE_LITERAL 1

#define QDK_ANGLE_PARAMETER 2

#define QDK_ANGLE_EXPRESSION 3

#define QDK_TIME_DT 1

#define QDK_TIME_NS 2

#define QDK_TIME_US 3

#define QDK_TIME_MS 4

#define QDK_TIME_S 5

#define QDK_FORMAT_CIRCUIT_BINARY 7

#define QDK_FORMAT_CIRCUIT_SEXPR 9

#define QDK_FORMAT_QASM 2

#define QDK_FORMAT_QIR 3

#define QDK_FORMAT_TEXT 4

#define QDK_FORMAT_DIAGRAM_JSON 5

#define QDK_PROFILE_BASE 1

#define QDK_PROFILE_ADAPTIVE_RI 2

#define QDK_PROFILE_ADAPTIVE_RIF 3

#define QDK_PROFILE_ADAPTIVE 4

#define QDK_PROFILE_UNRESTRICTED 5

#define QDK_PASS_REWRITE 1

#define QDK_PASS_EXPAND 2

#define QDK_DECISION_KEEP 0

#define QDK_DECISION_REPLACE 1

#define QDK_DECISION_REMOVE 2

#define QDK_DECISION_EXPAND 3

#define QDK_PASS_UNCHANGED 1

#define QDK_PASS_COMMITTED 2

#define QDK_PASS_BUDGET_EXHAUSTED 3

#define QDK_PASS_OSCILLATION 4

#define QDK_ANCHOR_CIRCUIT_BUILT 1

#define QDK_ANCHOR_GRAPH_LOWERED 2

#define QDK_ANCHOR_GRAPH_REWRITTEN 3

#define QDK_ANCHOR_CIRCUIT_RAISED 4

#define QDK_OPTIMIZATION_NONE 0

#define QDK_OPTIMIZATION_DEFAULT 1

#define QDK_OPTIMIZATION_AGGRESSIVE 2

#define QDK_REGISTER_QUANTUM 1

#define QDK_REGISTER_CLASSICAL 2

/**
 * A circuit-owned compile-time angle expression, not a runtime value.
 */
typedef struct QdkAngleExpression QdkAngleExpression;

typedef struct QdkBlock QdkBlock;

/**
 * A checked construction handle. Finish/cancel before release; closed children
 * may remain readable but still prevent releasing their parents.
 */
typedef struct QdkBuilder QdkBuilder;

/**
 * Opaque owning circuit. Release children before releasing their circuit.
 */
typedef struct QdkCircuit QdkCircuit;

/**
 * Callback-duration borrowed scope. Only `qdk_callback_offer` operates on it.
 */
typedef struct QdkCircuitCallbackScope QdkCircuitCallbackScope;

/**
 * Owned diagnostic, released only with `qdk_diagnostic_free`.
 */
typedef struct QdkDiagnostic QdkDiagnostic;

typedef struct QdkExpression QdkExpression;

/**
 * Owning read-only graph. Transformations return independent objects.
 */
typedef struct QdkGraph QdkGraph;

/**
 * Independent named graph angle, not a circuit-owned parameter expression.
 */
typedef struct QdkGraphAngle QdkGraphAngle;

/**
 * Owned single-use native graph builder. Free may discard unfinished work.
 */
typedef struct QdkGraphBuilder QdkGraphBuilder;

/**
 * Independent copied verifier results.
 */
typedef struct QdkGraphDiagnostics QdkGraphDiagnostics;

/**
 * Independent positional origins returned by graph raising.
 */
typedef struct QdkGraphOrigins QdkGraphOrigins;

/**
 * Owning callback registration; invoke synchronously with `qdk_graph_pass_run`.
 */
typedef struct QdkGraphPass QdkGraphPass;

/**
 * Independent copied record. IDs remain local to the source graph.
 */
typedef struct QdkGraphRecord QdkGraphRecord;

/**
 * Independent owned graph type. Nested elements are copied, not borrowed.
 */
typedef struct QdkGraphType QdkGraphType;

/**
 * An independent instruction copy. It can outlive its source circuit.
 */
typedef struct QdkInstruction QdkInstruction;

/**
 * A parameter reference registered with its owning circuit.
 */
typedef struct QdkParameter QdkParameter;

/**
 * Independent owned decision. Returning it from a callback transfers ownership.
 */
typedef struct QdkPassDecision QdkPassDecision;

/**
 * Independent copied native outcome and foreign callback count.
 */
typedef struct QdkPassResult QdkPassResult;

/**
 * Owning native pass pipeline. Registration and execution are non-reentrant.
 */
typedef struct QdkPipeline QdkPipeline;

/**
 * Independent native pipeline output and report.
 */
typedef struct QdkPipelineResult QdkPipelineResult;

typedef struct QdkVariable QdkVariable;

typedef uint32_t QdkStatus;

/**
 * Owned bytes, not NUL-terminated. Release only with `qdk_bytes_free`.
 */
typedef struct QdkBytes {
  uint8_t *data;
  size_t len;
} QdkBytes;

/**
 * Input angle: a literal or a live parameter/expression from the receiving circuit.
 */
typedef struct QdkAngle {
  uint32_t kind;
  double literal;
  const struct QdkParameter *parameter;
  const struct QdkAngleExpression *expression;
} QdkAngle;

typedef struct QdkAngleNode {
  /**
   * `QDK_ANGLE_LITERAL`, `QDK_ANGLE_PARAMETER` or `QDK_ANGLE_EXPRESSION`.
   */
  uint32_t kind;
  /**
   * Arithmetic tag for expression nodes.
   */
  uint32_t operation;
  double literal;
  uint32_t parameter_id;
  /**
   * Earlier node positions in this copied, root-last array.
   */
  uint32_t left;
  uint32_t right;
} QdkAngleNode;

typedef struct QdkParameterBinding {
  const struct QdkParameter *parameter;
  double value;
} QdkParameterBinding;

typedef struct QdkControlInfo {
  uint32_t kind;
  uint32_t loop_index_kind;
  uint32_t loop_index_id;
  size_t value_count;
  size_t case_count;
  uint8_t has_default;
  uint8_t has_label;
  uint8_t has_duration;
  double duration;
} QdkControlInfo;

typedef struct QdkLiteral {
  uint32_t kind;
  uint8_t boolean;
  int64_t integer;
  double real;
} QdkLiteral;

typedef struct QdkBodyOptions {
  uint32_t kind;
  const struct QdkExpression *condition;
  /**
   * NULL means no label; a non-NULL empty input means an empty label.
   */
  const uint8_t *label;
  size_t label_len;
} QdkBodyOptions;

typedef struct QdkForOptions {
  const struct QdkLiteral *values;
  size_t value_count;
  /**
   * NULL selects a generated unused name.
   */
  const uint8_t *name;
  size_t name_len;
  uint32_t index_kind;
} QdkForOptions;

/**
 * Exactly one owning index handle is populated, according to kind. It is
 * independent of the builder but still registered with the circuit.
 */
typedef struct QdkLoopIndex {
  uint32_t kind;
  struct QdkParameter *parameter;
  struct QdkVariable *variable;
} QdkLoopIndex;

typedef struct QdkComplex {
  double real;
  double imag;
} QdkComplex;

/**
 * All pointed-to input data is borrowed only for the append call.
 * Initialize unused fields to zero. Custom/matrix operations are storage-only.
 */
typedef struct QdkOperation {
  uint32_t kind;
  uint32_t gate;
  const uint32_t *qubits;
  size_t qubit_count;
  const uint32_t *clbits;
  size_t clbit_count;
  const struct QdkAngle *params;
  size_t param_count;
  const uint8_t *name;
  size_t name_len;
  const struct QdkComplex *matrix;
  size_t matrix_len;
  double duration;
  uint32_t time_unit;
} QdkOperation;

typedef struct QdkVariableInfo {
  uint32_t id;
  uint32_t value_type;
  struct QdkBytes name;
} QdkVariableInfo;

/**
 * Initialize unused fields to zero. `REGISTER_EQUAL` uses `register_name` and
 * `register_value`; it preserves the facade's per-bit register condition policy.
 */
typedef struct QdkExpressionInput {
  uint32_t kind;
  struct QdkLiteral literal;
  uint32_t clbit;
  const struct QdkVariable *variable;
  uint32_t operation;
  const struct QdkExpression *left;
  const struct QdkExpression *right;
  const uint8_t *register_name;
  size_t register_name_len;
  int64_t register_value;
} QdkExpressionInput;

/**
 * Copied node fields; operand and variable IDs remain local to the circuit.
 * Use `qdk_circuit_variable_at` with the same circuit to recover variable metadata.
 */
typedef struct QdkExpressionInfo {
  uint32_t kind;
  uint32_t value_type;
  struct QdkLiteral literal;
  uint32_t clbit;
  uint32_t variable_id;
  uint32_t operation;
  uint32_t left_id;
  uint32_t right_id;
} QdkExpressionInfo;

typedef struct QdkGraphInfo {
  size_t region_count;
  size_t node_count;
  size_t port_count;
  size_t edge_count;
  uint64_t revision;
} QdkGraphInfo;

/**
 * Length-delimited borrowed UTF-8 input; NULL is accepted when len is zero.
 */
typedef struct QdkString {
  const uint8_t *data;
  size_t len;
} QdkString;

typedef struct QdkGraphParameterBinding {
  struct QdkString name;
  double value;
} QdkGraphParameterBinding;

typedef struct QdkGraphEdge {
  uint32_t source;
  uint32_t target;
  uint32_t kind;
} QdkGraphEdge;

typedef struct QdkOptionalId {
  /**
   * Zero for absent, one for present (including ID zero).
   */
  uint32_t present;
  uint32_t value;
} QdkOptionalId;

typedef struct QdkGraphOrigin {
  /**
   * `QDK_ORIGIN_NONE` indicates no origin.
   */
  uint32_t kind;
  uint32_t index;
  /**
   * Used only for `QDK_ORIGIN_FIR`.
   */
  uint32_t package;
  uint32_t fir_kind;
} QdkGraphOrigin;

typedef struct QdkGraphEffectInput {
  uint32_t domain;
  uint32_t mode;
  uint32_t observable;
  /**
   * Zero for unknown, one for a named resource (possibly empty).
   */
  uint32_t has_resource;
  struct QdkString resource;
} QdkGraphEffectInput;

typedef struct QdkGraphPortInput {
  uint32_t node;
  uint32_t direction;
  uint32_t kind;
  const struct QdkGraphType *value_type;
  uint32_t stage;
  uint32_t usage;
} QdkGraphPortInput;

typedef struct QdkGraphPayload {
  uint32_t kind;
  double number;
  uint32_t classical_bit;
  /**
   * Only read for `QDK_PAYLOAD_ANGLE`; the angle remains caller-owned.
   */
  const struct QdkGraphAngle *angle;
  struct QdkGraphOrigin origin;
} QdkGraphPayload;

typedef struct QdkGraphBinding {
  uint32_t outer;
  uint32_t inner;
  uint32_t role;
} QdkGraphBinding;

typedef struct QdkGraphRegionInfo {
  uint32_t id;
  struct QdkOptionalId parent;
  uint32_t kind;
  struct QdkOptionalId owner;
  size_t argument_count;
  size_t result_count;
  size_t exit_count;
  size_t binding_count;
} QdkGraphRegionInfo;

typedef struct QdkGraphNodeInfo {
  uint32_t id;
  uint32_t region;
  size_t input_count;
  size_t output_count;
  size_t effect_count;
  uint32_t has_symbol;
  uint32_t payload_kind;
  double number;
  uint32_t classical_bit;
  struct QdkGraphOrigin origin;
} QdkGraphNodeInfo;

typedef struct QdkGraphPortInfo {
  uint32_t id;
  uint32_t node;
  uint32_t region;
  uint32_t direction;
  uint32_t kind;
  uint32_t stage;
  uint32_t usage;
} QdkGraphPortInfo;

typedef struct QdkGraphEffectInfo {
  uint32_t domain;
  uint32_t mode;
  uint32_t observable;
  uint32_t has_resource;
} QdkGraphEffectInfo;

typedef struct QdkGraphExit {
  uint32_t kind;
  uint32_t port;
} QdkGraphExit;

typedef struct QdkGraphTypeInfo {
  uint32_t kind;
  size_t child_count;
} QdkGraphTypeInfo;

typedef struct QdkGraphAngleInput {
  uint32_t kind;
  double literal;
  struct QdkString name;
  /**
   * Existing `QDK_ANGLE` arithmetic tags. COPY and NEG ignore right.
   */
  uint32_t operation;
  const struct QdkGraphAngle *left;
  const struct QdkGraphAngle *right;
} QdkGraphAngleInput;

typedef struct QdkGraphAngleInfo {
  uint32_t kind;
  double literal;
  uint32_t operation;
  size_t child_count;
  size_t parameter_count;
} QdkGraphAngleInfo;

typedef struct QdkCircuitInfo {
  uint32_t qubits;
  uint32_t clbits;
  size_t instructions;
  size_t parameters;
  size_t blocks;
  size_t quantum_registers;
  size_t classical_registers;
  double global_phase;
} QdkCircuitInfo;

typedef struct QdkInstructionInfo {
  uint32_t kind;
  /**
   * Zero for a non-standard operation.
   */
  uint32_t gate;
  uint32_t estimator_id;
  size_t qubit_count;
  size_t clbit_count;
  size_t param_count;
  size_t block_count;
  size_t matrix_len;
  /**
   * Duration and unit apply only to delay operations.
   */
  double duration;
  uint32_t time_unit;
} QdkInstructionInfo;

/**
 * A copied angle value. IDs are local to the original circuit, not global
 * identities; use `qdk_instruction_parameter` or
 * `qdk_instruction_angle_expression` to obtain a checked live handle.
 */
typedef struct QdkStoredAngle {
  uint32_t kind;
  double literal;
  uint32_t id;
} QdkStoredAngle;

typedef struct QdkGateInfo {
  /**
   * Owned UTF-8 bytes; release with `qdk_bytes_free`.
   */
  struct QdkBytes name;
  uint32_t qubit_arity;
  uint32_t clbit_arity;
  uint32_t param_arity;
  uint32_t estimator_id;
} QdkGateInfo;

/**
 * On failure, `failed_index` equals the number of successfully appended gates.
 * On success it equals accepted, the total gate count.
 */
typedef struct QdkBatchProgress {
  size_t accepted;
  size_t failed_index;
} QdkBatchProgress;

typedef struct QdkGateOperands {
  uint32_t gate;
  const uint32_t *qubits;
  size_t qubit_count;
} QdkGateOperands;

/**
 * Copied registration input. A zero `use_budget` selects the native default.
 */
typedef struct QdkGraphPassOptions {
  struct QdkString name;
  uint32_t kind;
  const struct QdkString *operations;
  size_t operation_count;
  uint32_t use_budget;
  size_t budget;
} QdkGraphPassOptions;

/**
 * Read-only callback-duration data. Copy strings before returning.
 */
typedef struct QdkPassMatch {
  uint64_t node;
  struct QdkString operation;
  size_t operands;
} QdkPassMatch;

/**
 * Return an owned decision (NULL means keep) and optional owned diagnostic.
 * Both outputs transfer to the library on return, even on failure. Never
 * unwind a foreign exception/panic across this ABI.
 */
typedef QdkStatus (*QdkGraphCallback)(uintptr_t context,
                                      const struct QdkPassMatch *matched,
                                      struct QdkPassDecision **decision,
                                      struct QdkDiagnostic **error);

/**
 * Runs exactly once after successful registration and the last use of context.
 * Must not unwind. The integer cookie is not a retained foreign-memory pointer.
 */
typedef void (*QdkContextDestructor)(uintptr_t context);

typedef struct QdkPassResultInfo {
  uint32_t outcome;
  size_t applications;
  size_t pending;
  size_t callback_count;
  size_t rule_count;
} QdkPassResultInfo;

/**
 * The working circuit is borrowed for this call. Its dependent handles are
 * callback-scoped and disposed child-first on return. Independent closed
 * clones may escape. No foreign panic/exception may cross this ABI.
 */
typedef QdkStatus (*QdkCircuitCallback)(uintptr_t context,
                                        struct QdkCircuitCallbackScope *scope,
                                        struct QdkCircuit *working,
                                        struct QdkDiagnostic **error);

typedef struct QdkPassEntryInfo {
  uint32_t anchor;
  uint32_t built_in;
  /**
   * Owned UTF-8 bytes.
   */
  struct QdkBytes name;
} QdkPassEntryInfo;

typedef struct QdkPipelineResultInfo {
  size_t executed_count;
  size_t changed_count;
  size_t diagnostic_count;
  size_t analyses_computed;
  /**
   * Counts circuit callbacks and per-match graph callbacks.
   */
  size_t callback_count;
} QdkPipelineResultInfo;

typedef struct QdkRegisterInput {
  uint32_t kind;
  const uint8_t *name;
  size_t name_len;
  uint32_t size;
} QdkRegisterInput;

typedef struct QdkRegisterInfo {
  /**
   * Owned UTF-8 bytes; release with `qdk_bytes_free`.
   */
  struct QdkBytes name;
  uint32_t start;
  uint32_t size;
} QdkRegisterInfo;

#define QDK_OK 0

#define QDK_INVALID_ARGUMENT 1

#define QDK_IN_USE 2

#define QDK_UNFINISHED 3

#define QDK_POISONED 4

#define QDK_INTERNAL_ERROR 5

#define QDK_ACTIVE 6

#define QDK_BORROWED 7

#define QDK_BUFFER_TOO_SMALL 8

#define QDK_WRONG_OWNER 9

#define QDK_WRONG_VARIANT 10

#define QDK_INVALID_STATE 11

#define QDK_CALLBACK_ERROR 12

#define QDK_ALREADY_SET 13

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Creates an owning circuit. On failure, `out` is NULL.
 *
 * Every `error` output is optional; when non-NULL it receives an owned
 * diagnostic on failure and NULL on success. Release its previous value
 * before reusing the output slot.
 *
 * # Safety
 * Non-NULL pointers must address writable, aligned output slots. Slots must
 * not alias one another or any input object.
 */
QdkStatus qdk_circuit_new(uint32_t qubits,
                          uint32_t clbits,
                          struct QdkCircuit **out,
                          struct QdkDiagnostic **error);

/**
 * Copies a closed circuit, including registers and symbolic state.
 *
 * # Safety
 * `circuit` must be a live library handle. Output slots obey
 * `qdk_circuit_new`'s contract. No concurrent access is allowed.
 */
QdkStatus qdk_circuit_clone(const struct QdkCircuit *circuit,
                            struct QdkCircuit **out,
                            struct QdkDiagnostic **error);

/**
 * Releases an owning circuit and sets its slot to NULL. NULL is a no-op.
 * `InUse`, `Active` or `Borrowed` leaves the owning slot and circuit intact.
 * Poisoned owners remain releasable once all children have been released.
 *
 * # Safety
 * A non-NULL slot must contain NULL or a live owning library handle, used
 * exclusively. Previously released pointers must never be passed again.
 */
QdkStatus qdk_circuit_free(struct QdkCircuit **circuit, struct QdkDiagnostic **error);

/**
 * Copies a diagnostic into owned UTF-8 bytes.
 *
 * # Safety
 * `diagnostic` is live; `out` and optional `error` are disjoint writable
 * slots. No concurrent access or freeing of `diagnostic` is allowed.
 */
QdkStatus qdk_diagnostic_message(const struct QdkDiagnostic *diagnostic,
                                 struct QdkBytes *out,
                                 struct QdkDiagnostic **error);

/**
 * Creates an owned UTF-8 diagnostic, including for a foreign callback failure.
 * NULL data is valid only for an empty message.
 *
 * # Safety
 * The input covers `len` initialized bytes for this call. Output slots are
 * valid, writable and disjoint from the input.
 */
QdkStatus qdk_diagnostic_new(const uint8_t *data,
                             size_t len,
                             struct QdkDiagnostic **out,
                             struct QdkDiagnostic **error);

/**
 * Copies diagnostic UTF-8 bytes without a terminator. Sets `needed` to the
 * complete byte length. Insufficient capacity returns `QDK_BUFFER_TOO_SMALL`
 * without writing any bytes; NULL data with zero capacity is a size query.
 *
 * # Safety
 * The diagnostic is live. Output pointers cover the declared capacity and
 * are disjoint from the diagnostic and one another.
 */
QdkStatus qdk_diagnostic_copy(const struct QdkDiagnostic *diagnostic,
                              uint8_t *data,
                              size_t capacity,
                              size_t *needed,
                              struct QdkDiagnostic **error);

/**
 * Releases a diagnostic and sets its slot to NULL. NULL is a no-op.
 *
 * # Safety
 * A non-NULL slot contains NULL or a live diagnostic allocated by this library.
 */
void qdk_diagnostic_free(struct QdkDiagnostic **diagnostic);

/**
 * Releases owned bytes and clears the descriptor. NULL is a no-op.
 *
 * # Safety
 * The descriptor must be unchanged since a successful library output, not
 * already released, and exclusively owned by the caller.
 */
void qdk_bytes_free(struct QdkBytes *bytes);

/**
 * Builds existing compile-time arithmetic. COPY and NEG ignore right; binary
 * operators require it. Every referenced handle must belong to this circuit.
 * # Safety
 * Circuit and non-NULL input handles are live; outputs are disjoint writable slots.
 */
QdkStatus qdk_circuit_angle(struct QdkCircuit *circuit,
                            uint32_t operation,
                            const struct QdkAngle *left,
                            const struct QdkAngle *right,
                            struct QdkAngleExpression **out,
                            struct QdkDiagnostic **error);

/**
 * Copies reachable expression nodes in root-last order, using earlier array
 * positions for child references. Parameter IDs remain local to the circuit.
 * # Safety
 * Expression is live; output buffers cover capacity and are disjoint.
 */
QdkStatus qdk_angle_expression_nodes(const struct QdkAngleExpression *expression,
                                     struct QdkAngleNode *data,
                                     size_t capacity,
                                     size_t *needed,
                                     struct QdkDiagnostic **error);

/**
 * Releases an expression handle, including during poisoned-owner teardown.
 * # Safety
 * Slot contains NULL or a live owning expression and is exclusively accessible.
 */
QdkStatus qdk_angle_expression_free(struct QdkAngleExpression **expression,
                                    struct QdkDiagnostic **error);

/**
 * Copies referenced parameter IDs in declaration order; unused declarations
 * are omitted. IDs are local to this circuit.
 * # Safety
 * Circuit is live; output buffers cover capacity and are disjoint.
 */
QdkStatus qdk_circuit_parameters(const struct QdkCircuit *circuit,
                                 uint32_t *data,
                                 size_t capacity,
                                 size_t *needed,
                                 struct QdkDiagnostic **error);

/**
 * Obtains a dependent handle for an existing owner-local parameter ID.
 * # Safety
 * Circuit is live; output slots are writable and disjoint.
 */
QdkStatus qdk_circuit_parameter_at(struct QdkCircuit *circuit,
                                   uint32_t id,
                                   struct QdkParameter **out,
                                   struct QdkDiagnostic **error);

/**
 * Copies a parameter's name as owned UTF-8 bytes.
 * # Safety
 * Parameter is live; output slots are writable and disjoint.
 */
QdkStatus qdk_parameter_name(const struct QdkParameter *parameter,
                             struct QdkBytes *out,
                             struct QdkDiagnostic **error);

/**
 * Returns a fully bound new circuit without modifying the input. Requires a
 * closed circuit and values for all referenced non-loop parameters.
 * # Safety
 * Circuit, bindings and parameter handles are live; outputs are disjoint.
 */
QdkStatus qdk_circuit_bind(const struct QdkCircuit *circuit,
                           const struct QdkParameterBinding *bindings,
                           size_t count,
                           struct QdkCircuit **out,
                           struct QdkDiagnostic **error);

/**
 * Copies control-flow metadata without borrowing the native program.
 * # Safety
 * Instruction is live; outputs are writable and disjoint.
 */
QdkStatus qdk_instruction_control(const struct QdkInstruction *instruction,
                                  struct QdkControlInfo *out,
                                  struct QdkDiagnostic **error);

/**
 * Copies for-loop values or the values of one switch case. For-loop queries
 * require `case_index` zero; default switch bodies have no case-value list.
 * # Safety
 * Instruction is live; output buffers cover capacity and are disjoint.
 */
QdkStatus qdk_instruction_values(const struct QdkInstruction *instruction,
                                 size_t case_index,
                                 struct QdkLiteral *data,
                                 size_t capacity,
                                 size_t *needed,
                                 struct QdkDiagnostic **error);

/**
 * Copies a box label. No label yields empty bytes; `has_label` in control
 * metadata distinguishes it from an explicitly empty label.
 * # Safety
 * Instruction is live; output slots are writable and disjoint.
 */
QdkStatus qdk_instruction_label(const struct QdkInstruction *instruction,
                                struct QdkBytes *out,
                                struct QdkDiagnostic **error);

/**
 * Obtains a registered block reference from an instruction's ordered branches.
 * The record must come from the supplied live circuit.
 * # Safety
 * Record and circuit are live; output slots are writable and disjoint.
 */
QdkStatus qdk_instruction_block(const struct QdkInstruction *instruction,
                                struct QdkCircuit *circuit,
                                size_t index,
                                struct QdkBlock **out,
                                struct QdkDiagnostic **error);

/**
 * Obtains a read-only block reference from a body or case builder.
 * # Safety
 * Builder is live; output slots are writable and disjoint.
 */
QdkStatus qdk_builder_block(const struct QdkBuilder *builder,
                            struct QdkBlock **out,
                            struct QdkDiagnostic **error);

/**
 * Copies a condition or switch selector as a registered expression handle.
 * # Safety
 * Record/circuit are live; writable outputs are disjoint.
 */
QdkStatus qdk_instruction_condition(const struct QdkInstruction *instruction,
                                    struct QdkCircuit *circuit,
                                    struct QdkExpression **out,
                                    struct QdkDiagnostic **error);

/**
 * Returns the number of instructions in a registered block.
 * # Safety
 * Block is live; output slots are writable and disjoint.
 */
QdkStatus qdk_block_len(const struct QdkBlock *block, size_t *out, struct QdkDiagnostic **error);

/**
 * Copies one instruction from a registered block.
 * # Safety
 * Block is live; output slots are writable and disjoint.
 */
QdkStatus qdk_block_instruction(const struct QdkBlock *block,
                                size_t index,
                                struct QdkInstruction **out,
                                struct QdkDiagnostic **error);

/**
 * Releases a block reference, including during poisoned-owner teardown.
 * # Safety
 * Slot contains NULL or a live owning block reference.
 */
QdkStatus qdk_block_free(struct QdkBlock **block, struct QdkDiagnostic **error);

/**
 * Begins if/while/moment/box/switch construction. A nested body must name its
 * currently open parent; NULL parent begins a root scope. Appends use this
 * scope until it finishes or cancels.
 * # Safety
 * Circuit, optional parent/condition and input bytes are live; outputs are disjoint.
 */
QdkStatus qdk_circuit_begin(struct QdkCircuit *circuit,
                            const struct QdkBuilder *parent,
                            const struct QdkBodyOptions *options,
                            struct QdkBuilder **out,
                            struct QdkDiagnostic **error);

/**
 * Opens the alternative branch of a successfully completed if body.
 * # Safety
 * The conditional and its parent circuit are live; outputs are disjoint.
 */
QdkStatus qdk_builder_otherwise(const struct QdkBuilder *conditional,
                                struct QdkBuilder **out,
                                struct QdkDiagnostic **error);

/**
 * Opens a case of the current switch. `default_case` is 0 for a nonempty value
 * list or 1 for the default (which must have no values).
 * # Safety
 * Switch and input values are live; writable outputs are disjoint.
 */
QdkStatus qdk_switch_case(const struct QdkBuilder *switch_,
                          const struct QdkLiteral *values,
                          size_t count,
                          uint8_t default_case,
                          struct QdkBuilder **out,
                          struct QdkDiagnostic **error);

/**
 * Begins a compile-time-parameter or runtime-variable for loop. The index
 * handle is returned separately and must also be released by the caller.
 * # Safety
 * Circuit, parent and options/values are live; output slots are writable and disjoint.
 */
QdkStatus qdk_circuit_for_begin(struct QdkCircuit *circuit,
                                const struct QdkBuilder *parent,
                                const struct QdkForOptions *options,
                                struct QdkBuilder **out,
                                struct QdkLoopIndex *index,
                                struct QdkDiagnostic **error);

/**
 * Finishes the current body. Closed child handles may remain allocated.
 * # Safety
 * Builder is live and exclusively accessible; error output is disjoint.
 */
QdkStatus qdk_builder_finish(struct QdkBuilder *builder, struct QdkDiagnostic **error);

/**
 * Cancels the current body without publishing its construct.
 * # Safety
 * Builder is live and exclusively accessible; error output is disjoint.
 */
QdkStatus qdk_builder_cancel(struct QdkBuilder *builder, struct QdkDiagnostic **error);

/**
 * Appends through an explicitly open builder, refusing spent or non-current
 * handles rather than accidentally editing another construction scope.
 * # Safety
 * Builder, operation and its buffers/handles are live; error output is disjoint.
 */
QdkStatus qdk_builder_append(struct QdkBuilder *builder,
                             const struct QdkOperation *operation,
                             struct QdkDiagnostic **error);

/**
 * Appends break or continue to the current loop context.
 * # Safety
 * Circuit is live and exclusively accessible; error output is disjoint.
 */
QdkStatus qdk_circuit_jump(struct QdkCircuit *circuit, uint32_t kind, struct QdkDiagnostic **error);

/**
 * Releases a completed builder. Unfinished release is refused except for
 * drop-only teardown of a poisoned owner; live children always prevent release.
 * # Safety
 * Slot contains NULL or a live owning builder and is exclusively accessible.
 */
QdkStatus qdk_builder_free(struct QdkBuilder **builder, struct QdkDiagnostic **error);

/**
 * Declares existing native typed runtime state. Target support remains a
 * compiler concern; declaring a variable does not initialize its runtime value.
 * # Safety
 * Circuit and name bytes are live; writable outputs are disjoint.
 */
QdkStatus qdk_circuit_variable(struct QdkCircuit *circuit,
                               const uint8_t *name,
                               size_t name_len,
                               uint32_t value_type,
                               struct QdkVariable **out,
                               struct QdkDiagnostic **error);

/**
 * Looks up an existing runtime variable by owner-local ID without declaring
 * or changing it. IDs from loop metadata or expression nodes must be used
 * with their original circuit; they are not cross-circuit identities.
 * An unknown ID returns `QDK_INVALID_ARGUMENT`. Release the dependent handle
 * before its circuit, or let its callback scope dispose it.
 * # Safety
 * Circuit is live; writable output slots are disjoint.
 */
QdkStatus qdk_circuit_variable_at(struct QdkCircuit *circuit,
                                  uint32_t id,
                                  struct QdkVariable **out,
                                  struct QdkDiagnostic **error);

/**
 * Copies variable identity, declared type and owned UTF-8 name.
 * # Safety
 * Variable is live; writable output slots are disjoint.
 */
QdkStatus qdk_variable_info(const struct QdkVariable *variable,
                            struct QdkVariableInfo *out,
                            struct QdkDiagnostic **error);

/**
 * Returns a registered child expression for traversal. Unary nodes accept
 * index zero; binary nodes accept zero and one.
 * # Safety
 * Expression is live; output slots are writable and disjoint.
 */
QdkStatus qdk_expression_operand(const struct QdkExpression *expression,
                                 size_t index,
                                 struct QdkExpression **out,
                                 struct QdkDiagnostic **error);

/**
 * Creates an existing native runtime expression with structural/type checks.
 * This does not promise that a target compiler supports the resulting program.
 * # Safety
 * Circuit and descriptor inputs are live; outputs are writable and disjoint.
 */
QdkStatus qdk_circuit_expression(struct QdkCircuit *circuit,
                                 const struct QdkExpressionInput *input,
                                 struct QdkExpression **out,
                                 struct QdkDiagnostic **error);

/**
 * Copies runtime expression node fields without exposing a native borrow.
 * # Safety
 * Expression is live; output slots are writable and disjoint.
 */
QdkStatus qdk_expression_info(const struct QdkExpression *expression,
                              struct QdkExpressionInfo *out,
                              struct QdkDiagnostic **error);

/**
 * Releases an expression, including during poisoned-owner teardown.
 * # Safety
 * Slot contains NULL or a live owning expression and is exclusively accessible.
 */
QdkStatus qdk_expression_free(struct QdkExpression **expression, struct QdkDiagnostic **error);

/**
 * Releases a runtime variable handle.
 * # Safety
 * Slot contains NULL or a live owning variable and is exclusively accessible.
 */
QdkStatus qdk_variable_free(struct QdkVariable **variable, struct QdkDiagnostic **error);

/**
 * Lowers a closed circuit using the existing supported graph crossing.
 * # Safety
 * Circuit is live; writable outputs are disjoint.
 */
QdkStatus qdk_circuit_to_graph(const struct QdkCircuit *circuit,
                               struct QdkGraph **out,
                               struct QdkDiagnostic **error);

/**
 * Raises a graph and returns positional origins. Registers are not recovered.
 * Both owning outputs are required and remain NULL on failure.
 * # Safety
 * Graph is live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_to_circuit(const struct QdkGraph *graph,
                               struct QdkCircuit **out,
                               struct QdkGraphOrigins **origins,
                               struct QdkDiagnostic **error);

/**
 * Clones a graph without borrowing or changing its source.
 * # Safety
 * Graph is live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_clone(const struct QdkGraph *graph,
                          struct QdkGraph **out,
                          struct QdkDiagnostic **error);

/**
 * Copies graph dimensions and revision.
 * # Safety
 * Graph is live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_info(const struct QdkGraph *graph,
                         struct QdkGraphInfo *out,
                         struct QdkDiagnostic **error);

/**
 * Partially rebinds named graph parameters into a new graph, retaining
 * untouched symbols and node/port/edge identity. Last duplicate name wins.
 * # Safety
 * Graph and binding input extents are live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_bind(const struct QdkGraph *graph,
                         const struct QdkGraphParameterBinding *bindings,
                         size_t count,
                         struct QdkGraph **out,
                         struct QdkDiagnostic **error);

/**
 * Exports the native safe snapshot, direct graph QIR, or QASM via circuit
 * raising. Only QIR reads `profile_tag`. No new graph vocabulary is admitted.
 * # Safety
 * Graph is live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_export(const struct QdkGraph *graph,
                           uint32_t format,
                           uint32_t profile_tag,
                           struct QdkBytes *out,
                           struct QdkDiagnostic **error);

/**
 * Imports a native graph snapshot with its existing limits, digest, schema
 * and verification checks. Other program formats are not accepted here.
 * # Safety
 * Input covers len initialized bytes; writable outputs are disjoint.
 */
QdkStatus qdk_graph_import(const uint8_t *data,
                           size_t len,
                           struct QdkGraph **out,
                           struct QdkDiagnostic **error);

/**
 * Copies graph edges without partial writes on insufficient capacity.
 * # Safety
 * Graph is live; output buffers cover capacity and are disjoint.
 */
QdkStatus qdk_graph_edges(const struct QdkGraph *graph,
                          struct QdkGraphEdge *data,
                          size_t capacity,
                          size_t *needed,
                          struct QdkDiagnostic **error);

/**
 * Runs whole-graph verification, returning an independent diagnostic list.
 * A nonempty list is a successful query, not a verified graph.
 * # Safety
 * Graph is live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_verify(const struct QdkGraph *graph,
                           struct QdkGraphDiagnostics **out,
                           size_t *count,
                           struct QdkDiagnostic **error);

/**
 * Copies one verifier diagnostic's rule code, message and optional node ID.
 * # Safety
 * Diagnostic list is live; all writable outputs are disjoint.
 */
QdkStatus qdk_graph_diagnostic(const struct QdkGraphDiagnostics *diagnostics,
                               size_t index,
                               struct QdkBytes *code,
                               struct QdkBytes *message,
                               struct QdkOptionalId *node,
                               struct QdkDiagnostic **error);

/**
 * Copies positional raised-circuit origins. NONE denotes an unattributed
 * instruction. Insufficient capacity performs no partial writes.
 * # Safety
 * Origins are live; output buffers cover capacity and are disjoint.
 */
QdkStatus qdk_graph_origins_copy(const struct QdkGraphOrigins *origins,
                                 struct QdkGraphOrigin *data,
                                 size_t capacity,
                                 size_t *needed,
                                 struct QdkDiagnostic **error);

/**
 * Releases a graph and clears its owning slot.
 * # Safety
 * Slot contains NULL or a live exclusively owned graph.
 */
QdkStatus qdk_graph_free(struct QdkGraph **value, struct QdkDiagnostic **error);

/**
 * Releases a copied diagnostic list and clears its slot.
 * # Safety
 * Slot contains NULL or a live exclusively owned diagnostic list.
 */
QdkStatus qdk_graph_diagnostics_free(struct QdkGraphDiagnostics **value,
                                     struct QdkDiagnostic **error);

/**
 * Releases copied origins and clears their slot.
 * # Safety
 * Slot contains NULL or live exclusively owned origins.
 */
QdkStatus qdk_graph_origins_free(struct QdkGraphOrigins **value, struct QdkDiagnostic **error);

/**
 * Creates an owning single-use graph builder.
 * # Safety
 * Writable output slots are disjoint.
 */
QdkStatus qdk_graph_builder_new(struct QdkGraphBuilder **out, struct QdkDiagnostic **error);

/**
 * Adds a region with an optional parent. Whole-graph validity is checked by
 * `qdk_graph_verify`, not by finish.
 * # Safety
 * Builder is live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_builder_region(struct QdkGraphBuilder *builder,
                                   uint32_t kind,
                                   struct QdkOptionalId parent,
                                   uint32_t *out,
                                   struct QdkDiagnostic **error);

/**
 * Adds a node, copying its operation name and effects.
 * # Safety
 * Inputs cover their declared extents; builder is live; outputs are disjoint.
 */
QdkStatus qdk_graph_builder_node(struct QdkGraphBuilder *builder,
                                 uint32_t region,
                                 struct QdkString operation,
                                 const struct QdkGraphEffectInput *effects,
                                 size_t effect_count,
                                 uint32_t *out,
                                 struct QdkDiagnostic **error);

/**
 * Adds a typed port, retaining no input handle.
 * # Safety
 * Builder, descriptor and type handle are live; outputs are disjoint.
 */
QdkStatus qdk_graph_builder_port(struct QdkGraphBuilder *builder,
                                 const struct QdkGraphPortInput *input,
                                 uint32_t *out,
                                 struct QdkDiagnostic **error);

/**
 * Connects ports with the native builder's local edge checks.
 * # Safety
 * Builder is live; optional diagnostic output is disjoint.
 */
QdkStatus qdk_graph_builder_connect(struct QdkGraphBuilder *builder,
                                    struct QdkGraphEdge edge,
                                    struct QdkDiagnostic **error);

/**
 * Copies a node's source symbol into the graph under construction.
 * # Safety
 * Builder and input byte extents are live; error output is disjoint.
 */
QdkStatus qdk_graph_builder_symbol(struct QdkGraphBuilder *builder,
                                   uint32_t node,
                                   struct QdkString package,
                                   struct QdkString item,
                                   struct QdkDiagnostic **error);

/**
 * Declares one typed value and an independent optional origin.
 * # Safety
 * Builder, descriptor and selected angle are live; error output is disjoint.
 */
QdkStatus qdk_graph_builder_payload(struct QdkGraphBuilder *builder,
                                    uint32_t node,
                                    const struct QdkGraphPayload *payload,
                                    struct QdkDiagnostic **error);

/**
 * Binds a child port across a parent boundary using native local checks.
 * # Safety
 * Builder is live; optional error output is disjoint.
 */
QdkStatus qdk_graph_builder_bind(struct QdkGraphBuilder *builder,
                                 uint32_t region,
                                 struct QdkGraphBinding binding,
                                 struct QdkDiagnostic **error);

/**
 * Declares the node that owns a region.
 * # Safety
 * Builder is live; optional error output is disjoint.
 */
QdkStatus qdk_graph_builder_region_owner(struct QdkGraphBuilder *builder,
                                         uint32_t region,
                                         uint32_t node,
                                         struct QdkDiagnostic **error);

/**
 * Marks a node demanded, using the existing native demand semantics.
 * # Safety
 * Builder is live; optional error output is disjoint.
 */
QdkStatus qdk_graph_builder_demand(struct QdkGraphBuilder *builder,
                                   uint32_t node,
                                   struct QdkDiagnostic **error);

/**
 * Produces a graph and spends the builder. Success does not imply whole-graph
 * verification; use `qdk_graph_verify`. Free the spent builder separately.
 * # Safety
 * Builder is live; writable output slots are disjoint.
 */
QdkStatus qdk_graph_builder_finish(struct QdkGraphBuilder *builder,
                                   struct QdkGraph **out,
                                   struct QdkDiagnostic **error);

/**
 * Releases a graph builder, discarding any unreturned graph.
 * # Safety
 * Slot contains NULL or a live exclusively owned builder.
 */
QdkStatus qdk_graph_builder_free(struct QdkGraphBuilder **value, struct QdkDiagnostic **error);

/**
 * Copies the region/node/port at an enumeration position (not an ID).
 * Enumeration is in ascending local ID order; records survive graph release.
 * # Safety
 * Graph is live; writable output slots are disjoint.
 */
QdkStatus qdk_graph_record(const struct QdkGraph *graph,
                           uint32_t kind,
                           size_t index,
                           struct QdkGraphRecord **out,
                           struct QdkDiagnostic **error);

/**
 * Copies region fields and interface sizes.
 * # Safety
 * Record is live; writable output slots are disjoint.
 */
QdkStatus qdk_graph_region_info(const struct QdkGraphRecord *record,
                                struct QdkGraphRegionInfo *out,
                                struct QdkDiagnostic **error);

/**
 * Copies node metadata, typed payload and origin.
 * # Safety
 * Record is live; writable output slots are disjoint.
 */
QdkStatus qdk_graph_node_info(const struct QdkGraphRecord *record,
                              struct QdkGraphNodeInfo *out,
                              struct QdkDiagnostic **error);

/**
 * Copies port metadata. Retrieve its independent type with `qdk_graph_record_type`.
 * # Safety
 * Record is live; writable output slots are disjoint.
 */
QdkStatus qdk_graph_port_info(const struct QdkGraphRecord *record,
                              struct QdkGraphPortInfo *out,
                              struct QdkDiagnostic **error);

/**
 * Copies a node operation, symbol package/item, or named effect resource.
 * Index is used only for the effect resource. Absent fields return `WrongVariant`.
 * # Safety
 * Record is live; writable output slots are disjoint.
 */
QdkStatus qdk_graph_record_text(const struct QdkGraphRecord *record,
                                uint32_t field,
                                size_t index,
                                struct QdkBytes *out,
                                struct QdkDiagnostic **error);

/**
 * Copies node inputs/outputs or region arguments/results in native order.
 * # Safety
 * Record is live; output buffers cover capacity and are disjoint.
 */
QdkStatus qdk_graph_record_ports(const struct QdkGraphRecord *record,
                                 uint32_t field,
                                 uint32_t *data,
                                 size_t capacity,
                                 size_t *needed,
                                 struct QdkDiagnostic **error);

/**
 * Copies a node's effect metadata. Named resources use `qdk_graph_record_text`.
 * # Safety
 * Record is live; output buffers cover capacity and are disjoint.
 */
QdkStatus qdk_graph_record_effects(const struct QdkGraphRecord *record,
                                   struct QdkGraphEffectInfo *data,
                                   size_t capacity,
                                   size_t *needed,
                                   struct QdkDiagnostic **error);

/**
 * Copies a region's interface bindings.
 * # Safety
 * Record is live; output buffers cover capacity and are disjoint.
 */
QdkStatus qdk_graph_record_bindings(const struct QdkGraphRecord *record,
                                    struct QdkGraphBinding *data,
                                    size_t capacity,
                                    size_t *needed,
                                    struct QdkDiagnostic **error);

/**
 * Copies a region's typed control exits.
 * # Safety
 * Record is live; output buffers cover capacity and are disjoint.
 */
QdkStatus qdk_graph_record_exits(const struct QdkGraphRecord *record,
                                 struct QdkGraphExit *data,
                                 size_t capacity,
                                 size_t *needed,
                                 struct QdkDiagnostic **error);

/**
 * Copies a port's type into an independent handle.
 * # Safety
 * Record is live; writable output slots are disjoint.
 */
QdkStatus qdk_graph_record_type(const struct QdkGraphRecord *record,
                                struct QdkGraphType **out,
                                struct QdkDiagnostic **error);

/**
 * Copies a symbolic node payload into an independent named angle.
 * # Safety
 * Record is live; writable output slots are disjoint.
 */
QdkStatus qdk_graph_record_angle(const struct QdkGraphRecord *record,
                                 struct QdkGraphAngle **out,
                                 struct QdkDiagnostic **error);

/**
 * Releases an independent graph record and clears its slot.
 * # Safety
 * Slot contains NULL or a live exclusively owned record.
 */
QdkStatus qdk_graph_record_free(struct QdkGraphRecord **value, struct QdkDiagnostic **error);

/**
 * Creates an owned type. ARRAY/LOCATION take one child, TUPLE takes any
 * number, and primitive kinds take none. Children remain caller-owned.
 * # Safety
 * Child array and handles are live; writable output slots are disjoint.
 */
QdkStatus qdk_graph_type_new(uint32_t kind,
                             const struct QdkGraphType *const *children,
                             size_t count,
                             struct QdkGraphType **out,
                             struct QdkDiagnostic **error);

/**
 * Copies the type's kind and number of nested children.
 * # Safety
 * Type is live; output slots are writable and disjoint.
 */
QdkStatus qdk_graph_type_info(const struct QdkGraphType *value,
                              struct QdkGraphTypeInfo *out,
                              struct QdkDiagnostic **error);

/**
 * Copies a nested type into an independent owning handle.
 * # Safety
 * Type is live; output slots are writable and disjoint.
 */
QdkStatus qdk_graph_type_child(const struct QdkGraphType *value,
                               size_t index,
                               struct QdkGraphType **out,
                               struct QdkDiagnostic **error);

/**
 * Releases an independent graph type and clears its slot.
 * # Safety
 * Slot contains NULL or a live exclusively owned type.
 */
QdkStatus qdk_graph_type_free(struct QdkGraphType **value, struct QdkDiagnostic **error);

/**
 * Creates a named literal/parameter/arithmetic angle, copying all operands.
 * # Safety
 * Descriptor and its selected inputs are live; output slots are disjoint.
 */
QdkStatus qdk_graph_angle_new(const struct QdkGraphAngleInput *input,
                              struct QdkGraphAngle **out,
                              struct QdkDiagnostic **error);

/**
 * Copies angle shape, literal value and referenced-parameter count.
 * # Safety
 * Angle is live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_angle_info(const struct QdkGraphAngle *angle,
                               struct QdkGraphAngleInfo *out,
                               struct QdkDiagnostic **error);

/**
 * Copies an angle's child (left=0, right=1) into an independent handle.
 * # Safety
 * Angle is live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_angle_child(const struct QdkGraphAngle *angle,
                                size_t index,
                                struct QdkGraphAngle **out,
                                struct QdkDiagnostic **error);

/**
 * Copies one referenced parameter name in native first-seen order.
 * # Safety
 * Angle is live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_angle_parameter(const struct QdkGraphAngle *angle,
                                    size_t index,
                                    struct QdkBytes *out,
                                    struct QdkDiagnostic **error);

/**
 * Releases an independent graph angle and clears its slot.
 * # Safety
 * Slot contains NULL or a live exclusively owned angle.
 */
QdkStatus qdk_graph_angle_free(struct QdkGraphAngle **value, struct QdkDiagnostic **error);

/**
 * Copies circuit dimensions and counts.
 * # Safety
 * The circuit is live; writable outputs are disjoint from it and each other.
 */
QdkStatus qdk_circuit_info(const struct QdkCircuit *circuit,
                           struct QdkCircuitInfo *out,
                           struct QdkDiagnostic **error);

/**
 * Reserves native instruction storage without changing circuit contents.
 * # Safety
 * The circuit is live and exclusively accessible; error output is disjoint.
 */
QdkStatus qdk_circuit_reserve(struct QdkCircuit *circuit,
                              size_t additional,
                              struct QdkDiagnostic **error);

/**
 * Sets numeric global phase using the native normalization rules. Non-finite
 * phase values remain representable.
 * # Safety
 * The circuit is live and exclusively accessible; error output is disjoint.
 */
QdkStatus qdk_circuit_set_global_phase(struct QdkCircuit *circuit,
                                       double phase,
                                       struct QdkDiagnostic **error);

/**
 * Copies a top-level instruction, including owned operation payloads.
 * # Safety
 * The circuit is live; output slots are writable and disjoint.
 */
QdkStatus qdk_circuit_instruction(const struct QdkCircuit *circuit,
                                  size_t index,
                                  struct QdkInstruction **out,
                                  struct QdkDiagnostic **error);

/**
 * Copies instruction metadata. Variant-specific payload access is separate.
 * # Safety
 * Instruction is live; writable outputs are disjoint.
 */
QdkStatus qdk_instruction_info(const struct QdkInstruction *instruction,
                               struct QdkInstructionInfo *out,
                               struct QdkDiagnostic **error);

/**
 * Copies the native operation name as owned UTF-8 bytes.
 * # Safety
 * Instruction is live; writable outputs are disjoint.
 */
QdkStatus qdk_instruction_name(const struct QdkInstruction *instruction,
                               struct QdkBytes *out,
                               struct QdkDiagnostic **error);

/**
 * Copies ordered qubit operands. Insufficient capacity reports needed length
 * without partial writes. NULL with zero capacity is a size query.
 * # Safety
 * Instruction is live; outputs cover their declared capacity and are disjoint.
 */
QdkStatus qdk_instruction_qubits(const struct QdkInstruction *instruction,
                                 uint32_t *data,
                                 size_t capacity,
                                 size_t *needed,
                                 struct QdkDiagnostic **error);

/**
 * Copies ordered classical operands under the same rules as the qubit query.
 * # Safety
 * Instruction is live; outputs cover their declared capacity and are disjoint.
 */
QdkStatus qdk_instruction_clbits(const struct QdkInstruction *instruction,
                                 uint32_t *data,
                                 size_t capacity,
                                 size_t *needed,
                                 struct QdkDiagnostic **error);

/**
 * Copies typed stored angles; no live parameter handle escapes this record.
 * # Safety
 * Instruction is live; outputs cover their declared capacity and are disjoint.
 */
QdkStatus qdk_instruction_angles(const struct QdkInstruction *instruction,
                                 struct QdkStoredAngle *data,
                                 size_t capacity,
                                 size_t *needed,
                                 struct QdkDiagnostic **error);

/**
 * Copies a unitary's row-major complex matrix; other variants are refused.
 * # Safety
 * Instruction is live; outputs cover their declared capacity and are disjoint.
 */
QdkStatus qdk_instruction_matrix(const struct QdkInstruction *instruction,
                                 struct QdkComplex *data,
                                 size_t capacity,
                                 size_t *needed,
                                 struct QdkDiagnostic **error);

/**
 * Recovers a live parameter handle from a copied record, checking the original
 * circuit identity rather than interpreting an ID in a different circuit.
 * # Safety
 * Instruction/circuit are live; writable outputs are disjoint.
 */
QdkStatus qdk_instruction_parameter(const struct QdkInstruction *instruction,
                                    struct QdkCircuit *circuit,
                                    size_t index,
                                    struct QdkParameter **out,
                                    struct QdkDiagnostic **error);

/**
 * Recovers a stored arithmetic expression at an angle index in a copied record.
 * The record must belong to this live circuit, including after import or clone;
 * a record from the source of a clone is not a record from the clone itself.
 * Non-expression angles return `QDK_WRONG_VARIANT`. Release the dependent
 * handle before its circuit, or let its callback scope dispose it.
 * # Safety
 * Instruction/circuit are live; writable outputs are disjoint.
 */
QdkStatus qdk_instruction_angle_expression(const struct QdkInstruction *instruction,
                                           struct QdkCircuit *circuit,
                                           size_t index,
                                           struct QdkAngleExpression **out,
                                           struct QdkDiagnostic **error);

/**
 * Releases an independent instruction record and clears its slot.
 * # Safety
 * Slot contains NULL or a live owning record and is exclusively accessible.
 */
QdkStatus qdk_instruction_free(struct QdkInstruction **instruction, struct QdkDiagnostic **error);

/**
 * Copies metadata for one explicit standard-gate tag.
 * # Safety
 * Output slots must be valid, writable and disjoint.
 */
QdkStatus qdk_gate_info(uint32_t tag, struct QdkGateInfo *out, struct QdkDiagnostic **error);

/**
 * Appends a checked operation to the current native construction body.
 * Expected validation failure appends no instruction.
 * # Safety
 * The circuit, descriptor, parameter handles and declared input buffers are
 * live and disjoint from error output. No concurrent access is allowed.
 */
QdkStatus qdk_circuit_append(struct QdkCircuit *circuit,
                             const struct QdkOperation *operation,
                             struct QdkDiagnostic **error);

/**
 * Appends one parameter-free standard gate per contiguous operand group,
 * using the native batch engine. A failed group retains its successful prefix.
 * Batches require a closed circuit.
 * # Safety
 * Circuit and operands are live; writable progress/error slots are disjoint.
 */
QdkStatus qdk_circuit_extend_standard(struct QdkCircuit *circuit,
                                      uint32_t tag,
                                      const uint32_t *qubits,
                                      size_t qubit_count,
                                      struct QdkBatchProgress *progress,
                                      struct QdkDiagnostic **error);

/**
 * Appends a mixed native batch of parameter-free standard gates. The first
 * invalid descriptor or native operand stops the batch after its valid prefix.
 * # Safety
 * Circuit, descriptors and each operand buffer are live; outputs are disjoint.
 */
QdkStatus qdk_circuit_extend(struct QdkCircuit *circuit,
                             const struct QdkGateOperands *gates,
                             size_t count,
                             struct QdkBatchProgress *progress,
                             struct QdkDiagnostic **error);

/**
 * Imports a complete circuit snapshot or OpenQASM 3 source. Display formats
 * and QIR are not import formats. Snapshot input is length-delimited bytes;
 * S-expressions and OpenQASM are UTF-8.
 * # Safety
 * Input covers len initialized bytes; output slots are writable and disjoint.
 */
QdkStatus qdk_circuit_import(uint32_t format,
                             const uint8_t *data,
                             size_t len,
                             struct QdkCircuit **out,
                             struct QdkDiagnostic **error);

/**
 * Exports a closed circuit through the existing native codec/compiler/display
 * path. `profile_tag` is required only for QIR and ignored for other formats.
 * Output bytes are owned and not NUL-terminated; failure leaves them empty.
 * # Safety
 * Circuit is live; writable output slots are disjoint from it and each other.
 */
QdkStatus qdk_circuit_export(const struct QdkCircuit *circuit,
                             uint32_t format,
                             uint32_t profile_tag,
                             struct QdkBytes *out,
                             struct QdkDiagnostic **error);

/**
 * Declares a parameter and returns a dependent handle. Release it before
 * releasing the circuit. The name is length-delimited UTF-8.
 *
 * # Safety
 * The circuit and input bytes are live; output slots are writable and disjoint.
 * Objects cannot be accessed concurrently.
 */
QdkStatus qdk_circuit_parameter(struct QdkCircuit *circuit,
                                const uint8_t *name,
                                size_t name_len,
                                struct QdkParameter **out,
                                struct QdkDiagnostic **error);

/**
 * Returns the owner-local parameter ID. The number is not a cross-circuit
 * identity and using it later requires a live matching circuit.
 *
 * # Safety
 * The parameter is live and outputs are writable and disjoint.
 */
QdkStatus qdk_parameter_id(const struct QdkParameter *parameter,
                           uint32_t *out,
                           struct QdkDiagnostic **error);

/**
 * Releases a dependent parameter and clears its slot. NULL is a no-op.
 * Release is permitted even if its circuit is poisoned.
 *
 * # Safety
 * The slot contains NULL or a live owning parameter handle.
 */
QdkStatus qdk_parameter_free(struct QdkParameter **parameter, struct QdkDiagnostic **error);

/**
 * Registers an owning graph callback. Success transfers responsibility for
 * context destruction; failure does not. Callbacks run on the calling thread.
 * Contexts and callbacks must support sequential use on any calling thread;
 * no concurrent access to this handle is permitted.
 * # Safety
 * Input extents/function pointers are valid; outputs are disjoint. Retained
 * state uses an integer token (for example runtime/cgo.Handle), not a Go pointer.
 */
QdkStatus qdk_graph_pass_new(const struct QdkGraphPassOptions *options,
                             QdkGraphCallback callback,
                             uintptr_t context,
                             QdkContextDestructor destructor,
                             struct QdkGraphPass **out,
                             struct QdkDiagnostic **error);

/**
 * Copies the registration name.
 * # Safety
 * Pass is live; writable outputs are disjoint.
 */
QdkStatus qdk_graph_pass_name(const struct QdkGraphPass *pass,
                              struct QdkBytes *out,
                              struct QdkDiagnostic **error);

/**
 * Creates an owned decision. KEEP/REMOVE take no operation names, REPLACE
 * takes one, and EXPAND takes a nonempty sequence. Inputs are copied.
 * # Safety
 * Input extents are valid; outputs are writable and disjoint.
 */
QdkStatus qdk_pass_decision_new(uint32_t kind,
                                const struct QdkString *operations,
                                size_t count,
                                struct QdkPassDecision **out,
                                struct QdkDiagnostic **error);

/**
 * Runs native matching and patch/fragment application without changing input.
 * Failure leaves graph and result outputs NULL; callback side effects cannot
 * be rolled back. Active input graph/pass release and reentry are refused.
 * # Safety
 * Handles are live; writable outputs are disjoint. Callback code must not
 * retain borrowed match data or unwind across the ABI.
 */
QdkStatus qdk_graph_pass_run(const struct QdkGraphPass *pass,
                             const struct QdkGraph *graph,
                             struct QdkGraph **out,
                             struct QdkPassResult **result,
                             struct QdkDiagnostic **error);

/**
 * Copies the native outcome, applied/pending counts and actual callback count.
 * # Safety
 * Result is live; writable outputs are disjoint.
 */
QdkStatus qdk_pass_result_info(const struct QdkPassResult *result,
                               struct QdkPassResultInfo *out,
                               struct QdkDiagnostic **error);

/**
 * Copies one native oscillation rule name.
 * # Safety
 * Result is live; writable outputs are disjoint.
 */
QdkStatus qdk_pass_result_rule(const struct QdkPassResult *result,
                               size_t index,
                               struct QdkBytes *out,
                               struct QdkDiagnostic **error);

/**
 * Releases a callback registration and its context after its last use.
 * # Safety
 * Slot contains NULL or a live exclusively owned registration.
 */
QdkStatus qdk_graph_pass_free(struct QdkGraphPass **value, struct QdkDiagnostic **error);

/**
 * Releases a decision not transferred to a callback result.
 * # Safety
 * Slot contains NULL or a live exclusively owned decision.
 */
QdkStatus qdk_pass_decision_free(struct QdkPassDecision **value, struct QdkDiagnostic **error);

/**
 * Releases an independent pass report.
 * # Safety
 * Slot contains NULL or a live exclusively owned report.
 */
QdkStatus qdk_pass_result_free(struct QdkPassResult **value, struct QdkDiagnostic **error);

/**
 * Offers an independent closed, unpoisoned, inactive and dependent-free
 * replacement. Acceptance moves ownership and immediately clears candidate.
 * Rejection leaves caller ownership unchanged. A second offer is `AlreadySet`.
 * # Safety
 * Scope is valid only during its callback. Candidate is a disjoint owning
 * slot containing a live circuit; error output is writable and disjoint.
 */
QdkStatus qdk_callback_offer(struct QdkCircuitCallbackScope *scope,
                             struct QdkCircuit **candidate,
                             struct QdkDiagnostic **error);

/**
 * Creates an owning native pipeline with or without built-ins (one or zero),
 * using an explicit optimization-level tag.
 * # Safety
 * Writable output slots are disjoint.
 */
QdkStatus qdk_pipeline_new(uint32_t built_ins,
                           uint32_t optimization,
                           struct QdkPipeline **out,
                           struct QdkDiagnostic **error);

/**
 * Registers a circuit callback at a circuit anchor. Success transfers context
 * destructor responsibility; every registration failure leaves it caller-owned.
 * Calls and destruction are synchronous on the calling thread, never concurrent.
 * # Safety
 * Pipeline, string extents and function pointers are valid. Context is an
 * integer token whose target supports sequential use on any calling thread.
 */
QdkStatus qdk_pipeline_add_circuit(struct QdkPipeline *pipeline,
                                   uint32_t anchor_tag,
                                   struct QdkString name,
                                   QdkCircuitCallback callback,
                                   uintptr_t cookie,
                                   QdkContextDestructor destructor,
                                   struct QdkDiagnostic **error);

/**
 * Registers a graph decision callback at a graph anchor, in registration order.
 * Context ownership transfers only on success.
 * # Safety
 * Inputs and callbacks are valid; context follows `qdk_graph_pass_new`'s contract.
 */
QdkStatus qdk_pipeline_add_graph(struct QdkPipeline *pipeline,
                                 uint32_t anchor_tag,
                                 const struct QdkGraphPassOptions *options,
                                 QdkGraphCallback callback,
                                 uintptr_t cookie,
                                 QdkContextDestructor destructor,
                                 struct QdkDiagnostic **error);

/**
 * Returns the native listing length, including enabled built-ins.
 * # Safety
 * Pipeline is live; writable outputs are disjoint.
 */
QdkStatus qdk_pipeline_count(const struct QdkPipeline *pipeline,
                             size_t *out,
                             struct QdkDiagnostic **error);

/**
 * Copies a native listing entry, ordered by anchor then registration.
 * # Safety
 * Pipeline is live; writable outputs are disjoint.
 */
QdkStatus qdk_pipeline_entry(const struct QdkPipeline *pipeline,
                             size_t index,
                             struct QdkPassEntryInfo *out,
                             struct QdkDiagnostic **error);

/**
 * Executes the native pipeline without modifying the input circuit. Neither
 * input nor pipeline can be freed or reentered while callbacks are running.
 * # Safety
 * Handles are live; writable outputs are disjoint; callbacks must not unwind.
 */
QdkStatus qdk_pipeline_run(const struct QdkPipeline *pipeline,
                           const struct QdkCircuit *circuit,
                           struct QdkPipelineResult **out,
                           struct QdkDiagnostic **error);

/**
 * Copies native report sizes, analysis count and actual foreign callback count.
 * # Safety
 * Result is live; writable outputs are disjoint.
 */
QdkStatus qdk_pipeline_result_info(const struct QdkPipelineResult *result,
                                   struct QdkPipelineResultInfo *out,
                                   struct QdkDiagnostic **error);

/**
 * Copies the output circuit into an independent owner. Native pipeline
 * graph crossings do not restore named registers.
 * # Safety
 * Result is live; writable outputs are disjoint.
 */
QdkStatus qdk_pipeline_result_circuit(const struct QdkPipelineResult *result,
                                      struct QdkCircuit **out,
                                      struct QdkDiagnostic **error);

/**
 * Copies the graph at the final graph anchor into an independent owner.
 * # Safety
 * Result is live; writable outputs are disjoint.
 */
QdkStatus qdk_pipeline_result_graph(const struct QdkPipelineResult *result,
                                    struct QdkGraph **out,
                                    struct QdkDiagnostic **error);

/**
 * Copies one executed pass entry in actual run order.
 * # Safety
 * Result is live; writable outputs are disjoint.
 */
QdkStatus qdk_pipeline_result_entry(const struct QdkPipelineResult *result,
                                    size_t index,
                                    struct QdkPassEntryInfo *out,
                                    struct QdkDiagnostic **error);

/**
 * Copies one name from the native changed-pass report.
 * # Safety
 * Result is live; writable outputs are disjoint.
 */
QdkStatus qdk_pipeline_result_changed(const struct QdkPipelineResult *result,
                                      size_t index,
                                      struct QdkBytes *out,
                                      struct QdkDiagnostic **error);

/**
 * Copies a native report diagnostic's pass name and message.
 * # Safety
 * Result is live; writable outputs are disjoint.
 */
QdkStatus qdk_pipeline_result_diagnostic(const struct QdkPipelineResult *result,
                                         size_t index,
                                         struct QdkBytes *pass,
                                         struct QdkBytes *message,
                                         struct QdkDiagnostic **error);

/**
 * Releases a pipeline and its successfully registered callback contexts.
 * # Safety
 * Slot contains NULL or a live exclusively owned pipeline.
 */
QdkStatus qdk_pipeline_free(struct QdkPipeline **value, struct QdkDiagnostic **error);

/**
 * Releases an independent pipeline result.
 * # Safety
 * Slot contains NULL or a live exclusively owned result.
 */
QdkStatus qdk_pipeline_result_free(struct QdkPipelineResult **value, struct QdkDiagnostic **error);

/**
 * Creates fixed, non-overlapping register ranges in declaration order. The
 * extra classical width follows the declared classical registers.
 * # Safety
 * Input arrays/strings cover their declared extents; outputs are writable and
 * disjoint from inputs and one another.
 */
QdkStatus qdk_circuit_from_registers(const struct QdkRegisterInput *registers,
                                     size_t count,
                                     uint32_t extra_clbits,
                                     struct QdkCircuit **out,
                                     struct QdkDiagnostic **error);

/**
 * Copies a named register range. No live register view escapes the owner.
 * # Safety
 * Circuit is live and writable output slots are disjoint.
 */
QdkStatus qdk_circuit_register(const struct QdkCircuit *circuit,
                               uint32_t kind,
                               size_t index,
                               struct QdkRegisterInfo *out,
                               struct QdkDiagnostic **error);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* QDK_CIRCUIT_H */
