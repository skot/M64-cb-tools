# PSU control and powered ASIC replies, 2026-11-03

**Verified: retained-driver PSU control and complete cold discovery on all
four ASIC UARTs.** Each slot returned 96 distinct identifiers, with CRC-valid
discovery status and BT1980 model replies. All tests ended with measured
PSU output 0 V / 0 A, resets asserted and switched logic power off.

Target: the assembled M64VL20 / CB6 V5 / H616 at `192.168.1.179`, with
P738B PSU and the same two hashboards used in preceding reports. The
operator confirmed coolant circulation and presence with access to the
main-power disconnect. No meter or scope was available; electrical
waveform, rail-calibration and coolant-flow measurements remain open.
USB/FEL and TEE are outside the required Mujina scope.

Successful four-slot discovery ran November 3, 21:22:52–21:23:54 UTC
(November 4, 05:22:52–05:23:54 board-local UTC+08:00). The controller
stayed in the same boot with kernel taint 0 and vendor applications
inhibited. Kernel logs were unchanged across every captured trial.

## Driver-backed PSU controls

The new explicit `--stock` backend uses the retained `pmbus-power`
driver at I2C4:0x58. No driver was unbound and no forced I2C access was
used. Native userspace PMBus commands remain subject to EBUSY; native
ownership is not needed for this backend.

```sh
./p738b-psu-tool disable --stock
./p738b-psu-tool set-voltage 21000 --stock
```

- Disable writes a single decimal `0` without a newline, then verifies
  enable, measured Vout and Iout reach zero within its bounded check.
- Voltage setting requires output enable/Vout/Iout all zero, both resets
  asserted and switched LDO off. It verifies H616 / CB6 V5 / P738B and the
  attached driver, accepts exact 10 mV increments in 15–25 V, and checks
  the centivolt setpoint by readback.
- The output-off roundtrip `18550 → 21000 → 22000 → 18550` mV passed.
  Enable/Vout/Iout stayed zero during all setpoint changes.
- Guarded trials subsequently demonstrated enable acknowledgement,
  a ramp to approximately 21.1 V and shutdown from powered conditions.
  Normal and failed trials restored the original 18.55 V setpoint after
  verifying output off.

These are kernel-driver measurements and acknowledgements. They do not
replace an independent voltage/current calibration or prove the native
userspace PEC implementation against the attached PSU.

## Bounded power and shutdown guard

`p738b-psu-tool trial` owns the diagnostic lock across PSU, GPIO and UART
operations. A separate child owns the only `enable=1` write. It retains
the same lock through shutdown even if its parent dies, runs in a
separate session, handles INT/TERM/HUP as shutdown requests and treats
parent-pipe loss as a cutoff. Queued commands are rejected on pipe
hangup, and nonblocking logging cannot stall a cutoff on a full pipe.

The guard monitors the boot ID, kernel taint, maintenance policy,
unexpected vendor/miner processes, driver identity, cooling settings,
board/PSU temperatures, PSU fault words, setpoint, measured voltage,
current/power and reset feedback. Deadline, fault, invalid telemetry,
loss of control or a selected limit breach asserts both resets, lowers
LDO, disables PSU output and verifies zero output. Failed/rejected
disable commands are retried promptly during the bounded discharge check.
Emergency shutdown bypasses normal session/taint/miner exclusions.

Defaults remain a one-second nominal deadline, 1 A and 25 W cutoffs.
Explicit CLI ranges allow at most five seconds, 7.5 A and 150 W; the
trial target is fixed at the stock normal-boot 21 V. These are test
limits, not mining limits. The successful ID trials explicitly selected
three seconds / 7.5 A / 150 W and sent no mining-work packets.

The guard is software operating through kernel/sysfs I/O. Deadline
servicing depends on responsive kernel operations; it is not a hard
real-time cutoff or a production cooling/PSU safety controller. Direct
SIGKILL of the guard or simultaneous system failure is not covered by
its signal handlers. PID 1 retains the OS hardware watchdog. Parent
SIGKILL can leave the temporary 21 V setpoint in place, although the
guard cuts output; the fault test restored it explicitly afterward.

## Characterization before reset release

Initial limits deliberately exposed the ramp and shutdown behavior before
attempting communication:

| Trial | Selected nominal deadline / cutoffs | Observation | Result |
| --- | --- | --- | --- |
| First reset-held pulse | 1 s / 1 A / 25 W | Last sample 11.28 V, 187 mA, 2 W | Deadline cutoff; full-voltage criterion not met; output returned to zero |
| Second reset-held pulse | 2 s / 1 A / 25 W | Current crossed 1 A while voltage was still ramping | Current guard tripped at 1296 mA; output returned to zero |
| Third reset-held pulse | 3 s / 5 A / 100 W | Last sample 18.81 V, 4734 mA, 89 W | Current guard tripped at 5046 mA; output returned to zero |
| Reset-return-interlocked pulse | 3.5 s / 7.5 A / 150 W | Reached about 21.11 V; both reset returns stayed low; sampled power rose to 150 W | Power guard tripped at 151 W; output returned to zero |

These software cutoff events were not PSU-reported faults. PSU status
and common-status were zero afterward. Reset-held power was still
rising: **a steady idle-power envelope was not established**. Reset
assertion must not be described as proof that all ASIC cores consume no
power. No further power-limit increase was used for discovery; the
complete reads fit within the preceding stable-voltage interval.

Dry hardware checks verified deadline and signal shutdown with output
kept disabled. A deliberate parent SIGKILL also returned reset/LDO/output
to off. Offline Linux fixtures cover powered mock shutdown, sensor fault,
both pipes closing, process-group termination, lock retention, a full
log pipe and an ENABLE queued after disconnect.

## Cold discovery method and results

The guard waits at least 1000 ms after the enable request and requires three
successive voltage samples in 19.95–22 V with changes no larger than
20 mV. It rechecks telemetry, pipe/cancellation state and remaining
budget immediately before releasing the selected physical board. After
100 ms it requires that board's reset-return high and the other board's
reset/return low. This is a short software gate, not an electrical
stability measurement.

The host uses 375000 baud / shift 0 with APB2 verified at 192 MHz.
A bare ID read first produced valid `1980` replies with the common
unassigned identifier `0xff0`. The tool correctly rejected that as a
complete enumeration. This independently verified powered TX/RX before
any configuration prefix was sent.

The subsequent explicit `--capture-prefix` test sends the fixed captured
1832-frame pre-PLL prefix for the selected slot, then reads:

```text
0x110, length 1: require IDs 0–95, payload 00
0x100, length 2: require IDs 0–95, payload 1980
```

| Slot / UART | Physical board | Discovery status IDs | BT1980 model IDs | Peak sampled Vout / Iout / Pout | Result |
| --- | --- | --- | --- | --- | --- |
| 0 / ttyS1 | 0 | 96 / 96 | 96 / 96 | 21.13 V / 6.328 A / 133 W | Pass |
| 1 / ttyS4 | 0 | 96 / 96 | 96 / 96 | 21.11 V / 6.765 A / 142 W | Pass |
| 2 / ttyS2 | 1 | 96 / 96 | 96 / 96 | 21.12 V / 6.593 A / 139 W | Pass |
| 3 / ttyS3 | 1 | 96 / 96 | 96 / 96 | 21.11 V / 7.046 A / 148 W | Pass |

Each slot yielded exactly 192 valid response frames, 96 for each query:
768 complete replies across 384 uniquely addressed ASICs on four UARTs.
The tool rejects CRC/truncation errors, wrong payloads and incomplete ID
sets. All four UART termios configurations matched their saved values
after each trial. Both boards' powered reset-return transitions were
therefore exercised. Slot 3 ended at the nominal deadline after its
complete replies; the other successful runs ended when the parent
closed the guard pipe after reading.

The prefix is capture data embedded in the binaries and auditable under
`profiles/`. It preserves both address-assignment passes, refreshes and
slot/chip-specific settings. Every frame is CRC/length checked against
a restricted write allowlist: `0x123/000/070/018/019/001/020`. It contains
no known PLL, core-control or mining-work register writes. Some allowed
register semantics and reset defaults are still unmapped; this is not
proof that cores were disabled. The successful early read position is
now validated on this unit, but the prefix is not demonstrated minimal
or portable to other revisions/bins. No automatic fallback replays the
later PLL/core/work sequence.

## Evidence, checks and remaining work

- Six host unit tests and 20 host integration tests passed; 21 Linux
  integration fixtures passed on the controller, including the Linux
  pipe-capacity case. Clippy passed and the AArch64 static bundle was
  rebuilt. Captured assigned/default identifier vectors are retained
  as decoder regressions.
- Exact-live stock `btminer` SHA-256:
  `a245dee07470ab3947ad036d4aab759bc64c876d5cd25573e053b44b29e94de8`.
- Exact-live `libwhatsminer.so` SHA-256:
  `8f05cefbe1d956a6b6ac311478f5d5a9f03b4598264ba9a7b7600cc784f2a68c`.
  Its power-enable/logic-power function bodies match the older retained
  library; a 16-byte difference elsewhere is not explained here.
- REA/Ghidra exact-live power ordering Evidence:
  `ev_36f2e6f1736ce622dd0e8fc94a5732b28710a2998c74c5923540e1e8e40ec326`;
  logic-power helper:
  `ev_71120a58a305eb5f035cad3677377535aa4a023cfe46e847e3fe3f77aa701b0e`.
  Matching stock boot sequencing:
  `ev_4eaa35ac5cdbc906aaeea8770d76be93604b4db1a9fe108b5e5e19b1f8f2b890`.
- Prefix source capture SHA-256:
  `80aee2d832be64e507d05c68a5eee7c86b236cef617dd169d916e07a5b344c45`.
  Full Evidence, raw runtime captures, summaries and vendor artifacts
  are retained locally under ignored `target/powered-replies-investigation`;
  vendor executables/libraries are not distributed with the tools.

Remaining: powered ASIC replies after the 12 Mbaud transition, PLL/core
startup, work submission/nonce validation, long-running Mujina shutdown
and cooling protection, steady idle-current characterization and
physical waveform/calibration measurements. This validates a bounded
cold discovery test, not continuous mining or unattended operation.

Final state: vendor startup inhibition armed; OS watchdog retained;
original kernel drivers attached; PSU enable/Vout/Iout `0/0/0` and
status/common-status `0x0/0x0`; resets and reset returns low; shared LDO
off; original 18.55 V setpoint restored; cooling enabled at 99% with
zero-speed mode 0. No stock mining restart was requested.
