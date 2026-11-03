//! Linux I2C transactions. Kernel-owned addresses are never forced.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

use anyhow::{Context, Result, ensure};

pub struct I2c(File);

#[repr(C)]
union Data {
    byte: u8,
    word: u16,
    block: [u8; 34],
}

#[repr(C)]
struct Smbus {
    direction: u8,
    command: u8,
    size: u32,
    data: *mut Data,
}

#[repr(C)]
struct Message {
    address: u16,
    flags: u16,
    length: u16,
    buffer: *mut u8,
}

#[repr(C)]
struct Transfer {
    messages: *mut Message,
    count: u32,
}

impl I2c {
    pub fn open(path: &Path, address: u16, pec: bool) -> Result<Self> {
        ensure!(
            (0x08..=0x77).contains(&address),
            "address must be an unreserved 7-bit I2C address"
        );
        ensure!(cfg!(target_os = "linux"), "hardware I2C requires Linux");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .with_context(|| format!("open {}", path.display()))?;
        // SAFETY: these Linux ioctls take integer arguments and a valid fd.
        check(unsafe { libc::ioctl(file.as_raw_fd(), 0x0703 as _, libc::c_ulong::from(address)) })
            .context("select I2C target (a bound kernel driver may own it; no force mode)")?;
        check(unsafe { libc::ioctl(file.as_raw_fd(), 0x0708 as _, libc::c_ulong::from(pec)) })
            .context("configure SMBus PEC")?;
        Ok(Self(file))
    }

    fn smbus(&self, command: u8, size: u32, direction: u8, data: &mut Data) -> Result<()> {
        let mut request = Smbus {
            direction,
            command,
            size,
            data,
        };
        // SAFETY: request and its union storage remain alive through ioctl.
        check(unsafe { libc::ioctl(self.0.as_raw_fd(), 0x0720 as _, &mut request) })
            .with_context(|| format!("SMBus command 0x{command:02x}"))?;
        Ok(())
    }

    pub fn byte(&self, command: u8) -> Result<u8> {
        let mut data = Data { byte: 0 };
        self.smbus(command, 2, 1, &mut data)?;
        // SAFETY: the successful BYTE_DATA ioctl initialized this member.
        Ok(unsafe { data.byte })
    }

    pub fn word(&self, command: u8) -> Result<u16> {
        let mut data = Data { word: 0 };
        self.smbus(command, 3, 1, &mut data)?;
        // SAFETY: the successful WORD_DATA ioctl initialized this member.
        Ok(unsafe { data.word })
    }

    pub fn block(&self, command: u8) -> Result<Vec<u8>> {
        let mut data = Data { block: [0; 34] };
        self.smbus(command, 5, 1, &mut data)?;
        // SAFETY: all 34 bytes were initialized before the BLOCK_DATA ioctl.
        let block = unsafe { data.block };
        ensure!((1..=32).contains(&block[0]), "invalid SMBus block length");
        Ok(block[1..=usize::from(block[0])].to_vec())
    }

    pub fn write_byte(&self, command: u8, value: u8) -> Result<()> {
        self.smbus(command, 2, 0, &mut Data { byte: value })
    }

    pub fn write_word(&self, command: u8, value: u16) -> Result<()> {
        self.smbus(command, 3, 0, &mut Data { word: value })
    }

    pub fn read_at(&self, address: u16, register: u8, length: u16) -> Result<Vec<u8>> {
        let mut register = register;
        let mut bytes = vec![0; usize::from(length)];
        let mut messages = [
            Message {
                address,
                flags: 0,
                length: 1,
                buffer: &mut register,
            },
            Message {
                address,
                flags: 1,
                length,
                buffer: bytes.as_mut_ptr(),
            },
        ];
        let mut transfer = Transfer {
            messages: messages.as_mut_ptr(),
            count: 2,
        };
        // SAFETY: both buffers and messages are valid until ioctl returns.
        let count = check(unsafe { libc::ioctl(self.0.as_raw_fd(), 0x0707 as _, &mut transfer) })?;
        ensure!(
            count == 2,
            "short I2C combined transfer: {count}/2 messages"
        );
        Ok(bytes)
    }
}

fn check(result: libc::c_int) -> Result<libc::c_int> {
    if result < 0 {
        Err(io::Error::last_os_error().into())
    } else {
        Ok(result)
    }
}
