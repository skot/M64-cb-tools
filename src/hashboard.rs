//! Physical-board EEPROM, temperature, reset and BT1980 link diagnostics.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use clap::ArgMatches;

use crate::{
    diagnostics,
    i2c::I2c,
    protocol,
    system::{System, UARTS, number, selection},
    uart::Uart,
};

pub fn run(sys: &System, args: &ArgMatches) -> Result<()> {
    let (command, values) = args.subcommand().expect("required subcommand");
    match command {
        "status" => {
            for (slot, uart) in UARTS.iter().enumerate() {
                println!(
                    "slot={slot} board={} uart={uart} present={}",
                    slot / 2,
                    sys.path(uart).exists()
                );
            }
            for gpio in [201, 202, 193, 194, 263] {
                println!(
                    "GPIO{gpio}: {}",
                    sys.gpio_read(gpio).unwrap_or_else(|_| "unexported".into())
                );
            }
            Ok(())
        }
        "temps" => {
            if !values.get_flag("native") {
                return diagnostics::temps(sys);
            }
            let _session = sys.session()?;
            let mut failures = 0;
            for (bus, address, name) in [
                (10, 0x48, "board0"),
                (10, 0x49, "board1"),
                (4, 0x4f, "inlet"),
            ] {
                let result = (|| -> Result<()> {
                    let device = I2c::open(&sys.path(format!("/dev/i2c-{bus}")), address, false)?;
                    let raw = device.read_at(address, 0, 2)?;
                    println!(
                        "{name}: {:.3} C raw={}",
                        protocol::lm75([raw[0], raw[1]]),
                        hex::encode(raw)
                    );
                    Ok(())
                })();
                if let Err(error) = result {
                    failures += 1;
                    eprintln!("{name}: ERROR ({error:#})");
                }
            }
            ensure!(failures == 0, "{failures} temperature reads failed");
            Ok(())
        }
        "eeprom" => {
            let board = *values.get_one::<u8>("board").unwrap();
            let address = 0x50 + u16::from(board);
            let path = format!("/sys/bus/i2c/devices/10-00{address:02x}/eeprom");
            let bytes = if sys.path(&path).exists() {
                let mut data = vec![0; 128];
                File::open(sys.path(&path))?
                    .read_exact(&mut data)
                    .context("read stock EEPROM sysfs image")?;
                data
            } else {
                let _session = sys.session()?;
                I2c::open(&sys.path("/dev/i2c-10"), address, false)?.read_at(address, 0, 128)?
            };
            protocol::print_eeprom(&bytes)
        }
        "decode-eeprom" => {
            protocol::print_eeprom(&std::fs::read(values.get_one::<String>("file").unwrap())?)
        }
        "decode-frame" => {
            let shift = *values.get_one::<u32>("shift").unwrap();
            let bytes: Vec<_> = hex::decode(values.get_one::<String>("hex").unwrap())?
                .into_iter()
                .map(|b| b.rotate_left(shift))
                .collect();
            let (id, kind, payload) = protocol::response(&bytes)?;
            println!("id={id} type={kind} payload={}", hex::encode(payload));
            Ok(())
        }
        "reset" => {
            let _session = sys.session()?;
            let board = *values.get_one::<u8>("board").unwrap();
            let release = values.get_one::<String>("state").unwrap() == "release";
            sys.gpio_set(201 + u32::from(board), release)?;
            println!(
                "board={board} slots={}:{} reset={}",
                board * 2,
                board * 2 + 1,
                if release { "released" } else { "asserted" }
            );
            Ok(())
        }
        "io-power" => {
            let _session = sys.session()?;
            let on = values.get_one::<String>("state").unwrap() == "on";
            // Both reset lines must be asserted before changing the shared rail.
            ensure!(
                sys.gpio_read(201)? == "0" && sys.gpio_read(202)? == "0",
                "assert BOTH physical-board resets before changing shared I/O power"
            );
            sys.gpio_set(263, on)?;
            if on {
                std::thread::sleep(Duration::from_millis(1013));
            }
            println!(
                "shared switched LDO={}; both resets remain asserted",
                if on { "on" } else { "off" }
            );
            Ok(())
        }
        "check" | "read-register" => uart(sys, command, values),
        _ => unreachable!(),
    }
}

fn uart(sys: &System, command: &str, values: &ArgMatches) -> Result<()> {
    let slots = selection(values.get_one::<String>("slot").unwrap(), 4)?;
    let baud = *values.get_one::<u32>("host-baud").unwrap();
    let shift = *values.get_one::<u32>("shift").unwrap();
    let timeout = Duration::from_millis(u64::from(*values.get_one::<u32>("timeout-ms").unwrap()));
    let (chip, register, length) = if command == "check" {
        (0xfff, 0x100, 2)
    } else {
        (
            number(values.get_one::<String>("chip").unwrap())?,
            number(values.get_one::<String>("register").unwrap())?,
            u32::from(*values.get_one::<u8>("length").unwrap()),
        )
    };
    ensure!(
        chip <= 0xfff && register <= 0xfff,
        "chip and register must fit 12 bits"
    );
    let tx = protocol::read_command(chip as u16, register as u16, length as u8, shift)?;
    let _session = sys.session()?;
    let mut failures = 0;
    for slot in slots {
        let result = (|| -> Result<()> {
            let mut port = Uart::open(&sys.path(UARTS[slot]), baud)?;
            println!(
                "slot={slot} host_baud={baud} shift={shift} TX={}",
                hex::encode(&tx)
            );
            let frames = port.read_register(&tx, shift, timeout)?;
            let mut identifiers = BTreeSet::new();
            let mut unrelated = 0;
            for frame in &frames {
                let (id, kind, payload) = protocol::response(frame)?;
                println!(
                    "slot={slot} RX={} id={id} type={kind} payload={}",
                    hex::encode(frame),
                    hex::encode(payload)
                );
                if kind == 0
                    && payload.len() == length as usize
                    && (chip == 0xfff || u32::from(id) == chip)
                {
                    if command == "check" {
                        ensure!(
                            payload == [0x19, 0x80],
                            "unexpected ASIC identity from {id}"
                        );
                    }
                    identifiers.insert(id);
                } else {
                    unrelated += 1;
                }
            }
            println!(
                "slot={slot} valid_frames={} matching_unique_ids={} unrelated={unrelated}",
                frames.len(),
                identifiers.len()
            );
            if command == "check" {
                let expected = *values.get_one::<u16>("expected-chips").unwrap();
                ensure!(
                    identifiers == (0..expected).collect(),
                    "expected contiguous identifiers 0..{} for {expected} chips",
                    expected - 1
                );
            } else {
                ensure!(!identifiers.is_empty(), "no matching register replies");
            }
            Ok(())
        })();
        if let Err(error) = result {
            failures += 1;
            eprintln!("slot={slot}: ERROR ({error:#})");
        }
    }
    ensure!(failures == 0, "{failures} UART checks failed");
    Ok(())
}
