ARG BASE_REGISTRY=registry.access.redhat.com
FROM ${BASE_REGISTRY}/ubi8/ubi:8.10

# Build for
# - arm64: docker build --progress=plain --platform linux/arm64 -t qdk-image:arm64 .
# - amd64: docker build --progress=plain --platform linux/amd64 -t qdk-image:amd64 .

ARG PYTHON_VERSION=3.11

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
    && cmake --version

CMD ["bash"]
