ARG BASE_REGISTRY=qdkacr-cfgqhwddajdaebhg.azurecr.io
# To build from upstream ubi8, set BASE_REGISTRY=registry.access.redhat.com

FROM ${BASE_REGISTRY}/ubi8/ubi@sha256:b76f9ead999002af783d7b7036b7b57ceee6033712ad07577240ee06489a3a40

# Install basic software building packages: git, gcc, make, etc.
# Installing the python3.11 package sets `/usr/bin/python3` as python3.11 (via /etc/alternatives)
# The tar, gzip, and xz packages are needed for handling compressed archives
# The curl, jq, and unzip packages are needed for Azure Artifacts and the msrustup installer
# The "*-devel" packages are needed if building Python from source
RUN dnf install -y \
        python3.11 \
        python3.11-pip \
        git \
        gcc \
        gcc-c++ \
        make \
        cmake \
        ninja-build \
        tar \
        gzip \
        xz \
        curl \
        jq \
        unzip \
        openssl-devel zlib-devel bzip2-devel libffi-devel xz-devel sqlite-devel \
    && dnf clean all \
    && rm -rf /var/cache/dnf

WORKDIR /work

# Sanity checks. As of Oct 7, 2026:
# - Python 3.11.13
# - git version 2.43.7
# - gcc (GCC) 8.5.0 20210514 (Red Hat 8.5.0-29)
# - Ninja 1.8.2
# - cmake version 3.26.5
RUN cat /etc/redhat-release \
    && ldd --version \
    && uname -m \
    && python3 -VV \
    && git --version \
    && gcc --version \
    && ninja --version \
    && cmake --version

CMD ["bash"]
