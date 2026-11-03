# M64 control-board tools

Standalone diagnostics for the WhatsMiner M64VL20, CB6 V5 SL V2.0,
Allwinner H616 control board. These standalone tools have no
dependency on the Mujina mining daemon, its USB discovery, or libudev.
They follow the small-tool layout of
[amlogic-cb-tools](https://github.com/skot/amlogic-cb-tools).

The implementation is built from the local M64 configuration, connector
notes, PMBus captures and BT1980 UART captures. **Controller-only bench
validation completed on 2026-11-03**, without hashboards or a PSU.
See [VALIDATION.md](VALIDATION.md) for verified behavior and the tests
that still require attached hardware.

## Tools

| Binary | Interface coverage |
| --- | --- |
| `m64-board-info` | Identity, UART/I2C presence, GPIO levels, hwmon, Ethernet link, USB identity, TEE/watchdog presence and watchdog attributes |
| `controlboard-misc` | Raw red/green LED outputs, IP Found/reset button inputs |
| `fan-tool` | Stock PWM percentages and kernel tachometer RPM |
| `p738b-psu-tool` | Stock PSU telemetry; native PEC-checked PMBus identity/telemetry, output disable and output-off voltage setting |
| `hashboard-m64` | Two physical EEPROMs, LM75 temperatures, shared resets/LDO, four UART register links, offline frame/EEPROM decode |
| `i2c-probe` | Target-specific reads of the six known I2C devices |

See [INTERFACES.md](INTERFACES.md) for wiring, evidence, confidence and
the distinction between implemented tests and inventory-only surfaces.
The Amlogic OLED experiment has no confirmed M64 equivalent. J1's
tach/reference assignments remain unresolved; no pin mapping is invented.

## Build and package

From the repository root:

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
rustup target add aarch64-unknown-linux-musl
sh scripts/package.sh
```

The archive is written to:

```text
target/m64-cb-tools/m64-cb-tools-aarch64-unknown-linux-musl.tar.gz
```

The packaging script selects `rust-lld`. No cross C compiler or board
libraries are needed. It includes all six static binaries, the status
capture script, documentation, license and a SHA-256 manifest.

## RAM-only deployment

The documented working SSH account is `micro` with UID 0. Substitute the
board's current address. Use an already configured SSH key; these tools
do not change SSH access, boot hooks or startup services.

```sh
scp -O target/m64-cb-tools/m64-cb-tools-aarch64-unknown-linux-musl.tar.gz \
    micro@BOARD_IP:/tmp/
ssh micro@BOARD_IP
cd /tmp
tar xzf m64-cb-tools-aarch64-unknown-linux-musl.tar.gz
cd m64-cb-tools-aarch64-unknown-linux-musl
sha256sum -c SHA256SUMS
sh ./capture-status.sh --controller-only /tmp/m64-first-status
```

Use `--controller-only` when no PSU or hashboards are attached; omit it
for a complete assembled miner. Stock SSH may require the per-command
options `-o HostKeyAlgorithms=+ssh-rsa` and
`-o PubkeyAcceptedAlgorithms=+ssh-rsa` with modern OpenSSH, for both `ssh`
and legacy `scp -O`.

Fetch the report directory before reboot. The capture script uses only
stock-driver reads and already exported GPIO levels. It skips EEPROM
access when no kernel EEPROM node exists. It does not open UARTs, stop
services, alter GPIO direction or scan an I2C bus. Its results file
records command exit status, **not** hardware certification. Inspect
every `UNAVAILABLE` entry.

## First interface checks

These commands can read stock-driver observations while stock mining
continues:

```sh
./m64-board-info
./controlboard-misc status
./fan-tool status
./fan-tool read-rpm
./p738b-psu-tool status
./hashboard-m64 status
./hashboard-m64 temps
./hashboard-m64 eeprom 0
./hashboard-m64 eeprom 1
```

EEPROM commands prefer `/sys/bus/i2c/devices/10-0050/eeprom` and
`10-0051/eeprom`. Without those nodes, direct I2C is attempted only when
the hardware session guard allows it. EEPROM reads print identity, raw
bytes and CRC results. A CRC mismatch is reported as a warning because
the reference machine mined with one such board.

Hwmon devices are discovered by directory and reported with their names,
rather than assuming a fixed `hwmon5`. Hwmon temperatures are raw
millidegrees Celsius; named thermal zones provide the SoC temperatures
on the tested stock kernel. Vendor power values retain their raw representation
until their units are confirmed on the current firmware. A water-cooled
M64VL20 may report zero control-board fan RPM; the PSU fan is separate.

PSU status rejects the stock driver's `unknow`/`(null)` identity when no
PSU is attached. Sentinel voltage values are not reported as measurements.

## Exclusive hardware experiments

Direct I2C/UART access and mutations take a shared nonblocking lock and
refuse to run while `btminer`, `btminer_test` or `power-aging` is running.
This serializes these tools; it cannot stop unrelated processes from
starting afterward or changing hardware. Arrange a controlled bench
session through the stock service procedures first. Stock supervision
and watchdog behavior must be understood before stopping mining.

These tools never stop daemons, unbind kernel drivers, disable watchdogs,
feed watchdogs or persist changes to startup configuration. `keyd` and
`system-monitor` can still change LEDs; their ownership must be handled
in the bench procedure. The tools leave GPIO exports and commanded
levels in place. Fan commands persist until another owner changes them.

```sh
./controlboard-misc buttons
./controlboard-misc set green high
./controlboard-misc set red low
./fan-tool set-pwm all 100
./hashboard-m64 temps --native
./i2c-probe
```

LED and button levels are raw; active polarity is not assumed. The
`buttons` command exports/configures inputs when necessary. GPIO status
alone never exports or reconfigures pins.

`i2c-probe` reads only P738B OPERATION, LM75 register 0, and EEPROM offset
0. It never performs a broad address sweep or SMBus quick write.
Kernel-owned addresses are rejected by `I2C_SLAVE`; there is no force
option. Prefer stock sysfs interfaces when the kernel owns the device.
Do not unbind production drivers simply to make the native tests pass.

Native P738B commands use kernel SMBus PEC on every transaction:

```sh
./p738b-psu-tool identity
./p738b-psu-tool native-status
./p738b-psu-tool disable
./p738b-psu-tool set-voltage 21000
```

Control writes require a PEC-valid P738B model response. Voltage
setting is limited to 15–25 V, requires OPERATION=0 and
VOUT_MODE=0x17, and verifies the encoded setpoint by readback. It rounds
to the nearest 1/512 V. There is deliberately no PSU output-enable
command: independent cooling/protection and shutdown must first be
validated. Output disable verifies OPERATION, which does not establish
that stored energy or measured output voltage has reached zero.

### Hashboard reset and logic power

Physical board 0 contains slots 0/1; board 1 contains slots 2/3. Resetting
a board resets both of its chains. GPIO263 switches a shared logic rail;
it is distinct from the PSU's high-power output.

```sh
./hashboard-m64 reset 0 assert
./hashboard-m64 reset 1 assert
./hashboard-m64 io-power on
./hashboard-m64 reset 0 release
./hashboard-m64 reset 1 release
```

`io-power` requires both reset values to read low, and waits 1013 ms
after switching on before returning. This reproduces the observed
logic-rail settling interval; it is **not** a complete ASIC startup
sequence. Reset release has no automatic power sequencing.

### Initialized UART link checks

```sh
./hashboard-m64 check 0
./hashboard-m64 check all --expected-chips 96
./hashboard-m64 read-register 2 0x110 1 --chip 0xfff
```

These checks require an already initialized, quiet chain with exclusive
ownership. They do not enumerate a cold chain, replay vendor startup,
configure PLLs/cores, change ASIC baud or submit mining work. Stopping
stock mining can change/reset the chain; a timeout after stopping it is
not proof of a defective UART. Cold-chain bring-up is the next milestone.

The default **host baud is 12,000,000**, matching the live M64 capture,
despite the 6 Mbaud ASIC configuration field. TX uses vendor ioctl
`0x5441 | (length << 16)` and right rotation by three; RX reverses the
rotation and validates length and CRC-8. Options `--host-baud`, `--shift`
and `--timeout-ms` allow controlled comparison. Discovery shift is zero;
choosing it does not perform discovery. The saved termios2 configuration
is restored on normal exit and ordinary errors; abrupt termination may
leave the selected host configuration in place.

`check` requires the complete expected contiguous chip-ID set and ASIC
model `0x1980`. Register reads print logical RX frames, identifiers,
response types, payloads and counts. No UART data means failure, rather
than a successful check with zero detected chips. Reads have finite
deadlines. Corrupt/truncated frames also fail the command.

## Offline use

```sh
./hashboard-m64 decode-frame 00000002198031 --shift 0
./hashboard-m64 decode-eeprom board0.bin
./m64-board-info --root /path/to/filesystem-snapshot
```

`--root` redirects known filesystem paths for fixture-based inspection.
Tests cover captured CRC/numeric formats, CLI ranges, stock-miner
exclusion, missing devices, EEPROM fallback behavior and shared-reset
requirements without accessing real devices.
