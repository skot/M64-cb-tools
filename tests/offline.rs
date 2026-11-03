//! CLI tests against temporary filesystem fixtures; no hardware access.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Board(PathBuf);

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
        Command::new(binary)
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

#[test]
fn active_miner_blocks_mutation_and_native_bus_reads() {
    let board = Board::new();
    board.put("/proc/42/comm", b"btminer\n");
    board.put("/sys/bitmicro/fan/fan0/pwm_duty", b"99\n");
    let result = board.run(env!("CARGO_BIN_EXE_fan-tool"), &["set-pwm", "0", "10"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("btminer (PID 42)"));
    assert_eq!(
        fs::read(board.0.join("sys/bitmicro/fan/fan0/pwm_duty")).unwrap(),
        b"99\n"
    );
    let result = board.run(env!("CARGO_BIN_EXE_i2c-probe"), &[]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("hardware is owned"));
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
    board.put(
        "/sys/class/thermal/thermal_zone0/type",
        b"cpu_thermal_zone\n",
    );
    board.put("/sys/class/thermal/thermal_zone0/temp", b"39833\n");
    let result = board.run(env!("CARGO_BIN_EXE_hashboard-m64"), &["temps"]);
    assert!(result.status.success());
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("cpu_thermal_zone") && output.contains("39833"));
}
