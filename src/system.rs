//! Filesystem access and serialization of hardware experiments.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};

pub const UARTS: [&str; 4] = ["/dev/ttyS1", "/dev/ttyS4", "/dev/ttyS2", "/dev/ttyS3"];

pub struct System {
    pub root: PathBuf,
}

pub struct Session(File);

impl System {
    pub fn path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.root
            .join(path.as_ref().strip_prefix("/").unwrap_or(path.as_ref()))
    }

    pub fn read(&self, path: &str) -> Result<String> {
        fs::read_to_string(self.path(path))
            .with_context(|| format!("read {path}"))
            .map(|s| s.trim().to_owned())
    }

    pub fn write(&self, path: &str, value: impl std::fmt::Display) -> Result<()> {
        // Never create a missing sysfs node or silently invent an interface.
        let mut file = OpenOptions::new()
            .write(true)
            .open(self.path(path))
            .with_context(|| format!("open {path} for writing"))?;
        writeln!(file, "{value}").with_context(|| format!("write {path}"))
    }

    pub fn show(&self, path: &str) -> bool {
        match self.read(path) {
            Ok(value) => {
                println!("{path}: {value}");
                true
            }
            Err(error) => {
                println!("{path}: UNAVAILABLE ({error:#})");
                false
            }
        }
    }

    pub fn children(&self, path: &str) -> Result<Vec<String>> {
        let mut names = fs::read_dir(self.path(path))
            .with_context(|| format!("list {path}"))?
            .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect::<io::Result<Vec<_>>>()?;
        names.sort();
        Ok(names)
    }

    pub fn miners(&self) -> Result<Vec<String>> {
        let mut found = Vec::new();
        for pid in self.children("/proc")? {
            if !pid.bytes().all(|c| c.is_ascii_digit()) {
                continue;
            }
            if let Ok(name) = self.read(&format!("/proc/{pid}/comm"))
                && matches!(name.as_str(), "btminer" | "btminer_test" | "power-aging")
            {
                found.push(format!("{name} (PID {pid})"));
            }
        }
        Ok(found)
    }

    pub fn session(&self) -> Result<Session> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.path("/tmp/m64-cb-tools.lock"))
            .context("open diagnostic session lock")?;
        // SAFETY: flock borrows a valid open file descriptor and no pointers.
        ensure!(
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
            "another M64 tool holds the diagnostic session lock"
        );
        let miners = self.miners()?;
        ensure!(
            miners.is_empty(),
            "hardware is owned by {}; stop mining through the stock service before testing",
            miners.join(", ")
        );
        Ok(Session(file))
    }

    pub fn gpio_read(&self, gpio: u32) -> Result<String> {
        self.read(&format!("/sys/class/gpio/gpio{gpio}/value"))
    }

    pub fn gpio_export(&self, gpio: u32, input: bool) -> Result<()> {
        let base = format!("/sys/class/gpio/gpio{gpio}");
        if !self.path(&base).exists() {
            self.write("/sys/class/gpio/export", gpio)?;
            for _ in 0..20 {
                if self.path(&base).exists() {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
        if input {
            self.write(&format!("{base}/direction"), "in")?;
        }
        Ok(())
    }

    pub fn gpio_set(&self, gpio: u32, high: bool) -> Result<()> {
        self.gpio_export(gpio, false)?;
        // sysfs direction=high/low sets direction and initial value atomically.
        self.write(
            &format!("/sys/class/gpio/gpio{gpio}/direction"),
            if high { "high" } else { "low" },
        )
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: the descriptor stays valid for this object's lifetime.
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

pub fn number(value: &str) -> Result<u32> {
    if let Some(hex) = value.strip_prefix("0x") {
        Ok(u32::from_str_radix(hex, 16)?)
    } else {
        Ok(value.parse()?)
    }
}

pub fn selection(value: &str, count: usize) -> Result<Vec<usize>> {
    if value == "all" {
        return Ok((0..count).collect());
    }
    let index: usize = value.parse()?;
    if index >= count {
        bail!("index must be 0..{} or all", count - 1);
    }
    Ok(vec![index])
}
