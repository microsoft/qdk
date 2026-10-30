ARG BASE_IMAGE=mcr.microsoft.com/azurelinux/base/core:3.0
FROM ${BASE_IMAGE}
ARG BUILD_ID
ARG QDK_SOURCE_SHA
LABEL qdk.copilot-sandbox.build-id=${BUILD_ID} \
    org.opencontainers.image.revision=${QDK_SOURCE_SHA}
RUN tdnf -y upgrade && tdnf -y install squid ca-certificates shadow-utils openssl && tdnf clean all && \
    find / -xdev -type f -perm /6000 -exec chmod a-s {} + && \
    usermod -L root
COPY squid.conf allowlist.txt allowlist.local.txt /etc/squid/
# Squid needs a certificate to peek at SNI; nothing trusts it and splice never decrypts.
RUN openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 36500 \
        -subj /CN=copilot-sandbox-proxy-unused \
        -keyout /etc/squid/peek.pem -out /etc/squid/peek.pem && \
    chown squid:squid /etc/squid/peek.pem && chmod 0400 /etc/squid/peek.pem && \
    squid -k parse -f /etc/squid/squid.conf
USER squid
EXPOSE 3128
ENTRYPOINT ["/usr/sbin/squid", "-N", "-f", "/etc/squid/squid.conf"]
