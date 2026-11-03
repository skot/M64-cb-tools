# M64 validation reports

The latest [maintenance-mode interface retest](RETEST-VALIDATION.md)
includes physical button presses and visible LEDs, GPIO/PWM transitions,
all four UARTs at discovery/mining baud, repeated EEPROM reads, driver
ownership refusals and the current boundaries on independent PSU control.
**Full independent control of all interfaces remains unverified.**

| Current coverage | Verified | Remaining |
| --- | --- | --- |
| Operator controls | Both visible LED colors; physical press/release of both buttons | Exact visible LED polarity; long-press actions deliberately inhibited |
| GPIO and cooling outputs | Both reset transitions, shared LDO command/readback, each PWM 99→98→99 | Powered reset returns, physical voltages/waveforms, tach reference and coolant flow |
| Four ASIC UARTs | Open/TX-ioctl acceptance, 375 kbaud/12 Mbaud driver readback, bounded timeout and exact termios restore | Physical TX/RX and powered CRC-valid ASIC replies |
| PSU, EEPROM and temperatures | Supported stock telemetry; repeated EEPROM reference matches; all eight hwmon/thermal reads | Native I2C/PEC and PSU controls blocked by kernel ownership; no independent PSU enable/protection |
| Platform ownership | Persistent vendor inhibition; PID 1 watchdog retained; Ethernet transfer; boot/taint/lock guards and Mujina-owner exclusion | USB/FEL and TEE functional calls; watchdog takeover/expiry; Mujina crash shutdown guardian |

For persistent vendor-service inhibition and rollback, see
[MAINTENANCE-MODE.md](MAINTENANCE-MODE.md). The original controller-only
bench report is preserved below; its remaining-work list describes that
earlier session, not the latest assembled-miner status.

# Controller-only bench validation, 2026-11-03

For the subsequent assembled-miner session, see
[POWERED-VALIDATION.md](POWERED-VALIDATION.md).

Tested on a CB6 V5 / Allwinner H616 control board running stock AArch64
Linux 4.9.170. The board had **no hashboards or PSU attached**. Tools were
deployed into `/tmp`, with checksums verified on the board. All six
static binaries executed successfully.

## Observed results

| Interface | Result | Limit of the check |
| --- | --- | --- |
| Ethernet | Link up, 100 Mb/s; SSH and binary transfer work | No throughput test |
| Red/green LED GPIOs | GPIO257/258 high and low writes matched value readback | Visible polarity was not independently observed; stock daemons continue managing LEDs |
| IP Found/reset buttons | GPIO234/233 input configuration and reads succeeded; both read 1 | No physical button presses requested |
| Cooling PWM | fan0 and fan1 each accepted 25% and 75%, with matching sysfs readback | No oscilloscope measurement or attached fan test |
| Cooling tachometers | All four kernel speed attributes read 0 | No rotating device to validate RPM |
| Environment LM75 | I2C4:0x4f hwmon temperature approximately 31.4–31.6 °C | Accessible without PSU/hashboards; physical sensor location remains unresolved |
| SoC temperatures | CPU, GPU, VE and DDR thermal-zone reads work, approximately 39–43 °C | No calibration comparison |
| Four UARTs | Each opened at 12 Mbaud, accepted the TX ioctl, then reached the bounded no-reply error | Expected with no ASICs attached; no electrical loopback, TX waveform or ASIC response validation |
| Hashboard I2C targets | 0x48/49 and 0x50/51 returned ENXIO | Expected absent devices; no sensor/EEPROM validation |
| Kernel-owned I2C targets | Native access to I2C4:0x58 and 0x4f rejected with EBUSY | Ownership alone does not establish physical PSU presence |
| PSU status | Missing identity reported as unavailable, exit 1 | No PSU control or telemetry validation |
| TEE/watchdog | Device nodes exist | No trusted-application calls, watchdog opens, feeds or reset tests |
| USB | Stock runtime USB sysfs classes absent | No host/FEL/gadget test |

The functional test restored both PWM settings to their original 0% and
restored the initial LED levels. Stock daemons may subsequently change
the LEDs. GPIO263 remained low, hashboard reset GPIOs remained
unexported, and the keepalive setting remained 1. No services were
stopped, no PSU control commands were issued, and no boot or persistent
firmware configuration was changed.

## Fixes established by this run

- Sysfs commands must be sent as one complete write. Formatting directly
  into a Rust `File` split the direction value and newline into separate
  writes: the first changed the direction, and the second returned
  EINVAL. The tool now assembles and submits one buffer and rejects short
  writes. LED, button and PWM commands were retested successfully.
- Disconnected PSU drivers publish `name=unknow`, `model=(null)` and
  sentinel voltages such as `6553500`. Status now rejects the missing
  identity before reporting telemetry.
- Stock SoC temperatures are under `/sys/class/thermal`, and UART
  counters are at `/proc/tty/driver/uart`. Inventory now discovers these.
- `capture-status.sh --controller-only` explicitly skips the PSU and
  hashboard EEPROM checks. The final controller capture completed with
  zero command failures and the intended skips.

## Remaining bench work

Attach the appropriate hardware before validating PSU PMBus telemetry and
controls, hashboard EEPROMs/sensors, cold ASIC initialization, register
responses, work submission, nonce handling, tachometer accuracy, and
actual PWM waveforms. GPIO readback and successful UART ioctls do not
establish electrical end-to-end operation.
