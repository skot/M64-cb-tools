# Assembled-miner validation, 2026-11-03

For the subsequent stopped-miner checks, see
[MAINTENANCE-VALIDATION.md](MAINTENANCE-VALIDATION.md).

Tested the CB6 V5 / H616 controller in the assembled M64VL20 with its
P738B PSU, two hashboards and user-confirmed coolant circulation. Stock
AArch64 Linux 4.9.170 and `btminer` were already running. The PSU was
already enabled before testing; the tools did not enable it.

The board uses `CST-8` (UTC+08:00), so its log timestamps were November
4. This report uses November 3 in the operator's America/New_York
timezone. Board-local timestamps below are not UTC.

## Scope and operating state

This session used stock-driver telemetry, kernel EEPROM files, GPIO
status, UART counters and read-only stock API requests. No stock service
was stopped or restarted. No UART was opened by the tools, no driver
was unbound, and no reset, LDO, cooling or PSU control write was issued.
Native I2C/PMBus and active UART commands were deferred because stock
software retained ownership.

The same `btminer` PID remained running, with 39 threads and
`TracerPid: 0`. It reported 96 effective chips on each of four logical
slots, approximately 143–145 TH/s combined real-time hashrate, and
ongoing tuning (`Upfreq Complete=0`, `Hash Stable=false`). These are stock
API observations, not independent chip enumeration by the tools.

## Verified observations

| Interface | Observed result | Remaining boundary |
| --- | --- | --- |
| Tool deployment | All six binary checksums verified; full status capture completed with eight zero-exit commands | Capture success does not certify every optional firmware attribute |
| PSU identity | P738B, hardware R00008, software 250315.250311 | Native userspace SMBus/PEC path not exercised |
| PSU telemetry | Approximately 239.75 V AC, 16.8 A input; 22.5 V and 168–169 A output; 3.98–4.00 kW input and about 3.80 kW output | Independent electrical measurement and control-write validation remain pending |
| PSU fan and temperatures | `fan_speed0=6000` RPM; `temp0..2` approximately 29.2, 37.0 and 32.9 °C | Sensor calibration and actual fan-speed comparison not performed |
| PSU state | Enable remained 1; status/common-status were both zero in initial and final supported-node snapshots | Transient warning records occurred during supplementary unsupported-node reads; see below |
| Hashboard EEPROM 0 | 128-byte read matched the August reference SHA-256 exactly; stored CRC 0x22 versus computed 0x35 | Known mismatch is surfaced, not repaired |
| Hashboard EEPROM 1 | 128-byte read matched the August reference SHA-256 exactly; CRC 0x4e passed | No writes/programming tested |
| Hashboard temperatures | I2C10:0x48 approximately 23.25 °C; 0x49 approximately 25.94 °C; matched paired stock slot temperatures | Direct userspace sensor reads and calibration remain pending |
| Other external sensors | I2C4:0x4f LM75 approximately 26.4–26.5 °C; I2C4:0x48 NST117 approximately 21.25 °C | NST117 physical location/medium not established |
| SoC temperatures | CPU/GPU/VE/DDR thermal-zone reads approximately 37–40 °C | Calibration comparison not performed |
| Four UART chains | All four RX counters increased by about 0.52–0.57 MB in a three-second window; existing parity counters stayed at 1 | Stock traffic only; independent tool TX/RX, CRC handling and register transactions still need exclusive ownership |
| Reset and return GPIOs | GPIO201/202 outputs high; GPIO193/194 inputs high; `active_low=0` | No reset transitions or timing measurements performed |
| Switched logic supply | GPIO263 output high | No switching or voltage measurement performed |
| Cooling control channels | Both enabled, PWM 99%, zero-speed setting 0; all four tach attributes zero | Settings left unchanged; actual PWM/tach operation still needs measurement or appropriate attached equipment |
| LEDs and buttons | GPIO257/258/234/233 readable, all high in the snapshot | No live actuation or physical button presses; prior controller-only GPIO tests are documented separately |
| Ethernet | Link remained up at 100 Mb/s; SSH and read-only API queries worked | No throughput/packet-error test |
| TEE/watchdog/USB | TEE and watchdog device nodes present; runtime USB classes absent; keepalive stayed 2 | No TEE calls, watchdog experiment or USB/FEL test |

EEPROM reference hashes, board order 0 then 1:

```text
b15a6861c944150a72da9adab469980677c0f846294411e4fa7a049f64c0e0c0
938f9d152fd411df03087b9c80ea6e4e0e6bf24c9121d4f5b624cd56d0ae6015
```

## Unsupported telemetry and recorded warnings

Some generic nodes exist in sysfs but are not supported by this PSU
profile. Supplementary reads of indexed phase-voltage/current nodes
returned errors. At the same board-local timestamp, 03:26:09, stock
software recorded codes 264–267, 270–274 and 276–280. Each record cited
the same mask, `0xffffffffc0000000`, including a transfer error and
several derived warnings. The matching timing makes these unsupported
reads the suspected trigger; it does not independently establish the
PSU's electrical behavior during that instant.

Subsequent supported-node snapshots showed status and common-status
both zero, the PSU still enabled, normal telemetry and continuing
hashing. The sampled warning files retained their 03:26 modification
time rather than being refreshed during the later checks. Warning
records were preserved and were not cleared. A separate boot-time code
100002 with reason `antiv` was present before these tests and remains
unclassified here.

The tool now avoids generic phase-node probing and the unsupported
`/sys/bitmicro/power/water_temp` node by default. The cached
`/tmp/water_temp` value is reported only as raw vendor state and has not
been validated as a physical water-temperature measurement. Coolant
flow rate and thermal shutdown protection were not tested.

## Tool updates from this session

- PSU status discovers the confirmed indexed `fan_speed0` and
  `temp0..2` telemetry when aggregate nodes are absent.
- PSU output-power telemetry (`pout`) is included.
- Raw units are documented for the tested P738B firmware: centivolts,
  milliamps, deci-degrees Celsius, watts and RPM as appropriate. Other
  attributes, such as current limits, retain their own units.
- Default captures avoid the unsupported water-temperature read.
- Thirteen offline tests and Clippy passed; the AArch64 release bundle
  was rebuilt for deployment and read-only retesting.

Direct PMBus transactions, reset/LDO transitions, independent ASIC
register reads, cold startup and mining-work/nonce validation remain
separate maintenance-session tasks. They were not attempted against
the running miner.
