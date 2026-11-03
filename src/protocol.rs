//! Numeric formats and capture-verified BT1980 framing.

use anyhow::{Result, ensure};

#[derive(Default)]
pub(crate) struct Replies {
    pending: Vec<u8>,
    frames: Vec<Vec<u8>>,
    crc_errors: usize,
}

impl Replies {
    pub fn push(&mut self, physical: &[u8], shift: u32) -> Result<()> {
        self.pending
            .extend(physical.iter().map(|b| b.rotate_left(shift)));
        while self.pending.len() >= 5 {
            let length =
                ((usize::from(self.pending[2] & 3) << 8) | usize::from(self.pending[3])) + 5;
            if self.pending.len() < length {
                break;
            }
            if response(&self.pending[..length]).is_ok() {
                self.frames.push(self.pending.drain(..length).collect());
                ensure!(
                    self.frames.len() <= 4096,
                    "excess UART traffic; unexpected transmitter or streaming mode"
                );
            } else {
                self.pending.remove(0);
                self.crc_errors += 1;
            }
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<Vec<u8>>> {
        ensure!(
            self.crc_errors == 0 && self.pending.is_empty(),
            "UART framing failed: {} CRC errors, {} trailing bytes; check host baud and shift",
            self.crc_errors,
            self.pending.len()
        );
        ensure!(
            !self.frames.is_empty(),
            "no CRC-valid UART replies before deadline (board must already be initialized)"
        );
        Ok(self.frames)
    }
}

/// Calculates BT1980 and M64 EEPROM CRC-8 (0x31, initial 0xff).
pub fn crc8(bytes: &[u8]) -> u8 {
    crc(bytes, 0xff, 0x31)
}

fn crc(bytes: &[u8], initial: u8, polynomial: u8) -> u8 {
    bytes.iter().fold(initial, |mut value, byte| {
        value ^= byte;
        for _ in 0..8 {
            value = if value & 0x80 != 0 {
                (value << 1) ^ polynomial
            } else {
                value << 1
            };
        }
        value
    })
}

/// Calculates SMBus PEC, including address and command bytes.
pub fn pec(bytes: &[u8]) -> u8 {
    crc(bytes, 0, 7)
}

/// Decodes the signed exponent and mantissa of a PMBus LINEAR11 word.
pub fn linear11(raw: u16) -> f64 {
    let exponent = (raw as i16) >> 11;
    let mantissa = ((raw << 5) as i16) >> 5;
    f64::from(mantissa) * 2_f64.powi(i32::from(exponent))
}

/// Decodes the LM75's signed, big-endian temperature register.
pub fn lm75(bytes: [u8; 2]) -> f64 {
    f64::from(i16::from_be_bytes(bytes)) / 256.0
}

/// Builds an aligned register-read command, with physical-byte rotation.
pub fn read_command(chip: u16, register: u16, length: u8, shift: u32) -> Result<Vec<u8>> {
    ensure!(
        chip <= 0xfff && register <= 0xfff,
        "chip and register must fit 12 bits"
    );
    ensure!(
        length > 0 && shift <= 7,
        "length must be 1..255 and shift 0..7"
    );
    let mut frame = vec![
        (chip >> 8) as u8,
        chip as u8,
        (register >> 4) as u8,
        (register << 4) as u8,
        length,
    ];
    frame.push(crc8(&frame));
    Ok(frame
        .into_iter()
        .map(|byte| byte.rotate_right(shift))
        .collect())
}

/// Decodes one logical response, rejecting corrupt and truncated frames.
pub fn response(frame: &[u8]) -> Result<(u16, u8, &[u8])> {
    ensure!(frame.len() >= 5, "truncated response header");
    let length = ((usize::from(frame[2] & 3) << 8) | usize::from(frame[3])) + 5;
    ensure!(frame.len() == length, "response length mismatch");
    ensure!(
        crc8(&frame[..length - 1]) == frame[length - 1],
        "BT1980 CRC mismatch"
    );
    Ok((
        (u16::from(frame[0] & 15) << 8) | u16::from(frame[1]),
        frame[2] >> 2,
        &frame[4..length - 1],
    ))
}

/// Prints EEPROM identity and surfaces the observed nonfatal CRC mismatch.
pub fn print_eeprom(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() == 128,
        "expected the stock 128-byte EEPROM image"
    );
    println!("raw={}", hex::encode(bytes));
    ensure!(bytes[..2] == [0xfa, 0x5a], "unrecognized M64 EEPROM magic");
    for (name, start, end) in [
        ("serial", 5, 25),
        ("hardware", 25, 40),
        ("bin", 45, 55),
        ("production", 55, 75),
    ] {
        println!(
            "{name}={}",
            String::from_utf8_lossy(&bytes[start..end]).trim_matches(['\0', ' '])
        );
    }
    let computed = crc8(&bytes[5..=0x77]);
    println!(
        "crc_stored=0x{:02x} crc_computed=0x{computed:02x} valid={}",
        bytes[0x78],
        computed == bytes[0x78]
    );
    if computed != bytes[0x78] {
        eprintln!("WARNING: EEPROM CRC mismatch (also observed on a working stock board)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_replies_retain_partial_frames_and_reject_corruption() {
        let logical = hex::decode("00000002198031").unwrap();
        let physical: Vec<_> = logical.iter().map(|b| b.rotate_right(3)).collect();
        for split in 0..=physical.len() {
            let mut replies = Replies::default();
            replies.push(&physical[..split], 3).unwrap();
            replies.push(&physical[split..], 3).unwrap();
            replies.push(&physical, 3).unwrap();
            assert_eq!(
                replies.finish().unwrap(),
                vec![logical.clone(), logical.clone()]
            );
        }
        let mut truncated = Replies::default();
        truncated.push(&physical[..physical.len() - 1], 3).unwrap();
        assert!(truncated.finish().is_err());
        let mut corrupt = physical.clone();
        corrupt[4] ^= 1;
        let mut replies = Replies::default();
        replies.push(&corrupt, 3).unwrap();
        assert!(replies.finish().is_err());
        assert!(Replies::default().finish().is_err());
    }

    #[test]
    fn captured_pmbus_values_and_pec() {
        assert_eq!(pec(&[0xb0, 0x88, 0xb1, 0xbd, 0xf3]), 0xb0);
        assert_eq!(linear11(0xf3bd), 239.25);
        assert_eq!(linear11(0xf293), 164.75);
        assert_eq!(linear11(0x13ec), 4016.0);
        assert_eq!(linear11(0x1aee), 6000.0);
        assert_eq!(linear11(0x07ff), -1.0);
        assert_eq!(lm75([0xff, 0x80]), -0.5);
    }

    #[test]
    fn captured_chip_id_and_corruption() {
        // M64 initialization capture: aligned read 0x100, record 12184.
        let logical = hex::decode("0fff100002e7").unwrap();
        let physical = read_command(0xfff, 0x100, 2, 3).unwrap();
        assert_eq!(
            physical
                .iter()
                .map(|b| b.rotate_left(3))
                .collect::<Vec<_>>(),
            logical
        );
        // Captured discovery status reply from identifier 0.
        assert_eq!(
            response(&hex::decode("000000010067").unwrap()).unwrap(),
            (0, 0, &[0][..])
        );
        let mut bytes = hex::decode("00000002198031").unwrap();
        assert_eq!(response(&bytes).unwrap(), (0, 0, &[0x19, 0x80][..]));
        // Live guarded cold-start replies, 2026-11-03: assigned endpoints
        // and the common unassigned identifier observed before addressing.
        for (wire, id) in [
            ("000100021980e2", 1),
            ("005f00021980e9", 95),
            ("0ff00002198089", 0xff0),
        ] {
            assert_eq!(
                response(&hex::decode(wire).unwrap()).unwrap(),
                (id, 0, &[0x19, 0x80][..])
            );
        }
        bytes[4] ^= 1;
        assert!(response(&bytes).is_err());
        assert!(response(&bytes[..4]).is_err());
        assert!(read_command(0x1000, 0x100, 2, 3).is_err());
        assert!(read_command(0xfff, 0x100, 0, 3).is_err());
    }
}
