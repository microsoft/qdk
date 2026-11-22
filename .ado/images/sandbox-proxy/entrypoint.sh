#!/bin/bash
# Squid listens only on loopback; the agent reaches it through a Unix socket on the shared relay volume.
set -euo pipefail

stop() {
    trap '' TERM INT
    kill -TERM "${children[@]}" 2>/dev/null || true
}

/usr/sbin/squid -N -f /etc/squid/squid.conf &
children=($!)
/usr/bin/socat UNIX-LISTEN:/run/qdk-relay/proxy.sock,fork,unlink-early,mode=0666,max-children=256 \
    TCP:127.0.0.1:3128 &
children+=($!)
trap stop TERM INT

# Exit when either process dies, so the agent loses connectivity rather than seeing a half-working proxy.
status=0
wait -n || status=$?
stop
wait || true
exit "$status"
