//! Fixed captured pre-PLL configuration, for an explicitly experimental early read.
//! This is not a captured minimal startup: its registers are not fully mapped.

use crate::protocol;
use anyhow::{Context, Result, ensure};
use std::time::Duration;

pub(crate) struct Frame {
    pub at: Duration,
    pub bytes: Vec<u8>,
}

fn validate(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() >= 7 && bytes.len() <= 10,
        "invalid cold-prefix frame length"
    );
    ensure!(
        protocol::crc8(&bytes[..bytes.len() - 1]) == bytes[bytes.len() - 1],
        "cold-prefix CRC mismatch"
    );
    ensure!(
        bytes.len() == usize::from(bytes[4]) + 6,
        "cold-prefix payload length mismatch"
    );
    if bytes[0] >= 0x10 {
        ensure!(
            bytes == hex::decode("fff0000401aa0c")?,
            "unrecognized packed cold-prefix command"
        );
        return Ok(());
    }
    let destination = (u16::from(bytes[0]) << 8) | u16::from(bytes[1]);
    let register = (u16::from(bytes[2]) << 4) | u16::from(bytes[3] >> 4);
    ensure!(bytes[3] & 15 == 4, "cold prefix must contain writes only");
    let data = &bytes[5..bytes.len() - 1];
    let valid = match register {
        0x123 => destination == 0xfff && data == [0x12, 0xf4],
        0x000 => destination == 0xfff && data == [0xaa],
        0x070 => {
            (destination == 0xfff || destination < 96)
                && matches!(data, [0x3f, 0x14, 0x3f, 0x14] | [0x3d, 0x15, 0x3d, 0x15])
        }
        0x018 => {
            (destination == 0xfff || destination < 96)
                && data.len() == 1
                && (0x32..=0x34).contains(&data[0])
        }
        0x019 => {
            (destination == 0xfff || destination < 96)
                && data.len() == 1
                && (0x24..=0x25).contains(&data[0])
        }
        0x001 => {
            destination == 0xfff && data.len() == 2 && u16::from_be_bytes([data[0], data[1]]) < 384
        }
        0x020 => destination < 384 && matches!(data, [0] | [1]),
        _ => false,
    };
    ensure!(
        valid,
        "cold-prefix write outside audited allowlist: register0x{register:03x}, chip0x{destination:03x}"
    );
    Ok(())
}

pub(crate) fn frames(slot: usize) -> Result<Vec<Frame>> {
    let text = match slot {
        0 => include_str!("../profiles/m64-vl20-slot0-pre-pll.hex"),
        1 => include_str!("../profiles/m64-vl20-slot1-pre-pll.hex"),
        2 => include_str!("../profiles/m64-vl20-slot2-pre-pll.hex"),
        3 => include_str!("../profiles/m64-vl20-slot3-pre-pll.hex"),
        _ => anyhow::bail!("invalid cold-prefix slot"),
    };
    let mut frames = Vec::new();
    let mut previous = 0;
    for line in text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let (at, hex) = line.split_once(' ').context("invalid cold-prefix record")?;
        let micros: u64 = at.parse()?;
        ensure!(
            micros >= previous && micros <= 500_000,
            "invalid cold-prefix timing"
        );
        previous = micros;
        let bytes = hex::decode(hex)?;
        validate(&bytes)?;
        frames.push(Frame {
            at: Duration::from_micros(micros),
            bytes,
        });
    }
    ensure!(frames.len() == 1832, "incomplete cold prefix");
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_fixed_prefixes_are_crc_valid_bounded_and_exclude_core_pll_work() {
        for slot in 0..4 {
            let frames = frames(slot).unwrap();
            assert_eq!(frames.len(), 1832);
            assert!(frames.last().unwrap().at < Duration::from_millis(500));
            assert_eq!(hex::encode(&frames[0].bytes), "0fff12340212f4f8");
        }
        for register in [0x030u16, 0x040, 0x200, 0x201, 0x820] {
            let mut frame = vec![
                0x0f,
                0xff,
                (register >> 4) as u8,
                ((register << 4) as u8) | 4,
                1,
                0,
            ];
            frame.push(protocol::crc8(&frame));
            assert!(validate(&frame).is_err());
        }
    }
}
