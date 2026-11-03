//! CLI tests against temporary filesystem fixtures; no hardware access.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Board(PathBuf);

fn tool_command(binary: &str) -> Command {
    let path = std::env::var_os("M64_TEST_BIN_DIR")
        .map(|dir| PathBuf::from(dir).join(std::path::Path::new(binary).file_name().unwrap()))
        .unwrap_or_else(|| PathBuf::from(binary));
    Command::new(path)
}

impl Board {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "m64-tools-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("tmp")).unwrap();
        fs::create_dir_all(root.join("proc")).unwrap();
        Self(root)
    }

    fn put(&self, path: &str, value: &[u8]) {
        let path = self.0.join(path.trim_start_matches('/'));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    fn run(&self, binary: &str, args: &[&str]) -> Output {
        tool_command(binary)
            .arg("--root")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Board {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn stock_psu_board() -> Board {
    let board = Board::new();
    for (path, value) in [
        ("/sys/bitmicro/platform", "ALLWINNER_H616"),
        ("/sys/bitmicro/hw_name", "CB6"),
        ("/sys/bitmicro/hw_version", "5"),
        ("/sys/bitmicro/power/model", "P738B"),
        ("/sys/bitmicro/power/enable", "0"),
        ("/sys/bitmicro/power/vout", "0"),
        ("/sys/bitmicro/power/iout", "0"),
        ("/sys/bitmicro/power/vout_set", "1855"),
        ("/sys/class/gpio/gpio201/value", "0"),
        ("/sys/class/gpio/gpio202/value", "0"),
        ("/sys/class/gpio/gpio263/value", "0"),
    ] {
        board.put(path, value.as_bytes());
    }
    let link = board.0.join("sys/bus/i2c/devices/4-0058/driver");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink("../../drivers/pmbus-power", link).unwrap();
    board
}

#[test]
fn stock_psu_voltage_and_disable_use_driver_without_native_i2c() {
    let board = stock_psu_board();
    let binary = env!("CARGO_BIN_EXE_p738b-psu-tool");
    let result = board.run(binary, &["set-voltage", "21000", "--stock"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/vout_set")).unwrap(),
        b"2100"
    );
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/enable")).unwrap(),
        b"0"
    );
    board.put("/sys/bitmicro/power/enable", b"1");
    let result = board.run(binary, &["disable", "--stock"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/enable")).unwrap(),
        b"0"
    );
    assert!(!board.0.join("dev/i2c-4").exists());
}

#[test]
fn stock_psu_voltage_refuses_energized_or_unreset_state() {
    for path in [
        "/sys/bitmicro/power/enable",
        "/sys/bitmicro/power/vout",
        "/sys/bitmicro/power/iout",
        "/sys/class/gpio/gpio201/value",
        "/sys/class/gpio/gpio202/value",
        "/sys/class/gpio/gpio263/value",
    ] {
        let board = stock_psu_board();
        board.put(path, b"1");
        let result = board.run(
            env!("CARGO_BIN_EXE_p738b-psu-tool"),
            &["set-voltage", "21000", "--stock"],
        );
        assert!(!result.status.success(), "unsafe state accepted: {path}");
        assert_eq!(
            fs::read(board.0.join("sys/bitmicro/power/vout_set")).unwrap(),
            b"1855"
        );
    }
}

#[test]
fn stock_psu_control_rejects_wrong_profile_driver_and_voltage_resolution() {
    for (path, value) in [
        ("/sys/bitmicro/platform", "OTHER"),
        ("/sys/bitmicro/hw_version", "4"),
        ("/sys/bitmicro/power/model", "OTHER"),
    ] {
        let board = stock_psu_board();
        board.put(path, value.as_bytes());
        board.put("/sys/bitmicro/power/enable", b"1");
        let result = board.run(
            env!("CARGO_BIN_EXE_p738b-psu-tool"),
            &["disable", "--stock"],
        );
        assert!(!result.status.success());
        assert_eq!(
            fs::read(board.0.join("sys/bitmicro/power/enable")).unwrap(),
            b"1"
        );
    }
    let board = stock_psu_board();
    let result = board.run(
        env!("CARGO_BIN_EXE_p738b-psu-tool"),
        &["set-voltage", "21001", "--stock"],
    );
    assert!(!result.status.success());
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/vout_set")).unwrap(),
        b"1855"
    );
    fs::remove_file(board.0.join("sys/bus/i2c/devices/4-0058/driver")).unwrap();
    let result = board.run(
        env!("CARGO_BIN_EXE_p738b-psu-tool"),
        &["set-voltage", "21000", "--stock"],
    );
    assert!(!result.status.success());
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/vout_set")).unwrap(),
        b"1855"
    );
}

fn trial_board() -> Board {
    let board = stock_psu_board();
    for (path, value) in [
        ("/sys/class/gpio/gpio193/value", "0"),
        ("/sys/class/gpio/gpio194/value", "0"),
        ("/tmp/eeprom/miner_type", "M64_VL20"),
        ("/proc/sys/kernel/random/boot_id", "fixture-boot"),
        ("/proc/sys/kernel/tainted", "0"),
        ("/sys/bitmicro/keepalive/enable", "0"),
        ("/data/m64-cb-tools/maintenance/ARMED", ""),
        ("/sys/bitmicro/power/vout_set", "2100"),
        ("/sys/bitmicro/power/status", "0x0"),
        ("/sys/bitmicro/power/common_status", "0x0"),
        ("/sys/bitmicro/power/pout", "0"),
        ("/sys/bitmicro/power/temp0", "200"),
        ("/sys/bitmicro/power/temp1", "200"),
        ("/sys/bitmicro/power/temp2", "200"),
        (
            "/sys/bus/i2c/devices/10-0048/hwmon/hwmon1/temp1_input",
            "20000",
        ),
        (
            "/sys/bus/i2c/devices/10-0049/hwmon/hwmon2/temp1_input",
            "20000",
        ),
    ] {
        board.put(path, value.as_bytes());
    }
    for channel in 0..2 {
        for (attr, value) in [("enable", "1"), ("zero_speed", "0"), ("pwm_duty", "99")] {
            board.put(
                &format!("/sys/bitmicro/fan/fan{channel}/{attr}"),
                value.as_bytes(),
            );
        }
    }
    for gpio in [201, 202, 263] {
        board.put(&format!("/sys/class/gpio/gpio{gpio}/direction"), b"low\n");
    }
    board
}

#[test]
fn power_trial_refuses_a_high_reset_return_before_power_changes() {
    let board = trial_board();
    board.put("/sys/class/gpio/gpio193/value", b"1");
    let result = board.run(
        env!("CARGO_BIN_EXE_p738b-psu-tool"),
        &["--expect-boot-id", "fixture-boot", "trial"],
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("reset-return"));
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/enable")).unwrap(),
        b"0"
    );
    assert_eq!(
        fs::read(board.0.join("sys/class/gpio/gpio263/direction")).unwrap(),
        b"low\n"
    );
}

#[test]
fn power_guard_deadline_turns_mock_psu_off_and_parent_loss_also_shuts_down() {
    use std::sync::{Arc, atomic::AtomicBool};
    use std::time::Duration;
    for dry in [false, true] {
        let board = trial_board();
        let mut command = tool_command(env!("CARGO_BIN_EXE_p738b-psu-tool"));
        command
            .arg("--root")
            .arg(&board.0)
            .args([
                "--expect-boot-id",
                "fixture-boot",
                "_guard",
                "--max-on-ms",
                "200",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if dry {
            command.arg("--dry-run");
        }
        let mut child = command.spawn().unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        assert_eq!(line, "READY\n");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_driver = stop.clone();
        let root = board.0.clone();
        // Atomic rename models telemetry transitions without partially written
        // fixture reads. This accesses only the temporary filesystem.
        let driver = std::thread::spawn(move || {
            while !stop_driver.load(Ordering::Relaxed) {
                let on = fs::read(root.join("sys/bitmicro/power/enable")).unwrap() == b"1";
                let next = root.join("sys/bitmicro/power/vout.next");
                fs::write(
                    &next,
                    if on {
                        b"2100".as_slice()
                    } else {
                        b"0".as_slice()
                    },
                )
                .unwrap();
                fs::rename(next, root.join("sys/bitmicro/power/vout")).unwrap();
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        board.put("/sys/class/gpio/gpio263/value", b"1");
        input.write_all(b"ENABLE\n").unwrap();
        line.clear();
        output.read_line(&mut line).unwrap();
        assert!(line.starts_with("ENABLED"), "{line}");
        if dry {
            drop(input);
        } // Simulate parent loss with the only writer gone.
        let mut log = String::new();
        output.read_to_string(&mut log).unwrap();
        let status = child.wait().unwrap();
        stop.store(true, Ordering::Relaxed);
        driver.join().unwrap();
        let mut errors = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut errors)
            .unwrap();
        assert!(status.success(), "{log}\n{errors}");
        assert!(
            log.contains(if dry {
                "STOP parent pipe closed"
            } else {
                "STOP deadline"
            }),
            "{log}"
        );
        assert!(log.contains("OFF enable/Vout/Iout=0/0/0"));
        assert_eq!(
            fs::read(board.0.join("sys/bitmicro/power/enable")).unwrap(),
            b"0"
        );
        for gpio in [201, 202, 263] {
            assert_eq!(
                fs::read(board.0.join(format!("sys/class/gpio/gpio{gpio}/direction"))).unwrap(),
                b"low\n"
            );
        }
    }
}

#[test]
fn power_guard_sensor_fault_cuts_output_without_diagnostic_session_lock() {
    let board = trial_board();
    let mut child = tool_command(env!("CARGO_BIN_EXE_p738b-psu-tool"))
        .arg("--root")
        .arg(&board.0)
        .args(["--expect-boot-id", "fixture-boot", "_guard", "--dry-run"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line, "READY\n");
    board.put("/sys/class/gpio/gpio263/value", b"1");
    input.write_all(b"ENABLE\n").unwrap();
    line.clear();
    output.read_line(&mut line).unwrap();
    assert!(line.starts_with("ENABLED"));
    board.put(
        "/sys/bus/i2c/devices/10-0048/hwmon/hwmon1/temp1_input",
        b"45000",
    );
    let mut log = String::new();
    output.read_to_string(&mut log).unwrap();
    assert!(!child.wait().unwrap().success());
    assert!(log.contains("OFF enable/Vout/Iout=0/0/0"));
    assert_eq!(
        fs::read(board.0.join("sys/class/gpio/gpio263/direction")).unwrap(),
        b"low\n"
    );
    assert!(!board.0.join("tmp/m64-cb-tools.lock").exists());
}

#[test]
fn power_guard_losing_both_parent_pipes_still_shuts_down() {
    let board = trial_board();
    let mut child = tool_command(env!("CARGO_BIN_EXE_p738b-psu-tool"))
        .arg("--root")
        .arg(&board.0)
        .args(["--expect-boot-id", "fixture-boot", "_guard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line, "READY\n");
    board.put("/sys/class/gpio/gpio263/value", b"1");
    input.write_all(b"ENABLE\n").unwrap();
    line.clear();
    output.read_line(&mut line).unwrap();
    assert!(line.starts_with("ENABLED"));
    // Model the loss of the parent's stdout reader as well as stdin writer.
    // Logging a broken pipe must never bypass emergency shutdown.
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/enable")).unwrap(),
        b"1"
    );
    drop(output);
    drop(input);
    let _ = child.wait().unwrap();
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/enable")).unwrap(),
        b"0"
    );
    assert_eq!(
        fs::read(board.0.join("sys/class/gpio/gpio263/direction")).unwrap(),
        b"low\n"
    );
}

#[test]
fn power_guard_rejects_enable_queued_after_parent_disconnect() {
    let board = trial_board();
    let mut child = tool_command(env!("CARGO_BIN_EXE_p738b-psu-tool"))
        .arg("--root")
        .arg(&board.0)
        .args(["--expect-boot-id", "fixture-boot", "_guard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line, "READY\n");
    // Freeze only a fixture process so ENABLE and hangup arrive together.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGSTOP) }, 0);
    std::thread::sleep(std::time::Duration::from_millis(20));
    board.put("/sys/class/gpio/gpio263/value", b"1");
    input.write_all(b"ENABLE\n").unwrap();
    drop(input);
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGCONT) }, 0);
    let mut log = String::new();
    output.read_to_string(&mut log).unwrap();
    let _ = child.wait().unwrap();
    assert!(!log.contains("ENABLED"), "{log}");
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/enable")).unwrap(),
        b"0"
    );
}

#[test]
fn power_guard_survives_parent_group_termination_and_retains_hardware_lock() {
    use std::os::unix::process::CommandExt;
    use std::sync::{Arc, atomic::AtomicBool};
    use std::time::{Duration, Instant};
    let board = trial_board();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let root = board.0.clone();
    let gpio = std::thread::spawn(move || {
        while !thread_stop.load(Ordering::Relaxed) {
            let high =
                fs::read(root.join("sys/class/gpio/gpio263/direction")).unwrap() == b"high\n";
            let next = root.join("sys/class/gpio/gpio263/next");
            fs::write(&next, if high { b"1" } else { b"0" }).unwrap();
            fs::rename(next, root.join("sys/class/gpio/gpio263/value")).unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    let mut child = tool_command(env!("CARGO_BIN_EXE_p738b-psu-tool"))
        .arg("--root")
        .arg(&board.0)
        .args([
            "--expect-boot-id",
            "fixture-boot",
            "trial",
            "--dry-run",
            "--max-on-ms",
            "5000",
        ])
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    let guard: i32 = line
        .trim()
        .strip_prefix("GUARD pid=")
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(unsafe { libc::getpgid(guard) }, child.id() as i32);
    let ready = Instant::now() + Duration::from_secs(1);
    while fs::read(board.0.join("sys/class/gpio/gpio263/value")).unwrap() != b"1"
        && Instant::now() < ready
    {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        fs::read(board.0.join("sys/class/gpio/gpio263/value")).unwrap(),
        b"1"
    );
    // Keep the detached fixture guard alive while proving its inherited lease
    // outlives the parent. No real device is involved in this SIGSTOP case.
    assert_eq!(unsafe { libc::kill(guard, libc::SIGSTOP) }, 0);
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(
        unsafe { libc::kill(-(child.id() as i32), libc::SIGTERM) },
        0
    );
    assert!(!child.wait().unwrap().success());
    let refusal = board.run(env!("CARGO_BIN_EXE_fan-tool"), &["set-pwm", "0", "99"]);
    assert!(!refusal.status.success());
    assert!(String::from_utf8_lossy(&refusal.stderr).contains("another M64 tool"));
    assert_eq!(unsafe { libc::kill(guard, libc::SIGCONT) }, 0);
    let deadline = Instant::now() + Duration::from_secs(1);
    while fs::read_to_string(board.0.join("sys/class/gpio/gpio263/direction"))
        .unwrap()
        .trim()
        != "low"
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(2));
    }
    stop.store(true, Ordering::Relaxed);
    gpio.join().unwrap();
    assert_eq!(
        fs::read_to_string(board.0.join("sys/class/gpio/gpio263/direction"))
            .unwrap()
            .trim(),
        "low"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn power_guard_full_log_pipe_cannot_block_shutdown() {
    use std::os::fd::AsRawFd;
    use std::time::Instant;
    let board = trial_board();
    let mut child = tool_command(env!("CARGO_BIN_EXE_p738b-psu-tool"))
        .arg("--root")
        .arg(&board.0)
        .args([
            "--expect-boot-id",
            "fixture-boot",
            "_guard",
            "--dry-run",
            "--max-on-ms",
            "5000",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    assert!(unsafe { libc::fcntl(output.get_ref().as_raw_fd(), libc::F_SETPIPE_SZ, 4096) } >= 0);
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line, "READY\n");
    board.put("/sys/class/gpio/gpio263/value", b"1");
    input.write_all(b"ENABLE\n").unwrap();
    line.clear();
    output.read_line(&mut line).unwrap();
    assert!(line.starts_with("ENABLED"));
    let start = Instant::now();
    // Intentionally retain but do not drain stdout. The small pipe fills
    // before the five-second deadline; nonblocking logging must fail off.
    assert!(!child.wait().unwrap().success());
    assert!(start.elapsed().as_millis() < 4500);
    assert_eq!(
        fs::read(board.0.join("sys/class/gpio/gpio263/direction")).unwrap(),
        b"low\n"
    );
    let mut errors = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut errors)
        .unwrap();
    assert!(errors.contains("os error 11"), "{errors}");
}

#[test]
fn power_guard_gates_reset_release_on_time_voltage_and_selected_board() {
    use std::sync::{Arc, atomic::AtomicBool};
    use std::time::Duration;
    for early in [true, false] {
        let board = trial_board();
        let mut child = tool_command(env!("CARGO_BIN_EXE_p738b-psu-tool"))
            .arg("--root")
            .arg(&board.0)
            .args([
                "--expect-boot-id",
                "fixture-boot",
                "_guard",
                "--id-slot",
                "0",
                "--max-on-ms",
                "2000",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        assert_eq!(line, "READY\n");
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let root = board.0.clone();
        let driver = std::thread::spawn(move || {
            let write = |path: &str, value: &[u8]| {
                let dest = root.join(path);
                let next = dest.with_extension("next");
                fs::write(&next, value).unwrap();
                fs::rename(next, dest).unwrap();
            };
            while !thread_stop.load(Ordering::Relaxed) {
                let on = fs::read(root.join("sys/bitmicro/power/enable")).unwrap() == b"1";
                write("sys/bitmicro/power/vout", if on { b"2100" } else { b"0" });
                let released = fs::read_to_string(root.join("sys/class/gpio/gpio201/direction"))
                    .unwrap()
                    .trim()
                    == "high";
                for path in [
                    "sys/class/gpio/gpio201/value",
                    "sys/class/gpio/gpio193/value",
                ] {
                    write(path, if released { b"1" } else { b"0" });
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        board.put("/sys/class/gpio/gpio263/value", b"1");
        input.write_all(b"ENABLE\n").unwrap();
        line.clear();
        output.read_line(&mut line).unwrap();
        assert!(line.starts_with("ENABLED"));
        if !early {
            loop {
                line.clear();
                assert!(output.read_line(&mut line).unwrap() > 0);
                let elapsed = line
                    .split_whitespace()
                    .find_map(|s| s.strip_prefix("elapsed_ms="))
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);
                if elapsed >= 1050 {
                    break;
                }
            }
        }
        input.write_all(b"RELEASE\n").unwrap();
        let mut released = false;
        if !early {
            loop {
                line.clear();
                if output.read_line(&mut line).unwrap() == 0 {
                    break;
                }
                if line.starts_with("RELEASED board=0 ") {
                    released = true;
                    break;
                }
            }
            assert!(released);
            assert_eq!(
                fs::read(board.0.join("sys/class/gpio/gpio201/value")).unwrap(),
                b"1"
            );
            assert_eq!(
                fs::read(board.0.join("sys/class/gpio/gpio193/value")).unwrap(),
                b"1"
            );
            assert_eq!(
                fs::read(board.0.join("sys/class/gpio/gpio202/value")).unwrap(),
                b"0"
            );
            assert_eq!(
                fs::read(board.0.join("sys/class/gpio/gpio194/value")).unwrap(),
                b"0"
            );
        }
        if !early {
            drop(input);
        }
        let mut log = String::new();
        output.read_to_string(&mut log).unwrap();
        let status = child.wait().unwrap();
        stop.store(true, Ordering::Relaxed);
        driver.join().unwrap();
        assert_eq!(status.success(), !early);
        assert!(log.contains("OFF enable/Vout/Iout=0/0/0"), "{log}");
        assert_eq!(
            fs::read(board.0.join("sys/bitmicro/power/enable")).unwrap(),
            b"0"
        );
        assert_eq!(
            fs::read_to_string(board.0.join("sys/class/gpio/gpio201/direction"))
                .unwrap()
                .trim(),
            "low"
        );
    }
}

#[test]
fn active_miner_blocks_mutation_and_native_bus_reads() {
    for miner in ["btminer", "btminer_test", "power-aging", "mujina-minerd"] {
        let board = Board::new();
        board.put("/proc/42/comm", format!("{miner}\n").as_bytes());
        board.put("/sys/bitmicro/fan/fan0/pwm_duty", b"99\n");
        let result = board.run(env!("CARGO_BIN_EXE_fan-tool"), &["set-pwm", "0", "10"]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains(&format!("{miner} (PID 42)")));
        assert_eq!(
            fs::read(board.0.join("sys/bitmicro/fan/fan0/pwm_duty")).unwrap(),
            b"99\n"
        );
        let result = board.run(env!("CARGO_BIN_EXE_i2c-probe"), &[]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("hardware is owned"));
        board.put("/sys/bitmicro/power/vout_set", b"1855");
        for args in [vec!["set-voltage", "21000", "--stock"], vec!["trial"]] {
            let result = board.run(env!("CARGO_BIN_EXE_p738b-psu-tool"), &args);
            assert!(!result.status.success());
            assert!(String::from_utf8_lossy(&result.stderr).contains("hardware is owned"));
            assert_eq!(
                fs::read(board.0.join("sys/bitmicro/power/vout_set")).unwrap(),
                b"1855"
            );
        }
    }
}

#[test]
fn status_does_not_export_gpio_or_create_missing_hardware() {
    let board = Board::new();
    board.put("/sys/class/gpio/export", b"unchanged");
    let result = board.run(env!("CARGO_BIN_EXE_controlboard-misc"), &["status"]);
    assert!(result.status.success());
    assert_eq!(
        fs::read(board.0.join("sys/class/gpio/export")).unwrap(),
        b"unchanged"
    );
    assert!(!board.0.join("sys/class/gpio/gpio257").exists());
    let result = board.run(env!("CARGO_BIN_EXE_fan-tool"), &["status"]);
    assert!(!result.status.success());
    let result = board.run(env!("CARGO_BIN_EXE_hashboard-m64"), &["temps"]);
    assert!(!result.status.success());
}

#[test]
fn eeprom_uses_kernel_node_while_mining_and_reports_crc_mismatch() {
    let board = Board::new();
    board.put("/proc/42/comm", b"btminer\n");
    let mut image = [0xff; 128];
    image[..2].copy_from_slice(&[0xfa, 0x5a]);
    image[5..25].copy_from_slice(b"HLM40XC0205616K60174");
    let crc = m64_cb_tools::protocol::crc8(&image[5..=0x77]);
    image[0x78] = crc ^ 1;
    board.put("/sys/bus/i2c/devices/10-0050/eeprom", &image);
    let result = board.run(env!("CARGO_BIN_EXE_hashboard-m64"), &["eeprom", "0"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("HLM40XC0205616K60174"));
    assert!(String::from_utf8_lossy(&result.stderr).contains("CRC mismatch"));
}

#[test]
fn shared_io_rail_requires_both_resets_asserted() {
    let board = Board::new();
    board.put("/sys/class/gpio/gpio201/value", b"0\n");
    board.put("/sys/class/gpio/gpio202/value", b"1\n");
    board.put("/sys/class/gpio/gpio263/direction", b"unchanged\n");
    let result = board.run(env!("CARGO_BIN_EXE_hashboard-m64"), &["io-power", "off"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("BOTH"));
    assert_eq!(
        fs::read(board.0.join("sys/class/gpio/gpio263/direction")).unwrap(),
        b"unchanged\n"
    );
}

#[test]
fn offline_frame_decoder_checks_crc_without_proc_or_device_access() {
    let board = Board::new();
    let result = board.run(
        env!("CARGO_BIN_EXE_hashboard-m64"),
        &["decode-frame", "00000002198031", "--shift", "0"],
    );
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("payload=1980"));
    let result = board.run(
        env!("CARGO_BIN_EXE_hashboard-m64"),
        &["decode-frame", "00000002198030", "--shift", "0"],
    );
    assert!(!result.status.success());
}

#[test]
fn disconnected_psu_placeholders_are_not_valid_telemetry() {
    let board = Board::new();
    board.put("/sys/bitmicro/power/name", b"unknow\n");
    board.put("/sys/bitmicro/power/model", b"(null)\n");
    board.put("/sys/bitmicro/power/vout", b"6553500\n");
    let result = board.run(env!("CARGO_BIN_EXE_p738b-psu-tool"), &["status"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("PSU unavailable"));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("6553500"));
}

#[test]
fn thermal_zones_are_available_without_soc_hwmon_nodes() {
    let board = Board::new();
    board.put("/sys/bitmicro/power/water_temp", b"UNSUPPORTED\n");
    board.put(
        "/sys/class/thermal/thermal_zone0/type",
        b"cpu_thermal_zone\n",
    );
    board.put("/sys/class/thermal/thermal_zone0/temp", b"39833\n");
    let result = board.run(env!("CARGO_BIN_EXE_hashboard-m64"), &["temps"]);
    assert!(result.status.success());
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("cpu_thermal_zone") && output.contains("39833"));
    assert!(!output.contains("/sys/bitmicro/power/water_temp"));
}

#[test]
fn powered_psu_status_reads_indexed_telemetry_without_mutating_setpoints() {
    let board = Board::new();
    board.put("/sys/bitmicro/power/water_temp", b"UNSUPPORTED\n");
    board.put("/sys/bitmicro/power/vin0", b"UNSUPPORTED\n");
    board.put("/proc/42/comm", b"btminer\n");
    for (name, value) in [
        ("name", "P738B"),
        ("model", "P738B"),
        ("enable", "1"),
        ("vout_set", "2255"),
        ("fan_speed0", "6000"),
        ("temp0", "290"),
        ("temp1", "367"),
        ("temp2", "325"),
        ("pout", "3794"),
    ] {
        board.put(&format!("/sys/bitmicro/power/{name}"), value.as_bytes());
    }
    let result = board.run(env!("CARGO_BIN_EXE_p738b-psu-tool"), &["status"]);
    assert!(result.status.success());
    let output = String::from_utf8_lossy(&result.stdout);
    for expected in [
        "fan_speed0: 6000",
        "temp0: 290",
        "temp1: 367",
        "temp2: 325",
        "pout: 3794",
    ] {
        assert!(output.contains(expected), "missing {expected}");
    }
    assert!(!output.contains("/power/fan_speed: UNAVAILABLE"));
    assert!(!output.contains("/power/temp: UNAVAILABLE"));
    assert!(!output.contains("/power/water_temp:"));
    assert!(!output.contains("/power/vin0:"));
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/enable")).unwrap(),
        b"1"
    );
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/power/vout_set")).unwrap(),
        b"2255"
    );
}

#[test]
fn kernel_exception_blocks_hardware_commands_but_allows_observation() {
    let board = Board::new();
    board.put("/proc/sys/kernel/tainted", b"512\n");
    board.put("/sys/bitmicro/fan/fan0/pwm_duty", b"99\n");
    let result = board.run(env!("CARGO_BIN_EXE_fan-tool"), &["set-pwm", "0", "99"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("kernel taint is 512"));
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/fan/fan0/pwm_duty")).unwrap(),
        b"99\n"
    );
    let result = board.run(env!("CARGO_BIN_EXE_fan-tool"), &["status"]);
    assert!(result.status.success());
}

#[test]
fn unexpected_reboot_blocks_hardware_session() {
    let board = Board::new();
    board.put("/proc/sys/kernel/random/boot_id", b"new-boot\n");
    board.put("/sys/bitmicro/fan/fan0/pwm_duty", b"99\n");
    let result = board.run(
        env!("CARGO_BIN_EXE_fan-tool"),
        &["--expect-boot-id", "old-boot", "set-pwm", "0", "99"],
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("boot ID changed"));
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/fan/fan0/pwm_duty")).unwrap(),
        b"99\n"
    );
    let result = board.run(
        env!("CARGO_BIN_EXE_controlboard-misc"),
        &["--expect-boot-id", "new-boot", "status"],
    );
    assert!(result.status.success());
}
