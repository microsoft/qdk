ARG TARGETARCH
FROM qdk-image:${TARGETARCH}

# Build for
# - arm64: docker build --progress=plain --platform linux/arm64 -t qdk-tools:arm64 .
# - amd64: docker build --progress=plain --platform linux/amd64 -t qdk-tools:amd64 .

# Toolchain versions
ARG RUST_VERSION=1.95.0
ARG NODE_VERSION=24.21.0
ARG MATURIN_VERSION=1.10.2
ARG AUDITWHEEL_VERSION=6.8.2
ARG PATCHELF_VERSION=0.19.1.0
ARG WASM_BINDGEN_VERSION=0.2.127
ARG BINARYEN_VERSION=123
ARG VSCE_VERSION=4.0.0

# This needs to be declared again after the FROM statement
ARG TARGETARCH

# Install Node.js
RUN case "${TARGETARCH}" in \
        amd64) NODE_ARCH=x64 ;; \
        arm64) NODE_ARCH=arm64 ;; \
        *) echo "Unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
    && NODE_DIST="node-v${NODE_VERSION}-linux-${NODE_ARCH}" \
    && curl -fsSLO \
        "https://nodejs.org/dist/v${NODE_VERSION}/${NODE_DIST}.tar.xz" \
    && tar -xJf "${NODE_DIST}.tar.xz" \
        -C /usr/local --strip-components=1 \
    && rm "${NODE_DIST}.tar.xz"

# Install Rust outside of /root so it is usable by non-root users (e.g. ADO container jobs).
# chmod is done in the same layer as the install to avoid duplicating files in a new layer.
ENV RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH="/opt/cargo/bin:${PATH}"

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y \
        --no-modify-path \
        --profile minimal \
        --default-toolchain "${RUST_VERSION}" \
        --component rustfmt,clippy \
        --target wasm32-unknown-unknown \
    && chmod -R a+w "${RUSTUP_HOME}" "${CARGO_HOME}"

# Install wasm-bindgen
RUN cargo install \
        wasm-bindgen-cli \
        --version "${WASM_BINDGEN_VERSION}" \
        --locked \
    && rm -rf "${CARGO_HOME}/registry" \
              "${CARGO_HOME}/git" \
    && chmod -R a+w "${CARGO_HOME}"

# Install Maturin and Auditwheel (plus patchelf, which 'auditwheel repair' requires)
RUN pip install --no-cache-dir \
        "auditwheel==${AUDITWHEEL_VERSION}" \
        "patchelf==${PATCHELF_VERSION}" \
        "maturin==${MATURIN_VERSION}"

# Install Binaryen from the GitHub release
RUN case "${TARGETARCH}" in \
        amd64) BINARYEN_ARCH=x86_64-linux ;; \
        arm64) BINARYEN_ARCH=aarch64-linux ;; \
        *) echo "Unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
    && BINARYEN_DIST="binaryen-version_${BINARYEN_VERSION}-${BINARYEN_ARCH}" \
    && curl -fsSLO \
        "https://github.com/WebAssembly/binaryen/releases/download/version_${BINARYEN_VERSION}/${BINARYEN_DIST}.tar.gz" \
    && mkdir -p /opt/binaryen \
    && tar -xzf "${BINARYEN_DIST}.tar.gz" \
        -C /opt/binaryen --strip-components=1 \
    && rm "${BINARYEN_DIST}.tar.gz"

ENV PATH="/opt/binaryen/bin:${PATH}"

# Install vsce globally
RUN npm install -g "@vscode/vsce@${VSCE_VERSION}"

WORKDIR /work

# Sanity checks
RUN rustc --version \
    && cargo --version \
    && wasm-bindgen --version \
    && wasm-opt --version \
    && maturin --version \
    && patchelf --version \
    && printf 'node: ' && node --version \
    && printf 'npm: ' && npm --version \
    && printf 'vsce: ' && vsce --version

CMD ["bash"]
