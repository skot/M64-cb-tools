# Maintenance-mode interface retest, 2026-11-03

**Full independent control of every M64 interface is not yet verified.**
The tools control the tested GPIO/PWM outputs, read the attached EEPROMs
and stock-driver telemetry, and configure all four ASIC UARTs. Native
PMBus/sensor access remains blocked by attached kernel drivers; powered
ASIC communication and a protected independent PSU startup remain open.

For the Mujina replacement goal, USB/FEL and TEE functional validation
are outside the required scope by operator direction. Their inventory
observations below are retained for completeness. Native userspace I2C
ownership is also not inherently required if Mujina uses the retained
kernel drivers; the required result is verified control and protection
through the chosen backend.

This pass followed installation of [persistent maintenance mode](MAINTENANCE-MODE.md)
on the assembled M64VL20 / H616 / CB6 V5 controller at `192.168.1.179`.
The main hardware sweep ran from 20:30:22 to 20:34:28 UTC on November 3
(04:30:22–04:34:28 November 4 board-local, UTC+08:00). Later checks
validated the updated binary ownership guard. External coolant
circulation was operator-reported, not measured by these tools.

PSU hashboard output stayed disabled at measured 0 V / 0 A throughout.
No output-enable, PSU setpoint change, I2C force-access, driver unbind or
driver rebind was performed. No reboot was requested during this pass.

## Interface results

“Verified” below refers to the stated software/readback or physical
observation. A successful ioctl or GPIO readback is not an electrical
measurement of a connector or rail.

| Interface | Retest evidence | What remains |
| --- | --- | --- |
| Red/green LEDs, GPIO257/258 | Each output toggled low/high with matching value readback. Operator observed both colors lighting separately during the pattern. Original raw levels restored: red 0, green 1. | Exact visible polarity/order was not separately recorded; connector waveform not measured. |
| IP Found button, GPIO234 | Input configuration/read passed. Operator pressed/released it; 200 ms sampling recorded `1 → 0 → 1`. | Long-press actions deliberately inhibited; no stock button handler tested. |
| Reset button, GPIO233 | Operator pressed/released it after IP Found; sampling recorded `1 → 0 → 1`. Both returned high. | This verifies input sensing, not factory-reset or reboot behavior. |
| Cooling PWM channels 0/1 | Each accepted `99 → 98 → 99` percent with matching readback. One-second 98% intervals used with PSU output off. Both ended enabled, zero-speed 0, PWM 99%. | No waveform/duty calibration or pump/flow measurement. Arbitrary enable/zero-speed transitions were not tested; maintenance boot restoration of `1/0/99` was verified in the preceding session. |
| Four tachometer inputs | `speed0/1` read successfully on both channels; all returned 0. | No attached tach source or reference RPM measurement; zero is not proof of working tach inputs. |
| Physical-board resets, GPIO201/202 | Each released high and asserted low independently, with direct GPIO readback after each command. Both ended low. | No powered ASIC reset propagation or electrical timing measurement. |
| Reset returns, GPIO193/194 | Both read low during the output-off reset tests. | Response to powered reset release remains unverified. |
| Shared switched logic rail, GPIO263 | On/readback 1 with both resets asserted, then off/readback 0. Tool applied its 1013 ms settling wait. | Rail voltage, current and physical settling time not measured. |
| EEPROM 0, I2C10:0x50 | Repeated 128-byte kernel reads and decoded outputs matched before/after testing and matched the retained reference hash. | Existing CRC warning unchanged: stored `0x22`, calculated `0x35`. No programming performed. |
| EEPROM 1, I2C10:0x51 | Repeated kernel reads and decoded outputs matched before/after and matched reference hash; CRC `0x4e` passed. | No EEPROM programming test. Native fallback not repeated because the earlier rebind was unsafe. |
| Hashboard LM75s, I2C10:0x48/49 | Both hwmon paths read about 20.3 °C at baseline. Drivers remained attached. | Native access returned EBUSY; no calibration or thermal-response experiment. |
| Other external temperature sensors | I2C4:0x4f LM75 read about 23.7 °C; I2C4:0x48 NST117 read about 20.2 °C. | Sensor location/medium and calibration remain unconfirmed. Native 0x4f access returned EBUSY. |
| SoC temperatures | CPU/GPU/VE/DDR thermal-zone reads succeeded, about 33.6–35.7 °C at baseline. | Calibration and thermal shutdown not tested. |
| P738B stock telemetry | Identity, firmware/hardware revision, supported electrical/fan/temperature/status nodes readable. Enable/Vout/Iout remained `0/0/0`; status/common-status remained `0x0/0x0`. Fan reported 6000 RPM. | Independent electrical/fan calibration; unsupported generic nodes deliberately avoided. |
| P738B native identity/status | Both commands refused target selection with EBUSY at I2C4:0x58. | No live userspace PEC/block/telemetry validation; driver ownership is not a transaction pass. |
| P738B native disable/set-voltage | Both commands refused target selection with EBUSY **before any PMBus control write**. The attempted setpoint argument was 18550 mV, matching the observed raw stock `vout_set=1855`. | Neither independent disable nor voltage programming was demonstrated. No output-enable command exists. Do not detach the stock driver to bypass this boundary. |
| ASIC slot 0, `/dev/ttyS1` | Open, termios2, vendor TX ioctl and bounded no-reply path exercised at 375000/shift 0 and 12000000/shift 3. Driver readback confirmed both rates; termios restored exactly. | No CRC-valid ASIC replies, electrical TX/RX loopback or powered identity set. |
| ASIC slot 1, `/dev/ttyS4` | Same two-rate transport/readback/restore checks passed. | Same powered/electrical boundary. |
| ASIC slot 2, `/dev/ttyS2` | Same two-rate transport/readback/restore checks passed. | Same powered/electrical boundary. |
| ASIC slot 3, `/dev/ttyS3` | Same two-rate transport/readback/restore checks passed. | Same powered/electrical boundary. |
| Ethernet | SSH, binary/evidence transfer and status capture worked at reported 100 Mb/s; RX/TX error and drop counters stayed zero across the captured interval. | No sustained throughput, packet loopback or link-mode changes tested. |
| Vendor keepalive/supervisors | Keepalive stayed 0; selected vendor applications stayed absent in the same boot. Persistent policy remained armed. | This pass did not repeat the preceding controlled-reboot tests or exercise live rollback. |
| Hardware watchdog | Read-only `ubus` status: running, timeout 16 s, feed interval 5 s, magicclose false. PID 1 retained its watchdog descriptor. | No watchdog takeover, deliberate expiry or Mujina crash protection test. Generic watchdog sysfs attributes are absent on this firmware. |
| USB/FEL/gadget | Inventory command ran; stock runtime USB/UDC classes absent. | Outside required Mujina validation scope; no external transaction or mode change tested. |
| TEE | `/dev/tee0` and `/dev/teepriv0` present. | Outside required Mujina validation scope; no trusted-application operation tested. |
| BTROM and J1 pin mapping | No additional protocol or electrical test. | BTROM protocol, exact J1 tach/reference pins and PWM waveform remain outside implemented validation. |

The stock PSU `errors` node is absent. Virtual network interfaces reject
some `carrier`/`speed` reads, and `/tmp/miner_type` is absent while
`/tmp/eeprom/miner_type` identifies M64_VL20. These inventory `UNAVAILABLE`
entries are retained; they are not physical-device failure diagnoses.

## UART and ownership details

APB2 read 192000000 Hz. During each of the eight baud tests, the vendor
driver's `ctrl_info` reported:

```text
375000 baud:   divisor 32
12000000 baud: divisor 1
```

Each test used a 2000 ms receive deadline, returned nonzero for no
CRC-valid replies, and restored its original termios configuration.
Output and logic power were off and resets asserted. These expected
errors verify bounded failure handling, not working ASIC chains. The
custom-ioctl requests did not increment the standard UART TX counters;
ioctl acceptance therefore must not be presented as measured wire TX.

All six targets in `i2c-probe` returned EBUSY, and all six driver links
matched their pre-test snapshots. Native temperature and PSU commands
failed at target selection; stopping vendor userspace does not release
kernel I2C ownership. No native read/write was counted as a success.

## Guards, tool update and evidence

- Shared-LDO enable was refused with one reset released; GPIO263 stayed
  low. Both resets were then asserted again.
- An incorrect expected boot ID refused an LED mutation.
- While a UART test held the diagnostic lock, a second mutation was
  refused. An overlapping LED pattern was likewise blocked; after the
  UART phase, cleanup restored the saved output state. The operator
  confirmed both LED colors from the visible pattern.
- Code review found that the ownership guard recognized the three stock
  miner names but omitted `mujina-minerd`. It now blocks mutations and
  native bus experiments while Mujina is running too. The existing
  regression test now checks all four names and unchanged PWM on refusal.
- All 15 Rust tests and Clippy passed. The AArch64 release bundle was
  rebuilt and deployed; checksums and an on-board fixture rejection of
  a simulated Mujina owner were checked without starting a real miner.
- Baseline and final eight-command status captures completed with zero
  command failures. Missing attributes and the known EEPROM warning
  remain visible. Kernel logs were unchanged across the hardware sweep;
  kernel taint stayed 0 and the boot ID stayed unchanged.

The physical button capture used a temporary read-only 200 ms sampler of
the same two GPIO value nodes used by `controlboard-misc`; no stock
button handler or automatic action was enabled.

EEPROM SHA-256, boards 0 and 1 respectively:

```text
b15a6861c944150a72da9adab469980677c0f846294411e4fa7a049f64c0e0c0
938f9d152fd411df03087b9c80ea6e4e0e6bf24c9121d4f5b624cd56d0ae6015
```

Raw captures, per-command statuses, UART driver/termios snapshots,
kernel-log comparisons and the test harness are retained locally under
ignored `target/retest-2026-11-03`. They include device identifiers and
are not published wholesale.

## State left for Mujina bring-up

Vendor application inhibition remains armed, PID 1 retains the hardware
watchdog, and all original kernel drivers remain attached. PSU output is
off at measured 0 V / 0 A, resets 201/202 are low, switched LDO263 is low,
both buttons are high, LEDs are restored, and both cooling channels are
enabled at 99% with zero-speed mode 0.

The remaining gates are independent PSU fail-off/control, protected cold
startup, powered four-chain identity/register exchange, and physical
cooling/thermal protection. See [BRINGUP.md](BRINGUP.md). This pass does
not qualify unattended output-on operation or complete Mujina mining.

## Next acceptance tests for Mujina

These are proposed tests, not additional completed validation.

1. **Powered ASIC replies.** First implement verified driver-backed PSU
   disable/setpoint control, a bounded enable interval and an independent
   shutdown guardian. Establish the remaining PSU/LDO/reset ordering and
   idle-current bounds from synchronized evidence. Implement cold
   discovery at 375000 baud / shift 0 and the required initialization;
   the existing `check` command does not initialize a cold chain. Once
   initialized, require complete CRC-valid BT1980 identity replies with
   IDs 0–95 on each of four chains at the configured operating baud.
   Repeat register reads to check consistency. Submit no mining work for
   this identity test. On timeout, fault, excess current/temperature or
   loss of the test process, assert resets, disable output and verify
   Vout/Iout return to zero. Thresholds must be established before enable.
2. **Tachometer inputs, if used.** Resolve the connector-to-input mapping
   and electrical levels before attaching a known tach source. With
   hashboard output off, drive each used input with a known open-drain
   pulse train, or a separately powered fan of known pulses/revolution.
   Compare `fan-tool read-rpm` against `RPM = 60 × frequency / pulses-per-revolution`
   at several rates; removing pulses must produce a missing/stalled
   indication after measuring the driver's update/timeout behavior.
   Pulse count must be measured or documented, not assumed. For unused
   inputs on this water-cooled installation, record not applicable;
   verify actual coolant-flow/thermal protection instead. The PSU's
   separately reported fan RPM is not a test of these four inputs.
3. **Electrical waveforms.** Use a scope for voltage/edge shape and a
   logic analyzer for timing/decoding on a verified low-voltage breakout.
   Identify J1 pins by continuity and measurement first: T43 is only a
   PWM candidate; T41/T42 roles remain unresolved. With PSU output off,
   change cooling PWM at safe bench values and measure frequency, duty,
   polarity and pin routing. An earlier J1 capture measured about 25 kHz
   on T43; confirm correspondence to the current channel commands.
   Capture UART TX/RX on each physical chain during known read requests,
   verifying bit periods, bytes and connector mapping. During the later
   protected powered test, capture switched-LDO, reset and reset-return
   timing together and confirm actual rail voltage and settling before
   reset release. Reset at the ASIC side may be around 1.2 V; confirm
   levels before selecting digital thresholds. Keep PSU auxiliary-power
   pins out of logic-analyzer inputs.

Prioritize PSU shutdown/control and powered identity replies. Tach and
cooling-PWM qualification depend on whether those signals participate in
the installed water loop; external coolant circulation must have an
independently verified protection path before unattended mining.
