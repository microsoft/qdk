ARG BASE_REGISTRY=qdkacr-cfgqhwddajdaebhg.azurecr.io
ARG BASE_TAG=latest
ARG LLVM_INSTALL_PREFIX=/opt/llvm-20

# Build LLVM in a separate image then copy the installation into the final image
FROM ${BASE_REGISTRY}/qdk-base:${BASE_TAG} AS builder

ARG LLVM_INSTALL_PREFIX
ARG LLVM_ARCHIVE=llvm-project-llvmorg-20.1.8.tar.gz

COPY ${LLVM_ARCHIVE} /tmp/llvm-project.tar.gz

RUN mkdir /tmp/llvm-project \
    && tar -xzf /tmp/llvm-project.tar.gz \
        --strip-components=1 \
        -C /tmp/llvm-project \
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
        --parallel 2

FROM ${BASE_REGISTRY}/qdk-base:${BASE_TAG}

ARG LLVM_INSTALL_PREFIX

COPY --from=builder ${LLVM_INSTALL_PREFIX}/ ${LLVM_INSTALL_PREFIX}/

ENV PATH="${LLVM_INSTALL_PREFIX}/bin:${PATH}"
ENV LLVM_SYS_201_PREFIX="${LLVM_INSTALL_PREFIX}"

# Build Python 3.15t into this image
ARG PY315_ARCHIVE=Python-3.15.0.tar.xz

COPY ${PY315_ARCHIVE} /tmp/${PY315_ARCHIVE}

RUN cd /tmp \
    && echo "ba4bed1ba346b916890b76d9e320451420aa69f6408997d33c66482eeae3d575  ${PY315_ARCHIVE}" | sha256sum -c - \
    && mkdir /tmp/python315 \
    && tar -xJf /tmp/${PY315_ARCHIVE} -C /tmp/python315 --strip-components=1 \
    && cd /tmp/python315 \
    && ./configure --disable-gil \
    && make \
    && make altinstall \
    && cd /tmp \
    && rm -rf /tmp/python315 /tmp/${PY315_ARCHIVE}
# /usr/local/bin/python3.15t will be installed by make altinstall (which in on the PATH)

# Install msrustup
ARG MSRUSTUP_FEED_URL
ARG MSRUSTUP_ACCESS_TOKEN
ARG RUST_TOOLCHAIN=ms-prod-1.95

# See https://eng.ms/docs/more/languages-at-microsoft/rust/articles/gettingstarted/install/ci/container-images.html
ENV CARGO_HOME=/opt/cargo
ENV MSRUSTUP_HOME=/opt/msrustup
ENV MSRUSTUP_TOOLCHAIN=${RUST_TOOLCHAIN}
ENV PATH="${MSRUSTUP_HOME}/multiplexers/bin:${CARGO_HOME}/bin:${PATH}"

COPY install-msrustup.sh /tmp/install-msrustup.sh

RUN export MSRUSTUP_ACCESS_TOKEN MSRUSTUP_FEED_URL && \
    pushd /tmp && bash ./install-msrustup.sh && \
    popd && /tmp/msrustup toolchain install ${RUST_TOOLCHAIN} && \
    rm -f /tmp/msrustup /tmp/install-msrustup.sh

# Allow all users to run the msrustup tools
RUN chmod -R a+rX ${MSRUSTUP_HOME}

WORKDIR /work

RUN test "$(llvm-config --prefix)" = "${LLVM_INSTALL_PREFIX}" \
    && echo "LLVM host target: $(llvm-config --host-target)" \
    && echo "LLVM targets built: $(llvm-config --targets-built)" \
    && echo "LLVM build mode: $(llvm-config --build-mode)" \
    && echo "LLVM assertions: $(llvm-config --assertion-mode)" \
    && echo "LLVM RTTI: $(llvm-config --has-rtti)" \
    && echo "LLVM shared mode: $(llvm-config --shared-mode)" \
    && echo "LLVM library directory: $(llvm-config --libdir)" \
    && echo "LLVM version: $(llvm-config --version)" \
    && echo "Python 3.15t: $(python3.15t -VV)" \
    && echo "Rustc version: $(rustc --version)" \
    && echo "Cargo version: $(cargo --version)"
