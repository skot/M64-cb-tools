//! Stock M64 UART ioctl transport with finite receive deadlines.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};

use crate::protocol;

// asm-generic termios2 ABI used by the AArch64 stock kernel.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Termios2 {
    input: u32,
    output: u32,
    control: u32,
    local: u32,
    line: u8,
    characters: [u8; 19],
    input_speed: u32,
    output_speed: u32,
}

pub struct Uart {
    file: File,
    original: Termios2,
}

impl Uart {
    pub fn open(path: &Path, baud: u32) -> Result<Self> {
        ensure!(
            cfg!(all(target_os = "linux", target_arch = "aarch64")),
            "UART experiments require the M64's AArch64 Linux termios2 ABI"
        );
        ensure!(
            (9_600..=12_000_000).contains(&baud),
            "host baud must be 9600..12000000"
        );
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
            .open(path)
            .with_context(|| format!("open {}", path.display()))?;
        let mut original = Termios2::default();
        // SAFETY: TCGETS2 writes the initialized, correctly sized ABI struct.
        check(unsafe { libc::ioctl(file.as_raw_fd(), 0x802c542a_u32 as _, &mut original) })?;
        let port = Self { file, original };
        let settings = Termios2 {
            // BOTHER | CS8 | CREAD | CLOCAL; raw 8N1, no flow control.
            control: 0x1000 | 0x30 | 0x80 | 0x800,
            input_speed: baud,
            output_speed: baud,
            ..Termios2::default()
        };
        // SAFETY: TCSETS2 reads a live termios2 struct for this valid fd.
        check(unsafe { libc::ioctl(port.file.as_raw_fd(), 0x402c542b as _, &settings) })
            .context("set host baud with termios2/BOTHER")?;
        let mut actual = Termios2::default();
        // SAFETY: same TCGETS2 contract as above.
        check(unsafe { libc::ioctl(port.file.as_raw_fd(), 0x802c542a_u32 as _, &mut actual) })?;
        ensure!(
            actual.input_speed == baud && actual.output_speed == baud,
            "UART driver did not accept requested host baud {baud}"
        );
        Ok(port)
    }

    pub fn read_register(
        &mut self,
        command: &[u8],
        shift: u32,
        timeout: Duration,
    ) -> Result<Vec<Vec<u8>>> {
        // The exclusive experiment owns RX: discard stale stock replies before
        // attributing any new frame to this request.
        check(unsafe { libc::tcflush(self.file.as_raw_fd(), libc::TCIFLUSH) })
            .context("flush stale UART replies")?;
        // Stock uart_write sends logical bytes through the custom kernel ioctl;
        // the caller has already applied the shift to physical TX bytes.
        self.transmit(command)?;
        let deadline = Instant::now() + timeout;
        let mut replies = protocol::Replies::default();
        while Instant::now() < deadline {
            let mut poll = libc::pollfd {
                fd: self.file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let wait = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(1, 100) as i32;
            // SAFETY: one valid pollfd is supplied for the duration of poll.
            let ready = unsafe { libc::poll(&mut poll, 1, wait) };
            if ready < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(io::Error::last_os_error().into());
            }
            ensure!(
                poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) == 0,
                "UART poll error: {}",
                poll.revents
            );
            if ready == 0 {
                continue;
            }
            let mut chunk = [0; 4096];
            match self.file.read(&mut chunk) {
                Ok(0) => continue,
                Ok(n) => replies.push(&chunk[..n], shift)?,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
        replies.finish()
    }

    pub(crate) fn transmit(&mut self, command: &[u8]) -> Result<()> {
        ensure!(
            !command.is_empty() && command.len() <= 256,
            "invalid UART transmit length"
        );
        let request = 0x5441_u64 | ((command.len() as u64) << 16);
        // SAFETY: the command buffer remains valid for its encoded length.
        check(unsafe { libc::ioctl(self.file.as_raw_fd(), request as _, command.as_ptr()) })
            .context("stock UART transmit ioctl")
    }
}

impl Drop for Uart {
    fn drop(&mut self) {
        // SAFETY: restores the saved ABI struct while the fd is still valid.
        if unsafe { libc::ioctl(self.file.as_raw_fd(), 0x402c542b as _, &self.original) } < 0 {
            eprintln!(
                "WARNING: could not restore UART settings: {}",
                io::Error::last_os_error()
            );
        }
    }
}

fn check(result: libc::c_int) -> Result<()> {
    if result < 0 {
        Err(io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}
