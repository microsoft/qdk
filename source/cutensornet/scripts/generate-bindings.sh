#!/usr/bin/env bash

set -euo pipefail

readonly ARTIFACT_NAME="cuquantum-linux-x86_64-26.06.0.17_cuda12-archive.tar.xz"
readonly ARTIFACT_SHA256="4c37aa346fab9023d985e79667b047e13a0c0f9b9fea7dfca453979b331c8f77"
readonly HEADER_SHA256="f70f31595c3c7b44682a7e4bdcd468504615983a4ec628f519cf18f0036a4687"
readonly REFERENCE_SHA256="8921d1acf0ff6d384a793893e92e10cadc850dfb29a0312726c31c4d692c3d7a"
readonly BINDGEN_VERSION="bindgen 0.72.1"
readonly CLANG_VERSION="Ubuntu clang version 14.0.0-1ubuntu1.1"
readonly CUDA_INCLUDE_DIR="/usr/local/cuda-12.9/targets/x86_64-linux/include"

readonly FUNCTION_PATTERN='^(cutensornetGetVersion|cutensornetGetCudartVersion|cutensornetGetErrorString|cutensornetGetLastError|cutensornetCreate|cutensornetDestroy|cutensornetCreateState|cutensornetDestroyState|cutensornetStateApplyTensorOperator|cutensornetStateFinalizeMPS|cutensornetStateConfigure|cutensornetCreateWorkspaceDescriptor|cutensornetDestroyWorkspaceDescriptor|cutensornetStatePrepare|cutensornetWorkspaceGetMemorySize|cutensornetWorkspaceSetMemory|cutensornetStateCompute|cutensornetStateCaptureMPS|cutensornetCreateNetworkOperator|cutensornetNetworkOperatorAppendProduct|cutensornetDestroyNetworkOperator|cutensornetCreateExpectation|cutensornetExpectationConfigure|cutensornetExpectationPrepare|cutensornetExpectationCompute|cutensornetDestroyExpectation)$'
readonly FUNCTION_NAMES='cutensornetCreate cutensornetCreateExpectation cutensornetCreateNetworkOperator cutensornetCreateState cutensornetCreateWorkspaceDescriptor cutensornetDestroy cutensornetDestroyExpectation cutensornetDestroyNetworkOperator cutensornetDestroyState cutensornetDestroyWorkspaceDescriptor cutensornetExpectationCompute cutensornetExpectationConfigure cutensornetExpectationPrepare cutensornetGetCudartVersion cutensornetGetErrorString cutensornetGetLastError cutensornetGetVersion cutensornetNetworkOperatorAppendProduct cutensornetStateApplyTensorOperator cutensornetStateCaptureMPS cutensornetStateCompute cutensornetStateConfigure cutensornetStateFinalizeMPS cutensornetStatePrepare cutensornetWorkspaceGetMemorySize cutensornetWorkspaceSetMemory'
readonly TYPE_PATTERN='^(cuDoubleComplex|cutensornetExpectationAttributes_t|cutensornetNetworkOperator_t|cutensornetStateExpectation_t|cutensornetTensorSVDAlgo_t|cutensornetStateMPSGaugeOption_t)$'
readonly REQUIRED_DECLARATIONS='cuDoubleComplex cutensornetExpectationAttributes_t cutensornetNetworkOperator_t cutensornetStateExpectation_t cutensornetExpectationAttributes_t_CUTENSORNET_EXPECTATION_CONFIG_NUM_HYPER_SAMPLES cutensornetTensorSVDAlgo_t cutensornetStateMPSGaugeOption_t cutensornetTensorSVDAlgo_t_CUTENSORNET_TENSOR_SVD_ALGO_GESVD cutensornetStateMPSGaugeOption_t_CUTENSORNET_STATE_MPS_GAUGE_SIMPLE'
readonly CONSTANT_PATTERN='^(CUTENSORNET_STATUS_.*|CUTENSORNET_STATE_PURITY_PURE|CUTENSORNET_BOUNDARY_CONDITION_OPEN|CUTENSORNET_STATE_CONFIG_MPS_SVD_ABS_CUTOFF|CUTENSORNET_STATE_CONFIG_MPS_SVD_REL_CUTOFF|CUTENSORNET_STATE_CONFIG_MPS_SVD_ALGO|CUTENSORNET_STATE_CONFIG_MPS_GAUGE_OPTION|CUTENSORNET_TENSOR_SVD_ALGO_GESVD|CUTENSORNET_STATE_MPS_GAUGE_SIMPLE|CUTENSORNET_WORKSIZE_PREF_RECOMMENDED|CUTENSORNET_MEMSPACE_DEVICE|CUTENSORNET_WORKSPACE_SCRATCH|CUTENSORNET_EXPECTATION_CONFIG_NUM_HYPER_SAMPLES)$'

usage() {
        cat <<EOF
Usage: generate-bindings.sh <${ARTIFACT_NAME}> <output.rs>

Regenerate the checked-in, reduced cuTensorNet 2.13 Rust FFI bindings from
the pinned NVIDIA cuQuantum archive.

Arguments:
    ${ARTIFACT_NAME}  Path to the pinned NVIDIA archive.
    output.rs         Destination for the generated Rust declarations.

Requires ${BINDGEN_VERSION}, ${CLANG_VERSION}, and CUDA 12.9 headers under
${CUDA_INCLUDE_DIR}. The script verifies pinned hashes and the approved
declaration surface, checks that two generations are byte-identical, and only
replaces output.rs after every check passes.
EOF
}

fail() {
    printf 'generate-bindings: %s\n' "$*" >&2
    exit 1
}

sha256_file() {
    local result
    result="$(sha256sum -- "$1")"
    printf '%s\n' "${result%% *}"
}

if [[ $# -eq 1 ]]; then
    case "$1" in
        -h | --help)
            usage
            exit 0
            ;;
    esac
fi

if [[ $# -ne 2 ]]; then
    usage >&2
    fail "expected an input archive and output path"
fi

readonly artifact_path="$1"
readonly output_path="$2"
readonly output_dir="$(dirname -- "$output_path")"

[[ "$(basename -- "$artifact_path")" == "$ARTIFACT_NAME" ]] ||
    fail "unexpected artifact name: $artifact_path"
[[ -f "$artifact_path" ]] || fail "artifact is not a regular file: $artifact_path"
[[ -d "$output_dir" ]] || fail "output directory does not exist: $output_dir"
[[ -d "$CUDA_INCLUDE_DIR" ]] || fail "CUDA include directory not found: $CUDA_INCLUDE_DIR"
[[ -f "$CUDA_INCLUDE_DIR/cuda_runtime_api.h" ]] ||
    fail "CUDA Runtime header not found under: $CUDA_INCLUDE_DIR"
command -v bindgen >/dev/null || fail "bindgen is not available"
command -v clang >/dev/null || fail "clang is not available"

actual_artifact_sha256="$(sha256_file "$artifact_path")"
[[ "$actual_artifact_sha256" == "$ARTIFACT_SHA256" ]] ||
    fail "artifact SHA-256 mismatch: $actual_artifact_sha256"

actual_bindgen_version="$(bindgen --version)"
[[ "$actual_bindgen_version" == "$BINDGEN_VERSION" ]] ||
    fail "bindgen version mismatch: $actual_bindgen_version"

clang_output="$(clang --version)"
actual_clang_version="${clang_output%%$'\n'*}"
[[ "$actual_clang_version" == "$CLANG_VERSION" ]] ||
    fail "clang version mismatch: $actual_clang_version"

temp_dir="$(mktemp -d)"
trap 'rm -rf -- "$temp_dir"' EXIT

tar -tf "$artifact_path" >"$temp_dir/archive-members.txt"
header_members=()
while IFS= read -r member; do
    case "$member" in
        */include/cutensornet.h) header_members+=("$member") ;;
    esac
done <"$temp_dir/archive-members.txt"

[[ ${#header_members[@]} -eq 1 ]] ||
    fail "expected one archive cutensornet.h, found ${#header_members[@]}"
readonly header_member="${header_members[0]}"
readonly header_dir_member="${header_member%/cutensornet.h}"

include_members=("$header_member")
while IFS= read -r member; do
    case "$member" in
        */) ;;
        "$header_dir_member"/cutensornet/*) include_members+=("$member") ;;
    esac
done <"$temp_dir/archive-members.txt"

mkdir "$temp_dir/extracted"
tar -xJf "$artifact_path" -C "$temp_dir/extracted" "${include_members[@]}"
readonly header_path="$temp_dir/extracted/$header_member"
actual_header_sha256="$(sha256_file "$header_path")"
[[ "$actual_header_sha256" == "$HEADER_SHA256" ]] ||
    fail "cutensornet.h SHA-256 mismatch: $actual_header_sha256"

readonly header_dir="$(dirname -- "$header_path")"
bindgen "$header_path" \
    --output "$temp_dir/reference.rs" \
    --no-layout-tests \
    --allowlist-function '^cutensornet.*' \
    --allowlist-type '^cutensornet.*' \
    --allowlist-var '^CUTENSORNET_.*' \
    -- \
    -I"$header_dir" \
    -I"$CUDA_INCLUDE_DIR"

actual_reference_sha256="$(sha256_file "$temp_dir/reference.rs")"
[[ "$actual_reference_sha256" == "$REFERENCE_SHA256" ]] ||
    fail "full reference output SHA-256 mismatch: $actual_reference_sha256"

for generated in "$temp_dir/reduced-a.rs" "$temp_dir/reduced-b.rs"; do
    bindgen "$header_path" \
        --output "$generated" \
        --no-layout-tests \
        --allowlist-function "$FUNCTION_PATTERN" \
        --allowlist-type "$TYPE_PATTERN" \
        --allowlist-var "$CONSTANT_PATTERN" \
        -- \
        -I"$header_dir" \
        -I"$CUDA_INCLUDE_DIR"
done

cmp --silent "$temp_dir/reduced-a.rs" "$temp_dir/reduced-b.rs" ||
    fail "reduced bindings were not deterministic"

python3 - "$temp_dir/reduced-a.rs" "$FUNCTION_NAMES" "$REQUIRED_DECLARATIONS" <<'PY'
import re
import sys

source = open(sys.argv[1], encoding="utf-8").read()
expected = set(sys.argv[2].split())
actual = set(re.findall(r"pub fn (cutensornet[A-Za-z0-9_]+)\s*\(", source))
if actual != expected:
    raise SystemExit(
        f"function surface mismatch: missing={sorted(expected - actual)}, "
        f"unexpected={sorted(actual - expected)}"
    )
missing_declarations = [name for name in sys.argv[3].split() if name not in source]
if missing_declarations:
    raise SystemExit(f"missing required declarations: {missing_declarations}")
print(f"selected_functions={len(actual)}")
print("required_declarations=9")
PY

{
    printf '%s\n' '// @generated by scripts/generate-bindings.sh; do not edit.'
    printf '%s\n' '// Source: NVIDIA cuQuantum 26.06.0, cutensornet.h 2.13.0.'
    cat "$temp_dir/reduced-a.rs"
} >"$temp_dir/output.rs"

mv -- "$temp_dir/output.rs" "$output_path"
readonly output_sha256="$(sha256_file "$output_path")"
readonly output_lines="$(wc -l <"$output_path")"

printf 'artifact_sha256=%s\n' "$actual_artifact_sha256"
printf 'header_sha256=%s\n' "$actual_header_sha256"
printf 'reference_sha256=%s\n' "$actual_reference_sha256"
printf 'output_sha256=%s\n' "$output_sha256"
printf 'output_lines=%s\n' "$output_lines"