//! Standalone M64 diagnostics, independent of the mining daemon.

pub mod cli;
mod cold_prefix;
mod diagnostics;
mod hashboard;
mod i2c;
mod power_trial;
pub mod protocol;
mod psu;
mod system;
mod uart;
