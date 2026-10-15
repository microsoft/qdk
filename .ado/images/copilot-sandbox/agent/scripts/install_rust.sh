#!/usr/bin/env bash
set -euo pipefail
if [[ $PACKAGE_SOURCE == public ]]; then
    export CARGO_HOME=/opt/rustup
    export HOME=/tmp/rustup-home
    mkdir -p "$HOME"
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o /tmp/rustup-init.sh
    bash /tmp/rustup-init.sh -y --no-modify-path --profile minimal \
        --default-toolchain "$RUST_CHANNEL" --component clippy --component rustfmt
    rustc --version
    cargo fmt --version
    cargo clippy --version
    chmod -R a+rX,a-w "$RUSTUP_HOME"
    exit 0
fi
[[ $PACKAGE_SOURCE == azure ]]
export MSRUSTUP_ACCESS_TOKEN
MSRUSTUP_ACCESS_TOKEN=$(</run/secrets/msrustup_access_token)
export MSRUSTUP_FEED_URL=$TOOLCHAIN_FEED
export CARGO_HOME=/tmp/cargo
bootstrap=/tmp/msrustup-bootstrap
mkdir -p "$bootstrap"
trap 'rm -rf -- "$bootstrap"' EXIT
cd "$bootstrap"
run() {
    local step=$1 status=0
    shift
    "$@" > "$bootstrap/$step.log" 2>&1 || status=$?
    cat "$bootstrap/$step.log"
    if (( status != 0 )); then
        printf 'msrustup %s failed (exit %s).\n' "$step" "$status" >&2
        exit "$status"
    fi
}
run bootstrap bash /run/secrets/msrustup_installer
run install-help ./msrustup toolchain install --help
components=()
if grep -Eq -- '(^|[[:space:],])--component([[:space:]=<]|$)' "$bootstrap/install-help.log"; then
    components=(--component clippy --component rustfmt)
fi
run toolchain ./msrustup toolchain install "$RUST_CHANNEL" "${components[@]}"
run default ./msrustup default "$RUST_CHANNEL"
install -D -m 0555 ./msrustup /opt/msrustup/bin/msrustup
rustc --version
cargo fmt --version
cargo clippy --version
chmod -R a+rX,a-w /opt/msrustup
