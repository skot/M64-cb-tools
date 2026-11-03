//! Command-line definitions shared by the standalone executables.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Arg, ArgAction, Command, value_parser};

use crate::{diagnostics, hashboard, psu, system::System};

/// Executes a named tool, printing contextual errors and exiting nonzero.
pub fn main(name: &'static str) {
    if let Err(error) = run(name) {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run(name: &'static str) -> Result<()> {
    let args = command(name).get_matches();
    let sys = System {
        root: args.get_one::<PathBuf>("root").unwrap().clone(),
    };
    match name {
        "m64-board-info" => diagnostics::info(&sys),
        "controlboard-misc" => diagnostics::misc(&sys, &args),
        "fan-tool" => diagnostics::fan(&sys, &args),
        "p738b-psu-tool" => psu::run(&sys, &args),
        "hashboard-m64" => hashboard::run(&sys, &args),
        "i2c-probe" => psu::probe(&sys),
        _ => unreachable!(),
    }
}

fn command(name: &'static str) -> Command {
    let root = Arg::new("root")
        .long("root")
        .global(true)
        .default_value("/")
        .value_parser(value_parser!(PathBuf))
        .help("Filesystem root (for offline snapshots and tests)");
    let app = Command::new(name)
        .version(env!("CARGO_PKG_VERSION"))
        .arg(root);
    match name {
        "m64-board-info" => app.about("M64 identity, UART/I2C, GPIO, cooling, Ethernet, USB, TEE and watchdog inventory; no device writes"),
        "i2c-probe" => app.about("Read known M64 targets only; no address sweep, no quick writes, no forced kernel-driver access"),
        "controlboard-misc" => app.about("CB6 V5 raw LED levels and button inputs").subcommand_required(true)
            .subcommand(Command::new("status").about("Read already exported GPIOs without changing direction"))
            .subcommand(Command::new("buttons").about("Export/configure both button inputs, then read raw levels"))
            .subcommand(Command::new("set").about("Set raw LED level; active polarity needs bench verification")
                .arg(Arg::new("target").required(true).value_parser(["red","green","all"]))
                .arg(Arg::new("level").required(true).value_parser(["high","low"]))),
        "fan-tool" => app.about("Stock bitmicro fan PWM and tachometer attributes").subcommand_required(true)
            .subcommand(Command::new("status"))
            .subcommand(Command::new("read-rpm").about("Read kernel tachometer RPM attributes; PSU fan is separate"))
            .subcommand(Command::new("set-pwm").about("Set vendor PWM percentage (does not enable fan or change zero_speed)")
                .arg(Arg::new("target").required(true).value_parser(["0","1","all"]))
                .arg(Arg::new("percent").required(true).value_parser(value_parser!(u8).range(0..=100)))),
        "p738b-psu-tool" => app.about("P738B sysfs telemetry and PEC-checked SMBus diagnostics; no output-enable command").subcommand_required(true)
            .subcommand(Command::new("status").about("Read stock driver attributes; safe alongside stock mining"))
            .subcommand(Command::new("native-status").about("Direct PMBus reads with mandatory PEC; requires unowned I2C target"))
            .subcommand(Command::new("identity").about("Read manufacturer SMBus blocks with mandatory PEC"))
            .subcommand(Command::new("disable").about("Write OPERATION=0 and verify readback"))
            .subcommand(Command::new("set-voltage").about("Set VOUT_COMMAND only with output OFF; 15000..25000 mV")
                .arg(Arg::new("millivolts").required(true).value_parser(value_parser!(u32).range(15000..=25000)))),
        "hashboard-m64" => app.about("M64VL20 physical boards 0/1 and logical UART slots 0..3").subcommand_required(true)
            .subcommand(Command::new("status"))
            .subcommand(Command::new("temps").about("Read discovered hwmon/vendor temperature nodes")
                .arg(Arg::new("native").long("native").action(ArgAction::SetTrue).help("Direct LM75 reads on I2C10:0x48/49 and I2C4:0x4f")))
            .subcommand(Command::new("eeprom").about("Read stock 128-byte EEPROM identity; prefers kernel eeprom node").arg(board()))
            .subcommand(Command::new("decode-eeprom").arg(Arg::new("file").required(true)))
            .subcommand(Command::new("decode-frame").about("Decode one physical RX frame offline").arg(Arg::new("hex").required(true)).arg(shift()))
            .subcommand(Command::new("reset").about("Drive shared reset for a physical board and its TWO slots")
                .arg(board()).arg(Arg::new("state").required(true).value_parser(["assert","release"])))
            .subcommand(Command::new("io-power").about("Switch shared LDO; requires both resets asserted; waits 1013ms on enable")
                .arg(Arg::new("state").required(true).value_parser(["on","off"])))
            .subcommand(uart(Command::new("check").about("Read 0x100 and require a complete BT1980 ID set; board must already be initialized"))
                .arg(Arg::new("expected-chips").long("expected-chips").default_value("96").value_parser(value_parser!(u16).range(1..=4095))))
            .subcommand(uart(Command::new("read-register").about("Send one serialized aligned read; no reset, startup replay or work submission"))
                .arg(Arg::new("chip").long("chip").default_value("0xfff"))
                .arg(Arg::new("register").required(true))
                .arg(Arg::new("length").required(true).value_parser(value_parser!(u8).range(1..=255)))),
        _ => unreachable!(),
    }
}

fn board() -> Arg {
    Arg::new("board")
        .required(true)
        .value_parser(value_parser!(u8).range(0..=1))
}

fn shift() -> Arg {
    Arg::new("shift")
        .long("shift")
        .default_value("3")
        .value_parser(value_parser!(u32).range(0..=7))
}

fn uart(command: Command) -> Command {
    let needs_slot = command.get_name() == "read-register";
    command
        .arg(
            Arg::new("slot")
                .required(needs_slot)
                .default_value("all")
                .value_parser(["0", "1", "2", "3", "all"]),
        )
        .arg(
            Arg::new("host-baud")
                .long("host-baud")
                .default_value("12000000")
                .value_parser(value_parser!(u32).range(9600..=12000000)),
        )
        .arg(shift())
        .arg(
            Arg::new("timeout-ms")
                .long("timeout-ms")
                .default_value("1000")
                .value_parser(value_parser!(u32).range(10..=10000)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_schemas_and_invalid_hardware_parameters() {
        for name in [
            "m64-board-info",
            "controlboard-misc",
            "fan-tool",
            "p738b-psu-tool",
            "hashboard-m64",
            "i2c-probe",
        ] {
            command(name).debug_assert();
        }
        for argv in [
            vec!["fan-tool", "set-pwm", "all", "101"],
            vec!["hashboard-m64", "reset", "2", "release"],
            vec!["p738b-psu-tool", "set-voltage", "26000"],
            vec!["hashboard-m64", "check", "0", "--timeout-ms", "0"],
        ] {
            assert!(command(argv[0]).try_get_matches_from(argv).is_err());
        }
    }
}
