ARG BASE_IMAGE=mcr.microsoft.com/azurelinux/base/core:3.0
FROM ${BASE_IMAGE}
ARG BUILD_ID
ARG QDK_SOURCE_SHA
LABEL qdk.copilot-sandbox.build-id=${BUILD_ID} \
    qdk.copilot-sandbox.interface=2 \
    org.opencontainers.image.revision=${QDK_SOURCE_SHA}
RUN tdnf -y upgrade && tdnf -y install squid socat ca-certificates shadow-utils openssl && tdnf clean all && \
    find / -xdev -type f -perm /6000 -exec chmod a-s {} + && \
    usermod -L root
COPY squid.conf allowlist.txt allowlist.local.txt /etc/squid/
COPY --chmod=0555 entrypoint.sh /usr/local/bin/proxy-entrypoint
# Named volumes copy this ownership on first mount, so only squid can create the relay socket.
RUN install -d -o squid -g squid -m 0755 /run/qdk-relay
# Squid needs a certificate to peek at SNI; nothing trusts it and splice never decrypts.
RUN openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 36500 \
        -subj /CN=copilot-sandbox-proxy-unused \
        -keyout /etc/squid/peek.pem -out /etc/squid/peek.pem && \
    chown squid:squid /etc/squid/peek.pem && chmod 0400 /etc/squid/peek.pem && \
    squid -k parse -f /etc/squid/squid.conf
USER squid
ENTRYPOINT ["/usr/local/bin/proxy-entrypoint"]
