# M64 interface map and validation boundaries

Target: M64VL20 / CB6 V5 SL V2.0. Other revisions need an independently
verified profile. Controller-only checks were exercised on 2026-11-03;
see [VALIDATION.md](VALIDATION.md). Subsequent live assembled-miner
observations are in [POWERED-VALIDATION.md](POWERED-VALIDATION.md).
Exclusive PSU-off checks are in
[MAINTENANCE-VALIDATION.md](MAINTENANCE-VALIDATION.md).
The [maintenance-mode retest](RETEST-VALIDATION.md) adds physical
button/LED observations and two-rate UART driver readback. The latest
[PSU/ASIC validation](PSU-ASIC-VALIDATION.md) verifies guarded retained-driver
power control and complete cold ASIC replies; full mining/protection
startup is still separate work.
For Mujina replacement, USB/FEL and TEE functional tests are outside the
required scope. Tach inputs require functional qualification only if
used by the installed cooling/protection system; inventory is retained.

| Interface | Mapping | Implemented check | Remaining validation |
| --- | --- | --- | --- |
| ASIC slot 0 | `/dev/ttyS1`, physical board 0 | Powered 375 kbaud cold status/model replies: all 96 IDs, CRC valid; termios restored; 12 Mbaud host setup verified | Powered 12 Mbaud transition, PLL/core setup, work/nonces and waveform measurement |
| ASIC slot 1 | `/dev/ttyS4`, physical board 0 | Same complete 96-ID powered cold checks passed | Same |
| ASIC slot 2 | `/dev/ttyS2`, physical board 1 | Same complete 96-ID powered cold checks passed | Same |
| ASIC slot 3 | `/dev/ttyS3`, physical board 1 | Same complete 96-ID powered cold checks passed | Same |
| Reset | GPIO201/202, active low, physical boards 0/1 | Guarded powered release after voltage gate; selected return high, other board low; reset restored on exit | Electrical levels/timing via scope |
| Reset return | GPIO193/194 | Both powered low→high→low paths exercised with selected physical-board reset | Independent voltage/propagation timing measurement |
| Shared switched LDO | GPIO263, active high | On/off command readback and LDO-before-PSU sequence used in successful guarded cold discovery | Physical voltage/settling measurement |
| Hashboard EEPROM | I2C10, `0x50`/`0x51`, BL24C02 | Repeated kernel images/reference hashes matched in maintenance mode; board-0 CRC mismatch unchanged | Programming untested. Historical native read matched, but driver rebind caused warning/reboot; use kernel paths |
| Hashboard temperature | I2C10, `0x48`/`0x49`, LM75 class | Hwmon reads matched stock paired-slot temperatures | Native reads and calibration comparison |
| Environment sensor | I2C4, `0x4f`, LM75 class | Hwmon read works with PSU and hashboards absent | Exact physical sensor location/medium |
| P738B PSU | I2C4, `0x58` | Stock telemetry, verified disable/output-off setpoint roundtrip and bounded 21 V enable/cutoff; separate child guard; driver remains attached | Native userspace PEC still EBUSY; steady idle characterization, calibration and continuous-mining protection |
| Cooling channels | `/sys/bitmicro/fan/fan0` and `fan1` | Each PWM 99→98→99 readback verified; enables 1, zero-speed 0; all four RPM attributes readable | Actual channel/tach wiring, waveform and reference RPM; no flow certification |
| LEDs | Red GPIO257, green GPIO258 | Raw high/low readback and operator-confirmed separate red/green illumination with vendor services stopped | Exact visible polarity/order and electrical measurement |
| Buttons | IP Found GPIO234, reset GPIO233 | Operator presses recorded 1→0→1 on each input | Long-press behavior deliberately inhibited |
| SoC temperature | Named hwmon and thermal-zone nodes | Attribute discovery/read | Thermal-zone reads verified on stock Linux |
| Ethernet | `/sys/class/net/*` | 100 Mb/s link and SSH deployment verified | Throughput/packet loopback not implemented |
| USB | `/sys/bus/usb/devices`, `/sys/class/udc` | VID/PID/product and gadget state observation | Functional tests outside required Mujina scope |
| TEE | `/dev/tee0`, `/dev/teepriv0` | Presence only | Functional calls outside required Mujina scope |
| Watchdog/keepalive | PID 1 watchdog descriptor, read-only `ubus` status; vendor keepalive sysfs | OS watchdog running 16 s / feed 5 s; vendor keepalive 0; bounded trial guard handles parent loss | Watchdog sysfs absent; no takeover/expiry or long-running Mujina crash-protection test |
| BTROM | I2C10 config targets `0x18–0x1b`, `0x1e`, `0x1f` | Not probed | Physical mapping and authenticated protocol |
| J1 cooling port | Ground confirmed; T43 25 kHz PWM candidate; T41/T42 unresolved | No electrical test | Continuity and waveform measurements |

UART connector pin continuity is high-confidence in the reference notes
but still lacks independent confirmation. Host mining baud is 12 Mbaud
from live evidence; the older connector table's nominal 6 Mbaud describes
the ASIC configuration and must not replace the observed host setting.

## Evidence

The evidence comes from the original M64 research checkout. Its source
filenames are listed below for traceability; that checkout is not
required to build or run these tools.

| Source in that checkout | Facts used |
| --- | --- |
| `controlboard-pinouts.md` | UART slot order, connector/I2C mapping, EEPROM capacity, reset polarity and observed 1.012 s LDO delay |
| `M64-P738B-communications.md` | PEC-valid PMBus commands, LINEAR11, VOUT_MODE=-9 and sysfs attributes |
| `M64-BT1980-communications.md` | Aligned ID/status reads, reply layout and CRC |
| `M64-UART-CAPTURE.md` | Logical/physical rotation and vendor TX ioctl |
| `spec/06-bt1980-driver.md` | 96 chips per VL20 chain, 12 Mbaud live host setting |
| `spec/07-power-thermal-fan.md` | Water cooling, temperature sources and fan behavior |
| `spec/11-eeprom-btrom-security.md` | EEPROM offsets, CRC scope and known stock CRC mismatch |
| `whatsminerM64.html` | LED/button GPIOs, cooling sysfs, hwmon and platform inventory |
| `tools/m64_wupt_retention_gate.sh` | Independently used GPIO234/257/258 mapping |

No vendor executable code is linked or required. The implementation uses
the documented interface behavior and small capture vectors.
