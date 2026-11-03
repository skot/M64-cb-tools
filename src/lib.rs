//! Standalone M64 diagnostics, independent of the mining daemon.

pub mod cli;
mod diagnostics;
mod hashboard;
mod i2c;
pub mod protocol;
mod psu;
mod system;
mod uart;
