#!/bin/sh
# Reversible vendor-userland isolation for a stock M64 CB6 V5 / P738B.
# Never enables the PSU, opens the hardware watchdog, or unbinds a driver.
set -eu
BASE=/data/m64-cb-tools/maintenance
SERVICES='btminer system-monitor remote-daemon api keyd'
PROCESSES='btminer system-monitor remote-daemon api api-v2 keyd btsync'
SELF_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

fail() { printf 'error: %s\n' "$*" >&2; exit 1; }

assert_board() {
    [ "$(id -u)" = 0 ] || fail 'Run as the privileged micro account.'
    [ "$(cat /sys/bitmicro/platform)" = ALLWINNER_H616 ] || fail 'Expected H616.'
    [ "$(cat /sys/bitmicro/hw_name)" = CB6 ] || fail 'Expected CB6.'
    [ "$(cat /sys/bitmicro/hw_version)" = 5 ] || fail 'Expected CB6 V5.'
}

assert_off() {
    [ "$(cat /proc/sys/kernel/tainted)" = 0 ] || fail 'Kernel exception state: do not continue.'
    [ "$(cat /sys/bitmicro/power/model)" = P738B ] || fail 'Expected identified P738B.'
    [ "$(cat /sys/bitmicro/power/enable)" = 0 ] || fail 'Stop mining and disable output first.'
    [ "$(cat /sys/bitmicro/power/vout)" = 0 ] || fail 'Output voltage is not zero.'
    [ "$(cat /sys/bitmicro/power/iout)" = 0 ] || fail 'Output current is not zero.'
    for app in btminer btminer_test power-aging mujina-minerd; do
        if pidof "$app" >/dev/null; then fail "$app must be stopped first."; fi
    done
}

watchdog_running() {
    ubus call system watchdog | grep -q '"status": "running"' || fail 'PID 1 watchdog is not running.'
    found=0
    for fd in /proc/1/fd/*; do
        case "$(readlink "$fd" 2>/dev/null || true)" in *watchdog*) found=1;; esac
    done
    [ "$found" = 1 ] || fail 'Cannot establish PID 1 watchdog ownership.'
}

mounted() {
    target=$(readlink -f "$1")
    awk -v target="$target" '$2 == target {found=1} END {exit !found}' /proc/mounts
}

bind_guard() {
    source=$1
    target=$2
    if mounted "$target"; then
        cmp -s "$source" "$target" || fail "Unexpected existing mount at $target."
    else
        mount -o bind "$source" "$target"
    fi
    cmp -s "$source" "$target" || fail "Guard verification failed: $target."
}

unmount_guard() {
    source=$1
    target=$2
    if mounted "$target"; then
        cmp -s "$source" "$target" || fail "Refusing to unmount an unrecognized guard at $target."
        umount "$target"
    fi
}

keepalive_off() {
    if [ -w /sys/bitmicro/keepalive/enable ]; then
        printf '0' > /sys/bitmicro/keepalive/enable
        [ "$(cat /sys/bitmicro/keepalive/enable)" = 0 ] || fail 'Vendor keepalive did not disable.'
    fi
}

restore_cooling() {
    # Restore the saved channel controls; PWM readback alone is insufficient
    # because this kernel boots with both channel enable values at zero.
    for channel in 0 1; do
        for attribute in zero_speed pwm_duty enable; do
            value=$(cat "$BASE/cooling.before/fan$channel.$attribute")
            case "$value" in ''|*[!0-9]*) fail 'Invalid cooling snapshot.';; esac
            case "$attribute" in
                pwm_duty) [ "$value" -le 100 ] || fail 'Invalid PWM snapshot.';;
                *) [ "$value" -le 1 ] || fail 'Invalid cooling control snapshot.';;
            esac
            path=/sys/bitmicro/fan/fan$channel/$attribute
            printf '%s' "$value" > "$path"
            [ "$(cat "$path")" = "$value" ] || fail "Cooling restore failed: $path."
        done
    done
}

apply_guards() {
    bind_guard "$BASE/keepalive-disabled.sh" /usr/bin/keepalive.sh
    for service in $SERVICES; do
        bind_guard "$BASE/guard-init.sh" "/etc/init.d/$service"
    done
    keepalive_off
}

prepare() {
    assert_board
    assert_off
    watchdog_running
    [ ! -e "$BASE" ] || fail "$BASE already exists; preserve its backup and use enter/status/restore."
    [ ! -e /etc/init.d/m64-maintenance ] || fail 'An existing maintenance init script needs review.'
    for service in $SERVICES; do
        [ -x "/etc/init.d/$service" ] || fail "Missing service $service."
        mounted "/etc/init.d/$service" && fail "Existing mount at service $service."
    done
    if mounted /usr/bin/keepalive.sh; then fail 'Existing keepalive mount needs review.'; fi
    mkdir -p "$BASE/backup/etc/init.d" "$BASE/backup/etc/rc.d" "$BASE/backup/usr/bin"
    chmod 700 /data/m64-cb-tools "$BASE"
    for service in $SERVICES; do
        cp -p "/etc/init.d/$service" "$BASE/backup/etc/init.d/$service"
        for link in /etc/rc.d/[SK][0-9][0-9]"$service"; do
            [ -e "$link" ] || [ -L "$link" ] || continue
            [ -L "$link" ] || fail "Unexpected non-symlink startup entry $link."
            cp -a "$link" "$BASE/backup/etc/rc.d/"
        done
    done
    cp -p /usr/bin/keepalive.sh "$BASE/backup/usr/bin/keepalive.sh"
    cat /sys/bitmicro/keepalive/enable > "$BASE/keepalive.before"
    cat /proc/sys/kernel/random/boot_id > "$BASE/boot-id.before"
    ubus call system watchdog > "$BASE/watchdog.before.json"
    mkdir "$BASE/cooling.before"
    for channel in 0 1; do
        for attribute in enable zero_speed pwm_duty; do
            cat "/sys/bitmicro/fan/fan$channel/$attribute" > "$BASE/cooling.before/fan$channel.$attribute"
        done
    done
    : > "$BASE/processes.before"
    for app in $PROCESSES; do
        printf '%s: ' "$app" >> "$BASE/processes.before"
        pidof "$app" >> "$BASE/processes.before" || true
    done
    for file in maintenance-mode.sh guard-init.sh keepalive-disabled.sh m64-maintenance.init; do
        cp -p "$SELF_DIR/$file" "$BASE/$file"
        chown 0:0 "$BASE/$file"
        chmod 755 "$BASE/$file"
        sh -n "$BASE/$file"
    done
    (cd "$BASE/backup" && sha256sum etc/init.d/* usr/bin/keepalive.sh > SHA256SUMS)
    touch "$BASE/READY"
    sync
    printf 'Prepared backups and payload at %s; no services changed.\n' "$BASE"
}

enter() {
    assert_board
    assert_off
    watchdog_running
    [ -f "$BASE/READY" ] || fail 'Run prepare and retain its backup first.'
    if [ -e /etc/init.d/m64-maintenance ]; then
        cmp -s "$BASE/m64-maintenance.init" /etc/init.d/m64-maintenance || fail 'Unexpected maintenance init file.'
    else
        cp -p "$BASE/m64-maintenance.init" /etc/init.d/m64-maintenance
        chown 0:0 /etc/init.d/m64-maintenance
    fi
    for priority in 01 88; do
        link=/etc/rc.d/S${priority}m64-maintenance
        if [ -L "$link" ]; then
            [ "$(readlink "$link")" = ../init.d/m64-maintenance ] || fail "Unexpected link $link."
        else
            [ ! -e "$link" ] || fail "Unexpected entry $link."
            ln -s ../init.d/m64-maintenance "$link"
        fi
    done
    sync
    touch "$BASE/ARMED"
    sync
    for service in $SERVICES; do
        for link in /etc/rc.d/[SK][0-9][0-9]"$service"; do
            [ -e "$link" ] || [ -L "$link" ] || continue
            [ -L "$link" ] || fail "Unexpected non-symlink startup entry $link."
            rm "$link"
        done
    done
    sync
    apply_guards
    # Remove the vendor respawner before retiring its managed applications.
    for service in remote-daemon system-monitor api keyd btminer; do
        "$BASE/backup/etc/init.d/$service" stop
        keepalive_off
        watchdog_running
    done
    for app in $PROCESSES; do
        if pidof "$app" >/dev/null; then fail "$app is still running; inhibit policy remains armed."; fi
    done
    assert_off
    keepalive_off
    watchdog_running
    sync
    printf 'Vendor applications stopped; startup and respawn inhibited. Hardware watchdog retained.\n'
    status
}

boot() {
    [ -f "$BASE/ARMED" ] || exit 0
    assert_board
    apply_guards
    # Fail off only for the positively identified, supported PSU. A second
    # S88 invocation covers later platform detection without starting mining.
    if [ "$(cat /sys/bitmicro/power/model 2>/dev/null || true)" = P738B ]; then
        printf '0' > /sys/bitmicro/power/enable
    fi
    restore_cooling
    touch /tmp/was_poweroff
    logger -t m64-maintenance 'Vendor application startup inhibited for Mujina development'
}

status() {
    printf 'Maintenance policy: '
    if [ -f "$BASE/ARMED" ]; then printf 'armed\n'; else printf 'not armed\n'; fi
    for app in $PROCESSES; do
        printf '%s: ' "$app"
        pidof "$app" || printf 'stopped\n'
    done
    for path in /proc/sys/kernel/random/boot_id /proc/sys/kernel/tainted \
        /sys/bitmicro/keepalive/enable /sys/bitmicro/power/enable \
        /sys/bitmicro/power/vout /sys/bitmicro/power/iout \
        /sys/bitmicro/power/status /sys/bitmicro/power/common_status \
        /sys/bitmicro/fan/fan0/enable /sys/bitmicro/fan/fan0/zero_speed \
        /sys/bitmicro/fan/fan0/pwm_duty /sys/bitmicro/fan/fan1/enable \
        /sys/bitmicro/fan/fan1/zero_speed /sys/bitmicro/fan/fan1/pwm_duty; do
        printf '%s: ' "$path"
        cat "$path"
    done
    printf 'Hardware watchdog (PID 1 remains in charge):\n'
    ubus call system watchdog
    printf 'Maintenance boot entries:\n'
    ls -l /etc/rc.d/S01m64-maintenance /etc/rc.d/S88m64-maintenance 2>/dev/null || true
}

restore() {
    assert_board
    assert_off
    watchdog_running
    [ -f "$BASE/READY" ] || fail 'No backup is available.'
    for service in $SERVICES; do
        unmount_guard "$BASE/guard-init.sh" "/etc/init.d/$service"
        cmp -s "$BASE/backup/etc/init.d/$service" "/etc/init.d/$service" || fail "Original $service changed."
    done
    unmount_guard "$BASE/keepalive-disabled.sh" /usr/bin/keepalive.sh
    cmp -s "$BASE/backup/usr/bin/keepalive.sh" /usr/bin/keepalive.sh || fail 'Original keepalive script changed.'
    for link in "$BASE"/backup/etc/rc.d/*; do
        [ -L "$link" ] || continue
        destination=/etc/rc.d/${link##*/}
        [ ! -e "$destination" ] && [ ! -L "$destination" ] || fail "Startup entry already exists: $destination."
        cp -a "$link" "$destination"
    done
    for priority in 01 88; do
        link=/etc/rc.d/S${priority}m64-maintenance
        if [ -L "$link" ]; then
            [ "$(readlink "$link")" = ../init.d/m64-maintenance ] || fail "Unexpected link $link."
            rm "$link"
        fi
    done
    if [ -e /etc/init.d/m64-maintenance ]; then
        cmp -s "$BASE/m64-maintenance.init" /etc/init.d/m64-maintenance || fail 'Maintenance init file changed.'
        rm /etc/init.d/m64-maintenance
    fi
    rm -f "$BASE/ARMED"
    keepalive_off
    watchdog_running
    sync
    printf 'Stock scripts/startup links restored; this command did not start services or enable the PSU.\n'
    printf 'Normal vendor respawning can resume after restore.\n'
    printf 'A future ordinary boot will again use stock startup; backups retained at %s.\n' "$BASE"
}

case "${1:-}" in
    status) status;;
    prepare|enter|restore)
        mkdir /tmp/m64-maintenance-change.lock || fail 'Another maintenance change may be running.'
        trap 'rmdir /tmp/m64-maintenance-change.lock' 0
        trap 'exit 1' HUP INT TERM
        case "$1" in prepare) prepare;; enter) enter;; restore) restore;; esac
        ;;
    boot) boot;;
    *) printf 'Usage: %s prepare|enter|status|restore\n' "$0" >&2; exit 2;;
esac
