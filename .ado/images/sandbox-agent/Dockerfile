ARG BASE_IMAGE=mcr.microsoft.com/azurelinux/base/core:3.0

#  Install system tools and rust
FROM ${BASE_IMAGE} AS tools
ARG BASE_IMAGE
ARG RUST_CHANNEL
ARG PACKAGE_SOURCE=public
ARG TOOLCHAIN_FEED
ARG CRATES_FEED
ARG BUILD_ID
ARG QDK_SOURCE_SHA
ARG QDK_BUILD_ARGS='["--qdk","--no-check","--no-test"]'
LABEL qdk.copilot-sandbox.build-id=${BUILD_ID} \
    org.opencontainers.image.revision=${QDK_SOURCE_SHA}
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
RUN tdnf -y upgrade && tdnf -y install \
    gcc binutils make glibc-devel kernel-headers pkgconf git tar xz unzip jq curl ca-certificates \
    python3 python3-devel python3-pip nodejs nodejs-npm gh \
    less procps-ng file time gdb which diffutils patch findutils shadow-utils sed grep gawk \
    && tdnf clean all
COPY scripts/ /opt/sandbox/scripts/
COPY runtime/ /opt/sandbox/runtime/
COPY copilot/ /opt/sandbox/copilot/
RUN useradd --create-home --uid 1000 --user-group --shell /bin/bash agent \
    && install -d -o agent -g agent /work /home/agent/.copilot /home/agent/.cargo /home/agent/.npm \
    && passwd -l root \
    && ! command -v sudo
ENV MSRUSTUP_HOME=/opt/msrustup \
    MSRUSTUP_TOOLCHAIN=${RUST_CHANNEL} \
    RUSTUP_HOME=/opt/rustup \
    RUSTUP_TOOLCHAIN=${RUST_CHANNEL} \
    PACKAGE_SOURCE=${PACKAGE_SOURCE} \
    VIRTUAL_ENV=/opt/qdk-venv \
    CARGO_HOME=/opt/cargo \
    npm_config_cache=/opt/npm-cache \
    NPM_CONFIG_GLOBALCONFIG=/opt/sandbox/npmrc \
    PYRIGHT_PYTHON_CACHE_DIR=/opt/pyright \
    QDK_BUILD_ARGS=${QDK_BUILD_ARGS} \
    PATH=/opt/qdk-venv/bin:/opt/msrustup/multiplexers/bin:/opt/msrustup/bin:/opt/rustup/bin:/usr/local/bin:/usr/bin:/bin
RUN --mount=type=bind,from=qdk,source=.ado/images/install-msrustup.sh,target=/opt/sandbox/install-msrustup.sh,readonly \
    --mount=type=secret,id=msrustup_access_token \
    --mount=type=tmpfs,target=/tmp \
    bash /opt/sandbox/scripts/install_rust.sh

# Set up feeds, install the Copilot CLI, and build the QDK (by default, not the whole repo)
FROM tools AS prepared
RUN --mount=type=bind,from=qdk,target=/qdk,readonly \
    --mount=type=secret,id=npmrc \
    --mount=type=secret,id=pip_index_url \
    --mount=type=secret,id=cargo_token \
    --mount=type=tmpfs,target=/tmp \
    bash /opt/sandbox/scripts/prepare.sh \
    && chown -R agent:agent /opt/qdk-venv /opt/cargo /opt/npm-cache /opt/pyright \
    && chmod -R a-w /opt/sandbox/copilot

# Lock things down (proxy, copilot hooks, etc)
FROM prepared AS runtime-base
COPY --chmod=0444 runtime/policy/copilot-sandbox.json /etc/github-copilot/policy.d/copilot-sandbox.json
RUN chown 0:0 /etc/github-copilot/policy.d/copilot-sandbox.json \
    && chmod 0444 /etc/github-copilot/policy.d/copilot-sandbox.json \
    && chmod 0755 /etc/github-copilot /etc/github-copilot/policy.d \
    && chmod 0555 /opt/sandbox/runtime/entrypoint.sh /opt/sandbox/runtime/sandbox-push \
        /opt/sandbox/runtime/hooks/* /opt/sandbox/runtime/policy/tool_policy.py \
    && ln -s /opt/sandbox/copilot/node_modules/.bin/copilot /usr/local/bin/copilot \
    && ln -s /opt/sandbox/runtime/sandbox-push /usr/local/bin/sandbox-push \
    && find / -xdev -type f -perm /6000 -exec chmod a-s {} +
ENV HOME=/home/agent \
    CARGO_NET_OFFLINE=true \
    PIP_NO_INDEX=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    npm_config_offline=true \
    npm_config_audit=false \
    npm_config_fund=false \
    npm_config_update_notifier=false \
    COPILOT_AUTO_UPDATE=false \
    PYRIGHT_PYTHON_GLOBAL_NODE=1 \
    PYRIGHT_PYTHON_NODEJS_WHEEL=0 \
    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 \
    HTTPS_PROXY=http://proxy:3128 \
    HTTP_PROXY=http://proxy:3128 \
    NO_PROXY=localhost,127.0.0.1
USER 1000:1000
WORKDIR /work

# Make sure the QDK still builds locked down
FROM runtime-base AS verify
RUN --network=none --mount=type=bind,from=qdk,target=/qdk,readonly \
    bash -c 'source /opt/sandbox/runtime/entrypoint.sh; verify_policy' \
    && bash /opt/sandbox/scripts/build_qdk.sh \
    && printf '%s\n' "$QDK_BUILD_ARGS" > /tmp/offline-verified

# Actual image for running copilot
FROM runtime-base AS runtime
COPY --from=verify --chmod=0444 /tmp/offline-verified /opt/sandbox/offline-verified
ENTRYPOINT ["/opt/sandbox/runtime/entrypoint.sh"]
CMD ["copilot", "--allow-all"]
