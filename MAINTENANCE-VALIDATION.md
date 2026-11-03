# PSU-off maintenance validation, 2026-11-03

This session followed the assembled-miner read-only checks. The operator
requested stopping `btminer` to permit exclusive hardware access.

## Stock shutdown and observed state

The installed `/usr/bin/poweroff.sh` and its service/helper dependencies
were inspected before use. On this firmware, that script stops mining,
creates `/tmp/was_poweroff`, requests log preservation and disables the
PSU hashboard output; it does not shut down the controller. Its paired
power-on helper starts stock mining again, but was not executed.

The stock shutdown completed at 03:37 UTC on November 4, corresponding
to November 3 in America/New_York. It printed an `unknown operand`
warning and a redundant `killall` message; independent checks confirmed
the intended shutdown state:

- `btminer` absent; the existing `system-monitor` PID remained running.
- PSU enable 0, measured output voltage 0 and output current 0.
- PSU status and common-status both `0x0`.
- Keepalive 1, the stock stopped-miner setting.
- Hashboard resets GPIO201/202 low, reset-return GPIO193/194 low, and
  shared switched LDO GPIO263 low.
- Both cooling PWM channels remained at 99%.

The controller stayed reachable and did not reboot during the checks.
No power-on command or voltage-setting command was issued.

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

This validates the native I2C EEPROM read path against real hardware,
without forced I2C access or EEPROM programming. The known board-0 CRC
mismatch remained unchanged.

## Final state and remaining work

The final check at 03:40 UTC confirmed:

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

The miner was deliberately left in this stopped, output-off state.
Active ASIC UART/register checks need a separately planned powered
bring-up. Direct sensor I2C and native PSU SMBus/PEC paths remain
unverified; the tools do not bypass the attached kernel drivers. No
watchdog, TEE, USB/FEL, physical button, tachometer or signal-waveform
experiment was added in this session.
