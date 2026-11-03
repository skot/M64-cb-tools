# M64 interface map and validation boundaries

Target: M64VL20 / CB6 V5 SL V2.0. Other revisions need an independently
verified profile. Controller-only checks were exercised on 2026-11-03;
see [VALIDATION.md](VALIDATION.md). Subsequent live assembled-miner
observations are in [POWERED-VALIDATION.md](POWERED-VALIDATION.md).
Exclusive PSU-off checks are in
[MAINTENANCE-VALIDATION.md](MAINTENANCE-VALIDATION.md).

| Interface | Mapping | Implemented check | Remaining validation |
| --- | --- | --- | --- |
| ASIC slot 0 | `/dev/ttyS1`, physical board 0 | CRC-checked ID/register read | Vendor ioctl/termios behavior; cold startup |
| ASIC slot 1 | `/dev/ttyS4`, physical board 0 | Same | Same |
| ASIC slot 2 | `/dev/ttyS2`, physical board 1 | Same | Same |
| ASIC slot 3 | `/dev/ttyS3`, physical board 1 | Same | Same |
| Reset | GPIO201/202, active low, physical boards 0/1 | PSU-off assert/release command readback verified | Powered return propagation and electrical timing |
| Reset return | GPIO193/194 | Read exported raw level | GPIO194 not directly captured in reference notes |
| Shared switched LDO | GPIO263, active high | PSU-off on/off command readback verified with both resets asserted | Physical voltage/settling measurement |
| Hashboard EEPROM | I2C10, `0x50`/`0x51`, BL24C02 | Kernel and native reads matched exactly after EEPROM-only driver handoff; bindings restored | No programming tested; board-0 CRC mismatch unchanged |
| Hashboard temperature | I2C10, `0x48`/`0x49`, LM75 class | Hwmon reads matched stock paired-slot temperatures | Native reads and calibration comparison |
| Environment sensor | I2C4, `0x4f`, LM75 class | Hwmon read works with PSU and hashboards absent | Exact physical sensor location/medium |
| P738B PSU | I2C4, `0x58` | Live stock-driver telemetry including indexed fan and temperatures | Native SMBus/PEC and controls; unsupported generic nodes can affect error reporting |
| Cooling channels | `/sys/bitmicro/fan/fan0` and `fan1` | PWM percentage and kernel `speed0/1` reads | Actual channel/tach wiring and enabled state |
| LEDs | Red GPIO257, green GPIO258 | Raw high/low and status | Confirm visible polarity and daemon ownership |
| Buttons | IP Found GPIO234, reset GPIO233 | Input export/read | Confirm press polarity; avoid stock long-press actions |
| SoC temperature | Named hwmon and thermal-zone nodes | Attribute discovery/read | Thermal-zone reads verified on stock Linux |
| Ethernet | `/sys/class/net/*` | 100 Mb/s link and SSH deployment verified | Throughput/packet loopback not implemented |
| USB | `/sys/bus/usb/devices`, `/sys/class/udc` | VID/PID/product and gadget state observation | External host/FEL test not implemented |
| TEE | `/dev/tee0`, `/dev/teepriv0` | Presence only | No trusted-application calls |
| Watchdog/keepalive | `/sys/class/watchdog`, `/sys/bitmicro/keepalive/enable` | Read state/identity/timeout/nowayout | No open/feed/reset experiment |
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
