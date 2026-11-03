# PSU-off maintenance validation, 2026-11-03

**Correction after reviewing the next boot:** the EEPROM driver rebind
caused a vendor-kernel warning, which triggered an automatic supervisor
reboot and resumed stock mining. The native reads matched, but this was
not a clean or stable driver handoff. Do not repeat the unbind/rebind
procedure on this stock kernel.

This session followed the assembled-miner read-only checks. The operator
requested stopping `btminer` to permit exclusive hardware access.

## Stock shutdown and observed state

The installed `/usr/bin/poweroff.sh` and its service/helper dependencies
were inspected before use. On this firmware, that script stops mining,
creates `/tmp/was_poweroff`, requests log preservation and disables the
PSU hashboard output; it does not shut down the controller. Its paired
power-on helper starts stock mining again, but was not executed.

The stock shutdown completed at 03:37 board-local time on November 4
(UTC+08:00), corresponding to November 3 in America/New_York. It printed an `unknown operand`
warning and a redundant `killall` message; independent checks confirmed
the intended shutdown state:

- `btminer` absent; the existing `system-monitor` PID remained running.
- PSU enable 0, measured output voltage 0 and output current 0.
- PSU status and common-status both `0x0`.
- Keepalive 1, the stock stopped-miner setting.
- Hashboard resets GPIO201/202 low, reset-return GPIO193/194 low, and
  shared switched LDO GPIO263 low.
- Both cooling PWM channels remained at 99%.

The controller stayed reachable through the final 03:40 check, but an
automatic reboot had already been queued. No power-on command or
voltage-setting command was issued by the tools.

## Exclusive-access checks

| Check | Result | Limit |
| --- | --- | --- |
| Button inputs | GPIO234/233 input commands and reads passed; both read high | No physical presses tested |
| Cooling PWM writes | Each channel accepted its existing 99% setting with matching readback | No cooling-level change or waveform measurement |
| Reset outputs | Each physical board's reset output accepted release/high and assert/low with matching GPIO readback | ASIC reset propagation not established with high-power output off |
| Reset returns | GPIO193/194 stayed low during the PSU-off reset tests | A powered-chain propagation test remains necessary |
| Shared switched LDO | On/readback passed with both resets asserted; off/readback passed afterward | GPIO command/readback only; no physical voltage measurement |
| Kernel EEPROM reads | Both 128-byte images readable after shutdown | Board 0 retained its known CRC warning |
| Temperatures and PSU telemetry | Supported stock-driver reads continued to work with output off | Native PSU and sensor drivers remained attached |
| Native probe ownership | All six configured I2C targets returned EBUSY while their kernel drivers were attached | Stopping a userspace miner does not release kernel I2C ownership |

Before each test phase, the script checked that `btminer` was absent and
PSU enable, measured output voltage and output current were all zero.
Cleanup restored both resets asserted and the switched LDO off.

## Native EEPROM handoff

With output still verified off, only the two EEPROM devices, `10-0050`
and `10-0051`, were temporarily unbound from `at24`. The temperature
drivers and PSU `pmbus-power` driver were not unbound.

The missing kernel EEPROM nodes made `hashboard-m64 eeprom` use its
native `/dev/i2c-10` combined-transfer fallback. Both complete decoded
outputs, including all 128 raw bytes and CRC results, matched the
kernel-driver reads exactly. Both EEPROM drivers were then rebound;
the restored kernel paths returned identical content again.

The native read data matched, without forced I2C access or EEPROM
programming, and the known board-0 CRC mismatch remained unchanged.
However, rebinding the driver was unsafe on this vendor kernel: its
extra `/sys/bitmicro/eeprom` registration was not cleaned up normally,
and the rebind attempted to create an existing node.

## Subsequent automatic reboot

The operator confirmed that they did not reboot or power-cycle the
controller. Saved logs establish this sequence, in board-local
UTC+08:00 time:

```text
03:39:39.82 kernel: cannot create duplicate filename '/bitmicro/eeprom/eeprom1'
03:39:39    Prepare to reboot as exception occurred
03:41:10    Reboot as exception occurred
```

The saved `last_exception_sys` snapshot contains the kernel warning and
stack trace during the rebind shell process. After reboot, the board
had a new boot ID, `/tmp/was_poweroff` was absent, and stock `btminer`
was running with PSU output enabled again. This is evidence of a
controller-requested exception reboot; the earlier PSU transfer-warning
records do not establish that a PSU watchdog caused this reboot.

This overturns the earlier interpretation of the final-state snapshot
as a lasting maintenance state. Kernel driver rebinding must not be
used as a routine workaround for I2C ownership on this stock firmware.

## Final state and remaining work

The final check at 03:40 board-local time confirmed:

```text
btminer                 stopped
system-monitor          running
PSU enable/Vout/Iout     0 / 0 / 0
PSU status/common       0x0 / 0x0
keepalive               1
reset outputs 201/202   0 / 0
switched LDO 263        0
cooling PWM 0/1         99% / 99%
EEPROM drivers          both attached to at24
board-temperature       both attached to lm75
PSU driver              attached to pmbus-power
intentional-off marker  present
```

The snapshot above was accurate when taken, but did not persist through
the queued reboot. The miner subsequently resumed stock startup.
Active ASIC UART/register checks need a separately planned powered
bring-up. Direct sensor I2C and native PSU SMBus/PEC paths remain
unverified; the tools do not bypass the attached kernel drivers. No
watchdog, TEE, USB/FEL, physical button, tachometer or signal-waveform
experiment was added in this session.
