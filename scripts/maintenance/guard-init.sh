#!/bin/sh
# Bind-mounted over selected vendor init scripts while maintenance is armed.
set -u
BASE=${M64_MAINT_BASE:-/data/m64-cb-tools/maintenance}
SERVICE=${0##*/}
case "$SERVICE" in [SK][0-9][0-9]*) SERVICE=${SERVICE#???};; esac
case "$SERVICE" in btminer|system-monitor|remote-daemon|api|keyd) :;; *) exit 1;; esac
case "${1:-}" in
    start|restart|reload|boot|enable|start_service)
        printf 'M64 maintenance: stock %s %s is blocked.\n' "$SERVICE" "$1" >&2
        exit 1
        ;;
    stop|stop_btminer|shutdown|status|enabled|disable|help)
        "$BASE/backup/etc/init.d/$SERVICE" "$@"
        result=$?
        case "$SERVICE:$1" in
            btminer:stop|btminer:stop_btminer|btminer:shutdown)
                if [ -w /sys/bitmicro/keepalive/enable ]; then
                    printf '0' > /sys/bitmicro/keepalive/enable || exit 1
                fi
                ;;
        esac
        exit "$result"
        ;;
    *) printf 'M64 maintenance: unsupported stock service action.\n' >&2; exit 1;;
esac
