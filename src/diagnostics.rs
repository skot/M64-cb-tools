//! Board inventory, LEDs, buttons, cooling and kernel health interfaces.

use anyhow::{Result, ensure};
use clap::ArgMatches;

use crate::system::{System, UARTS, selection};

pub fn info(sys: &System) -> Result<()> {
    println!("Profile: M64VL20 / CB6 V5 SL V2.0 / Allwinner H616");
    println!("Expected profile only; compare it to the identity below before driving hardware.");
    for path in [
        "/proc/sys/kernel/osrelease",
        "/proc/device-tree/model",
        "/tmp/platform.ini",
        "/tmp/miner_type",
        "/tmp/eeprom/miner_type",
    ] {
        sys.show(path);
    }
    println!("hardware owners: {}", sys.miners()?.join(", "));
    for (slot, uart) in UARTS.iter().enumerate() {
        println!(
            "slot {slot}: {uart}, physical board {}, present={}",
            slot / 2,
            sys.path(uart).exists()
        );
    }
    for bus in [4, 10] {
        sys.show(&format!("/sys/class/i2c-adapter/i2c-{bus}/name"));
        println!(
            "/dev/i2c-{bus}: present={}",
            sys.path(format!("/dev/i2c-{bus}")).exists()
        );
    }
    println!("PSU: I2C4:0x58; inlet: I2C4:0x4f; boards: I2C10:0x48/49,0x50/51");
    for (name, gpio) in gpios() {
        println!(
            "{name}: GPIO{gpio}, value={}",
            sys.gpio_read(gpio).unwrap_or_else(|_| "unexported".into())
        );
    }
    for path in [
        "/sys/bitmicro/keepalive/enable",
        "/sys/class/watchdog/watchdog0/identity",
        "/sys/class/watchdog/watchdog0/state",
        "/sys/class/watchdog/watchdog0/timeout",
        "/sys/class/watchdog/watchdog0/nowayout",
    ] {
        sys.show(path);
    }
    let serial = if sys.path("/proc/tty/driver/uart").exists() {
        "/proc/tty/driver/uart"
    } else {
        "/proc/tty/driver/serial"
    };
    sys.show(serial);
    for path in [
        "/dev/tee0",
        "/dev/teepriv0",
        "/dev/watchdog",
        "/dev/watchdog0",
    ] {
        println!("{path}: present={}", sys.path(path).exists());
    }
    for class in ["/sys/class/net", "/sys/bus/usb/devices", "/sys/class/udc"] {
        match sys.children(class) {
            Ok(names) => {
                for name in names {
                    for attr in if class.ends_with("net") {
                        &["operstate", "carrier", "speed"][..]
                    } else if class.ends_with("udc") {
                        &["state"][..]
                    } else {
                        &["idVendor", "idProduct", "product"][..]
                    } {
                        let path = format!("{class}/{name}/{attr}");
                        if sys.path(&path).exists() {
                            sys.show(&path);
                        }
                    }
                }
            }
            Err(e) => println!("{class}: UNAVAILABLE ({e:#})"),
        }
    }
    if let Err(error) = temps(sys) {
        println!("temperatures: UNAVAILABLE ({error:#})");
    }
    Ok(())
}

fn gpios() -> [(&'static str, u32); 9] {
    [
        ("red-led", 257),
        ("green-led", 258),
        ("ip-button", 234),
        ("reset-button", 233),
        ("reset-0:1", 201),
        ("reset-2:3", 202),
        ("reset-detect-0:1", 193),
        ("reset-detect-2:3", 194),
        ("io-power", 263),
    ]
}

pub fn misc(sys: &System, args: &ArgMatches) -> Result<()> {
    match args.subcommand().expect("required subcommand") {
        ("status", _) => {
            for (name, gpio) in gpios() {
                println!(
                    "{name}: GPIO{gpio} raw={} (polarity is not inferred)",
                    sys.gpio_read(gpio).unwrap_or_else(|_| "unexported".into())
                );
            }
        }
        ("buttons", _) => {
            let _session = sys.session()?;
            for gpio in [234, 233] {
                sys.gpio_export(gpio, true)?;
                println!("GPIO{gpio}: raw={}", sys.gpio_read(gpio)?);
            }
        }
        ("set", values) => {
            let _session = sys.session()?;
            let high = values.get_one::<String>("level").unwrap() == "high";
            let target = values.get_one::<String>("target").unwrap();
            for (name, gpio) in [("red", 257), ("green", 258)] {
                if target == "all" || target == name {
                    sys.gpio_set(gpio, high)?;
                    println!(
                        "{name}: GPIO{gpio} set {}",
                        if high { "high" } else { "low" }
                    );
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub fn fan(sys: &System, args: &ArgMatches) -> Result<()> {
    let (command, values) = args.subcommand().expect("required subcommand");
    if command == "status" || command == "read-rpm" {
        let mut successes = 0;
        for fan in 0..2 {
            let base = format!("/sys/bitmicro/fan/fan{fan}");
            for attr in ["enable", "pwm_duty", "zero_speed", "speed0", "speed1"] {
                if command == "status" || attr.starts_with("speed") {
                    successes += usize::from(sys.show(&format!("{base}/{attr}")));
                }
            }
        }
        ensure!(successes > 0, "no readable M64 cooling attributes");
        println!("M64VL20 water cooling can legitimately report 0 RPM; PSU fan is separate.");
    } else {
        let percent = *values.get_one::<u8>("percent").unwrap();
        let channels = selection(values.get_one::<String>("target").unwrap(), 2)?;
        let _session = sys.session()?;
        // Validate every requested node before changing the first channel.
        for channel in &channels {
            sys.read(&format!("/sys/bitmicro/fan/fan{channel}/pwm_duty"))?;
        }
        for channel in channels {
            let path = format!("/sys/bitmicro/fan/fan{channel}/pwm_duty");
            sys.write(&path, percent)?;
            let actual = sys.read(&path)?;
            println!("{path}: requested={percent} readback={actual}");
            ensure!(actual.parse::<u8>()? == percent, "fan PWM readback differs");
        }
    }
    Ok(())
}

pub fn temps(sys: &System) -> Result<()> {
    let mut successes = 0;
    if let Ok(names) = sys.children("/sys/class/hwmon") {
        for name in names {
            let base = format!("/sys/class/hwmon/{name}");
            let identity = sys
                .read(&format!("{base}/name"))
                .unwrap_or_else(|_| "unknown".into());
            if let Ok(attrs) = sys.children(&base) {
                for attr in attrs
                    .into_iter()
                    .filter(|n| n.starts_with("temp") && n.ends_with("_input"))
                {
                    println!("sensor={identity}");
                    successes += usize::from(sys.show(&format!("{base}/{attr}")));
                }
            }
        }
    }
    if let Ok(names) = sys.children("/sys/class/thermal") {
        for name in names
            .into_iter()
            .filter(|name| name.starts_with("thermal_zone"))
        {
            let base = format!("/sys/class/thermal/{name}");
            let identity = sys
                .read(&format!("{base}/type"))
                .unwrap_or_else(|_| "unknown".into());
            println!("sensor={identity}");
            successes += usize::from(sys.show(&format!("{base}/temp")));
        }
    }
    for path in ["/sys/bitmicro/power/temp", "/tmp/water_temp"] {
        if sys.path(path).exists() {
            successes += usize::from(sys.show(path));
        }
    }
    println!(
        "temperature attributes read={successes}; hwmon/thermal values are millidegrees C; vendor values remain raw"
    );
    ensure!(successes > 0, "no readable temperature attributes");
    Ok(())
}
