#!/usr/bin/env bash

set -euo pipefail

readonly ARTIFACT_NAME="cuquantum-linux-x86_64-26.06.0.17_cuda12-archive.tar.xz"
readonly ARTIFACT_SHA256="4c37aa346fab9023d985e79667b047e13a0c0f9b9fea7dfca453979b331c8f77"
readonly HEADER_SHA256="f70f31595c3c7b44682a7e4bdcd468504615983a4ec628f519cf18f0036a4687"
readonly REFERENCE_SHA256="8921d1acf0ff6d384a793893e92e10cadc850dfb29a0312726c31c4d692c3d7a"
readonly BINDGEN_VERSION="bindgen 0.72.1"
readonly CLANG_VERSION="Ubuntu clang version 14.0.0-1ubuntu1.1"
readonly RUST_EDITION="2024"
readonly CUDA_INCLUDE_DIR="/usr/local/cuda-12.9/targets/x86_64-linux/include"
readonly MANIFEST_NAME="cutensornet-symbols.txt"

readonly TYPE_PATTERN='^(cuDoubleComplex|cutensornetExpectationAttributes_t|cutensornetNetworkOperator_t|cutensornetStateExpectation_t|cutensornetTensorSVDAlgo_t|cutensornetStateMPSGaugeOption_t|cutensornetNetworkDescriptor_t|cutensornetContractionOptimizerConfig_t|cutensornetContractionOptimizerInfo_t|cutensornetContractionPath_t|cutensornetSlicingConfig_t|cutensornetSliceGroup_t|cutensornetNetworkAttributes_t|cutensornetContractionOptimizerConfigAttributes_t|cutensornetContractionOptimizerInfoAttributes_t|cutensornetTensorQualifiers_t|cutensornetComputeType_t)$'
# The reduced pass deliberately allowlists no vars: every constant this crate
# uses is an enumerator, and bindgen emits those as <type>_<VARIANT> once the
# enclosing type is reachable. The full reference pass below still allowlists
# vars because its output is the pinned toolchain check and must not change.
readonly REQUIRED_DECLARATIONS='cuDoubleComplex cutensornetExpectationAttributes_t cutensornetNetworkOperator_t cutensornetStateExpectation_t cutensornetExpectationAttributes_t_CUTENSORNET_EXPECTATION_CONFIG_NUM_HYPER_SAMPLES cutensornetTensorSVDAlgo_t cutensornetStateMPSGaugeOption_t cutensornetTensorSVDAlgo_t_CUTENSORNET_TENSOR_SVD_ALGO_GESVD cutensornetStateMPSGaugeOption_t_CUTENSORNET_STATE_MPS_GAUGE_SIMPLE cutensornetNetworkDescriptor_t cutensornetContractionOptimizerConfig_t cutensornetContractionOptimizerInfo_t cutensornetContractionPath_t cutensornetNodePair_t cutensornetSlicingConfig_t cutensornetSliceInfoPair_t cutensornetSliceGroup_t cutensornetNetworkAttributes_t cutensornetContractionOptimizerConfigAttributes_t cutensornetContractionOptimizerInfoAttributes_t cutensornetTensorQualifiers_t cutensornetComputeType_t cutensornetComputeType_t_CUTENSORNET_COMPUTE_64F cutensornetComputeType_t_CUTENSORNET_COMPUTE_32F cutensornetComputeType_t_CUTENSORNET_COMPUTE_TF32 cutensornetComputeType_t_CUTENSORNET_COMPUTE_3XTF32'

usage() {
        cat <<EOF
Usage: generate-bindings.sh <${ARTIFACT_NAME}> <output.rs>

Regenerate the checked-in, reduced cuTensorNet 2.13 Rust FFI bindings from
the pinned NVIDIA cuQuantum archive.

Arguments:
    ${ARTIFACT_NAME}  Path to the pinned NVIDIA archive.
    output.rs         Destination for the generated Rust declarations.

Requires ${BINDGEN_VERSION}, ${CLANG_VERSION}, rustfmt, and CUDA 12.9 headers
under ${CUDA_INCLUDE_DIR}. The script verifies pinned hashes and the approved
declaration surface, normalises formatting to Rust edition ${RUST_EDITION},
checks that two generations are byte-identical, and only replaces output.rs
after every check passes.
EOF
}

fail() {
    printf 'generate-bindings: %s\n' "$*" >&2
    exit 1
}

# The symbol manifest is the single source of truth for the FFI surface; the
# same file drives the generate-loader binary, so the bindgen allowlist and the
# loader can never disagree about which symbols exist.
readonly manifest_path="$(dirname -- "${BASH_SOURCE[0]}")/${MANIFEST_NAME}"
[[ -f "$manifest_path" ]] || fail "symbol manifest not found: $manifest_path"

manifest_symbols() {
    awk '!/^[[:space:]]*#/ && NF { print $1 }' "$manifest_path" | sort -u
}

FUNCTION_NAMES="$(manifest_symbols | tr '\n' ' ')"
readonly FUNCTION_NAMES="${FUNCTION_NAMES% }"
[[ -n "$FUNCTION_NAMES" ]] || fail "symbol manifest declares no functions"

readonly FUNCTION_PATTERN="^($(manifest_symbols | paste -sd '|' -))$"

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
command -v rustfmt >/dev/null || fail "rustfmt is not available"

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
        -- \
        -I"$header_dir" \
        -I"$CUDA_INCLUDE_DIR"

    # bindgen invokes rustfmt with its own default edition, which disagrees
    # with this crate's edition on wrapped return types. Normalise here so the
    # generated file satisfies `cargo fmt --check` without being hand-edited.
    rustfmt --edition "$RUST_EDITION" "$generated" ||
        fail "rustfmt failed on $generated"
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
print(f"required_declarations={len(sys.argv[3].split())}")
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