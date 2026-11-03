//! Short reset-held characterization; only the separate guard can enable PSU output.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use clap::ArgMatches;

use crate::{
    cold_prefix, protocol,
    system::{System, UARTS},
    uart::Uart,
};

const POWER: &str = "/sys/bitmicro/power";
const TARGET_CV: u32 = 2100;
static CANCEL: AtomicBool = AtomicBool::new(false);

extern "C" fn request_shutdown(_: libc::c_int) {
    CANCEL.store(true, Ordering::Relaxed);
}

fn configure_guard_process() -> Result<()> {
    // Signals only set a lock-free flag; all hardware I/O stays in the loop.
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        // SAFETY: sigaction is a C POD and the handler has the required ABI.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = request_shutdown as *const () as usize;
        // SAFETY: valid initialized sigaction storage, no old-action output.
        ensure!(
            unsafe { libc::sigemptyset(&mut action.sa_mask) } == 0,
            "cannot initialize signal mask"
        );
        ensure!(
            unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } == 0,
            "cannot install shutdown signal handler"
        );
    }
    let fd = std::io::stdout().as_raw_fd();
    // SAFETY: F_GETFL/F_SETFL use the live stdout fd and integer flags only.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    ensure!(
        flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == 0,
        "cannot make guardian logging nonblocking"
    );
    Ok(())
}
#[derive(Clone, Copy)]
struct Limits {
    current: u32,
    power: u32,
}
impl Limits {
    fn from_args(args: &ArgMatches) -> Self {
        Self {
            current: *args.get_one::<u32>("max-current-ma").unwrap(),
            power: *args.get_one::<u32>("max-power-w").unwrap(),
        }
    }
}

struct Sample {
    voltage: u32,
    current: u32,
    power: u32,
}

fn number(sys: &System, attr: &str) -> Result<u32> {
    sys.read(&format!("{POWER}/{attr}"))?
        .parse()
        .with_context(|| format!("invalid PSU {attr}"))
}

fn common(sys: &System, require_target: bool, limits: Limits) -> Result<Sample> {
    sys.verify_boot()?;
    for (path, expected) in [
        ("/sys/bitmicro/platform", "ALLWINNER_H616"),
        ("/sys/bitmicro/hw_name", "CB6"),
        ("/sys/bitmicro/hw_version", "5"),
        ("/sys/bitmicro/power/model", "P738B"),
        ("/tmp/eeprom/miner_type", "M64_VL20"),
        ("/sys/bitmicro/keepalive/enable", "0"),
        ("/proc/sys/kernel/tainted", "0"),
    ] {
        ensure!(
            sys.read(path)? == expected,
            "trial requires {path}={expected}"
        );
    }
    ensure!(
        sys.expected_boot_id.is_some(),
        "trial requires --expect-boot-id"
    );
    ensure!(
        sys.path("/data/m64-cb-tools/maintenance/ARMED").exists(),
        "persistent maintenance policy must be armed"
    );
    ensure!(sys.miners()?.is_empty(), "a miner is running");
    for pid in sys.children("/proc")? {
        if !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Ok(name) = sys.read(&format!("/proc/{pid}/comm")) {
            ensure!(
                !matches!(
                    name.as_str(),
                    "system-monitor" | "remote-daemon" | "api" | "api-v2" | "keyd"
                ),
                "vendor application {name} is running"
            );
        }
    }
    let driver = std::fs::read_link(sys.path("/sys/bus/i2c/devices/4-0058/driver"))?;
    ensure!(
        driver.file_name().is_some_and(|name| name == "pmbus-power"),
        "PSU driver changed"
    );
    for channel in 0..2 {
        for (attr, expected) in [("enable", "1"), ("zero_speed", "0"), ("pwm_duty", "99")] {
            ensure!(
                sys.read(&format!("/sys/bitmicro/fan/fan{channel}/{attr}"))? == expected,
                "cooling channel {channel} {attr} changed"
            );
        }
    }
    for attr in ["status", "common_status"] {
        let value = sys.read(&format!("{POWER}/{attr}"))?;
        let raw = value.strip_prefix("0x").unwrap_or(&value);
        ensure!(
            u64::from_str_radix(raw, if value.starts_with("0x") { 16 } else { 10 })? == 0,
            "PSU fault {attr}={value}"
        );
    }
    for device in ["10-0048", "10-0049"] {
        let base = format!("/sys/bus/i2c/devices/{device}/hwmon");
        let nodes = sys.children(&base)?;
        ensure!(
            nodes.len() == 1,
            "cannot identify board temperature {device}"
        );
        let temp: i32 = sys
            .read(&format!("{base}/{}/temp1_input", nodes[0]))?
            .parse()?;
        ensure!(
            (-10_000..=40_000).contains(&temp),
            "board {device} temperature outside characterization range: {temp} mC"
        );
    }
    for attr in ["temp0", "temp1", "temp2"] {
        ensure!(number(sys, attr)? <= 600, "PSU {attr} exceeds 60 C");
    }
    let current = number(sys, "iout")?;
    ensure!(
        current <= limits.current,
        "output current {current} mA exceeds {} mA characterization cutoff",
        limits.current
    );
    let power = number(sys, "pout")?;
    ensure!(
        power <= limits.power,
        "output power {power} W exceeds {} W characterization cutoff",
        limits.power
    );
    let voltage = number(sys, "vout")?;
    ensure!(voltage <= 2200, "output voltage exceeds 22 V cutoff");
    if require_target {
        ensure!(
            number(sys, "vout_set")? == TARGET_CV,
            "21 V setpoint changed"
        );
    }
    Ok(Sample {
        voltage,
        current,
        power,
    })
}

fn zero(sys: &System) -> Result<()> {
    for attr in ["enable", "vout", "iout"] {
        ensure!(number(sys, attr)? == 0, "PSU {attr} has not reached zero");
    }
    Ok(())
}

fn resets_asserted(sys: &System) -> Result<()> {
    ensure!(
        sys.gpio_read(201)? == "0" && sys.gpio_read(202)? == "0",
        "both resets must stay asserted during characterization"
    );
    ensure!(
        sys.gpio_read(193)? == "0" && sys.gpio_read(194)? == "0",
        "reset-return inputs must be low while resets are asserted"
    );
    Ok(())
}

fn reset_state(sys: &System, released: Option<u8>) -> Result<()> {
    for board in 0..2u8 {
        let expected = if released == Some(board) { "1" } else { "0" };
        ensure!(
            sys.gpio_read(201 + u32::from(board))? == expected,
            "unexpected reset output on board {board}"
        );
        ensure!(
            sys.gpio_read(193 + u32::from(board))? == expected,
            "unexpected reset return on board {board}"
        );
    }
    Ok(())
}

// Shutdown intentionally bypasses the diagnostic session, taint and miner
// guards: it must still cut output when one of those conditions becomes bad.
fn shutdown(sys: &System) -> Result<()> {
    let mut errors = Vec::new();
    for gpio in [201, 202, 263] {
        if let Err(e) = sys.gpio_set(gpio, false) {
            errors.push(format!("GPIO{gpio}: {e:#}"));
        }
    }
    let mut retry_disable = false;
    if let Err(e) = sys.write_decimal(&format!("{POWER}/enable"), 0) {
        errors.push(format!("PSU disable: {e:#}"));
        retry_disable = true;
    }
    let end = Instant::now() + Duration::from_secs(5);
    while zero(sys).is_err() && Instant::now() < end {
        thread::sleep(Duration::from_millis(100));
        if retry_disable || number(sys, "enable").ok() != Some(0) {
            retry_disable = sys.write_decimal(&format!("{POWER}/enable"), 0).is_err();
        }
    }
    if let Err(e) = zero(sys) {
        errors.push(format!("output discharge: {e:#}"));
    }
    ensure!(errors.is_empty(), "shutdown errors: {}", errors.join("; "));
    Ok(())
}

struct Cutoff<'a> {
    sys: &'a System,
    done: bool,
}
impl Drop for Cutoff<'_> {
    fn drop(&mut self) {
        if !self.done {
            let _ = shutdown(self.sys);
        }
    }
}

/// Hidden child mode: commands arrive through one pipe; EOF means parent loss.
/// The child owns the only enable=1 write and permits at most one arm request.
pub fn guard(sys: &System, args: &ArgMatches) -> Result<()> {
    let limits = Limits::from_args(args);
    common(sys, true, limits)?;
    zero(sys)?;
    resets_asserted(sys)?;
    ensure!(
        sys.gpio_read(263)? == "0",
        "guard must start with switched LDO off"
    );
    let mut cutoff = Cutoff { sys, done: false };
    configure_guard_process()?;
    let max_on = Duration::from_millis(u64::from(*args.get_one::<u32>("max-on-ms").unwrap()));
    let dry = args.get_flag("dry-run");
    let selected = args.get_one::<u8>("id-slot").copied();
    let result = (|| -> Result<()> {
        writeln!(std::io::stdout(), "READY")?;
        std::io::stdout().flush()?;
        let setup_deadline = Instant::now() + Duration::from_secs(5);
        let mut armed = None;
        let mut voltage_seen = false;
        let mut stable_samples = 0u32;
        let mut previous_voltage: Option<u32> = None;
        let mut released = None;
        let mut input = String::new();
        loop {
            if CANCEL.load(Ordering::Relaxed) {
                writeln!(std::io::stdout(), "STOP signal")?;
                break;
            }
            if armed.is_some_and(|start| Instant::now().duration_since(start) >= max_on) {
                writeln!(std::io::stdout(), "STOP deadline")?;
                break;
            }
            let sample = common(sys, true, limits)?;
            reset_state(sys, released)?;
            if let Some(start) = armed {
                ensure!(sys.gpio_read(263)? == "1", "switched LDO lost");
                ensure!(
                    number(sys, "enable")? == u32::from(!dry),
                    "PSU enable changed unexpectedly"
                );
                if Instant::now().duration_since(start) >= max_on {
                    writeln!(std::io::stdout(), "STOP deadline")?;
                    break;
                }
                if (1995..=2200).contains(&sample.voltage) {
                    voltage_seen = true;
                    stable_samples =
                        if previous_voltage.is_some_and(|v| v.abs_diff(sample.voltage) <= 2) {
                            stable_samples + 1
                        } else {
                            1
                        };
                } else {
                    stable_samples = 0;
                }
                previous_voltage = Some(sample.voltage);
                writeln!(
                    std::io::stdout(),
                    "SAMPLE elapsed_ms={} enable={} vout_cv={} iout_ma={} pout_w={} stable={stable_samples}",
                    Instant::now().duration_since(start).as_millis(),
                    number(sys, "enable")?,
                    sample.voltage,
                    sample.current,
                    sample.power
                )?;
            } else {
                ensure!(
                    Instant::now() < setup_deadline,
                    "guard preparation deadline expired"
                );
                zero(sys)?;
            }
            let mut pfd = libc::pollfd {
                fd: std::io::stdin().as_raw_fd(),
                events: libc::POLLIN | libc::POLLHUP,
                revents: 0,
            };
            // SAFETY: one valid poll descriptor, no device fd is opened here.
            let wait = armed.map_or(50, |start| {
                (start + max_on)
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(50) as i32
            });
            let polled = unsafe { libc::poll(&mut pfd, 1, wait) };
            if polled < 0
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
            {
                continue;
            }
            ensure!(polled >= 0, "guard input poll failed");
            if polled == 0 {
                continue;
            }
            if pfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
                writeln!(std::io::stdout(), "STOP parent pipe closed")?;
                break;
            }
            let mut bytes = [0; 64];
            let n = std::io::stdin().read(&mut bytes)?;
            if n == 0 {
                writeln!(std::io::stdout(), "STOP parent pipe closed")?;
                break;
            }
            input.push_str(std::str::from_utf8(&bytes[..n])?);
            ensure!(input.len() <= 64, "invalid guard protocol");
            while let Some(end) = input.find('\n') {
                let command = input[..end].to_owned();
                input.drain(..=end);
                match command.as_str() {
                    "ENABLE" => {
                        ensure!(armed.is_none(), "guard refuses a second enable");
                        zero(sys)?;
                        common(sys, true, limits)?;
                        resets_asserted(sys)?;
                        ensure!(
                            sys.gpio_read(263)? == "1",
                            "switched LDO must be established before enable"
                        );
                        ensure!(
                            !CANCEL.load(Ordering::Relaxed) && Instant::now() < setup_deadline,
                            "enable cancelled or preparation deadline expired"
                        );
                        pfd.revents = 0;
                        // Check closure again after the telemetry/precondition
                        // reads so queued ENABLE cannot outlive its parent.
                        let ready = unsafe { libc::poll(&mut pfd, 1, 0) };
                        ensure!(
                            ready >= 0
                                && pfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL)
                                    == 0,
                            "parent disconnected before enable"
                        );
                        // Establish the cutoff deadline BEFORE touching output.
                        armed = Some(Instant::now());
                        if !dry {
                            sys.write_decimal(&format!("{POWER}/enable"), 1)?;
                            ensure!(
                                number(sys, "enable")? == 1,
                                "PSU did not acknowledge enable"
                            );
                        }
                        writeln!(std::io::stdout(), "ENABLED dry_run={dry}")?;
                        std::io::stdout().flush()?;
                    }
                    "STOP" => {
                        writeln!(std::io::stdout(), "STOP requested")?;
                        return Ok(());
                    }
                    "RELEASE" => {
                        let slot =
                            selected.context("reset release was not authorized for this trial")?;
                        ensure!(!dry && released.is_none(), "invalid reset release phase");
                        let start = armed.context("PSU must be enabled before reset release")?;
                        ensure!(
                            start.elapsed() >= Duration::from_millis(1000)
                                && start.elapsed() < max_on,
                            "reset release outside allowed time window"
                        );
                        ensure!(
                            max_on.saturating_sub(start.elapsed()) >= Duration::from_millis(250),
                            "insufficient guard time remaining for reset release"
                        );
                        ensure!(
                            stable_samples >= 3,
                            "need three consecutive stable voltage samples"
                        );
                        let sample = common(sys, true, limits)?;
                        ensure!(
                            (1995..=2200).contains(&sample.voltage)
                                && previous_voltage
                                    .is_some_and(|v| v.abs_diff(sample.voltage) <= 2),
                            "voltage changed before reset release"
                        );
                        resets_asserted(sys)?;
                        ensure!(!CANCEL.load(Ordering::Relaxed), "reset release cancelled");
                        pfd.revents = 0;
                        let ready = unsafe { libc::poll(&mut pfd, 1, 0) };
                        ensure!(
                            ready >= 0
                                && pfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL)
                                    == 0,
                            "parent disconnected before reset release"
                        );
                        ensure!(
                            !CANCEL.load(Ordering::Relaxed)
                                && max_on.saturating_sub(start.elapsed())
                                    >= Duration::from_millis(250),
                            "reset-release deadline expired during precondition checks"
                        );
                        released = Some(slot / 2);
                        sys.gpio_set(201 + u32::from(slot / 2), true)?;
                        thread::sleep(Duration::from_millis(100));
                        common(sys, true, limits)?;
                        reset_state(sys, released)?;
                        ensure!(
                            !CANCEL.load(Ordering::Relaxed) && start.elapsed() < max_on,
                            "reset release interrupted or timed out"
                        );
                        writeln!(
                            std::io::stdout(),
                            "RELEASED board={} elapsed_ms={}",
                            slot / 2,
                            start.elapsed().as_millis()
                        )?;
                        std::io::stdout().flush()?;
                    }
                    _ => bail!("invalid guard command"),
                }
            }
        }
        ensure!(
            dry || voltage_seen,
            "characterization did not observe output within 5% of 21 V before cutoff"
        );
        Ok(())
    })();
    let off = shutdown(sys);
    cutoff.done = off.is_ok();
    if let Err(e) = &result {
        eprintln!("GUARD TRIP: {e:#}");
    }
    off?;
    writeln!(std::io::stdout(), "OFF enable/Vout/Iout=0/0/0")?;
    result
}

struct GuardProcess {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl GuardProcess {
    fn start(sys: &System, args: &ArgMatches, lease: std::os::fd::RawFd) -> Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("--root")
            .arg(&sys.root)
            .arg("--expect-boot-id")
            .arg(sys.expected_boot_id.as_ref().unwrap())
            .arg("_guard")
            .arg("--max-on-ms")
            .arg(args.get_one::<u32>("max-on-ms").unwrap().to_string())
            .arg("--max-current-ma")
            .arg(args.get_one::<u32>("max-current-ma").unwrap().to_string())
            .arg("--max-power-w")
            .arg(args.get_one::<u32>("max-power-w").unwrap().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if args.get_flag("dry-run") {
            command.arg("--dry-run");
        }
        if let Some(slot) = args.get_one::<u8>("id-slot") {
            command.arg("--id-slot").arg(slot.to_string());
        }
        // SAFETY: only async-signal-safe syscalls after fork. FD 3 retains the
        // parent's shared flock through guard shutdown if the parent dies.
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(lease, 3) < 0 || libc::fcntl(3, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().context("start independent power guard")?;
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut guard = Self {
            child,
            input,
            output,
        };
        let mut line = String::new();
        guard.output.read_line(&mut line)?;
        ensure!(
            line.trim() == "READY",
            "guard did not acknowledge readiness: {line}"
        );
        Ok(guard)
    }

    fn enable(&mut self) -> Result<()> {
        ensure!(
            self.child.try_wait()?.is_none(),
            "guard exited before enable"
        );
        self.input.as_mut().unwrap().write_all(b"ENABLE\n")?;
        self.input.as_mut().unwrap().flush()?;
        Ok(())
    }

    fn release_when_stable(&mut self) -> Result<()> {
        loop {
            let mut line = String::new();
            ensure!(
                self.output.read_line(&mut line)? > 0,
                "guard ended before stable-voltage gate"
            );
            print!("{line}");
            if line.starts_with("SAMPLE ") {
                let value = |key: &str| -> Option<u32> {
                    line.split_whitespace()
                        .find_map(|p| p.strip_prefix(key))
                        .and_then(|p| p.parse().ok())
                };
                if value("elapsed_ms=").is_some_and(|v| v >= 1000)
                    && value("stable=").is_some_and(|v| v >= 3)
                {
                    break;
                }
            }
        }
        self.input.as_mut().unwrap().write_all(b"RELEASE\n")?;
        loop {
            let mut line = String::new();
            ensure!(
                self.output.read_line(&mut line)? > 0,
                "guard ended before reset release"
            );
            print!("{line}");
            if line.starts_with("RELEASED ") {
                return Ok(());
            }
        }
    }

    fn finish(&mut self) -> Result<()> {
        self.input.take();
        let mut log = String::new();
        self.output.read_to_string(&mut log)?;
        print!("{log}");
        ensure!(
            self.child.wait()?.success(),
            "independent guard tripped; see captured telemetry"
        );
        Ok(())
    }
}

impl Drop for GuardProcess {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.wait();
    }
}

pub fn run(sys: &System, args: &ArgMatches) -> Result<()> {
    let _session = sys.session()?;
    let limits = Limits::from_args(args);
    common(sys, false, limits)?;
    let original = number(sys, "vout_set")?;
    ensure!(
        (1500..=2500).contains(&original),
        "starting setpoint outside supported range"
    );
    zero(sys)?;
    resets_asserted(sys)?;
    ensure!(sys.gpio_read(263)? == "0", "start with switched LDO off");
    let slot = args.get_one::<u8>("id-slot").copied();
    ensure!(
        slot.is_none() || *args.get_one::<u32>("max-on-ms").unwrap() >= 2500,
        "ID trials require an explicit --max-on-ms of at least 2500"
    );
    let prefix = if args.get_flag("capture-prefix") {
        Some(cold_prefix::frames(usize::from(slot.unwrap()))?)
    } else {
        None
    };
    let mut port = if let Some(slot) = slot {
        ensure!(
            sys.read("/sys/kernel/debug/clk/apb2/clk_rate")? == "192000000",
            "cold read requires verified 192 MHz APB2 clock"
        );
        Some(Uart::open(&sys.path(UARTS[usize::from(slot)]), 375_000)?)
    } else {
        None
    };
    let result = (|| -> Result<()> {
        // Keep the setpoint attempt inside the cleanup scope, including a
        // rejected/short write before the child has been started.
        sys.write_decimal(&format!("{POWER}/vout_set"), TARGET_CV)?;
        thread::sleep(Duration::from_millis(10));
        common(sys, true, limits)?;
        let mut guard = GuardProcess::start(sys, args, _session.as_raw_fd())?;
        println!("GUARD pid={}", guard.child.id());
        sys.gpio_set(263, true)?;
        let ldo_deadline = Instant::now() + Duration::from_millis(20);
        while sys.gpio_read(263)? != "1" && Instant::now() < ldo_deadline {
            thread::sleep(Duration::from_millis(1));
        }
        ensure!(
            sys.gpio_read(263)? == "1",
            "switched LDO command readback failed"
        );
        guard.enable()?;
        let action = (|| -> Result<()> {
            if let Some(slot) = slot {
                guard.release_when_stable()?;
                let port = port.as_mut().unwrap();
                let check_power = || -> Result<()> {
                    ensure!(
                        sys.gpio_read(263)? == "1"
                            && sys.gpio_read(201 + u32::from(slot / 2))? == "1",
                        "guard withdrew logic/reset; abort UART experiment"
                    );
                    Ok(())
                };
                if let Some(prefix) = &prefix {
                    println!(
                        "EXPERIMENT slot={slot} pre_pll_frames={} no_known_PLL_core_or_work_register_writes",
                        prefix.len()
                    );
                    let start = Instant::now();
                    for (index, frame) in prefix.iter().enumerate() {
                        if index % 16 == 0 {
                            check_power()?;
                        }
                        let deadline = start + frame.at;
                        if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                            thread::sleep(wait);
                        }
                        port.transmit(&frame.bytes)?;
                    }
                    check_power()?;
                    let discovery = read_ids(port, slot, 0x110, 1, &[0]);
                    check_power()?;
                    let identity = read_ids(port, slot, 0x100, 2, &[0x19, 0x80]);
                    discovery?;
                    identity?;
                    return Ok(());
                }
                check_power()?;
                read_ids(port, slot, 0x100, 2, &[0x19, 0x80])?;
            } else {
                // The child owns the deadline and shutdown. Process death
                // closes its input pipe in the kernel.
                while guard.child.try_wait()?.is_none() {
                    thread::sleep(Duration::from_millis(20));
                }
            }
            Ok(())
        })();
        let finish = guard.finish();
        action?;
        finish
    })();
    let off = shutdown(sys);
    if off.is_ok() {
        sys.write_decimal(&format!("{POWER}/vout_set"), original)?;
        ensure!(
            number(sys, "vout_set")? == original,
            "original setpoint did not restore"
        );
    }
    off?;
    result
}

fn read_ids(port: &mut Uart, slot: u8, register: u16, length: u8, expected: &[u8]) -> Result<()> {
    let request = protocol::read_command(0xfff, register, length, 0)?;
    println!(
        "slot={slot} cold_read register=0x{register:03x} host_baud=375000 shift=0 TX={}",
        hex::encode(&request)
    );
    let frames = port.read_register(&request, 0, Duration::from_millis(100))?;
    let mut ids = BTreeSet::new();
    for frame in frames {
        let (id, kind, payload) = protocol::response(&frame)?;
        println!(
            "slot={slot} RX={} id={id} type={kind} payload={}",
            hex::encode(&frame),
            hex::encode(payload)
        );
        ensure!(
            kind == 0 && payload == expected,
            "unexpected cold read reply"
        );
        ids.insert(id);
    }
    println!(
        "slot={slot} register=0x{register:03x} unique_ids={}",
        ids.len()
    );
    ensure!(
        ids == (0..96).collect(),
        "cold read requires complete IDs0..95; got {}",
        ids.len()
    );
    Ok(())
}
