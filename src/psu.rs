//! P738B monitoring through stock sysfs or PEC-checked native SMBus.

use anyhow::{Result, ensure};
use clap::ArgMatches;

use crate::{i2c::I2c, protocol, system::System};

pub fn run(sys: &System, args: &ArgMatches) -> Result<()> {
    let (command, values) = args.subcommand().expect("required subcommand");
    if command == "status" {
        let name = sys.read("/sys/bitmicro/power/name").unwrap_or_default();
        let model = sys.read("/sys/bitmicro/power/model").unwrap_or_default();
        println!("/sys/bitmicro/power/name: {name}");
        println!("/sys/bitmicro/power/model: {model}");
        let identified = |text: &str| {
            !matches!(
                text.to_ascii_lowercase().as_str(),
                "" | "unknow" | "unknown" | "(null)"
            )
        };
        ensure!(
            identified(&name) || identified(&model),
            "PSU unavailable: stock driver has no valid PSU identity; telemetry placeholders are not measurements"
        );
        let mut successes = 0;
        let nodes = sys.children("/sys/bitmicro/power").unwrap_or_default();
        for attr in [
            "vender",
            "serial_no",
            "sw_version",
            "hw_version",
            "date",
            "enable",
            "vout",
            "vout_set",
            "iout",
            "iout_limit",
            "vin",
            "iin",
            "pin",
            "pout",
            "fan_enable",
            "fan_speed",
            "temp",
            "status",
            "errors",
            "common_status",
        ] {
            let path = format!("/sys/bitmicro/power/{attr}");
            // Stock firmware can expose only indexed fan/temperature nodes.
            // Restrict discovery to these read-only telemetry families.
            let indexed: Vec<_> = nodes
                .iter()
                .filter(|name| {
                    matches!(attr, "fan_speed" | "temp")
                        && name.strip_prefix(attr).is_some_and(|suffix| {
                            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
                        })
                })
                .collect();
            if !sys.path(&path).exists() && !indexed.is_empty() {
                for name in indexed {
                    successes += usize::from(sys.show(&format!("/sys/bitmicro/power/{name}")));
                }
            } else {
                successes += usize::from(sys.show(&path));
            }
        }
        ensure!(
            successes > 0,
            "no readable power attributes; try native-status only if no kernel driver owns 0x58"
        );
        println!(
            "Vendor attributes are raw. On tested stock P738B firmware: vout/vout_set/vin are centivolts; iout/iin are milliamps; indexed temp values are tenths of a degree C; pin/pout are watts; fan_speed values are RPM. Other nodes retain their driver-specific units."
        );
        println!(
            "Unsupported generic phase and water_temp nodes are not probed; reading them can affect the stock driver's error reporting."
        );
        return Ok(());
    }
    let _session = sys.session()?;
    let device = I2c::open(&sys.path("/dev/i2c-4"), 0x58, true)?;
    if matches!(command, "disable" | "set-voltage") {
        let model = device.block(0x9a)?;
        ensure!(
            String::from_utf8_lossy(&model).contains("P738B"),
            "PSU model is not P738B; refusing control write"
        );
    }
    match command {
        "native-status" => native_status(&device),
        "identity" => {
            let mut errors = 0;
            for (name, register) in [
                ("manufacturer", 0x99),
                ("model", 0x9a),
                ("revision", 0x9b),
                ("location", 0x9c),
                ("date", 0x9d),
                ("serial", 0x9e),
            ] {
                match device.block(register) {
                    Ok(bytes) => println!(
                        "{name}: {} raw={}",
                        String::from_utf8_lossy(&bytes),
                        hex::encode(&bytes)
                    ),
                    Err(e) => {
                        errors += 1;
                        eprintln!("{name}: ERROR ({e:#})");
                    }
                }
            }
            ensure!(errors == 0, "{errors} PSU identity reads failed");
            Ok(())
        }
        "disable" => {
            device.write_byte(0x01, 0)?;
            ensure!(
                device.byte(0x01)? == 0,
                "PSU did not acknowledge output disable"
            );
            println!("PSU OPERATION=0; verify measured output voltage independently");
            Ok(())
        }
        "set-voltage" => {
            let millivolts = *values.get_one::<u32>("millivolts").unwrap();
            ensure!(device.byte(0x01)? == 0, "set-voltage requires output OFF");
            ensure!(
                device.byte(0x20)? == 0x17,
                "unexpected VOUT_MODE; expected ULINEAR16 exponent -9"
            );
            let raw = ((millivolts * 512 + 500) / 1000) as u16;
            device.write_word(0x21, raw)?;
            let actual = device.word(0x21)?;
            ensure!(
                actual == raw,
                "VOUT_COMMAND readback mismatch: requested {raw}, actual {actual}"
            );
            println!(
                "VOUT_COMMAND={:.6} V raw=0x{raw:04x}; output remains off",
                f64::from(raw) / 512.0
            );
            Ok(())
        }
        _ => unreachable!(),
    }
}

fn native_status(device: &I2c) -> Result<()> {
    let mut failures = 0;
    for (name, register) in [
        ("operation", 0x01),
        ("vout_mode", 0x20),
        ("status_vout", 0x7a),
        ("status_iout", 0x7b),
        ("status_input", 0x7c),
        ("status_temperature", 0x7d),
        ("status_other", 0x7f),
        ("status_fans", 0x81),
    ] {
        match device.byte(register) {
            Ok(value) => println!("{name}: 0x{value:02x}"),
            Err(e) => {
                failures += 1;
                eprintln!("{name}: ERROR ({e:#})");
            }
        }
    }
    let mode = device.byte(0x20)?;
    ensure!(
        mode == 0x17,
        "unsupported VOUT_MODE 0x{mode:02x}; refusing assumed scaling"
    );
    for (name, register, unit, voltage) in [
        ("vout_set", 0x21, "V", true),
        ("vin", 0x88, "V", false),
        ("iin", 0x89, "A", false),
        ("vout", 0x8b, "V", true),
        ("iout", 0x8c, "A", false),
        ("temp1", 0x8d, "C", false),
        ("temp2", 0x8e, "C", false),
        ("temp3", 0x8f, "C", false),
        ("fan", 0x90, "RPM", false),
        ("pin", 0x97, "W", false),
    ] {
        match device.word(register) {
            Ok(raw) => println!(
                "{name}: {:.6} {unit} raw=0x{raw:04x}",
                if voltage {
                    f64::from(raw) / 512.0
                } else {
                    protocol::linear11(raw)
                }
            ),
            Err(e) => {
                failures += 1;
                eprintln!("{name}: ERROR ({e:#})");
            }
        }
    }
    ensure!(
        failures == 0,
        "{failures} native PSU reads failed (including possible PEC failures)"
    );
    Ok(())
}

pub fn probe(sys: &System) -> Result<()> {
    let _session = sys.session()?;
    let mut failures = 0;
    // Target-specific reads only: no SMBus quick writes or broad address scan.
    for (bus, address, kind) in [
        (4, 0x58, "P738B"),
        (4, 0x4f, "LM75 inlet"),
        (10, 0x48, "LM75 board 0"),
        (10, 0x49, "LM75 board 1"),
        (10, 0x50, "EEPROM board 0"),
        (10, 0x51, "EEPROM board 1"),
    ] {
        let result = (|| -> Result<String> {
            let device = I2c::open(
                &sys.path(format!("/dev/i2c-{bus}")),
                address,
                address == 0x58,
            )?;
            if address == 0x58 {
                Ok(format!("OPERATION=0x{:02x}", device.byte(1)?))
            } else {
                let data = device.read_at(address, 0, 2)?;
                Ok(format!("register0={}", hex::encode(data)))
            }
        })();
        match result {
            Ok(value) => println!("i2c-{bus}:0x{address:02x} {kind}: OK {value}"),
            Err(e) => {
                failures += 1;
                println!("i2c-{bus}:0x{address:02x} {kind}: UNAVAILABLE ({e:#})");
            }
        }
    }
    ensure!(
        failures == 0,
        "{failures} known targets unavailable; a bound kernel driver is not evidence of a missing device"
    );
    Ok(())
}
