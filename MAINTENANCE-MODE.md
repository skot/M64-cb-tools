# Persistent maintenance mode for Mujina development

The optional `scripts/maintenance/` helper isolates the selected stock
applications on the tested H616 / CB6 V5 / P738B system. This is a
development configuration for replacing `btminer` with Mujina. It is
separate from the six diagnostic binaries and is installed only by an
explicit `prepare` / `enter` operation.
The subsequent [PSU/ASIC tests](PSU-ASIC-VALIDATION.md) used this mode
for guarded power control and complete cold replies on all four slots.

## Disabled and retained components

| Component | Maintenance behavior |
| --- | --- |
| `btminer` | Stopped; stock init startup and restart actions blocked |
| `system-monitor` | Stopped; exception-triggered vendor reboot supervisor inhibited |
| `remote-daemon` | Stopped; stock management startup blocked |
| `api`, `api-v2` | Stopped through the stock `api` service; startup blocked |
| `keyd` | Stopped; stock button/LED service startup blocked |
| `btsync` | Checked absent; vendor keepalive respawner made inert |
| `/usr/bin/keepalive.sh` | Runtime bind mount of an inert script, reapplied during boot |
| `/sys/bitmicro/keepalive/enable` | Vendor miner keepalive set to `0` |
| PID 1 `procd` hardware watchdog | Retained: running, 16-second timeout, feed interval 5 seconds |
| SSH, network, ubus, logging, NTP, cron | Retained; root crontab was empty on this board |
| Kernel PSU, LM75, EEPROM and UART drivers | Remain attached; no driver handoff |
| Stock platform/config discovery, `ipfilter`, web server, TEE services | Retained; this is not a stripped operating system |

The helper removes the saved vendor service startup links and bind-mounts
a guard over the five service scripts. It blocks `start`, `restart`,
`reload`, `boot`, `enable` and `start_service`; permitted stop/status
actions delegate to the saved originals. This blocks the inspected stock
service/respawner paths, not arbitrary privileged execution of vendor
binaries. Firmware upgrades, factory restore and alternative launch paths
have not been tested with this policy.

Persistent hooks `S01m64-maintenance` and `S88m64-maintenance` reapply the
guards around stock platform detection, disable identified P738B output,
restore the saved cooling channel controls and recreate
`/tmp/was_poweroff`. The root filesystem remains read-only. Scripts,
backups and startup changes live on writable `/data` (`/etc` points into
`/data/etc`). No PSU-enable action is implemented.

## Install and inspect

These commands require the privileged `micro` account on the supported
board, stopped mining, a clean kernel and verified output-off telemetry.
Do not use the helper on another board profile without reviewing it.

From the root of an extracted bundle:

```sh
sh ./maintenance/maintenance-mode.sh prepare
```

In a source checkout, use `sh scripts/maintenance/maintenance-mode.sh prepare`.

`prepare` saves original service scripts, exact startup symlinks, the
original keepalive script, checksums and state snapshots without stopping
services. It also records both channels' `enable`, `zero_speed` and
`pwm_duty` values for boot restoration. Verify those settings before
entering maintenance; their readback does not prove physical coolant flow.
Retain an off-board copy of `/data/m64-cb-tools/maintenance` before
continuing. An existing backup directory is never overwritten by prepare.

```sh
sh /data/m64-cb-tools/maintenance/maintenance-mode.sh enter
sh /data/m64-cb-tools/maintenance/maintenance-mode.sh status
```

`enter` arms the boot policy, disables the selected startup links, applies
guards and stops the vendor applications using their saved service
scripts. A partial failure leaves the inhibition armed for inspection.
It does not open, close or feed the hardware watchdog. Status shows
processes, boot ID, kernel taint, PSU telemetry, cooling controls and
watchdog status. The hardware watchdog remains owned by PID 1.

The helper's `status` command does not run the full Mujina power-up gate.
Recheck reset/LDO state, driver ownership, UART configuration, cooling and
power telemetry in each new session. Diagnostic binaries deployed under
`/tmp` must be redeployed after reboot.

## Restore stock service policy

With Mujina and mining stopped, a clean kernel and verified zero output:

```sh
sh /data/m64-cb-tools/maintenance/maintenance-mode.sh restore
```

Restore checks the guarded mounts and underlying original scripts,
unmounts the guards, restores the exact saved startup symlinks, removes
the maintenance boot hooks and clears `ARMED`. It retains backups and the
PID 1 watchdog. The command itself does not invoke service starts or
enable the PSU; normal vendor respawning can resume afterward, and the
next ordinary boot again uses stock startup and can mine. Vendor
keepalive remains `0` until stock software changes it. Cooling is not
reduced by restore.

Rollback has been inspected and its shell syntax checked, but has **not
been exercised live**: the tested miner was deliberately left in
maintenance mode. Restore is not a firmware recovery mechanism.

## Validation on 2026-11-03

Tests used the assembled miner at `192.168.1.179`, with external coolant
circulation reported by the operator and PSU hashboard output off.

- Original scripts and exact startup links were saved on-board and an
  original backup archive was fetched off-board before runtime changes.
- All selected processes were absent after entering maintenance.
  An attempted stock `btminer start` was rejected; the replaced keepalive
  script returned without spawning vendor applications.
- The unchanged hardware watchdog continued running beyond its
  16-second timeout; PID 1 retained its watchdog descriptor.
- A controlled reboot confirmed that stock mining and supervisors did
  not restart. The first boot revealed that the kernel resets both
  cooling channel enables and PWM to zero. The boot hook was corrected
  to restore the previous `enable=1`, `zero_speed=0`, `pwm_duty=99` values
  for both channels. For this existing installation the snapshot was
  reconstructed from saved pre-maintenance readback; new preparations
  capture it directly.
- A second controlled reboot checked the corrected cooling restoration
  together with the startup guards. PSU output remained disabled with
  measured 0 V / 0 A, both reset GPIOs read low, switched LDO read low,
  and kernel taint remained zero.
- Offline guard tests cover direct, S-prefixed and K-prefixed service
  invocation for all five services and all six blocked startup actions.
  Each installed shell script passes `sh -n`.

No PSU output-enable, voltage-setting or driver unbind/rebind operation
was used for this maintenance-mode validation. Raw captures and backup
archives are kept locally under ignored `target/supervisor-isolation-2026-11-03`;
vendor binaries and device identifiers are not published here.

## Remaining Mujina work

This policy removes vendor application supervision; it does not replace
its protection functions. PID 1 can keep feeding its hardware watchdog
while Mujina has crashed or stopped controlling the PSU. A separate
bounded shutdown guard and cold-discovery test are now validated in
[PSU-ASIC-VALIDATION.md](PSU-ASIC-VALIDATION.md). Before continuous mining,
implement and validate long-running Mujina shutdown, cooling interlocks
and the remaining startup sequence in [BRINGUP.md](BRINGUP.md). The short
bench guard does not qualify unattended operation.
