#!/bin/sh
# Read-only stock-driver observations. Never stops services or opens UARTs.
set -eu
OUT=${1:-"./m64-status-$(date -u +%Y%m%dT%H%M%SZ)"}
BIN_DIR=${2:-"$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"}
# Refuse an existing directory so an earlier capture cannot be overwritten.
mkdir "$OUT"
FAILURES=0
printf 'command\texit_status\n' > "$OUT/results.tsv"
capture() {
    name=$1
    shift
    if "$@" > "$OUT/$name.txt" 2>&1; then
        result=0
    else
        result=$?
        FAILURES=$((FAILURES + 1))
    fi
    printf '%s\t%s\n' "$name" "$result" >> "$OUT/results.tsv"
}
capture board-info "$BIN_DIR/m64-board-info"
capture gpio "$BIN_DIR/controlboard-misc" status
capture cooling "$BIN_DIR/fan-tool" status
capture psu "$BIN_DIR/p738b-psu-tool" status
capture hashboards "$BIN_DIR/hashboard-m64" status
capture temperatures "$BIN_DIR/hashboard-m64" temps
for board in 0 1; do
    case $board in 0) address=0050;; 1) address=0051;; esac
    if [ -r "/sys/bus/i2c/devices/10-$address/eeprom" ]; then
        capture "eeprom-$board" "$BIN_DIR/hashboard-m64" eeprom "$board"
    else
        printf 'eeprom-%s\tSKIP (no kernel EEPROM node; direct I2C needs an exclusive session)\n' "$board" >> "$OUT/results.tsv"
    fi
done
printf 'Saved observations to %s (%s command failures).\n' "$OUT" "$FAILURES"
printf 'Exit status 0 means commands ran; inspect UNAVAILABLE entries and interface coverage.\n'
[ "$FAILURES" -eq 0 ]
