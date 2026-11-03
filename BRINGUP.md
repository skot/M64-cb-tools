# Hashboard power-up investigation

Bounded independent PSU control and pre-PLL cold discovery are now
validated on the assembled miner: see
[PSU-ASIC-VALIDATION.md](PSU-ASIC-VALIDATION.md). This document preserves
the investigation and remaining requirements for full Mujina startup;
continuous mining and production protection are not implemented here.

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

These mining-profile values do not establish an idle-test allowance.
Later guarded characterization reached about 21.1 V with resets held,
but power was still rising when the selected 150 W cutoff operated.
A steady idle-current bound remains unestablished. Successful short
discovery used explicit 7.5 A / 150 W / three-second limits and shut
down after the replies; those are not continuous-mining limits.

The retained `power_hal_set_enable` pseudocode raises GPIO263 before
calling the PSU enable callback. Its disable branch lowers GPIO263
before calling the PSU disable callback. Separately measured startup
timing establishes that the LDO rises about 1.012 seconds before reset
release. Exact-live library analysis subsequently confirmed the same
LDO-before-PSU ordering. The bounded test verified command/readback and
powered reset-return transitions; synchronized electrical measurements
of the rails and reset signals remain open.

The two controls remain distinct:

- `/sys/bitmicro/power/enable` controls high-power PSU output.
- GPIO263 controls the shared switched logic supply.
- GPIO201/202 hold the two physical hashboards in active-low reset.

Fresh analysis of the matching stock boot helper shows a one-second
water-mode wait after enable followed by enable readback, then powered
reset-return checks before UART initialization. That helper does not
call the separately named voltage-stability function. Our test additionally
requires repeated near-target voltage samples before releasing reset.
Reproducing only an enable write omits the rest of this sequence.

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
The later [interface retest](RETEST-VALIDATION.md) also verified the tools'
own termios2/BOTHER configuration at both 375000 and 12000000 on all four
UARTs, with matching vendor driver baud/divisor readback and exact
termios restoration. Physical TX/RX and powered ASIC replies remain open.

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

A later bounded experiment successfully inserted status/model reads
after the captured 1832-frame pre-PLL prefix on all four slots, producing
96 complete CRC-valid IDs per query. This establishes a working early
read on this unit, not a minimal prefix or proof that cores were disabled.
The allowed registers are not all semantically mapped. No known PLL,
core-control or mining-work register writes were used for this test.

## Implemented gates and remaining startup work

1. The bounded trial owns PSU/GPIO/UART operations under the shared
   diagnostic lock, retained by its independent guard through shutdown.
   Kernel PSU and temperature drivers remain attached.
2. Trials require measured output off, both reset outputs/returns low
   and LDO off. Established stock command ordering is reproduced;
   physical rail and signal timing still needs measurement.
3. Driver-backed setpoint and disable are verified. Trials use the
   supported 21 V boot target and verify voltage feedback before reset
   release; the policy minimum is not used as a guessed ID voltage.
4. The separate process now enforces bounded duration and selected
   current/power, fault and temperature cutoffs. A steady idle envelope,
   long-running Mujina heartbeat/shutdown protection and verified
   coolant-flow/thermal interlocks remain required for continuous work.
   Software cutoff timing depends on responsive kernel I/O.
5. Pre-PLL discovery at 375000 baud / shift 0 works on all four slots.
   The validated captured prefix is not minimal. Powered 12 Mbaud
   transition, PLL/core setup and mining work/nonces remain unverified.
6. Both status and model queries returned all 96 CRC-valid IDs per slot.
   Preserve these complete-set requirements in further initialization.
7. The trial restores resets, LDO and PSU output off and verifies zero
   driver voltage/current on normal exit, timeout and tested faults.
   Parent death is handled by the separate guard; the temporary setpoint
   may need explicit restoration after parent SIGKILL.
8. Treat any changed boot ID or kernel exception as a new session. The
   vendor supervisor can reboot after a warning, and ordinary stock
   boot can resume mining. Boot-ID checks alone do not prevent that
   automatic startup. The optional [maintenance policy](MAINTENANCE-MODE.md)
   now provides reboot-tested inhibition of the selected stock startup
   and respawn paths. It does not provide the independent power guardian.

The tools now expose boot ID, uptime and kernel taint in inventory.
Hardware-session commands reject a nonzero taint value, and the global
`--expect-boot-id ID` option refuses a different controller boot. These
guards are implemented and tested. The later bounded power guard and
test results are described in [PSU-ASIC-VALIDATION.md](PSU-ASIC-VALIDATION.md);
they do not constitute the full production mining/protection controller.

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
