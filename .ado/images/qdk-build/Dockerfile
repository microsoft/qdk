ARG BASE_REGISTRY=qdkacr-cfgqhwddajdaebhg.azurecr.io
ARG BASE_TAG=latest

ARG LLVM_INSTALL_PREFIX=/opt/llvm-20
ARG RUST_TOOLCHAIN=ms-prod-1.95
ARG LLVM_ARCHIVE=llvm-project-llvmorg-20.1.8.tar.gz
ARG PY315_ARCHIVE=Python-3.15.1.tar.xz
ARG PY315_ARCHIVE_SHA256=673ae7232d84c894729882c8257d300ef2d26523911bfd3917ec8ce7f526f99b

# Build tools into a separate image then copy the installation into the final image
FROM ${BASE_REGISTRY}/qdk-base:${BASE_TAG} AS builder

# Install msrustup into the builder image
# Note: Do this first as it requires an access token, which we don't want to expire waiting for other builds
ARG MSRUSTUP_FEED_URL
ARG MSRUSTUP_ACCESS_TOKEN
ARG RUST_TOOLCHAIN

# See https://eng.ms/docs/more/languages-at-microsoft/rust/articles/gettingstarted/install/ci/container-images.html
ENV CARGO_HOME=/opt/cargo
ENV MSRUSTUP_HOME=/opt/msrustup
ENV MSRUSTUP_TOOLCHAIN=${RUST_TOOLCHAIN}
ENV PATH="${MSRUSTUP_HOME}/multiplexers/bin:${CARGO_HOME}/bin:${PATH}"

COPY install-msrustup.sh /tmp/install-msrustup.sh

RUN export MSRUSTUP_ACCESS_TOKEN MSRUSTUP_FEED_URL && \
    (cd /tmp && bash ./install-msrustup.sh) && \
    /tmp/msrustup toolchain install ${RUST_TOOLCHAIN} && \
    chmod -R a+rX ${MSRUSTUP_HOME}

# Build LLVM in the builder image
ARG LLVM_INSTALL_PREFIX
ARG LLVM_ARCHIVE

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
# Using more than 2 parallel jobs may cause high memory usage and slow down the build on machines with limited resources

# Build Python 3.15t into this image
ARG PY315_ARCHIVE
ARG PY315_ARCHIVE_SHA256
COPY ${PY315_ARCHIVE} /tmp/${PY315_ARCHIVE}

RUN cd /tmp \
    && echo "${PY315_ARCHIVE_SHA256}  ${PY315_ARCHIVE}" | sha256sum -c - \
    && mkdir /tmp/python315 \
    && tar -xJf /tmp/${PY315_ARCHIVE} -C /tmp/python315 --strip-components=1 \
    && cd /tmp/python315 \
    && ./configure --disable-gil \
    && make -j2 \
    && make altinstall \
    && rm -rf /usr/local/lib/python3.15t/test
# /usr/local/lib/python3.15t/test is large (~170MB) and can be removed. It's only needed for testing CPython itself


# End of builder stage
# Start of final image stage


FROM ${BASE_REGISTRY}/qdk-base:${BASE_TAG}

ARG LLVM_INSTALL_PREFIX
ARG RUST_TOOLCHAIN

# Copy the msrustup installation over
COPY --from=builder /opt/msrustup /opt/msrustup
COPY --from=builder /opt/cargo /opt/cargo
ENV CARGO_HOME=/opt/cargo
ENV MSRUSTUP_HOME=/opt/msrustup
ENV MSRUSTUP_TOOLCHAIN=${RUST_TOOLCHAIN}
ENV PATH="${MSRUSTUP_HOME}/multiplexers/bin:${CARGO_HOME}/bin:${PATH}"

# Copy the LLVM files over
COPY --from=builder ${LLVM_INSTALL_PREFIX}/ ${LLVM_INSTALL_PREFIX}/
ENV PATH="${LLVM_INSTALL_PREFIX}/bin:${PATH}"
ENV LLVM_SYS_201_PREFIX="${LLVM_INSTALL_PREFIX}"

# Copy the Python installation over (this is all that exists under /usr/local at this point anyway)
COPY --from=builder /usr/local /usr/local
# python3.15t will be available on the PATH (installed in /usr/local/bin)


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
    && python3.15t -VV \
    && rustc --version \
    && cargo --version
