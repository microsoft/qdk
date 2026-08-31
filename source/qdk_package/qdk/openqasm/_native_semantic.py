# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# Re-exports the semantic OpenQASM native surface. The node classes live in
# `qdk._native._semantic`, an attribute-only namespace with no `sys.modules`
# entry, so a dotted import of its members is impossible and each name is
# rebound by assignment. The two result-surface names are registered on the
# flat native module. The sibling `_native_semantic.pyi` stub declares all of
# them at module level, which is what lets callers annotate with plain names.

# pyright: reportAttributeAccessIssue=false

from .._native import (
    AnalysisResult,
    analyze,
    _semantic,
)

CastKind = _semantic.CastKind
IOKind = _semantic.IOKind
Angle = _semantic.Angle
Duration = _semantic.Duration
Type = _semantic.Type
IntType = _semantic.IntType
UintType = _semantic.UintType
FloatType = _semantic.FloatType
AngleType = _semantic.AngleType
ComplexType = _semantic.ComplexType
BitType = _semantic.BitType
BoolType = _semantic.BoolType
DurationType = _semantic.DurationType
StretchType = _semantic.StretchType
QubitType = _semantic.QubitType
HardwareQubitType = _semantic.HardwareQubitType
BitArrayType = _semantic.BitArrayType
QubitArrayType = _semantic.QubitArrayType
ArrayType = _semantic.ArrayType
StaticArrayReferenceType = _semantic.StaticArrayReferenceType
DynArrayReferenceType = _semantic.DynArrayReferenceType
GateType = _semantic.GateType
FunctionType = _semantic.FunctionType
RangeType = _semantic.RangeType
SetType = _semantic.SetType
VoidType = _semantic.VoidType
ErrorType = _semantic.ErrorType
Symbol = _semantic.Symbol
SymbolTable = _semantic.SymbolTable
SemanticExpression = _semantic.SemanticExpression
SemanticStatement = _semantic.SemanticStatement
Program = _semantic.Program
HardwareQubit = _semantic.HardwareQubit
QuantumGateModifier = _semantic.QuantumGateModifier
RangeDefinition = _semantic.RangeDefinition
DiscreteSet = _semantic.DiscreteSet
SwitchCase = _semantic.SwitchCase
SubroutineParameter = _semantic.SubroutineParameter
GateParameter = _semantic.GateParameter
ErrorExpression = _semantic.ErrorExpression
Identifier = _semantic.Identifier
CapturedIdentifier = _semantic.CapturedIdentifier
UnaryExpression = _semantic.UnaryExpression
BinaryExpression = _semantic.BinaryExpression
LiteralExpression = _semantic.LiteralExpression
FunctionCall = _semantic.FunctionCall
BuiltinFunctionCall = _semantic.BuiltinFunctionCall
Cast = _semantic.Cast
IndexExpression = _semantic.IndexExpression
ParenExpression = _semantic.ParenExpression
QuantumMeasurement = _semantic.QuantumMeasurement
RuntimeSizeof = _semantic.RuntimeSizeof
DurationOf = _semantic.DurationOf
Concatenation = _semantic.Concatenation
AliasStatement = _semantic.AliasStatement
ClassicalAssignment = _semantic.ClassicalAssignment
QuantumBarrier = _semantic.QuantumBarrier
Box = _semantic.Box
CompoundStatement = _semantic.CompoundStatement
BreakStatement = _semantic.BreakStatement
CalibrationStatement = _semantic.CalibrationStatement
CalibrationGrammarDeclaration = _semantic.CalibrationGrammarDeclaration
ClassicalDeclaration = _semantic.ClassicalDeclaration
ContinueStatement = _semantic.ContinueStatement
SubroutineDefinition = _semantic.SubroutineDefinition
CalibrationDefinition = _semantic.CalibrationDefinition
DelayInstruction = _semantic.DelayInstruction
EndStatement = _semantic.EndStatement
ExpressionStatement = _semantic.ExpressionStatement
ExternDeclaration = _semantic.ExternDeclaration
ForInLoop = _semantic.ForInLoop
QuantumGate = _semantic.QuantumGate
BranchingStatement = _semantic.BranchingStatement
IndexedClassicalAssignment = _semantic.IndexedClassicalAssignment
InputDeclaration = _semantic.InputDeclaration
OutputDeclaration = _semantic.OutputDeclaration
Pragma = _semantic.Pragma
QuantumGateDefinition = _semantic.QuantumGateDefinition
QubitDeclaration = _semantic.QubitDeclaration
QubitArrayDeclaration = _semantic.QubitArrayDeclaration
QuantumReset = _semantic.QuantumReset
ReturnStatement = _semantic.ReturnStatement
SwitchStatement = _semantic.SwitchStatement
WhileLoop = _semantic.WhileLoop
ErrorStatement = _semantic.ErrorStatement
