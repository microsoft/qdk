// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use qsc_data_structures::{span::Span, target::TargetCapabilityFlags};
use thiserror::Error;

use crate::RuntimeFeatureFlags;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("cannot use a dynamic bool value")]
    #[diagnostic(help(
        "using a bool value that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-bool"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicBool"))]
    UseOfDynamicBool(#[label] Span),

    #[error("cannot use a dynamic integer value")]
    #[diagnostic(help(
        "using an integer value that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-integer"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicInt"))]
    UseOfDynamicInt(#[label] Span),

    #[error("cannot use a dynamic Pauli value")]
    #[diagnostic(help(
        "using a Pauli value that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-pauli"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicPauli"))]
    UseOfDynamicPauli(#[label] Span),

    #[error("cannot use a dynamic Range value")]
    #[diagnostic(help(
        "using a Range value that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-range"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicRange"))]
    UseOfDynamicRange(#[label] Span),

    #[error("cannot use a dynamic double value")]
    #[diagnostic(help(
        "using a double value that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-double"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicDouble"))]
    UseOfDynamicDouble(#[label] Span),

    #[error("cannot use a dynamic qubit")]
    #[diagnostic(help(
        "using a qubit whose allocation depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-qubit"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicQubit"))]
    UseOfDynamicQubit(#[label] Span),

    #[error("cannot use a dynamic Result")]
    #[diagnostic(help(
        "using a Result variable whose value depends on a measurement result is not supported by the current target"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-result"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicResult"))]
    UseOfDynamicResult(#[label] Span),

    #[error("cannot use a dynamic tuple")]
    #[diagnostic(help(
        "using a tuple whose members depend on a measurement result is not supported by the current target"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-tuple"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicTuple"))]
    UseOfDynamicTuple(#[label] Span),

    #[error("cannot use a dynamic big integer value")]
    #[diagnostic(help(
        "using a big integer value that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-big-integer"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicBigInt"))]
    UseOfDynamicBigInt(#[label] Span),

    #[error("cannot use a dynamic string value")]
    #[diagnostic(help(
        "using a string value that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-string"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicString"))]
    UseOfDynamicString(#[label] Span),

    #[error("cannot use a dynamic exponent")]
    #[diagnostic(help(
        "using an exponent that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-exponent"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicExponent"))]
    UseOfDynamicExponent(#[label] Span),

    #[error("cannot use an array with dynamic contents")]
    #[diagnostic(help(
        "using an array whose contents depend on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-array"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicArray"))]
    UseOfDynamicArray(#[label] Span),

    #[error("cannot use a dynamically-sized array")]
    #[diagnostic(help(
        "using an array whose size depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamically-sized-array"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicallySizedArray"))]
    UseOfDynamicallySizedArray(#[label] Span),

    #[error("cannot use a dynamic user-defined type")]
    #[diagnostic(help(
        "using a user-defined type in which one or more of its members depend on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-user-defined-type"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicUdt"))]
    UseOfDynamicUdt(#[label] Span),

    #[error("cannot use a dynamic function")]
    #[diagnostic(help(
        "using a function whose resolution depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-function"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicArrowFunction"))]
    UseOfDynamicArrowFunction(#[label] Span),

    #[error("cannot use a dynamic operation")]
    #[diagnostic(help(
        "using an operation whose resolution depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-operation"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicArrowOperation"))]
    UseOfDynamicArrowOperation(#[label] Span),

    #[error("cannot call a cyclic function with a dynamic value as argument")]
    #[diagnostic(help(
        "calling a cyclic function with an argument value that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#call-to-cyclic-function-with-dynamic-argument"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.CallToCyclicFunctionWithDynamicArg"))]
    CallToCyclicFunctionWithDynamicArg(#[label] Span),

    #[error("cannot define a cyclic operation specialization")]
    #[diagnostic(help(
        "operation specializations that contain call cycles are not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#cyclic-operation-definition"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.CyclicOperationSpec"))]
    CyclicOperationSpec(#[label] Span),

    #[error("cannot call a cyclic operation")]
    #[diagnostic(help(
        "calling an operation specialization that contains call cycles is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#call-to-cyclic-operation"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.CallToCyclicOperation"))]
    CallToCyclicOperation(#[label] Span),

    #[error("cannot call a function or operation whose resolution is dynamic")]
    #[diagnostic(help(
        "calling a function or operation whose resolution depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#call-to-dynamic-callee"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.CallToDynamicCallee"))]
    CallToDynamicCallee(#[label] Span),

    #[error("cannot perform a measurement within a dynamic scope")]
    #[diagnostic(help(
        "performing a measurement within a scope that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#measurement-within-a-dynamic-scope"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.MeasurementWithinDynamicScope"))]
    MeasurementWithinDynamicScope(#[label] Span),

    #[error("cannot call a custom measurement")]
    #[diagnostic(help("cannot call a custom measurement in the configured target profile"))]
    #[diagnostic(url("https://aka.ms/qdk.qir#call-to-custom-measurement"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.CallToCustomMeasurement"))]
    CallToCustomMeasurement(#[label] Span),

    #[error("cannot call a custom reset")]
    #[diagnostic(help("cannot call a custom reset in the configured target profile"))]
    #[diagnostic(url("https://aka.ms/qdk.qir#call-to-custom-reset"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.CallToCustomReset"))]
    CallToCustomReset(#[label] Span),

    #[error("cannot access an array using a dynamic index")]
    #[diagnostic(help(
        "accessing an array using an index that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-array-index"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicIndex"))]
    UseOfDynamicIndex(#[label] Span),

    #[error("cannot use a return within a dynamic scope")]
    #[diagnostic(help(
        "using a return within a scope that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#return-within-a-dynamic-scope"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.ReturnWithinDynamicScope"))]
    ReturnWithinDynamicScope(#[label] Span),

    #[error("cannot have a loop with a dynamic condition")]
    #[diagnostic(help(
        "using a loop with a condition that depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#loop-with-dynamic-condition"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.LoopWithDynamicCondition"))]
    LoopWithDynamicCondition(#[label] Span),

    #[error("cannot use a bool value as an output")]
    #[diagnostic(help(
        "using a bool value as an output is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-bool-output"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfBoolOutput"))]
    UseOfBoolOutput(#[label] Span),

    #[error("cannot use a double value as an output")]
    #[diagnostic(help(
        "using a Double as an output is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-double-output"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDoubleOutput"))]
    UseOfDoubleOutput(#[label] Span),

    #[error("cannot use an integer value as an output")]
    #[diagnostic(help(
        "using an integer as an output is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-integer-output"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfIntOutput"))]
    UseOfIntOutput(#[label] Span),

    #[error("cannot use value with advanced type as an output")]
    #[diagnostic(help(
        "using a value of type callable, range, big integer, Pauli, Qubit or string as an output is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-advanced-output"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfAdvancedOutput"))]
    UseOfAdvancedOutput(#[label] Span),

    #[error("cannot use a dynamic generic parameter")]
    #[diagnostic(help(
        "using a generic parameter whose resolution depends on a measurement result is not supported by the configured target profile"
    ))]
    #[diagnostic(url("https://aka.ms/qdk.qir#use-of-dynamic-generic"))]
    #[diagnostic(code("Qsc.CapabilitiesCk.UseOfDynamicGeneric"))]
    UseOfDynamicGeneric(#[label] Span),
}

#[must_use]
pub fn generate_errors_from_runtime_features(
    runtime_features: RuntimeFeatureFlags,
    span: Span,
) -> Vec<Error> {
    let mut errors = Vec::<Error>::new();

    // Errors are reported in order of relative importance, which makes it easier to read them
    // and is helpful during partial evaluation when only the first error is reported to the user.
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicBool) {
        errors.push(Error::UseOfDynamicBool(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicInt) {
        errors.push(Error::UseOfDynamicInt(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicPauli) {
        errors.push(Error::UseOfDynamicPauli(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicRange) {
        errors.push(Error::UseOfDynamicRange(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicDouble) {
        errors.push(Error::UseOfDynamicDouble(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicQubit) {
        errors.push(Error::UseOfDynamicQubit(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicResult) {
        errors.push(Error::UseOfDynamicResult(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicBigInt) {
        errors.push(Error::UseOfDynamicBigInt(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicString) {
        errors.push(Error::UseOfDynamicString(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicExponent) {
        errors.push(Error::UseOfDynamicExponent(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicArray) {
        errors.push(Error::UseOfDynamicArray(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicallySizedArray) {
        errors.push(Error::UseOfDynamicallySizedArray(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicTuple) {
        errors.push(Error::UseOfDynamicTuple(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicUdt) {
        errors.push(Error::UseOfDynamicUdt(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicArrowFunction) {
        errors.push(Error::UseOfDynamicArrowFunction(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicArrowOperation) {
        errors.push(Error::UseOfDynamicArrowOperation(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::CallToCyclicFunctionWithDynamicArg) {
        errors.push(Error::CallToCyclicFunctionWithDynamicArg(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::CyclicOperationSpec) {
        errors.push(Error::CyclicOperationSpec(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::CallToCyclicOperation) {
        errors.push(Error::CallToCyclicOperation(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::CallToDynamicCallee) {
        errors.push(Error::CallToDynamicCallee(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::MeasurementWithinDynamicScope) {
        errors.push(Error::MeasurementWithinDynamicScope(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicIndex) {
        errors.push(Error::UseOfDynamicIndex(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::ReturnWithinDynamicScope) {
        errors.push(Error::ReturnWithinDynamicScope(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::LoopWithDynamicCondition) {
        errors.push(Error::LoopWithDynamicCondition(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfBoolOutput) {
        errors.push(Error::UseOfBoolOutput(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDoubleOutput) {
        errors.push(Error::UseOfDoubleOutput(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfIntOutput) {
        errors.push(Error::UseOfIntOutput(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfAdvancedOutput) {
        errors.push(Error::UseOfAdvancedOutput(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::CallToCustomMeasurement) {
        errors.push(Error::CallToCustomMeasurement(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::CallToCustomReset) {
        errors.push(Error::CallToCustomReset(span));
    }
    if runtime_features.contains(RuntimeFeatureFlags::UseOfDynamicGeneric) {
        errors.push(Error::UseOfDynamicGeneric(span));
    }
    errors
}

#[must_use]
pub fn get_missing_runtime_features(
    runtime_features: RuntimeFeatureFlags,
    target_capabilities: TargetCapabilityFlags,
) -> RuntimeFeatureFlags {
    let missing_capabilities = !target_capabilities & runtime_features.target_capabilities();
    runtime_features.contributing_features(missing_capabilities)
}
