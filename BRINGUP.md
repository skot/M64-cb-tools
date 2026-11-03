# Hashboard power-up investigation

This is an evidence and implementation plan, not a validated independent
cold-start recipe. Stock startup works on the assembled miner; the
current tools do not yet implement all of its transport and protection
behavior.

## Current finding: why mining resumed

The operator did not reboot the board after the PSU-off tests. Saved
logs show a duplicate vendor EEPROM sysfs node during an `at24` rebind
at 03:39:39 board-local time (UTC+08:00), followed by an explicit
supervisor-requested exception reboot at 03:41:10. Stock startup then resumed mining and enabled
the PSU. The RAM-only intentional-off marker was gone after reboot.

The earlier PSU watchdog-warning record was timestamped 03:26:09 in
the same board-local timezone,
alongside the unsupported telemetry-probe errors. It does not establish
that a PSU watchdog caused the later reboot. The saved exception and
software-reboot log provide the direct evidence for that event.

Do not repeat kernel driver unbind/rebind on this vendor firmware to
work around I2C ownership. In particular, successful data reads and
restored standard driver links do not establish that the extra vendor
sysfs state was restored correctly.

## Established stock power path

The inspected `/usr/bin/poweron.sh` removes `/tmp/was_poweroff` and
starts the stock `btminer` service. That service sets keepalive mode 2
and starts the original daemon. This resumes stock initialization and
mining, rather than providing exclusive access for the diagnostic tools.
The script was inspected, not executed during this investigation;
stock mining was already running again when the board was checked.

The P738B profile establishes these policy settings:

| Setting | Value | Meaning |
| --- | --- | --- |
| Normal boot target | 2100 centivolts = 21 V | Stock normal-start setting |
| High-power boot target | 2200 centivolts = 22 V | Separate high-power setting |
| Policy minimum | 1500 centivolts = 15 V | Not a demonstrated ID-test voltage |
| Normal power limit | 4200 W | Mining profile; not an idle-test allowance |
| Output current profile | 295 A, maximum 305 A | Mining limits; not an idle-test threshold |

These values do not by themselves qualify a low-energy independent
test. No measured reset-held current bound was established in the
reviewed evidence.

The retained `power_hal_set_enable` pseudocode raises GPIO263 before
calling the PSU enable callback. Its disable branch lowers GPIO263
before calling the PSU disable callback. Separately measured startup
timing establishes that the LDO rises about 1.012 seconds before reset
release. The complete synchronized relationship between PSU output
stability and all reset/LDO transitions still needs to be recorded.

The two controls remain distinct:

- `/sys/bitmicro/power/enable` controls high-power PSU output.
- GPIO263 controls the shared switched logic supply.
- GPIO201/202 hold the two physical hashboards in active-low reset.

The stock software also performs voltage ramping/stability checks and
ASIC initialization. Reproducing only the enable write omits those
operations.

## Voltage encoding

Do not mix the policy increment with native PMBus encoding:

```text
Kernel sysfs target: 21 V = 2100 centivolts
Native PMBus target: 21 V = 10752 = 0x2a00, with VOUT_MODE 0x17
Native wire unit:    1/512 V per LSB
Profile step 2:      a 20 mV policy increment, not a PMBus wire LSB
```

## Cold discovery is different from the existing check command

The tools' `hashboard-m64 check` is an initialized-chain register read.
Its normal 12 Mbaud, shift-3 setting must not be used as a presumed
cold-discovery configuration.

Retained code establishes a logical default baud of 375000 for the
24 MHz oscillator / BT1980 family and discovery shift 0. The captured
`uart_open` second argument is chip count 96, not a baud or timeout.

Offline native analysis establishes that the vendor `tty_set_baud`
maps logical 375000 to `stty` proxy speed 110 and logical 12000000 to
proxy speed 134. H616 UART opening can configure the shared APB2 clock
through vendor debugfs, selecting 192 MHz for this profile when needed.
Read-only inspection of the running board found:

```text
stty for all four ASIC UARTs: 134
APB2 clock:                  192000000 Hz
ttyS1 ctrl_info last baud:   12000000, divisor 1
```

Thus the running 12 Mbaud observation is supported, but ordinary
termios readback is not enough to validate cold clock configuration.
An output-off check then applied the supported stock proxy 110 to each
UART without sending data. All four driver reports showed baud 375000
and divisor 32 with APB2 at 192 MHz. Every saved termios configuration
was restored afterward. This confirms the programmed discovery rate
on this kernel/clock setup; it is not a scope measurement of the wire.
No factor-of-two discovery mapping is needed for the observed setup.

The stock shutdown for this check completed at 03:56:58 board-local
time (UTC+08:00). Output
enable, measured voltage and output current remained zero, and kernel
taint remained zero after the UART check. No PSU-enable or voltage
write was issued during the investigation.

Through 04:02:04 board-local time, over five minutes after this clean
shutdown, the boot ID was unchanged, kernel taint remained zero,
`btminer` was absent and output remained 0 V / 0 A. That observation
corroborates the recorded exception/reboot sequence; it does not prove
that every possible PSU-watchdog behavior has been tested.

The initialization capture also contains details absent from a simple
prefix-plus-96-address experiment:

- Per-slot prefix values differ.
- A 0..383 assignment pass is followed by a 0..95 pass, with repeated
  configuration broadcasts.
- The first captured status read follows many core-control writes.
- The captured identity read occurs after the baud/rotation transition.

An earlier core-disabled identity gate is an experiment to design and
validate, not a captured proven minimal recipe. No mining work or core
enable sequence should be replayed merely to obtain an ID response.

## Requirements before independent energization

1. Establish an explicit owner for PSU, GPIOs and all four UARTs. Leave
   the kernel PSU and temperature drivers attached.
2. Use a verified output-off starting state, with both resets asserted,
   and establish the required LDO/PSU ordering from synchronized stock
   evidence before writing a new startup controller.
3. Set and verify a profile-supported boot target while output is off;
   do not extrapolate a discovery voltage from the policy minimum.
4. Add bounded enable duration, validated idle-current/power bounds,
   supported fault/temperature reads and a separate shutdown guardian.
   Preserve coolant circulation; existing temperature readings do not
   prove flow rate or thermal protection.
5. Reproduce the verified 375000 discovery baud / shift-0 transport on
   the correct shared-clock setup, and resolve the minimum discovery
   sequence without enabling unrestricted hashing.
6. Require complete, CRC-valid discovery and identity sets before
   progressing to further configuration or diagnostic work.
7. Assert resets, disable output and verify measured voltage/current
   have fallen on every exit, timeout or fault.
8. Treat any changed boot ID or kernel exception as a new session. The
   vendor supervisor can reboot after a warning, and ordinary stock
   boot can resume mining. Boot-ID checks alone do not prevent that
   automatic startup. The optional [maintenance policy](MAINTENANCE-MODE.md)
   now provides reboot-tested inhibition of the selected stock startup
   and respawn paths. It does not provide the independent power guardian.

The tools now expose boot ID, uptime and kernel taint in inventory.
Hardware-session commands reject a nonzero taint value, and the global
`--expect-boot-id ID` option refuses a different controller boot. These
guards are implemented and tested; they do not constitute the power-up
guardian or an output-enable command. Fifteen offline tests pass, and
the deployed boot-ID guard rejected an intentionally mismatched ID.

The miner was initially left stopped after the output-off baud check: both resets
asserted, switched LDO off, cooling still at 99%, supervisor running and
PSU hashboard output at 0 V / 0 A. As the earlier reboot demonstrated,
that is a point-in-time state, not a persistent startup inhibition.
The later maintenance-mode session installed and reboot-tested the
persistent inhibition described in [MAINTENANCE-MODE.md](MAINTENANCE-MODE.md).

## Evidence and limitations

Existing research sources: `controlboard-pinouts.md`,
`M64-P738B-communications.md`, `M64-BT1980-communications.md`,
`re/decompiled/lib_power.c` and `re/decompiled/lib_core.c`. Saved runtime
evidence: `/data/logs/last_exception_sys`, supervisor reboot log entries,
current UART settings/clock readback and the operator's confirmation.

The retained library SHA-256 is:

```text
2c2b94c966bf0fb1034ca4d06e406edbf82a8a69bb35f8bc56ba778dc62f8674
```

REA/Ghidra offline function Evidence:

- `tty_set_baud`, address `0x13f340`:
  `ev_147505aea9ba67414df6807f38cacbf1d3b8424a1b80d707106985148e5010c4`
- `h6_tty_open`, address `0x13f540`:
  `ev_e39b0f0f667fce9e71180bf1710465e2b362f4ecab9d467f07ab6dcef6ecd566`

These are decompiler observations, not original source or live proof of
every branch. Complete Evidence JSON is retained locally under the
ignored `target/bringup-investigation` directory. No vendor binary is
distributed with these tools.
