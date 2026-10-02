ARG BASE_REGISTRY=registry.access.redhat.com
FROM ${BASE_REGISTRY}/ubi8/ubi:8.10

# Build for
# - arm64: docker build --progress=plain --platform linux/arm64 -t qdk-image:arm64 .
# - amd64: docker build --progress=plain --platform linux/amd64 -t qdk-image:amd64 .

# Note: Update LLVM_SYS_201_PREFIX and LLVM_INSTALL_PREFIX when changing LLVM version
ARG PYTHON_VERSION=3.11
ARG LLVM_TAG=llvmorg-20.1.8
ARG LLVM_INSTALL_PREFIX=/opt/llvm-20

# Install Python and basic build tools
RUN dnf install -y \
        python${PYTHON_VERSION} \
        python${PYTHON_VERSION}-pip \
        git \
        gcc \
        gcc-c++ \
        make \
        cmake \
        ninja-build \
        curl \
        tar \
        gzip \
        xz \
    && dnf clean all \
    && rm -rf /var/cache/dnf

RUN git clone \
        --branch "${LLVM_TAG}" \
        --depth 1 \
        https://github.com/llvm/llvm-project.git \
        /tmp/llvm-project \
    && cmake \
        -S /tmp/llvm-project/llvm \
        -B /tmp/llvm-build \
        -G Ninja \
        -DCMAKE_BUILD_TYPE=MinSizeRel \
        -DCMAKE_INSTALL_PREFIX="${LLVM_INSTALL_PREFIX}" \
        -DLLVM_TARGETS_TO_BUILD=Native \
        -DLLVM_INCLUDE_TESTS=OFF \
        -DLLVM_INCLUDE_BENCHMARKS=OFF \
        -DLLVM_INCLUDE_EXAMPLES=OFF \
        -DLLVM_INCLUDE_DOCS=OFF \
        -DLLVM_ENABLE_LIBXML2=OFF \
        -DLLVM_ENABLE_LIBEDIT=OFF \
        -DLLVM_ENABLE_LIBPFM=OFF \
        -DLLVM_ENABLE_BINDINGS=OFF \
        -DLLVM_ENABLE_ZLIB=OFF \
        -DLLVM_ENABLE_ZSTD=OFF \
    && cmake --build /tmp/llvm-build \
        --target install \
        --parallel 4 \
    && rm -rf /tmp/llvm-project /tmp/llvm-build

ENV PATH="${LLVM_INSTALL_PREFIX}/bin:${PATH}"
ENV LLVM_SYS_201_PREFIX="${LLVM_INSTALL_PREFIX}"

# Create a default Python environment
RUN python${PYTHON_VERSION} -m venv /opt/venv \
    && /opt/venv/bin/pip install --no-cache-dir --upgrade pip build

ENV PATH="/opt/venv/bin:${PATH}"

WORKDIR /work

# Sanity checks
RUN cat /etc/redhat-release \
    && ldd --version \
    && uname -m \
    && python --version \
    && gcc --version \
    && git --version \
    && ninja --version \
    && cmake --version \
    && llvm-config --version

CMD ["bash"]
